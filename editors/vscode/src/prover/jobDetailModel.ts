/**
 * The job panel's view model: everything the panel shows, worked out from the
 * job, its result and its event history. Pure — no `vscode` import, no HTML.
 */

import { fileStem, type BuildError } from './buildLog';
import { isCancelableStatus, isStreamTerminalStatus, isTerminalStatus, isTextArtifactKind } from './jobStatus';
import { jobVerdict, type ResultState, type Tone, type Verdict } from './jobVerdict';

/** Shown until a restored panel has fetched its job. */
const LOADING_JOB: Verdict = {
    kind: 'loading',
    tone: 'muted',
    icon: 'loading',
    badge: 'Loading…',
    headline: 'Loading this proof job…',
    body: 'The job is shown once the proof server has answered.',
    caveats: [],
    claim: 'none',
};
import { newRunModel, type RunModel, type RunStep } from './runModel';
import type { ArtifactInfo, AssumptionReport, JobResponse, JobResultResponse, ObligationDto } from './types';
import { normalizedAssumptionReports, normalizedVerifierOutcome, verifierAcceptedPartial } from './verifierEvidence';

/** Live-update transport the hosting view is in. */
export type LiveMode = 'sse' | 'poll' | 'terminal';

export interface JobViewInput {
    job: JobResponse;
    result?: JobResultResponse | null;
    /** Defaults: `loaded` with a result, `loading` for a finished job without one. */
    resultState?: ResultState;
    /** False until the job detail (not just a list row) has been fetched. */
    detailLoaded?: boolean;
    /** False for a panel restored after a reload, before its first fetch. */
    statusKnown?: boolean;
    run?: RunModel;
    buildError?: BuildError | null;
    /** The `.inf` the user proved, when this extension compiled the upload. */
    source?: string | null;
    /** Fetch-failure text; stale data stays visible below it. */
    error?: string | null;
    live: LiveMode;
}

export interface ActionView {
    /** `data-action` the webview posts back. */
    action: string;
    label: string;
    icon?: 'file' | 'diff' | 'linkExternal' | 'log' | 'goto' | 'rerun' | 'refresh' | 'stop' | 'copy' | 'download';
    /** Accessible name when the label alone is ambiguous. */
    ariaLabel?: string;
    /** Extra `data-*` attributes (escaped at render). */
    data?: Record<string, string>;
    style?: 'primary' | 'secondary' | 'danger';
}

/** `stopped`: the step a finished run ended in without completing it. */
export type StepState = 'done' | 'active' | 'todo' | 'unknown' | 'stopped' | 'final';

export interface StepView {
    key: RunStep | 'result';
    label: string;
    state: StepState;
    tone?: Tone;
    /** When the step started (done/active), for the tooltip. */
    at?: string;
}

export interface ObligationView {
    name: string;
    shortName: string;
    status: string;
    statusLabel: string;
    tone: Tone;
    kindLabel: string;
    contentLabel: string;
    by?: 'template' | 'agent';
    attempts?: number;
    duration?: string;
    goal?: string;
    lastError?: string;
    specList?: string;
    /**
     * Where "Go to" leads: the theorem's proof in the returned file once the
     * obligation is proved, else its statement in the submitted file. The host
     * finds the theorem by name; `line` (the proof hole the server reported)
     * is the fallback for the submitted file.
     */
    goto: { target: 'proof'; artifact: string } | { target: 'source'; line?: number };
    group: 'active' | 'problem' | 'waiting' | 'done';
}

export interface CheckView {
    label: string;
    state: 'pass' | 'fail' | 'warn' | 'unknown';
    detail?: string;
}

export interface EvidenceView {
    kind: 'verdict' | 'worker-only' | 'not-applicable';
    outcome?: string;
    verifiedAt?: string;
    checks: CheckView[];
    policyId?: string;
    /** Null when the per-theorem evidence is missing or malformed. */
    reports: AssumptionReport[] | null;
}

export interface FileView {
    id: string;
    kind: string;
    kindLabel: string;
    filename: string;
    size: string;
    text: boolean;
}

export interface TechRow {
    field: string;
    label: string;
    value: string;
    mono?: boolean;
}

export interface ClockView {
    /** Budget start (submission; queue time counts). */
    start?: string;
    deadline?: string;
    /** When the run ended, for finished jobs whose history says so. */
    end?: string;
    budgetSeconds?: number;
    running: boolean;
}

