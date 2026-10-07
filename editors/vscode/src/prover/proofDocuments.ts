import * as crypto from 'crypto';
import * as vscode from 'vscode';

import { ProverApi } from './api';
import { resolveConfig } from './config';
import type { ArtifactInfo } from './types';

/** URI scheme for read-only artifact documents. */
export const PROOF_SCHEME = 'inference-proof';

/** Thrown when a downloaded artifact does not match its recorded SHA-256. */
export class IntegrityError extends Error {
    constructor(filename: string) {
        super(`${filename} failed its integrity check (SHA-256 mismatch).`);
        this.name = 'IntegrityError';
    }
}

/** Download an artifact and verify it against the server's SHA-256. */
export async function fetchVerified(
    api: ProverApi,
    jobId: string,
    info: ArtifactInfo,
): Promise<Buffer> {
    const content = await api.downloadArtifact(jobId, info.id);
    const digest = crypto.createHash('sha256').update(content).digest('hex');
    if (digest.toLowerCase() !== info.sha256.toLowerCase()) {
        throw new IntegrityError(info.filename);
    }
    return content;
}

/** Basename only: an artifact filename is worker-influenced. */
export function safeArtifactName(filename: string): string {
    return (
        filename
            .split(/[\\/]/)
            .pop()
            ?.replace(/[\u0000-\u001f]/g, '') || 'artifact'
    );
}

function artifactUri(jobId: string, info: ArtifactInfo): vscode.Uri {
    return vscode.Uri.from({
        scheme: PROOF_SCHEME,
        path: `/${jobId}/${info.id}/${safeArtifactName(info.filename)}`,
    });
}

/**
 * Read-only documents for job artifacts (`inference-proof:/<job>/<artifact>/<name>`).
 *
 * Content is downloaded and SHA-256-checked before it is shown; a document
 * reopened after a window reload is fetched again the same way.
 */
export class ProofDocuments implements vscode.TextDocumentContentProvider, vscode.Disposable {
    private readonly contents = new Map<string, string>();
    private readonly registration: vscode.Disposable;

    constructor(private readonly secrets: vscode.SecretStorage) {
        this.registration = vscode.workspace.registerTextDocumentContentProvider(PROOF_SCHEME, this);
    }

    async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
        const cached = this.contents.get(uri.toString());
        if (cached !== undefined) {
            return cached;
        }
        const [, jobId, artifactId] = uri.path.split('/');
        const config = await resolveConfig(this.secrets).catch(() => null);
        if (!config || !jobId || !artifactId) {
            return '';
        }
        const api = new ProverApi(config.serverUrl, config.apiKey);
        const info = (await api.listArtifacts(jobId)).find((a) => a.id === artifactId);
        if (!info) {
            return '';
        }
        return this.load(api, jobId, info, uri);
    }

    private async load(
        api: ProverApi,
        jobId: string,
        info: ArtifactInfo,
        uri = artifactUri(jobId, info),
    ): Promise<string> {
        const text = (await fetchVerified(api, jobId, info)).toString('utf-8');
        this.contents.set(uri.toString(), text);
        return text;
    }

    /** Download (verified) and show one artifact read-only. */
    async open(api: ProverApi, jobId: string, info: ArtifactInfo): Promise<void> {
        const uri = artifactUri(jobId, info);
        await this.load(api, jobId, info, uri);
        const doc = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(doc, { preview: false });
    }

    /** Diff the submitted input against the completed proof. */
    async compare(
        api: ProverApi,
        jobId: string,
        input: ArtifactInfo,
        completed: ArtifactInfo,
        title: string,
    ): Promise<void> {
        const left = artifactUri(jobId, input);
        const right = artifactUri(jobId, completed);
        await Promise.all([
            this.load(api, jobId, input, left),
            this.load(api, jobId, completed, right),
        ]);
        await vscode.commands.executeCommand('vscode.diff', left, right, title);
    }

    dispose(): void {
        this.registration.dispose();
        this.contents.clear();
    }
}
