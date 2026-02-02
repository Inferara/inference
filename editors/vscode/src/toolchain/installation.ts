import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { PlatformInfo } from './platform';
import { inferenceHome } from './home';
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

const DEFAULT_DIST_SERVER = 'https://inference-lang.org';
const RELEASES_PATH = '/releases.json';

function manifestUrl(): string {
    const server = process.env['INFS_DIST_SERVER']?.trim();
    const base = server && server.length > 0
        ? server.replace(/\/+$/, '')
        : DEFAULT_DIST_SERVER;
    return `${base}${RELEASES_PATH}`;
}

/**
 * Run the full installation flow.
 * Fetches manifest, downloads infs, extracts, runs `infs install`, verifies with `infs doctor`.
 * Throws on failure with a descriptive error message.
 */
export async function installToolchain(
    platform: PlatformInfo,
    onProgress?: InstallProgressCallback,
): Promise<InstallResult> {
    onProgress?.({
        stage: 'fetching-manifest',
        message: 'Fetching release manifest...',
    });

    const manifest = await fetchJson<ReleaseEntry[]>(manifestUrl());

    if (!Array.isArray(manifest)) {
        throw new Error('Invalid release manifest: expected an array.');
    }
    for (const entry of manifest) {
        if (
            typeof entry?.version !== 'string' ||
            typeof entry?.stable !== 'boolean' ||
            !Array.isArray(entry?.files)
        ) {
            throw new Error(
                `Invalid release manifest entry: ${JSON.stringify(entry)?.slice(0, 200)}`,
            );
        }
    }

    const match = findLatestRelease(manifest, platform);
    if (!match) {
        throw new Error(
            `No compatible infs release found for ${platform.id}.`,
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
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-'));
    const archivePath = path.join(tmpDir, archiveName);

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
            fs.rmSync(tmpDir, { recursive: true, force: true });
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

    const binDir = path.join(inferenceHome(), 'bin');
    const sep = process.platform === 'win32' ? ';' : ':';
    const augmentedPath = `${binDir}${sep}${process.env['PATH'] ?? ''}`;

    const installResult = await exec(infsPath, ['install'], {
        timeoutMs: 120_000,
        env: { PATH: augmentedPath },
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
            env: { PATH: augmentedPath },
        });
        if (doctorResult.exitCode !== 0) {
            doctorWarnings = true;
        } else {
            const { parseDoctorOutput } = await import('./doctor');
            const parsed = parseDoctorOutput(doctorResult.stdout);
            if (parsed.hasErrors || parsed.hasWarnings) {
                doctorWarnings = true;
            }
        }
    } catch {
        doctorWarnings = true;
    }

    return { infsPath, version, doctorWarnings };
}
