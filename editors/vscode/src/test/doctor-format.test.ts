import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { formatDoctorChecks } from '../toolchain/doctorFormat';
import type { DoctorResult } from '../toolchain/doctor';

/**
 * Tests for doctor output formatting.
 * Covers QA Section 4.2.
 */

function makeDoctorResult(overrides: Partial<DoctorResult>): DoctorResult {
    return {
        checks: overrides.checks ?? [],
        hasErrors: overrides.hasErrors ?? false,
        hasWarnings: overrides.hasWarnings ?? false,
        summary: overrides.summary ?? '',
    };
}

describe('formatDoctorChecks (QA Section 4.2)', () => {
    it('formats checks with [OK], [WARN], [FAIL] tags', () => {
        const result = makeDoctorResult({
            checks: [
                { name: 'infs binary', status: 'ok', message: 'Found at /usr/bin/infs' },
                { name: 'infc', status: 'warn', message: 'Not found' },
                { name: 'Default toolchain', status: 'fail', message: 'Not installed' },
            ],
            summary: 'Some checks failed.',
        });

        const lines = formatDoctorChecks(result);

        assert.ok(lines.some((l) => l.includes('[OK]') && l.includes('infs binary')));
        assert.ok(lines.some((l) => l.includes('[WARN]') && l.includes('infc')));
        assert.ok(lines.some((l) => l.includes('[FAIL]') && l.includes('Default toolchain')));
    });

    it('includes summary line', () => {
        const result = makeDoctorResult({
            checks: [
                { name: 'Platform', status: 'ok', message: 'linux-x64' },
            ],
            summary: 'All checks passed.',
        });

        const lines = formatDoctorChecks(result);
        assert.ok(lines.includes('All checks passed.'));
    });

    it('wraps output with separator lines', () => {
        const result = makeDoctorResult({
            checks: [
                { name: 'Platform', status: 'ok', message: 'linux-x64' },
            ],
        });

        const lines = formatDoctorChecks(result);
        assert.strictEqual(lines[0], '--- Doctor Report ---');
        assert.strictEqual(lines[lines.length - 1], '---------------------');
    });

    it('empty checks list produces just separator lines', () => {
        const result = makeDoctorResult({ checks: [], summary: '' });
        const lines = formatDoctorChecks(result);
        assert.strictEqual(lines.length, 2);
        assert.strictEqual(lines[0], '--- Doctor Report ---');
        assert.strictEqual(lines[1], '---------------------');
    });

    it('omits blank line and summary when summary is empty', () => {
        const result = makeDoctorResult({
            checks: [
                { name: 'test', status: 'ok', message: 'passed' },
            ],
            summary: '',
        });

        const lines = formatDoctorChecks(result);
        // Should be: separator, check line, separator (no blank line)
        assert.strictEqual(lines.length, 3);
    });
});
