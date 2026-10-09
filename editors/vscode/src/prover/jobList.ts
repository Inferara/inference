/**
 * Pure helpers for the Proof Jobs view. No `vscode` import — unit-tested
 * under plain node.
 */

import { isCancelableStatus, isTerminalStatus } from './jobStatus';
import { listVerdict } from './jobVerdict';
import type { JobResponse, JobStatus } from './types';

/** Jobs per page (the server allows up to 100). */
export const PAGE_SIZE = 50;

/** Statuses offered by the filter, in lifecycle order. */
export const FILTER_STATUSES: readonly JobStatus[] = [
    'Accepted', 'Queued', 'Provisioning', 'Booting', 'Running', 'Verifying', 'Canceling',
    'Succeeded', 'PartialSuccess', 'CompileGoals', 'Failed', 'TimedOut', 'Lost', 'Canceled',
    'ProvisionFailed',
];

/** Plain filter labels; the raw status is shown beside each. */
export const FILTER_LABELS: Readonly<Record<JobStatus, string>> = {
    Accepted: 'Accepted',
    Queued: 'Waiting for a worker',
    Provisioning: 'Starting a worker',
    Booting: 'Worker booting',
    Running: 'Proving',
    Verifying: 'Verifying',
    Canceling: 'Canceling',
    Succeeded: 'Succeeded',
    PartialSuccess: 'Partially proved',
    CompileGoals: 'Compiled only',
    Failed: 'Failed',
    TimedOut: 'Timed out',
    Lost: 'Worker lost',
    Canceled: 'Canceled',
    ProvisionFailed: "Couldn't start",
};

/** Jobs the server can still move (Lost is re-queued by the server). */
export function isActiveJob(job: Pick<JobResponse, 'status'>): boolean {
    return !isTerminalStatus(job.status) || job.status === 'Lost';
}

/** The portal opens on the certificate for runs that can carry one. */
export function hasCertificate(job: Pick<JobResponse, 'status'>): boolean {
    return job.status === 'Succeeded' || job.status === 'PartialSuccess' || job.status === 'CompileGoals';
}

/**
 * Tree `contextValue` for a job: `inference.proofJob` plus one of
 * `.cancelable`, `.active` (Verifying/Canceling), `.lost` or `.terminal`,
 * plus `.proof` when the run can carry a completed proof.
 * package.json menus match these with `viewItem =~ /…/`.
 */
export function jobContextValue(job: Pick<JobResponse, 'status'>): string {
    let value = 'inference.proofJob';
    if (isCancelableStatus(job.status)) {
        value += '.cancelable';
    } else if (job.status === 'Lost') {
        value += '.lost';
    } else if (isTerminalStatus(job.status)) {
        value += '.terminal';
    } else {
        value += '.active';
    }
    if (job.status === 'Succeeded' || job.status === 'PartialSuccess') {
        value += '.proof';
    }
    return value;
}

/**
 * Merge a fresh first page with older pages already loaded: the first page
 * replaces everything it covers; older jobs past its last `createdAt` stay.
 */
export function mergeFirstPage(fresh: JobResponse[], loaded: JobResponse[], pageSize: number): JobResponse[] {
    // A short first page is the whole (filtered) list.
    if (fresh.length < pageSize) {
        return fresh;
    }
    const oldest = Date.parse(fresh[fresh.length - 1].createdAt ?? '');
    if (Number.isNaN(oldest)) {
        return fresh;
    }
    const ids = new Set(fresh.map((j) => j.id));
    return [
        ...fresh,
        ...loaded.filter((j) => !ids.has(j.id) && Date.parse(j.createdAt ?? '') < oldest),
    ];
}

/** "just now", "4m ago", "2h ago", "yesterday", "3d ago", else a short date. */
export function relativeTime(iso: string | null | undefined, now: number): string {
    const t = Date.parse(iso ?? '');
    if (Number.isNaN(t)) {
        return '';
    }
    const s = Math.max(0, Math.round((now - t) / 1000));
    if (s < 45) return 'just now';
    const m = Math.round(s / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.round(m / 60);
    if (h < 24) return `${h}h ago`;
    const d = Math.round(h / 24);
    if (d === 1) return 'yesterday';
    if (d < 7) return `${d}d ago`;
    return new Date(t).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** Row description: `Verified · 15/15 · 2h ago`. */
export function describeJob(job: JobResponse, now: number): string {
    const parts = [listVerdict(job).badge];
    if (typeof job.holesClosed === 'number' && typeof job.holesTotal === 'number' && job.holesTotal > 0) {
        parts.push(`${job.holesClosed}/${job.holesTotal}`);
    }
    const when = relativeTime(job.createdAt, now);
    if (when) {
        parts.push(when);
    }
    return parts.join(' · ');
}

export interface PollState {
    /** Some loaded job can still move. */
    active: boolean;
    /** The Proof Jobs view is visible. */
    visible: boolean;
    /** Milliseconds since this window submitted a job, or null. */
    msSinceSubmit: number | null;
    /** Consecutive failed loads. */
    errors: number;
}

/**
 * When to load the job list next (ms), or null to stop: fast right after a
 * submit, steady while jobs move (also hidden, for the status bar and finish
 * notices), slow when idle and visible, off when idle and hidden, and backing
 * off on errors.
 */
export function pollDelay(state: PollState): number | null {
    if (!state.active && !state.visible) {
        return null;
    }
    if (state.errors > 0) {
        return Math.min(300_000, 10_000 * 2 ** Math.min(state.errors - 1, 5));
    }
    if (state.msSinceSubmit !== null && state.msSinceSubmit < 120_000) {
        return 5_000;
    }
    return state.active ? 10_000 : 60_000;
}

/**
 * Jobs that finished since the previous load, among those this window saw
 * moving (`watched`). A job that disappears or was never seen active is not
 * reported.
 */
export function finishedJobs(
    previous: ReadonlyMap<string, JobStatus>,
    next: readonly JobResponse[],
    watched: ReadonlySet<string>,
): JobResponse[] {
    return next.filter((job) => {
        const before = previous.get(job.id);
        return (
            watched.has(job.id) &&
            before !== undefined &&
            isActiveJob({ status: before }) &&
            !isActiveJob(job)
        );
    });
}
