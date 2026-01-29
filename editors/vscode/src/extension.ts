import * as vscode from 'vscode';
import { detectPlatform } from './toolchain/platform';
import { detectInfs } from './toolchain/detection';
import { getSettings } from './config/settings';
import { exec } from './utils/exec';
import { compareSemver } from './utils/semver';
import { registerInstallCommand } from './commands/install';
import { registerDoctorCommand } from './commands/doctor';
import { registerSelectVersionCommand } from './commands/selectVersion';
import { registerUpdateCommand, checkForUpdates } from './commands/update';
import { createStatusBar, updateStatusBar } from './ui/statusBar';
import { runDoctor } from './toolchain/doctor';

/** Minimum infs CLI version the extension can work with. */
const MIN_INFS_VERSION = '0.0.1-beta.1';

const outputChannel = vscode.window.createOutputChannel('Inference');

export function activate(context: vscode.ExtensionContext) {
    context.subscriptions.push(outputChannel);

    const statusBarItem = createStatusBar();
    context.subscriptions.push(statusBarItem);

    context.subscriptions.push(
        vscode.commands.registerCommand('inference.showOutput', () => {
            outputChannel.show();
        }),
    );

    context.subscriptions.push(registerInstallCommand(outputChannel));
    context.subscriptions.push(
        registerDoctorCommand(outputChannel, statusBarItem),
    );

    context.subscriptions.push(registerUpdateCommand(outputChannel));
    context.subscriptions.push(registerSelectVersionCommand(outputChannel));

    checkToolchain(statusBarItem).catch((err) =>
        outputChannel.appendLine(`Toolchain check failed: ${err}`),
    );
}

export function deactivate() {
    // Nothing to clean up
}

async function checkToolchain(
    statusBarItem: vscode.StatusBarItem,
): Promise<void> {
    const platform = detectPlatform();
    if (!platform) {
        outputChannel.appendLine(
            `Unsupported platform: ${process.platform}-${process.arch}`,
        );
        updateStatusBar(statusBarItem, null);
        vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', false);
        vscode.window
            .showWarningMessage(
                `Inference: unsupported platform (${process.platform}-${process.arch}).`,
                'Download Page',
            )
            .then((action) => {
                if (action === 'Download Page') {
                    vscode.env.openExternal(
                        vscode.Uri.parse(
                            'https://github.com/Inferara/inference/releases',
                        ),
                    );
                }
            });
        return;
    }
    outputChannel.appendLine(`Platform: ${platform.id}`);

    const infsPath = detectInfs();
    if (!infsPath) {
        outputChannel.appendLine('infs binary not found.');
        updateStatusBar(statusBarItem, null);
        vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', false);
        const settings = getSettings();
        if (settings.autoInstall) {
            notifyMissing();
        }
        return;
    }
    outputChannel.appendLine(`infs found: ${infsPath}`);

    const versionOk = await checkInfsVersion(infsPath);
    if (!versionOk) {
        updateStatusBar(statusBarItem, null);
        vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', false);
        return;
    }

    outputChannel.appendLine('Toolchain detection complete.');
    vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', true);

    const doctorResult = await runDoctor(infsPath);
    updateStatusBar(statusBarItem, doctorResult);

    checkForUpdates(infsPath, outputChannel).catch((err) =>
        outputChannel.appendLine(`Update check failed: ${err}`),
    );
}

/**
 * Run `infs version` and check the output against MIN_INFS_VERSION.
 * Returns true if version is acceptable.
 */
async function checkInfsVersion(infsPath: string): Promise<boolean> {
    try {
        const result = await exec(infsPath, ['version']);
        if (result.exitCode !== 0) {
            outputChannel.appendLine(
                `infs version failed (exit ${result.exitCode}): ${result.stderr}`,
            );
            return false;
        }
        // Output format: "infs 0.1.0"
        const match = result.stdout.match(/^infs\s+(\S+)/);
        if (!match) {
            outputChannel.appendLine(
                `Could not parse infs version from: ${result.stdout.trim()}`,
            );
            return false;
        }
        const version = match[1];
        outputChannel.appendLine(`infs version: ${version}`);

        if (compareSemver(version, MIN_INFS_VERSION) < 0) {
            outputChannel.appendLine(
                `infs version ${version} is below minimum ${MIN_INFS_VERSION}.`,
            );
            vscode.window.showWarningMessage(
                `Inference: infs version ${version} is outdated (minimum: ${MIN_INFS_VERSION}). Please update.`,
                'Update',
            );
            return false;
        }
        return true;
    } catch (err) {
        outputChannel.appendLine(`Failed to run infs version: ${err}`);
        return false;
    }
}

function notifyMissing(): void {
    vscode.window
        .showInformationMessage(
            'Inference toolchain not found. Would you like to install it?',
            'Install',
            'Download Manually',
            'Configure Path',
        )
        .then((action) => {
            if (action === 'Install') {
                vscode.commands.executeCommand('inference.installToolchain');
            } else if (action === 'Download Manually') {
                vscode.env.openExternal(
                    vscode.Uri.parse(
                        'https://github.com/Inferara/inference/releases',
                    ),
                );
            } else if (action === 'Configure Path') {
                vscode.commands.executeCommand(
                    'workbench.action.openSettings',
                    'inference.path',
                );
            }
        });
}
