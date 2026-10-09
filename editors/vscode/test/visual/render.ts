/**
 * Visual harness for the job panel: renders every panel state under four
 * VS Code themes to standalone HTML, for screenshots and design review.
 * Not packaged (`test/**` is in .vscodeignore).
 *
 *   npx tsx test/visual/render.ts <out-dir>
 */

import * as fs from 'fs';
import * as path from 'path';

import { renderJobDetailHtml, type RenderOptions } from '../../src/prover/jobDetailHtml';
import { applyEvent, newRunModel, type RunModel } from '../../src/prover/runModel';
import type { EventEnvelope, JobResponse, JobResultResponse, ObligationDto } from '../../src/prover/types';

const NONCE = 'visual';
const NOW = Date.now();
const iso = (offsetSeconds: number) => new Date(NOW + offsetSeconds * 1000).toISOString();

const THEMES: Record<string, { bodyClass: string; vars: Record<string, string> }> = {
    dark: {
        bodyClass: 'vscode-dark',
        vars: {
            'font-family': '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
            'font-size': '13px',
            'editor-font-family': 'Menlo, Monaco, "Courier New", monospace',
            'editor-font-size': '12px',
            'editor-background': '#1f1f1f',
            foreground: '#cccccc',
            descriptionForeground: '#9d9d9d',
            'testing-iconPassed': '#73c991',
            'testing-iconFailed': '#f14c4c',
            errorForeground: '#f85149',
            'editorWarning-foreground': '#cca700',
            'charts-purple': '#b180d7',
            'progressBar-background': '#0078d4',
            'widget-border': '#313131',
            'panel-border': '#2b2b2b',
            'textCodeBlock-background': '#2b2b2b',
            focusBorder: '#0078d4',
            'button-background': '#0078d4',
            'button-foreground': '#ffffff',
            'button-hoverBackground': '#026ec1',
            'button-border': '#ffffff12',
            'button-secondaryBackground': '#313131',
            'button-secondaryForeground': '#cccccc',
            'button-secondaryHoverBackground': '#3c3c3c',
            'textLink-foreground': '#4daafc',
            'textLink-activeForeground': '#4daafc',
            'list-hoverBackground': '#2a2d2e',
            'input-background': '#313131',
            'input-foreground': '#cccccc',
            'input-border': '#3c3c3c',
        },
    },
    light: {
        bodyClass: 'vscode-light',
        vars: {
            'font-family': '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
            'font-size': '13px',
            'editor-font-family': 'Menlo, Monaco, "Courier New", monospace',
            'editor-font-size': '12px',
            'editor-background': '#ffffff',
            foreground: '#3b3b3b',
            descriptionForeground: '#616161',
            'testing-iconPassed': '#388a34',
            'testing-iconFailed': '#f14c4c',
            errorForeground: '#f85149',
            'editorWarning-foreground': '#bf8803',
            'charts-purple': '#652d90',
            'progressBar-background': '#005fb8',
            'widget-border': '#e5e5e5',
            'panel-border': '#e5e5e5',
            'textCodeBlock-background': '#f3f3f3',
            focusBorder: '#005fb8',
            'button-background': '#005fb8',
            'button-foreground': '#ffffff',
            'button-hoverBackground': '#0258a8',
            'button-border': '#0000001a',
            'button-secondaryBackground': '#e5e5e5',
            'button-secondaryForeground': '#3b3b3b',
            'button-secondaryHoverBackground': '#cccccc',
            'textLink-foreground': '#005fb8',
            'textLink-activeForeground': '#005fb8',
            'list-hoverBackground': '#f2f2f2',
            'input-background': '#ffffff',
            'input-foreground': '#3b3b3b',
            'input-border': '#cecece',
        },
    },
    'hc-dark': {
        bodyClass: 'vscode-high-contrast',
        vars: {
            'font-family': '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
            'font-size': '13px',
            'editor-font-family': 'Menlo, Monaco, "Courier New", monospace',
            'editor-font-size': '12px',
            'editor-background': '#000000',
            foreground: '#ffffff',
            descriptionForeground: '#ffffff',
            'testing-iconPassed': '#73c991',
            'testing-iconFailed': '#f14c4c',
            errorForeground: '#f48771',
            'editorWarning-foreground': '#ffd370',
            'charts-purple': '#b180d7',
            'progressBar-background': '#6fc3df',
            contrastBorder: '#6fc3df',
            contrastActiveBorder: '#f38518',
            'textCodeBlock-background': '#000000',
            focusBorder: '#f38518',
            'button-background': '#000000',
            'button-foreground': '#ffffff',
            'button-border': '#6fc3df',
            'button-secondaryForeground': '#ffffff',
            'textLink-foreground': '#21a6ff',
            'textLink-activeForeground': '#21a6ff',
            'input-background': '#000000',
            'input-foreground': '#ffffff',
            'input-border': '#6fc3df',
        },
    },
    'hc-light': {
        bodyClass: 'vscode-high-contrast vscode-high-contrast-light',
        vars: {
            'font-family': '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
            'font-size': '13px',
            'editor-font-family': 'Menlo, Monaco, "Courier New", monospace',
            'editor-font-size': '12px',
            'editor-background': '#ffffff',
            foreground: '#292929',
            descriptionForeground: '#292929',
            'testing-iconPassed': '#388a34',
            'testing-iconFailed': '#b5200d',
            errorForeground: '#b5200d',
            'editorWarning-foreground': '#895503',
            'charts-purple': '#652d90',
            'progressBar-background': '#0f4a85',
            contrastBorder: '#0f4a85',
            contrastActiveBorder: '#006bbd',
            'textCodeBlock-background': '#ffffff',
            focusBorder: '#006bbd',
            'button-background': '#0f4a85',
            'button-foreground': '#ffffff',
            'button-border': '#0f4a85',
            'button-secondaryForeground': '#292929',
            'textLink-foreground': '#0f4a85',
            'textLink-activeForeground': '#0f4a85',
            'input-background': '#ffffff',
            'input-foreground': '#292929',
            'input-border': '#0f4a85',
        },
    },
};

