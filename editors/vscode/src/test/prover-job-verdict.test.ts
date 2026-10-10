import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { isProvedSuccess } from '../prover/jobPresentation';
import { jobVerdict, listVerdict } from '../prover/jobVerdict';
import type { ClaimClass, JobResponse, JobResultResponse, JobStatus, RunMode } from '../prover/types';
import { resultMatchesJob, verifierAcceptedFull } from '../prover/verifierEvidence';

const JOB: JobResponse = {
    id: 'j', status: 'Succeeded', mode: 'prove', claimClass: 'Verified', holesTotal: 1, holesClosed: 1,
    obligations: [{ name: 'valid_m__S', kind: 'spec', content: 'grounded', status: 'proved' }],
};
const RESULT: JobResultResponse = {
    id: 'j', status: 'Succeeded', mode: 'prove', claimClass: 'Verified', holesTotal: 1, holesClosed: 1,
    verificationOutcome: 'succeeded', compileOk: true, assumptionsClosed: true, verifiedClean: true,
    admitsDetected: false, statementsImmutable: true, markersRemaining: 0,
    assumptionPolicyId: 'p', assumptionReports: [{ target: 'valid_m__S', assumptions: [], kernelOutput: 'Closed', policyPassed: true }],
    artifacts: [],
};

describe('listVerdict', () => {
    const cases: Array<[Partial<JobResponse>, string, string]> = [
        [{ status: 'Queued' }, 'queued', 'Queued'],
        [{ status: 'Booting' }, 'starting', 'Starting'],
        [{ status: 'Running' }, 'proving', 'Proving'],
        [{ status: 'Verifying' }, 'verifying', 'Verifying'],
        [{ status: 'Canceling' }, 'canceling', 'Canceling…'],
        [{}, 'verified', 'Verified'],
        [{ claimClass: 'StructuralOnly' }, 'structural', 'Structural only'],
        [{ claimClass: 'Unverified' }, 'unverified', 'Not verified'],
        [{ mode: 'compile-goals' }, 'compile-only', 'Compiled only'],
        [{ status: 'CompileGoals', mode: 'compile-goals' }, 'compile-only', 'Compiled only'],
        [{ status: 'PartialSuccess', claimClass: 'PartialVerified', holesTotal: 2, holesClosed: 1 }, 'partial', 'Partially proved'],
        [{ status: 'Failed', errorCode: 'compile_failed' }, 'compile-error', 'Compile error'],
        [{ status: 'Failed', errorCode: 'spec_refuted' }, 'refuted', 'Refuted'],
        [{ status: 'Failed', errorCode: 'harness_error' }, 'failed', 'Failed'],
        [{ status: 'TimedOut' }, 'timed-out', 'Timed out'],
        [{ status: 'ProvisionFailed' }, 'provision-failed', "Couldn't start"],
        [{ status: 'Lost' }, 'lost', 'Worker lost'],
        [{ status: 'Canceled' }, 'canceled', 'Canceled'],
    ];
    for (const [patch, kind, badge] of cases) {
        it(`${patch.status ?? 'Succeeded'} ${patch.claimClass ?? ''} ${patch.errorCode ?? ''} → ${kind}`, () => {
            const v = listVerdict({ ...JOB, ...patch });
            assert.strictEqual(v.kind, kind);
            assert.strictEqual(v.badge, badge);
        });
    }

    it('words the harness error without its code', () => {
        const v = listVerdict({ ...JOB, status: 'Failed', errorCode: 'harness_error', errorReason: 'x' });
        assert.ok(v.body.includes('not a problem with your file'));
        assert.ok(!v.body.includes('harness_error'));
    });
});

