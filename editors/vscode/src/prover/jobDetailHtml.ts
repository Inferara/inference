/**
 * Pure HTML renderer for the proof-job panel.
 *
 * Free of any `vscode` import so it is unit-tested under plain node. Every
 * server-originated string is HTML-escaped here; the webview script never
 * interpolates data into markup (activity rows use textContent).
 *
 * The page is a fixed shell (`renderShell`) with named regions the host
 * patches in place (`renderRegions`), so updates keep focus, open sections
 * and scroll positions. `renderJobDetailHtml` returns shell + regions in one
 * document for tests and the visual harness.
 */

import { jobDetailScript } from './jobDetailScript';
import {
    buildJobView,
    type ActionView,
    type CheckView,
    type EvidenceView,
    type JobView,
    type JobViewInput,
    type LiveMode,
    type ObligationView,
    type StepView,
} from './jobDetailModel';
import { JOB_DETAIL_STYLES } from './jobDetailStyles';
import type { Tone, VerdictIcon } from './jobVerdict';
import { EVENT_LOG_CAP, type ActivityRow } from './runModel';
import type { JobResponse } from './types';
import { icon, type IconName } from './webviewIcons';

export { isCancelableStatus, isStreamTerminalStatus, isTerminalStatus, isTextArtifactKind } from './jobStatus';
export { EVENT_LOG_CAP } from './runModel';
export type { LiveMode } from './jobDetailModel';

