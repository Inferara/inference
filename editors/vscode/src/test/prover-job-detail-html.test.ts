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
import { JOB_DETAIL_STYLES } from '../prover/jobDetailStyles';
import { applyEvent, newRunModel } from '../prover/runModel';
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

/** The verdict the panel shows (`#verdict` data attributes). */
function verdictOf(html: string): { kind: string; tone: string; claim: string } {
    const m = /id="verdict" data-verdict="([^"]+)" data-tone="([^"]+)" data-claim="([^"]+)"/.exec(html);
    assert.ok(m, 'the panel renders a verdict');
    return { kind: m[1], tone: m[2], claim: m[3] };
}

/** No positive claim and no success tone. */
function assertNoProofClaim(html: string, message?: string): void {
    const v = verdictOf(html);
    assert.notStrictEqual(v.tone, 'ok', message);
    assert.strictEqual(v.claim, 'none', message);
    assert.ok(!html.includes('data-verdict="verified"'), message);
}

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
    it('renders the core job facts in plain words, raw values only under Technical details', () => {
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(html.includes('fixture-stdlib.v'));
        assert.ok(html.includes('data-field="mode"><code>prove</code>'));
        assert.ok(html.includes('data-field="claimClass"><code>Verified</code>'));
        assert.ok(html.includes('data-field="status"><code>Succeeded</code>'));
        assert.ok(html.includes('aria-valuenow="2"'));
        assert.ok(html.includes('aria-valuemax="2"'));
        assert.ok(html.includes('2 of 2 obligations closed'));
        // Short names in the list, full theorem names in the details.
        assert.ok(html.includes('>Add</span>'));
        assert.ok(html.includes('<code>valid_fixture__Comm</code>'));
        assert.ok(html.includes('>behavior</span>'));
        assert.ok(html.includes('States a behavioral property of the compiled module'));
        assert.ok(html.includes('4.2 s'));
        assert.ok(html.includes('1m 1s'));
    });

    it('makes no claim while the verifier verdict is loading, and says so when it failed', () => {
        const loading = renderJobDetailHtml(JOB, BASE_OPTS);
        assertNoProofClaim(loading);
        assert.strictEqual(verdictOf(loading).kind, 'loading');

        const failed = renderJobDetailHtml(JOB, { ...BASE_OPTS, resultState: 'failed' });
        assertNoProofClaim(failed);
        assert.ok(failed.includes('Verifier evidence unavailable'));
    });

    it('fails closed for Succeeded records without durable prove mode', () => {
        for (const mode of [
            undefined,
            null,
            'unknown',
        ] satisfies Array<RunMode | null | undefined>) {
            const html = renderJobDetailHtml({ ...JOB, mode }, { ...BASE_OPTS, result: { ...RESULT, mode } });
            assertNoProofClaim(html, `mode ${String(mode)} must not be a proof`);
            assert.strictEqual(verdictOf(html).tone, 'warn');
            assert.ok(html.includes('was not recorded as a proof run'));
        }
        const compileGoals = renderJobDetailHtml({ ...JOB, mode: 'compile-goals' }, BASE_OPTS);
        assertNoProofClaim(compileGoals);
        assert.strictEqual(verdictOf(compileGoals).kind, 'compile-only');
    });

    it('fails closed when job and result proof modes disagree', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, mode: 'compile-goals' },
        });
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'conflict');
        assert.ok(html.includes('The job record and its result disagree'));
    });

    it('fails closed when Succeeded proof counts are incomplete', () => {
        const html = renderJobDetailHtml(
            { ...JOB, holesTotal: 2, holesClosed: 1 },
            BASE_OPTS,
        );
        assertNoProofClaim(html);
        assert.ok(html.includes('does not show every proof hole closed'));
    });

    it('fails closed when job and result proof counts disagree', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, holesClosed: 1 },
        });
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'conflict');
    });

    it('fails closed when independent verifier flags reject Succeeded', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, verifiedClean: false, admitsDetected: true },
        });
        assertNoProofClaim(html);
        assert.deepStrictEqual(verdictOf(html), { kind: 'rejected', tone: 'err', claim: 'none' });
        assert.ok(html.includes('Independent verification failed'));
        assert.ok(html.includes('Some proofs are incomplete (still admitted).'));
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
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'rejected');
        assert.ok(html.includes('Only approved assumptions are used'));
        assert.ok(html.includes('policy rejected'));
        assert.ok(html.includes('relies on assumptions the policy does not allow'));
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
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'rejected');
        assert.ok(html.includes('outcome is “partial”, not “succeeded”'));
    });

    it('renders CompileGoals as compiled-only, never a success', () => {
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
        assertNoProofClaim(html);
        assert.deepStrictEqual(verdictOf(html), { kind: 'compile-only', tone: 'warn', claim: 'none' });
        assert.ok(html.includes('data-field="mode"><code>compile-goals</code>'));
        assert.ok(html.includes('Compiled — nothing was proved'));
        assert.ok(html.includes('No proof was accepted or returned.'));

        const failedCompile = renderJobDetailHtml(
            {
                ...compileGoals,
                status: 'Failed',
                outcome: 'Failed',
                errorCode: 'WORKER_FAILED',
            },
            BASE_OPTS,
        );
        assertNoProofClaim(failedCompile);
        assert.ok(failedCompile.includes('This was a compile-only run'));
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
        assert.ok(html.includes('How this was verified'));
        assert.ok(html.includes('Not applicable — this run only compiled the file.'));
        assert.ok(!html.includes('class="checks"'));
    });

    it('renders the per-obligation goal state', () => {
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(html.includes('1 + 1 = 2'));
        assert.ok(html.includes('forall n m : nat, n + m = m + n'));
        assert.ok(html.includes('pre class="goal"'));
    });

    it('fails closed to "not classified" when legacy obligation content is missing', () => {
        const html = renderJobDetailHtml(
            {
                ...JOB,
                obligations: [{ name: 'legacy_obligation', kind: 'spec' }],
            },
            BASE_OPTS,
        );
        assert.ok(html.includes('legacy_obligation'));
        assert.ok(html.includes('Not classified'));
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
        assert.ok(!html.includes('data-field="provider"'));
        assert.ok(!html.includes('data-field="agent'));
        assert.ok(!html.includes('Raw response'));
    });

    it('escapes server-originated strings everywhere', () => {
        const hostile: JobResponse = {
            ...JOB,
            status: 'Failed',
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
        assert.ok(!html.includes('X<Y'));
        assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;.v'));
        assert.ok(html.includes('&lt;script&gt;steal()&lt;/script&gt;'));
        assert.ok(html.includes('&quot;; window.close(); //'));
    });

    it('escapes activity rows', () => {
        const run = newRunModel();
        applyEvent(run, {
            schemaVersion: 1, jobId: JOB.id, seq: 1, type: 'log', ts: '2026-06-05T10:00:01Z',
            payload: { message: '<script>document.title="pwn"</script>' },
        });
        const html = renderJobDetailHtml(JOB, { ...BASE_OPTS, run });
        assert.ok(!html.includes('<script>document.title'));
        assert.ok(html.includes('&lt;script&gt;document.title'));
    });

    it('carries the CSP nonce on the only script tag', () => {
        const html = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(html.includes(`script-src 'nonce-test-nonce-123'`));
        assert.ok(html.includes('<script nonce="test-nonce-123">'));
        // Exactly one <script ...> opening tag in the document.
        assert.strictEqual(html.match(/<script[\s>]/g)?.length, 1);
        assert.ok(html.includes("default-src 'none'"));
    });

    it('shows the fetch-error banner while keeping stale data', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            error: 'Could not reach the proof server.',
        });
        assert.ok(html.includes('role="alert"'));
        assert.ok(html.includes('Could not reach the proof server.'));
        assert.ok(html.includes('fixture-stdlib.v'));
    });

    it('explains a failed job in words and keeps the raw code under Technical details', () => {
        const failed: JobResponse = {
            ...JOB,
            status: 'Failed',
            errorCode: 'VERIFICATION_FAILED',
            errorReason: 'admits detected',
        };
        const html = renderJobDetailHtml(failed, BASE_OPTS);
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'failed');
        assert.ok(html.includes('<p>admits detected</p>'));
        assert.ok(html.includes('data-field="errorCode"><code>VERIFICATION_FAILED</code>'));
        assert.ok(html.includes('auto-refresh stopped'));
        assert.ok(html.includes('data-action="resubmit"'));
    });

    it('explains known error codes instead of showing them', () => {
        const html = renderJobDetailHtml(
            { ...JOB, status: 'TimedOut', errorCode: 'QUEUE_TIMEOUT', errorReason: 'queue' },
            BASE_OPTS,
        );
        assert.strictEqual(verdictOf(html).kind, 'timed-out');
        assert.ok(html.includes('No worker became free before the time budget ran out.'));
    });

    it('reflects the live-update mode in the header', () => {
        const running: JobResponse = { ...JOB, status: 'Running' };
        const sse = renderJobDetailHtml(running, { ...BASE_OPTS, live: 'sse' });
        assert.ok(sse.includes('Live — streaming events'));
        const poll = renderJobDetailHtml(running, {
            ...BASE_OPTS,
            live: 'poll',
        });
        assert.ok(poll.includes('checking every 5 seconds'));
    });

    it('offers Cancel Job only when the status is cancelable', () => {
        const running: JobResponse = { ...JOB, status: 'Running' };
        const withCancel = renderJobDetailHtml(running, { ...BASE_OPTS, live: 'sse' });
        assert.ok(withCancel.includes('data-action="cancel"'));
        assert.ok(withCancel.includes('Cancel Job'));
        for (const status of ['Verifying', 'Canceling', 'Succeeded'] as const) {
            const without = renderJobDetailHtml({ ...JOB, status }, BASE_OPTS);
            assert.ok(!without.includes('data-action="cancel"'), status);
        }
    });

    it('claims a verified proof only with full independent verifier evidence', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: RESULT,
        });
        assert.deepStrictEqual(verdictOf(html), { kind: 'verified', tone: 'ok', claim: 'proved' });
        assert.ok(html.includes('Proved and independently verified'));
        assert.ok(html.includes('2 state behavioral properties of the compiled Wasm.'));
        assert.ok(html.includes('How this was verified'));
        assert.ok(html.includes('The returned file compiles'));
        assert.ok(html.includes('Every proof is complete (no admits)'));
        assert.ok(html.includes('Theorem statements are unchanged'));
        assert.ok(html.includes('No proof markers left'));
        assert.ok(html.includes('Kernel-reported assumptions'));
        assert.ok(html.includes('Classical_Prop.classic'));
        assert.ok(html.includes('section variable'));
        assert.ok(html.includes('Normalized Print Assumptions output'));
        assert.ok(html.includes('completed.v'));
        assert.ok(html.includes('report.json'));
        assert.ok(html.includes('20.0 KiB'));
        assert.ok(html.includes('data-artifact="a1a1a1a1-0000-7000-8000-000000000001"'));
        assert.ok(html.includes('aria-label="Download completed.v"'));
        // No result → no verifier section.
        const bare = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(!bare.includes('How this was verified'));
    });

    it('fails closed and escapes malformed-looking assumption evidence', () => {
        const missing = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, assumptionReports: null },
        });
        assertNoProofClaim(missing);
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
        assertNoProofClaim(malformed);
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
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'rejected');
        assert.ok(html.includes('assumption evidence is incomplete or rejected'));
    });

    it('shows a result without verifier provenance as not independently verified', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: {
                ...RESULT,
                verificationOutcome: null,
                verifiedAt: null,
            },
        });
        assertNoProofClaim(html);
        assert.ok(html.includes('Not independently verified'));
        assert.ok(html.includes('checked only inside the worker'));
        assert.ok(!html.includes('class="checks"'));
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
        assertNoProofClaim(html);
        assert.ok(html.includes('Behavior not verified'));
        assert.ok(html.includes('did not provide a claim class'));
    });

    it('labels StructuralOnly as structural validity only, never a behavioral proof', () => {
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
        assert.deepStrictEqual(verdictOf(html), { kind: 'structural', tone: 'warn', claim: 'structural' });
        assert.ok(!html.includes('data-claim="proved"'));
        assert.ok(html.includes('Structural validity only'));
        assert.ok(html.includes('this is not a functional-correctness result'));
        assert.ok(html.includes('data-field="claimClass"><code>StructuralOnly</code>'));
        assert.ok(html.includes('Structural (module typing)'));
    });

    it('fails closed when job and result claim classes disagree', () => {
        const html = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, claimClass: 'StructuralOnly' },
        });
        assertNoProofClaim(html);
        assert.strictEqual(verdictOf(html).kind, 'conflict');
    });

    it('renders verifier-backed PartialSuccess as partially proved', () => {
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
        assert.deepStrictEqual(verdictOf(html), { kind: 'partial', tone: 'warn', claim: 'partial' });
        assert.ok(html.includes('1 of 2 obligations closed and independently re-verified; the other 1 remain admitted'));
        assert.ok(html.includes('Every closed proof is complete'));
        assert.ok(html.includes('as expected for a partial result'));
        assert.ok(!html.includes('Independent verification failed'));
    });

    it('renders activity rows grouped by obligation, and the empty placeholder', () => {
        const run = newRunModel();
        applyEvent(run, { schemaVersion: 1, jobId: JOB.id, seq: 1, type: 'job.accepted', ts: '2026-06-05T10:00:01Z', payload: { holes: 3 } });
        applyEvent(run, { schemaVersion: 1, jobId: JOB.id, seq: 2, type: 'obligation.started', ts: '2026-06-05T10:00:05Z', payload: { name: 'valid_fixture__Add' } });
        const html = renderJobDetailHtml(JOB, { ...BASE_OPTS, run });
        assert.ok(html.includes('Accepted — 3 proof holes'));
        assert.ok(html.includes('<div class="grp" title="valid_fixture__Add">Add</div>'));
        assert.ok(html.includes('data-obligation="valid_fixture__Add"'));
        const empty = renderJobDetailHtml(JOB, BASE_OPTS);
        assert.ok(empty.includes('No activity yet.'));
    });

    it('tolerates a minimal list-shaped job (everything optional missing)', () => {
        const minimal: JobResponse = { id: 'abc', status: 'Queued' };
        const html = renderJobDetailHtml(minimal, BASE_OPTS);
        assert.ok(html.includes('abc'));
        assert.strictEqual(verdictOf(html).kind, 'queued');
        assert.ok(html.includes('Waiting for a worker'));
        assert.ok(!html.includes('role="progressbar"'));
        assert.ok(html.includes('Obligations appear once the worker has compiled the file.'));
    });

    it('shows neutral loading placeholders before the detail arrives', () => {
        const html = renderJobDetailHtml(JOB, { ...BASE_OPTS, detailLoaded: false });
        assert.ok(html.includes('Loading obligations…'));
        assert.ok(html.includes('aria-busy="true"'));
        assert.ok(!html.includes('No obligations'));
        assertNoProofClaim(html);

        const restored = renderJobDetailHtml({ id: 'abc', status: 'Accepted' }, {
            ...BASE_OPTS, detailLoaded: false, statusKnown: false,
        });
        assert.strictEqual(verdictOf(restored).kind, 'loading');
        assert.ok(restored.includes('Loading this proof job…'));
        assert.ok(!restored.includes('data-action="cancel"'));
    });

    it('shows the first compiler error inline with a fix hint and a link to the line', () => {
        const failed: JobResponse = {
            ...JOB,
            status: 'Failed',
            mode: 'compile-goals',
            filename: 'clamp.v',
            claimClass: 'Unverified',
            holesClosed: 0,
            errorCode: 'compile_failed',
            errorReason: 'file did not compile (see build.log)',
        };
        const result: JobResultResponse = {
            id: failed.id, status: 'Failed', holesTotal: 2, holesClosed: 0,
            artifacts: [
                { id: 'in', kind: 'InputV', filename: 'input.v', sizeBytes: 10, sha256: 'aa' },
                { id: 'log', kind: 'BuildLog', filename: 'build.log', sizeBytes: 10, sha256: 'bb' },
            ],
        };
        const html = renderJobDetailHtml(failed, {
            ...BASE_OPTS,
            result,
            source: 'clamp.inf',
            buildError: { line: 53, startChar: 11, endChar: 16, message: 'clamp already exists.' },
        });
        assert.deepStrictEqual(verdictOf(html), { kind: 'compile-error', tone: 'err', claim: 'none' });
        assert.ok(html.includes('The prover could not compile your file'));
        assert.ok(html.includes('<pre class="excerpt">Line 53: clamp already exists.</pre>'));
        assert.ok(html.includes('names the module after the file'));
        assert.ok(html.includes('data-action="gotoLine" data-artifact="in" data-line="53"'));
        assert.ok(html.includes('Open build log'));
        assert.ok(!html.includes('data-action="resubmit"'), 'running it again would fail the same way');
    });

    it('uses theme colours only, with focus, high-contrast and reduced-motion rules', () => {
        assert.ok(!/#fff\b|#1e1e1e/i.test(JOB_DETAIL_STYLES));
        assert.ok(JOB_DETAIL_STYLES.includes(':focus-visible'));
        assert.ok(JOB_DETAIL_STYLES.includes('body.vscode-high-contrast'));
        assert.ok(JOB_DETAIL_STYLES.includes('prefers-reduced-motion'));
        assert.ok(JOB_DETAIL_STYLES.includes('--vscode-testing-iconPassed'));
    });

    it('calls obligations proved only when the run claims a proof', () => {
        const verified = renderJobDetailHtml(JOB, { ...BASE_OPTS, result: RESULT });
        assert.ok(verified.includes('<span class="obl-status">Proved</span>'));
        const rejected = renderJobDetailHtml(JOB, {
            ...BASE_OPTS,
            result: { ...RESULT, verifiedClean: false, admitsDetected: true },
        });
        assert.ok(!rejected.includes('<span class="obl-status">Proved</span>'));
        assert.ok(rejected.includes('<span class="obl-status">Closed — not verified</span>'));
        const running = renderJobDetailHtml({ ...JOB, status: 'Running' }, { ...BASE_OPTS, live: 'sse' });
        assert.ok(running.includes('<span class="obl-status">Closed</span>'));
    });

    it('labels the verdict for assistive technology', () => {
        const html = renderJobDetailHtml(JOB, { ...BASE_OPTS, result: RESULT });
        assert.ok(/id="verdict"[^>]*role="status" aria-live="polite"/.test(html));
        assert.ok(html.includes('role="radiogroup" aria-label="Show activity"'));
        assert.ok(html.includes('aria-label="Filter activity"'));
        assert.ok(html.includes('<span class="sr-only">, done</span>'));
    });
});
