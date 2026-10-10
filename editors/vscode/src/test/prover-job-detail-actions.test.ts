import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { buildJobView } from '../prover/jobDetailModel';
import { EVENT_LOG_CAP, renderJobDetailHtml, type RenderOptions } from '../prover/jobDetailHtml';
import { applyEvent, newRunModel } from '../prover/runModel';
import type { EventEnvelope, JobResponse, JobResultResponse } from '../prover/types';

const OPTS: RenderOptions = { nonce: 'n', cspSource: 'csp', live: 'terminal' };
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
const event = (seq: number, type: string, payload: unknown = {}): EventEnvelope => ({
    schemaVersion: 1, jobId: 'job-1', seq, type, ts: `2026-10-07T00:00:${String(seq).padStart(2, '0')}Z`, payload,
});

describe('run steps', () => {
    it('marks a live job done/active/todo', () => {
        const view = buildJobView({ job: { ...JOB, status: 'Running' }, live: 'sse' });
        assert.deepStrictEqual(view.steps.map((s) => [s.key, s.state]), [
            ['submitted', 'done'], ['worker', 'done'], ['proving', 'active'], ['verifying', 'todo'], ['result', 'todo'],
        ]);
    });

    it('does not claim steps a finished job never reached', () => {
        const view = buildJobView({ job: { ...JOB, status: 'ProvisionFailed' }, live: 'terminal' });
        assert.deepStrictEqual(view.steps.map((s) => s.state), ['done', 'unknown', 'unknown', 'unknown', 'final']);
        assert.deepStrictEqual(view.steps.at(-1), { key: 'result', label: "Couldn't start", state: 'final', tone: 'err' });
    });

    it('marks the steps the event history shows', () => {
        const run = newRunModel();
        for (const [seq, type] of [[1, 'job.accepted'], [2, 'vm.online'], [3, 'job.compiling'], [4, 'job.verifying'], [5, 'job.completed']] as const) {
            applyEvent(run, event(seq, type));
        }
        const view = buildJobView({ job: JOB, run, live: 'terminal' });
        assert.deepStrictEqual(view.steps.map((s) => s.state), ['done', 'done', 'done', 'done', 'final']);
        assert.strictEqual(view.clock.end, '2026-10-07T00:00:05Z');
    });
});

