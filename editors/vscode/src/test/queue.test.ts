import * as assert from 'node:assert';
import { describe, it } from 'node:test';
import { SerialQueue } from '../lsp/queue';

/**
 * Tests for the pure serial promise queue that serializes LanguageClient
 * lifecycle operations (start/stop/restart) in src/lsp/client.ts. queue.ts has
 * no vscode dependency, so it is imported directly (extension tests cannot
 * import vscode-importing modules) — the same pattern lsp-resolve.test.ts and
 * timeout.test.ts use.
 *
 * The invariants pinned here are the ones client.ts documents as the queue's
 * reason to exist: operations run one at a time in submission order (overlapping
 * lifecycle triggers serialize), a composite stop-then-start restart runs
 * atomically, and a rejected operation is isolated so it neither is swallowed
 * from its own caller nor wedges the queue for later operations.
 */

/** Resolve after one macrotask turn, so queued microtasks flush first. */
function nextTurn(): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, 0));
}

describe('SerialQueue ordering', () => {
    it('runs operations one at a time in submission order', async () => {
        const queue = new SerialQueue();
        const events: string[] = [];
        const op = (name: string) => async () => {
            events.push(`start:${name}`);
            await Promise.resolve();
            events.push(`end:${name}`);
        };

        // Enqueue three overlapping operations before any has finished.
        await Promise.all([
            queue.enqueue(op('a')),
            queue.enqueue(op('b')),
            queue.enqueue(op('c')),
        ]);

        // No interleaving: each operation fully settles before the next starts.
        assert.deepStrictEqual(events, [
            'start:a',
            'end:a',
            'start:b',
            'end:b',
            'start:c',
            'end:c',
        ]);
    });

    it('does not start an operation until the previous one settles', async () => {
        const queue = new SerialQueue();
        const events: string[] = [];
        let releaseFirst: () => void = () => undefined;
        const firstGate = new Promise<void>((resolve) => {
            releaseFirst = resolve;
        });

        const first = queue.enqueue(async () => {
            events.push('start:first');
            await firstGate;
            events.push('end:first');
        });
        const second = queue.enqueue(async () => {
            events.push('start:second');
        });

        // While the first operation is gated, the second must not have started,
        // even after the microtask/macrotask queues drain.
        await nextTurn();
        assert.deepStrictEqual(
            events,
            ['start:first'],
            'the second operation waits for the first to settle',
        );

        releaseFirst();
        await Promise.all([first, second]);
        assert.deepStrictEqual(events, [
            'start:first',
            'end:first',
            'start:second',
        ]);
    });

    it('runs a composite stop-then-start restart atomically before the next op', async () => {
        // client.ts models restart as a single enqueued operation that awaits
        // stop then start. The queue must run that whole unit before any operation
        // enqueued behind it, and stop must precede start within it.
        const queue = new SerialQueue();
        const events: string[] = [];
        const stop = async () => {
            events.push('stop');
            await Promise.resolve();
        };
        const start = async () => {
            events.push('start');
            await Promise.resolve();
        };

        const restart = queue.enqueue(async () => {
            await stop();
            await start();
        });
        const other = queue.enqueue(async () => {
            events.push('other');
        });

        await Promise.all([restart, other]);
        assert.deepStrictEqual(
            events,
            ['stop', 'start', 'other'],
            'restart is stop-then-start, and the next op waits for the whole restart',
        );
    });
});

describe('SerialQueue rejection isolation', () => {
    it('rejects the failing operation to its own caller', async () => {
        const queue = new SerialQueue();
        const failure = new Error('boom');
        await assert.rejects(
            queue.enqueue(async () => {
                throw failure;
            }),
            (err: unknown) => err === failure,
        );
    });

    it('a rejected operation does not wedge the queue', async () => {
        const queue = new SerialQueue();
        const events: string[] = [];

        const failing = queue.enqueue(async () => {
            events.push('start:fail');
            throw new Error('boom');
        });
        const following = queue.enqueue(async () => {
            events.push('start:next');
        });

        await assert.rejects(failing, /boom/);
        await following;
        assert.deepStrictEqual(
            events,
            ['start:fail', 'start:next'],
            'the operation after a rejected one still runs',
        );
    });

    it('isolates an operation that throws synchronously', async () => {
        const queue = new SerialQueue();
        const events: string[] = [];

        const throwing = queue.enqueue(() => {
            events.push('threw');
            throw new Error('sync');
        });
        const following = queue.enqueue(async () => {
            events.push('after');
        });

        await assert.rejects(throwing, /sync/);
        await following;
        assert.deepStrictEqual(events, ['threw', 'after']);
    });

    it('keeps serving operations after a rejection resolves normally', async () => {
        const queue = new SerialQueue();

        await assert.rejects(
            queue.enqueue(async () => {
                throw new Error('first fails');
            }),
            /first fails/,
        );

        let ran = false;
        await queue.enqueue(async () => {
            ran = true;
        });
        assert.ok(ran, 'a later operation resolves normally after an earlier rejection');
    });
});
