/**
 * What a job's result means, in plain words — the single wording source for
 * the job panel, the Proof Jobs view, the status bar and notifications.
 * Pure — no `vscode` import.
 *
 * The claim rules are fail-closed and unchanged from the portal's certificate
 * (`portal/src/lib/certificate.ts`): a positive claim needs a Succeeded
 * prove-mode run with every hole closed, claim class Verified, a result that
 * matches the job, and the independent verifier's full acceptance. Anything
 * short of that is worded as "not verified", never as a proof.
 */

import { duplicateNameHint, type BuildError } from './buildLog';
import { isProvedSuccess } from './jobPresentation';
import { errorCodeText } from './runModel';
import type { ClaimClass, JobResponse, JobResultResponse, ObligationDto } from './types';
import {
    hasCompleteAssumptionEvidence,
    normalizedVerifierOutcome,
    resultMatchesJob,
    verifierAcceptedFull,
    verifierAcceptedPartial,
} from './verifierEvidence';

export type Tone = 'ok' | 'warn' | 'err' | 'refuted' | 'active' | 'muted';

/** Icon keys shared by the webview glyphs and the tree's codicons. */
export type VerdictIcon =
    | 'pass'
    | 'warning'
    | 'error'
    | 'refuted'
    | 'running'
    | 'waiting'
    | 'verifying'
    | 'canceled'
    | 'loading';

export type VerdictKind =
    | 'queued'
    | 'starting'
    | 'proving'
    | 'verifying'
    | 'canceling'
    | 'loading'
    | 'verified'
    | 'structural'
    | 'partial'
    | 'compile-only'
    | 'unverified'
    | 'conflict'
    | 'rejected'
    | 'compile-error'
    | 'completed-compile-error'
    | 'refuted'
    | 'failed'
    | 'timed-out'
    | 'provision-failed'
    | 'lost'
    | 'canceled';

export interface Verdict {
    kind: VerdictKind;
    tone: Tone;
    icon: VerdictIcon;
    /** One or two words for badges and list rows. */
    badge: string;
    headline: string;
    body: string;
    /** Further plain-language notes, most important first. */
    caveats: string[];
    /** The strongest claim this verdict makes; `none` for everything else. */
    claim: 'proved' | 'partial' | 'structural' | 'none';
}

/** Whether the terminal result (`GET /result`) is loaded. */
export type ResultState = 'n/a' | 'loading' | 'loaded' | 'failed';

export interface VerdictInput {
    job: JobResponse;
    result?: JobResultResponse | null;
    resultState?: ResultState;
    /** First error of the server's build log, when the run failed to compile. */
    buildError?: BuildError | null;
    /** File stems the user knows the job by (`clamp` for clamp.inf / clamp.v). */
    fileStems?: readonly string[];
}

function plural(n: number, one: string, many = `${one}s`): string {
    return `${n} ${n === 1 ? one : many}`;
}

function verdict(
    kind: VerdictKind,
    tone: Tone,
    icon: VerdictIcon,
    badge: string,
    headline: string,
    body: string,
    caveats: string[] = [],
    claim: Verdict['claim'] = 'none',
): Verdict {
    return { kind, tone, icon, badge, headline, body, caveats, claim };
}

/** Verdicts of a job that is still moving (status alone decides). */
function liveVerdict(job: JobResponse): Verdict | null {
    switch (job.status) {
        case 'Accepted':
            return verdict('queued', 'active', 'waiting', 'Accepted', 'Accepted',
                'The proof server accepted the file and is queuing it.');
        case 'Queued':
            return verdict('queued', 'active', 'waiting', 'Queued', 'Waiting for a worker',
                'The job waits until an isolated worker is free. Waiting time counts toward its time budget.');
        case 'Provisioning':
        case 'Booting':
            return verdict('starting', 'active', 'waiting', 'Starting', 'Preparing an isolated worker',
                'A fresh worker is starting for this job. This usually takes a minute or two.');
        case 'Running':
            return verdict('proving', 'active', 'running', 'Proving', 'Proving obligations',
                'The worker compiles the file and closes its proof holes one by one.');
        case 'Verifying':
            return verdict('verifying', 'active', 'verifying', 'Verifying',
                'Independently verifying the returned proof',
                'A separate verifier re-checks the completed file before any proof is claimed.');
        case 'Canceling':
            return verdict('canceling', 'muted', 'canceled', 'Canceling…', 'Canceling',
                'The server is stopping this job. While its worker is still starting this can take up to about 90 seconds.');
        default:
            return null;
    }
}

