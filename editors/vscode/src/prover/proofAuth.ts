import * as vscode from 'vscode';

import { AuthError, ProverApi } from './api';
import { apiKeySecret, describeError, getServerUrl, updateConfiguredContext } from './config';

/**
 * `inference.setProverApiKey` and `inference.clearProverApiKey`.
 *
 * A new key is checked against `GET /api/v1/me` before it is stored, so a
 * typo fails here rather than as an empty jobs list later. Service keys are
 * issued by the proof-server operator.
 */
export function registerProverAuthCommands(
    context: vscode.ExtensionContext,
    log: vscode.LogOutputChannel,
    onChanged: () => void,
): vscode.Disposable {
    const secrets = context.secrets;

    const setKey = vscode.commands.registerCommand('inference.setProverApiKey', async () => {
        let serverUrl: string;
        try {
            serverUrl = getServerUrl();
        } catch (err) {
            vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
            return;
        }
        const key = await vscode.window.showInputBox({
            title: 'Inference Proof Server API Key',
            prompt: `API key for ${serverUrl}, issued by your proof server operator.`,
            password: true,
            ignoreFocusOut: true,
            placeHolder: 'infp_key…',
        });
        const trimmed = key?.trim();
        if (!trimmed) {
            return;
        }
        try {
            const me = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: 'Checking the API key…' },
                () => new ProverApi(serverUrl, trimmed).getMe(),
            );
            await secrets.store(apiKeySecret(serverUrl), trimmed);
            log.info(`Prover: API key stored for ${serverUrl} (role ${me.role}).`);
            vscode.window.showInformationMessage(
                `Inference: connected to ${serverUrl} as a ${me.role} account.`,
            );
        } catch (err) {
            if (err instanceof AuthError) {
                vscode.window.showErrorMessage(
                    'Inference: the proof server rejected this API key; nothing was stored.',
                );
                return;
            }
            log.warn(`Prover: key check failed: ${err instanceof Error ? err.message : err}`);
            const choice = await vscode.window.showWarningMessage(
                `Couldn't check the API key with ${serverUrl}.`,
                { modal: true, detail: `${describeError(err)}\n\nSave it anyway and check it later?` },
                'Save Anyway',
            );
            if (choice !== 'Save Anyway') {
                return;
            }
            await secrets.store(apiKeySecret(serverUrl), trimmed);
            log.info(`Prover: API key stored for ${serverUrl} without a check.`);
        }
        await updateConfiguredContext(secrets);
        onChanged();
    });

    const clearKey = vscode.commands.registerCommand('inference.clearProverApiKey', async () => {
        let serverUrl: string;
        try {
            serverUrl = getServerUrl();
        } catch (err) {
            vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
            return;
        }
        const confirmed = await vscode.window.showWarningMessage(
            `Remove the API key for ${serverUrl}?`,
            { modal: true, detail: 'Open proof job panels close. Your jobs stay on the proof server.' },
            'Remove Key',
        );
        if (confirmed !== 'Remove Key') {
            return;
        }
        await secrets.delete(apiKeySecret(serverUrl));
        log.info(`Prover: API key for ${serverUrl} cleared.`);
        vscode.window.showInformationMessage(`Inference: removed the API key for ${serverUrl}.`);
        await updateConfiguredContext(secrets);
        onChanged();
    });

    return vscode.Disposable.from(setKey, clearKey);
}
