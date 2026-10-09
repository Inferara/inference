import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    describeJob,
    FILTER_STATUSES,
    finishedJobs,
    hasCertificate,
    isActiveJob,
    jobContextValue,
    mergeFirstPage,
    pollDelay,
    relativeTime,
} from '../prover/jobList';
import type { JobResponse, JobStatus } from '../prover/types';

const job = (id: string, createdAt: string, status: JobStatus = 'Succeeded'): JobResponse => ({
    id,
    status,
    createdAt,
});

describe('jobContextValue', () => {
    it('separates cancelable, terminal and proof-carrying jobs', () => {
        assert.strictEqual(jobContextValue({ status: 'Running' }), 'inference.proofJob.cancelable');
        assert.strictEqual(jobContextValue({ status: 'Verifying' }), 'inference.proofJob.active');
        assert.strictEqual(jobContextValue({ status: 'Lost' }), 'inference.proofJob.lost');
        assert.strictEqual(jobContextValue({ status: 'Failed' }), 'inference.proofJob.terminal');
        assert.strictEqual(jobContextValue({ status: 'Succeeded' }), 'inference.proofJob.terminal.proof');
        assert.strictEqual(jobContextValue({ status: 'PartialSuccess' }), 'inference.proofJob.terminal.proof');
    });

    it('covers every status the filter offers', () => {
        assert.strictEqual(FILTER_STATUSES.length, 15);
        for (const status of FILTER_STATUSES) {
            assert.match(jobContextValue({ status }), /^inference\.proofJob/);
        }
    });
});

describe('mergeFirstPage', () => {
    const page = [job('c', '2026-10-07T03:00:00Z'), job('b', '2026-10-07T02:00:00Z')];

    it('a short first page is the whole list', () => {
        assert.deepStrictEqual(mergeFirstPage(page, [job('old', '2026-10-01T00:00:00Z')], 3), page);
    });

    it('a full first page keeps older loaded jobs and drops replaced ones', () => {
        const loaded = [
            job('b', '2026-10-07T02:00:00Z', 'Running'),
            job('a', '2026-10-07T01:00:00Z'),
        ];
        assert.deepStrictEqual(
            mergeFirstPage(page, loaded, 2).map((j) => `${j.id}:${j.status}`),
            ['c:Succeeded', 'b:Succeeded', 'a:Succeeded'],
        );
    });
});

describe('relativeTime and describeJob', () => {
    const now = Date.parse('2026-10-09T12:00:00Z');
    it('words recent times', () => {
        assert.strictEqual(relativeTime('2026-10-09T11:59:40Z', now), 'just now');
        assert.strictEqual(relativeTime('2026-10-09T11:56:00Z', now), '4m ago');
        assert.strictEqual(relativeTime('2026-10-09T10:00:00Z', now), '2h ago');
        assert.strictEqual(relativeTime('2026-10-08T12:00:00Z', now), 'yesterday');
        assert.strictEqual(relativeTime('2026-10-06T12:00:00Z', now), '3d ago');
        assert.strictEqual(relativeTime('nonsense', now), '');
    });

    it('describes a row with its verdict, counts and time', () => {
        assert.strictEqual(
            describeJob({ id: 'a', status: 'Succeeded', mode: 'prove', claimClass: 'Verified', holesTotal: 15, holesClosed: 15, createdAt: '2026-10-09T10:00:00Z' }, now),
            'Verified · 15/15 · 2h ago',
        );
        assert.strictEqual(
            describeJob({ id: 'b', status: 'Failed', errorCode: 'compile_failed', createdAt: '2026-10-08T12:00:00Z' }, now),
            'Compile error · yesterday',
        );
    });
});

describe('pollDelay', () => {
    const base = { active: false, visible: true, msSinceSubmit: null, errors: 0 };
    it('polls fast after a submit, steadily while jobs move, slowly when idle', () => {
        assert.strictEqual(pollDelay({ ...base, active: true, msSinceSubmit: 30_000 }), 5_000);
        assert.strictEqual(pollDelay({ ...base, active: true }), 10_000);
        assert.strictEqual(pollDelay({ ...base, active: true, visible: false }), 10_000);
        assert.strictEqual(pollDelay(base), 60_000);
        assert.strictEqual(pollDelay({ ...base, visible: false }), null);
    });

    it('backs off on errors, capped at five minutes', () => {
        assert.strictEqual(pollDelay({ ...base, errors: 1 }), 10_000);
        assert.strictEqual(pollDelay({ ...base, errors: 3 }), 40_000);
        assert.strictEqual(pollDelay({ ...base, errors: 20 }), 300_000);
    });
});

describe('finishedJobs', () => {
    it('reports watched jobs that went from moving to finished', () => {
        const previous = new Map([['a', 'Running'], ['b', 'Running'], ['c', 'Succeeded']] as const);
        const next: JobResponse[] = [
            { id: 'a', status: 'Succeeded' },
            { id: 'b', status: 'Running' },
            { id: 'c', status: 'Succeeded' },
            { id: 'd', status: 'Failed' },
        ];
        assert.deepStrictEqual(finishedJobs(previous, next, new Set(['a', 'b', 'c', 'd'])).map((j) => j.id), ['a']);
        assert.deepStrictEqual(finishedJobs(previous, next, new Set(['b'])).map((j) => j.id), []);
    });

    it('treats Lost as still moving (the server re-queues it)', () => {
        const previous = new Map([['a', 'Running']] as const);
        assert.deepStrictEqual(finishedJobs(previous, [{ id: 'a', status: 'Lost' }], new Set(['a'])), []);
        assert.ok(isActiveJob({ status: 'Lost' }));
        assert.ok(hasCertificate({ status: 'CompileGoals' }));
        assert.ok(!hasCertificate({ status: 'Failed' }));
    });
});
