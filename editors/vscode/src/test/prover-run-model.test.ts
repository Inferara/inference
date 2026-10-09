import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { applyEvent, changesSummary, EVENT_LOG_CAP, newRunModel } from '../prover/runModel';
import type { EventEnvelope } from '../prover/types';

const ev = (seq: number, type: string, payload: unknown = {}, ts = `2026-10-09T10:00:${String(seq % 60).padStart(2, '0')}Z`): EventEnvelope => ({
    schemaVersion: 1, jobId: 'j', seq, type, ts, payload,
});

describe('run model', () => {
    it('ignores replays at or below the cursor', () => {
        const run = newRunModel();
        assert.strictEqual(applyEvent(run, ev(2, 'job.accepted')).applied, true);
        assert.strictEqual(applyEvent(run, ev(2, 'job.accepted')).applied, false);
        assert.strictEqual(applyEvent(run, ev(1, 'job.queued')).applied, false);
        assert.strictEqual(run.rows.length, 1);
    });

    it('drops heartbeats from the activity but keeps the last signal', () => {
        const run = newRunModel();
        const { row } = applyEvent(run, ev(1, 'heartbeat'));
        assert.strictEqual(row, null);
        assert.strictEqual(run.lastSignalAt, '2026-10-09T10:00:01Z');
        assert.strictEqual(changesSummary('heartbeat'), false);
        assert.strictEqual(changesSummary('obligation.proved'), true);
    });

    it('records steps, the deadline, source lines and how obligations were closed', () => {
        const run = newRunModel();
        applyEvent(run, ev(1, 'job.accepted', { holes: 2 }));
        applyEvent(run, ev(2, 'job.queued', { deadlineUtc: '2026-10-09T10:30:00Z' }));
        applyEvent(run, ev(3, 'vm.online'));
        applyEvent(run, ev(4, 'obligations.discovered', { obligations: [{ name: 'a', sourceLine: 10 }, { name: 'b', sourceLine: 20 }] }));
        applyEvent(run, ev(5, 'obligation.proved', { name: 'a', attempt: 0, by: 'template' }));
        applyEvent(run, ev(6, 'obligation.proved', { name: 'b', attempt: 2 }));
        applyEvent(run, ev(7, 'job.verifying'));
        applyEvent(run, ev(8, 'job.completed', { outcome: 'Succeeded' }));
        assert.strictEqual(run.deadlineUtc, '2026-10-09T10:30:00Z');
        assert.deepStrictEqual(run.sourceLines, { a: 10, b: 20 });
        assert.deepStrictEqual(run.provedBy, { a: 'template', b: 'agent' });
        assert.strictEqual(run.attempts.b, 2);
        assert.ok(run.finished.worker && run.finished.proving && run.finished.verifying);
        assert.strictEqual(run.endedAt, '2026-10-09T10:00:08Z');
        assert.deepStrictEqual(run.rows.map((r) => r.text), [
            'Accepted — 2 proof holes',
            'Waiting for a worker',
            'Worker ready',
            'Found 2 proof obligations',
            'Proved by a built-in template',
            'Proved (attempt 2)',
            'Independently verifying the returned proof',
            'Run finished',
        ]);
    });

    it('files agent activity under its obligation and attempt', () => {
        const run = newRunModel();
        const { row } = applyEvent(run, ev(1, 'agent.activity', {
            schemaVersion: 1, agentRef: 'claude-code', attempt: 2, theorem: 'valid_m__S', kind: 'thinking', detail: 'try induction', usage: {},
        }));
        assert.deepStrictEqual(row && { cat: row.cat, obligation: row.obligation, attempt: row.attempt, text: row.text }, {
            cat: 'agent', obligation: 'valid_m__S', attempt: 2, text: 'thinking — try induction',
        });
    });

    it('words errors and warnings without raw codes', () => {
        const run = newRunModel();
        assert.strictEqual(applyEvent(run, ev(1, 'job.errored', { errorCode: 'QUEUE_TIMEOUT' })).row?.text,
            'No worker became free before the time budget ran out.');
        assert.strictEqual(applyEvent(run, ev(2, 'obligation.failed', { name: 'a', error: 'timeout' })).row?.cat, 'warn');
        applyEvent(run, ev(3, 'integrity.warning', { detail: 'statement changed' }));
        assert.deepStrictEqual(run.integrityWarnings, ['statement changed']);
    });

    it('keeps at most EVENT_LOG_CAP rows, newest last', () => {
        const run = newRunModel();
        for (let i = 1; i <= EVENT_LOG_CAP + 5; i++) {
            applyEvent(run, ev(i, 'log', { message: `line ${i}` }));
        }
        assert.strictEqual(run.rows.length, EVENT_LOG_CAP);
        assert.strictEqual(run.rows.at(-1)?.text, `line ${EVENT_LOG_CAP + 5}`);
        assert.strictEqual(run.rows[0].text, 'line 6');
    });
});
