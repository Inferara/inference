import * as vscode from 'vscode';

import { ApiError, AuthError } from './api';
import { checkServerUrl } from './serverUrl';

/**
 * SecretStorage key for a server's API key. Keys are per server, so switching
 * between the hosted service and a local deployment keeps both.
 */
export function apiKeySecret(serverUrl: string): string {
    return `inference.prover.apiKey:${serverUrl}`;
}

/** Context key: an API key is stored (drives the Proof Jobs welcome view). */
export const CONFIGURED_CONTEXT = 'inference.prover.configured';

/** Setting for the proof server; empty means the hosted service. */
export const SERVER_URL_SETTING = 'inference.prover.serverUrl';

export interface ProverConfig {
    serverUrl: string;
    apiKey: string;
}

/** The configured server URL is not acceptable (see serverUrl.ts). */
export class ProverConfigError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'ProverConfigError';
    }
}

/** The effective proof-server URL; throws {@link ProverConfigError} when invalid. */
export function getServerUrl(): string {
    const raw = vscode.workspace
        .getConfiguration('inference')
        .get<string>('prover.serverUrl', '');
    const checked = checkServerUrl(raw);
    if (!checked.ok) {
        throw new ProverConfigError(checked.error);
    }
    return checked.url;
}

/**
 * The server URL and stored API key, or null when no key is stored.
 * Throws {@link ProverConfigError} when the URL setting is invalid.
 */
export async function resolveConfig(
    secrets: vscode.SecretStorage,
): Promise<ProverConfig | null> {
    const serverUrl = getServerUrl();
    const apiKey = await secrets.get(apiKeySecret(serverUrl));
    return apiKey ? { serverUrl, apiKey } : null;
}

/** Whether a key is stored for the configured server (false on an invalid URL). */
export async function hasApiKey(secrets: vscode.SecretStorage): Promise<boolean> {
    try {
        return Boolean(await resolveConfig(secrets));
    } catch {
        return false;
    }
}

/** Recompute {@link CONFIGURED_CONTEXT}; returns whether a key is stored. */
export async function updateConfiguredContext(
    secrets: vscode.SecretStorage,
): Promise<boolean> {
    const configured = await hasApiKey(secrets);
    await vscode.commands.executeCommand('setContext', CONFIGURED_CONTEXT, configured);
    return configured;
}

/**
 * The configuration for a user action, or undefined after telling the user
 * what is missing (no key → offer to set one; invalid URL → open the setting).
 */
export async function requireConfig(
    secrets: vscode.SecretStorage,
): Promise<ProverConfig | undefined> {
    let config: ProverConfig | null;
    try {
        config = await resolveConfig(secrets);
    } catch (err) {
        const action = await vscode.window.showErrorMessage(
            `Inference: ${describeError(err)}`,
            'Open Setting',
        );
        if (action === 'Open Setting') {
            await vscode.commands.executeCommand('workbench.action.openSettings', SERVER_URL_SETTING);
        }
        return undefined;
    }
    if (!config) {
        const action = await vscode.window.showWarningMessage(
            'Inference: set your proof server API key first.',
            'Set API Key',
        );
        if (action === 'Set API Key') {
            await vscode.commands.executeCommand('inference.setProverApiKey');
        }
        return undefined;
    }
    return config;
}

/** One user-facing sentence for a prover failure. */
export function describeError(err: unknown): string {
    if (err instanceof ProverConfigError || err instanceof AuthError || err instanceof ApiError) {
        return err.message;
    }
    // Transport failures from api.ts name the URL; the user needs the gist.
    if (err instanceof Error && err.message && !/^(Network error|Connection timed out)/.test(err.message)) {
        return err.message;
    }
    return 'Could not reach the proof server.';
}