describe('jobVerdict claim rules', () => {
    it('claims a proof only with a matching, fully accepted verifier result', () => {
        assert.strictEqual(jobVerdict({ job: JOB, result: RESULT }).claim, 'proved');
        assert.strictEqual(jobVerdict({ job: JOB }).kind, 'loading');
        assert.strictEqual(jobVerdict({ job: JOB, resultState: 'failed' }).claim, 'none');
    });

    it('claims structural validity only with a matching, fully accepted verifier result', () => {
        const job: JobResponse = { ...JOB, claimClass: 'StructuralOnly', obligations: [{ name: 'valid_m', kind: 'module', status: 'proved' }] };
        const result: JobResultResponse = {
            ...RESULT, claimClass: 'StructuralOnly',
            assumptionReports: [{ target: 'valid_m', assumptions: [], kernelOutput: 'Closed', policyPassed: true }],
        };
        const accepted = jobVerdict({ job, result });
        assert.strictEqual(accepted.claim, 'structural');
        assert.ok(accepted.body.includes('independently re-verified'));
        assert.strictEqual(jobVerdict({ job }).kind, 'loading');
        assert.strictEqual(jobVerdict({ job, resultState: 'failed' }).claim, 'none');
        assert.strictEqual(jobVerdict({ job, result: { ...result, verificationOutcome: null } }).claim, 'none');
        assert.strictEqual(jobVerdict({ job, result: { ...result, compileOk: false } }).claim, 'none');
        assert.strictEqual(jobVerdict({ job, result: { ...result, claimClass: 'Verified' } }).claim, 'none');
    });

    it('never shows the success tone without the full evidence (exhaustive over key fields)', () => {
        const statuses: JobStatus[] = ['Succeeded', 'PartialSuccess', 'CompileGoals', 'Failed'];
        const modes: Array<RunMode | null> = ['prove', 'compile-goals', 'unknown', null];
        const claims: Array<ClaimClass | null> = ['Verified', 'StructuralOnly', 'PartialVerified', 'Unverified', null];
        const outcomes = ['succeeded', 'partial', null];
        const flagSets: Array<Partial<JobResultResponse>> = [
            {},
            { compileOk: false },
            { verifiedClean: false, admitsDetected: true },
            { statementsImmutable: false },
            { markersRemaining: 1 },
            { assumptionsClosed: false },
            { assumptionReports: [] },
        ];
        for (const status of statuses) {
            for (const mode of modes) {
                for (const claimClass of claims) {
                    for (const verificationOutcome of outcomes) {
                        for (const flags of flagSets) {
                            for (const closed of [0, 1]) {
                                const job: JobResponse = { ...JOB, status, mode, claimClass, holesClosed: closed };
                                const result: JobResultResponse = {
                                    ...RESULT, ...flags, status, mode, claimClass, verificationOutcome, holesClosed: closed,
                                };
                                const v = jobVerdict({ job, result });
                                const fullyBacked =
                                    isProvedSuccess(job.status, job.mode, job.holesTotal, job.holesClosed, job.claimClass) &&
                                    resultMatchesJob(job, result) &&
                                    verifierAcceptedFull(result, ['valid_m__S']);
                                if (v.tone === 'ok' || v.claim === 'proved') {
                                    assert.ok(fullyBacked, JSON.stringify({ status, mode, claimClass, verificationOutcome, flags, closed }));
                                }
                                if (v.claim === 'structural') {
                                    const structurallyBacked =
                                        isProvedSuccess(job.status, job.mode, job.holesTotal, job.holesClosed, 'Verified') &&
                                        job.claimClass === 'StructuralOnly' &&
                                        resultMatchesJob(job, result) &&
                                        verifierAcceptedFull(result, ['valid_m__S']);
                                    assert.ok(structurallyBacked, JSON.stringify({ status, mode, claimClass, verificationOutcome, flags, closed }));
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    it('explains an empty or tautological specification next to a proof', () => {
        const job: JobResponse = {
            ...JOB,
            holesTotal: 2,
            holesClosed: 2,
            obligations: [
                { name: 'valid_m__S', kind: 'spec', content: 'grounded', status: 'proved' },
                { name: 'valid_m__T', kind: 'spec', content: 'tautology', status: 'proved' },
            ],
        };
        const result: JobResultResponse = {
            ...RESULT, holesTotal: 2, holesClosed: 2,
            assumptionReports: [
                { target: 'valid_m__S', assumptions: [], kernelOutput: 'Closed', policyPassed: true },
                { target: 'valid_m__T', assumptions: [], kernelOutput: 'Closed', policyPassed: true },
            ],
        };
        const v = jobVerdict({ job, result });
        assert.strictEqual(v.kind, 'verified');
        assert.ok(v.caveats.some((c) => c.includes('tautology')));
        assert.ok(v.body.includes('1 states a behavioral property'));
    });

    it('turns a compile failure into the first compiler error with a fix hint', () => {
        const v = jobVerdict({
            job: { ...JOB, status: 'Failed', errorCode: 'compile_failed', claimClass: 'Unverified' },
            buildError: { line: 53, message: 'clamp already exists.' },
            fileStems: ['clamp'],
        });
        assert.strictEqual(v.kind, 'compile-error');
        assert.strictEqual(v.body, 'Line 53: clamp already exists.');
        assert.ok(v.caveats[0].includes('names the module after the file'));
    });

    it('explains why Canceling can take a while', () => {
        assert.ok(listVerdict({ ...JOB, status: 'Canceling' }).body.includes('about 90 seconds'));
    });
});
