import * as vscode from 'vscode';
import { detectPlatform, PlatformInfo } from '../toolchain/platform';
import {
    installToolchain,
    InstallProgress,
    InstallProgressCallback,
    InstallResult,
} from '../toolchain/installation';

/** Guard against concurrent install attempts. */
let installing = false;

/**
 * Register the inference.installToolchain command.
 * Returns the Disposable to add to context.subscriptions.
 */
export function registerInstallCommand(
    outputChannel: vscode.OutputChannel,
): vscode.Disposable {
    return vscode.commands.registerCommand(
        'inference.installToolchain',
        async () => {
            if (installing) {
                vscode.window.showInformationMessage(
                    'Inference toolchain installation is already in progress.',
                );
                return;
            }

            const platform = detectPlatform();
            if (!platform) {
                vscode.window.showErrorMessage(
                    `Inference: unsupported platform (${process.platform}-${process.arch}).`,
                );
                return;
            }

            installing = true;
            try {
                const result = await installWithProgress(
                    platform,
                    outputChannel,
                );
                outputChannel.appendLine(
                    `Toolchain v${result.version} installed at ${result.infsPath}`,
                );
                vscode.commands.executeCommand(
                    'setContext', 'inference.toolchainInstalled', true,
                );
                vscode.commands.executeCommand('inference.runDoctor');
                notifyInstallSuccess(result.version, result.doctorWarnings);
            } catch (err) {
                const message =
                    err instanceof Error ? err.message : String(err);
                outputChannel.appendLine(`Installation failed: ${message}`);
                notifyInstallError(message);
            } finally {
                installing = false;
            }
        },
    );
}

/** Run the installation with a VS Code progress notification. */
function installWithProgress(
    platform: PlatformInfo,
    outputChannel: vscode.OutputChannel,
): Promise<InstallResult> {
    return vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: 'Inference Toolchain',
            cancellable: false,
        },
        async (progress) => {
            const onProgress: InstallProgressCallback = (
                p: InstallProgress,
            ) => {
                outputChannel.appendLine(p.message);
                if (p.stage === 'downloading' && p.bytesTotal) {
                    const pct = Math.round(
                        ((p.bytesReceived ?? 0) / p.bytesTotal) * 100,
                    );
                    progress.report({ message: `${p.message} (${pct}%)` });
                } else {
                    progress.report({ message: p.message });
                }
            };
            return installToolchain(platform, onProgress);
        },
    );
}

/** Show a notification that the toolchain was installed successfully. */
function notifyInstallSuccess(
    version: string,
    doctorWarnings: boolean,
): void {
    if (doctorWarnings) {
        vscode.window
            .showWarningMessage(
                `Inference toolchain v${version} installed, but doctor reported issues. See output for details.`,
                'Show Output',
            )
            .then((action) => {
                if (action === 'Show Output') {
                    vscode.commands.executeCommand('inference.showOutput');
                }
            });
    } else {
        vscode.window
            .showInformationMessage(
                `Inference toolchain v${version} installed successfully.`,
                'Show Output',
            )
            .then((action) => {
                if (action === 'Show Output') {
                    vscode.commands.executeCommand('inference.showOutput');
                }
            });
    }
}

/** Show an error notification for installation failure. */
function notifyInstallError(errorMessage: string): void {
    vscode.window
        .showErrorMessage(
            `Inference toolchain installation failed: ${errorMessage}`,
            'Retry',
            'Download Manually',
            'Settings',
        )
        .then((action) => {
            if (action === 'Retry') {
                vscode.commands.executeCommand('inference.installToolchain');
            } else if (action === 'Download Manually') {
                vscode.env.openExternal(
                    vscode.Uri.parse(
                        'https://github.com/Inferara/inference/releases',
                    ),
                );
            } else if (action === 'Settings') {
                vscode.commands.executeCommand(
                    'workbench.action.openSettings',
                    'inference.path',
                );
            }
        });
}
