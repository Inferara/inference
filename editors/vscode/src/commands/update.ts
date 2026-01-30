import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { fetchVersions, getCurrentVersion } from '../toolchain/versions';
import { compareSemver } from '../utils/semver';
import { getSettings } from '../config/settings';
import { performVersionChange } from './versionChange';

/** Guard against concurrent update operations. */
let updating = false;

/**
 * Register the inference.updateToolchain command.
 * Checks for updates and prompts the user to install if available.
 */
export function registerUpdateCommand(
    outputChannel: vscode.OutputChannel,
): vscode.Disposable {
    return vscode.commands.registerCommand(
        'inference.updateToolchain',
        async () => {
            if (updating) {
                vscode.window.showInformationMessage(
                    'Update check is already in progress.',
                );
                return;
            }

            const infsPath = detectInfs();
            if (!infsPath) {
                vscode.window
                    .showWarningMessage(
                        'Inference toolchain not found. Install it first.',
                        'Install',
                    )
                    .then((action) => {
                        if (action === 'Install') {
                            vscode.commands.executeCommand(
                                'inference.installToolchain',
                            );
                        }
                    });
                return;
            }

            updating = true;
            try {
                await checkForUpdatesImpl(infsPath, outputChannel, true);
            } finally {
                updating = false;
            }
        },
    );
}

/**
 * Check for toolchain updates on activation.
 * Respects the `inference.checkForUpdates` setting.
 * This is a no-op if checks are disabled.
 */
export async function checkForUpdates(
    infsPath: string,
    outputChannel: vscode.OutputChannel,
): Promise<void> {
    if (updating) {
        return;
    }
    const settings = getSettings();
    if (!settings.checkForUpdates) {
        return;
    }
    updating = true;
    try {
        await checkForUpdatesImpl(infsPath, outputChannel, false);
    } finally {
        updating = false;
    }
}

async function checkForUpdatesImpl(
    infsPath: string,
    outputChannel: vscode.OutputChannel,
    userInitiated: boolean,
): Promise<void> {
    const currentVersion = await getCurrentVersion(infsPath);
    if (!currentVersion) {
        outputChannel.appendLine('Update check: could not determine current version.');
        if (userInitiated) {
            vscode.window.showErrorMessage(
                'Inference: Could not determine the current toolchain version.',
            );
        }
        return;
    }

    outputChannel.appendLine(`Update check: current version is ${currentVersion}.`);

    const versions = await fetchVersions(infsPath);
    if (!versions) {
        outputChannel.appendLine('Update check: failed to fetch available versions.');
        if (userInitiated) {
            vscode.window.showErrorMessage(
                'Inference: Failed to check for updates.',
            );
        }
        return;
    }

    const candidates = versions
        .filter((v) => v.available_for_current);

    if (candidates.length === 0) {
        outputChannel.appendLine('Update check: no versions available for this platform.');
        if (userInitiated) {
            vscode.window.showInformationMessage(
                'Inference: No toolchain versions available for this platform.',
            );
        }
        return;
    }

    const sorted = [...candidates].sort((a, b) =>
        compareSemver(b.version, a.version),
    );
    const latest = sorted[0];

    if (compareSemver(currentVersion, latest.version) >= 0) {
        outputChannel.appendLine(
            `Update check: toolchain is up to date (v${currentVersion}).`,
        );
        if (userInitiated) {
            vscode.window.showInformationMessage(
                `Inference toolchain is up to date (v${currentVersion}).`,
            );
        }
        return;
    }

    outputChannel.appendLine(
        `Update check: v${latest.version} available (current: v${currentVersion}).`,
    );

    const action = await vscode.window.showInformationMessage(
        `Inference toolchain update available: v${latest.version} (current: v${currentVersion})`,
        'Update',
        'Release Notes',
    );

    if (action === 'Update') {
        await performVersionChange(infsPath, latest.version, outputChannel, 'Updating to');
    } else if (action === 'Release Notes') {
        vscode.env.openExternal(
            vscode.Uri.parse(
                `https://github.com/Inferara/inference/releases/tag/v${latest.version}`,
            ),
        );
    }
}
