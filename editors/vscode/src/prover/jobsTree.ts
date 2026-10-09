import * as vscode from 'vscode';

import { AuthError, ProverApi } from './api';
import { describeError, resolveConfig } from './config';
import { JobOrigins } from './jobOrigins';
import {
    describeJob,
    FILTER_LABELS,
    isActiveJob,
    jobContextValue,
    mergeFirstPage,
    PAGE_SIZE,
    pollDelay,
} from './jobList';
import { listVerdict, type Verdict, type VerdictIcon } from './jobVerdict';
import type { JobResponse, JobStatus } from './types';

type NodeKind = 'job' | 'group' | 'message' | 'more';

/** Codicon and theme colour per verdict icon (tree rows, status bar). */
export const VERDICT_CODICONS: Readonly<Record<VerdictIcon, { id: string; color?: string }>> = {
    pass: { id: 'pass-filled', color: 'testing.iconPassed' },
    warning: { id: 'warning', color: 'list.warningForeground' },
    error: { id: 'error', color: 'testing.iconFailed' },
    refuted: { id: 'close', color: 'charts.purple' },
    running: { id: 'sync~spin', color: 'charts.blue' },
    waiting: { id: 'clock', color: 'charts.blue' },
    verifying: { id: 'workspace-trusted', color: 'charts.blue' },
    canceled: { id: 'circle-slash', color: 'descriptionForeground' },
    loading: { id: 'loading~spin' },
};

export function verdictThemeIcon(verdict: Verdict): vscode.ThemeIcon {
    const icon = VERDICT_CODICONS[verdict.icon];
    return new vscode.ThemeIcon(icon.id, icon.color ? new vscode.ThemeColor(icon.color) : undefined);
}

/** What the last load ended with, for the status bar and notifications. */
export interface JobsSnapshot {
    jobs: readonly JobResponse[];
    /** A problem to show instead of jobs, or null. */
    problem: { kind: 'auth' | 'config' | 'network'; message: string } | null;
    configured: boolean;
    serverUrl: string | null;
}

export class JobTreeItem extends vscode.TreeItem {
    constructor(
        label: string,
        public readonly nodeKind: NodeKind,
        public readonly job?: JobResponse,
        public readonly parent?: JobTreeItem,
        collapsible = vscode.TreeItemCollapsibleState.None,
    ) {
        super(label, collapsible);
        if (nodeKind === 'job' && job) {
            this.id = job.id;
            this.contextValue = jobContextValue(job);
        }
    }
}

/**
 * The Proof Jobs view: the caller's jobs, newest first, in "In progress" and
 * "Finished" groups (flat when filtered or when nothing is running), with a
 * "Load more" node for older pages. Loads on a schedule from
 * {@link pollDelay}; a refresh reloads the first page and keeps older pages.
 */
export class ProofJobsProvider implements vscode.TreeDataProvider<JobTreeItem>, vscode.Disposable {
    private readonly changed = new vscode.EventEmitter<JobTreeItem | undefined | null>();
    readonly onDidChangeTreeData = this.changed.event;
    private readonly loaded = new vscode.EventEmitter<JobsSnapshot>();
    /** Fires after every load (and reset). */
    readonly onDidLoad = this.loaded.event;

    private jobs: JobResponse[] = [];
    private hasMore = false;
    private problem: JobsSnapshot['problem'] = null;
    private configured = false;
    private serverUrl: string | null = null;
    private timer: ReturnType<typeof setTimeout> | undefined;
    private loading: Promise<void> | null = null;
    private filter: JobStatus | undefined;
    private visible = false;
    private errors = 0;
    private lastSubmit: number | null = null;
    /** Bumped on a server or account change; replies from older generations are dropped. */
    private generation = 0;
    private groups = new Map<string, JobTreeItem>();

    constructor(
        private readonly secrets: vscode.SecretStorage,
        private readonly log: vscode.LogOutputChannel,
        private readonly origins: JobOrigins,
    ) {}

    get statusFilter(): JobStatus | undefined {
        return this.filter;
    }

    get snapshot(): JobsSnapshot {
        return { jobs: this.jobs, problem: this.problem, configured: this.configured, serverUrl: this.serverUrl };
    }

    /** Show only `status` (undefined = all); a load in flight finishes first. */
    async setFilter(status: JobStatus | undefined): Promise<void> {
        this.filter = status;
        this.jobs = [];
        await vscode.commands.executeCommand('setContext', 'inference.prover.filtered', status !== undefined);
        await this.loading;
        return this.refresh();
    }

    /** A job was just submitted from this window: poll quickly for a while. */
    noteSubmitted(): void {
        this.lastSubmit = Date.now();
    }

    /** Reload the first page; concurrent calls share one load. */
    refresh(): Promise<void> {
        return this.loading ?? this.track(this.load(false));
    }

