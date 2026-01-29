import * as vscode from 'vscode';
import { detectInfs } from '../toolchain/detection';
import { runDoctor, DoctorResult } from '../toolchain/doctor';
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

            const infsPath = detectInfs();
            if (!infsPath) {
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
                    `Running infs doctor (${infsPath})...`,
                );
                const result = await runDoctor(infsPath);

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

                formatDoctorOutput(outputChannel, result);
                updateStatusBar(statusBarItem, result);

                if (result.hasErrors) {
                    vscode.window
                        .showErrorMessage(
                            `Inference doctor: ${result.summary}`,
                            'Show Output',
                        )
                        .then((action) => {
                            if (action === 'Show Output') {
                                outputChannel.show();
                            }
                        });
                } else if (result.hasWarnings) {
                    vscode.window
                        .showWarningMessage(
                            `Inference doctor: ${result.summary}`,
                            'Show Output',
                        )
                        .then((action) => {
                            if (action === 'Show Output') {
                                outputChannel.show();
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

const STATUS_TAGS: Record<string, string> = {
    ok: '[OK]  ',
    warn: '[WARN]',
    fail: '[FAIL]',
};

function formatDoctorOutput(
    outputChannel: vscode.OutputChannel,
    result: DoctorResult,
): void {
    outputChannel.appendLine('--- Doctor Report ---');
    for (const check of result.checks) {
        const tag = STATUS_TAGS[check.status] ?? `[${check.status.toUpperCase()}]`;
        outputChannel.appendLine(`  ${tag} ${check.name}: ${check.message}`);
    }
    if (result.summary) {
        outputChannel.appendLine('');
        outputChannel.appendLine(result.summary);
    }
    outputChannel.appendLine('---------------------');
}
