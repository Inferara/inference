import * as vscode from 'vscode';
import * as path from 'path';
import { detectPlatform } from './toolchain/platform';
import { detectInfs, inferenceHome } from './toolchain/detection';
import { exec } from './utils/exec';
import { compareSemver } from './utils/semver';
import { registerInstallCommand } from './commands/install';
import { registerDoctorCommand } from './commands/doctor';
import { registerSelectVersionCommand } from './commands/selectVersion';
import { registerUpdateCommand, checkForUpdates } from './commands/update';
import { createStatusBar, updateStatusBar } from './ui/statusBar';
import { InferenceConfigProvider, ConfigItem } from './ui/configTree';
import { runDoctor } from './toolchain/doctor';

/** Minimum infs CLI version the extension can work with. */
const MIN_INFS_VERSION = '0.0.1-beta.1';

const outputChannel = vscode.window.createOutputChannel('Inference', { log: true });

export function activate(context: vscode.ExtensionContext) {
    context.subscriptions.push(outputChannel);

    const statusBarItem = createStatusBar();
    context.subscriptions.push(statusBarItem);

    context.subscriptions.push(
        vscode.commands.registerCommand('inference.showOutput', () => {
            outputChannel.show();
        }),
    );

    context.subscriptions.push(registerInstallCommand(outputChannel, statusBarItem));
    context.subscriptions.push(
        registerDoctorCommand(outputChannel, statusBarItem),
    );

    context.subscriptions.push(registerUpdateCommand(outputChannel));
    context.subscriptions.push(registerSelectVersionCommand(outputChannel));

    const configProvider = new InferenceConfigProvider();
    const configView = vscode.window.createTreeView('inference.configView', {
        treeDataProvider: configProvider,
    });
    context.subscriptions.push(configView);
    context.subscriptions.push(configProvider);

    context.subscriptions.push(
        vscode.commands.registerCommand('inference.refreshConfigView', () => {
            configProvider.refresh();
        }),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('inference.applyTerminalPath', () => {
            applyTerminalPath(context);
        }),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand(
            'inference.copyConfigValue',
            (item: ConfigItem) => {
                if (item.copyValue) {
                    vscode.env.clipboard.writeText(item.copyValue);
                    vscode.window.showInformationMessage(
                        `Copied: ${item.copyValue}`,
                    );
                }
            },
        ),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand(
            'inference.revealConfigPath',
            (item: ConfigItem) => {
                if (item.copyValue) {
                    vscode.commands.executeCommand(
                        'revealFileInOS',
                        vscode.Uri.file(item.copyValue),
                    );
                }
            },
        ),
    );

    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('inference')) {
                configProvider.refresh();
            }
        }),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('inference.resetPathAcceptance', () => {
            const home = inferenceHome();
            const stateKey = `${PATH_FALLBACK_KEY}:${home}`;
            context.globalState.update(stateKey, undefined);
            vscode.window.showInformationMessage(
                'Inference: PATH fallback preference has been reset.',
            );
        }),
    );

    applyTerminalPath(context);

    checkToolchain(context, statusBarItem, configProvider).catch((err) =>
        outputChannel.error(`Toolchain check failed: ${err}`),
    );
}

export function deactivate() {
    // Nothing to clean up
}

/**
 * Prepend `INFERENCE_HOME/bin` to PATH for all integrated terminals.
 *
 * Uses VS Code's `EnvironmentVariableCollection` so that:
 * - New terminals automatically have the toolchain on PATH.
 * - Existing terminals show a relaunch indicator when the collection changes.
 *
 * Exported so that install/update commands can re-apply the mutation to
 * trigger the relaunch indicator on already-open terminals.
 */
export function applyTerminalPath(context: vscode.ExtensionContext): void {
    const binDir = path.join(inferenceHome(), 'bin');
    const sep = process.platform === 'win32' ? ';' : ':';
    const env = context.environmentVariableCollection;
    env.prepend('PATH', binDir + sep);
    env.description = 'Adds the Inference toolchain to PATH';
}

/** globalState key prefix for remembering PATH fallback acceptance. */
const PATH_FALLBACK_KEY = 'inference.acceptedPathFallback';

