/**
 * "Prove This File": compile an `.inf` locally in proof mode and submit the
 * generated `.v`, refusing a compiler the proof server does not accept.
 *
 * No `vscode` import — every effect is injected so each gate is unit-tested
 * under plain node. The VS Code layer (proveFile.ts) supplies the effects and
 * turns each outcome into UI.
 */

import * as path from 'path';

import type { ProverApi } from './api';
import { generatedVPath, submitVFile } from './submission';
import {
    compareIdentity,
    parseAbiVersion,
    parseCommitHash,
    parseInfcVersion,
    type InfcIdentity,
} from './toolchainIdentity';
import type { AcceptedToolchain, JobResponse } from './types';

/** What a spawned command returned. */
export interface CommandResult {
    exitCode: number;
    stdout: string;
    stderr: string;
    timedOut?: boolean;
    aborted?: boolean;
}

export interface ProveDeps {
    /** The `infs` binary, or null when the toolchain is not installed. */
    locateInfs(): string | null;
    /** The `infc` that `infs build` will spawn (from `infs doctor`), or null. */
    resolveInfc(infsPath: string): Promise<string | null>;
    run(
        command: string,
        args: string[],
        options: { cwd?: string; env?: Record<string, string>; timeoutMs?: number },
    ): Promise<CommandResult>;
    readFile(filePath: string): Promise<Uint8Array>;
    api: Pick<ProverApi, 'getMeta' | 'submitJob'>;
    /**
     * Asked only when the server does not publish its accepted compiler.
     * Resolve true to compile and submit without the identity check.
     */
    confirmUnchecked(): Promise<boolean>;
    /** Progress for the notification and the output channel. */
    progress(message: string): void;
}

export type ProveOutcome =
    | { kind: 'submitted'; job: JobResponse; vPath: string; holes: number; checked: boolean }
    | { kind: 'no-infs' }
    | { kind: 'no-infc' }
    | {
          kind: 'toolchain-mismatch';
          local: InfcIdentity;
          accepted: AcceptedToolchain;
          reasons: string[];
      }
    | { kind: 'build-failed'; exitCode: number; output: string; timedOut: boolean }
    | { kind: 'no-output'; output: string }
    | { kind: 'preflight-failed'; vPath: string; problem: string }
    | { kind: 'cancelled' };

/** Build budget: the compiler is fast; this bounds a hang, not a real build. */
const BUILD_TIMEOUT_MS = 5 * 60_000;
const IDENTITY_TIMEOUT_MS = 15_000;

/** The local identity, or null when the user cancelled while it was probed. */
async function probeIdentity(deps: ProveDeps, infcPath: string): Promise<InfcIdentity | null> {
    const results = await Promise.all(
        ['--commit-hash', '--version', '--abi-version'].map((flag) =>
            deps.run(infcPath, [flag], { timeoutMs: IDENTITY_TIMEOUT_MS }),
        ),
    );
    if (results.some((r) => r.aborted)) {
        return null;
    }
    const [commit, version, abi] = results.map((r) => (r.exitCode === 0 ? r.stdout : ''));
    return {
        commit: parseCommitHash(commit),
        version: parseInfcVersion(version),
        abiVersion: parseAbiVersion(abi),
    };
}

export async function proveInfFile(infPath: string, deps: ProveDeps): Promise<ProveOutcome> {
    const infsPath = deps.locateInfs();
    if (!infsPath) {
        return { kind: 'no-infs' };
    }

    deps.progress('Checking the compiler…');
    const infcPath = await deps.resolveInfc(infsPath);
    if (!infcPath) {
        return { kind: 'no-infc' };
    }
    const [local, meta] = await Promise.all([probeIdentity(deps, infcPath), deps.api.getMeta()]);
    if (!local) {
        return { kind: 'cancelled' };
    }
    const verdict = compareIdentity(local, meta.acceptedToolchain);
    if (verdict.kind === 'mismatch') {
        return {
            kind: 'toolchain-mismatch',
            local,
            accepted: meta.acceptedToolchain!,
            reasons: verdict.reasons,
        };
    }
    if (verdict.kind === 'unchecked' && !(await deps.confirmUnchecked())) {
        return { kind: 'cancelled' };
    }

    // Single-file mode from the source's folder: infc writes <dir>/out/<stem>.v.
    // INFC_PATH pins the build to the binary whose identity was just checked.
    const cwd = path.dirname(infPath);
    deps.progress(`Compiling ${path.basename(infPath)} in proof mode…`);
    const build = await deps.run(infsPath, ['build', infPath, '-v'], {
        cwd,
        env: { INFC_PATH: infcPath },
        timeoutMs: BUILD_TIMEOUT_MS,
    });
    if (build.aborted) {
        return { kind: 'cancelled' };
    }
    const output = [build.stdout, build.stderr].filter(Boolean).join('\n');
    if (build.exitCode !== 0 || build.timedOut) {
        return {
            kind: 'build-failed',
            exitCode: build.exitCode,
            output,
            timedOut: build.timedOut === true,
        };
    }
    const vPath = generatedVPath(build.stdout, cwd);
    if (!vPath) {
        return { kind: 'no-output', output };
    }

    deps.progress(`Submitting ${path.basename(vPath)}…`);
    const bytes = await deps.readFile(vPath);
    const submitted = await submitVFile(deps.api, path.basename(vPath), bytes, meta.maxUploadBytes);
    if (submitted.kind === 'preflight-failed') {
        return { kind: 'preflight-failed', vPath, problem: submitted.problem };
    }
    return {
        kind: 'submitted',
        job: submitted.job,
        vPath,
        holes: submitted.holes,
        checked: verdict.kind === 'match',
    };
}
