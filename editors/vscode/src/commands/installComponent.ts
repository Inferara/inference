import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { exec, ExecResult } from '../utils/exec';
import {
    ComponentName,
    KNOWN_COMPONENTS,
    componentAddArgs,
} from '../toolchain/components';

/** Guard against concurrent component install attempts. */
let installing = false;

/**
 * Timeout for a component install. Managed downloads can be ~100 MB, far
 * beyond the default exec timeout, so allow up to ten minutes.
 */
const INSTALL_TIMEOUT_MS = 600_000;

/**
 * Register the inference.installComponent command.
 * Returns the Disposable to add to context.subscriptions.
 */
export function registerInstallComponentCommand(
    outputChannel: vscode.OutputChannel,
): vscode.Disposable {
    return vscode.commands.registerCommand(
        'inference.installComponent',
        async (component: string = 'wasm-opt') => {
            if (installing) {
                vscode.window.showInformationMessage(
                    'Inference component installation is already in progress.',
                );
                return;
            }

            if (!isKnownComponent(component)) {
                vscode.window.showErrorMessage(
                    `Inference: unknown component '${component}'.`,
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

            installing = true;
            try {
                const result = await installWithProgress(
                    detection.path,
                    component,
                    outputChannel,
                );
                if (result.stdout) {
                    outputChannel.appendLine(result.stdout);
                }
                if (result.stderr) {
                    outputChannel.appendLine(result.stderr);
                }

                if (result.exitCode === 0) {
                    vscode.window.showInformationMessage(
                        `Inference: component '${component}' installed.`,
                    );
                    vscode.commands.executeCommand('inference.runDoctor');
                } else {
                    notifyInstallError(component);
                }
            } catch (err) {
                const message =
                    err instanceof Error ? err.message : String(err);
                outputChannel.appendLine(
                    `Component installation failed: ${message}`,
                );
                notifyInstallError(component);
            } finally {
                installing = false;
            }
        },
    );
}

/** Type guard: whether a string is a known component name. */
function isKnownComponent(name: string): name is ComponentName {
    return (KNOWN_COMPONENTS as readonly string[]).includes(name);
}

/** Run `infs component add <component>` with a VS Code progress notification. */
function installWithProgress(
    infsPath: string,
    component: ComponentName,
    outputChannel: vscode.OutputChannel,
): Thenable<ExecResult> {
    return vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: 'Inference Component',
            cancellable: false,
        },
        async (progress) => {
            progress.report({ message: `Installing ${component}...` });
            outputChannel.appendLine(`Installing component '${component}'...`);
            return exec(infsPath, componentAddArgs(component), {
                timeoutMs: INSTALL_TIMEOUT_MS,
            });
        },
    );
}

/** Show an error notification for component installation failure. */
function notifyInstallError(component: ComponentName): void {
    vscode.window
        .showErrorMessage(
            `Inference: failed to install component '${component}'. See output for details.`,
            'Show Output',
            'Retry',
        )
        .then((action) => {
            if (action === 'Show Output') {
                vscode.commands.executeCommand('inference.showOutput');
            } else if (action === 'Retry') {
                vscode.commands.executeCommand(
                    'inference.installComponent',
                    component,
                );
            }
        });
}
