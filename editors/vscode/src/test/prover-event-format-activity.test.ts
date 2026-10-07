import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { formatEventLine } from '../prover/eventFormat';
import type { EventEnvelope } from '../prover/types';

const env = (type: string, payload: unknown): EventEnvelope => ({
    schemaVersion: 1, jobId: 'j', seq: 1, type, ts: '2026-10-07T10:20:30Z', payload,
});
const activity = (kind: string, detail: string, extra: Record<string, unknown> = {}) =>
    formatEventLine(env('agent.activity', { schemaVersion: 1, agentRef: 'codex-cli', attempt: 2,
        theorem: 'valid_m__Spec', kind, detail, usage: {}, ...extra }));

describe('agent activity lines', () => {
    it('reads like a transcript, scoped to the theorem and attempt', () => {
        assert.match(activity('text', 'Trying induction.'), /agent \[valid_m__Spec #2\] Trying induction\.$/);
        assert.match(activity('thinking', 'consider n'), /thinking — consider n$/);
        assert.match(activity('tool:bash', 'coqc m.v'), /bash: coqc m\.v$/);
        assert.match(activity('tool_result', ''), /result: \(empty\)$/);
        assert.match(activity('system/init', 'model ready'), /init — model ready$/);
    });

    it('summarizes a finished attempt with its reported cost', () => {
        assert.match(activity('result', '', { usage: { reportedCostUsd: 0.0123 } }), /attempt finished — \$0\.012$/);
        assert.match(activity('result is_error=true', ''), /cost unknown \(agent error\)$/);
    });

    it('folds whitespace and clips long details instead of dumping JSON', () => {
        const line = activity('text', `${'word '.repeat(200)}\n\n tail`);
        assert.ok(!line.includes('{'));
        assert.ok(line.endsWith('…'));
        assert.ok(!line.includes('\n'));
    });
});

describe('refutation, limits and phases', () => {
    it('formats refutations and server-side agent limits', () => {
        assert.match(formatEventLine(env('obligation.refuted', { name: 't' })),
            /obligation\.refuted — t — machine-checked refutation accepted$/);
        assert.match(formatEventLine(env('agent.limited', { code: 'AGENT_REQUEST_LIMIT' })),
            /Agent request limit reached/);
    });

    it('never shows deployment-internal routing on phase events', () => {
        const line = formatEventLine(env('job.provisioning', { provider: 'ec2', instanceId: 'i-0abc' }));
        assert.match(line, /job\.provisioning$/);
        assert.ok(!line.includes('ec2') && !line.includes('i-0abc'));
    });
});
