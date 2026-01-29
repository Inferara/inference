import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { parseDoctorOutput } from '../toolchain/doctor';

describe('parseDoctorOutput', () => {
    it('parses all-OK output', () => {
        const stdout = [
            'Checking Inference toolchain installation...',
            '',
            '  [OK] infs binary: Found at /home/user/.inference/bin/infs',
            '  [OK] Platform: Detected linux-x64',
            '  [OK] Toolchain directory: Found at /home/user/.inference',
            '  [OK] Default toolchain: Set to 0.1.0',
            '  [OK] inf-llc: Found inf-llc in PATH',
            '  [OK] rust-lld: Found rust-lld in PATH',
            '',
            'All checks passed. The toolchain is ready to use.',
        ].join('\n');

        const result = parseDoctorOutput(stdout);

        assert.strictEqual(result.checks.length, 6);
        assert.strictEqual(result.hasErrors, false);
        assert.strictEqual(result.hasWarnings, false);
        assert.strictEqual(
            result.summary,
            'All checks passed. The toolchain is ready to use.',
        );

        for (const check of result.checks) {
            assert.strictEqual(check.status, 'ok');
        }

        assert.strictEqual(result.checks[0].name, 'infs binary');
        assert.strictEqual(
            result.checks[0].message,
            'Found at /home/user/.inference/bin/infs',
        );
    });

    it('parses output with warnings', () => {
        const stdout = [
            'Checking Inference toolchain installation...',
            '',
            '  [OK] infs binary: Found at /home/user/.inference/bin/infs',
            '  [OK] Platform: Detected linux-x64',
            '  [WARN] libLLVM: Not found in /path. Some features may not work.',
            '',
            'Some warnings were found. The toolchain may work but could have issues.',
        ].join('\n');

        const result = parseDoctorOutput(stdout);

        assert.strictEqual(result.checks.length, 3);
        assert.strictEqual(result.hasErrors, false);
        assert.strictEqual(result.hasWarnings, true);
        assert.strictEqual(result.checks[2].status, 'warn');
        assert.strictEqual(result.checks[2].name, 'libLLVM');
        assert.strictEqual(
            result.summary,
            'Some warnings were found. The toolchain may work but could have issues.',
        );
    });

    it('parses output with failures', () => {
        const stdout = [
            'Checking Inference toolchain installation...',
            '',
            '  [FAIL] infs binary: Not found',
            '  [FAIL] Toolchain directory: Not found at /home/user/.inference',
            '',
            "Some checks failed. Run 'infs install' to install the toolchain.",
        ].join('\n');

        const result = parseDoctorOutput(stdout);

        assert.strictEqual(result.checks.length, 2);
        assert.strictEqual(result.hasErrors, true);
        assert.strictEqual(result.hasWarnings, false);

        for (const check of result.checks) {
            assert.strictEqual(check.status, 'fail');
        }
    });

    it('parses mixed statuses', () => {
        const stdout = [
            'Checking Inference toolchain installation...',
            '',
            '  [OK] infs binary: Found at /usr/local/bin/infs',
            '  [WARN] Default toolchain: No default toolchain set.',
            '  [FAIL] inf-llc: Not found.',
            '',
            "Some checks failed. Run 'infs install' to install the toolchain.",
        ].join('\n');

        const result = parseDoctorOutput(stdout);

        assert.strictEqual(result.checks.length, 3);
        assert.strictEqual(result.hasErrors, true);
        assert.strictEqual(result.hasWarnings, true);
        assert.strictEqual(result.checks[0].status, 'ok');
        assert.strictEqual(result.checks[1].status, 'warn');
        assert.strictEqual(result.checks[2].status, 'fail');
    });

    it('handles empty output', () => {
        const result = parseDoctorOutput('');

        assert.strictEqual(result.checks.length, 0);
        assert.strictEqual(result.hasErrors, false);
        assert.strictEqual(result.hasWarnings, false);
        assert.strictEqual(result.summary, '');
    });

    it('ignores PATH conflict continuation lines', () => {
        const stdout = [
            'Checking Inference toolchain installation...',
            '',
            '  [OK] infs binary: Found at /usr/local/bin/infs',
            '',
            '  [WARN] PATH conflict detected:',
            '         /usr/local/bin/infs shadows /home/user/.inference/bin/infs',
            '',
            'Some warnings were found. The toolchain may work but could have issues.',
        ].join('\n');

        const result = parseDoctorOutput(stdout);

        assert.strictEqual(result.checks.length, 1);
        assert.strictEqual(result.checks[0].name, 'infs binary');
    });

    it('extracts summary from last non-check line', () => {
        const stdout = [
            'Checking Inference toolchain installation...',
            '',
            '  [OK] Platform: Detected linux-x64',
            '',
            'All checks passed. The toolchain is ready to use.',
            '',
        ].join('\n');

        const result = parseDoctorOutput(stdout);

        assert.strictEqual(
            result.summary,
            'All checks passed. The toolchain is ready to use.',
        );
    });
});
