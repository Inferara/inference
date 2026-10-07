import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { EVENT_LOG_CAP, phaseRibbon, renderJobDetailHtml, type RenderOptions } from '../prover/jobDetailHtml';
import type { JobResponse, JobResultResponse } from '../prover/types';

const OPTS: RenderOptions = { nonce: 'n', cspSource: 'csp', live: 'terminal', eventLog: [], canCancel: false };
const JOB: JobResponse = {
    id: 'job-1', status: 'Succeeded', filename: 'm.v', createdAt: '2026-10-07T00:00:00Z',
    maxWallClockSeconds: 900, holesTotal: 1, holesClosed: 1, mode: 'prove', claimClass: 'Verified',
    obligations: [{ name: 't', kind: 'spec', status: 'refuted' }],
};
const artifact = (id: string, kind: string) => ({ id, kind, filename: `${id}.v`, sizeBytes: 1, sha256: 'x' });
const RESULT = (kinds: string[]): JobResultResponse => ({
    id: 'job-1', status: 'Succeeded', holesTotal: 1, holesClosed: 1,
    artifacts: kinds.map((k, i) => artifact(`a${i}`, k)),
});

describe('phaseRibbon', () => {
    it('marks a live job done/active/todo', () => {
        assert.deepStrictEqual(phaseRibbon('Running', false).map((c) => c.state),
            ['done', 'done', 'done', 'done', 'active', 'todo', 'todo']);
    });

    it('does not claim phases a finished job may never have reached', () => {
        const chips = phaseRibbon('ProvisionFailed', false);
        assert.ok(chips.slice(0, -1).every((c) => c.state === 'past'));
        assert.deepStrictEqual(chips.at(-1), { name: 'ProvisionFailed', state: 'err' });
        assert.deepStrictEqual(phaseRibbon('Succeeded', true).at(-1), { name: 'Succeeded', state: 'ok' });
        assert.deepStrictEqual(phaseRibbon('Succeeded', false).at(-1), { name: 'Succeeded', state: 'warn' });
    });
});

describe('job detail actions', () => {
    it('offers open/compare/certificate for a completed proof', () => {
        const html = renderJobDetailHtml(JOB, { ...OPTS, result: RESULT(['InputV', 'CompletedV', 'BuildLog']) });
        assert.ok(html.includes('Open completed proof'));
        assert.ok(html.includes('id="compare"'));
        assert.ok(html.includes('Open certificate in portal'));
        // Every text artifact can open read-only; binary ones only download.
        assert.strictEqual(html.match(/class="open-artifact"/g)?.length, 4);
    });

    it('offers the refutation and no comparison when there is no completed proof', () => {
        const html = renderJobDetailHtml({ ...JOB, status: 'Failed' }, { ...OPTS, result: RESULT(['InputV', 'RefutationV']) });
        assert.ok(html.includes('Open refutation'));
        assert.ok(!html.includes('id="compare"'));
    });

    it('styles a refuted obligation as a failure', () => {
        assert.ok(renderJobDetailHtml(JOB, OPTS).includes('<span class="badge err">refuted</span>'));
    });

    it('shows the time budget, deadline and source, escaped', () => {
        const html = renderJobDetailHtml(JOB, { ...OPTS, source: '<x>.inf' });
        assert.ok(html.includes('15m'));
        assert.ok(/Deadline/.test(html));
        assert.ok(html.includes('&lt;x&gt;.inf'));
    });

    it('keeps one nonce-bound script and the larger log cap', () => {
        const html = renderJobDetailHtml(JOB, { ...OPTS, result: RESULT(['CompletedV']) });
        assert.strictEqual(html.match(/<script[\s>]/g)?.length, 1);
        assert.ok(html.includes(`MAX_LOG_NODES = ${EVENT_LOG_CAP}`));
        assert.strictEqual(EVENT_LOG_CAP, 2000);
    });
});
