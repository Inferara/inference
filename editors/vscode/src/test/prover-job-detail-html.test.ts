import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    escapeHtml,
    isCancelableStatus,
    isStreamTerminalStatus,
    isTerminalStatus,
    renderJobDetailHtml,
    type RenderOptions,
} from '../prover/jobDetailHtml';
import type {
    JobResponse,
    JobResultResponse,
    RunMode,
} from '../prover/types';

const BASE_OPTS: RenderOptions = {
    nonce: 'test-nonce-123',
    cspSource: 'vscode-webview://test-source',
    error: null,
    live: 'terminal',
    eventLog: [],
    canCancel: false,
};

const JOB: JobResponse = {
    id: '019e9b46-3c0e-765a-8212-2487dc9f3cc9',
    status: 'Succeeded',
    filename: 'fixture-stdlib.v',
    createdAt: '2026-06-05T10:00:00Z',
    provider: 'kubernetes',
    agentRef: 'claude-code',
    maxWallClockSeconds: 1800,
    holesTotal: 2,
    holesClosed: 2,
    holesFailed: 0,
    lastEventSeq: 21,
    mode: 'prove',
    claimClass: 'Verified',
    obligations: [
        {
            name: 'valid_fixture__Add',
            kind: 'spec',
            content: 'grounded',
            status: 'proved',
            attempts: 1,
            durationMs: 4200,
            goal: '1 + 1 = 2',
        },
        {
            name: 'valid_fixture__Comm',
            kind: 'spec',
            content: 'grounded',
            status: 'proved',
            attempts: 1,
            durationMs: 61_000,
            goal: 'forall n m : nat, n + m = m + n',
        },
    ],
    outcome: 'Succeeded',
};

const RESULT: JobResultResponse = {
    id: JOB.id,
    status: 'Succeeded',
    mode: 'prove',
    claimClass: 'Verified',
    verificationOutcome: 'succeeded',
    verifiedAt: '2026-06-05T10:02:00Z',
    compileOk: true,
    assumptionsClosed: true,
    verifiedClean: true,
    admitsDetected: false,
    statementsImmutable: true,
    markersRemaining: 0,
    holesTotal: 2,
    holesClosed: 2,
    assumptionPolicyId: 'wasm-verifier@test/assumptions-v1',
    assumptionReports: [
        {
            target: 'valid_fixture__Add',
            assumptions: [{
                name: 'Coq.Logic.Classical_Prop.classic',
                type: 'forall P : Prop, or P (not P)',
                kind: 'axiom',
            }, {
                name: 'P',
                type: 'Prop',
                kind: 'sectionVariable',
            }],
            kernelOutput: 'Section Variables:\nP\n: Prop\nAxioms:\nClassical_Prop.classic : forall P : Prop, or P (not P)',
            policyPassed: true,
        },
        {
            target: 'valid_fixture__Comm',
            assumptions: [],
            kernelOutput: 'Closed under the global context',
            policyPassed: true,
        },
    ],
    artifacts: [
        {
            id: 'a1a1a1a1-0000-7000-8000-000000000001',
            kind: 'CompletedV',
            filename: 'completed.v',
            sizeBytes: 20_480,
            sha256: 'ab'.repeat(32),
        },
        {
            id: 'a1a1a1a1-0000-7000-8000-000000000002',
            kind: 'Report',
            filename: 'report.json',
            sizeBytes: 512,
            sha256: 'cd'.repeat(32),
        },
    ],
};

describe('escapeHtml', () => {
    it('escapes all five HTML metacharacters', () => {
        assert.strictEqual(
            escapeHtml(`<script>alert("x&y'")</script>`),
            '&lt;script&gt;alert(&quot;x&amp;y&#39;&quot;)&lt;/script&gt;',
        );
    });
});

describe('isTerminalStatus', () => {
    it('matches the server state machine', () => {
        for (const s of [
            'Succeeded',
            'CompileGoals',
            'PartialSuccess',
            'Failed',
            'TimedOut',
            'Lost',
            'Canceled',
            'ProvisionFailed',
        ]) {
            assert.ok(isTerminalStatus(s), `${s} should be terminal`);
        }
        for (const s of [
            'Accepted',
            'Queued',
            'Provisioning',
            'Booting',
            'Running',
            'Verifying',
            'Canceling',
        ]) {
            assert.ok(!isTerminalStatus(s), `${s} should be active`);
        }
    });
});

