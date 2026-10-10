import * as path from 'path';
import * as vscode from 'vscode';

import { ApiError, AuthError, ProverApi } from './api';
import { describeError, requireConfig, type ProverConfig } from './config';
import { isActiveJob, relativeTime } from './jobList';
import type { JobOrigin } from './jobOrigins';
import { listVerdict } from './jobVerdict';
import { DEFAULT_MAX_UPLOAD_BYTES, preflight, submitVFile } from './submission';
import type { JobResponse } from './types';

/** What a finished submission hands to the views. */
export interface SubmitHooks {
    /** Refresh the jobs view, reveal the job and open its panel. */
    showJob(job: JobResponse, origin?: JobOrigin): Promise<void>;
    /** Submit a finished job's input again as a new job and show it. */
    runAgain(job: JobResponse): Promise<void>;
    /**
     * Call when a notice about a job opens; the check it returns turns false
     * once the server or account changes, as the job then belongs to neither.
     */
    sameAccount(): () => boolean;
}

/**
 * Tell the user that the server returned an existing job for this exact
 * file (its panel opens as usual), and offer Run Again for a finished one.
 */
export function notifyReplay(job: JobResponse, name: string, hooks: SubmitHooks): void {
    const when = relativeTime(job.createdAt, Date.now());
    if (isActiveJob(job)) {
        void vscode.window.showInformationMessage(
            `Inference: ${name} is already being proved${when ? ` (started ${when})` : ''}. Showing that job.`,
        );
        return;
    }
    const verdict = listVerdict(job);
    const message = `Inference: this exact file was already proved${when ? ` ${when}` : ''} — ${verdict.headline}. Showing that job.`;
    const show = verdict.claim !== 'none' ? vscode.window.showInformationMessage : vscode.window.showWarningMessage;
    const same = hooks.sameAccount();
    void show(message, 'Run Again').then((choice) => {
        if (choice === 'Run Again' && same()) {
            void hooks.runAgain(job);
        }
    });
}

const UPLOAD_NOTICE_KEY = 'inference.prover.uploadNotice';

/**
 * Show the portal's upload notice once per server and remember the answer.
 * Returns false when the user declines.
 */
export async function confirmUpload(
    context: vscode.ExtensionContext,
    serverUrl: string,
    filename: string,
): Promise<boolean> {
    const key = `${UPLOAD_NOTICE_KEY}:${serverUrl}`;
    if (context.globalState.get<boolean>(key)) {
        return true;
    }
    const choice = await vscode.window.showWarningMessage(
        `Upload ${filename} to ${serverUrl}?`,
        {
            modal: true,
            detail:
                'The Rocq file contains your program logic and specifications. Submitted files and proof results are visible to your account and the proof server operator, and an AI prover may send file content to its model provider. Use only code you are allowed to share.\n\nYou are asked once per proof server.',
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

/**
 * Save an open, dirty document first, like running a debug session does.
 * Returns false only when saving failed.
 */
export async function saveBeforeRun(uri: vscode.Uri): Promise<boolean> {
    const doc = vscode.workspace.textDocuments.find((d) => d.uri.toString() === uri.toString());
    if (!doc?.isDirty) {
        return true;
    }
    const saved = await doc.save();
    if (saved) {
        vscode.window.setStatusBarMessage(`$(save) Saved ${path.basename(uri.fsPath)}`, 3000);
    } else {
        vscode.window.showErrorMessage(`Inference: ${path.basename(uri.fsPath)} could not be saved, so it was not submitted.`);
    }
    return saved;
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
        if (!uri || !(await saveBeforeRun(uri))) {
            return;
        }
        const filename = path.basename(uri.fsPath);
        const api = new ProverApi(config.serverUrl, config.apiKey);
        try {
            const bytes = await vscode.workspace.fs.readFile(uri);
            const cap = await uploadCap(api);
            // Check the file before asking to upload it: a doomed upload never asks.
            const checked = preflight(filename, bytes, cap);
            if (!checked.ok) {
                vscode.window.showErrorMessage(`Inference: ${checked.problem}`);
                return;
            }
            if (!(await confirmUpload(context, config.serverUrl, filename))) {
                return;
            }
            const outcome = await vscode.window.withProgress(
                { location: vscode.ProgressLocation.Notification, title: `Uploading ${filename}…` },
                () => submitVFile(api, filename, bytes, cap),
            );
            if (outcome.kind === 'preflight-failed') {
                vscode.window.showErrorMessage(`Inference: ${outcome.problem}`);
                return;
            }
            log.info(`Prover: submitted ${filename} (${outcome.holes} holes) → job ${outcome.job.id}${outcome.replayed ? ' (existing job)' : ''}`);
            if (outcome.replayed) {
                notifyReplay(outcome.job, filename, hooks);
            }
            await hooks.showJob(outcome.job, { vPath: uri.fsPath, vSha256: outcome.sha256 });
        } catch (err) {
            log.error(`Prover: submit ${filename}: ${err instanceof Error ? err.message : err}`);
            vscode.window.showErrorMessage(`Inference: ${submitErrorMessage(err)}`);
        }
    });
}
