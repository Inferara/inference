import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';
import { getSettings } from '../config/settings';
import { isExecutable } from '../toolchain/detection';
import { inferenceHome } from '../toolchain/home';
import { lspActionForConfigChange, resolveLspBinary } from './resolve';

/**
 * Lifecycle management for the `inference-lsp` language server client.
 *
 * A single LanguageClient instance is managed at module level. All lifecycle
 * operations (start/stop/restart) are serialized through an internal promise
 * queue so overlapping triggers (activation, configuration changes, the
 * restart command, post-install retries) cannot race each other.
 *
 * When the server binary cannot be found the client stays stopped QUIETLY:
 * a line is written to the main Inference output channel, but no user-facing
 * notification is shown (the toolchain check already owns that conversation).
 */

let mainChannel: vscode.LogOutputChannel | undefined;
let serverChannel: vscode.OutputChannel | undefined;
let client: LanguageClient | undefined;
let queue: Promise<void> = Promise.resolve();

/** Serialize a lifecycle operation behind all previously queued ones. */
function enqueue(operation: () => Promise<void>): Promise<void> {
    const next = queue.then(operation);
    queue = next.catch(() => undefined);
    return next;
}

/**
 * Initialize the language client module. Must be called once during
 * activation, before any other function in this module.
 */
export function initializeLspClient(
    context: vscode.ExtensionContext,
    outputChannel: vscode.LogOutputChannel,
): void {
    mainChannel = outputChannel;
    serverChannel = vscode.window.createOutputChannel(
        'Inference Language Server',
        { log: true },
    );
    context.subscriptions.push(serverChannel);
    context.subscriptions.push(
        new vscode.Disposable(() => {
            void stopLspClient();
        }),
    );
}

/** Whether a language client is currently active. */
export function isLspRunning(): boolean {
    return client !== undefined;
}

/**
 * Start the language client if it is enabled and not already running.
 * Quiet when the binary is missing: logs to the output channel only.
 */
export function startLspClient(): Promise<void> {
    return enqueue(() => doStart());
}

/** Stop the language client if it is running. */
export function stopLspClient(): Promise<void> {
    return enqueue(() => doStop());
}

/** Restart the language client, re-resolving the server binary. */
export function restartLspClient(): Promise<void> {
    return enqueue(async () => {
        await doStop();
        await doStart();
    });
}

/**
 * Start the language client if it is not running yet. Called after a
 * successful toolchain install/update so the server comes up without a
 * window reload. Never rejects; failures are logged.
 */
export function ensureLspStarted(): Promise<void> {
    return enqueue(async () => {
        if (client) {
            return;
        }
        await doStart();
    }).catch((err) => {
        mainChannel?.error(`Language server start failed: ${err}`);
    });
}

/**
 * React to a configuration change event: restart on any `inference.lsp.*`
 * change while enabled, stop when disabled.
 */
export function handleLspConfigChange(
    event: vscode.ConfigurationChangeEvent,
): Promise<void> {
    const action = lspActionForConfigChange({
        affectsLsp: event.affectsConfiguration('inference.lsp'),
        enabled: getSettings().lspEnabled,
        running: isLspRunning(),
    });
    switch (action) {
        case 'restart':
            return restartLspClient();
        case 'stop':
            return stopLspClient();
        case 'none':
            return Promise.resolve();
    }
}

async function doStart(): Promise<void> {
    if (client) {
        return;
    }

    const settings = getSettings();
    if (!settings.lspEnabled) {
        mainChannel?.info(
            'Language server disabled (inference.lsp.enabled is false).',
        );
        return;
    }

    const resolution = resolveLspBinary({
        configuredPath: settings.lspPath,
        inferenceHome: inferenceHome(),
        isWindows: process.platform === 'win32',
        envPath: process.env['PATH'] || '',
        isExecutable,
    });

    if (!resolution) {
        if (settings.lspPath) {
            mainChannel?.warn(
                `Language server not started: inference.lsp.path is set to ${settings.lspPath} but it is not executable.`,
            );
        } else {
            mainChannel?.info(
                `Language server not started: inference-lsp not found (searched ${inferenceHome()}/bin and PATH). Install or update the toolchain to enable it.`,
            );
        }
        return;
    }

    const serverOptions: ServerOptions = {
        command: resolution.path,
        transport: TransportKind.stdio,
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'inference' }],
        outputChannel: serverChannel,
    };
    const candidate = new LanguageClient(
        'inference-lsp',
        'Inference Language Server',
        serverOptions,
        clientOptions,
    );

    try {
        await candidate.start();
    } catch (err) {
        mainChannel?.error(
            `Language server failed to start (${resolution.path}): ${err}`,
        );
        await candidate.dispose().catch(() => undefined);
        return;
    }

    client = candidate;
    mainChannel?.info(
        `Language server started: ${resolution.path} (${resolution.source})`,
    );
}

async function doStop(): Promise<void> {
    if (!client) {
        return;
    }
    const stopping = client;
    client = undefined;
    try {
        await stopping.stop();
        mainChannel?.info('Language server stopped.');
    } catch (err) {
        mainChannel?.warn(`Language server stop failed: ${err}`);
    } finally {
        await stopping.dispose().catch(() => undefined);
    }
}