    /** Append the next older page (dropped if the server or account changes meanwhile). */
    async loadMore(): Promise<void> {
        const generation = this.generation;
        await this.loading;
        if (generation !== this.generation || this.jobs.length === 0) {
            return;
        }
        return this.track(this.load(true));
    }

    /** Forget everything loaded: the server or the account changed. */
    reset(): void {
        this.generation++;
        this.jobs = [];
        this.hasMore = false;
        this.problem = null;
        this.loading = null;
        this.errors = 0;
        this.changed.fire(undefined);
        this.loaded.fire(this.snapshot);
    }

    private track(load: Promise<void>): Promise<void> {
        const tracked: Promise<void> = load.finally(() => {
            if (this.loading === tracked) {
                this.loading = null;
            }
            this.schedule();
        });
        this.loading = tracked;
        return tracked;
    }

    setVisible(visible: boolean): void {
        this.visible = visible;
        if (visible) {
            void this.refresh();
        } else {
            this.schedule();
        }
    }

    private schedule(): void {
        if (this.timer) {
            clearTimeout(this.timer);
            this.timer = undefined;
        }
        const delay = pollDelay({
            active: this.jobs.some(isActiveJob),
            visible: this.visible,
            msSinceSubmit: this.lastSubmit === null ? null : Date.now() - this.lastSubmit,
            errors: this.errors,
        });
        if (delay !== null && this.configured) {
            this.timer = setTimeout(() => void this.refresh(), delay);
        }
    }

    /** The node for a loaded job (for TreeView.reveal, which matches by id). */
    findNode(jobId: string): JobTreeItem | undefined {
        const job = this.jobs.find((j) => j.id === jobId);
        if (!job) {
            return undefined;
        }
        const grouped = this.grouped();
        const parent = grouped ? this.group(isActiveJob(job) ? 'active' : 'finished', 0) : undefined;
        return this.makeNode(job, parent);
    }

    /** The name a job is shown under: the `.inf` it came from, else the upload. */
    displayName(job: JobResponse): string {
        const origin = this.serverUrl ? this.origins.get(this.serverUrl, job.id) : undefined;
        return JobOrigins.displayName(origin) ?? job.filename ?? job.id;
    }

    private makeNode(job: JobResponse, parent?: JobTreeItem): JobTreeItem {
        const verdict = listVerdict(job);
        const name = this.displayName(job);
        const node = new JobTreeItem(name, 'job', job, parent);
        node.iconPath = verdictThemeIcon(verdict);
        node.description = describeJob(job, Date.now());
        const tooltip = new vscode.MarkdownString(undefined, true);
        tooltip.appendMarkdown(`$(${VERDICT_CODICONS[verdict.icon].id.replace('~spin', '')}) **${escapeMd(verdict.headline)}**\n\n`);
        tooltip.appendMarkdown(`${escapeMd(verdict.body)}\n\n`);
        const facts = [
            name !== job.filename && job.filename ? `${escapeMd(name)} → ${code(job.filename)}` : undefined,
            typeof job.holesTotal === 'number' && job.holesTotal > 0 ? `${job.holesClosed ?? 0}/${job.holesTotal} obligations` : undefined,
            job.createdAt ? `submitted ${new Date(job.createdAt).toLocaleString()}` : undefined,
        ].filter(Boolean);
        if (facts.length) {
            tooltip.appendMarkdown(`${facts.join(' · ')}\n\n`);
        }
        tooltip.appendMarkdown(`Job ${code(job.id)}`);
        node.tooltip = tooltip;
        node.accessibilityInformation = { label: `${name}, ${verdict.badge}, ${node.description}` };
        node.command = { title: 'Open Proof Job', command: 'inference.openProofJob', arguments: [node] };
        return node;
    }

    private async load(more: boolean): Promise<void> {
        const generation = this.generation;
        let config;
        try {
            config = await resolveConfig(this.secrets);
        } catch (err) {
            this.configured = true;
            this.jobs = [];
            this.problem = { kind: 'config', message: describeError(err) };
            this.fire();
            return;
        }
        if (!config) {
            this.configured = false;
            this.jobs = [];
            this.problem = null;
            this.fire();
            return;
        }
        this.configured = true;
        this.serverUrl = config.serverUrl;
        try {
            const api = new ProverApi(config.serverUrl, config.apiKey);
            const before = more ? this.jobs[this.jobs.length - 1]?.createdAt ?? undefined : undefined;
            const page = await api.listJobs({ status: this.filter, limit: PAGE_SIZE, before });
            if (generation !== this.generation) {
                return; // a reply from the previous server or account
            }
            this.jobs = more ? [...this.jobs, ...page] : mergeFirstPage(page, this.jobs, PAGE_SIZE);
            if (more || page.length < PAGE_SIZE || this.jobs.length === page.length) {
                // A full page may have older jobs behind it. A refresh that kept
                // older loaded pages keeps the answer from the last "Load more".
                this.hasMore = page.length === PAGE_SIZE;
            }
            this.problem = null;
            this.errors = 0;
        } catch (err) {
            if (generation !== this.generation) {
                return;
            }
            this.errors++;
            const auth = err instanceof AuthError && err.status === 401;
            this.problem = auth
                ? { kind: 'auth', message: 'The proof server rejected the API key.' }
                : { kind: 'network', message: describeError(err) };
            if (auth) {
                this.jobs = [];
                this.hasMore = false;
            }
            this.log.error(`Prover: listing jobs: ${err instanceof Error ? err.message : err}`);
        }
        this.fire();
    }