describe('isStreamTerminalStatus', () => {
    it('mirrors the server stream contract: Lost keeps the stream open', () => {
        // StreamEvents only sends `end` for terminal statuses OTHER than
        // Lost — the reaper may re-queue a Lost job (bounded retry), so the
        // client must keep its transport alive.
        assert.ok(!isStreamTerminalStatus('Lost'));
        assert.ok(isTerminalStatus('Lost')); // still terminal for display
        for (const s of [
            'Succeeded',
            'CompileGoals',
            'PartialSuccess',
            'Failed',
            'TimedOut',
            'Canceled',
            'ProvisionFailed',
        ]) {
            assert.ok(isStreamTerminalStatus(s), `${s} should end the stream`);
        }
        for (const s of ['Running', 'Verifying', 'Canceling', 'Queued']) {
            assert.ok(!isStreamTerminalStatus(s), `${s} is live`);
        }
    });
});

describe('isCancelableStatus', () => {
    it('mirrors the statuses POST /cancel accepts', () => {
        for (const s of [
            'Accepted',
            'Queued',
            'Provisioning',
            'Booting',
            'Running',
        ]) {
            assert.ok(isCancelableStatus(s), `${s} should be cancelable`);
        }
        // Verifying and Canceling are NOT cancelable (server returns 409),
        // and neither is anything terminal.
        for (const s of [
            'Verifying',
            'Canceling',
            'Succeeded',
            'CompileGoals',
            'Failed',
        ]) {
            assert.ok(!isCancelableStatus(s), `${s} should not be cancelable`);
        }
    });
});

