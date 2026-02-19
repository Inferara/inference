import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import {
    findLatestRelease,
    toolFromUrl,
    osFromUrl,
    platformOs,
    type ReleaseEntry,
    type ManifestPlatform,
} from '../toolchain/manifest';

/**
 * Additional manifest parsing tests complementing installation.test.ts.
 * Covers QA Section 4.1 (manifest logic).
 */

function makeRelease(overrides: Partial<ReleaseEntry>): ReleaseEntry {
    return {
        version: overrides.version ?? '0.0.1',
        stable: overrides.stable ?? false,
        files: overrides.files ?? [],
    };
}

describe('manifest: findLatestRelease pre-release ordering', () => {
    it('prefers release over pre-release at same major.minor.patch', () => {
        const manifest: ReleaseEntry[] = [
            makeRelease({
                version: '1.0.0-beta.1',
                files: [{ url: 'https://x.com/infs-linux-x64.tar.gz', sha256: 'a' }],
            }),
            makeRelease({
                version: '1.0.0',
                files: [{ url: 'https://x.com/infs-linux-x64.tar.gz', sha256: 'b' }],
            }),
        ];
        const result = findLatestRelease(manifest, { id: 'linux-x64' });
        assert.ok(result);
        assert.strictEqual(result.release.version, '1.0.0');
    });

    it('handles multiple pre-release versions correctly', () => {
        const manifest: ReleaseEntry[] = [
            makeRelease({
                version: '1.0.0-alpha.1',
                files: [{ url: 'https://x.com/infs-linux-x64.tar.gz', sha256: 'a' }],
            }),
            makeRelease({
                version: '1.0.0-beta.2',
                files: [{ url: 'https://x.com/infs-linux-x64.tar.gz', sha256: 'b' }],
            }),
            makeRelease({
                version: '1.0.0-beta.1',
                files: [{ url: 'https://x.com/infs-linux-x64.tar.gz', sha256: 'c' }],
            }),
        ];
        const result = findLatestRelease(manifest, { id: 'linux-x64' });
        assert.ok(result);
        assert.strictEqual(result.release.version, '1.0.0-beta.2');
    });

    it('falls back to older release if latest has no matching platform', () => {
        const manifest: ReleaseEntry[] = [
            makeRelease({
                version: '2.0.0',
                files: [{ url: 'https://x.com/infs-macos-arm64.tar.gz', sha256: 'a' }],
            }),
            makeRelease({
                version: '1.5.0',
                files: [{ url: 'https://x.com/infs-linux-x64.tar.gz', sha256: 'b' }],
            }),
        ];
        const result = findLatestRelease(manifest, { id: 'linux-x64' });
        assert.ok(result);
        assert.strictEqual(result.release.version, '1.5.0');
    });
});

describe('manifest: URL parsing edge cases', () => {
    it('toolFromUrl handles deeply nested URL paths', () => {
        assert.strictEqual(
            toolFromUrl('https://cdn.example.com/v1/releases/stable/infs-linux-x64.tar.gz'),
            'infs',
        );
    });

    it('osFromUrl handles URL with query string', () => {
        assert.strictEqual(
            osFromUrl('https://example.com/infs-linux-x64.tar.gz?token=abc'),
            'linux',
        );
    });

    it('toolFromUrl returns empty for bare domain URL', () => {
        assert.strictEqual(toolFromUrl('https://example.com'), 'example.com');
    });

    it('platformOs maps all 3 supported platform IDs', () => {
        const platforms: ManifestPlatform[] = [
            { id: 'linux-x64' },
            { id: 'macos-arm64' },
            { id: 'windows-x64' },
        ];
        const results = platforms.map(platformOs);
        assert.deepStrictEqual(results, ['linux', 'macos', 'windows']);
    });

    it('platformOs returns empty for unknown platform', () => {
        assert.strictEqual(platformOs({ id: 'solaris-sparc' }), '');
    });
});

describe('manifest: findLatestRelease with single entry', () => {
    it('returns the only matching entry', () => {
        const manifest: ReleaseEntry[] = [
            makeRelease({
                version: '0.1.0',
                files: [{ url: 'https://x.com/infs-windows-x64.zip', sha256: 'w' }],
            }),
        ];
        const result = findLatestRelease(manifest, { id: 'windows-x64' });
        assert.ok(result);
        assert.strictEqual(result.release.version, '0.1.0');
        assert.strictEqual(result.sha256, 'w');
    });
});