function events(run: RunModel, list: Array<[number, string, unknown?]>): RunModel {
    let seq = run.lastSeq;
    for (const [offset, type, payload] of list) {
        applyEvent(run, { schemaVersion: 1, jobId: 'j', seq: ++seq, type, ts: iso(offset), payload: payload ?? {} } as EventEnvelope);
    }
    return run;
}

const TELEMETRY_OBLIGATIONS: ObligationDto[] = [
    { name: 'valid_telemetry_controller', kind: 'module', content: 'structural', status: 'proved', attempts: 0, durationMs: 1400, goal: '1 goal\n  ho : host\n  ============================\n  ValidModule telemetry_controller' },
    { name: 'valid_telemetry_controller__low_power_inhibits_payload', kind: 'spec', content: 'grounded', status: 'proved', attempts: 2, durationMs: 63_000, specList: 'low_power_inhibits_payload', goal: '1 goal\n  ============================\n  ValidSpecFI telemetry_controller [:: low_power_inhibits_payload]' },
    { name: 'valid_telemetry_controller__overheat_inhibits_payload', kind: 'spec', content: 'grounded', status: 'proved', attempts: 1, durationMs: 41_000 },
    { name: 'valid_telemetry_controller__nominal_allows_payload', kind: 'spec', content: 'grounded', status: 'proved', attempts: 1, durationMs: 38_500 },
];

const TELEMETRY: JobResponse = {
    id: '01a1157b-8009-7c5e-8d07-8151bf449253', status: 'Succeeded', filename: 'telemetry_controller.input.v',
    createdAt: iso(-330), maxWallClockSeconds: 1800, holesTotal: 4, holesClosed: 4, holesFailed: 0,
    mode: 'prove', outcome: 'Succeeded', claimClass: 'Verified', obligations: TELEMETRY_OBLIGATIONS,
};

