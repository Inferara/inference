/**
 * What a job's event history says about its run: when each step started and
 * finished, how each obligation was closed, and a readable activity list.
 * Pure — no `vscode` import.
 *
 * Event types and payloads per docs/CONTRACTS.md ("Event types"). The job
 * DTO stays the authority for status and counts; this model only adds what
 * the DTO does not carry (step times, `by`, source lines, the deadline).
 */

import { AGENT_LIMIT_MESSAGES, summarize, summarizeActivity } from './eventFormat';
import type { EventEnvelope } from './types';

/** Activity rows kept per job, in the host and in the webview. */
export const EVENT_LOG_CAP = 2000;

/** Steps: lifecycle (step), agent transcript (agent), problems, diagnostics. */
export type ActivityCategory = 'step' | 'agent' | 'warn' | 'error' | 'detail';

export interface ActivityRow {
    seq: number;
    /** RFC 3339 event time. */
    ts: string;
    cat: ActivityCategory;
    /** Obligation the row belongs to, when the event names one. */
    obligation?: string;
    attempt?: number;
    text: string;
}

export type RunStep = 'submitted' | 'worker' | 'proving' | 'verifying';

export interface RunModel {
    /** Highest event `seq` applied (the dedupe cursor). */
    lastSeq: number;
    /** When each step started / finished, from event times. */
    reached: Partial<Record<RunStep, string>>;
    finished: Partial<Record<RunStep, string>>;
    /** From `job.queued` (older servers do not put it on the job). */
    deadlineUtc?: string;
    /** Time of the event that ended the run, when one did. */
    endedAt?: string;
    /** Time of the newest event, heartbeats included. */
    lastSignalAt?: string;
    /** 1-based line of each obligation in the submitted `.v`. */
    sourceLines: Record<string, number>;
    provedBy: Record<string, 'template' | 'agent'>;
    attempts: Record<string, number>;
    integrityWarnings: string[];
    rows: ActivityRow[];
}

export function newRunModel(): RunModel {
    return {
        lastSeq: 0,
        reached: {},
        finished: {},
        sourceLines: {},
        provedBy: {},
        attempts: {},
        integrityWarnings: [],
        rows: [],
    };
}

function asRecord(payload: unknown): Record<string, unknown> {
    return typeof payload === 'object' && payload !== null
        ? (payload as Record<string, unknown>)
        : {};
}

function str(v: unknown): string | undefined {
    return typeof v === 'string' && v.trim().length > 0 ? v : undefined;
}

function num(v: unknown): number | undefined {
    return typeof v === 'number' && Number.isFinite(v) ? v : undefined;
}

function plural(n: number, one: string, many = `${one}s`): string {
    return `${n} ${n === 1 ? one : many}`;
}