export function escapeHtml(value: string): string {
    return value
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

const esc = escapeHtml;

export const REGION_IDS = [
    'banner',
    'header',
    'steps',
    'verdict',
    'progress',
    'obligations',
    'evidence',
    'files',
    'tech',
] as const;

export type RegionId = (typeof REGION_IDS)[number];
export type Regions = Record<RegionId, string>;

const VERDICT_ICONS: Record<VerdictIcon, IconName> = {
    pass: 'pass',
    warning: 'warning',
    error: 'error',
    refuted: 'refuted',
    running: 'running',
    waiting: 'waiting',
    verifying: 'verifying',
    canceled: 'canceled',
    loading: 'loading',
};

function verdictIcon(name: VerdictIcon): string {
    return icon(VERDICT_ICONS[name], name === 'running' || name === 'loading' ? 'spin' : '');
}

function toneIcon(tone: Tone): IconName {
    switch (tone) {
        case 'ok':
            return 'pass';
        case 'err':
            return 'error';
        case 'refuted':
            return 'refuted';
        case 'warn':
            return 'warning';
        case 'active':
            return 'running';
        default:
            return 'dot';
    }
}

function dataAttrs(data: Record<string, string> | undefined): string {
    return Object.entries(data ?? {})
        .map(([k, v]) => ` data-${k.replace(/[^a-z0-9-]/g, '')}="${esc(v)}"`)
        .join('');
}

function button(a: ActionView, extraClass = ''): string {
    const cls = [a.style ?? 'secondary', extraClass].filter(Boolean).join(' ');
    const focusKey = `${a.action}:${a.data?.artifact ?? ''}:${a.data?.line ?? ''}:${a.data?.obligation ?? ''}`;
    return `<button type="button" class="${cls}" data-action="${esc(a.action)}"${dataAttrs(a.data)} data-focus-key="${esc(focusKey)}"${
        a.ariaLabel ? ` aria-label="${esc(a.ariaLabel)}"` : ''
    }>${a.icon ? icon(a.icon) : ''}${esc(a.label)}</button>`;
}

// ── Regions ─────────────────────────────────────────────────────────────

function bannerRegion(view: JobView): string {
    if (!view.banner) {
        return '';
    }
    const tone = view.banner.tone;
    return `<div class="banner tone-${tone}" role="alert">${icon(tone === 'err' ? 'error' : 'warning')}<span>${esc(view.banner.text)}</span></div>`;
}

function liveIndicator(live: LiveMode): string {
    switch (live) {
        case 'sse':
            return `<span class="live" title="Live — streaming events from the proof server">${icon('dot')}Live</span>`;
        case 'poll':
            return `<span class="live" title="Live stream unavailable — checking every 5 seconds">${icon('dot')}Polling</span>`;
        default:
            return '';
    }
}

function headerRegion(view: JobView): string {
    const v = view.verdict;
    return `<div class="header">
    <div>
        <div class="title tone-${v.tone}">${verdictIcon(v.icon)}<h1>${esc(view.title)}</h1><span class="badge tone-${v.tone}" data-verdict="${v.kind}" data-tone="${v.tone}">${esc(v.badge)}</span></div>
        ${view.subtitle ? `<div class="subtitle">Uploaded as <code>${esc(view.subtitle)}</code></div>` : ''}
    </div>
    <div class="header-actions">${liveIndicator(view.live)}${view.headerActions.map((a) => button(a)).join('')}</div>
</div>`;
}

const STEP_SR: Record<StepView['state'], string> = {
    done: 'done',
    active: 'in progress',
    todo: 'not started',
    unknown: 'not recorded',
    stopped: 'stopped here',
    final: 'result',
};

function stepIcon(step: StepView, verdict: JobView['verdict']): string {
    switch (step.state) {
        case 'done':
            return icon('pass');
        case 'active':
            return step.key === 'result' ? verdictIcon(verdict.icon)
                : step.key === 'worker' || step.key === 'submitted' ? icon('waiting') : icon('running', 'spin');
        case 'todo':
            return icon('todo');
        case 'unknown':
            return icon('dot');
        case 'stopped':
            return icon(step.tone === 'muted' ? 'canceled' : toneIcon(step.tone ?? 'err'));
        case 'final':
            return verdictIcon(verdict.icon);
    }
}

function stepsRegion(view: JobView): string {
    const items = view.steps.map((step, i) => {
        const tone = step.tone ? ` tone-${step.tone}` : '';
        const title = step.at ? ` title="Started ${esc(new Date(step.at).toLocaleTimeString())}"` : '';
        return `<li class="step ${step.state}${tone}" data-step="${step.key}" data-state="${step.state}"${title}>${
            i > 0 ? `<span class="sep" aria-hidden="true">${icon('chevron')}</span>` : ''
        }${stepIcon(step, view.verdict)}<span>${esc(step.label)}</span><span class="sr-only">, ${STEP_SR[step.state]}</span></li>`;
    });
    return `<ol class="steps" aria-label="Run steps">${items.join('')}</ol>`;
}

function verdictRegion(view: JobView): string {
    const v = view.verdict;
    const code = (v.kind === 'compile-error' || v.kind === 'completed-compile-error') && view.verdictBodyIsCode;
    return `<section class="verdict tone-${v.tone}" id="verdict" data-verdict="${v.kind}" data-tone="${v.tone}" data-claim="${v.claim}" role="status" aria-live="polite">
    <h2>${verdictIcon(v.icon)}<span>${esc(v.headline)}</span></h2>
    ${code ? `<pre class="excerpt">${esc(v.body)}</pre>` : `<p>${esc(v.body)}</p>`}
    ${v.caveats.length ? `<ul class="caveats">${v.caveats.map((c) => `<li>${esc(c)}</li>`).join('')}</ul>` : ''}
    ${view.actions.length ? `<div class="actions">${view.actions.map((a) => button(a)).join('')}</div>` : ''}
</section>`;
}

function clockFallback(view: JobView): string {
    const budget = view.clock.budgetSeconds;
    if (!budget) {
        return '';
    }
    const h = Math.floor(budget / 3600);
    const m = Math.floor((budget % 3600) / 60);
    return `Time budget ${h ? `${h}h${m ? ` ${m}m` : ''}` : m ? `${m}m` : `${budget}s`}`;
}

function progressRegion(view: JobView): string {
    const p = view.progress;
    const clock = `<span class="clock" data-clock aria-hidden="true">${esc(clockFallback(view))}</span>`;
    if (!p.known) {
        return `<div class="progress" aria-busy="true"><div class="progress-head"><h2>Obligations</h2>${clock}</div><div class="bar loading"></div></div>`;
    }
    const pct = (n: number) => (p.total > 0 ? Math.min(100, Math.round((n / p.total) * 1000) / 10) : 0);
    return `<div class="progress">
    <div class="progress-head"><h2>Obligations <span class="count">${p.closed}/${p.total}</span></h2>${
        p.failed > 0 ? `<span class="failed-note">${p.failed} not closed</span>` : ''
    }${clock}</div>
    <div class="bar" role="progressbar" aria-label="Obligations closed" aria-valuemin="0" aria-valuemax="${p.total}" aria-valuenow="${p.closed}" aria-valuetext="${p.closed} of ${p.total} obligations closed${
        p.failed > 0 ? `, ${p.failed} not closed` : ''
    }"><span class="closed" style="width: ${pct(p.closed)}%"></span><span class="failed" style="width: ${pct(p.failed)}%"></span></div>
</div>`;
}

function obligationDetails(o: ObligationView): string {
    const key = `obl:${o.name}`;
    const meta = [
        o.by ? `<span>${o.by === 'template' ? 'template' : `agent${o.attempts && o.attempts > 0 ? ` #${o.attempts}` : ''}`}</span>` : '',
        o.duration ? `<span>${esc(o.duration)}</span>` : '',
        `<span class="obl-status">${esc(o.statusLabel)}</span>`,
    ].join('');
    const facts = [
        `<dt>Theorem</dt><dd><code>${esc(o.name)}</code></dd>`,
        `<dt>Statement</dt><dd>${esc(o.contentLabel)}</dd>`,
        o.specList ? `<dt>Specification</dt><dd><code>${esc(o.specList)}</code></dd>` : '',
        o.attempts ? `<dt>Attempts</dt><dd>${o.attempts}</dd>` : '',
    ].join('');
    const goto = o.goto.target === 'proof'
        ? button({
              action: 'gotoTheorem', label: 'Go to proof', icon: 'goto', data: { obligation: o.name, artifact: o.goto.artifact },
              ariaLabel: `Show the proof of ${o.shortName} in the completed file`,
          }, 'link')
        : button({
              action: 'gotoTheorem', label: 'Go to theorem', icon: 'goto',
              data: { obligation: o.name, ...(o.goto.line !== undefined ? { line: String(o.goto.line) } : {}) },
              ariaLabel: `Go to ${o.shortName} in the submitted file`,
          }, 'link');
    const actions = [
        goto,
        `<button type="button" class="link" data-action="show-activity" data-obligation="${esc(o.name)}" data-focus-key="${esc(`act:${o.name}`)}" aria-label="${esc(`Show activity for ${o.shortName}`)}">${icon('search')}Show activity</button>`,
    ].join('');
    return `<details class="obl tone-${o.tone}" data-key="${esc(key)}" data-status="${esc(o.status)}" data-kind="${esc(o.kindLabel)}">
    <summary data-focus-key="${esc(key)}"><span class="obl-icon">${o.status === 'pending' || o.statusLabel === 'Not attempted' ? icon('todo') : icon(toneIcon(o.tone), o.status === 'running' ? 'spin' : '')}</span><span class="obl-main"><span class="obl-name" title="${esc(o.name)}">${esc(o.shortName)}</span><span class="chip">${esc(o.kindLabel)}</span></span><span class="obl-meta">${meta}</span><span class="chev">${icon('chevron')}</span></summary>
    <div class="obl-body">
        <dl class="facts">${facts}</dl>
        ${o.lastError ? `<div class="obl-error">${icon('error')}<span>${esc(o.lastError)}</span></div>` : ''}
        ${o.goal ? `<div class="goal-label">Goal</div><pre class="goal">${esc(o.goal)}</pre>` : ''}
        <div class="actions small">${actions}</div>
    </div>
</details>`;
}

const GROUP_TITLES: Record<ObligationView['group'], string> = {
    active: 'Working',
    problem: 'Not closed',
    waiting: 'Waiting',
    done: 'Proved',
};

function obligationsRegion(view: JobView): string {
    const list = view.obligations;
    if (list === null) {
        return `<div class="obligations" aria-busy="true"><div class="skeleton"></div><div class="skeleton"></div><div class="skeleton"></div><span class="sr-only">Loading obligations…</span></div>`;
    }
    if (list.length === 0) {
        const text = view.terminal || view.verdict.kind === 'canceling'
            ? 'No obligations were reported for this run.'
            : 'Obligations appear once the worker has compiled the file.';
        return `<p class="empty">${text}</p>`;
    }
    if (list.length <= 10) {
        return `<div class="obligations">${list.map(obligationDetails).join('')}</div>`;
    }
    const groups = (['active', 'problem', 'waiting', 'done'] as const)
        .map((g) => [g, list.filter((o) => o.group === g)] as const)
        .filter(([, items]) => items.length > 0)
        .map(([g, items]) => `<details class="group" open data-key="grp:${g}"><summary data-focus-key="grp:${g}"><span class="chev">${icon('chevron')}</span>${esc(GROUP_TITLES[g])} (${items.length})</summary>${items.map(obligationDetails).join('')}</details>`);
    return `<div class="obligations">${groups.join('')}</div>`;
}

const CHECK_ICONS: Record<CheckView['state'], IconName> = { pass: 'pass', fail: 'error', warn: 'warning', unknown: 'dot' };
const CHECK_SR: Record<CheckView['state'], string> = { pass: 'passed', fail: 'failed', warn: 'partial', unknown: 'not reported' };

function evidenceBody(e: EvidenceView): string {
    if (e.kind === 'worker-only') {
        return '<p>Not independently verified — this result was checked only inside the worker.</p>';
    }
    if (e.kind === 'not-applicable') {
        return '<p>Not applicable — this run only compiled the file.</p>';
    }
    const checks = e.checks
        .map((c) => `<li class="${c.state}">${icon(CHECK_ICONS[c.state])}<span>${esc(c.label)}${c.detail ? ` — ${esc(c.detail)}` : ''}<span class="sr-only">: ${CHECK_SR[c.state]}</span></span></li>`)
        .join('');
    const reports = e.reports
        ? e.reports
              .map((r) => {
                  const rows = r.assumptions.length
                      ? r.assumptions
                            .map((a) => `<tr><td>${a.kind === 'axiom' ? 'axiom' : 'section variable'}</td><td><code>${esc(a.name)}</code></td><td><code>${esc(a.type)}</code></td></tr>`)
                            .join('')
                      : '<tr><td colspan="3">Closed under the global context; no assumptions.</td></tr>';
                  return `<details class="assume" data-key="${esc(`asm:${r.target}`)}"><summary data-focus-key="${esc(`asm:${r.target}`)}"><span class="chev">${icon('chevron')}</span>${icon(r.policyPassed ? 'pass' : 'error')}<code>${esc(r.target)}</code><span class="muted">— ${r.policyPassed ? 'policy passed' : 'policy rejected'} · ${r.assumptions.length} ${r.assumptions.length === 1 ? 'assumption' : 'assumptions'}</span></summary>
<table><thead><tr><th scope="col">Kind</th><th scope="col">Dependency</th><th scope="col">Kernel type</th></tr></thead><tbody>${rows}</tbody></table>
<details data-key="${esc(`kout:${r.target}`)}"><summary data-focus-key="${esc(`kout:${r.target}`)}"><span class="chev">${icon('chevron')}</span>Normalized Print Assumptions output</summary><pre class="goal">${esc(r.kernelOutput)}</pre></details></details>`;
              })
              .join('')
        : '<p class="muted">Per-theorem Print Assumptions evidence is unavailable.</p>';
    return `<p class="muted">Independent verifier: <strong>${esc(e.outcome ?? '')}</strong>${e.verifiedAt ? ` · ${esc(e.verifiedAt)}` : ''}</p>
<ul class="checks">${checks}</ul>
<h3>Kernel-reported assumptions</h3>
${e.policyId ? `<p class="muted">Reviewed policy <code>${esc(e.policyId)}</code></p>` : ''}
${reports}`;
}

function section(key: string, title: string, body: string, open = false): string {
    return `<details class="section" data-key="${key}"${open ? ' open' : ''}><summary data-focus-key="${key}"><span class="chev">${icon('chevron')}</span><h2>${title}</h2></summary><div class="section-body">${body}</div></details>`;
}

function evidenceRegion(view: JobView): string {
    if (!view.evidence) {
        return '';
    }
    return section('sec:evidence', 'How this was verified', evidenceBody(view.evidence), view.verdict.kind === 'rejected');
}

function filesRegion(view: JobView): string {
    if (!view.files) {
        return '';
    }
    const body = view.files.length
        ? `<ul class="files">${view.files
              .map((f) => `<li>${icon('file')}<span class="file-name"><code>${esc(f.filename)}</code><span class="muted">${esc(f.kindLabel)} · ${esc(f.size)}</span></span><span class="file-actions">${
                  f.text
                      ? button({ action: 'openArtifact', label: 'Open', data: { artifact: f.id }, ariaLabel: `Open ${f.filename}` }, 'link')
                      : ''
              }${button({ action: 'downloadArtifact', label: 'Download', icon: 'download', data: { artifact: f.id }, ariaLabel: `Download ${f.filename}` }, 'link')}</span></li>`)
              .join('')}</ul>`
        : '<p class="muted">No files were stored for this run.</p>';
    return section('sec:files', `Files <span class="muted">(${view.files.length})</span>`, body);
}

function techRegion(view: JobView): string {
    const rows = view.tech
        .map((r) => `<dt>${esc(r.label)}</dt><dd data-field="${esc(r.field)}">${r.mono ? `<code>${esc(r.value)}</code>` : esc(r.value)}</dd>`)
        .join('');
    return section('sec:tech', 'Technical details', `<dl class="tech">${rows}</dl><div class="actions small">${button({ action: 'copyJobId', label: 'Copy job ID', icon: 'copy' }, 'link')}</div>`);
}

/** Every region of the panel for one view, as escaped HTML. */
export function renderRegions(view: JobView): Regions {
    return {
        banner: bannerRegion(view),
        header: headerRegion(view),
        steps: stepsRegion(view),
        verdict: verdictRegion(view),
        progress: progressRegion(view),
        obligations: obligationsRegion(view),
        evidence: evidenceRegion(view),
        files: filesRegion(view),
        tech: techRegion(view),
    };
}

/** Activity rows as markup identical to what the webview script builds. */
export function renderActivityRows(rows: readonly ActivityRow[]): string {
    if (rows.length === 0) {
        return '<div class="empty">No activity yet.</div>';
    }
    let last: string | null = null;
    const out: string[] = [];
    for (const r of rows) {
        const key = r.obligation ?? '';
        if (key !== last && (key || last)) {
            const short = key ? key.slice(key.lastIndexOf('__') > 0 ? key.lastIndexOf('__') + 2 : 0) || key : 'Run';
            out.push(`<div class="grp"${key ? ` title="${esc(key)}"` : ''}>${esc(short)}</div>`);
        }
        last = key;
        const d = new Date(r.ts);
        const p = (n: number) => String(n).padStart(2, '0');
        const clock = Number.isNaN(d.getTime()) ? '' : `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
        out.push(`<div class="row" data-cat="${r.cat}" data-seq="${r.seq}"${r.obligation ? ` data-obligation="${esc(r.obligation)}"` : ''}><time datetime="${esc(r.ts)}">${clock}</time><span class="text">${
            r.attempt ? `<span class="att">#${r.attempt}</span>` : ''
        }${esc(r.text)}</span></div>`);
    }
    return out.join('');
}

export interface ShellOptions {
    nonce: string;
    cspSource: string;
    /** Regions to render inline (tests, harness); empty placeholders otherwise. */
    regions?: Partial<Regions>;
    rows?: readonly ActivityRow[];
}

/** The page: regions, the activity section and the script. */
export function renderShell(opts: ShellOptions): string {
    const region = (id: RegionId, fallback = '') => `<div id="region-${id}">${opts.regions?.[id] ?? fallback}</div>`;
    const loading = '<div class="skeleton"></div>';
    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src 'unsafe-inline' ${opts.cspSource}; script-src 'nonce-${opts.nonce}';">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>${JOB_DETAIL_STYLES}</style>
</head>
<body>
<main>
    ${region('banner')}
    ${region('header', loading)}
    ${region('steps')}
    ${region('verdict', loading)}
    ${region('progress')}
    ${region('obligations')}
    ${region('evidence')}
    ${region('files')}
    ${region('tech')}
    <section class="activity" id="activity" aria-labelledby="activity-title">
        <div class="activity-head">
            <h2 id="activity-title">Activity</h2>
            <div class="activity-tools">
                <div class="segmented" role="radiogroup" aria-label="Show activity">
                    <button type="button" role="radio" aria-checked="true" data-action="filter" data-value="steps" data-focus-key="filter:steps">Steps</button>
                    <button type="button" role="radio" aria-checked="false" tabindex="-1" data-action="filter" data-value="agent" data-focus-key="filter:agent">Agent</button>
                    <button type="button" role="radio" aria-checked="false" tabindex="-1" data-action="filter" data-value="all" data-focus-key="filter:all">All</button>
                </div>
                <label class="search">${icon('search')}<input id="activity-search" type="search" placeholder="Filter" aria-label="Filter activity" spellcheck="false"></label>
            </div>
        </div>
        <div id="log" class="log" data-filter="steps" role="log" aria-label="Activity" tabindex="0">${renderActivityRows(opts.rows ?? [])}</div>
    </section>
    <div class="footer"><span id="checked" aria-live="polite"></span></div>
</main>
<script nonce="${opts.nonce}">${jobDetailScript(EVENT_LOG_CAP)}</script>
</body>
</html>`;
}

export interface RenderOptions extends Omit<JobViewInput, 'job'> {
    /** CSP nonce for the inline script. */
    nonce: string;
    /** `webview.cspSource` of the hosting panel. */
    cspSource: string;
}

/** One complete document (shell + regions + activity) for a job. */
export function renderJobDetailHtml(job: JobResponse, opts: RenderOptions): string {
    const view = buildJobView({ ...opts, job });
    return renderShell({
        nonce: opts.nonce,
        cspSource: opts.cspSource,
        regions: renderRegions(view),
        rows: opts.run?.rows ?? [],
    });
}
