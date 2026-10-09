import * as crypto from 'crypto';
import * as vscode from 'vscode';

import { ApiError, ProverApi } from './api';
import { describeError, requireConfig } from './config';
import { fetchVerified, type ProofDocuments } from './proofDocuments';
import { portalJobUrl } from './serverUrl';
import type { JobResponse } from './types';

type JobRef = Pick<JobResponse, 'id'> & { filename?: string | null };

function label(job: JobRef): string {
    return job.filename ?? job.id;
}

/**
 * Confirm, then `POST /cancel`. Returns true when the job's state moved
 * (accepted, or already past cancelable) so callers refresh.
 */
export async function cancelJobInteractive(
    secrets: vscode.SecretStorage,
    log: vscode.LogOutputChannel,
    job: JobRef,
): Promise<boolean> {
    const confirmed = await vscode.window.showWarningMessage(
        `Cancel proof job "${label(job)}"?`,
        {
            modal: true,
            detail: 'A running worker stops at its next heartbeat; partial results are kept. While a worker is still starting, canceling can take up to about 90 seconds.',
        },
        'Cancel Job',
    );
    if (confirmed !== 'Cancel Job') {
        return false;
    }
    const config = await requireConfig(secrets);
    if (!config) {
        return false;
    }
    try {
        const res = await new ProverApi(config.serverUrl, config.apiKey).cancelJob(job.id);
        log.info(`Prover: cancel requested for job ${job.id} → ${res.status}`);
        vscode.window.setStatusBarMessage(`$(circle-slash) Canceling ${label(job)}…`, 5000);
        return true;
    } catch (err) {
        if (err instanceof ApiError && err.code === 'NOT_CANCELABLE') {
            vscode.window.showInformationMessage(
                `Inference: ${err.detail ?? 'the job can no longer be canceled.'}`,
            );
            return true;
        }
        log.error(`Prover: cancel job ${job.id}: ${err instanceof Error ? err.message : err}`);
        vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
        return false;
    }
}

/** Confirm, then `DELETE` a finished job. Returns true when it was deleted. */
export async function deleteJobInteractive(
    secrets: vscode.SecretStorage,
    log: vscode.LogOutputChannel,
    job: JobRef,
): Promise<boolean> {
    const confirmed = await vscode.window.showWarningMessage(
        `Delete proof job "${label(job)}"?`,
        {
            modal: true,
            detail: 'It disappears from your jobs now and its input, proof, reports and events are purged within 7 days. Save the certificate and artifacts you want to keep first.',
        },
        'Delete Job',
    );
    if (confirmed !== 'Delete Job') {
        return false;
    }
    const config = await requireConfig(secrets);
    if (!config) {
        return false;
    }
    try {
        await new ProverApi(config.serverUrl, config.apiKey).deleteJob(job.id);
        log.info(`Prover: deleted job ${job.id}`);
        return true;
    } catch (err) {
        log.error(`Prover: delete job ${job.id}: ${err instanceof Error ? err.message : err}`);
        vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
        return false;
    }
}

/**
 * Submit a finished job's input again as a new job. The key is random on
 * purpose: the content-derived key would return the existing job.
 */
export async function resubmitJob(
    secrets: vscode.SecretStorage,
    log: vscode.LogOutputChannel,
    job: JobRef,
): Promise<JobResponse | undefined> {
    const config = await requireConfig(secrets);
    if (!config) {
        return undefined;
    }
    const api = new ProverApi(config.serverUrl, config.apiKey);
    try {
        return await vscode.window.withProgress(
            { location: vscode.ProgressLocation.Notification, title: `Running ${label(job)} again…` },
            async () => {
                const input = (await api.listArtifacts(job.id)).find((a) => a.kind === 'InputV');
                if (!input) {
                    throw new Error('The job has no stored input to resubmit.');
                }
                const bytes = await fetchVerified(api, job.id, input);
                // The server stores every input as `input.v`; keep the name the job was submitted under.
                const filename = job.filename || input.filename;
                const fresh = await api.submitJob(filename, bytes, {}, crypto.randomUUID());
                log.info(`Prover: resubmitted job ${job.id} → ${fresh.id}`);
                return fresh;
            },
        );
    } catch (err) {
        log.error(`Prover: resubmit job ${job.id}: ${err instanceof Error ? err.message : err}`);
        vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
        return undefined;
    }
}

/** Diff a job's input against its completed proof. */
export async function compareProof(
    secrets: vscode.SecretStorage,
    log: vscode.LogOutputChannel,
    documents: ProofDocuments,
    job: JobRef,
): Promise<void> {
    const config = await requireConfig(secrets);
    if (!config) {
        return;
    }
    const api = new ProverApi(config.serverUrl, config.apiKey);
    try {
        const artifacts = await api.listArtifacts(job.id);
        const input = artifacts.find((a) => a.kind === 'InputV');
        const completed = artifacts.find((a) => a.kind === 'CompletedV');
        if (!input || !completed) {
            vscode.window.showInformationMessage(
                'Inference: this job has no completed proof to compare.',
            );
            return;
        }
        await documents.compare(
            api,
            job.id,
            input,
            completed,
            `${label(job)}: submitted ↔ completed`,
        );
    } catch (err) {
        log.error(`Prover: compare job ${job.id}: ${err instanceof Error ? err.message : err}`);
        vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
    }
}

/** Open the job's portal page (certificate tab by default). */
export async function openInPortal(
    secrets: vscode.SecretStorage,
    job: JobRef,
    page: 'certificate' | 'job' = 'certificate',
): Promise<void> {
    const config = await requireConfig(secrets);
    if (config) {
        const tab = page === 'certificate' ? 'certificate' : undefined;
        await vscode.env.openExternal(vscode.Uri.parse(portalJobUrl(config.serverUrl, job.id, tab)));
    }
}
