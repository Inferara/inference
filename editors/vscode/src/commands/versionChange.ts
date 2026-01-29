import * as vscode from 'vscode';
import { installAndSetDefault } from '../toolchain/versions';

/**
 * Perform a version change (install + set default) with progress UI.
 *
 * Shared by both the "Select Version" and "Update Toolchain" commands.
 */
export async function performVersionChange(
    infsPath: string,
    version: string,
    outputChannel: vscode.OutputChannel,
    actionVerb: string,
): Promise<void> {
    await vscode.window.withProgress(
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
                vscode.commands.executeCommand('inference.runDoctor');
                vscode.window
                    .showInformationMessage(
                        `Inference toolchain ${actionVerb.toLowerCase()} to v${version}.`,
                        'Show Output',
                    )
                    .then((action) => {
                        if (action === 'Show Output') {
                            outputChannel.show();
                        }
                    });
                return;
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
        },
    );
}
