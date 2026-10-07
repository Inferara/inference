import * as cp from 'child_process';
import { StringDecoder } from 'string_decoder';

import type { ExecResult } from './exec';

export interface RunOptions {
    cwd?: string;
    /** Merged over `process.env`. */
    env?: Record<string, string>;
    /** Kill the child after this long (default 10 minutes). */
    timeoutMs?: number;
    /** Abort kills the child; the result then has `aborted: true`. */
    signal?: AbortSignal;
    /** Called once per complete stdout/stderr line, as it arrives. */
    onLine?: (line: string, stream: 'stdout' | 'stderr') => void;
}

export interface RunResult extends ExecResult {
    timedOut: boolean;
    aborted: boolean;
}

const DEFAULT_TIMEOUT_MS = 10 * 60_000;
/** Grace between SIGTERM and SIGKILL. */
const KILL_GRACE_MS = 2_000;

/**
 * Run a command, streaming its output line by line, with a timeout and
 * cancellation. Unlike {@link exec}, a long build stays observable and the
 * user can stop it.
 *
 * Resolves on exit (including non-zero, timeout and abort); rejects only when
 * the command cannot be spawned.
 */
export function run(command: string, args: string[], options: RunOptions = {}): Promise<RunResult> {
    return new Promise((resolve, reject) => {
        if (options.signal?.aborted) {
            resolve({ exitCode: 1, stdout: '', stderr: '', timedOut: false, aborted: true });
            return;
        }
        const child = cp.spawn(command, args, {
            cwd: options.cwd,
            env: options.env ? { ...process.env, ...options.env } : undefined,
            stdio: ['ignore', 'pipe', 'pipe'],
        });

        const out = { stdout: '', stderr: '' };
        const pending = { stdout: '', stderr: '' };
        const decoders = { stdout: new StringDecoder('utf8'), stderr: new StringDecoder('utf8') };
        let timedOut = false;
        let aborted = false;
        let killTimer: NodeJS.Timeout | undefined;

        const consume = (stream: 'stdout' | 'stderr', text: string) => {
            out[stream] += text;
            pending[stream] += text;
            const lines = pending[stream].split(/\r?\n/);
            pending[stream] = lines.pop() ?? '';
            for (const line of lines) {
                options.onLine?.(line, stream);
            }
        };
        child.stdout.on('data', (chunk: Buffer) => consume('stdout', decoders.stdout.write(chunk)));
        child.stderr.on('data', (chunk: Buffer) => consume('stderr', decoders.stderr.write(chunk)));

        const stop = () => {
            child.kill('SIGTERM');
            killTimer = setTimeout(() => child.kill('SIGKILL'), KILL_GRACE_MS);
        };
        const timer = setTimeout(() => {
            timedOut = true;
            stop();
        }, options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
        const onAbort = () => {
            aborted = true;
            stop();
        };
        options.signal?.addEventListener('abort', onAbort, { once: true });

        const finish = () => {
            clearTimeout(timer);
            if (killTimer) {
                clearTimeout(killTimer);
            }
            options.signal?.removeEventListener('abort', onAbort);
        };
        child.on('error', (err) => {
            finish();
            reject(err);
        });
        child.on('close', (code) => {
            finish();
            for (const stream of ['stdout', 'stderr'] as const) {
                const tail = decoders[stream].end();
                out[stream] += tail;
                const rest = pending[stream] + tail;
                if (rest) {
                    options.onLine?.(rest, stream);
                }
            }
            resolve({
                exitCode: code ?? 1,
                stdout: out.stdout,
                stderr: out.stderr,
                timedOut,
                aborted,
            });
        });
    });
}