/** Plain notes about obligations whose statement says little. */
function contentCaveats(obligations: readonly ObligationDto[]): string[] {
    const count = (content: string) =>
        obligations.filter((o) => (o.content ?? 'unknown') === content).length;
    const notes: string[] = [];
    const empty = count('empty');
    const tautology = count('tautology');
    const unguarded = count('unguarded');
    const unknown = count('unknown');
    if (empty) {
        notes.push(`${plural(empty, 'obligation has', 'obligations have')} an empty specification.`);
    }
    if (tautology) {
        notes.push(`${plural(tautology, 'obligation is a tautology', 'obligations are tautologies')}: true for any program.`);
    }
    if (unguarded) {
        notes.push(`${plural(unguarded, 'obligation is', 'obligations are')} not tied to the compiled module.`);
    }
    if (unknown) {
        notes.push(`${plural(unknown, 'obligation', 'obligations')} could not be classified.`);
    }
    return notes;
}

/** Why the verifier's evidence does not support the claim. */
export function verifierRejections(
    result: JobResultResponse,
    expectedTargets: string[],
    partial: boolean,
): string[] {
    const reasons: string[] = [];
    const outcome = normalizedVerifierOutcome(result);
    const expectedOutcome = partial ? 'partial' : 'succeeded';
    if (outcome !== expectedOutcome) {
        reasons.push(`The verifier's outcome is “${outcome ?? 'missing'}”, not “${expectedOutcome}”.`);
    }
    if (result.compileOk !== true) {
        reasons.push('The returned file did not compile.');
    }
    if (!partial && (result.verifiedClean !== true || result.admitsDetected !== false)) {
        reasons.push('Some proofs are incomplete (still admitted).');
    }
    if (partial && (result.verifiedClean !== false || result.admitsDetected !== true)) {
        reasons.push('The admitted obligations do not match a partial result.');
    }
    if (result.statementsImmutable !== true) {
        reasons.push('Theorem statements were changed.');
    }
    if (result.markersRemaining !== 0) {
        reasons.push(
            typeof result.markersRemaining === 'number'
                ? `${plural(result.markersRemaining, 'proof marker remains', 'proof markers remain')}.`
                : 'Remaining proof markers were not reported.',
        );
    }
    if (result.assumptionsClosed !== true) {
        reasons.push('The proof relies on assumptions the policy does not allow.');
    } else if (!hasCompleteAssumptionEvidence(result, expectedTargets)) {
        reasons.push('The per-theorem assumption evidence is incomplete or rejected.');
    }
    return reasons;
}

function terminalFailure(job: JobResponse, input: VerdictInput): Verdict {
    const code = job.errorCode ?? null;
    const compileOnlyNote =
        job.mode === 'compile-goals' ? ['This was a compile-only run; no proof was accepted or returned.'] : [];
    if (code === 'spec_refuted') {
        return verdict('refuted', 'refuted', 'refuted', 'Refuted', 'Specification refuted',
            'The prover produced a machine-checked demonstration that an obligation cannot hold. This is a formal negative result, not an error.');
    }
    if (code === 'compile_failed' || code === 'completed_compile_failed') {
        const returned = code === 'completed_compile_failed';
        const error = input.buildError;
        const body = error
            ? `${error.line !== undefined ? `Line ${error.line}: ` : ''}${error.message}`
            : returned
              ? 'The completed file the worker returned did not compile. Open the build log for the compiler output.'
              : 'The submitted file did not compile on the proof server. Open the build log for the compiler output.';
        const hint = error ? duplicateNameHint(error.message, input.fileStems ?? []) : null;
        return returned
            ? verdict('completed-compile-error', 'err', 'error', 'Failed', 'The returned proof did not compile', body,
                hint ? [hint] : [])
            : verdict('compile-error', 'err', 'error', 'Compile error', 'The prover could not compile your file', body,
                hint ? [hint] : []);
    }
    switch (job.status) {
        case 'TimedOut': {
            const total = job.holesTotal ?? 0;
            const closed = job.holesClosed ?? 0;
            const body = code === 'QUEUE_TIMEOUT'
                ? 'No worker became free before the time budget ran out.'
                : 'The run used its whole time budget before it finished.';
            const caveats = total > 0 && code !== 'QUEUE_TIMEOUT'
                ? [`${closed} of ${plural(total, 'obligation')} were closed before the limit; none is claimed as proved.`]
                : [];
            return verdict('timed-out', 'err', 'error', 'Timed out', 'Time limit reached', body, caveats);
        }
        case 'ProvisionFailed':
            return verdict('provision-failed', 'err', 'error', "Couldn't start", "Couldn't start a worker",
                'This is a problem on the proof server, not with your file. Run it again later.',
                job.errorReason ? [job.errorReason] : []);
        case 'Lost':
            return verdict('lost', 'warn', 'warning', 'Worker lost', 'Worker lost',
                'The worker stopped responding. The server retries lost jobs automatically; if this one stays lost, run it again.');
        case 'Canceled':
            return verdict('canceled', 'muted', 'canceled', 'Canceled', 'Canceled',
                'The run was canceled before it finished.');
        default:
            return verdict('failed', 'err', 'error', 'Failed', 'Not proved',
                errorCodeText(code) ?? job.errorReason ?? 'No proof was produced for this run.',
                compileOnlyNote);
    }
}