async function checkToolchain(
    context: vscode.ExtensionContext,
    statusBarItem: vscode.StatusBarItem,
    configProvider: InferenceConfigProvider,
): Promise<void> {
    const platform = detectPlatform();
    const home = inferenceHome();
    const homeIsDefault = !process.env['INFERENCE_HOME'];
    const distServer = process.env['INFS_DIST_SERVER'];

    outputChannel.info('Inference Activation');

    if (!platform) {
        outputChannel.warn(
            `Platform:         ${process.platform}-${process.arch} (unsupported)`,
        );
        outputChannel.info(
            `INFERENCE_HOME:   ${home} ${homeIsDefault ? '(default)' : '(env)'}`,
        );
        outputChannel.info(
            `INFS_DIST_SERVER: ${distServer ?? '(not set, using production)'}`,
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

    outputChannel.info(`Platform:         ${platform.id}`);
    outputChannel.info(
        `INFERENCE_HOME:   ${home} ${homeIsDefault ? '(default)' : '(env)'}`,
    );
    outputChannel.info(
        `INFS_DIST_SERVER: ${distServer ?? '(not set, using production)'}`,
    );

    const detection = detectInfs();
    if (!detection) {
        outputChannel.error('infs binary:      not found');
        outputChannel.error('Toolchain status: errors');
        updateStatusBar(statusBarItem, null);
        vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', false);
        notifyMissing();
        return;
    }
    outputChannel.info(
        `infs binary:      ${detection.path} (${detection.source})`,
    );

    if (!homeIsDefault && detection.source === 'path') {
        const stateKey = `${PATH_FALLBACK_KEY}:${home}`;
        const accepted = context.globalState.get<string>(stateKey);

        if (accepted === '*') {
            outputChannel.info(
                `Note: Using PATH binary (notification permanently suppressed for this INFERENCE_HOME).`,
            );
        } else if (accepted === detection.path) {
            outputChannel.info(
                `Note: Using PATH binary (previously accepted for this INFERENCE_HOME).`,
            );
        } else {
            outputChannel.warn(
                `INFERENCE_HOME is set to ${home} but infs was not found there. Using binary from PATH instead.`,
            );
            vscode.window.showWarningMessage(
                `Inference: infs binary not found in INFERENCE_HOME (${home}). Found via PATH instead.`,
                'Install',
                'Dismiss',
            ).then((action) => {
                if (action === 'Install') {
                    vscode.commands.executeCommand('inference.installToolchain');
                } else {
                    context.globalState.update(stateKey, '*');
                }
            });
        }
    }

    const versionOk = await checkInfsVersion(detection.path);
    if (!versionOk) {
        outputChannel.error('Toolchain status: errors');
        updateStatusBar(statusBarItem, null);
        vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', false);
        return;
    }

    vscode.commands.executeCommand('setContext', 'inference.toolchainInstalled', true);

    const doctorResult = await runDoctor(detection.path);
    updateStatusBar(statusBarItem, doctorResult);
    configProvider.refresh(detection, doctorResult);

    const status = doctorResult?.hasErrors
        ? 'errors'
        : doctorResult?.hasWarnings
            ? 'warnings'
            : 'healthy';
    if (doctorResult?.hasErrors) {
        outputChannel.error(`Toolchain status: ${status}`);
    } else if (doctorResult?.hasWarnings) {
        outputChannel.warn(`Toolchain status: ${status}`);
    } else {
        outputChannel.info(`Toolchain status: ${status}`);
    }

    checkForUpdates(detection.path, outputChannel).catch((err) =>
        outputChannel.error(`Update check failed: ${err}`),
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
            outputChannel.error(
                `infs version failed (exit ${result.exitCode}): ${result.stderr}`,
            );
            return false;
        }
        // Output format: "infs 0.1.0"
        const match = result.stdout.match(/^infs\s+(\S+)/);
        if (!match) {
            outputChannel.error(
                `Could not parse infs version from: ${result.stdout.trim()}`,
            );
            return false;
        }
        const version = match[1];
        outputChannel.info(`infs version: ${version}`);

        if (compareSemver(version, MIN_INFS_VERSION) < 0) {
            outputChannel.warn(
                `infs version ${version} is below minimum ${MIN_INFS_VERSION}.`,
            );
            vscode.window
                .showWarningMessage(
                    `Inference: infs version ${version} is outdated (minimum: ${MIN_INFS_VERSION}). Please update.`,
                    'Update',
                )
                .then((action) => {
                    if (action === 'Update') {
                        vscode.commands.executeCommand('inference.updateToolchain');
                    }
                });
            return false;
        }
        return true;
    } catch (err) {
        outputChannel.error(`Failed to run infs version: ${err}`);
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
