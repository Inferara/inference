/**
 * Submission helpers shared by "Prove This File" and "Submit Proof".
 *
 * No `vscode` import — unit-tested under plain node. The checks mirror what
 * the server enforces on `POST /api/v1/jobs` (VFileValidator) so a doomed
 * upload fails locally with a clear message instead of a round trip.
 */

import * as crypto from 'crypto';
import * as path from 'path';

import type { ProverApi } from './api';
import type { JobResponse, SubmitJobOptions } from './types';

/** The literal hole marker the compiler emits; an unfilled hole does not compile. */
export const HOLE_MARKER = '(* TODO: fill the proof *)';

/** The server's upload cap when `/meta` is unavailable (4 MiB). */
export const DEFAULT_MAX_UPLOAD_BYTES = 4 * 1024 * 1024;

export function countHoles(text: string): number {
    let count = 0;
    for (let i = text.indexOf(HOLE_MARKER); i !== -1; i = text.indexOf(HOLE_MARKER, i + HOLE_MARKER.length)) {
        count++;
    }
    return count;
}

export type Preflight =
    | { ok: true; text: string; holes: number }
    | { ok: false; problem: string };

/** Check a `.v` upload the way the server will (size, UTF-8, NUL, holes). */
export function preflight(
    filename: string,
    bytes: Uint8Array,
    maxUploadBytes: number = DEFAULT_MAX_UPLOAD_BYTES,
): Preflight {
    if (!filename.endsWith('.v')) {
        return { ok: false, problem: `${filename} is not a Rocq .v file.` };
    }
    if (bytes.byteLength > maxUploadBytes) {
        return {
            ok: false,
            problem: `${filename} is ${bytes.byteLength} bytes; the server accepts at most ${maxUploadBytes}.`,
        };
    }
    let text: string;
    try {
        text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
        return { ok: false, problem: `${filename} is not valid UTF-8.` };
    }
    if (text.includes('\u0000')) {
        return { ok: false, problem: `${filename} contains NUL bytes.` };
    }
    const holes = countHoles(text);
    if (holes === 0) {
        return {
            ok: false,
            problem: `${filename} has no proof holes, so there is nothing to prove. A hole is the marker "${HOLE_MARKER}".`,
        };
    }
    return { ok: true, text, holes };
}

/**
 * Content-derived `Idempotency-Key`, identical to the portal's
 * (`portal/src/lib/contractProfile.ts::idempotencyUuid`): resubmitting the
 * same file with the same options returns the existing job from either client.
 */
export function idempotencyKey(text: string, options: SubmitJobOptions = {}): string {
    const digest = crypto
        .createHash('sha256')
        .update(`${JSON.stringify(options)} ${text}`, 'utf8')
        .digest();
    digest[6] = (digest[6] & 0x0f) | 0x40; // version 4
    digest[8] = (digest[8] & 0x3f) | 0x80; // variant
    const hex = digest.subarray(0, 16).toString('hex');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * The `.v` path infc reports (`V generated at: <path>`), resolved against the
 * directory the build ran in. The last report wins.
 */
export function generatedVPath(stdout: string, cwd: string): string | null {
    const matches = [...stdout.matchAll(/^V generated at:\s*(.+?)\s*$/gm)];
    const reported = matches.at(-1)?.[1];
    return reported ? path.resolve(cwd, reported) : null;
}

export type SubmitOutcome =
    | { kind: 'submitted'; job: JobResponse; holes: number }
    | { kind: 'preflight-failed'; problem: string };

/** Preflight, then submit with the content-derived key. API errors propagate. */
export async function submitVFile(
    api: Pick<ProverApi, 'submitJob'>,
    filename: string,
    bytes: Uint8Array,
    maxUploadBytes?: number,
): Promise<SubmitOutcome> {
    const checked = preflight(filename, bytes, maxUploadBytes);
    if (!checked.ok) {
        return { kind: 'preflight-failed', problem: checked.problem };
    }
    // No options: the server picks provider, agent and budget.
    const job = await api.submitJob(filename, bytes, {}, idempotencyKey(checked.text));
    return { kind: 'submitted', job, holes: checked.holes };
}
