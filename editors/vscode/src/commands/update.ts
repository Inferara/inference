import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { fetchVersions, getCurrentVersion } from '../toolchain/versions';
import { checkUpdateAvailable } from '../toolchain/updateCheck';
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

            const detection = detectInfs();
            if (!detection) {
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
                await checkForUpdatesImpl(detection.path, outputChannel, true);
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

    const result = checkUpdateAvailable(currentVersion, versions);

    switch (result.status) {
        case 'no-current-version':
            outputChannel.appendLine('Update check: could not determine current version.');
            if (userInitiated) {
                vscode.window.showErrorMessage(
                    'Inference: Could not determine the current toolchain version.',
                );
            }
            return;

        case 'no-versions':
            outputChannel.appendLine('Update check: no versions available for this platform.');
            if (userInitiated) {
                vscode.window.showInformationMessage(
                    'Inference: No toolchain versions available for this platform.',
                );
            }
            return;

        case 'up-to-date':
            outputChannel.appendLine(
                `Update check: toolchain is up to date (v${result.version}).`,
            );
            if (userInitiated) {
                vscode.window.showInformationMessage(
                    `Inference toolchain is up to date (v${result.version}).`,
                );
            }
            return;

        case 'update-available': {
            outputChannel.appendLine(
                `Update check: v${result.latest} available (current: v${result.current}).`,
            );

            const action = await vscode.window.showInformationMessage(
                `Inference toolchain update available: v${result.latest} (current: v${result.current})`,
                'Update',
                'Release Notes',
            );

            if (action === 'Update') {
                await performVersionChange(infsPath, result.latest, outputChannel, 'Updating to');
            } else if (action === 'Release Notes') {
                vscode.env.openExternal(
                    vscode.Uri.parse(
                        `https://github.com/Inferara/inference/releases/tag/v${result.latest}`,
                    ),
                );
            }
            return;
        }
    }
}
