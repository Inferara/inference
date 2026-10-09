import * as path from 'path';
import * as vscode from 'vscode';

import { ApiError, AuthError, ProverApi } from './api';
import { describeError, requireConfig, type ProverConfig } from './config';
import type { JobOrigin } from './jobOrigins';
import { DEFAULT_MAX_UPLOAD_BYTES, submitVFile } from './submission';
import type { JobResponse } from './types';

/** What a finished submission hands to the views. */
export interface SubmitHooks {
    /** Refresh the jobs view, reveal the job and open its panel. */
    showJob(job: JobResponse, origin?: JobOrigin): Promise<void>;
}

const UPLOAD_NOTICE_KEY = 'inference.prover.uploadNotice';

/**
 * Show the portal's upload notice once per server and remember the answer.
 * Returns false when the user declines.
 */
export async function confirmUpload(
    context: vscode.ExtensionContext,
    serverUrl: string,
): Promise<boolean> {
    const key = `${UPLOAD_NOTICE_KEY}:${serverUrl}`;
    if (context.globalState.get<boolean>(key)) {
        return true;
    }
    const choice = await vscode.window.showWarningMessage(
        `Upload to ${serverUrl}?`,
        {
            modal: true,
            detail:
                'The generated Rocq file contains your program logic and specifications. Submitted files and proof results are visible to your account and the platform operator, and an AI prover may send file content to its model provider. Use only code you are allowed to share.',
        },
        'Upload',
    );
    if (choice !== 'Upload') {
        return false;
    }
    await context.globalState.update(key, true);
    return true;
}

/** Map a submit failure to an actionable message. */
export function submitErrorMessage(err: unknown): string {
    if (err instanceof AuthError) {
        return err.message;
    }
    if (err instanceof ApiError) {
        switch (err.code) {
            case 'NO_PROOF_HOLES':
                return 'The file contains no proof holes, so there is nothing to prove.';
            case 'QUOTA_EXCEEDED':
                return `Quota exceeded: ${err.detail ?? 'too many jobs.'}`;
            case 'PAYLOAD_TOO_LARGE':
                return 'The file exceeds the server upload limit.';
            case 'MAINTENANCE':
            case 'PROVIDER_UNAVAILABLE':
            case 'VERIFIER_UNAVAILABLE':
            case 'AGENT_UNAVAILABLE':
                return `The server cannot run jobs right now: ${err.detail ?? err.code}`;
        }
    }
    return describeError(err);
}

/** The server upload cap from `/meta`, or the default when unavailable. */
export async function uploadCap(api: ProverApi): Promise<number> {
    try {
        return (await api.getMeta()).maxUploadBytes || DEFAULT_MAX_UPLOAD_BYTES;
    } catch {
        return DEFAULT_MAX_UPLOAD_BYTES;
    }
}

/** A file to act on: explicit argument → active editor → open dialog. */
export async function pickFile(
    arg: unknown,
    extension: '.inf' | '.v',
    dialog: { title: string; filters: Record<string, string[]> },
): Promise<vscode.Uri | undefined> {
    if (arg instanceof vscode.Uri && arg.path.endsWith(extension)) {
        return arg;
    }
    const active = vscode.window.activeTextEditor?.document;
    if (active && active.uri.scheme === 'file' && active.uri.path.endsWith(extension)) {
        return active.uri;
    }
    const picked = await vscode.window.showOpenDialog({
        canSelectMany: false,
        openLabel: 'Select',
        ...dialog,
    });
    return picked?.[0];
}

/** Save an open, dirty document first; false when the user declines. */
export async function saveIfDirty(uri: vscode.Uri, verb: string): Promise<boolean> {
    const doc = vscode.workspace.textDocuments.find((d) => d.uri.toString() === uri.toString());
    if (!doc?.isDirty) {
        return true;
    }
    const choice = await vscode.window.showWarningMessage(
        `"${path.basename(uri.fsPath)}" has unsaved changes.`,
        { modal: true },
        `Save and ${verb}`,
    );
    return choice === `Save and ${verb}` && (await doc.save());
}

/** `inference.submitProof`: submit an existing `.v` file. */
export function registerSubmitProofCommand(
    context: vscode.ExtensionContext,
    log: vscode.LogOutputChannel,
    hooks: SubmitHooks,
): vscode.Disposable {
    return vscode.commands.registerCommand('inference.submitProof', async (arg?: unknown) => {
        const config: ProverConfig | undefined = await requireConfig(context.secrets);
        if (!config) {
            return;
        }
        const uri = await pickFile(arg, '.v', {
            title: 'Select the Rocq .v file to prove',
            filters: { 'Rocq source': ['v'] },
        });
        if (!uri || !(await saveIfDirty(uri, 'Submit')) || !(await confirmUpload(context, config.serverUrl))) {
            return;
        }
        const filename = path.basename(uri.fsPath);
        const api = new ProverApi(config.serverUrl, config.apiKey);
        try {
            const outcome = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: `Submitting ${filename}…` },
                async () => submitVFile(api, filename, await vscode.workspace.fs.readFile(uri), await uploadCap(api)),
            );
            if (outcome.kind === 'preflight-failed') {
                vscode.window.showErrorMessage(`Inference: ${outcome.problem}`);
                return;
            }
            log.info(`Prover: submitted ${filename} (${outcome.holes} holes) → job ${outcome.job.id}`);
            await hooks.showJob(outcome.job, { vPath: uri.fsPath, vSha256: outcome.sha256 });
        } catch (err) {
            log.error(`Prover: submit ${filename}: ${err instanceof Error ? err.message : err}`);
            vscode.window.showErrorMessage(`Inference: ${submitErrorMessage(err)}`);
        }
    });
}