const artifacts = (kinds: string[]) => kinds.map((kind, i) => ({
    id: `art-${i}`, kind,
    filename: ({ InputV: 'input.v', CompletedV: 'completed.v', Report: 'report.json', Goals: 'goals.json', BuildLog: 'build.log', RefutationV: 'refutation.v', RefutationMd: 'refutation.md' } as Record<string, string>)[kind] ?? `${kind}.txt`,
    sizeBytes: 1000 + i * 4321, sha256: 'ab'.repeat(32),
}));

const TELEMETRY_RESULT: JobResultResponse = {
    id: TELEMETRY.id, status: 'Succeeded', mode: 'prove', claimClass: 'Verified', verificationOutcome: 'succeeded',
    verifiedAt: iso(-20), compileOk: true, assumptionsClosed: true, verifiedClean: true, admitsDetected: false,
    statementsImmutable: true, markersRemaining: 0, holesTotal: 4, holesClosed: 4,
    assumptionPolicyId: 'wasm-verifier@7741edb/coq@8.20.1/coq-wasm@v2.2.0/assumptions-v1',
    assumptionReports: TELEMETRY_OBLIGATIONS.map((o, i) => ({
        target: o.name,
        assumptions: i === 1 ? [{ name: 'Coq.Logic.FunctionalExtensionality.functional_extensionality_dep', type: 'forall (A : Type) (B : A -> Type) (f g : forall x : A, B x), (forall x : A, f x = g x) -> f = g', kind: 'axiom' as const }] : [],
        kernelOutput: i === 1 ? 'Axioms:\nfunctional_extensionality_dep : forall ...' : 'Closed under the global context',
        policyPassed: true,
    })),
    artifacts: artifacts(['InputV', 'CompletedV', 'Report', 'Goals', 'BuildLog']),
};

function telemetryRun(done: boolean): RunModel {
    const run = events(newRunModel(), [
        [-330, 'job.accepted', { holes: 4, filename: 'telemetry_controller.input.v' }],
        [-330, 'job.queued', { deadlineUtc: iso(1470) }],
        [-329, 'job.provisioning', { attempt: 1 }],
        [-328, 'vm.booting', {}],
        [-205, 'vm.online', {}],
        [-205, 'job.compiling', { holes: 4 }],
        [-196, 'obligations.discovered', { obligations: TELEMETRY_OBLIGATIONS.map((o, i) => ({ name: o.name, sourceLine: 80 + i * 6 })) }],
        [-196, 'job.proving', { holes: 4 }],
        [-196, 'obligation.started', { name: TELEMETRY_OBLIGATIONS[0].name }],
        [-195, 'obligation.proved', { name: TELEMETRY_OBLIGATIONS[0].name, attempt: 0, by: 'template' }],
        [-194, 'obligation.started', { name: TELEMETRY_OBLIGATIONS[1].name }],
        [-193, 'obligation.attempt', { name: TELEMETRY_OBLIGATIONS[1].name, attempt: 1 }],
        [-190, 'agent.activity', { schemaVersion: 1, agentRef: 'claude-code', attempt: 1, theorem: TELEMETRY_OBLIGATIONS[1].name, kind: 'thinking', detail: 'The spec says the payload is inhibited when battery_mv < 3300. Unfold the function body and case on the comparison.', usage: {} }],
        [-170, 'agent.activity', { schemaVersion: 1, agentRef: 'claude-code', attempt: 1, theorem: TELEMETRY_OBLIGATIONS[1].name, kind: 'tool:Bash', detail: 'coqc -Q . Proof telemetry_controller.v', usage: {} }],
        [-160, 'agent.activity', { schemaVersion: 1, agentRef: 'claude-code', attempt: 1, theorem: TELEMETRY_OBLIGATIONS[1].name, kind: 'tool_result', detail: 'Error: Unable to unify "i32" with "nat".', usage: {} }],
        [-150, 'obligation.attempt', { name: TELEMETRY_OBLIGATIONS[1].name, attempt: 2 }],
        [-140, 'agent.activity', { schemaVersion: 1, agentRef: 'claude-code', attempt: 2, theorem: TELEMETRY_OBLIGATIONS[1].name, kind: 'text', detail: 'Using Wasm_int.Int32 lemmas instead of nat arithmetic.', usage: {} }],
        [-131, 'obligation.proved', { name: TELEMETRY_OBLIGATIONS[1].name, attempt: 2, durationMs: 63000 }],
        [-130, 'obligation.started', { name: TELEMETRY_OBLIGATIONS[2].name }],
        [-89, 'obligation.proved', { name: TELEMETRY_OBLIGATIONS[2].name, attempt: 1, durationMs: 41000 }],
        [-88, 'obligation.started', { name: TELEMETRY_OBLIGATIONS[3].name }],
    ]);
    if (done) {
        events(run, [
            [-50, 'obligation.proved', { name: TELEMETRY_OBLIGATIONS[3].name, attempt: 1, durationMs: 38500 }],
            [-49, 'job.verifying', {}],
            [-20, 'job.completed', { outcome: 'Succeeded' }],
        ]);
    }
    return run;
}

