import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { parseCurrentVersion } from '../toolchain/versions';
import { compareSemver } from '../utils/semver';

/**
 * Extended tests for version parsing and minimum version checking.
 * Covers QA Section 9.2-9.3.
 */

describe('parseCurrentVersion edge cases (QA Section 9.2)', () => {
    it('returns null for whitespace-only input', () => {
        assert.strictEqual(parseCurrentVersion('   \n  '), null);
    });

    it('returns null for version string without "infs" prefix', () => {
        assert.strictEqual(parseCurrentVersion('0.1.0'), null);
    });

    it('returns null for "infs" followed by only whitespace', () => {
        assert.strictEqual(parseCurrentVersion('infs   '), null);
    });

    it('handles extra whitespace between infs and version', () => {
        const result = parseCurrentVersion('infs   0.3.0\n');
        assert.strictEqual(result, '0.3.0');
    });

    it('returns null for tab-separated format', () => {
        const result = parseCurrentVersion('infs\t0.3.0\n');
        assert.strictEqual(result, '0.3.0');
    });

    it('returns null for completely empty input', () => {
        assert.strictEqual(parseCurrentVersion(''), null);
    });
});

describe('minimum version check (QA Section 9.3)', () => {
    const MIN_INFS_VERSION = '0.0.1-beta.1';

    it('version below minimum: 0.0.0 < 0.0.1-beta.1', () => {
        assert.ok(compareSemver('0.0.0', MIN_INFS_VERSION) < 0);
    });

    it('version equal to minimum passes', () => {
        assert.strictEqual(compareSemver(MIN_INFS_VERSION, MIN_INFS_VERSION), 0);
    });

    it('version above minimum: 0.0.1 > 0.0.1-beta.1 (release > pre-release)', () => {
        assert.ok(compareSemver('0.0.1', MIN_INFS_VERSION) > 0);
    });

    it('version well above minimum: 1.0.0 > 0.0.1-beta.1', () => {
        assert.ok(compareSemver('1.0.0', MIN_INFS_VERSION) > 0);
    });

    it('earlier pre-release is below minimum: 0.0.1-alpha.1 < 0.0.1-beta.1', () => {
        assert.ok(compareSemver('0.0.1-alpha.1', MIN_INFS_VERSION) < 0);
    });

    it('later pre-release is above minimum: 0.0.1-beta.2 > 0.0.1-beta.1', () => {
        assert.ok(compareSemver('0.0.1-beta.2', MIN_INFS_VERSION) > 0);
    });

    it('v-prefixed version works: v0.0.1 > 0.0.1-beta.1', () => {
        assert.ok(compareSemver('v0.0.1', MIN_INFS_VERSION) > 0);
    });

    it('pre-release with different tag: 0.0.1-rc.1 > 0.0.1-beta.1', () => {
        assert.ok(compareSemver('0.0.1-rc.1', MIN_INFS_VERSION) > 0);
    });
});