    private fire(): void {
        this.groups.clear();
        this.changed.fire(undefined);
        this.loaded.fire(this.snapshot);
    }

    /** Grouped when unfiltered and something is in progress. */
    private grouped(): boolean {
        return !this.filter && this.jobs.some(isActiveJob);
    }

    private group(kind: 'active' | 'finished', count: number): JobTreeItem {
        const existing = this.groups.get(kind);
        if (existing) {
            return existing;
        }
        const label = kind === 'active' ? 'In progress' : 'Finished';
        const node = new JobTreeItem(label, 'group', undefined, undefined, vscode.TreeItemCollapsibleState.Expanded);
        node.id = `group:${kind}`;
        node.description = count > 0 ? String(count) : undefined;
        node.contextValue = `inference.proofJobGroup.${kind}`;
        this.groups.set(kind, node);
        return node;
    }

    getTreeItem(element: JobTreeItem): vscode.TreeItem {
        return element;
    }

    getParent(element: JobTreeItem): JobTreeItem | undefined {
        return element.parent;
    }

    private problemNode(): JobTreeItem {
        const problem = this.problem!;
        const node = new JobTreeItem(
            problem.kind === 'auth'
                ? 'API key rejected — set a new key'
                : problem.kind === 'config'
                  ? problem.message
                  : "Can't reach the proof server — retry",
            'message',
        );
        node.iconPath = new vscode.ThemeIcon(problem.kind === 'auth' ? 'key' : 'error', new vscode.ThemeColor('list.errorForeground'));
        node.tooltip = problem.message;
        node.command = problem.kind === 'auth'
            ? { title: 'Set API Key', command: 'inference.setProverApiKey' }
            : problem.kind === 'config'
              ? { title: 'Open Setting', command: 'workbench.action.openSettings', arguments: ['inference.prover.serverUrl'] }
              : { title: 'Retry', command: 'inference.refreshProofJobs' };
        return node;
    }

    getChildren(element?: JobTreeItem): JobTreeItem[] {
        if (element?.nodeKind === 'group') {
            const kind = element.id === 'group:active' ? 'active' : 'finished';
            const jobs = this.jobs.filter((j) => isActiveJob(j) === (kind === 'active'));
            const items = jobs.map((job) => this.makeNode(job, element));
            if (kind === 'finished' && this.hasMore) {
                items.push(this.moreNode(element));
            }
            return items;
        }
        if (element || !this.configured) {
            return []; // unconfigured → the welcome view shows instead
        }
        if (this.problem && (this.problem.kind !== 'network' || this.jobs.length === 0)) {
            return [this.problemNode()];
        }
        const problem = this.problem ? [this.problemNode()] : [];
        if (this.jobs.length === 0) {
            const node = new JobTreeItem(
                this.filter ? `No “${FILTER_LABELS[this.filter]}” jobs.` : 'No proof jobs yet. Prove an .inf file to start.',
                'message',
            );
            node.iconPath = new vscode.ThemeIcon('info');
            if (this.filter) {
                node.command = { title: 'Show All Jobs', command: 'inference.clearProofJobsFilter' };
                node.tooltip = 'Click to show all jobs';
            }
            return [node];
        }
        if (this.grouped()) {
            const active = this.jobs.filter(isActiveJob).length;
            return [...problem, this.group('active', active), this.group('finished', this.jobs.length - active)];
        }
        const items = this.jobs.map((job) => this.makeNode(job));
        if (this.hasMore) {
            items.push(this.moreNode());
        }
        return [...problem, ...items];
    }

    private moreNode(parent?: JobTreeItem): JobTreeItem {
        const more = new JobTreeItem('Load more…', 'more', undefined, parent);
        more.iconPath = new vscode.ThemeIcon('ellipsis');
        more.command = { title: 'Load More', command: 'inference.loadMoreProofJobs' };
        return more;
    }

    dispose(): void {
        if (this.timer) {
            clearTimeout(this.timer);
        }
        this.changed.dispose();
        this.loaded.dispose();
    }
}

function escapeMd(text: string): string {
    return text.replace(/[\\`*_{}[\]()#+\-.!|<>]/g, (c) => `\\${c}`);
}

/** An inline code span (backticks inside are replaced; code spans take no escapes). */
function code(text: string): string {
    return `\`${text.replace(/`/g, "'")}\``;
}
