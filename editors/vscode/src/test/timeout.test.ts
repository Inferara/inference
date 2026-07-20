import * as assert from 'node:assert';
import { describe, it } from 'node:test';
import { TimeoutError, TimeoutHost, withTimeout } from '../utils/timeout';

/**
 * Tests for the pure promise-timeout helper that bounds LanguageClient
 * start() in src/lsp/client.ts. timeout.ts has no vscode dependency, so it
 * is imported directly.
 *
 * Two styles are used, mirroring lsp-resolve.test.ts:
 * - An injected fake TimeoutHost for deterministic scheduling assertions
 *   (which timer was set, that it was cleared, stale-fire behavior).
 * - Real timers with short deadlines for end-to-end settlement ordering.
 */

/** Fake TimeoutHost recording scheduling calls and allowing manual fire. */
class FakeHost implements TimeoutHost {
    callback: (() => void) | undefined;
    requestedMs: number | undefined;
    cleared: unknown[] = [];
    readonly handle = Symbol('timer');

    setTimeout(callback: () => void, ms: number): unknown {
        this.callback = callback;
        this.requestedMs = ms;
        return this.handle;
    }

    clearTimeout(handle: unknown): void {
        this.cleared.push(handle);
    }

    fire(): void {
        assert.ok(this.callback, 'a timer callback was scheduled');
        this.callback();
    }
}

/** A promise that never settles. */
function pending<T>(): Promise<T> {
    return new Promise<T>(() => undefined);
}

/** Wait for one macrotask turn so late settlements can propagate. */
function nextTurn(): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, 0));
}

describe('TimeoutError', () => {
    it('is an Error with name TimeoutError and the given message', () => {
        const err = new TimeoutError('deadline elapsed');
        assert.ok(err instanceof Error);
        assert.ok(err instanceof TimeoutError);
        assert.strictEqual(err.name, 'TimeoutError');
        assert.strictEqual(err.message, 'deadline elapsed');
    });
});

describe('withTimeout (injected host)', () => {
    it('schedules the timer with the requested duration', () => {
        const host = new FakeHost();
        void withTimeout(pending(), 1234, 'stuck', host).catch(
            () => undefined,
        );
        assert.strictEqual(host.requestedMs, 1234);
    });

    it('rejects with a TimeoutError carrying the message when the timer fires', async () => {
        const host = new FakeHost();
        const result = withTimeout(pending(), 30_000, 'stuck', host);
        host.fire();
        await assert.rejects(
            result,
            (err: unknown) =>
                err instanceof TimeoutError && err.message === 'stuck',
        );
    });

    it('clears the timer when the promise resolves first', async () => {
        const host = new FakeHost();
        const value = await withTimeout(Promise.resolve(7), 100, 'x', host);
        assert.strictEqual(value, 7);
        assert.deepStrictEqual(host.cleared, [host.handle]);
    });

    it('clears the timer when the promise rejects first', async () => {
        const host = new FakeHost();
        const failure = new Error('spawn failed');
        await assert.rejects(
            withTimeout(Promise.reject(failure), 100, 'x', host),
            (err: unknown) => err === failure,
        );
        assert.deepStrictEqual(host.cleared, [host.handle]);
    });

    it('a stale timer firing after resolution is a no-op', async () => {
        const host = new FakeHost();
        const result = withTimeout(Promise.resolve('ok'), 100, 'x', host);
        assert.strictEqual(await result, 'ok');
        host.fire();
        assert.strictEqual(await result, 'ok');
    });

    it('a stale timer firing after rejection keeps the original error', async () => {
        const host = new FakeHost();
        const failure = new Error('original');
        const result = withTimeout(
            Promise.reject(failure),
            100,
            'timeout message',
            host,
        );
        await assert.rejects(result, (err: unknown) => err === failure);
        host.fire();
        await assert.rejects(result, (err: unknown) => err === failure);
    });

    it('discards a resolution that arrives after the timeout', async () => {
        const host = new FakeHost();
        let resolveLate: (value: number) => void = () => undefined;
        const late = new Promise<number>((resolve) => {
            resolveLate = resolve;
        });
        const result = withTimeout(late, 5, 'too slow', host);
        host.fire();
        await assert.rejects(result, TimeoutError);
        resolveLate(42);
        await nextTurn();
        await assert.rejects(result, TimeoutError);
    });

    it('swallows a rejection that arrives after the timeout (no unhandled rejection)', async () => {
        const unhandled: unknown[] = [];
        const onUnhandled = (reason: unknown) => {
            unhandled.push(reason);
        };
        process.on('unhandledRejection', onUnhandled);
        try {
            const host = new FakeHost();
            let rejectLate: (err: Error) => void = () => undefined;
            const late = new Promise<never>((_resolve, reject) => {
                rejectLate = reject;
            });
            const result = withTimeout(late, 5, 'too slow', host);
            host.fire();
            await assert.rejects(result, TimeoutError);
            rejectLate(new Error('late failure'));
            await nextTurn();
            await nextTurn();
            assert.deepStrictEqual(unhandled, []);
            await assert.rejects(result, TimeoutError);
        } finally {
            process.off('unhandledRejection', onUnhandled);
        }
    });
});

describe('withTimeout (real timers)', () => {
    it('resolves with the value when the promise settles before the deadline', async () => {
        const value = await withTimeout(
            Promise.resolve('fast'),
            1000,
            'never',
        );
        assert.strictEqual(value, 'fast');
    });

    it('propagates the original rejection when the promise fails before the deadline', async () => {
        const failure = new Error('boom');
        await assert.rejects(
            withTimeout(Promise.reject(failure), 1000, 'never'),
            (err: unknown) => err === failure && !(err instanceof TimeoutError),
        );
    });

    it('rejects with TimeoutError when the promise never settles', async () => {
        await assert.rejects(
            withTimeout(pending(), 10, 'hung server'),
            (err: unknown) =>
                err instanceof TimeoutError && err.message === 'hung server',
        );
    });

    it('an already-settled promise wins over a zero deadline (microtasks run before timers)', async () => {
        const value = await withTimeout(Promise.resolve(1), 0, 'x');
        assert.strictEqual(value, 1);
    });

    it('a slow resolution loses to a shorter deadline', async () => {
        const slow = new Promise<string>((resolve) =>
            setTimeout(() => resolve('late'), 50),
        );
        await assert.rejects(
            withTimeout(slow, 5, 'deadline'),
            TimeoutError,
        );
    });

    it('a fast resolution beats a longer deadline', async () => {
        const fast = new Promise<string>((resolve) =>
            setTimeout(() => resolve('early'), 5),
        );
        const value = await withTimeout(fast, 1000, 'deadline');
        assert.strictEqual(value, 'early');
    });
});