function fmtMs(ms: number): string {
    if (ms < 1000) {
        return `${ms} ms`;
    }
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`;
}

/** Plain-language reason for a server error code; null when it has none. */
export function errorCodeText(code: string | null | undefined): string | null {
    switch (code) {
        case 'compile_failed':
            return 'The submitted file did not compile on the proof server.';
        case 'completed_compile_failed':
            return 'The returned proof did not compile.';
        case 'spec_refuted':
            return 'A specification was refuted.';
        case 'harness_error':
            return 'The proof worker hit an internal error. This is not a problem with your file.';
        case 'worker_terminated':
            return 'The worker was stopped before it finished.';
        case 'WORKER_LOST':
            return 'The worker stopped responding too many times.';
        case 'QUEUE_TIMEOUT':
            return 'No worker became free before the time budget ran out.';
        case 'TIMED_OUT':
            return 'The time budget ran out.';
        case 'VERIFIER_UNAVAILABLE':
            return 'The independent verifier was unavailable, so the result could not be checked.';
        case 'VERIFIER_ERROR':
            return 'The independent verifier failed, so the result could not be checked.';
        default:
            return null;
    }
}

function markReached(run: RunModel, step: RunStep, ts: string): void {
    run.reached[step] ??= ts;
}

function markFinished(run: RunModel, step: RunStep, ts: string): void {
    markReached(run, step, ts);
    run.finished[step] ??= ts;
}

/** The step and timing side of one event. */
function applyTiming(run: RunModel, env: EventEnvelope, p: Record<string, unknown>): void {
    const ts = env.ts;
    switch (env.type) {
        case 'job.accepted':
            markFinished(run, 'submitted', ts);
            break;
        case 'job.queued':
            markFinished(run, 'submitted', ts);
            run.deadlineUtc = str(p.deadlineUtc) ?? run.deadlineUtc;
            break;
        case 'job.provisioning':
        case 'vm.booting':
            markFinished(run, 'submitted', ts);
            markReached(run, 'worker', ts);
            break;
        case 'vm.online':
            markFinished(run, 'submitted', ts);
            markFinished(run, 'worker', ts);
            break;
        case 'job.compiling':
        case 'obligations.discovered':
        case 'job.proving':
        case 'obligation.started':
        case 'obligation.attempt':
        case 'obligation.proved':
        case 'obligation.failed':
        case 'obligation.refuted':
        case 'agent.activity':
            markFinished(run, 'submitted', ts);
            markFinished(run, 'worker', ts);
            markReached(run, 'proving', ts);
            break;
        case 'job.verifying':
            markFinished(run, 'proving', ts);
            markReached(run, 'verifying', ts);
            break;
        case 'job.completed':
            if (run.reached.verifying) {
                markFinished(run, 'verifying', ts);
            } else if (run.reached.proving) {
                markFinished(run, 'proving', ts);
            }
            run.endedAt = ts;
            break;
        case 'job.errored':
        case 'job.cancelled':
            run.endedAt = ts;
            break;
    }
}

/** Obligation facts the job DTO does not carry. */
function applyObligation(run: RunModel, env: EventEnvelope, p: Record<string, unknown>): void {
    const name = str(p.name);
    switch (env.type) {
        case 'obligations.discovered': {
            const list = Array.isArray(p.obligations) ? p.obligations : [];
            for (const item of list) {
                const o = asRecord(item);
                const oName = str(o.name);
                const line = num(o.sourceLine);
                if (oName && line !== undefined && line > 0) {
                    run.sourceLines[oName] = line;
                }
            }
            break;
        }
        case 'obligation.attempt': {
            const attempt = num(p.attempt);
            if (name && attempt !== undefined) {
                run.attempts[name] = attempt;
            }
            break;
        }
        case 'obligation.proved': {
            if (name) {
                const attempt = num(p.attempt);
                // Template proofs say so (`by: "template"`, attempt 0); agent
                // proofs carry no `by`.
                run.provedBy[name] = p.by === 'template' || attempt === 0 ? 'template' : 'agent';
                if (attempt !== undefined && attempt > 0) {
                    run.attempts[name] = attempt;
                }
            }
            break;
        }
    }
}

/** The activity row for one event, or null when it adds nothing to read. */
function rowFor(env: EventEnvelope, p: Record<string, unknown>): Omit<ActivityRow, 'seq' | 'ts'> | null {
    const name = str(p.name);
    const attempt = num(p.attempt);
    switch (env.type) {
        case 'heartbeat':
        case 'usage.tick':
            return null;
        case 'job.accepted': {
            const holes = num(p.holes);
            return {
                cat: 'step',
                text: `Accepted${holes !== undefined ? ` — ${plural(holes, 'proof hole')}` : ''}`,
            };
        }
        case 'job.queued':
            return { cat: 'step', text: 'Waiting for a worker' };
        case 'job.provisioning': {
            const n = num(p.attempt);
            return {
                cat: 'step',
                text: n !== undefined && n > 1
                    ? `Preparing an isolated worker (attempt ${n})`
                    : 'Preparing an isolated worker',
            };
        }
        case 'vm.booting':
            return { cat: 'step', text: 'Worker is starting' };
        case 'vm.online':
            return { cat: 'step', text: 'Worker ready' };
        case 'job.compiling':
            return { cat: 'step', text: 'Checking that the submitted file compiles' };
        case 'obligations.discovered': {
            const n = Array.isArray(p.obligations) ? p.obligations.length : undefined;
            return {
                cat: 'step',
                text: n !== undefined ? `Found ${plural(n, 'proof obligation')}` : 'Proof obligations identified',
            };
        }
        case 'job.proving': {
            const holes = num(p.holes);
            return {
                cat: 'step',
                text: holes !== undefined ? `Proving ${plural(holes, 'obligation')}` : 'Proving obligations',
            };
        }
        case 'obligation.started':
            return { cat: 'step', obligation: name, text: 'Started' };
        case 'obligation.attempt':
            return {
                cat: 'step',
                obligation: name,
                text: attempt !== undefined ? `Attempt ${attempt}` : 'New attempt',
            };
        case 'obligation.proved': {
            const duration = num(p.durationMs);
            const byTemplate = p.by === 'template' || attempt === 0;
            const how = byTemplate ? 'Proved by a built-in template' : 'Proved';
            const extra = [
                !byTemplate && attempt !== undefined ? `attempt ${attempt}` : undefined,
                duration !== undefined ? fmtMs(duration) : undefined,
            ].filter(Boolean);
            return {
                cat: 'step',
                obligation: name,
                text: extra.length ? `${how} (${extra.join(', ')})` : how,
            };
        }
        case 'obligation.failed':
            return {
                cat: 'warn',
                obligation: name,
                text: `Not closed${str(p.error) ? ` — ${str(p.error)}` : ''}`,
            };
        case 'obligation.refuted':
            return {
                cat: 'warn',
                obligation: name,
                text: 'Refuted — a machine-checked counterexample was accepted',
            };
        case 'job.verifying':
            return { cat: 'step', text: 'Independently verifying the returned proof' };
        case 'job.completed':
            return { cat: 'step', text: 'Run finished' };
        case 'job.cancelling':
            return { cat: 'step', text: 'Cancellation requested' };
        case 'job.cancelled':
            return { cat: 'step', text: 'Canceled' };
        case 'job.errored': {
            const code = str(p.errorCode);
            const reason = errorCodeText(code) ?? str(p.errorReason) ?? 'The run ended with an error.';
            return { cat: 'error', text: reason };
        }
        case 'integrity.warning':
            return {
                cat: 'warn',
                text: `Integrity warning — ${str(p.detail) ?? str(p.message) ?? 'this run needs review'}`,
            };
        case 'agent.limited': {
            const code = str(p.code);
            return {
                cat: 'warn',
                text: (code && AGENT_LIMIT_MESSAGES[code]) ?? 'A generation limit was reported.',
            };
        }
        case 'agent.activity': {
            const theorem = str(p.theorem);
            return {
                cat: 'agent',
                obligation: theorem,
                attempt,
                text: summarizeActivity(p),
            };
        }
        case 'setup.progress':
        case 'log': {
            const message = str(p.message) ?? str(p.step);
            return message ? { cat: 'detail', text: message } : null;
        }
        default: {
            const summary = summarize(env);
            return { cat: 'detail', text: summary ? `${env.type} — ${summary}` : env.type };
        }
    }
}

/**
 * Apply one event. Events at or below the cursor are replays (the stream and
 * the poll fallback overlap) and change nothing. Returns whether the event was
 * new and the activity row it added, if any.
 */
export function applyEvent(
    run: RunModel,
    env: EventEnvelope,
): { applied: boolean; row: ActivityRow | null } {
    if (typeof env.seq === 'number') {
        if (env.seq <= run.lastSeq) {
            return { applied: false, row: null };
        }
        run.lastSeq = env.seq;
    }
    const p = asRecord(env.payload);
    if (typeof env.ts === 'string' && !Number.isNaN(Date.parse(env.ts))) {
        run.lastSignalAt = env.ts;
        applyTiming(run, env, p);
    }
    applyObligation(run, env, p);
    if (env.type === 'integrity.warning') {
        run.integrityWarnings.push(str(p.detail) ?? str(p.message) ?? 'This run needs review.');
    }
    const partial = rowFor(env, p);
    if (!partial) {
        return { applied: true, row: null };
    }
    const row: ActivityRow = {
        seq: typeof env.seq === 'number' ? env.seq : run.lastSeq,
        ts: env.ts,
        ...partial,
        text: partial.text.length > 600 ? `${partial.text.slice(0, 600)}…` : partial.text,
    };
    if (!row.obligation) {
        delete row.obligation;
    }
    if (row.attempt === undefined) {
        delete row.attempt;
    }
    run.rows.push(row);
    if (run.rows.length > EVENT_LOG_CAP) {
        run.rows.splice(0, run.rows.length - EVENT_LOG_CAP);
    }
    return { applied: true, row };
}

/** Whether an event can change what the panel shows above the activity list. */
export function changesSummary(type: string): boolean {
    return type !== 'agent.activity' && type !== 'heartbeat' && type !== 'usage.tick' && type !== 'log';
}
