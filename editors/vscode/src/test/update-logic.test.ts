import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { checkUpdateAvailable } from '../toolchain/updateCheck';
import type { VersionInfo } from '../toolchain/versions';

/**
 * Tests for update check logic.
 * Covers QA Section 4.3.
 */

function makeVersion(overrides: Partial<VersionInfo>): VersionInfo {
    return {
        version: overrides.version ?? '0.0.1',
        stable: overrides.stable ?? false,
        platforms: overrides.platforms ?? ['linux'],
        available_for_current: overrides.available_for_current ?? true,
    };
}

describe('checkUpdateAvailable (QA Section 4.3)', () => {
    it('already on latest returns up-to-date (QA 4.3.1)', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0' }),
            makeVersion({ version: '0.9.0' }),
        ];

        const result = checkUpdateAvailable('1.0.0', versions);
        assert.strictEqual(result.status, 'up-to-date');
        if (result.status === 'up-to-date') {
            assert.strictEqual(result.version, '1.0.0');
        }
    });

    it('older version returns update-available with correct versions (QA 4.3.2)', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '2.0.0' }),
            makeVersion({ version: '1.0.0' }),
        ];

        const result = checkUpdateAvailable('1.0.0', versions);
        assert.strictEqual(result.status, 'update-available');
        if (result.status === 'update-available') {
            assert.strictEqual(result.current, '1.0.0');
            assert.strictEqual(result.latest, '2.0.0');
        }
    });

    it('no current version returns no-current-version (QA 4.3.5)', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0' }),
        ];

        const result = checkUpdateAvailable(null, versions);
        assert.strictEqual(result.status, 'no-current-version');
    });

    it('no versions available returns no-versions', () => {
        const result = checkUpdateAvailable('1.0.0', null);
        assert.strictEqual(result.status, 'no-versions');
    });

    it('empty versions array returns no-versions', () => {
        const result = checkUpdateAvailable('1.0.0', []);
        assert.strictEqual(result.status, 'no-versions');
    });

    it('only filters available_for_current candidates', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '3.0.0', available_for_current: false }),
            makeVersion({ version: '1.0.0', available_for_current: true }),
        ];

        const result = checkUpdateAvailable('1.0.0', versions);
        assert.strictEqual(result.status, 'up-to-date');
    });

    it('pre-release version ordering works correctly (QA 4.3.8)', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0-beta.2' }),
            makeVersion({ version: '1.0.0-beta.1' }),
            makeVersion({ version: '1.0.0-alpha.1' }),
        ];

        const result = checkUpdateAvailable('1.0.0-alpha.1', versions);
        assert.strictEqual(result.status, 'update-available');
        if (result.status === 'update-available') {
            assert.strictEqual(result.latest, '1.0.0-beta.2');
        }
    });

    it('version above all available returns up-to-date', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0' }),
            makeVersion({ version: '0.9.0' }),
        ];

        const result = checkUpdateAvailable('2.0.0', versions);
        assert.strictEqual(result.status, 'up-to-date');
    });

    it('all versions unavailable for current platform returns no-versions', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '2.0.0', available_for_current: false }),
            makeVersion({ version: '1.0.0', available_for_current: false }),
        ];

        const result = checkUpdateAvailable('0.5.0', versions);
        assert.strictEqual(result.status, 'no-versions');
    });
});