const COMPILE_ONLY = (): Verdict =>
    verdict('compile-only', 'warn', 'warning', 'Compiled only', 'Compiled — nothing was proved',
        'The file compiled with every proof hole admitted and its goals were extracted. No proof was accepted or returned.');

/**
 * The verdict for the job panel: the job detail plus, for finished runs, the
 * terminal result and its verifier evidence.
 */
export function jobVerdict(input: VerdictInput): Verdict {
    const { job } = input;
    const live = liveVerdict(job);
    if (live) {
        return live;
    }
    const result = input.result ?? null;
    const resultState: ResultState = input.resultState ?? (result ? 'loaded' : 'n/a');
    const obligations = job.obligations ?? [];
    const total = job.holesTotal ?? 0;
    const closed = job.holesClosed ?? 0;
    const provedTargets = obligations.filter((o) => o.status === 'proved').map((o) => o.name);
    const outcome = normalizedVerifierOutcome(result);
    const hasVerifierVerdict = outcome !== null;
    const matches = result ? resultMatchesJob(job, result) : false;

    if (job.status === 'CompileGoals') {
        return COMPILE_ONLY();
    }

    if (job.status === 'Succeeded') {
        if (job.mode === 'compile-goals') {
            return COMPILE_ONLY();
        }
        if (job.mode !== 'prove') {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not a proof result',
                'This run was not recorded as a proof run, so no proof is claimed.');
        }
        const complete =
            typeof job.holesTotal === 'number' && job.holesTotal > 0 &&
            typeof job.holesClosed === 'number' && job.holesClosed === job.holesTotal;
        if (!complete) {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not proved',
                'The record does not show every proof hole closed, so no proof is claimed.');
        }
        if (result && !matches) {
            return verdict('conflict', 'warn', 'warning', 'Inconsistent', 'Inconsistent record',
                'The job record and its result disagree, so this view makes no proof claim.');
        }
        if (result && hasVerifierVerdict && !verifierAcceptedFull(result, provedTargets)) {
            return verdict('rejected', 'err', 'error', 'Rejected', 'Independent verification failed',
                'The independent verifier did not accept this result, so no proof is claimed.',
                verifierRejections(result, provedTargets, false));
        }
        if (job.claimClass !== 'Verified' && job.claimClass !== 'StructuralOnly') {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Behavior not verified',
                job.claimClass
                    ? 'The server did not classify this result as a behavioral proof, so the closed obligations are not shown as proved behavior.'
                    : 'The server did not provide a claim class, so the closed obligations are not shown as proved behavior.');
        }
        // Structural and behavioral claims need the same evidence: a matching
        // result the independent verifier fully accepted (as in the portal).
        if (!result) {
            return resultState === 'failed'
                ? verdict('unverified', 'warn', 'warning', 'Not verified', 'Verifier evidence unavailable',
                    "The verifier's verdict could not be loaded, so this view makes no proof claim. Refresh to try again.")
                : verdict('loading', 'muted', 'loading', 'Loading…', "Loading the verifier's verdict…",
                    'The result is shown once the independent verifier evidence has loaded.');
        }
        if (!hasVerifierVerdict) {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not independently verified',
                'This result was checked only inside the worker; no independent verification is recorded, so no proof is claimed.');
        }
        const structural = job.claimClass === 'StructuralOnly';
        // isProvedSuccess checks the status, mode and counts; a structural run
        // carries its own claim class, which resultMatchesJob compares.
        const claimFor = (claimClass: ClaimClass | null | undefined) => (structural ? 'Verified' : claimClass);
        const proved =
            isProvedSuccess(job.status, job.mode, job.holesTotal, job.holesClosed, claimFor(job.claimClass)) &&
            isProvedSuccess(result.status, result.mode, result.holesTotal, result.holesClosed, claimFor(result.claimClass)) &&
            matches &&
            verifierAcceptedFull(result, provedTargets);
        if (!proved) {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not proved',
                'This view cannot confirm a proof for this run.');
        }
        if (structural) {
            return verdict('structural', 'warn', 'warning', 'Structural only', 'Structural validity only',
                `All ${plural(total, 'obligation')} closed and independently re-verified. They establish structural validity (Wasm module typing) only — this is not a functional-correctness result.`,
                contentCaveats(obligations), 'structural');
        }
        const grounded = obligations.filter((o) => o.content === 'grounded').length;
        return verdict('verified', 'ok', 'pass', 'Verified', 'Proved and independently verified',
            `All ${plural(total, 'obligation')} closed and re-checked by the independent verifier.${grounded > 0 ? ` ${grounded} ${grounded === 1 ? 'states a behavioral property' : 'state behavioral properties'} of the compiled Wasm.` : ''}`,
            contentCaveats(obligations), 'proved');
    }

    if (job.status === 'PartialSuccess') {
        const partialCounts = job.mode === 'prove' && total > 0 && closed > 0 && closed < total;
        if (!partialCounts) {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not a valid partial proof',
                'A partial proof needs some, but not all, obligations closed in a proof run, so no proof is claimed.');
        }
        if (result && !matches) {
            return verdict('conflict', 'warn', 'warning', 'Inconsistent', 'Inconsistent record',
                'The job record and its result disagree, so this view makes no proof claim.');
        }
        if (job.claimClass !== 'PartialVerified') {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Partial result not verified',
                'The server did not classify this as a verified partial result, so no proof is claimed.');
        }
        if (!result) {
            return resultState === 'failed'
                ? verdict('unverified', 'warn', 'warning', 'Not verified', 'Verifier evidence unavailable',
                    "The verifier's verdict could not be loaded, so this view makes no proof claim. Refresh to try again.")
                : verdict('loading', 'muted', 'loading', 'Loading…', "Loading the verifier's verdict…",
                    'The result is shown once the independent verifier evidence has loaded.');
        }
        if (!hasVerifierVerdict) {
            return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not independently verified',
                'This partial result was checked only inside the worker; no independent verification is recorded.');
        }
        if (verifierAcceptedPartial(result, provedTargets)) {
            return verdict('partial', 'warn', 'warning', 'Partially proved', 'Partially proved',
                `${closed} of ${plural(total, 'obligation')} closed and independently re-verified; the other ${total - closed} remain admitted (unproved).`,
                contentCaveats(obligations.filter((o) => o.status === 'proved')), 'partial');
        }
        return verdict('rejected', 'err', 'error', 'Rejected', 'Independent verification failed',
            'The independent verifier did not accept this partial result, so no proof is claimed.',
            verifierRejections(result, provedTargets, true));
    }

    return terminalFailure(job, input);
}

