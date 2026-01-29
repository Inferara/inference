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
});