interface Fixture {
    job: JobResponse;
    opts: Omit<RenderOptions, 'nonce' | 'cspSource'>;
}

const BOUNDS: JobResponse = {
    id: '01a1202a-2143-7271-ad0b-ca5788a4a451', status: 'Succeeded', filename: 'bounds.v', createdAt: iso(-160),
    maxWallClockSeconds: 1800, holesTotal: 1, holesClosed: 1, holesFailed: 0, mode: 'prove', outcome: 'Succeeded',
    claimClass: 'StructuralOnly',
    obligations: [{ name: 'valid_bounds', kind: 'module', content: 'structural', status: 'proved', attempts: 0, durationMs: 1439, goal: '1 goal\n  hfc : host_function_class\n  memory : Memory\n  ho : host\n  ============================\n  ValidModule bounds' }],
};

const FIXTURES: Record<string, Fixture> = {
    verified: { job: TELEMETRY, opts: { live: 'terminal', result: TELEMETRY_RESULT, run: telemetryRun(true), source: 'telemetry_controller.inf' } },
    running: {
        job: {
            ...TELEMETRY, status: 'Running', outcome: null, claimClass: 'Unverified', holesClosed: 2, mode: null,
            obligations: TELEMETRY_OBLIGATIONS.map((o, i) => ({ ...o, status: i < 2 ? 'proved' : i === 2 ? 'running' : 'pending', durationMs: i < 2 ? o.durationMs : null })),
        },
        opts: { live: 'sse', run: telemetryRun(false), source: 'telemetry_controller.inf' },
    },
    structural: {
        job: BOUNDS,
        opts: {
            live: 'terminal', source: 'bounds.inf',
            result: {
                id: BOUNDS.id, status: 'Succeeded', mode: 'prove', claimClass: 'StructuralOnly', verificationOutcome: 'succeeded', verifiedAt: iso(-10),
                compileOk: true, assumptionsClosed: true, verifiedClean: true, admitsDetected: false, statementsImmutable: true, markersRemaining: 0,
                holesTotal: 1, holesClosed: 1, assumptionPolicyId: 'wasm-verifier@7741edb/assumptions-v1',
                assumptionReports: [{ target: 'valid_bounds', assumptions: [], kernelOutput: 'Closed under the global context', policyPassed: true }],
                artifacts: artifacts(['InputV', 'CompletedV', 'Report', 'Goals', 'BuildLog']),
            },
            run: events(newRunModel(), [
                [-160, 'job.accepted', { holes: 1 }], [-160, 'job.queued', {}], [-159, 'job.provisioning', {}], [-158, 'vm.booting', {}],
                [-30, 'vm.online', {}], [-30, 'job.compiling', {}], [-20, 'obligations.discovered', { obligations: [{ name: 'valid_bounds', sourceLine: 83 }] }],
                [-20, 'job.proving', { holes: 1 }], [-20, 'obligation.started', { name: 'valid_bounds' }],
                [-19, 'obligation.proved', { name: 'valid_bounds', attempt: 0, by: 'template' }], [-18, 'job.verifying', {}], [-6, 'job.completed', {}],
            ]),
        },
    },
    'compile-error': {
        job: {
            id: '01a11fe4-78f9-7dbe-bde5-9a6b007de656', status: 'Failed', filename: 'clamp.v', createdAt: iso(-130),
            maxWallClockSeconds: 1800, holesTotal: 1, holesClosed: 0, holesFailed: 0, mode: 'compile-goals', outcome: 'Failed',
            claimClass: 'Unverified', errorCode: 'compile_failed', errorReason: 'file did not compile (see build.log)',
            obligations: [{ name: 'valid_clamp', kind: 'module', content: 'structural', status: 'pending' }],
        },
        opts: {
            live: 'terminal', source: 'clamp.inf',
            buildError: { line: 53, startChar: 11, endChar: 16, message: 'clamp already exists.' },
            result: { id: '01a11fe4-78f9-7dbe-bde5-9a6b007de656', status: 'Failed', holesTotal: 1, holesClosed: 0, artifacts: artifacts(['InputV', 'Report', 'BuildLog']) },
            run: events(newRunModel(), [
                [-130, 'job.accepted', { holes: 1 }], [-130, 'job.queued', {}], [-129, 'job.provisioning', {}], [-128, 'vm.booting', {}],
                [-6, 'vm.online', {}], [-6, 'job.compiling', {}], [-1, 'job.errored', { errorCode: 'compile_failed' }],
            ]),
        },
    },
    rejected: {
        job: TELEMETRY,
        opts: { live: 'terminal', result: { ...TELEMETRY_RESULT, verifiedClean: false, admitsDetected: true, markersRemaining: 1 }, run: telemetryRun(true), source: 'telemetry_controller.inf' },
    },
    partial: {
        job: {
            ...TELEMETRY, status: 'PartialSuccess', outcome: 'PartialSuccess', claimClass: 'PartialVerified', holesClosed: 3, holesFailed: 1,
            obligations: TELEMETRY_OBLIGATIONS.map((o, i) => (i === 3 ? { ...o, status: 'failed', lastError: 'attempt budget exhausted after 3 attempts' } : o)),
        },
        opts: {
            live: 'terminal', source: 'telemetry_controller.inf', run: telemetryRun(true),
            result: { ...TELEMETRY_RESULT, status: 'PartialSuccess', claimClass: 'PartialVerified', verificationOutcome: 'partial', verifiedClean: false, admitsDetected: true, holesClosed: 3, assumptionReports: TELEMETRY_RESULT.assumptionReports!.slice(0, 3) },
        },
    },
    refuted: {
        job: { ...TELEMETRY, status: 'Failed', outcome: 'Failed', claimClass: 'Unverified', holesClosed: 3, errorCode: 'spec_refuted', errorReason: 'spec refuted', obligations: TELEMETRY_OBLIGATIONS.map((o, i) => (i === 3 ? { ...o, status: 'refuted' } : o)) },
        opts: { live: 'terminal', source: 'telemetry_controller.inf', run: telemetryRun(true), result: { id: TELEMETRY.id, status: 'Failed', holesTotal: 4, holesClosed: 3, artifacts: artifacts(['InputV', 'RefutationV', 'RefutationMd', 'Report']) } },
    },
    'timed-out': {
        job: { ...BOUNDS, status: 'TimedOut', outcome: 'TimedOut', claimClass: 'Unverified', holesClosed: 0, errorCode: 'QUEUE_TIMEOUT', errorReason: 'queue timeout', obligations: [] },
        opts: { live: 'terminal', source: 'bounds.inf', result: { id: BOUNDS.id, status: 'TimedOut', holesTotal: 1, holesClosed: 0, artifacts: [] } },
    },
    canceling: {
        job: { ...BOUNDS, status: 'Canceling', outcome: null, claimClass: 'Unverified', holesClosed: 0, obligations: [] },
        opts: { live: 'sse', source: 'bounds.inf', run: events(newRunModel(), [[-20, 'job.accepted', { holes: 1 }], [-20, 'job.queued', {}], [-19, 'job.provisioning', {}], [-18, 'vm.booting', {}], [-9, 'job.cancelling', {}]]) },
    },
    queued: {
        job: { id: '01a120af-bfdd-7424-8f43-f65b7ee04981', status: 'Queued', filename: 'bounds.v', createdAt: iso(-4), maxWallClockSeconds: 1800, holesTotal: 1, holesClosed: 0, obligations: [] },
        opts: { live: 'sse', source: 'bounds.inf', run: events(newRunModel(), [[-4, 'job.accepted', { holes: 1 }], [-4, 'job.queued', {}]]) },
    },
    loading: { job: { id: '01a120af-bfdd-7424-8f43-f65b7ee04981', status: 'Succeeded', filename: 'bounds.v' }, opts: { live: 'terminal', detailLoaded: false, source: 'bounds.inf' } },
    many: {
        job: {
            ...TELEMETRY, status: 'Running', outcome: null, mode: null, claimClass: 'Unverified', holesTotal: 15, holesClosed: 9, holesFailed: 1,
            obligations: Array.from({ length: 15 }, (_, i): ObligationDto => ({
                name: i === 0 ? 'valid_command_receiver' : `valid_command_receiver__spec_${String(i).padStart(2, '0')}_${['accepts_valid_frame', 'rejects_bad_crc', 'ignores_replay', 'bounds_length'][i % 4]}`,
                kind: i === 0 ? 'module' : 'spec', content: i === 0 ? 'structural' : 'grounded',
                status: i < 9 ? 'proved' : i === 9 ? 'failed' : i === 10 ? 'running' : 'pending',
                attempts: i < 10 ? 1 + (i % 3) : null, durationMs: i < 10 ? 12000 + i * 3100 : null,
                lastError: i === 9 ? 'The term "Wasm_int.Int32.repr 0" has type "i32" while it is expected to have type "nat".' : null,
            })),
        },
        opts: { live: 'poll', source: 'command_receiver.inf', run: telemetryRun(false) },
    },
};

