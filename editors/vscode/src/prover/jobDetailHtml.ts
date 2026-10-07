/**
 * Pure HTML renderer for the proof-job detail webview.
 *
 * Deliberately free of any `vscode` import so it can be unit-tested under
 * plain node. All server-originated strings are HTML-escaped here — the
 * webview must never interpolate them raw. Styling uses VS Code theme
 * variables (`--vscode-*`) so the view follows the user's theme.
 */

import { isProvedSuccess } from './jobPresentation';
import type {
    AssumptionReport,
    JobResponse,
    JobResultResponse,
    ObligationDto,
} from './types';

/** Statuses the control plane treats as terminal (mirrors the server). */
const TERMINAL_STATUSES: ReadonlySet<string> = new Set([
    'Succeeded',
    'CompileGoals',
    'PartialSuccess',
    'Failed',
    'TimedOut',
    'Lost',
    'Canceled',
    'ProvisionFailed',
]);

/**
 * Statuses `POST /jobs/{id}/cancel` accepts (mirrors the server's switch);
 * everything else — including Verifying and Canceling — is 409 NOT_CANCELABLE.
 */
const CANCELABLE_STATUSES: ReadonlySet<string> = new Set([
    'Accepted',
    'Queued',
    'Provisioning',
    'Booting',
    'Running',
]);

/** Event-log lines kept per job, in the host and in the webview DOM. */
export const EVENT_LOG_CAP = 2000;

/** Artifact kinds that are text and open as read-only documents. */
const TEXT_ARTIFACT_KINDS: ReadonlySet<string> = new Set([
    'InputV',
    'CompletedV',
    'Report',
    'Goals',
    'BuildLog',
    'AgentLog',
    'RefutationV',
    'RefutationMd',
]);

export function isTextArtifactKind(kind: string): boolean {
    return TEXT_ARTIFACT_KINDS.has(kind);
}

/** The run phases in order; the last chip is the terminal result. */
const PHASES = ['Accepted', 'Queued', 'Provisioning', 'Booting', 'Running', 'Verifying'] as const;

export type PhaseState = 'done' | 'active' | 'todo' | 'past' | 'ok' | 'warn' | 'err';

/**
 * Phase ribbon chips for a status. A live job marks earlier phases done and
 * its own active. A finished job does not record which phases it reached
 * (ProvisionFailed never booted), so earlier chips are neutral and only the
 * result chip carries a tone.
 */
export function phaseRibbon(
    status: string,
    provedSuccess: boolean,
): Array<{ name: string; state: PhaseState }> {
    const index = (PHASES as readonly string[]).indexOf(status);
    if (index >= 0) {
        return [
            ...PHASES.map((name, i) => ({
                name,
                state: (i < index ? 'done' : i === index ? 'active' : 'todo') as PhaseState,
            })),
            { name: 'Result', state: 'todo' },
        ];
    }
    if (status === 'Canceling') {
        return [
            ...PHASES.map((name) => ({ name, state: 'past' as PhaseState })),
            { name: 'Canceling', state: 'active' },
        ];
    }
    const tone = statusClass(status, provedSuccess);
    return [
        ...PHASES.map((name) => ({ name, state: 'past' as PhaseState })),
        { name: status, state: tone === 'active' ? 'warn' : tone },
    ];
}

function fmtBudget(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return [h ? `${h}h` : '', m ? `${m}m` : '', s || (!h && !m) ? `${s}s` : '']
        .filter(Boolean)
        .join(' ');
}

export function isTerminalStatus(status: string): boolean {
    return TERMINAL_STATUSES.has(status);
}

/**
 * Statuses that end the server's event stream. `Lost` is soft-terminal: the
 * reaper re-queues it (bounded retry, Lost → Queued) and `StreamEvents`
 * deliberately keeps the SSE stream open without an `end` frame — so the
 * client must keep its transport (stream + polls) alive on Lost too, or it
 * freezes on a job that is actively being rerun.
 */
export function isStreamTerminalStatus(status: string): boolean {
    return TERMINAL_STATUSES.has(status) && status !== 'Lost';
}

export function isCancelableStatus(status: string): boolean {
    return CANCELABLE_STATUSES.has(status);
}

