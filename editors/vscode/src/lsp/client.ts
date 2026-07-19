import * as path from 'path';
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
import { TimeoutError, withTimeout } from '../utils/timeout';
import { lspActionForConfigChange, resolveLspBinary } from './resolve';

/**
 * Lifecycle management for the `inference-lsp` language server client.
 *
 * A single LanguageClient instance is managed at module level. All lifecycle
 * operations (start/stop/restart) are serialized through an internal promise
 * queue so overlapping triggers (activation, configuration changes, the
 * restart command, post-install/toolchain-switch restarts) cannot race each
 * other.
 *
 * When the server binary cannot be found the client stays stopped QUIETLY:
 * a line is written to the main Inference output channel, but no user-facing
 * notification is shown (the toolchain check already owns that conversation).
 * A start that times out is the exception: a binary WAS found but never
 * answered the initialize handshake, which no other component reports, so it
 * is surfaced with a warning notification in addition to the log line.
 */

/**
 * How long a spawned server may take to complete the LSP initialize
 * handshake before the start attempt is abandoned and the process disposed.
 * vscode-languageclient's `start()` has no timeout of its own, so a binary
 * that spawns but never responds would otherwise block the serialized
 * lifecycle queue forever.
 */
const LSP_START_TIMEOUT_MS = 30_000;

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
 * React to a configuration change event: restart on any `inference.lsp.*`
 * change while enabled, stop when disabled.
 *
 * Only the event's `affectsConfiguration` answer is sampled at event time;
 * the enabled/running inputs of the decision are re-read when the queued
 * operation actually runs. Deciding at event time would race in-flight
 * lifecycle operations: the module-level `client` reflects only completed
 * queue work (it is assigned after `start()` resolves), so a disable
 * arriving while a start was still in flight used to see `running == false`,
 * skip the stop, and leave the server running against `enabled: false`.
 * Deferring the decision makes the last setting win regardless of
 * interleaving.
 */
export function handleLspConfigChange(
    event: vscode.ConfigurationChangeEvent,
): Promise<void> {
    if (!event.affectsConfiguration('inference.lsp')) {
        return Promise.resolve();
    }
    return enqueue(async () => {
        const action = lspActionForConfigChange({
            affectsLsp: true,
            enabled: getSettings().lspEnabled,
            running: client !== undefined,
        });
        switch (action) {
            case 'restart':
                await doStop();
                await doStart();
                return;
            case 'stop':
                await doStop();
                return;
            case 'none':
                return;
        }
    });
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
                `Language server not started: inference-lsp not found (searched ${path.join(inferenceHome(), 'bin')} and PATH). Install or update the toolchain to enable it.`,
            );
        }
        return;
    }

    const serverOptions: ServerOptions = {
        command: resolution.path,
        transport: TransportKind.stdio,
    };
    const clientOptions: LanguageClientOptions = {
        // File-scheme documents only, matching the server: its URI layer
        // deliberately ignores non-file schemes, so untitled buffers get no
        // analysis until saved as `.inf`. Advertising `untitled` here would
        // promise features the server cannot deliver.
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
        await withTimeout(
            candidate.start(),
            LSP_START_TIMEOUT_MS,
            `no response to the initialize request within ${LSP_START_TIMEOUT_MS / 1000}s`,
        );
    } catch (err) {
        mainChannel?.error(
            `Language server failed to start (${resolution.path}): ${err}`,
        );
        if (err instanceof TimeoutError) {
            notifyStartTimeout();
        }
        await candidate.dispose().catch(() => undefined);
        return;
    }

    client = candidate;
    mainChannel?.info(
        `Language server started: ${resolution.path} (${resolution.source})`,
    );
}

/**
 * Warn the user that a spawned server never answered the initialize request
 * and was shut down. Unlike the missing-binary case this failure mode has no
 * other reporter, so it gets a notification on top of the output-channel log.
 */
function notifyStartTimeout(): void {
    void vscode.window
        .showWarningMessage(
            `Inference language server did not respond within ${LSP_START_TIMEOUT_MS / 1000} seconds and was shut down.`,
            'Show Output',
        )
        .then((action) => {
            if (action === 'Show Output') {
                void vscode.commands.executeCommand('inference.showOutput');
            }
        });
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
