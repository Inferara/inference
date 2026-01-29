import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { parseVersionsOutput, parseCurrentVersion } from '../toolchain/versions';

describe('parseVersionsOutput', () => {
    it('parses a valid JSON array with multiple versions', () => {
        const stdout = JSON.stringify([
            {
                version: '0.2.0',
                stable: true,
                platforms: ['linux', 'macos', 'windows'],
                available_for_current: true,
            },
            {
                version: '0.1.0',
                stable: true,
                platforms: ['linux'],
                available_for_current: true,
            },
            {
                version: '0.3.0-alpha',
                stable: false,
                platforms: ['linux'],
                available_for_current: false,
            },
        ]);

        const result = parseVersionsOutput(stdout);

        assert.strictEqual(result.length, 3);
        assert.strictEqual(result[0].version, '0.2.0');
        assert.strictEqual(result[0].stable, true);
        assert.deepStrictEqual(result[0].platforms, ['linux', 'macos', 'windows']);
        assert.strictEqual(result[0].available_for_current, true);
        assert.strictEqual(result[2].version, '0.3.0-alpha');
        assert.strictEqual(result[2].stable, false);
        assert.strictEqual(result[2].available_for_current, false);
    });

    it('parses a single version entry', () => {
        const stdout = JSON.stringify([
            {
                version: '0.1.0',
                stable: true,
                platforms: ['linux'],
                available_for_current: true,
            },
        ]);

        const result = parseVersionsOutput(stdout);

        assert.strictEqual(result.length, 1);
        assert.strictEqual(result[0].version, '0.1.0');
    });

    it('returns empty array for empty JSON array', () => {
        const result = parseVersionsOutput('[]');

        assert.strictEqual(result.length, 0);
    });

    it('returns empty array for invalid JSON', () => {
        const result = parseVersionsOutput('not json');

        assert.strictEqual(result.length, 0);
    });

    it('returns empty array for empty string', () => {
        const result = parseVersionsOutput('');

        assert.strictEqual(result.length, 0);
    });

    it('returns empty array for JSON object instead of array', () => {
        const result = parseVersionsOutput('{"version": "0.1.0"}');

        assert.strictEqual(result.length, 0);
    });
});

describe('parseCurrentVersion', () => {
    it('parses standard version output', () => {
        const result = parseCurrentVersion('infs 0.2.0\n');

        assert.strictEqual(result, '0.2.0');
    });

    it('parses pre-release version', () => {
        const result = parseCurrentVersion('infs 0.1.0-beta.1\n');

        assert.strictEqual(result, '0.1.0-beta.1');
    });

    it('returns null for empty string', () => {
        const result = parseCurrentVersion('');

        assert.strictEqual(result, null);
    });

    it('returns null for unexpected format', () => {
        const result = parseCurrentVersion('unknown output');

        assert.strictEqual(result, null);
    });

    it('parses version without trailing newline', () => {
        const result = parseCurrentVersion('infs 1.0.0');

        assert.strictEqual(result, '1.0.0');
    });

    it('parses version with build metadata', () => {
        const result = parseCurrentVersion('infs 0.1.0+build.123\n');

        assert.strictEqual(result, '0.1.0+build.123');
    });

    it('returns null for just "infs" with no version', () => {
        const result = parseCurrentVersion('infs ');

        assert.strictEqual(result, null);
    });

    it('returns null for version output with different tool name', () => {
        const result = parseCurrentVersion('infc 0.1.0\n');

        assert.strictEqual(result, null);
    });
});

describe('parseVersionsOutput edge cases', () => {
    it('preserves entries with extra fields', () => {
        const stdout = JSON.stringify([
            {
                version: '0.1.0',
                stable: true,
                platforms: ['linux'],
                available_for_current: true,
                extra_field: 'ignored',
            },
        ]);

        const result = parseVersionsOutput(stdout);
        assert.strictEqual(result.length, 1);
        assert.strictEqual(result[0].version, '0.1.0');
    });

    it('returns empty array for null JSON', () => {
        const result = parseVersionsOutput('null');
        assert.strictEqual(result.length, 0);
    });

    it('returns empty array for numeric JSON', () => {
        const result = parseVersionsOutput('42');
        assert.strictEqual(result.length, 0);
    });

    it('returns empty array for string JSON', () => {
        const result = parseVersionsOutput('"hello"');
        assert.strictEqual(result.length, 0);
    });
});
