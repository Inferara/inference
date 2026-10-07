/**
 * Pure helpers for the Proof Jobs view. No `vscode` import — unit-tested
 * under plain node.
 */

import { isCancelableStatus, isTerminalStatus } from './jobDetailHtml';
import type { JobResponse, JobStatus } from './types';

/** Jobs per page (the server allows up to 100). */
export const PAGE_SIZE = 50;

/** Statuses offered by the filter, in lifecycle order. */
export const FILTER_STATUSES: readonly JobStatus[] = [
    'Accepted', 'Queued', 'Provisioning', 'Booting', 'Running', 'Verifying', 'Canceling',
    'Succeeded', 'PartialSuccess', 'CompileGoals', 'Failed', 'TimedOut', 'Lost', 'Canceled',
    'ProvisionFailed',
];

/**
 * Tree `contextValue` for a job: `inference.proofJob` plus `.cancelable` or
 * `.terminal`, plus `.proof` when the run can carry a completed proof.
 * package.json menus match these with `viewItem =~ /…/`.
 */
export function jobContextValue(job: Pick<JobResponse, 'status'>): string {
    let value = 'inference.proofJob';
    if (isCancelableStatus(job.status)) {
        value += '.cancelable';
    } else if (isTerminalStatus(job.status)) {
        value += '.terminal';
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