export function escapeHtml(value: string): string {
    return value
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

/** Map a job status onto one of the badge color classes defined in the CSS. */
function statusClass(
    status: string,
    provedSuccess: boolean,
): 'ok' | 'warn' | 'err' | 'active' {
    switch (status) {
        case 'Succeeded':
            return provedSuccess ? 'ok' : 'warn';
        case 'CompileGoals':
        case 'PartialSuccess':
        case 'Canceled':
            return 'warn';
        case 'Failed':
        case 'TimedOut':
        case 'Lost':
        case 'ProvisionFailed':
            return 'err';
        default:
            return 'active';
    }
}

function obligationStatusClass(status: string | null | undefined): string {
    switch (status) {
        case 'proved':
            return 'ok';
        case 'failed':
        case 'refuted':
            return 'err';
        case 'skipped':
            return 'warn';
        default:
            return 'active';
    }
}

function fmtDuration(ms: number | null | undefined): string {
    if (ms === null || ms === undefined) {
        return '—';
    }
    if (ms < 1000) {
        return `${ms} ms`;
    }
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`;
}

function fmtBytes(bytes: number): string {
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
    return Number.isNaN(d.getTime()) ? escapeHtml(iso) : d.toLocaleString();
}

function metaRow(label: string, valueHtml: string): string {
    return `<div class="meta-label">${escapeHtml(label)}</div><div class="meta-value">${valueHtml}</div>`;
}

function obligationRow(o: ObligationDto): string {
    const status = o.status ?? 'pending';
    const content = o.content ?? 'unknown';
    const main = `<tr>
        <td><code>${escapeHtml(o.name)}</code></td>
        <td>${escapeHtml(o.kind)}</td>
        <td>${escapeHtml(content)}</td>
        <td><span class="badge ${obligationStatusClass(status)}">${escapeHtml(status)}</span></td>
        <td class="num">${o.attempts ?? '—'}</td>
        <td class="num">${fmtDuration(o.durationMs)}</td>
        <td class="error-cell">${o.lastError ? escapeHtml(o.lastError) : ''}</td>
    </tr>`;
    // The goal state at this obligation's `Proof.` (compile-goals flow) — shown
    // as a full-width sub-row beneath the obligation so multi-line goals are readable.
    const goalRow = o.goal
        ? `<tr class="goal-row"><td colspan="7"><div class="goal-label">goal</div><pre class="goal">${escapeHtml(o.goal)}</pre></td></tr>`
        : '';
    return main + goalRow;
}

/** ✓/✗ badge for a verifier verdict flag (null = verifier did not report). */
function verdictFlag(
    label: string,
    value: boolean | null | undefined,
    goodWhen: boolean,
    unexpectedTone: 'warn' | 'err' = 'err',
): string {
    if (value === null || value === undefined) {
        return metaRow(label, '<span class="dim">—</span>');
    }
    const good = value === goodWhen;
    return metaRow(
        label,
        `<span class="badge ${good ? 'ok' : unexpectedTone}">${value ? 'yes' : 'no'}</span>`,
    );
}

function markersFlag(value: number | null | undefined): string {
    if (value === null || value === undefined) {
        return metaRow('Proof markers remaining', '<span class="dim">—</span>');
    }
    return metaRow(
        'Proof markers remaining',
        `<span class="badge ${value === 0 ? 'ok' : 'err'}">${value}</span>`,
    );
}

function normalizedVerifierOutcome(
    result: JobResultResponse | null | undefined,
): string | null {
    const value = result?.verificationOutcome?.trim().toLowerCase();
    return value || null;
}

function normalizedAssumptionReports(
    result: JobResultResponse,
): AssumptionReport[] | null {
    const reports = result.assumptionReports;
    if (!Array.isArray(reports)) return null;
    for (const report of reports) {
        if (!report || typeof report.target !== 'string' || !report.target ||
            typeof report.kernelOutput !== 'string' || !report.kernelOutput ||
            typeof report.policyPassed !== 'boolean' ||
            !Array.isArray(report.assumptions)) {
            return null;
        }
        for (const assumption of report.assumptions) {
            if (!assumption || typeof assumption.name !== 'string' || !assumption.name ||
                typeof assumption.type !== 'string' || !assumption.type ||
                (assumption.kind !== 'axiom' && assumption.kind !== 'sectionVariable')) {
                return null;
            }
        }
    }
    return reports;
}

function hasCompleteAssumptionEvidence(
    result: JobResultResponse,
    expectedTargets: string[],
): boolean {
    const reports = normalizedAssumptionReports(result);
    if (!result.assumptionPolicyId?.trim() || !reports) return false;
    const names = new Set(reports.map((report) => report.target));
    const expected = new Set(expectedTargets);
    return reports.length === result.holesClosed
        && names.size === reports.length
        && expected.size === expectedTargets.length
        && expected.size === result.holesClosed
        && reports.every((report) => expected.has(report.target))
        && reports.every((report) => report.policyPassed);
}

function verifierAcceptedFull(
    result: JobResultResponse,
    expectedTargets: string[],
): boolean {
    return (
        normalizedVerifierOutcome(result) === 'succeeded' &&
        result.compileOk === true &&
        result.assumptionsClosed === true &&
        result.verifiedClean === true &&
        result.admitsDetected === false &&
        result.statementsImmutable === true &&
        result.markersRemaining === 0 &&
        hasCompleteAssumptionEvidence(result, expectedTargets)
    );
}

function verifierAcceptedPartial(
    result: JobResultResponse,
    expectedTargets: string[],
): boolean {
    // Partial verification intentionally leaves the unfilled obligations
    // admitted, so verifiedClean=false/admitsDetected=true are expected here.
    return (
        normalizedVerifierOutcome(result) === 'partial' &&
        result.compileOk === true &&
        result.assumptionsClosed === true &&
        result.verifiedClean === false &&
        result.admitsDetected === true &&
        result.statementsImmutable === true &&
        result.markersRemaining === 0 &&
        hasCompleteAssumptionEvidence(result, expectedTargets)
    );
}

function assumptionEvidenceHtml(result: JobResultResponse): string {
    const reports = normalizedAssumptionReports(result);
    if (!reports || !result.assumptionPolicyId?.trim()) {
        return '<h2>Kernel-reported assumptions</h2><p class="dim">Per-theorem Print Assumptions evidence is unavailable.</p>';
    }
    const renderedReports = reports.map((report) => {
        const rows = report.assumptions.length
            ? report.assumptions.map((assumption) => `<tr>
        <td>${escapeHtml(assumption.kind)}</td>
        <td><code>${escapeHtml(assumption.name)}</code></td>
        <td><code>${escapeHtml(assumption.type)}</code></td>
    </tr>`).join('')
            : '<tr><td colspan="3">Closed under the global context; no assumptions reported.</td></tr>';
        return `<h3><code>${escapeHtml(report.target)}</code> <span class="badge ${report.policyPassed ? 'ok' : 'err'}">${report.policyPassed ? 'policy passed' : 'policy rejected'}</span></h3>
    <table><thead><tr><th>Kind</th><th>Dependency</th><th>Kernel type</th></tr></thead><tbody>${rows}</tbody></table>
    <details><summary>Normalized Print Assumptions output</summary><pre class="goal">${escapeHtml(report.kernelOutput)}</pre></details>`;
    }).join('');
    return `<h2>Kernel-reported assumptions</h2>
    <p>Reviewed policy: <code>${escapeHtml(result.assumptionPolicyId)}</code></p>${renderedReports}`;
}

function artifactRow(a: JobResultResponse['artifacts'][number]): string {
    const open = isTextArtifactKind(a.kind)
        ? `<button class="open-artifact" data-artifact="${escapeHtml(a.id)}">Open</button> `
        : '';
    return `<tr>
        <td><code>${escapeHtml(a.filename)}</code></td>
        <td>${escapeHtml(a.kind)}</td>
        <td class="num">${fmtBytes(a.sizeBytes)}</td>
        <td>${open}<button class="download" data-artifact="${escapeHtml(a.id)}">Download</button></td>
    </tr>`;
}

/**
 * Result actions: open the proof or refutation, compare it with the input,
 * and open the exportable certificate in the portal (the claim rules have one
 * implementation, the portal's certificate builder).
 */
function resultActions(result: JobResultResponse, terminal: boolean): string {
    const kinds = new Set(result.artifacts.map((a) => a.kind));
    const buttons: string[] = [];
    const completed = result.artifacts.find((a) => a.kind === 'CompletedV');
    if (completed) {
        buttons.push(`<button class="open-artifact" data-artifact="${escapeHtml(completed.id)}">Open completed proof</button>`);
        if (kinds.has('InputV')) {
            buttons.push('<button id="compare">Compare with input</button>');
        }
    }
    const refutation = result.artifacts.find((a) => a.kind === 'RefutationV');
    if (refutation) {
        buttons.push(`<button class="open-artifact" data-artifact="${escapeHtml(refutation.id)}">Open refutation</button>`);
    }
    if (terminal) {
        buttons.push('<button id="certificate">Open certificate in portal</button>');
    }
    return buttons.length > 0 ? `<div class="toolbar">${buttons.join('')}</div>` : '';
}

/** Live-update mode the hosting view is currently in. */
export type LiveMode = 'sse' | 'poll' | 'terminal';

function liveNote(mode: LiveMode): string {
    switch (mode) {
        case 'sse':
            return 'Live — streaming events from the server.';
        case 'poll':
            return 'Polling every 5 s (live stream unavailable).';
        case 'terminal':
            return 'Job is terminal — auto-refresh stopped.';
    }
}

export interface RenderOptions {
    /** Name of the source `.inf`, when this extension compiled the upload. */
    source?: string | null;
    /** CSP nonce for the inline script. */
    nonce: string;
    /** `webview.cspSource` of the hosting panel. */
    cspSource: string;
    /** Fetch-failure banner (stale data below stays visible), or null. */
    error?: string | null;
    /** Current update mode, shown next to the Refresh button. */
    live: LiveMode;
    /** Pre-formatted event log lines (oldest first); escaped at render. */
    eventLog: readonly string[];
    /** Show the Cancel button (status must be server-cancelable). */
    canCancel: boolean;
    /** Terminal result metadata and artifacts, once fetched. */
    result?: JobResultResponse | null;
}

/** Render the full webview HTML document for one job. */
export function renderJobDetailHtml(job: JobResponse, opts: RenderOptions): string {
    const status = job.status ?? 'Unknown';
    const total = job.holesTotal ?? 0;
    const closed = job.holesClosed ?? 0;
    const failed = job.holesFailed ?? 0;
    const pct = total > 0 ? Math.round((closed / total) * 100) : 0;
    const obligations = job.obligations ?? [];
    const provedTargets = obligations
        .filter((obligation) => obligation.status === 'proved')
        .map((obligation) => obligation.name);
    const jobHasCompleteProofs =
        job.status === 'Succeeded' &&
        job.mode === 'prove' &&
        typeof job.holesTotal === 'number' &&
        job.holesTotal > 0 &&
        typeof job.holesClosed === 'number' &&
        job.holesClosed === job.holesTotal;
    const jobClaimsProof = isProvedSuccess(
        job.status,
        job.mode,
        job.holesTotal,
        job.holesClosed,
        job.claimClass,
    );
    const jobClaimsPartialProof =
        job.status === 'PartialSuccess' &&
        job.mode === 'prove' &&
        total > 0 &&
        closed > 0 &&
        closed < total;
    const verifierOutcome = normalizedVerifierOutcome(opts.result);
    const hasVerifierVerdict = verifierOutcome !== null;
    const fullVerifierAccepted = Boolean(
        opts.result && verifierAcceptedFull(opts.result, provedTargets),
    );
    const partialVerifierAccepted = Boolean(
        opts.result && verifierAcceptedPartial(opts.result, provedTargets),
    );
    const resultMatchesJob = Boolean(
        opts.result &&
            opts.result.id === job.id &&
            opts.result.status === job.status &&
            opts.result.mode === job.mode &&
            opts.result.holesTotal === job.holesTotal &&
            opts.result.holesClosed === job.holesClosed &&
            opts.result.claimClass === job.claimClass,
    );
    const resultAgreesWithProof = Boolean(
        opts.result &&
        resultMatchesJob &&
            isProvedSuccess(
                opts.result.status,
                opts.result.mode,
                opts.result.holesTotal,
                opts.result.holesClosed,
                opts.result.claimClass,
            ) &&
            hasVerifierVerdict &&
            fullVerifierAccepted,
    );
    const provedSuccess = jobClaimsProof && resultAgreesWithProof;

    const ribbon = `<div class="ribbon">${phaseRibbon(status, provedSuccess)
        .map((chip) => `<span class="phase ${chip.state}">${escapeHtml(chip.name)}</span>`)
        .join('<span class="phase-sep">›</span>')}</div>`;

    const budget = typeof job.maxWallClockSeconds === 'number' && job.maxWallClockSeconds > 0
        ? job.maxWallClockSeconds
        : null;
    const created = job.createdAt ? new Date(job.createdAt) : null;
    const deadline = budget !== null && created && !Number.isNaN(created.getTime())
        ? new Date(created.getTime() + budget * 1000).toISOString()
        : null;

    const errorBanner = opts.error
        ? `<div class="banner err-banner">${escapeHtml(opts.error)}</div>`
        : '';

    const verdictBlock =
        job.errorCode || job.errorReason
            ? `<div class="banner err-banner"><strong>${escapeHtml(job.errorCode ?? 'ERROR')}</strong>${
                  job.errorReason ? `: ${escapeHtml(job.errorReason)}` : ''
              }</div>`
            : '';

    const compileGoalsNotice =
        job.status === 'CompileGoals'
            ? '<div class="banner warn-banner"><strong>Compile-only result.</strong> The file compiled and goals were extracted; no proof was accepted or returned.</div>'
            : job.mode === 'compile-goals' && job.status !== 'Succeeded'
              ? '<div class="banner warn-banner"><strong>Compile-only run.</strong> No proof was accepted or returned.</div>'
              : '';

    const structuralOnlyNotice =
        job.status === 'Succeeded' && job.claimClass === 'StructuralOnly'
            ? '<div class="banner warn-banner"><strong>Structural validity only.</strong> The obligations were closed, but none establishes a behavioral property of the compiled module. This is not a functional-correctness claim.</div>'
            : '';

    const succeededTrustNotice =
        job.status !== 'Succeeded'
            ? ''
            : job.mode === 'compile-goals'
              ? '<div class="banner warn-banner"><strong>Not proven.</strong> This record says Succeeded, but compile-goals mode returned no accepted proof.</div>'
              : job.mode !== 'prove'
                ? `<div class="banner warn-banner"><strong>Not proven.</strong> This record says Succeeded, but its persisted mode is <code>${escapeHtml(job.mode ?? 'missing')}</code>. Only Succeeded with persisted mode <code>prove</code> is treated as proof success.</div>`
                : !jobHasCompleteProofs
                  ? '<div class="banner warn-banner"><strong>Not proven.</strong> This record says Succeeded in prove mode, but it does not record at least one proof hole with every hole closed.</div>'
                  : opts.result && !resultMatchesJob
                    ? `<div class="banner warn-banner"><strong>Not proven.</strong> Job detail and result DTOs do not agree on a Succeeded <code>prove</code>-mode run, so this view does not claim proof success.</div>`
                    : hasVerifierVerdict && !fullVerifierAccepted
                      ? '<div class="banner warn-banner"><strong>Not proven.</strong> The independent verifier evidence rejects this Succeeded result.</div>'
                      : job.claimClass === 'StructuralOnly'
                        ? ''
                        : job.claimClass !== 'Verified'
                          ? `<div class="banner warn-banner"><strong>Behavioral claim unverified.</strong> The server ${job.claimClass ? `classified this result as <code>${escapeHtml(job.claimClass)}</code>` : 'did not provide a claim class'}, so closed obligations are not shown as behavioral proof.</div>`
                          : !opts.result
                            ? '<div class="banner warn-banner"><strong>Certificate evidence unavailable.</strong> The terminal result and its trusted-verifier evidence have not been loaded, so this view does not claim proof success.</div>'
                          : !hasVerifierVerdict
                            ? '<div class="banner warn-banner"><strong>Worker-reported result.</strong> This legacy result has no independent verifier provenance.</div>'
                            : '';

    const partialTrustNotice =
        job.status !== 'PartialSuccess'
            ? ''
            : !jobClaimsPartialProof
              ? '<div class="banner warn-banner"><strong>Not proven.</strong> A partial proof requires at least one, but not all, recorded holes to be closed in prove mode.</div>'
              : opts.result && !resultMatchesJob
                ? '<div class="banner warn-banner"><strong>Not independently verified.</strong> Job detail and result DTOs disagree about this partial run.</div>'
                : job.claimClass !== 'PartialVerified'
                  ? '<div class="banner warn-banner"><strong>Partial claim unverified.</strong> The durable claim class does not support a verified partial result.</div>'
                : !opts.result || !hasVerifierVerdict
                  ? '<div class="banner warn-banner"><strong>Worker-reported partial result.</strong> No independent verifier provenance is available.</div>'
                  : partialVerifierAccepted
                    ? '<div class="banner warn-banner"><strong>Partial result accepted.</strong> The independent verifier confirmed the closed proofs; unfinished obligations remain admitted.</div>'
                    : '<div class="banner err-banner"><strong>Independent verification failed.</strong> The verifier evidence does not support this PartialSuccess result.</div>';

    const proofTrustNotice =
        structuralOnlyNotice + succeededTrustNotice + partialTrustNotice;

    const obligationsTable =
        obligations.length > 0
            ? `<table>
        <thead><tr><th>Obligation</th><th>Kind</th><th>Content</th><th>Status</th><th>Attempts</th><th>Duration</th><th>Last error</th></tr></thead>
        <tbody>${obligations.map(obligationRow).join('\n')}</tbody>
    </table>`
            : '<p class="dim">No obligations reported yet.</p>';

    const eventLogHtml =
        opts.eventLog.length > 0
            ? opts.eventLog
                  .map((line) => `<div>${escapeHtml(line)}</div>`)
                  .join('')
            : '<div class="dim">No events yet.</div>';

    const verificationSection = opts.result
        ? hasVerifierVerdict
            ? `<h2>Independent verifier verdict</h2>
    <div class="meta">
        ${metaRow('Outcome', `<span class="badge ${job.status === 'PartialSuccess' && partialVerifierAccepted ? 'warn' : fullVerifierAccepted ? 'ok' : 'err'}">${escapeHtml(verifierOutcome!)}</span>`)}
        ${metaRow('Result claim class', `<span class="badge ${provedSuccess ? 'ok' : 'warn'}">${escapeHtml(opts.result.claimClass ?? 'missing')}</span>`)}
        ${metaRow('Verified at', fmtTimestamp(opts.result.verifiedAt))}
        ${verdictFlag('Compilation passed', opts.result.compileOk, true)}
        ${verdictFlag('Assumption policy passed', opts.result.assumptionsClosed, true)}
        ${verdictFlag('Verified clean (all holes closed, no admits)', opts.result.verifiedClean, true, partialVerifierAccepted ? 'warn' : 'err')}
        ${verdictFlag('Admits detected', opts.result.admitsDetected, false, partialVerifierAccepted ? 'warn' : 'err')}
        ${verdictFlag('Statements immutable', opts.result.statementsImmutable, true)}
        ${markersFlag(opts.result.markersRemaining)}
    </div>`
            : '<h2>Verification</h2><p class="dim">No independent verifier provenance is available; this is a worker-reported legacy result.</p>'
        : '';

    const resultSection = opts.result
        ? `${resultActions(opts.result, isTerminalStatus(status))}
    ${verificationSection}
    ${hasVerifierVerdict ? assumptionEvidenceHtml(opts.result) : ''}
    <h2>Artifacts</h2>
    ${
        opts.result.artifacts.length > 0
            ? `<table>
        <thead><tr><th>File</th><th>Kind</th><th>Size</th><th></th></tr></thead>
        <tbody>${opts.result.artifacts.map(artifactRow).join('\n')}</tbody>
    </table>`
            : '<p class="dim">No artifacts stored.</p>'
    }`
        : '';

    const cancelButton = opts.canCancel
        ? '<button id="cancel" class="danger">Cancel Job</button>'
        : '';

    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src 'unsafe-inline' ${opts.cspSource}; script-src 'nonce-${opts.nonce}';">
<style>
    body {
        font-family: var(--vscode-font-family);
        color: var(--vscode-foreground);
        padding: 0 1.2em 2em;
        max-width: 60em;
    }
    h1 { font-size: 1.3em; font-weight: 600; display: flex; align-items: center; gap: 0.6em; }
    code { font-family: var(--vscode-editor-font-family); }
    .badge {
        display: inline-block; padding: 0.1em 0.65em; border-radius: 1em;
        font-size: 0.82em; font-weight: 600; color: var(--vscode-badge-foreground);
        background: var(--vscode-badge-background);
    }
    .badge.ok   { background: var(--vscode-testing-iconPassed); color: #fff; }
    .badge.err  { background: var(--vscode-errorForeground);    color: #fff; }
    .badge.warn { background: var(--vscode-editorWarning-foreground, #c90); color: #1e1e1e; }
    .badge.active { background: var(--vscode-progressBar-background, #0a7acc); color: #fff; }
    .ribbon { display: flex; flex-wrap: wrap; align-items: center; gap: 0.3em; margin: 0.4em 0 1em; }
    .phase { padding: 0.1em 0.6em; border-radius: 3px; font-size: 0.82em;
             border: 1px solid var(--vscode-panel-border, #444); color: var(--vscode-descriptionForeground); }
    .phase.done { color: var(--vscode-foreground); }
    .phase.active { background: var(--vscode-progressBar-background, #0a7acc); color: #fff; border-color: transparent; }
    .phase.ok { background: var(--vscode-testing-iconPassed); color: #fff; border-color: transparent; }
    .phase.warn { background: var(--vscode-editorWarning-foreground, #c90); color: #1e1e1e; border-color: transparent; }
    .phase.err { background: var(--vscode-errorForeground); color: #fff; border-color: transparent; }
    .phase.past, .phase.todo { opacity: 0.6; }
    .phase-sep { color: var(--vscode-descriptionForeground); }
    .meta {
        display: grid; grid-template-columns: max-content 1fr; gap: 0.35em 1.2em;
        margin: 1em 0;
    }
    .meta-label { color: var(--vscode-descriptionForeground); }
    .progress-track {
        background: var(--vscode-input-background, rgba(127, 127, 127, 0.2));
        border: 1px solid var(--vscode-panel-border, #444);
        border-radius: 4px; height: 8px; position: relative;
        margin-top: 0.4em; overflow: hidden;
    }
    .progress-fill {
        position: absolute; top: 0; bottom: 0; left: 0;
        background: var(--vscode-testing-iconPassed, #3c3);
    }
    table { border-collapse: collapse; width: 100%; margin-top: 0.6em; }
    th, td { text-align: left; padding: 0.35em 0.8em 0.35em 0; vertical-align: top; }
    th { color: var(--vscode-descriptionForeground); font-weight: 600;
         border-bottom: 1px solid var(--vscode-panel-border, #444); }
    td.num { text-align: right; }
    td.error-cell { color: var(--vscode-errorForeground); font-size: 0.9em; max-width: 24em; }
    .goal-row td { padding-top: 0; padding-bottom: 0.6em; }
    .goal-label { color: var(--vscode-descriptionForeground); font-size: 0.8em; text-transform: uppercase; letter-spacing: 0.04em; }
    pre.goal {
        margin: 0.2em 0 0; padding: 0.5em 0.8em;
        font-family: var(--vscode-editor-font-family);
        font-size: 0.9em; line-height: 1.45;
        background: var(--vscode-textCodeBlock-background, rgba(127,127,127,0.1));
        border-left: 3px solid var(--vscode-panel-border, #444);
        border-radius: 3px; white-space: pre-wrap; overflow-wrap: anywhere;
    }
    .banner { padding: 0.5em 0.8em; border-radius: 4px; margin: 0.8em 0; }
    .err-banner {
        background: var(--vscode-inputValidation-errorBackground, rgba(200,0,0,0.15));
        border: 1px solid var(--vscode-inputValidation-errorBorder, var(--vscode-errorForeground));
    }
    .warn-banner {
        background: var(--vscode-inputValidation-warningBackground, rgba(204,153,0,0.12));
        border: 1px solid var(--vscode-inputValidation-warningBorder, var(--vscode-editorWarning-foreground, #c90));
    }
    .dim { color: var(--vscode-descriptionForeground); }
    .toolbar { display: flex; align-items: center; gap: 1em; margin-top: 1.4em; flex-wrap: wrap; }
    button {
        background: var(--vscode-button-background); color: var(--vscode-button-foreground);
        border: none; padding: 0.35em 1em; border-radius: 3px; cursor: pointer;
    }
    button:hover { background: var(--vscode-button-hoverBackground); }
    button.danger {
        background: var(--vscode-inputValidation-errorBackground, #5a1d1d);
        color: var(--vscode-foreground);
        border: 1px solid var(--vscode-inputValidation-errorBorder, var(--vscode-errorForeground));
    }
    button.download { font-size: 0.85em; padding: 0.2em 0.8em; }
    .event-log {
        font-family: var(--vscode-editor-font-family);
        font-size: 0.85em; line-height: 1.5;
        background: var(--vscode-textCodeBlock-background, rgba(127,127,127,0.1));
        border-radius: 4px; padding: 0.6em 0.9em;
        max-height: 20em; overflow-y: auto; white-space: pre-wrap;
        overflow-wrap: anywhere;
    }
    h2 { font-size: 1em; font-weight: 600; margin-top: 1.6em; }
</style>
</head>
<body>
    ${errorBanner}
    <h1>${escapeHtml(job.filename ?? job.id)} <span class="badge ${statusClass(status, provedSuccess)}">${escapeHtml(status)}</span></h1>
    ${ribbon}

    <div class="meta">
        ${metaRow('Job ID', `<code>${escapeHtml(job.id)}</code>`)}
        ${opts.source ? metaRow('Source', `<code>${escapeHtml(opts.source)}</code>`) : ''}
        ${metaRow('Submitted', fmtTimestamp(job.createdAt))}
        ${budget !== null ? metaRow('Time budget', escapeHtml(fmtBudget(budget))) : ''}
        ${deadline ? metaRow('Deadline', fmtTimestamp(deadline)) : ''}
        ${metaRow('Mode', escapeHtml(job.mode ?? '—'))}
        ${metaRow('Outcome', escapeHtml(job.outcome ?? '—'))}
        ${metaRow('Claim class', `<span class="badge ${provedSuccess ? 'ok' : 'warn'}">${escapeHtml(job.claimClass ?? 'missing')}</span>`)}
    </div>

    ${compileGoalsNotice}
    ${proofTrustNotice}

    <h2>Proof holes: ${closed}/${total} closed${failed > 0 ? `, ${failed} failed` : ''}</h2>
    <div class="progress-track"><div class="progress-fill" style="width: ${pct}%"></div></div>

    ${verdictBlock}

    <h2>Obligations</h2>
    ${obligationsTable}

    ${resultSection}

    <h2>Events</h2>
    <div id="event-log" class="event-log">${eventLogHtml}</div>

    <div class="toolbar">
        <button id="refresh">Refresh</button>
        ${cancelButton}
        <span id="live-note" class="dim">${escapeHtml(liveNote(opts.live))}</span>
        <span id="checked" class="dim"></span>
    </div>

    <script nonce="${opts.nonce}">
        const vscode = acquireVsCodeApi();
        const MAX_LOG_NODES = ${EVENT_LOG_CAP};
        const NOTES = {
            sse: 'Live — streaming events from the server.',
            poll: 'Polling every 5 s (live stream unavailable).',
            terminal: 'Job is terminal — auto-refresh stopped.',
        };

        document.getElementById('refresh').addEventListener('click', () => {
            vscode.postMessage({ command: 'refresh' });
        });
        const cancelBtn = document.getElementById('cancel');
        if (cancelBtn) {
            cancelBtn.addEventListener('click', () => {
                vscode.postMessage({ command: 'cancel' });
            });
        }
        for (const btn of document.querySelectorAll('button.download')) {
            btn.addEventListener('click', () => {
                vscode.postMessage({
                    command: 'downloadArtifact',
                    artifactId: btn.getAttribute('data-artifact'),
                });
            });
        }
        for (const btn of document.querySelectorAll('button.open-artifact')) {
            btn.addEventListener('click', () => {
                vscode.postMessage({
                    command: 'openArtifact',
                    artifactId: btn.getAttribute('data-artifact'),
                });
            });
        }
        for (const [id, command] of [['compare', 'compare'], ['certificate', 'openCertificate']]) {
            const btn = document.getElementById(id);
            if (btn) {
                btn.addEventListener('click', () => vscode.postMessage({ command }));
            }
        }

        const logEl = document.getElementById('event-log');
        logEl.scrollTop = logEl.scrollHeight;

        window.addEventListener('message', (event) => {
            const message = event.data;
            if (!message) {
                return;
            }
            if (message.command === 'checked') {
                document.getElementById('checked').textContent =
                    'Last checked ' + message.at + ' — no changes.';
            } else if (message.command === 'mode') {
                const note = NOTES[message.mode];
                if (note) {
                    document.getElementById('live-note').textContent = note;
                }
            } else if (message.command === 'event') {
                // Keep autoscroll only when the user is already at the bottom.
                const stick = logEl.scrollTop + logEl.clientHeight
                    >= logEl.scrollHeight - 8;
                const placeholder = logEl.querySelector('.dim');
                if (placeholder) {
                    placeholder.remove();
                }
                const div = document.createElement('div');
                div.textContent = message.line; // textContent: no HTML parsing
                logEl.appendChild(div);
                while (logEl.childElementCount > MAX_LOG_NODES) {
                    logEl.removeChild(logEl.firstElementChild);
                }
                if (stick) {
                    logEl.scrollTop = logEl.scrollHeight;
                }
            }
        });
    </script>
</body>
</html>`;
}
