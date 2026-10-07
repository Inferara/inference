import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { FILTER_STATUSES, jobContextValue, mergeFirstPage } from '../prover/jobList';
import type { JobResponse, JobStatus } from '../prover/types';

const job = (id: string, createdAt: string, status: JobStatus = 'Succeeded'): JobResponse => ({
    id,
    status,
    createdAt,
});

describe('jobContextValue', () => {
    it('separates cancelable, terminal and proof-carrying jobs', () => {
        assert.strictEqual(jobContextValue({ status: 'Running' }), 'inference.proofJob.cancelable');
        assert.strictEqual(jobContextValue({ status: 'Verifying' }), 'inference.proofJob');
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
