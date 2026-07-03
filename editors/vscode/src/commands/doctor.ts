import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { runDoctor, DoctorResult } from '../toolchain/doctor';
import { formatDoctorChecks } from '../toolchain/doctorFormat';
import { wasmOptNeedsAttention } from '../toolchain/components';
import { updateStatusBar } from '../ui/statusBar';

/** Guard against concurrent doctor runs. */
let running = false;

/**
 * Register the inference.runDoctor command.
 *
 * When invoked: detect infs → run doctor → display results in output
 * channel → update status bar → show notification summary.
 */
export function registerDoctorCommand(
    outputChannel: vscode.OutputChannel,
    statusBarItem: vscode.StatusBarItem,
): vscode.Disposable {
    return vscode.commands.registerCommand(
        'inference.runDoctor',
        async () => {
            if (running) {
                return;
            }

            const detection = detectInfs();
            if (!detection) {
                outputChannel.appendLine('Doctor: infs binary not found.');
                updateStatusBar(statusBarItem, null);
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

            running = true;
            try {
                outputChannel.appendLine(
                    `Running infs doctor (${detection.path})...`,
                );
                const result = await runDoctor(detection.path);

                if (!result) {
                    outputChannel.appendLine(
                        'Doctor: failed to execute infs doctor.',
                    );
                    updateStatusBar(statusBarItem, null);
                    vscode.window.showErrorMessage(
                        'Inference: Failed to run doctor. See output for details.',
                    );
                    return;
                }

                for (const line of formatDoctorChecks(result)) {
                    outputChannel.appendLine(line);
                }
                updateStatusBar(statusBarItem, result);
                vscode.commands.executeCommand('inference.refreshConfigView');

                if (result.hasErrors) {
                    const actions = ['Show Output'];
                    if (wasmOptNeedsAttention(result)) {
                        actions.push('Install wasm-opt');
                    }
                    vscode.window
                        .showErrorMessage(
                            `Inference doctor: ${result.summary}`,
                            ...actions,
                        )
                        .then((action) => {
                            if (action === 'Show Output') {
                                outputChannel.show();
                            } else if (action === 'Install wasm-opt') {
                                vscode.commands.executeCommand(
                                    'inference.installComponent',
                                    'wasm-opt',
                                );
                            }
                        });
                } else if (result.hasWarnings) {
                    const actions = ['Show Output'];
                    if (wasmOptNeedsAttention(result)) {
                        actions.push('Install wasm-opt');
                    }
                    vscode.window
                        .showWarningMessage(
                            `Inference doctor: ${result.summary}`,
                            ...actions,
                        )
                        .then((action) => {
                            if (action === 'Show Output') {
                                outputChannel.show();
                            } else if (action === 'Install wasm-opt') {
                                vscode.commands.executeCommand(
                                    'inference.installComponent',
                                    'wasm-opt',
                                );
                            }
                        });
                } else {
                    vscode.window.showInformationMessage(
                        'Inference: Toolchain is healthy.',
                    );
                }
            } finally {
                running = false;
            }
        },
    );
}