function themed(html: string, theme: (typeof THEMES)[string], clock: unknown): string {
    const vars = Object.entries(theme.vars).map(([k, v]) => `--vscode-${k}: ${v};`).join('\n');
    const stub = `<script nonce="${NONCE}">
window.__posted = [];
window.acquireVsCodeApi = () => ({ postMessage: (m) => window.__posted.push(m), getState: () => window.__state, setState: (s) => { window.__state = s; } });
window.addEventListener('load', () => window.postMessage({ command: 'clock', clock: ${JSON.stringify(clock)} }, '*'));
</script>`;
    return html
        .replace('<style>', `<style>:root { ${vars} } html, body { background: var(--vscode-editor-background); }</style>\n<style>`)
        .replace('<body>', `<body class="${theme.bodyClass}">`)
        .replace(`<script nonce="${NONCE}">`, `${stub}\n<script nonce="${NONCE}">`);
}

function main(): void {
    const out = path.resolve(process.argv[2] ?? 'visual-out');
    fs.mkdirSync(out, { recursive: true });
    const index: string[] = [];
    for (const [name, fixture] of Object.entries(FIXTURES)) {
        const html = renderJobDetailHtml(fixture.job, { ...fixture.opts, nonce: NONCE, cspSource: 'vscode-webview://visual' });
        const deadline = fixture.opts.run?.deadlineUtc ?? (fixture.job.createdAt ? new Date(Date.parse(fixture.job.createdAt) + (fixture.job.maxWallClockSeconds ?? 0) * 1000).toISOString() : undefined);
        const terminal = !['Accepted', 'Queued', 'Provisioning', 'Booting', 'Running', 'Verifying', 'Canceling'].includes(fixture.job.status);
        const clock = { start: fixture.job.createdAt, deadline, end: terminal ? fixture.opts.run?.endedAt : undefined, budgetSeconds: fixture.job.maxWallClockSeconds, running: !terminal };
        for (const [themeName, theme] of Object.entries(THEMES)) {
            const file = `${name}.${themeName}.html`;
            fs.writeFileSync(path.join(out, file), themed(html, theme, clock));
            index.push(file);
        }
    }
    fs.writeFileSync(path.join(out, 'index.json'), JSON.stringify(index, null, 2));
    console.log(`${index.length} pages in ${out}`);
}

main();
