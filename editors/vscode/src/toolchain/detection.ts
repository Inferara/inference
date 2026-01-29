import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { getSettings } from '../config/settings';
import { detectPlatform } from './platform';

/** Resolve the INFERENCE_HOME directory (default: ~/.inference). */
export function inferenceHome(): string {
    return process.env['INFERENCE_HOME'] || path.join(os.homedir(), '.inference');
}

/** Check whether a file exists and is executable (or just exists on Windows). */
export function isExecutable(filePath: string): boolean {
    try {
        const mode = process.platform === 'win32'
            ? fs.constants.F_OK
            : fs.constants.X_OK;
        fs.accessSync(filePath, mode);
        return true;
    } catch {
        return false;
    }
}

/**
 * Search PATH for the given binary name.
 * Returns the first match or null.
 */
export function findInPath(binaryName: string): string | null {
    const envPath = process.env['PATH'] || '';
    const sep = process.platform === 'win32' ? ';' : ':';
    const dirs = envPath.split(sep).filter(Boolean);
    for (const dir of dirs) {
        const candidate = path.join(dir, binaryName);
        if (isExecutable(candidate)) {
            return candidate;
        }
    }
    return null;
}

/**
 * Detect infs binary location.
 *
 * Search order:
 * 1. Custom path from settings (inference.path)
 * 2. System PATH
 * 3. Default managed location (~/.inference/bin/infs)
 *
 * Returns the resolved absolute path or null if not found.
 */
export function detectInfs(): string | null {
    const platform = detectPlatform();
    const binaryName = platform?.binaryName ?? 'infs';

    const settings = getSettings();
    if (settings.path) {
        if (isExecutable(settings.path)) {
            return settings.path;
        }
        return null;
    }

    const pathResult = findInPath(binaryName);
    if (pathResult) {
        return pathResult;
    }

    const managedPath = path.join(inferenceHome(), 'bin', binaryName);
    if (isExecutable(managedPath)) {
        return managedPath;
    }

    return null;
}
