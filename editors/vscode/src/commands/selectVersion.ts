import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { fetchVersions, getCurrentVersion, installAndSetDefault } from '../toolchain/versions';
import { compareSemver } from '../utils/semver';

/** Guard against concurrent select operations. */
let selecting = false;

/**
 * Register the inference.selectVersion command.
 * Shows a QuickPick with available toolchain versions and switches to the selected one.
 */
export function registerSelectVersionCommand(
    outputChannel: vscode.OutputChannel,
): vscode.Disposable {
    return vscode.commands.registerCommand(
        'inference.selectVersion',
        async () => {
            if (selecting) {
                vscode.window.showInformationMessage(
                    'Version selection is already in progress.',
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

            selecting = true;
            try {
                const versions = await fetchVersions(infsPath);
                if (!versions) {
                    vscode.window.showErrorMessage(
                        'Inference: Failed to fetch available versions.',
                    );
                    return;
                }

                const currentVersion = await getCurrentVersion(infsPath);

                // Filter to versions available for current platform and sort by semver descending
                const available = versions
                    .filter((v) => v.available_for_current)
                    .sort((a, b) => compareSemver(b.version, a.version));

                if (available.length === 0) {
                    vscode.window.showInformationMessage(
                        'No toolchain versions available for this platform.',
                    );
                    return;
                }

                const items: vscode.QuickPickItem[] = available.map((v) => {
                    const tags: string[] = [];
                    if (v.version === currentVersion) {
                        tags.push('current');
                    }
                    if (v.stable) {
                        tags.push('stable');
                    }
                    return {
                        label: v.version,
                        description: tags.length > 0 ? `(${tags.join(', ')})` : undefined,
                    };
                });

                if (currentVersion) {
                    const idx = items.findIndex((i) => i.label === currentVersion);
                    if (idx > 0) {
                        const [item] = items.splice(idx, 1);
                        items.unshift(item);
                    }
                }

                const picked = await vscode.window.showQuickPick(items, {
                    placeHolder: 'Select toolchain version',
                    matchOnDescription: true,
                });

                if (!picked) {
                    return;
                }

                const selectedVersion = picked.label;
                if (selectedVersion === currentVersion) {
                    vscode.window.showInformationMessage(
                        `Already using toolchain v${selectedVersion}.`,
                    );
                    return;
                }

                await switchVersion(infsPath, selectedVersion, outputChannel);
            } finally {
                selecting = false;
            }
        },
    );
}

async function switchVersion(
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
            progress.report({ message: `Switching to v${version}...` });
            outputChannel.appendLine(`Switching to toolchain v${version}...`);

            const result = await installAndSetDefault(infsPath, version);

            if (result.success) {
                outputChannel.appendLine(`Switched to toolchain v${version}.`);
                vscode.window
                    .showInformationMessage(
                        `Switched to Inference toolchain v${version}.`,
                        'Show Output',
                    )
                    .then((action) => {
                        if (action === 'Show Output') {
                            outputChannel.show();
                        }
                    });
                return;
            }

            outputChannel.appendLine(`Version switch failed: ${result.error}`);

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
