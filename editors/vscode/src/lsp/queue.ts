/**
 * Pure serial promise queue that serializes the LanguageClient lifecycle
 * operations (start/stop/restart) in src/lsp/client.ts. This module MUST NOT
 * import `vscode` (directly or transitively) so it stays importable from plain
 * `node:test` files, mirroring how `lsp/resolve.ts` and `utils/timeout.ts` are
 * structured — the queue's race-freedom invariants were previously untestable
 * because they lived inline in the vscode-importing client module.
 */

/**
 * Runs enqueued async operations one at a time, in submission order.
 *
 * Each {@link enqueue} chains its operation after every previously enqueued one
 * and returns a promise that settles with that operation's own result, so a
 * caller both waits for its turn and observes its own success or failure.
 *
 * A rejected operation is isolated: the promise returned to its caller rejects
 * (the failure is not swallowed from the caller), but the internal chain the
 * next operation waits on absorbs the rejection, so one failed operation can
 * never wedge the queue — later operations still run in order.
 */
export class SerialQueue {
    /**
     * Tail of the internal chain the next operation is scheduled after. It is
     * kept rejection-free (each link ends in `.catch`) so a failed operation
     * does not poison the chain the following operation awaits.
     */
    private tail: Promise<void> = Promise.resolve();

    /** Chain `operation` after all previously enqueued ones. */
    enqueue(operation: () => Promise<void>): Promise<void> {
        const next = this.tail.then(operation);
        this.tail = next.catch(() => undefined);
        return next;
    }
}
