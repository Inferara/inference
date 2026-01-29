import * as assert from 'node:assert';
import { describe, it } from 'node:test';
import { compareSemver } from '../utils/semver';

describe('compareSemver', () => {
    it('equal versions return 0', () => {
        assert.strictEqual(compareSemver('1.2.3', '1.2.3'), 0);
    });

    it('higher major is positive', () => {
        assert.ok(compareSemver('2.0.0', '1.0.0') > 0);
    });

    it('lower major is negative', () => {
        assert.ok(compareSemver('0.1.0', '1.0.0') < 0);
    });

    it('higher minor is positive', () => {
        assert.ok(compareSemver('1.2.0', '1.1.0') > 0);
    });

    it('higher patch is positive', () => {
        assert.ok(compareSemver('1.0.2', '1.0.1') > 0);
    });

    it('handles missing patch', () => {
        assert.strictEqual(compareSemver('1.0', '1.0.0'), 0);
    });

    it('0.1.0 >= 0.1.0', () => {
        assert.ok(compareSemver('0.1.0', '0.1.0') >= 0);
    });

    it('0.0.9 < 0.1.0', () => {
        assert.ok(compareSemver('0.0.9', '0.1.0') < 0);
    });

    it('strips v prefix from both arguments', () => {
        assert.strictEqual(compareSemver('v1.2.3', '1.2.3'), 0);
        assert.strictEqual(compareSemver('1.2.3', 'v1.2.3'), 0);
        assert.strictEqual(compareSemver('V1.2.3', 'v1.2.3'), 0);
    });

    it('pre-release has lower precedence than release', () => {
        assert.ok(compareSemver('1.0.0-alpha', '1.0.0') < 0);
        assert.ok(compareSemver('1.0.0', '1.0.0-alpha') > 0);
    });

    it('compares pre-release identifiers alphabetically', () => {
        assert.ok(compareSemver('1.0.0-alpha', '1.0.0-beta') < 0);
        assert.ok(compareSemver('1.0.0-beta', '1.0.0-alpha') > 0);
    });

    it('compares numeric pre-release identifiers as numbers', () => {
        assert.ok(compareSemver('1.0.0-alpha.1', '1.0.0-alpha.2') < 0);
        assert.ok(compareSemver('1.0.0-alpha.10', '1.0.0-alpha.2') > 0);
    });

    it('numeric identifiers have lower precedence than alphanumeric', () => {
        assert.ok(compareSemver('1.0.0-1', '1.0.0-alpha') < 0);
    });

    it('shorter pre-release set has lower precedence when prefix matches', () => {
        assert.ok(compareSemver('1.0.0-alpha', '1.0.0-alpha.1') < 0);
        assert.ok(compareSemver('1.0.0-alpha.1', '1.0.0-alpha') > 0);
    });

    it('equal pre-release versions return 0', () => {
        assert.strictEqual(compareSemver('1.0.0-beta.1', '1.0.0-beta.1'), 0);
    });

    it('0.0.1-beta.1 >= 0.0.1-beta.1 (MIN_INFS_VERSION self-compare)', () => {
        assert.ok(compareSemver('0.0.1-beta.1', '0.0.1-beta.1') >= 0);
    });

    it('0.1.0 >= 0.0.1-beta.1 (release beats pre-release)', () => {
        assert.ok(compareSemver('0.1.0', '0.0.1-beta.1') > 0);
    });
});