export interface JobView {
    jobId: string;
    title: string;
    /** The uploaded `.v` name, shown under an `.inf` title. */
    subtitle?: string;
    verdict: Verdict;
    /** The verdict body is compiler output (shown as code). */
    verdictBodyIsCode: boolean;
    steps: StepView[];
    progress: { known: boolean; total: number; closed: number; failed: number };
    clock: ClockView;
    /** Null while the job detail is loading. */
    obligations: ObligationView[] | null;
    actions: ActionView[];
    headerActions: ActionView[];
    /** Null for runs that have not finished (or whose result is loading). */
    evidence: EvidenceView | null;
    files: FileView[] | null;
    tech: TechRow[];
    banner: { tone: 'err' | 'warn'; text: string } | null;
    live: LiveMode;
    terminal: boolean;
}

const KIND_LABELS: Record<string, string> = {
    module: 'structure',
    spec: 'behavior',
    exists_spec: 'existence',
    unique_spec: 'uniqueness',
};

const CONTENT_LABELS: Record<string, string> = {
    grounded: 'States a behavioral property of the compiled module',
    structural: 'Structural (module typing)',
    empty: 'Empty specification',
    tautology: 'Tautology — true for any program',
    unguarded: 'Not tied to the compiled module',
};

const STATUS_LABELS: Record<string, [string, Tone]> = {
    pending: ['Waiting', 'muted'],
    running: ['Working', 'active'],
    proved: ['Proved', 'ok'],
    failed: ['Not closed', 'err'],
    skipped: ['Skipped — out of time', 'warn'],
    refuted: ['Refuted', 'refuted'],
};

const ARTIFACT_LABELS: Record<string, string> = {
    InputV: 'Submitted file',
    CompletedV: 'Completed proof',
    Report: 'Run report',
    Goals: 'Proof goals',
    BuildLog: 'Build log',
    AgentLog: 'Agent log',
    RefutationV: 'Refutation',
    RefutationMd: 'Refutation notes',
};

