import * as fs from 'fs';
import * as path from 'path';
import { getSettings } from '../config/settings';
import { detectPlatform } from './platform';
import { inferenceHome } from './home';

export { inferenceHome };

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

/** Source where the infs binary was found. */
export type InfsSource = 'settings' | 'managed' | 'path';

/** Result of infs binary detection. */
export interface InfsDetection {
    path: string;
    source: InfsSource;
}

/**
 * Detect infs binary location.
 *
 * Search order:
 * 1. Custom path from settings (inference.path)
 * 2. Managed location (INFERENCE_HOME/bin/infs)
 * 3. System PATH
 *
 * Returns the resolved absolute path and source, or null if not found.
 */
export function detectInfs(): InfsDetection | null {
    const platform = detectPlatform();
    const binaryName = platform?.binaryName ?? 'infs';

    const settings = getSettings();
    if (settings.path) {
        if (isExecutable(settings.path)) {
            return { path: settings.path, source: 'settings' };
        }
        return null;
    }

    const managedPath = path.join(inferenceHome(), 'bin', binaryName);
    if (isExecutable(managedPath)) {
        return { path: managedPath, source: 'managed' };
    }

    const pathResult = findInPath(binaryName);
    if (pathResult) {
        return { path: pathResult, source: 'path' };
    }

    return null;
}