describe('renderJobDetailHtml', () => {
    it('renders the core job facts', () => {
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(html.includes('fixture-stdlib.v'));
        assert.ok(html.includes('Succeeded'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('Certificate evidence unavailable'));
        assert.ok(
            html.includes(
                '<div class="meta-label">Mode</div><div class="meta-value">prove</div>',
            ),
        );
        assert.ok(html.includes('2/2 closed'));
        assert.ok(html.includes('valid_fixture__Add'));
        assert.ok(html.includes('valid_fixture__Comm'));
        assert.ok(html.includes('<th>Content</th>'));
        assert.ok(html.includes('grounded'));
        assert.ok(html.includes('Claim class'));
        assert.ok(html.includes('Verified'));
        assert.ok(html.includes('4200 ms') || html.includes('4.2 s'));
        assert.ok(html.includes('1m 1s'));
    });

    it('fails closed for Succeeded records without durable prove mode', () => {
        for (const mode of [
            undefined,
            null,
            'unknown',
            'compile-goals',
        ] satisfies Array<RunMode | null | undefined>) {
            const html = renderJobDetailHtml({ ...JOB, mode }, BASE_OPTS);
            assert.ok(
                html.includes('<span class="badge warn">Succeeded</span>'),
                `mode ${String(mode)} should be warning`,
            );
            assert.ok(
                !html.includes('<span class="badge ok">Succeeded</span>'),
                `mode ${String(mode)} must not be green`,
            );
            assert.ok(html.includes('<strong>Not proven.</strong>'));
        }
    });

    it('fails closed when job and result proof modes disagree', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, mode: 'compile-goals' },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('<strong>Not proven.</strong>'));
        assert.ok(html.includes('Job detail and result DTOs do not agree'));
    });

    it('fails closed when Succeeded proof counts are incomplete', () => {
        const html = renderJobDetailHtml(
            { ...JOB, holesTotal: 2, holesClosed: 1 },
            BASE_OPTS,
        );
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('every hole closed'));
    });

    it('fails closed when job and result proof counts disagree', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, holesClosed: 1 },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('Job detail and result DTOs do not agree'));
    });

    it('fails closed when independent verifier flags reject Succeeded', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, verifiedClean: false, admitsDetected: true },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('independent verifier evidence rejects'));
    });

    it('fails closed when a target fails the assumption policy', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                assumptionsClosed: false,
                verifiedClean: false,
                assumptionReports: RESULT.assumptionReports!.map((report, index) =>
                    index === 0 ? { ...report, policyPassed: false } : report),
            },
        });
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('independent verifier evidence rejects'));
        assert.ok(html.includes('Assumption policy passed'));
        assert.ok(html.includes('policy rejected'));
    });

    it('fails closed when verifier outcome disagrees with Succeeded', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                verificationOutcome: 'partial',
                verifiedClean: false,
                admitsDetected: true,
            },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('independent verifier evidence rejects'));
    });

    it('renders CompileGoals as a terminal warning, never a green success', () => {
        const mode: RunMode = 'compile-goals';
        const compileGoals: JobResponse = {
            ...JOB,
            status: 'CompileGoals',
            mode,
            claimClass: 'Unverified',
            holesClosed: 0,
            outcome: 'CompileGoals',
        };
        const html = renderJobDetailHtml(compileGoals, BASE_OPTS);
        assert.ok(isTerminalStatus(compileGoals.status));
        assert.ok(isStreamTerminalStatus(compileGoals.status));
        assert.ok(html.includes('<span class="badge warn">CompileGoals</span>'));
        assert.ok(!html.includes('<span class="badge ok">CompileGoals</span>'));
        assert.ok(
            html.includes(
                '<div class="meta-label">Mode</div><div class="meta-value">compile-goals</div>',
            ),
        );
        assert.ok(html.includes('<strong>Compile-only result.</strong>'));
        assert.ok(html.includes('no proof was accepted or returned'));

        const failedCompile = renderJobDetailHtml(
            {
                ...compileGoals,
                status: 'Failed',
                outcome: 'Failed',
                errorCode: 'WORKER_FAILED',
            },
            BASE_OPTS,
        );
        assert.ok(failedCompile.includes('<strong>Compile-only run.</strong>'));
        assert.ok(!failedCompile.includes('The file compiled'));
    });

    it('does not invent a verifier verdict for compile-goals results', () => {
        const compileGoals: JobResponse = {
            ...JOB,
            status: 'CompileGoals',
            mode: 'compile-goals',
            claimClass: 'Unverified',
            holesClosed: 0,
            outcome: 'CompileGoals',
        };
        const compileResult: JobResultResponse = {
            ...RESULT,
            status: 'CompileGoals',
            mode: 'compile-goals',
            claimClass: 'Unverified',
            verificationOutcome: null,
            verifiedAt: null,
            compileOk: null,
            assumptionsClosed: null,
            verifiedClean: null,
            admitsDetected: null,
            statementsImmutable: null,
            markersRemaining: null,
            holesClosed: 0,
        };
        const html = renderJobDetailHtml(compileGoals, {
            ...BASE_OPTS,
            result: compileResult,
        });
        assert.ok(html.includes('<h2>Verification</h2>'));
        assert.ok(
            html.includes(
                'No independent verifier provenance is available; this is a worker-reported legacy result.',
            ),
        );
        assert.ok(!html.includes('Independent verifier verdict'));
        assert.ok(!html.includes('Verified clean (compiles, no admits)'));
    });

    it('renders the per-obligation goal state', () => {
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(html.includes('1 + 1 = 2'));
        assert.ok(html.includes('forall n m : nat, n + m = m + n'));
        assert.ok(html.includes('pre class="goal"'));
    });

    it('fails closed to unknown when legacy obligation content is missing', () => {
        const html = renderJobDetailHtml(
            {
                ...JOB,
                obligations: [{ name: 'legacy_obligation', kind: 'spec' }],
            },
            BASE_OPTS,
        );
        assert.ok(html.includes('legacy_obligation'));
        assert.ok(html.includes('<td>unknown</td>'));
    });

    it('escapes goal text (server-originated)', () => {
        const hostile: JobResponse = {
            ...JOB,
            obligations: [
                {
                    name: 'v',
                    kind: 'spec',
                    content: 'grounded',
                    goal: '<script>alert(1)</script>',
                },
            ],
        };
        const html = renderJobDetailHtml(hostile, BASE_OPTS);
        assert.ok(!html.includes('<script>alert(1)'));
        assert.ok(html.includes('&lt;script&gt;alert(1)&lt;/script&gt;'));
    });

    it('does not surface deployment-internal provider/agent to the user', () => {
        // The compute backend and the proof agent are internal details: the
        // user neither picks them nor needs to see them. (They remain on the
        // JobResponse for the API/e2e, just not rendered.)
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(!html.includes('claude-code'));
        assert.ok(!html.includes('kubernetes'));
        assert.ok(!/>\s*Provider\s*</.test(html));
        assert.ok(!/>\s*Agent\s*</.test(html));
        // And no raw-JSON dump (it would leak both, and is the dirty-JSON view
        // this webview replaced).
        assert.ok(!html.includes('Raw response'));
    });

    it('escapes server-originated strings everywhere', () => {
        const hostile: JobResponse = {
            ...JOB,
            filename: '<img src=x onerror=alert(1)>.v',
            errorCode: 'X<Y',
            errorReason: '<script>steal()</script>',
            obligations: [
                {
                    name: 'valid_<b>bold</b>',
                    kind: 'unknown',
                    content: 'unknown',
                    status: 'failed',
                    lastError: '"; window.close(); //',
                },
            ],
        };
        const html = renderJobDetailHtml(hostile, BASE_OPTS);
        assert.ok(!html.includes('<img src=x'));
        assert.ok(!html.includes('<script>steal'));
        assert.ok(!html.includes('valid_<b>'));
        assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;.v'));
        assert.ok(html.includes('&lt;script&gt;steal()&lt;/script&gt;'));
    });

    it('escapes event-log lines', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            eventLog: ['12:00:00  log — <script>document.title="pwn"</script>'],
        });
        assert.ok(!html.includes('<script>document.title'));
        assert.ok(html.includes('&lt;script&gt;document.title'));
    });

    it('carries the CSP nonce on the only script tag', () => {
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(html.includes(`script-src 'nonce-test-nonce-123'`));
        assert.ok(html.includes('<script nonce="test-nonce-123">'));
        // Exactly one <script ...> opening tag in the document.
        assert.strictEqual(html.match(/<script[\s>]/g)?.length, 1);
    });

    it('shows the fetch-error banner while keeping stale data', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            error: 'Could not reach the proof server.',
        });
        assert.ok(html.includes('Could not reach the proof server.'));
        assert.ok(html.includes('fixture-stdlib.v'));
    });

    it('shows the error block for failed jobs', () => {
        const failed: JobResponse = {
            ...JOB,
            status: 'Failed',
            errorCode: 'VERIFICATION_FAILED',
            errorReason: 'admits detected',
        };
        const html = renderJobDetailHtml(failed, BASE_OPTS);
        assert.ok(html.includes('VERIFICATION_FAILED'));
        assert.ok(html.includes('admits detected'));
        assert.ok(html.includes('auto-refresh stopped'));
    });

    it('reflects the live-update mode in the toolbar note', () => {
        const running: JobResponse = { ...JOB, status: 'Running' };
        const sse = renderJobDetailHtml(running, { ...BASE_OPTS, live: 'sse' });
        assert.ok(sse.includes('Live — streaming events'));
        const poll = renderJobDetailHtml(running, {
            ...BASE_OPTS,
            live: 'poll',
        });
        assert.ok(poll.includes('Polling every 5 s'));
    });

    it('shows the Cancel button only when the status is cancelable', () => {
        const running: JobResponse = { ...JOB, status: 'Running' };
        const withCancel = renderJobDetailHtml(running, {
            ...BASE_OPTS,
            live: 'sse',
            canCancel: true,
        });
        assert.ok(withCancel.includes('id="cancel"'));
        const without = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(!without.includes('id="cancel"'));
    });

    it('renders an independent verifier verdict only with explicit provenance', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: RESULT,
        });
        assert.ok(html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('Independent verifier verdict'));
        assert.ok(html.includes('Compilation passed'));
        assert.ok(html.includes('Verified clean (all holes closed, no admits)'));
        assert.ok(html.includes('Statements immutable'));
        assert.ok(html.includes('Proof markers remaining'));
        assert.ok(html.includes('Kernel-reported assumptions'));
        assert.ok(html.includes('Classical_Prop.classic'));
        assert.ok(html.includes('sectionVariable'));
        assert.ok(html.includes('Normalized Print Assumptions output'));
        assert.ok(html.includes('6/5/2026') || html.includes('2026'));
        assert.ok(html.includes('completed.v'));
        assert.ok(html.includes('report.json'));
        assert.ok(html.includes('20.0 KiB'));
        assert.ok(
            html.includes(
                'data-artifact="a1a1a1a1-0000-7000-8000-000000000001"',
            ),
        );
        // No result → no verdict section.
        const bare = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(!bare.includes('Independent verifier verdict'));
    });

    it('fails closed and escapes malformed-looking assumption evidence', () => {
        const missing = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, assumptionReports: null },
        });
        assert.ok(!missing.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(missing.includes('Print Assumptions evidence is unavailable'));

        const malformed = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                assumptionReports: [{
                    target: 'valid_fixture__Add',
                    assumptions: null,
                    kernelOutput: 'Axioms:\nmalformed',
                    policyPassed: true,
                } as any],
            },
        });
        assert.ok(!malformed.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(malformed.includes('Print Assumptions evidence is unavailable'));

        const hostile = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                assumptionReports: [{
                    target: '<img src=x onerror=y>',
                    assumptions: [{
                        name: '<script>axiom</script>',
                        type: '<b>False</b>',
                        kind: 'axiom',
                    }],
                    kernelOutput: '<script>kernel</script>',
                    policyPassed: true,
                }, RESULT.assumptionReports![1]],
            },
        });
        assert.ok(!hostile.includes('<script>axiom'));
        assert.ok(hostile.includes('&lt;script&gt;axiom&lt;/script&gt;'));
        assert.ok(!hostile.includes('<img src=x'));
    });

    it('fails closed when reports name different obligations', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                assumptionReports: [
                    { ...RESULT.assumptionReports![0], target: 'foreign_a' },
                    { ...RESULT.assumptionReports![1], target: 'foreign_b' },
                ],
            },
        });
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('independent verifier evidence rejects'));
    });

    it('shows null-provenance Verified success as worker-reported', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                verificationOutcome: null,
                verifiedAt: null,
            },
        });
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('<strong>Worker-reported result.</strong>'));
        assert.ok(html.includes('worker-reported legacy result'));
        assert.ok(!html.includes('Independent verifier verdict'));
    });

    it('fails closed when a Succeeded record has no claim class', () => {
        const job: JobResponse = { ...JOB, claimClass: undefined };
        const html = renderJobDetailHtml(job, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                claimClass: undefined,
                verificationOutcome: null,
                verifiedAt: null,
            },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('<strong>Behavioral claim unverified.</strong>'));
        assert.ok(html.includes('did not provide a claim class'));
    });

    it('warning-labels StructuralOnly without claiming behavioral proof', () => {
        const job: JobResponse = {
            ...JOB,
            claimClass: 'StructuralOnly',
            obligations: JOB.obligations?.map((obligation) => ({
                ...obligation,
                content: 'structural',
            })),
        };
        const html = renderJobDetailHtml(job, {
            ...BASE_OPTS,
            result: { ...RESULT, claimClass: 'StructuralOnly' },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('<strong>Structural validity only.</strong>'));
        assert.ok(html.includes('not a functional-correctness claim'));
        assert.ok(html.includes('StructuralOnly'));
        assert.ok(html.includes('structural'));
    });

    it('fails closed when job and result claim classes disagree', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, claimClass: 'StructuralOnly' },
        });
        assert.ok(html.includes('<span class="badge warn">Succeeded</span>'));
        assert.ok(!html.includes('<span class="badge ok">Succeeded</span>'));
        assert.ok(html.includes('Job detail and result DTOs do not agree'));
    });

    it('renders verifier-backed PartialSuccess as an accepted warning', () => {
        const partialJob: JobResponse = {
            ...JOB,
            status: 'PartialSuccess',
            outcome: 'PartialSuccess',
            claimClass: 'PartialVerified',
            holesClosed: 1,
            holesFailed: 1,
            obligations: JOB.obligations?.map((obligation, index) => ({
                ...obligation,
                status: index === 0 ? 'proved' : 'failed',
            })),
        };
        const partialResult: JobResultResponse = {
            ...RESULT,
            status: 'PartialSuccess',
            claimClass: 'PartialVerified',
            holesClosed: 1,
            verificationOutcome: 'partial',
            compileOk: true,
            verifiedClean: false,
            admitsDetected: true,
            statementsImmutable: true,
            markersRemaining: 0,
            assumptionReports: [RESULT.assumptionReports![0]],
        };
        const html = renderJobDetailHtml(partialJob, {
            ...BASE_OPTS,
            result: partialResult,
        });
        assert.ok(html.includes('<span class="badge warn">PartialSuccess</span>'));
        assert.ok(html.includes('<strong>Partial result accepted.</strong>'));
        assert.ok(!html.includes('<strong>Independent verification failed.</strong>'));
        assert.ok(!html.includes('<strong>Not proven.</strong>'));
        assert.ok(html.includes('unfinished obligations remain admitted'));
        assert.ok(html.includes('<span class="badge warn">partial</span>'));
    });

    it('renders the event log lines and the empty placeholder', () => {
        const lines = ['10:00:01  job.accepted — main.v, 3 holes', '10:00:05  vm.online'];
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            eventLog: lines,
        });
        for (const line of lines) {
            assert.ok(html.includes(line));
        }
        const empty = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(empty.includes('No events yet.'));
    });

    it('tolerates a minimal list-shaped job (everything optional missing)', () => {
        const minimal: JobResponse = { id: 'abc', status: 'Queued' };
        const html = renderJobDetailHtml(minimal, BASE_OPTS);
        assert.ok(html.includes('abc'));
        assert.ok(html.includes('Queued'));
        assert.ok(html.includes('0/0 closed'));
        assert.ok(html.includes('No obligations reported yet.'));
    });
});
