import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { buildVersionPickItems } from '../toolchain/versionPicker';
import type { VersionInfo } from '../toolchain/versions';

/**
 * Tests for version picker item building logic.
 * Covers QA Section 4.4.
 */

function makeVersion(overrides: Partial<VersionInfo>): VersionInfo {
    return {
        version: overrides.version ?? '0.0.1',
        stable: overrides.stable ?? false,
        platforms: overrides.platforms ?? ['linux'],
        available_for_current: overrides.available_for_current ?? true,
    };
}

describe('buildVersionPickItems (QA Section 4.4)', () => {
    it('sorts descending by semver', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '0.1.0' }),
            makeVersion({ version: '1.0.0' }),
            makeVersion({ version: '0.5.0' }),
        ];

        const items = buildVersionPickItems(versions, null);

        assert.deepStrictEqual(
            items.map((i) => i.label),
            ['1.0.0', '0.5.0', '0.1.0'],
        );
    });

    it('current version has "(current)" tag', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0' }),
            makeVersion({ version: '0.9.0' }),
        ];

        const items = buildVersionPickItems(versions, '0.9.0');

        const current = items.find((i) => i.label === '0.9.0');
        assert.ok(current);
        assert.ok(current.description?.includes('current'));
    });

    it('current version appears first in the list', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '2.0.0' }),
            makeVersion({ version: '1.0.0' }),
            makeVersion({ version: '0.5.0' }),
        ];

        const items = buildVersionPickItems(versions, '0.5.0');

        assert.strictEqual(items[0].label, '0.5.0');
    });

    it('stable versions have "(stable)" tag', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0', stable: true }),
            makeVersion({ version: '1.1.0-beta', stable: false }),
        ];

        const items = buildVersionPickItems(versions, null);

        const stable = items.find((i) => i.label === '1.0.0');
        assert.ok(stable);
        assert.ok(stable.description?.includes('stable'));

        const beta = items.find((i) => i.label === '1.1.0-beta');
        assert.ok(beta);
        assert.strictEqual(beta.description, undefined);
    });

    it('current + stable both tagged: "(current, stable)"', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0', stable: true }),
        ];

        const items = buildVersionPickItems(versions, '1.0.0');

        assert.strictEqual(items[0].description, '(current, stable)');
    });

    it('only available_for_current versions included', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '2.0.0', available_for_current: true }),
            makeVersion({ version: '1.0.0', available_for_current: false }),
            makeVersion({ version: '0.5.0', available_for_current: true }),
        ];

        const items = buildVersionPickItems(versions, null);

        assert.strictEqual(items.length, 2);
        assert.ok(items.every((i) => i.label !== '1.0.0'));
    });

    it('returns empty array when no versions are available', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '1.0.0', available_for_current: false }),
        ];

        const items = buildVersionPickItems(versions, null);
        assert.strictEqual(items.length, 0);
    });

    it('current version already at top is not duplicated', () => {
        const versions: VersionInfo[] = [
            makeVersion({ version: '2.0.0' }),
            makeVersion({ version: '1.0.0' }),
        ];

        const items = buildVersionPickItems(versions, '2.0.0');

        assert.strictEqual(items.length, 2);
        assert.strictEqual(items[0].label, '2.0.0');
    });
});
