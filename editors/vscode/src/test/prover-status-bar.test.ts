import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { proverStatusState } from '../prover/proverStatusBarState';
import type { DoctorResult } from '../toolchain/doctor';
import type { JobResponse } from '../prover/types';
import { determineStatusBarState, failingChecks, toolchainHealth } from '../ui/statusBarState';

const nameOf = (job: JobResponse) => (job.filename ?? job.id).replace(/\.v$/, '.inf');

describe('proverStatusState', () => {
    it('is hidden with nothing running and nothing finished', () => {
        assert.strictEqual(proverStatusState({ jobs: [{ id: 'a', status: 'Succeeded' }], problem: null, lastFinished: null, nameOf }).visible, false);
    });

    it('names the one running job with its progress', () => {
        const state = proverStatusState({
            jobs: [{ id: 'a', status: 'Running', filename: 'telemetry.v', holesTotal: 15, holesClosed: 3 }],
            problem: null, lastFinished: null, nameOf,
        });
        assert.deepStrictEqual([state.visible, state.icon, state.text, state.jobId], [true, 'running', 'telemetry 3/15', 'a']);
    });

    it('counts several running jobs and opens the view on click', () => {
        const state = proverStatusState({
            jobs: [{ id: 'a', status: 'Queued' }, { id: 'b', status: 'Verifying' }],
            problem: null, lastFinished: null, nameOf,
        });
        assert.deepStrictEqual([state.text, state.jobId], ['Proving 2', null]);
    });

    it('shows the last finished result, and server problems first', () => {
        const done: JobResponse = { id: 'a', status: 'Succeeded', mode: 'prove', claimClass: 'Verified', holesTotal: 1, holesClosed: 1, filename: 'b.v' };
        const state = proverStatusState({ jobs: [done], problem: null, lastFinished: done, nameOf });
        assert.deepStrictEqual([state.icon, state.text], ['pass', 'b']);
        assert.ok(state.tooltip.includes('Proved and independently verified'));
        const problem = proverStatusState({ jobs: [], problem: { kind: 'network', message: 'offline' }, lastFinished: done, nameOf });
        assert.deepStrictEqual([problem.icon, problem.text], ['server', 'Proof server']);
    });
});

describe('toolchainHealth', () => {
    const doctor = (checks: DoctorResult['checks']): DoctorResult => ({
        checks,
        hasErrors: checks.some((c) => c.status === 'fail'),
        hasWarnings: checks.some((c) => c.status === 'warn'),
        summary: 'Some checks failed.',
    });

    it('is degraded, not red, when only non-compiler checks fail', () => {
        const result = doctor([
            { name: 'Platform', status: 'fail', message: 'Unsupported platform: linux on aarch64' },
            { name: 'Resolved infc', status: 'ok', message: '/x/infc (source: sibling of infs)' },
        ]);
        assert.strictEqual(toolchainHealth(result), 'degraded');
        const state = determineStatusBarState(result);
        assert.deepStrictEqual([state.icon, state.background], ['warning', 'none']);
        assert.ok(state.tooltip.includes('the compiler works'));
        assert.deepStrictEqual(failingChecks(result), ['Platform — Unsupported platform: linux on aarch64']);
    });

    it('stays an error when no working compiler resolved', () => {
        const result = doctor([
            { name: 'Platform', status: 'fail', message: 'x' },
            { name: 'Resolved infc', status: 'fail', message: 'not found' },
        ]);
        assert.strictEqual(toolchainHealth(result), 'errors');
        assert.strictEqual(determineStatusBarState(result).background, 'error');
        assert.strictEqual(toolchainHealth(null), 'missing');
    });
});