describe('job panel actions', () => {
    it('offers open/compare/certificate for a completed proof', () => {
        const html = renderJobDetailHtml(JOB, { ...OPTS, result: RESULT(['InputV', 'CompletedV', 'BuildLog']) });
        assert.ok(html.includes('Open proof'));
        assert.ok(html.includes('data-action="compare"'));
        assert.ok(html.includes('aria-label="Open the certificate in the portal"'));
        // Every text artifact opens read-only (3 files + the proof button).
        assert.strictEqual(html.match(/data-action="openArtifact"/g)?.length, 4);
    });

    it('offers the refutation and the portal page when there is no completed proof', () => {
        const html = renderJobDetailHtml(
            { ...JOB, status: 'Failed', errorCode: 'spec_refuted' },
            { ...OPTS, result: RESULT(['InputV', 'RefutationV']) },
        );
        assert.ok(html.includes('Open refutation'));
        assert.ok(html.includes('Specification refuted'));
        assert.ok(!html.includes('data-action="compare"'));
        assert.ok(html.includes('data-action="openPortal"'));
        assert.ok(!html.includes('data-action="openCertificate"'));
    });

    it('styles a refuted obligation distinctly', () => {
        const html = renderJobDetailHtml(JOB, OPTS);
        assert.ok(html.includes('class="obl tone-refuted"'));
        assert.ok(html.includes('data-status="refuted"'));
        assert.ok(html.includes('>Refuted</span>'));
    });

    it('shows the time budget, deadline and source, escaped', () => {
        const html = renderJobDetailHtml(JOB, { ...OPTS, source: '<x>.inf' });
        assert.ok(html.includes('data-field="budget">15m'));
        assert.ok(html.includes('data-field="deadline"'));
        assert.ok(html.includes('&lt;x&gt;.inf'));
        assert.ok(html.includes('Uploaded as <code>m.v</code>'));
    });

    it('prefers the deadline the server or the event history gives', () => {
        const fromJob = buildJobView({ job: { ...JOB, deadlineUtc: '2026-10-07T01:00:00Z' }, live: 'terminal' });
        assert.strictEqual(fromJob.clock.deadline, '2026-10-07T01:00:00Z');
        const run = newRunModel();
        applyEvent(run, event(1, 'job.queued', { deadlineUtc: '2026-10-07T02:00:00Z' }));
        assert.strictEqual(buildJobView({ job: JOB, run, live: 'terminal' }).clock.deadline, '2026-10-07T02:00:00Z');
        assert.strictEqual(buildJobView({ job: JOB, live: 'terminal' }).clock.deadline, '2026-10-07T00:15:00.000Z');
    });

    it('links an open obligation to its theorem in the submitted file, with the reported line as fallback', () => {
        const run = newRunModel();
        applyEvent(run, event(1, 'obligations.discovered', { obligations: [{ name: 't', sourceLine: 98 }] }));
        const [fromEvents] = buildJobView({ job: JOB, run, live: 'terminal' }).obligations ?? [];
        assert.deepStrictEqual(fromEvents.goto, { target: 'source', line: 98 });
        const html = renderJobDetailHtml(JOB, { ...OPTS, run });
        assert.ok(html.includes('data-action="gotoTheorem" data-obligation="t" data-line="98"'));
        assert.ok(html.includes('Go to theorem'));
        assert.ok(!html.includes('Go to line'));
        const [unknown] = buildJobView({ job: JOB, live: 'terminal' }).obligations ?? [];
        assert.deepStrictEqual(unknown.goto, { target: 'source' });
    });

    it('titles the closed group like its rows, never "Proved" without a claim', () => {
        const obligations = Array.from({ length: 11 }, (_, i) => ({ name: `t${i}`, kind: 'spec' as const, status: 'proved' as const }));
        const unverified = renderJobDetailHtml({ ...JOB, holesTotal: 11, holesClosed: 11, obligations }, { ...OPTS, resultState: 'failed' });
        assert.ok(unverified.includes('Closed — not verified (11)'));
        assert.ok(!unverified.includes('Proved (11)'));
        const running = renderJobDetailHtml({ ...JOB, status: 'Running', holesTotal: 12, holesClosed: 11, obligations }, { ...OPTS, live: 'sse' });
        assert.ok(running.includes('Closed (11)'));
    });

    it('links a proved obligation to its proof in the returned file', () => {
        const job: JobResponse = { ...JOB, obligations: [{ name: 't', kind: 'spec', status: 'proved', sourceLine: 7 }] };
        const result = RESULT(['InputV', 'CompletedV']);
        const [proved] = buildJobView({ job, result, live: 'terminal' }).obligations ?? [];
        assert.deepStrictEqual(proved.goto, { target: 'proof', artifact: 'a1' });
        const html = renderJobDetailHtml(job, { ...OPTS, result });
        assert.ok(html.includes('data-action="gotoTheorem" data-obligation="t" data-artifact="a1"'));
        assert.ok(html.includes('Go to proof'));
        // Without a returned file it points at the submitted one.
        const [noProof] = buildJobView({ job, result: RESULT(['InputV']), live: 'terminal' }).obligations ?? [];
        assert.deepStrictEqual(noProof.goto, { target: 'source', line: 7 });
    });

    it('keeps one nonce-bound script and the activity cap', () => {
        const html = renderJobDetailHtml(JOB, { ...OPTS, result: RESULT(['CompletedV']) });
        assert.strictEqual(html.match(/<script[\s>]/g)?.length, 1);
        assert.ok(html.includes(`const CAP = ${EVENT_LOG_CAP};`));
        assert.strictEqual(EVENT_LOG_CAP, 2000);
    });
});
