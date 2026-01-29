import * as fs from 'fs';
import * as path from 'path';
import { exec } from './exec';

export interface ExtractOptions {
    /** Path to the archive file. */
    archivePath: string;
    /** Directory to extract into (created if it doesn't exist). */
    destDir: string;
}

/**
 * Extract an archive to the destination directory.
 * Detects format from file extension (.tar.gz or .zip).
 * On Unix, sets executable permission on extracted binaries.
 */
export async function extractArchive(options: ExtractOptions): Promise<void> {
    fs.mkdirSync(options.destDir, { recursive: true });

    if (
        options.archivePath.endsWith('.tar.gz') ||
        options.archivePath.endsWith('.tgz')
    ) {
        await extractTarGz(options.archivePath, options.destDir);
    } else if (options.archivePath.endsWith('.zip')) {
        await extractZip(options.archivePath, options.destDir);
    } else {
        throw new Error(
            `Unsupported archive format: ${path.basename(options.archivePath)}`,
        );
    }

    if (process.platform !== 'win32') {
        setExecutablePermissions(options.destDir);
    }
}

async function extractTarGz(
    archivePath: string,
    destDir: string,
): Promise<void> {
    const result = await exec('tar', ['-xzf', archivePath, '-C', destDir]);
    if (result.exitCode !== 0) {
        throw new Error(
            `tar extraction failed (exit ${result.exitCode}): ${result.stderr}`,
        );
    }
}

async function extractZip(
    archivePath: string,
    destDir: string,
): Promise<void> {
    const result = await exec('powershell', [
        '-NoProfile',
        '-Command',
        `Expand-Archive -Path '${archivePath}' -DestinationPath '${destDir}' -Force`,
    ]);
    if (result.exitCode !== 0) {
        throw new Error(
            `zip extraction failed (exit ${result.exitCode}): ${result.stderr}`,
        );
    }
}

/** Set executable permissions on files in the directory (non-recursive, top level only). */
function setExecutablePermissions(dir: string): void {
    try {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        for (const entry of entries) {
            if (entry.isFile()) {
                const filePath = path.join(dir, entry.name);
                fs.chmodSync(filePath, 0o755);
            }
        }
    } catch {
        // Best-effort: don't fail if chmod fails
    }
}
