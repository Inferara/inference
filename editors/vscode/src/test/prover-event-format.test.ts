import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { formatEventLine } from '../prover/eventFormat';
import type { EventEnvelope } from '../prover/types';

function env(type: string, payload: unknown): EventEnvelope {
    return {
        schemaVersion: 1,
        jobId: 'j1',
        seq: 1,
        type,
        ts: '2026-06-06T10:20:30Z',
        phase: 'run',
        level: 'info',
        payload,
    };
}

describe('formatEventLine', () => {
    it('explains queue waiting and expiry', () => {
        assert.match(
            formatEventLine(env('job.queued', { deadlineUtc: '2026-06-06T10:30:00Z' })),
            /job\.queued — waiting for a worker$/,
        );
        assert.match(
            formatEventLine(env('job.errored', {
                errorCode: 'QUEUE_TIMEOUT',
                errorReason: 'job deadline expired while waiting for a worker',
            })),
            /QUEUE_TIMEOUT: job deadline expired while waiting for a worker$/,
        );
    });

    it('formats obligation lifecycle events', () => {
        assert.match(
            formatEventLine(
                env('obligation.proved', {
                    name: 'valid_main__Spec',
                    attempt: 2,
                    durationMs: 84211,
                }),
            ),
            /obligation\.proved — valid_main__Spec — attempt 2 — 1m 24s$/,
        );
        assert.match(
            formatEventLine(
                env('obligation.failed', {
                    name: 'valid_x',
                    error: 'coqc timeout',
                }),
            ),
            /obligation\.failed — valid_x — coqc timeout$/,
        );
        assert.match(
            formatEventLine(env('obligation.started', { name: 'valid_y' })),
            /obligation\.started — valid_y$/,
        );
    });

    it('formats discovery, acceptance and completion', () => {
        assert.match(
            formatEventLine(
                env('obligations.discovered', {
                    obligations: [{ name: 'a' }, { name: 'b' }],
                }),
            ),
            /obligations\.discovered — 2 obligation\(s\)$/,
        );
        assert.match(
            formatEventLine(
                env('job.accepted', { filename: 'main.v', holes: 3 }),
            ),
            /job\.accepted — main\.v, 3 holes$/,
        );
        assert.match(
            formatEventLine(env('job.completed', { outcome: 'partial' })),
            /job\.completed — partial$/,
        );
    });

    it('uses the message for log/setup events and nothing for heartbeats', () => {
        assert.match(
            formatEventLine(env('log', { message: 'building context' })),
            /log — building context$/,
        );
        assert.match(formatEventLine(env('heartbeat', {})), /heartbeat$/);
        assert.match(formatEventLine(env('usage.tick', { tokens: 5 })), /usage\.tick$/);
    });

    it('falls back to compact JSON for unknown types and tolerates junk payloads', () => {
        assert.match(
            formatEventLine(env('something.new', { a: 1 })),
            /something\.new — \{"a":1\}$/,
        );
        // Junk payloads must never throw.
        assert.ok(formatEventLine(env('log', null)));
        assert.ok(formatEventLine(env('obligation.proved', 'not-an-object')));
        assert.ok(formatEventLine(env('job.accepted', 42)));
    });

    it('caps very long lines', () => {
        const line = formatEventLine(
            env('log', { message: 'x'.repeat(10_000) }),
        );
        assert.ok(line.length <= 401, `line was ${line.length} chars`);
        assert.ok(line.endsWith('…'));
    });

    it('starts with a clock derived from the event timestamp', () => {
        const line = formatEventLine(env('log', { message: 'm' }));
        assert.match(line, /^\d{2}:\d{2}:\d{2} {2}log — m$/);
    });

    it('tolerates an unparseable timestamp', () => {
        const bad = { ...env('log', { message: 'm' }), ts: 'not-a-date' };
        assert.match(formatEventLine(bad), /^not-a-date {2}log — m$/);
    });
});
