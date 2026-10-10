import * as vscode from 'vscode';

import { finishedJobs, isActiveJob, newestJob, nextStatuses } from './jobList';
import { listVerdict } from './jobVerdict';
import { VERDICT_CODICONS, type JobsSnapshot } from './jobsTree';
import { clickedJob, proverStatusState } from './proverStatusBarState';
import type { JobResponse, JobStatus } from './types';

/** Internal command behind the status bar item. */
export const SHOW_PROVER_STATUS_COMMAND = 'inference.showProverStatus';

export interface ProverStatusHooks {
    nameOf(job: JobResponse): string;
    openJob(job: JobResponse): Promise<void>;
    runAgain(job: JobResponse): Promise<void>;
    /** The job's panel is the active editor tab (no notice needed). */
    isPanelActive(jobId: string): boolean;
}

/**
 * The proving status bar item and the "job finished" notices. Both follow the
 * Proof Jobs loads ({@link JobsSnapshot}); the job list keeps loading while
 * jobs move, even when its view is hidden.
 */
export class ProverStatus implements vscode.Disposable {
    private readonly item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, -1);
    private readonly command: vscode.Disposable;
    private previous = new Map<string, JobStatus>();
    private readonly watched = new Set<string>();
    private lastFinished: JobResponse | null = null;
    private target: string | null = null;
    private snapshot: JobsSnapshot | null = null;
    /** Counts resets, so a notice answered after one does not act on the new server. */
    private generation = 0;

    constructor(private readonly hooks: ProverStatusHooks) {
        this.item.name = 'Inference Proof Jobs';
        this.item.command = SHOW_PROVER_STATUS_COMMAND;
        this.command = vscode.commands.registerCommand(SHOW_PROVER_STATUS_COMMAND, () => this.onClick());
    }

    /** A job was just submitted here: watch it and stop showing the last result. */
    watch(job: JobResponse): void {
        this.watched.add(job.id);
        // Its submitted status, so a finish before the next load is noticed.
        if (!this.previous.has(job.id)) {
            this.previous.set(job.id, job.status);
        }
        this.lastFinished = null;
        if (this.snapshot) {
            this.update(this.snapshot);
        }
    }

    /** The server or account changed: forget its jobs and last result. */
    reset(): void {
        this.generation++;
        this.previous = new Map();
        this.watched.clear();
        this.lastFinished = null;
        this.target = null;
        this.snapshot = null;
        this.item.hide();
    }

    update(snapshot: JobsSnapshot): void {
        this.snapshot = snapshot;
        const jobs = snapshot.tracking;
        for (const job of jobs) {
            if (isActiveJob(job)) {
                this.watched.add(job.id);
            }
        }
        const finished = finishedJobs(this.previous, jobs, this.watched);
        this.previous = nextStatuses(this.previous, jobs, this.watched);
        for (const job of finished) {
            this.watched.delete(job.id);
            if (!this.hooks.isPanelActive(job.id)) {
                void this.notify(job);
            }
        }
        this.lastFinished = newestJob(finished) ?? this.lastFinished;
        const state = proverStatusState({
            jobs: snapshot.configured ? jobs : [],
            problem: snapshot.problem,
            lastFinished: this.lastFinished,
            nameOf: (job) => this.hooks.nameOf(job),
        });
        this.target = state.jobId;
        if (!state.visible) {
            this.item.hide();
            return;
        }
        const icon = state.icon === 'server' ? 'cloud' : VERDICT_CODICONS[state.icon].id;
        this.item.text = `$(${icon}) ${state.text}`;
        this.item.tooltip = state.tooltip;
        this.item.backgroundColor = state.icon === 'server'
            ? new vscode.ThemeColor('statusBarItem.warningBackground')
            : undefined;
        this.item.show();
    }

    private async notify(job: JobResponse): Promise<void> {
        const verdict = listVerdict(job);
        const name = this.hooks.nameOf(job);
        const counts = typeof job.holesTotal === 'number' && job.holesTotal > 0
            ? ` (${job.holesClosed ?? 0}/${job.holesTotal})`
            : '';
        const message = `Inference: ${name} — ${verdict.headline}${counts}.`;
        const good = verdict.claim !== 'none';
        // A returned proof that did not compile may succeed on another run; a
        // submitted file that did not compile fails again until it is changed.
        const retry = ['failed', 'timed-out', 'provision-failed', 'completed-compile-error'].includes(verdict.kind);
        const actions = retry ? ['Open', 'Run Again'] : ['Open'];
        const generation = this.generation;
        const choice = good && verdict.tone === 'ok'
            ? await vscode.window.showInformationMessage(message, ...actions)
            : await vscode.window.showWarningMessage(message, ...actions);
        if (generation !== this.generation) {
            return; // the job belongs to the server or account used before
        }
        if (choice === 'Open') {
            await this.hooks.openJob(job);
        } else if (choice === 'Run Again') {
            await this.hooks.runAgain(job);
        }
    }

    private async onClick(): Promise<void> {
        const job = clickedJob(this.target, this.snapshot?.tracking ?? [], this.lastFinished);
        if (job) {
            if (!isActiveJob(job)) {
                this.lastFinished = null; // seen
            }
            await this.hooks.openJob(job);
            if (this.snapshot) {
                this.update(this.snapshot);
            }
            return;
        }
        await vscode.commands.executeCommand('inference.proofJobsView.focus');
    }

    dispose(): void {
        this.item.dispose();
        this.command.dispose();
    }
}