export function fmtDuration(ms: number | null | undefined): string | undefined {
    if (ms === null || ms === undefined || !Number.isFinite(ms)) {
        return undefined;
    }
    if (ms < 1000) {
        return `${ms} ms`;
    }
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`;
}

export function fmtBudget(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return [h ? `${h}h` : '', m ? `${m}m` : '', s || (!h && !m) ? `${s}s` : '']
        .filter(Boolean)
        .join(' ');
}

export function fmtBytes(bytes: number): string {
    if (bytes < 1024) {
        return `${bytes} B`;
    }
    const kib = bytes / 1024;
    return kib < 1024 ? `${kib.toFixed(1)} KiB` : `${(kib / 1024).toFixed(2)} MiB`;
}

function fmtTimestamp(iso: string | null | undefined): string {
    if (!iso) {
        return '—';
    }
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

/** `valid_main__Comm` → `Comm`; names without a `__` part stay whole. */
export function shortObligationName(name: string): string {
    const i = name.lastIndexOf('__');
    return i > 0 && i + 2 < name.length ? name.slice(i + 2) : name;
}

const GROUP_ORDER: Record<ObligationView['group'], number> = { active: 0, problem: 1, waiting: 2, done: 3 };

/**
 * An obligation closed by the worker is "Proved" only once the run's verdict
 * claims a proof; while running it is "Closed", and in a finished run without
 * a claim it is "Closed — not verified".
 */
function obligationView(
    o: ObligationDto,
    run: RunModel,
    finished: boolean,
    claimed: boolean,
    completedId: string | undefined,
): ObligationView {
    const status = o.status ?? 'pending';
    const [statusLabel, tone]: [string, Tone] =
        finished && (status === 'pending' || status === 'running')
            ? ['Not attempted', 'muted']
            : status === 'proved' && !claimed
              ? finished ? ['Closed — not verified', 'warn'] : ['Closed', 'ok']
              : STATUS_LABELS[status] ?? [status, 'muted'];
    const group: ObligationView['group'] =
        status === 'running' ? 'active'
            : status === 'failed' || status === 'refuted' ? 'problem'
                : status === 'proved' ? 'done' : 'waiting';
    const view: ObligationView = {
        name: o.name,
        shortName: shortObligationName(o.name),
        status,
        statusLabel,
        tone,
        kindLabel: KIND_LABELS[o.kind] ?? 'other',
        contentLabel: CONTENT_LABELS[o.content ?? 'unknown'] ?? 'Not classified',
        goto: { target: 'source' },
        group,
    };
    const by = status === 'proved' ? run.provedBy[o.name] : undefined;
    if (by) view.by = by;
    const attempts = o.attempts ?? run.attempts[o.name];
    if (typeof attempts === 'number' && attempts > 0) view.attempts = attempts;
    const duration = fmtDuration(o.durationMs);
    if (duration) view.duration = duration;
    if (o.goal) view.goal = o.goal;
    if (o.lastError) view.lastError = o.lastError;
    if (o.specList) view.specList = o.specList;
    const line = o.sourceLine ?? run.sourceLines[o.name];
    if (status === 'proved' && completedId) {
        view.goto = { target: 'proof', artifact: completedId };
    } else if (typeof line === 'number' && line > 0) {
        view.goto = { target: 'source', line };
    }
    return view;
}

function steps(job: JobResponse, run: RunModel, verdict: Verdict): StepView[] {
    const order: Array<[RunStep, string]> = [
        ['submitted', 'Submitted'],
        ['worker', 'Worker'],
        ['proving', 'Proving'],
        ['verifying', 'Verifying'],
    ];
    const activeIndex: Record<string, number> = {
        Accepted: 0,
        Queued: 1,
        Provisioning: 1,
        Booting: 1,
        Running: 2,
        Verifying: 3,
    };
    const at = (key: RunStep) => run.reached[key];
    const live = activeIndex[job.status];
    if (live !== undefined) {
        return [
            ...order.map(([key, label], i): StepView => ({
                key,
                label,
                state: i < live ? 'done' : i === live ? 'active' : 'todo',
                ...(at(key) ? { at: at(key) } : {}),
            })),
            { key: 'result', label: 'Result', state: 'todo' },
        ];
    }
    // Finished or canceling: a step counts as done only when the history shows
    // it (a run that never got a worker did not "prove"). Submitted always was.
    // A run that did not succeed stopped in the last step it reached.
    const reached = order.map(([key]) => key === 'submitted' || Boolean(run.reached[key]));
    const last = reached.lastIndexOf(true);
    const stoppedAt =
        verdict.claim === 'none' && last > 0 && !run.finished[order[last][0]] ? last : -1;
    return [
        ...order.map(([key, label], i): StepView => ({
            key,
            label,
            state: i === stoppedAt ? 'stopped' : reached[i] ? 'done' : 'unknown',
            ...(i === stoppedAt ? { tone: verdict.tone } : {}),
            ...(at(key) ? { at: at(key) } : {}),
        })),
        {
            key: 'result',
            label: verdict.badge,
            state: job.status === 'Canceling' ? 'active' : 'final',
            tone: verdict.tone,
        },
    ];
}

function evidence(job: JobResponse, result: JobResultResponse): EvidenceView {
    const outcome = normalizedVerifierOutcome(result);
    if (outcome === null) {
        return {
            kind: job.mode === 'compile-goals' || job.status === 'CompileGoals' ? 'not-applicable' : 'worker-only',
            checks: [],
            reports: null,
        };
    }
    const provedTargets = (job.obligations ?? []).filter((o) => o.status === 'proved').map((o) => o.name);
    const partial = job.status === 'PartialSuccess' && verifierAcceptedPartial(result, provedTargets);
    const flag = (value: boolean | null | undefined, good: boolean): CheckView['state'] =>
        value === null || value === undefined ? 'unknown' : value === good ? 'pass' : 'fail';
    const reports = normalizedAssumptionReports(result);
    const complete = result.verifiedClean === true && result.admitsDetected === false;
    const admitted = Math.max(0, (result.holesTotal ?? 0) - (result.holesClosed ?? 0));
    const checks: CheckView[] = [
        { label: 'The returned file compiles', state: flag(result.compileOk, true) },
        partial
            ? {
                  label: 'Every closed proof is complete',
                  state: 'warn',
                  detail: `${admitted} unproved ${admitted === 1 ? 'obligation remains' : 'obligations remain'} admitted, as expected for a partial result.`,
              }
            : {
                  label: 'Every proof is complete (no admits)',
                  state: result.verifiedClean === null || result.verifiedClean === undefined ||
                      result.admitsDetected === null || result.admitsDetected === undefined
                      ? 'unknown'
                      : complete ? 'pass' : 'fail',
              },
        { label: 'Theorem statements are unchanged', state: flag(result.statementsImmutable, true) },
        {
            label: 'No proof markers left',
            state: result.markersRemaining === null || result.markersRemaining === undefined
                ? 'unknown'
                : result.markersRemaining === 0 ? 'pass' : 'fail',
            ...(typeof result.markersRemaining === 'number' && result.markersRemaining > 0
                ? { detail: `${result.markersRemaining} left` }
                : {}),
        },
        {
            label: 'Only approved assumptions are used',
            state: result.assumptionsClosed !== true
                ? flag(result.assumptionsClosed, true)
                : reports && reports.every((r) => r.policyPassed) ? 'pass' : 'fail',
        },
    ];
    return {
        kind: 'verdict',
        outcome,
        ...(result.verifiedAt ? { verifiedAt: fmtTimestamp(result.verifiedAt) } : {}),
        checks,
        ...(result.assumptionPolicyId?.trim() ? { policyId: result.assumptionPolicyId } : {}),
        reports: result.assumptionPolicyId?.trim() ? reports : null,
    };
}

function verdictActions(job: JobResponse, result: JobResultResponse | null, verdict: Verdict, buildError: BuildError | null | undefined): ActionView[] {
    const actions: ActionView[] = [];
    const artifacts = result?.artifacts ?? [];
    const find = (kind: string) => artifacts.find((a) => a.kind === kind);
    const completed = find('CompletedV');
    if (completed) {
        actions.push({
            action: 'openArtifact', label: 'Open proof', icon: 'file', style: 'primary',
            ariaLabel: `Open the completed proof ${completed.filename}`, data: { artifact: completed.id },
        });
        if (find('InputV')) {
            actions.push({ action: 'compare', label: 'Compare with input', icon: 'diff', ariaLabel: 'Compare the completed proof with the submitted file' });
        }
    }
    const refutation = find('RefutationV');
    if (refutation) {
        actions.push({
            action: 'openArtifact', label: 'Open refutation', icon: 'file', style: completed ? 'secondary' : 'primary',
            ariaLabel: `Open the refutation ${refutation.filename}`, data: { artifact: refutation.id },
        });
    }
    if (verdict.kind === 'compile-error' || verdict.kind === 'completed-compile-error') {
        const input = find(verdict.kind === 'compile-error' ? 'InputV' : 'CompletedV');
        if (buildError?.line !== undefined && input) {
            actions.push({
                action: 'gotoLine', label: `Go to line ${buildError.line}`, icon: 'goto', style: 'primary',
                ariaLabel: `Go to line ${buildError.line} of ${input.filename}`,
                data: { artifact: input.id, line: String(buildError.line) },
            });
        }
        const log = find('BuildLog');
        if (log) {
            actions.push({ action: 'openArtifact', label: 'Open build log', icon: 'log', ariaLabel: 'Open the build log', data: { artifact: log.id } });
        }
    }
    if (isTerminalStatus(job.status)) {
        const proofLike = job.status === 'Succeeded' || job.status === 'PartialSuccess' || job.status === 'CompileGoals';
        actions.push(proofLike
            ? { action: 'openCertificate', label: 'Certificate', icon: 'linkExternal', ariaLabel: 'Open the certificate in the portal' }
            : { action: 'openPortal', label: 'Open in portal', icon: 'linkExternal', ariaLabel: 'Open this job in the portal' });
    }
    if (['failed', 'timed-out', 'provision-failed', 'lost', 'canceled'].includes(verdict.kind)) {
        actions.push({ action: 'resubmit', label: 'Run Again', icon: 'rerun', ariaLabel: 'Run this proof job again' });
    }
    return actions;
}

function techRows(job: JobResponse, clock: ClockView, live: LiveMode): TechRow[] {
    const rows: TechRow[] = [
        { field: 'id', label: 'Job ID', value: job.id, mono: true },
        { field: 'submitted', label: 'Submitted', value: fmtTimestamp(job.createdAt) },
    ];
    if (clock.budgetSeconds) rows.push({ field: 'budget', label: 'Time budget', value: fmtBudget(clock.budgetSeconds) });
    if (clock.deadline) rows.push({ field: 'deadline', label: 'Deadline', value: fmtTimestamp(clock.deadline) });
    rows.push({ field: 'status', label: 'Status', value: job.status, mono: true });
    rows.push({ field: 'mode', label: 'Mode', value: job.mode ?? '—', mono: true });
    if (job.outcome) rows.push({ field: 'outcome', label: 'Outcome', value: job.outcome, mono: true });
    rows.push({ field: 'claimClass', label: 'Claim class', value: job.claimClass ?? 'missing', mono: true });
    if (job.errorCode) rows.push({ field: 'errorCode', label: 'Error code', value: job.errorCode, mono: true });
    if (job.errorReason) rows.push({ field: 'errorReason', label: 'Error detail', value: job.errorReason });
    rows.push({
        field: 'updates',
        label: 'Updates',
        value: live === 'sse' ? 'Live — streaming events' : live === 'poll' ? 'Polling every 5 s (live stream unavailable)' : 'Finished — auto-refresh stopped',
    });
    return rows;
}

export function buildJobView(input: JobViewInput): JobView {
    const { job } = input;
    const run = input.run ?? newRunModel();
    const result = input.result ?? null;
    const terminal = isTerminalStatus(job.status);
    const resultState: ResultState =
        input.resultState ?? (result ? 'loaded' : isStreamTerminalStatus(job.status) ? 'loading' : 'n/a');
    const detailLoaded = input.detailLoaded ?? true;
    const fileStems = [fileStem(input.source), fileStem(job.filename)].filter((s): s is string => Boolean(s));
    const statusKnown = input.statusKnown ?? true;
    const verdict = statusKnown
        ? jobVerdict({ job, result, resultState, buildError: input.buildError, fileStems })
        : LOADING_JOB;

    const budget = typeof job.maxWallClockSeconds === 'number' && job.maxWallClockSeconds > 0
        ? job.maxWallClockSeconds
        : undefined;
    const created = job.createdAt ? Date.parse(job.createdAt) : NaN;
    const deadline = job.deadlineUtc ?? run.deadlineUtc ??
        (budget !== undefined && !Number.isNaN(created) ? new Date(created + budget * 1000).toISOString() : undefined);
    const clock: ClockView = {
        ...(job.createdAt ? { start: job.createdAt } : {}),
        ...(deadline ? { deadline } : {}),
        ...(terminal && run.endedAt ? { end: run.endedAt } : {}),
        ...(budget !== undefined ? { budgetSeconds: budget } : {}),
        running: !terminal,
    };

    const completedId = result?.artifacts.find((a) => a.kind === 'CompletedV')?.id;
    const obligations = detailLoaded && statusKnown
        ? (job.obligations ?? [])
              .map((o) => obligationView(o, run, terminal, verdict.claim !== 'none', completedId))
              .map((view, index) => ({ view, index }))
              .sort((a, b) => GROUP_ORDER[a.view.group] - GROUP_ORDER[b.view.group] || a.index - b.index)
              .map(({ view }) => view)
        : null;

    const headerActions: ActionView[] = [
        { action: 'refresh', label: 'Refresh', icon: 'refresh', style: 'secondary', ariaLabel: 'Refresh this job' },
    ];
    if (statusKnown && isCancelableStatus(job.status)) {
        headerActions.push({ action: 'cancel', label: 'Cancel Job', icon: 'stop', style: 'danger' });
    }

    const title = input.source ?? job.filename ?? job.id;
    const banner = input.error
        ? { tone: 'err' as const, text: input.error }
        : run.integrityWarnings.length > 0
          ? { tone: 'warn' as const, text: `Integrity warning: ${run.integrityWarnings[run.integrityWarnings.length - 1]} This run needs review.` }
          : null;

    return {
        jobId: job.id,
        title,
        ...(input.source && job.filename && job.filename !== input.source ? { subtitle: job.filename } : {}),
        verdict,
        verdictBodyIsCode: Boolean(input.buildError) &&
            (verdict.kind === 'compile-error' || verdict.kind === 'completed-compile-error'),
        steps: statusKnown
            ? steps(job, run, verdict)
            : steps({ ...job, status: 'Accepted' }, run, verdict).map((s) => ({ ...s, state: 'todo' as const })),
        progress: {
            known: detailLoaded && typeof job.holesTotal === 'number',
            total: job.holesTotal ?? 0,
            closed: job.holesClosed ?? 0,
            failed: job.holesFailed ?? 0,
        },
        clock,
        obligations,
        actions: statusKnown ? verdictActions(job, result, verdict, input.buildError) : [],
        headerActions,
        evidence: result && ['Succeeded', 'PartialSuccess', 'CompileGoals'].includes(job.status)
            ? evidence(job, result)
            : null,
        files: result
            ? result.artifacts.map((a: ArtifactInfo) => ({
                  id: a.id,
                  kind: a.kind,
                  kindLabel: ARTIFACT_LABELS[a.kind] ?? a.kind,
                  filename: a.filename,
                  size: fmtBytes(a.sizeBytes),
                  text: isTextArtifactKind(a.kind),
              }))
            : null,
        tech: techRows(job, clock, input.live),
        banner,
        live: input.live,
        terminal,
    };
}
