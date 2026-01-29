import * as cp from 'child_process';

export interface ExecResult {
    exitCode: number;
    stdout: string;
    stderr: string;
}

/** Default timeout for child processes (30 seconds). */
const DEFAULT_TIMEOUT_MS = 30_000;

/**
 * Run a command and capture its output.
 *
 * Resolves with ExecResult on completion (including non-zero exit).
 * Rejects only on spawn failure or timeout.
 */
export function exec(
    command: string,
    args: string[],
    options?: { timeoutMs?: number; cwd?: string },
): Promise<ExecResult> {
    const timeout = options?.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    return new Promise((resolve, reject) => {
        const child = cp.spawn(command, args, {
            cwd: options?.cwd,
            stdio: ['ignore', 'pipe', 'pipe'],
            timeout,
        });

        const stdoutChunks: Buffer[] = [];
        const stderrChunks: Buffer[] = [];

        child.stdout.on('data', (chunk: Buffer) => stdoutChunks.push(chunk));
        child.stderr.on('data', (chunk: Buffer) => stderrChunks.push(chunk));

        child.on('error', (err) => reject(err));

        child.on('close', (code) => {
            resolve({
                exitCode: code ?? 1,
                stdout: Buffer.concat(stdoutChunks).toString('utf-8'),
                stderr: Buffer.concat(stderrChunks).toString('utf-8'),
            });
        });
    });
}
