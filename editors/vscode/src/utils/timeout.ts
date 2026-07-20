/**
 * Pure promise-timeout helper. This module MUST NOT import `vscode`
 * (directly or transitively) so it stays importable from plain `node:test`
 * files; timer access is injectable through {@link TimeoutHost}, mirroring
 * how `lsp/resolve.ts` injects its environment access.
 */

/** Timer scheduling used by {@link withTimeout}; injectable for tests. */
export interface TimeoutHost {
    setTimeout(callback: () => void, ms: number): unknown;
    clearTimeout(handle: unknown): void;
}

/** Rejection reason produced by {@link withTimeout} when the deadline elapses. */
export class TimeoutError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'TimeoutError';
    }
}

/**
 * Settle with `promise`, unless `timeoutMs` elapses first, in which case
 * reject with a {@link TimeoutError} carrying `message`.
 *
 * JavaScript promises are not cancellable, so after a timeout the input
 * promise keeps running; its eventual settlement is observed and discarded,
 * which guarantees a late rejection can never surface as an unhandled
 * rejection. When the input promise settles before the deadline the timer
 * is cleared so no stray handle keeps the event loop alive.
 */
export function withTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
    message: string,
    host: TimeoutHost = { setTimeout, clearTimeout },
): Promise<T> {
    return new Promise<T>((resolve, reject) => {
        let timedOut = false;
        const timer = host.setTimeout(() => {
            timedOut = true;
            reject(new TimeoutError(message));
        }, timeoutMs);
        promise.then(
            (value) => {
                host.clearTimeout(timer);
                if (!timedOut) {
                    resolve(value);
                }
            },
            (err: unknown) => {
                host.clearTimeout(timer);
                if (!timedOut) {
                    reject(err);
                }
            },
        );
    });
}
