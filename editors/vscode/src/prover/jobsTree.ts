import * as vscode from 'vscode';

import { AuthError, ProverApi } from './api';
import { describeError, resolveConfig } from './config';
import { jobContextValue, mergeFirstPage, PAGE_SIZE } from './jobList';
import { jobStatusIconId, jobStatusLabel } from './jobPresentation';
import type { JobResponse, JobStatus } from './types';

/** Auto-refresh cadence while the view is visible (milliseconds). */
const AUTO_REFRESH_MS = 10_000;
type NodeKind = 'job' | 'message' | 'more';

export class JobTreeItem extends vscode.TreeItem {
    constructor(
        label: string,
        public readonly nodeKind: NodeKind,
        public readonly job?: JobResponse,
    ) {
        super(label, vscode.TreeItemCollapsibleState.None);
        if (nodeKind === 'job' && job) {
            this.id = job.id;
            this.contextValue = jobContextValue(job);
        }
    }
}

function describeJob(job: JobResponse): string {
    const parts = [jobStatusLabel(job.status, job.mode, job.holesTotal, job.holesClosed, job.claimClass)];
    if (typeof job.holesClosed === 'number' && typeof job.holesTotal === 'number') {
        parts.push(`holes ${job.holesClosed}/${job.holesTotal}`);
    }
    return parts.join(' · ');
}

/**
 * The Proof Jobs view: the caller's jobs, newest first, optionally filtered
 * by status, with a "Load more" node for older pages. Refreshes every
 * {@link AUTO_REFRESH_MS} ms while visible; a refresh reloads the first page
 * and keeps older pages already loaded.
 */
export class ProofJobsProvider implements vscode.TreeDataProvider<JobTreeItem>, vscode.Disposable {
    private readonly changed = new vscode.EventEmitter<JobTreeItem | undefined | null>();
    readonly onDidChangeTreeData = this.changed.event;

    private jobs: JobResponse[] = [];
    private hasMore = false;
    private errorMessage: string | null = null;
    private configured = false;
    private timer: ReturnType<typeof setInterval> | undefined;
    private loading: Promise<void> | null = null;
    private filter: JobStatus | undefined;

    constructor(
        private readonly secrets: vscode.SecretStorage,
        private readonly log: vscode.LogOutputChannel,
    ) {}

    get statusFilter(): JobStatus | undefined {
        return this.filter;
    }

    /** Show only `status` (undefined = all); a load in flight finishes first. */
    async setFilter(status: JobStatus | undefined): Promise<void> {
        this.filter = status;
        this.jobs = [];
        await this.loading;
        return this.refresh();
    }

    /** Reload the first page; concurrent calls share one load. */
    refresh(): Promise<void> {
        if (!this.loading) {
            this.loading = this.load(false).finally(() => {
                this.loading = null;
            });
        }
        return this.loading;
    }

    /** Append the next older page. */
    async loadMore(): Promise<void> {
        await this.loading;
        this.loading = this.load(true).finally(() => {
            this.loading = null;
        });
        return this.loading;
    }

    setVisible(visible: boolean): void {
        if (visible) {
            void this.refresh();
            this.timer ??= setInterval(() => void this.refresh(), AUTO_REFRESH_MS);
        } else if (this.timer) {
            clearInterval(this.timer);
            this.timer = undefined;
        }
    }

    /** The node for a loaded job (for TreeView.reveal, which matches by id). */
    findNode(jobId: string): JobTreeItem | undefined {
        const job = this.jobs.find((j) => j.id === jobId);
        return job ? this.makeNode(job) : undefined;
    }

    private makeNode(job: JobResponse): JobTreeItem {
        const node = new JobTreeItem(job.filename ?? job.id, 'job', job);
        node.iconPath = new vscode.ThemeIcon(
            jobStatusIconId(job.status, job.mode, job.holesTotal, job.holesClosed, job.claimClass),
        );
        node.description = describeJob(job);
        node.tooltip = `${job.id}\nstatus: ${jobStatusLabel(job.status, job.mode, job.holesTotal, job.holesClosed, job.claimClass)}\nclaim class: ${job.claimClass ?? 'unknown'}${job.createdAt ? `\nsubmitted: ${new Date(job.createdAt).toLocaleString()}` : ''}`;
        node.command = { title: 'Open Proof Job', command: 'inference.openProofJob', arguments: [node] };
        return node;
    }

    private async load(more: boolean): Promise<void> {
        let config;
        try {
            config = await resolveConfig(this.secrets);
        } catch (err) {
            this.configured = true;
            this.jobs = [];
            this.errorMessage = describeError(err);
            this.changed.fire(undefined);
            return;
        }
        if (!config) {
            this.configured = false;
            this.jobs = [];
            this.errorMessage = null;
            this.changed.fire(undefined);
            return;
        }
        this.configured = true;
        try {
            const api = new ProverApi(config.serverUrl, config.apiKey);
            const before = more ? this.jobs[this.jobs.length - 1]?.createdAt ?? undefined : undefined;
            const page = await api.listJobs({ status: this.filter, limit: PAGE_SIZE, before });
            this.jobs = more ? [...this.jobs, ...page] : mergeFirstPage(page, this.jobs, PAGE_SIZE);
            if (more || page.length < PAGE_SIZE || this.jobs.length === page.length) {
                // A full page may have older jobs behind it. A refresh that kept
                // older loaded pages keeps the answer from the last "Load more".
                this.hasMore = page.length === PAGE_SIZE;
            }
            this.errorMessage = null;
        } catch (err) {
            this.jobs = [];
            this.hasMore = false;
            this.errorMessage =
                err instanceof AuthError && err.status === 401
                    ? 'Authentication failed. Check your API key.'
                    : describeError(err);
            this.log.error(`Prover: listing jobs: ${err instanceof Error ? err.message : err}`);
        }
        this.changed.fire(undefined);
    }

    getTreeItem(element: JobTreeItem): vscode.TreeItem {
        return element;
    }

    getParent(): undefined {
        return undefined;
    }

    getChildren(element?: JobTreeItem): JobTreeItem[] {
        if (element || !this.configured) {
            return []; // unconfigured → the welcome view shows instead
        }
        if (this.errorMessage) {
            const node = new JobTreeItem(this.errorMessage, 'message');
            node.iconPath = new vscode.ThemeIcon('error');
            node.tooltip = this.errorMessage;
            return [node];
        }
        if (this.jobs.length === 0) {
            const node = new JobTreeItem(
                this.filter ? `No ${this.filter} jobs.` : 'No proof jobs yet.',
                'message',
            );
            node.iconPath = new vscode.ThemeIcon('info');
            return [node];
        }
        const items = this.jobs.map((job) => this.makeNode(job));
        if (this.hasMore) {
            const more = new JobTreeItem('Load more…', 'more');
            more.iconPath = new vscode.ThemeIcon('ellipsis');
            more.command = { title: 'Load More', command: 'inference.loadMoreProofJobs' };
            items.push(more);
        }
        return items;
    }

    dispose(): void {
        if (this.timer) {
            clearInterval(this.timer);
        }
        this.changed.dispose();
    }
}
