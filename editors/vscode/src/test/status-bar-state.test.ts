import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { determineStatusBarState } from '../ui/statusBarState';
import type { DoctorResult } from '../toolchain/doctor';

/**
 * Tests for status bar state determination logic.
 * Covers QA Section 3.
 */

function makeDoctorResult(overrides: Partial<DoctorResult>): DoctorResult {
    return {
        checks: overrides.checks ?? [],
        hasErrors: overrides.hasErrors ?? false,
        hasWarnings: overrides.hasWarnings ?? false,
        summary: overrides.summary ?? '',
    };
}

describe('determineStatusBarState (QA Section 3)', () => {
    it('null result shows dash icon and "not found" tooltip', () => {
        const state = determineStatusBarState(null);
        assert.strictEqual(state.icon, 'dash');
        assert.ok(state.tooltip.toLowerCase().includes('not found'));
        assert.strictEqual(state.background, 'none');
    });

    it('healthy result shows check icon and "healthy" tooltip', () => {
        const result = makeDoctorResult({ summary: 'All checks passed' });
        const state = determineStatusBarState(result);
        assert.strictEqual(state.icon, 'check');
        assert.ok(state.tooltip.toLowerCase().includes('healthy'));
        assert.strictEqual(state.background, 'none');
    });

    it('warnings show warning icon and warning background', () => {
        const result = makeDoctorResult({
            hasWarnings: true,
            summary: 'Some warnings found',
        });
        const state = determineStatusBarState(result);
        assert.strictEqual(state.icon, 'warning');
        assert.strictEqual(state.background, 'warning');
        assert.ok(state.tooltip.includes('Some warnings found'));
    });

    it('errors show error icon and error background', () => {
        const result = makeDoctorResult({
            hasErrors: true,
            summary: 'Critical failure',
        });
        const state = determineStatusBarState(result);
        assert.strictEqual(state.icon, 'error');
        assert.strictEqual(state.background, 'error');
        assert.ok(state.tooltip.includes('Critical failure'));
    });

    it('errors take priority over warnings', () => {
        const result = makeDoctorResult({
            hasErrors: true,
            hasWarnings: true,
            summary: 'Errors and warnings',
        });
        const state = determineStatusBarState(result);
        assert.strictEqual(state.icon, 'error');
        assert.strictEqual(state.background, 'error');
    });

    it('errors with empty summary use fallback text', () => {
        const result = makeDoctorResult({ hasErrors: true, summary: '' });
        const state = determineStatusBarState(result);
        assert.ok(state.tooltip.includes('Toolchain errors detected'));
    });

    it('warnings with empty summary use fallback text', () => {
        const result = makeDoctorResult({ hasWarnings: true, summary: '' });
        const state = determineStatusBarState(result);
        assert.ok(state.tooltip.includes('Toolchain warnings detected'));
    });
});
