import * as path from 'path';
import type * as vscode from 'vscode';

/** Where a job's upload came from on this machine. */
export interface JobOrigin {
    /** The `.inf` that was compiled, for Prove This File. */
    infPath?: string;
    /** The `.v` that was uploaded. */
    vPath: string;
    /** SHA-256 (hex) of the uploaded bytes, to tell whether `vPath` changed since. */
    vSha256: string;
}

interface StoredOrigin extends JobOrigin {
    at: number;
}

const KEY = 'inference.prover.jobOrigins';
/** Origins kept per workspace; the oldest are dropped first. */
export const ORIGIN_CAP = 200;

/** Keep the newest `cap` entries (pure, for tests). */
export function pruneOrigins<T extends { at: number }>(
    entries: Record<string, T>,
    cap: number,
): Record<string, T> {
    const sorted = Object.entries(entries).sort(([, a], [, b]) => b.at - a.at);
    return Object.fromEntries(sorted.slice(0, cap));
}

/**
 * Remembers which local files each submitted job came from, per workspace,
 * so the job panel can name the `.inf` and jump to lines in the local `.v`.
 */
export class JobOrigins {
    constructor(private readonly memento: vscode.Memento) {}

    private key(serverUrl: string, jobId: string): string {
        return `${serverUrl}|${jobId}`;
    }

    get(serverUrl: string, jobId: string): JobOrigin | undefined {
        const all = this.memento.get<Record<string, StoredOrigin>>(KEY) ?? {};
        const found = all[this.key(serverUrl, jobId)];
        if (!found || typeof found.vPath !== 'string' || typeof found.vSha256 !== 'string') {
            return undefined;
        }
        const { at: _at, ...origin } = found;
        return origin;
    }

    async set(serverUrl: string, jobId: string, origin: JobOrigin): Promise<void> {
        const all = this.memento.get<Record<string, StoredOrigin>>(KEY) ?? {};
        all[this.key(serverUrl, jobId)] = { ...origin, at: Date.now() };
        await this.memento.update(KEY, pruneOrigins(all, ORIGIN_CAP));
    }

    /** The name the user knows the job by: the `.inf`, else the `.v`. */
    static displayName(origin: JobOrigin | undefined): string | undefined {
        if (!origin) {
            return undefined;
        }
        return path.basename(origin.infPath ?? origin.vPath);
    }
}
