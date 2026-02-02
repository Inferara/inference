import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { fetchVersions, getCurrentVersion } from '../toolchain/versions';
import { buildVersionPickItems } from '../toolchain/versionPicker';
import { performVersionChange } from './versionChange';

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

            selecting = true;
            try {
                const versions = await fetchVersions(detection.path);
                if (!versions) {
                    vscode.window.showErrorMessage(
                        'Inference: Failed to fetch available versions.',
                    );
                    return;
                }

                const currentVersion = await getCurrentVersion(detection.path);

                const items = buildVersionPickItems(versions, currentVersion);

                if (items.length === 0) {
                    vscode.window.showInformationMessage(
                        'No toolchain versions available for this platform.',
                    );
                    return;
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

                await performVersionChange(detection.path, selectedVersion, outputChannel, 'Switching to');
            } finally {
                selecting = false;
            }
        },
    );
}
