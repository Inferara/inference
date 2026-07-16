import * as path from 'path';

/**
 * Pure helpers for locating the `inference-lsp` language server binary and
 * deciding lifecycle actions. This module MUST NOT import `vscode` (directly
 * or transitively) so it stays importable from plain `node:test` files; all
 * environment access is injected through {@link ResolveLspBinaryOptions}.
 *
 * The resolution semantics deliberately mirror `detectInfs()` in
 * `src/toolchain/detection.ts`:
 * 1. Explicit configured path (`inference.lsp.path`) — if set but not
 *    executable, resolution FAILS without falling back.
 * 2. Managed location (`INFERENCE_HOME/bin/<binary>`).
 * 3. System PATH.
 * Resolution is attempted even on platforms without prebuilt toolchain
 * support, matching detectInfs() which still probes a bare binary name there.
 */

/** Source where the inference-lsp binary was found. */
export type LspBinarySource = 'settings' | 'managed' | 'path';

/** Result of inference-lsp binary resolution. */
export interface LspBinaryResolution {
    path: string;
    source: LspBinarySource;
}

/** Inputs for {@link resolveLspBinary}; all environment access is explicit. */
export interface ResolveLspBinaryOptions {
    /** Value of the `inference.lsp.path` setting; empty means auto-detect. */
    configuredPath: string;
    /** Resolved INFERENCE_HOME directory. */
    inferenceHome: string;
    /** Whether resolving for Windows (`.exe` suffix, `;` PATH separator). */
    isWindows: boolean;
    /** Value of the PATH environment variable. */
    envPath: string;
    /** Probe that reports whether a candidate file is executable. */
    isExecutable: (filePath: string) => boolean;
}

/** Platform-specific file name of the language server binary. */
export function lspBinaryName(isWindows: boolean): string {
    return isWindows ? 'inference-lsp.exe' : 'inference-lsp';
}

/**
 * Resolve the inference-lsp binary location.
 *
 * Search order (same as `detectInfs()`):
 * 1. Configured path — used verbatim; when set but not executable the
 *    result is `null` with NO fallback to other locations.
 * 2. Managed location: `<inferenceHome>/bin/<lspBinaryName>`.
 * 3. First match on PATH.
 */
export function resolveLspBinary(
    options: ResolveLspBinaryOptions,
): LspBinaryResolution | null {
    if (options.configuredPath) {
        if (options.isExecutable(options.configuredPath)) {
            return { path: options.configuredPath, source: 'settings' };
        }
        return null;
    }

    const binaryName = lspBinaryName(options.isWindows);

    const managedPath = path.join(options.inferenceHome, 'bin', binaryName);
    if (options.isExecutable(managedPath)) {
        return { path: managedPath, source: 'managed' };
    }

    const sep = options.isWindows ? ';' : ':';
    const dirs = options.envPath.split(sep).filter(Boolean);
    for (const dir of dirs) {
        const candidate = path.join(dir, binaryName);
        if (options.isExecutable(candidate)) {
            return { path: candidate, source: 'path' };
        }
    }

    return null;
}

/** Lifecycle action to take in response to a configuration change. */
export type LspConfigChangeAction = 'restart' | 'stop' | 'none';

/**
 * Decide how the language client should react to a configuration change.
 *
 * - Changes outside `inference.lsp.*` are ignored.
 * - Disabling stops a running client (no-op when already stopped).
 * - Any `inference.lsp.*` change while enabled triggers a restart so the
 *   client re-resolves the binary; restart also covers the not-yet-running
 *   case (stop is a no-op, then a fresh start is attempted).
 */
export function lspActionForConfigChange(change: {
    affectsLsp: boolean;
    enabled: boolean;
    running: boolean;
}): LspConfigChangeAction {
    if (!change.affectsLsp) {
        return 'none';
    }
    if (!change.enabled) {
        return change.running ? 'stop' : 'none';
    }
    return 'restart';
}
