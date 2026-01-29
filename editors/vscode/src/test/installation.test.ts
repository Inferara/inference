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

const linuxPlatform: ManifestPlatform & {
    archiveExtension: string;
    binaryName: string;
} = {
    id: 'linux-x64',
    archiveExtension: '.tar.gz',
    binaryName: 'infs',
};

const macosPlatform: ManifestPlatform = {
    id: 'macos-arm64',
};

const windowsPlatform: ManifestPlatform = {
    id: 'windows-x64',
};

function makeManifest(entries: Partial<ReleaseEntry>[]): ReleaseEntry[] {
    return entries.map((e) => ({
        version: e.version ?? '0.0.1',
        stable: e.stable ?? false,
        files: e.files ?? [],
    }));
}

describe('manifest helpers', () => {
    it('toolFromUrl extracts tool name', () => {
        assert.strictEqual(
            toolFromUrl('https://example.com/infs-linux-x64.tar.gz'),
            'infs',
        );
        assert.strictEqual(
            toolFromUrl('https://example.com/infc-macos-arm64.tar.gz'),
            'infc',
        );
    });

    it('osFromUrl extracts OS name', () => {
        assert.strictEqual(
            osFromUrl('https://example.com/infs-linux-x64.tar.gz'),
            'linux',
        );
        assert.strictEqual(
            osFromUrl('https://example.com/infs-macos-arm64.tar.gz'),
            'macos',
        );
        assert.strictEqual(
            osFromUrl('https://example.com/infs-windows-x64.zip'),
            'windows',
        );
    });

    it('platformOs maps platform IDs to OS strings', () => {
        assert.strictEqual(platformOs({ id: 'linux-x64' }), 'linux');
        assert.strictEqual(platformOs({ id: 'macos-arm64' }), 'macos');
        assert.strictEqual(platformOs({ id: 'windows-x64' }), 'windows');
        assert.strictEqual(platformOs({ id: 'freebsd-x64' }), '');
    });
});

describe('findLatestRelease', () => {
    it('returns null for empty manifest', () => {
        const result = findLatestRelease([], linuxPlatform, 'stable');
        assert.strictEqual(result, null);
    });

    it('returns null when no matching platform', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-macos-arm64.tar.gz',
                        sha256: 'abc123',
                    },
                ],
            },
        ]);
        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.strictEqual(result, null);
    });

    it('returns latest stable release for stable channel', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash100',
                    },
                ],
            },
            {
                version: '2.0.0',
                stable: false,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash200',
                    },
                ],
            },
            {
                version: '1.5.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash150',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.release.version, '1.5.0');
        assert.strictEqual(result.sha256, 'hash150');
    });

    it('returns latest release (any) for latest channel', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash100',
                    },
                ],
            },
            {
                version: '2.0.0',
                stable: false,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash200',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'latest');
        assert.ok(result !== null);
        assert.strictEqual(result.release.version, '2.0.0');
        assert.strictEqual(result.sha256, 'hash200');
    });

    it('correctly matches infs tool (not infc) for the given platform OS', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infc-linux-x64.tar.gz',
                        sha256: 'hash-infc',
                    },
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash-infs',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.sha256, 'hash-infs');
        assert.ok(result.fileUrl.includes('infs-linux'));
    });

    it('sorts by semver correctly (higher version preferred)', () => {
        const manifest = makeManifest([
            {
                version: '0.9.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash-09',
                    },
                ],
            },
            {
                version: '1.2.3',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash-123',
                    },
                ],
            },
            {
                version: '1.2.2',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash-122',
                    },
                ],
            },
            {
                version: '1.10.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'hash-1100',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.release.version, '1.10.0');
        assert.strictEqual(result.sha256, 'hash-1100');
    });

    it('returns sha256 field from matched file', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'expected-sha256-value',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.sha256, 'expected-sha256-value');
    });

    it('matches macos platform correctly', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-macos-arm64.tar.gz',
                        sha256: 'mac-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, macosPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.sha256, 'mac-hash');
    });

    it('matches windows platform correctly', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-windows-x64.zip',
                        sha256: 'win-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, windowsPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.sha256, 'win-hash');
    });

    it('returns null when no stable releases exist for stable channel', () => {
        const manifest = makeManifest([
            {
                version: '2.0.0-beta',
                stable: false,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'beta-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.strictEqual(result, null);
    });

    it('returns fileUrl from matched file', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://releases.example.com/v1.0.0/infs-linux-x64.tar.gz',
                        sha256: 'some-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(
            result.fileUrl,
            'https://releases.example.com/v1.0.0/infs-linux-x64.tar.gz',
        );
    });

    it('prefers stable release over unstable pre-release with higher version', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'stable-hash',
                    },
                ],
            },
            {
                version: '2.0.0-alpha.1',
                stable: false,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'alpha-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.release.version, '1.0.0');
    });

    it('returns null when releases have files but none match platform', () => {
        const manifest = makeManifest([
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-macos-arm64.tar.gz',
                        sha256: 'mac-hash',
                    },
                    {
                        url: 'https://example.com/infs-windows-x64.zip',
                        sha256: 'win-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.strictEqual(result, null);
    });

    it('skips releases with empty files array', () => {
        const manifest = makeManifest([
            {
                version: '2.0.0',
                stable: true,
                files: [],
            },
            {
                version: '1.0.0',
                stable: true,
                files: [
                    {
                        url: 'https://example.com/infs-linux-x64.tar.gz',
                        sha256: 'fallback-hash',
                    },
                ],
            },
        ]);

        const result = findLatestRelease(manifest, linuxPlatform, 'stable');
        assert.ok(result !== null);
        assert.strictEqual(result.release.version, '1.0.0');
        assert.strictEqual(result.sha256, 'fallback-hash');
    });
});

describe('manifest helper edge cases', () => {
    it('toolFromUrl returns empty string for URL with no path segments', () => {
        assert.strictEqual(toolFromUrl('https://example.com/'), '');
    });

    it('toolFromUrl handles URL with no dashes in filename', () => {
        assert.strictEqual(toolFromUrl('https://example.com/singlename.tar.gz'), 'singlename.tar.gz');
    });

    it('osFromUrl returns empty string for URL with only tool name', () => {
        assert.strictEqual(osFromUrl('https://example.com/infs.tar.gz'), '');
    });

    it('platformOs returns empty for unrecognized platform', () => {
        assert.strictEqual(platformOs({ id: 'linux-arm64' }), '');
        assert.strictEqual(platformOs({ id: '' }), '');
    });
});
