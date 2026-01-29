import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { fetchVersions, getCurrentVersion, installAndSetDefault } from '../toolchain/versions';
import { compareSemver } from '../utils/semver';
import { getSettings } from '../config/settings';

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
 * Respects the `inference.checkForUpdates` and `inference.channel` settings.
 * This is a no-op if checks are disabled.
 */
export async function checkForUpdates(
    infsPath: string,
    outputChannel: vscode.OutputChannel,
): Promise<void> {
    const settings = getSettings();
    if (!settings.checkForUpdates) {
        return;
    }
    if (settings.channel === 'none') {
        return;
    }
    await checkForUpdatesImpl(infsPath, outputChannel, false);
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

    const settings = getSettings();
    const channel =
        settings.channel === 'stable' || settings.channel === 'latest'
            ? settings.channel
            : 'stable';

    const candidates = versions
        .filter((v) => v.available_for_current)
        .filter((v) => (channel === 'stable' ? v.stable : true));

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
        await performUpdate(infsPath, latest.version, outputChannel);
    } else if (action === 'Release Notes') {
        vscode.env.openExternal(
            vscode.Uri.parse(
                `https://github.com/Inferara/inference/releases/tag/v${latest.version}`,
            ),
        );
    }
}

async function performUpdate(
    infsPath: string,
    version: string,
    outputChannel: vscode.OutputChannel,
): Promise<void> {
    await vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: 'Inference Toolchain',
            cancellable: false,
        },
        async (progress) => {
            progress.report({ message: `Updating to v${version}...` });
            outputChannel.appendLine(`Updating to toolchain v${version}...`);

            const result = await installAndSetDefault(infsPath, version);

            if (result.success) {
                outputChannel.appendLine(`Updated to toolchain v${version}.`);
                vscode.window
                    .showInformationMessage(
                        `Inference toolchain updated to v${version}.`,
                        'Show Output',
                    )
                    .then((action) => {
                        if (action === 'Show Output') {
                            outputChannel.show();
                        }
                    });
                return;
            }

            outputChannel.appendLine(`Update failed: ${result.error}`);

            if (result.installedButNotDefault) {
                vscode.window
                    .showWarningMessage(
                        `Inference: v${version} was installed but could not be set as default. Run \`infs default ${version}\` manually.`,
                        'Show Output',
                    )
                    .then((action) => {
                        if (action === 'Show Output') {
                            outputChannel.show();
                        }
                    });
            } else {
                vscode.window.showErrorMessage(
                    `Inference: Failed to install v${version}: ${result.error}`,
                );
            }
        },
    );
}