/**
 * The verdict for a job list row (no result, no events). The list DTO carries
 * the server's claim class, which the server sets to Verified only after the
 * independent verifier accepted the run.
 */
export function listVerdict(job: JobResponse): Verdict {
    const live = liveVerdict(job);
    if (live) {
        return live;
    }
    const total = job.holesTotal ?? 0;
    const closed = job.holesClosed ?? 0;
    if (job.status === 'CompileGoals' || (job.status === 'Succeeded' && job.mode === 'compile-goals')) {
        return COMPILE_ONLY();
    }
    if (job.status === 'Succeeded') {
        if (isProvedSuccess(job.status, job.mode, job.holesTotal, job.holesClosed, job.claimClass)) {
            return verdict('verified', 'ok', 'pass', 'Verified', 'Proved and independently verified',
                `All ${plural(total, 'obligation')} closed and re-checked by the independent verifier.`, [], 'proved');
        }
        const complete = job.mode === 'prove' && total > 0 && closed === total;
        if (complete && job.claimClass === 'StructuralOnly') {
            return verdict('structural', 'warn', 'warning', 'Structural only', 'Structural validity only',
                `All ${plural(total, 'obligation')} closed. They establish structural validity only — not functional correctness.`,
                [], 'structural');
        }
        return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not verified',
            'This result is not shown as a proof. Open it for details.');
    }
    if (job.status === 'PartialSuccess') {
        if (job.mode === 'prove' && job.claimClass === 'PartialVerified' && closed > 0 && closed < total) {
            return verdict('partial', 'warn', 'warning', 'Partially proved', 'Partially proved',
                `${closed} of ${plural(total, 'obligation')} closed and independently re-verified; the rest remain admitted.`,
                [], 'partial');
        }
        return verdict('unverified', 'warn', 'warning', 'Not verified', 'Not verified',
            'This partial result is not shown as a proof. Open it for details.');
    }
    return terminalFailure(job, { job });
}
