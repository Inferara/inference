/**
 * What the proving status bar item shows. Pure — no `vscode` import.
 */

import { isActiveJob } from './jobList';
import { listVerdict, type VerdictIcon } from './jobVerdict';
import type { JobResponse } from './types';

export interface ProverStatusInput {
    jobs: readonly JobResponse[];
    problem: { kind: 'auth' | 'config' | 'network'; message: string } | null;
    /** The newest job that finished while this window watched it. */
    lastFinished: JobResponse | null;
    /** The name the user knows a job by (`.inf`, else the upload). */
    nameOf(job: JobResponse): string;
}

export interface ProverStatusState {
    visible: boolean;
    /** Codicon key from the verdict, or a fixed one. */
    icon: VerdictIcon | 'server';
    text: string;
    tooltip: string;
    /** Job to open on click; null = show the Proof Jobs view. */
    jobId: string | null;
}

function stem(name: string): string {
    const dot = name.lastIndexOf('.');
    return dot > 0 ? name.slice(0, dot) : name;
}

export function proverStatusState(input: ProverStatusInput): ProverStatusState {
    if (input.problem && input.problem.kind !== 'config') {
        return {
            visible: true,
            icon: 'server',
            text: 'Proof server',
            tooltip: input.problem.kind === 'auth'
                ? 'The proof server rejected the API key. Click to see Proof Jobs.'
                : `Can't reach the proof server: ${input.problem.message}`,
            jobId: null,
        };
    }
    const active = input.jobs.filter(isActiveJob);
    if (active.length === 1) {
        const job = active[0];
        const verdict = listVerdict(job);
        const counts = typeof job.holesTotal === 'number' && job.holesTotal > 0
            ? ` ${job.holesClosed ?? 0}/${job.holesTotal}`
            : '';
        return {
            visible: true,
            icon: verdict.icon,
            text: `${stem(input.nameOf(job))}${counts}`,
            tooltip: `${input.nameOf(job)}: ${verdict.headline}. Click to open the job.`,
            jobId: job.id,
        };
    }
    if (active.length > 1) {
        return {
            visible: true,
            icon: 'running',
            text: `Proving ${active.length}`,
            tooltip: `${active.length} proof jobs are in progress: ${active.map((j) => input.nameOf(j)).join(', ')}. Click to see Proof Jobs.`,
            jobId: null,
        };
    }
    if (input.lastFinished) {
        const verdict = listVerdict(input.lastFinished);
        return {
            visible: true,
            icon: verdict.icon,
            text: stem(input.nameOf(input.lastFinished)),
            tooltip: `${input.nameOf(input.lastFinished)}: ${verdict.headline}. Click to open the job.`,
            jobId: input.lastFinished.id,
        };
    }
    return { visible: false, icon: 'server', text: '', tooltip: '', jobId: null };
}
