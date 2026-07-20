import * as os from 'os';
import * as path from 'path';

/**
 * Resolution of the managed toolchain root ("inference home").
 *
 * This module is the single source of truth for where the extension expects
 * an infs-managed installation. Every consumer (LSP binary resolution,
 * toolchain detection, install destination, doctor, terminal PATH prepend)
 * must derive the root from {@link inferenceHome} so they cannot diverge
 * from each other — or from `infs` itself.
 *
 * The derivation mirrors `ToolchainPaths::new()` in
 * `apps/infs/src/toolchain/paths.rs`:
 * 1. `INFERENCE_HOME` environment variable override (used verbatim).
 * 2. Windows: the OS roaming application-data directory joined with
 *    `inference` — `%APPDATA%\inference` (infs uses `dirs::data_dir()`,
 *    which resolves the same folder via the Known Folder API).
 * 3. All other platforms, including macOS: `~/.inference`.
 *
 * One deliberate deviation: when `APPDATA` is unset on Windows, infs errors
 * out and asks the user to set `INFERENCE_HOME`, but the extension must keep
 * activating (detection falls through to the PATH tier), so this function
 * stays total and falls back to the conventional roaming location under the
 * user profile, `<homedir>\AppData\Roaming\inference`.
 *
 * The function MUST NOT import `vscode` (directly or transitively) so it
 * stays importable from plain `node:test` files; environment and platform
 * are injectable parameters defaulting to the real process values.
 */
export function inferenceHome(
    env: NodeJS.ProcessEnv = process.env,
    platform: NodeJS.Platform = process.platform,
): string {
    const override = env['INFERENCE_HOME'];
    if (override) {
        return override;
    }
    if (platform === 'win32') {
        const appData = env['APPDATA'];
        if (appData) {
            return path.win32.join(appData, 'inference');
        }
        return path.win32.join(
            os.homedir(),
            'AppData',
            'Roaming',
            'inference',
        );
    }
    return path.posix.join(os.homedir(), '.inference');
}
