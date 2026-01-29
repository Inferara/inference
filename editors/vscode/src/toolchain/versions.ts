import { exec } from '../utils/exec';

/** Version info returned by `infs versions --json`. */
export interface VersionInfo {
    version: string;
    stable: boolean;
    platforms: string[];
    available_for_current: boolean;
}

/**
 * Parse the JSON output of `infs versions --json`.
 * Returns an empty array if the output is invalid.
 */
export function parseVersionsOutput(stdout: string): VersionInfo[] {
    try {
        const parsed = JSON.parse(stdout);
        if (!Array.isArray(parsed)) {
            return [];
        }
        return parsed;
    } catch {
        return [];
    }
}

/**
 * Parse the version string from `infs version` output.
 * Expected format: "infs X.Y.Z"
 * Returns the version string or null on parse failure.
 */
export function parseCurrentVersion(stdout: string): string | null {
    const match = stdout.match(/^infs\s+(\S+)/);
    return match ? match[1] : null;
}

/**
 * Run `infs versions --json` and parse the output.
 * Returns null if the command fails.
 */
export async function fetchVersions(
    infsPath: string,
): Promise<VersionInfo[] | null> {
    try {
        const result = await exec(infsPath, ['versions', '--json']);
        if (result.exitCode !== 0) {
            return null;
        }
        return parseVersionsOutput(result.stdout);
    } catch {
        return null;
    }
}

/**
 * Run `infs version` and parse the current version.
 * Returns null if the command fails or the output is unexpected.
 */
export async function getCurrentVersion(
    infsPath: string,
): Promise<string | null> {
    try {
        const result = await exec(infsPath, ['version']);
        if (result.exitCode !== 0) {
            return null;
        }
        return parseCurrentVersion(result.stdout);
    } catch {
        return null;
    }
}

/** Result of an install-and-set-default operation. */
export interface SwitchResult {
    success: boolean;
    installedButNotDefault: boolean;
    error?: string;
}

/**
 * Install a toolchain version and set it as default.
 *
 * Runs `infs install VERSION` followed by `infs default VERSION`.
 * Handles the partial-success case where install succeeds but setting default fails.
 */
export async function installAndSetDefault(
    infsPath: string,
    version: string,
): Promise<SwitchResult> {
    const installResult = await exec(infsPath, ['install', version], {
        timeoutMs: 120_000,
    });
    if (installResult.exitCode !== 0) {
        const detail = installResult.stderr || installResult.stdout;
        return { success: false, installedButNotDefault: false, error: detail };
    }

    const defaultResult = await exec(infsPath, ['default', version]);
    if (defaultResult.exitCode !== 0) {
        const detail = defaultResult.stderr || defaultResult.stdout;
        return { success: false, installedButNotDefault: true, error: detail };
    }

    return { success: true, installedButNotDefault: false };
}
