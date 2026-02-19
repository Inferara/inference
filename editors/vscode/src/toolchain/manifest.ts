import { compareSemver } from '../utils/semver';

/** A platform-specific file entry from the manifest. */
export interface FileEntry {
    url: string;
    sha256: string;
}

/** A single release entry from the manifest. */
export interface ReleaseEntry {
    version: string;
    stable: boolean;
    files: FileEntry[];
}

/** Minimal platform info needed for manifest matching. */
export interface ManifestPlatform {
    id: string;
}

/**
 * Extract tool name from a manifest file URL.
 * URL format: `https://.../tool-os-arch.ext` (e.g., `infs-linux-x64.tar.gz`).
 */
export function toolFromUrl(url: string): string {
    const filename = url.split('/').pop() ?? '';
    return filename.split('-')[0] ?? '';
}

/**
 * Extract OS name from a manifest file URL.
 * URL format: `https://.../tool-os-arch.ext` (e.g., `infs-linux-x64.tar.gz`).
 */
export function osFromUrl(url: string): string {
    const filename = url.split('/').pop() ?? '';
    const parts = filename.split('-');
    return parts.length > 1 ? parts[1] : '';
}

/** Map platform ID to the OS string used in manifest URLs. */
export function platformOs(platform: ManifestPlatform): string {
    if (platform.id === 'linux-x64') {
        return 'linux';
    }
    if (platform.id === 'macos-arm64') {
        return 'macos';
    }
    if (platform.id === 'windows-x64') {
        return 'windows';
    }
    return '';
}

/**
 * Find the latest release from the manifest for the given platform.
 * Matches the `infs` tool artifact for the platform's OS.
 * Returns the release entry, file URL, and sha256, or null if not found.
 */
export function findLatestRelease(
    manifest: ReleaseEntry[],
    platform: ManifestPlatform,
): { release: ReleaseEntry; fileUrl: string; sha256: string } | null {
    if (manifest.length === 0) {
        return null;
    }

    const sorted = [...manifest].sort((a, b) =>
        compareSemver(b.version, a.version),
    );

    const os = platformOs(platform);

    for (const release of sorted) {
        const file = release.files.find(
            (f) => toolFromUrl(f.url) === 'infs' && osFromUrl(f.url) === os,
        );
        if (file) {
            return { release, fileUrl: file.url, sha256: file.sha256 };
        }
    }

    return null;
}
