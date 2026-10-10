/**
 * Pure helpers for the Proof Jobs view. No `vscode` import — unit-tested
 * under plain node.
 */

import { ApiError } from './api';
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
 * submit (even before the list shows the new job), steady while jobs move
 * (also hidden, for the status bar and finish notices), slow when idle and
 * visible, off when idle and hidden, and backing off on errors.
 */
export function pollDelay(state: PollState): number | null {
    const recentSubmit = state.msSinceSubmit !== null && state.msSinceSubmit < 120_000;
    if (!state.active && !state.visible && !recentSubmit) {
        return null;
    }
    if (state.errors > 0) {
        return Math.min(300_000, 10_000 * 2 ** Math.min(state.errors - 1, 5));
    }
    if (recentSubmit) {
        return 5_000;
    }
    return state.active ? 10_000 : 60_000;
}

/** At most this many moving jobs outside the first page are re-read per load. */
export const MAX_STALE_REFRESH = 10;

/**
 * Moving jobs known from earlier loads that a fresh first page no longer
 * covers (newer jobs pushed them out): re-read them one by one, or they would
 * look moving forever. Newest first, at most {@link MAX_STALE_REFRESH}.
 */
export function staleActiveJobs(known: readonly JobResponse[], firstPage: readonly JobResponse[]): JobResponse[] {
    const fresh = new Set(firstPage.map((j) => j.id));
    const seen = new Set<string>();
    return known
        .filter((j) => {
            if (fresh.has(j.id) || seen.has(j.id) || !isActiveJob(j)) {
                return false;
            }
            seen.add(j.id);
            return true;
        })
        .slice(0, MAX_STALE_REFRESH);
}

/**
 * A stale job after its re-read: the server's copy, or the last known one when
 * the read fails, so it stays tracked and the next load tries again. Only a
 * job the server no longer has (deleted) is dropped.
 */
export async function rereadJob(
    job: JobResponse,
    read: (id: string) => Promise<JobResponse>,
): Promise<JobResponse | null> {
    try {
        return await read(job.id);
    } catch (err) {
        return err instanceof ApiError && err.status === 404 ? null : job;
    }
}

/**
 * The statuses to compare the next load against: the loaded ones, plus the
 * last known status of watched jobs the load did not include (a list request
 * that started before the submit, or a filtered page).
 */
export function nextStatuses(
    previous: ReadonlyMap<string, JobStatus>,
    jobs: readonly JobResponse[],
    watched: ReadonlySet<string>,
): Map<string, JobStatus> {
    const next = new Map(jobs.map((j) => [j.id, j.status] as [string, JobStatus]));
    for (const id of watched) {
        const before = previous.get(id);
        if (!next.has(id) && before !== undefined) {
            next.set(id, before);
        }
    }
    return next;
}

/** The most recently submitted job (by `createdAt`), or null. */
export function newestJob(jobs: readonly JobResponse[]): JobResponse | null {
    let newest: JobResponse | null = null;
    for (const job of jobs) {
        if (!newest || Date.parse(job.createdAt ?? '') > Date.parse(newest.createdAt ?? '')) {
            newest = job;
        }
    }
    return newest;
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
