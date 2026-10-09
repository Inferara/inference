import * as vscode from 'vscode';
import { installAndSetDefault } from '../toolchain/versions';
import { restartLspClient } from '../lsp/client';

/**
 * Perform a version change (install + set default) with progress UI.
 * `actionVerb` words the progress (e.g. "Switching to"). Resolves true when
 * the version is installed and is the default.
 *
 * Shared by "Select Version", "Update Toolchain" and "Prove This File".
 */
export async function performVersionChange(
    infsPath: string,
    version: string,
    outputChannel: vscode.OutputChannel,
    actionVerb: string,
): Promise<boolean> {
    return vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: 'Inference Toolchain',
            cancellable: false,
        },
        async (progress) => {
            progress.report({ message: `${actionVerb} v${version}...` });
            outputChannel.appendLine(`${actionVerb} toolchain v${version}...`);

            const result = await installAndSetDefault(infsPath, version);

            if (result.success) {
                outputChannel.appendLine(
                    `${actionVerb} toolchain v${version} complete.`,
                );
                vscode.commands.executeCommand(
                    'setContext', 'inference.toolchainInstalled', true,
                );
                vscode.commands.executeCommand('inference.applyTerminalPath');
                vscode.commands.executeCommand('inference.runDoctor');
                // Restart (not merely ensure-started): a running server
                // still executes the old toolchain's binary, and stop is a
                // no-op when the server is not running.
                restartLspClient().catch((err) =>
                    outputChannel.appendLine(
                        `Language server restart failed: ${err}`,
                    ),
                );
                vscode.window
                    .showInformationMessage(
                        `Inference toolchain v${version} is now the default.`,
                        'Show Output',
                    )
                    .then((action) => {
                        if (action === 'Show Output') {
                            outputChannel.show();
                        }
                    });
                return true;
            }

            outputChannel.appendLine(
                `${actionVerb} failed: ${result.error}`,
            );

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
            return false;
        },
    );
}
