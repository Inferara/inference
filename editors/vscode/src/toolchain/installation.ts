import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { PlatformInfo } from './platform';
import { inferenceHome } from './detection';
import { getSettings } from '../config/settings';
import { fetchJson, downloadFile, sha256File } from '../utils/download';
import { extractArchive } from '../utils/extract';
import { exec } from '../utils/exec';
import {
    ReleaseEntry,
    findLatestRelease,
} from './manifest';

export type { FileEntry, ReleaseEntry } from './manifest';
export { findLatestRelease } from './manifest';

/** Progress updates emitted during installation. */
export interface InstallProgress {
    stage:
        | 'fetching-manifest'
        | 'downloading'
        | 'extracting'
        | 'installing'
        | 'verifying';
    message: string;
    bytesReceived?: number;
    bytesTotal?: number;
}

export type InstallProgressCallback = (progress: InstallProgress) => void;

/** Result of a successful installation. */
export interface InstallResult {
    infsPath: string;
    version: string;
    doctorWarnings: boolean;
}

export const MANIFEST_URL = 'https://inference-lang.org/releases.json';

/**
 * Run the full installation flow.
 * Fetches manifest, downloads infs, extracts, runs `infs install`, verifies with `infs doctor`.
 * Throws on failure with a descriptive error message.
 */
export async function installToolchain(
    platform: PlatformInfo,
    onProgress?: InstallProgressCallback,
): Promise<InstallResult> {
    const settings = getSettings();
    const channel =
        settings.channel === 'stable' || settings.channel === 'latest'
            ? settings.channel
            : 'stable';

    onProgress?.({
        stage: 'fetching-manifest',
        message: 'Fetching release manifest...',
    });

    const manifest = await fetchJson<ReleaseEntry[]>(MANIFEST_URL);

    const match = findLatestRelease(manifest, platform, channel);
    if (!match) {
        throw new Error(
            `No compatible infs release found for ${platform.id} in the ${channel} channel.`,
        );
    }

    const { release, fileUrl, sha256 } = match;
    const version = release.version;

    onProgress?.({
        stage: 'downloading',
        message: `Downloading infs v${version}...`,
    });

    const destDir = path.join(inferenceHome(), 'bin');
    fs.mkdirSync(destDir, { recursive: true });

    const archiveName = `infs-${platform.id}${platform.archiveExtension}`;
    const archivePath = path.join(os.tmpdir(), archiveName);

    try {
        await downloadFile(fileUrl, {
            destPath: archivePath,
            onProgress: (received, total) => {
                onProgress?.({
                    stage: 'downloading',
                    message: `Downloading infs v${version}...`,
                    bytesReceived: received,
                    bytesTotal: total,
                });
            },
        });

        const actualHash = await sha256File(archivePath);
        if (actualHash !== sha256) {
            throw new Error(
                `SHA-256 verification failed for infs v${version}. Expected ${sha256}, got ${actualHash}.`,
            );
        }

        onProgress?.({
            stage: 'extracting',
            message: 'Extracting archive...',
        });

        await extractArchive({ archivePath, destDir });
    } finally {
        try {
            fs.unlinkSync(archivePath);
        } catch {
            // best-effort cleanup
        }
    }

    const infsPath = path.join(destDir, platform.binaryName);
    if (!fs.existsSync(infsPath)) {
        throw new Error(
            `infs binary not found at ${infsPath} after extraction.`,
        );
    }

    onProgress?.({
        stage: 'installing',
        message: 'Running infs install...',
    });

    const installResult = await exec(infsPath, ['install'], {
        timeoutMs: 120_000,
    });
    if (installResult.exitCode !== 0) {
        throw new Error(
            `infs install failed (exit ${installResult.exitCode}): ${installResult.stderr || installResult.stdout}`,
        );
    }

    onProgress?.({
        stage: 'verifying',
        message: 'Verifying installation...',
    });

    let doctorWarnings = false;
    try {
        const doctorResult = await exec(infsPath, ['doctor'], {
            timeoutMs: 30_000,
        });
        if (doctorResult.exitCode !== 0) {
            doctorWarnings = true;
        }
    } catch {
        doctorWarnings = true;
    }

    return { infsPath, version, doctorWarnings };
}
