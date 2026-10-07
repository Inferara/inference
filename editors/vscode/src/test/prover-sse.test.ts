import * as assert from 'node:assert';
import * as http from 'node:http';
import { describe, it, after } from 'node:test';

import { SseParser, openEventStream, type SseMessage, type StreamState } from '../prover/sse';

describe('SseParser', () => {
    it('parses a complete server-shaped frame', () => {
        const p = new SseParser();
        const msgs = p.feed(
            'id: 7\nevent: obligation.proved\ndata: {"seq":7,"type":"obligation.proved"}\n\n',
        );
        assert.strictEqual(msgs.length, 1);
        assert.deepStrictEqual(msgs[0], {
            event: 'obligation.proved',
            data: '{"seq":7,"type":"obligation.proved"}',
            id: '7',
        });
        assert.strictEqual(p.lastEventId, '7');
    });

    it('handles frames split at arbitrary chunk boundaries', () => {
        const frame =
            'id: 12\nevent: log\ndata: {"msg":"héllo wörld"}\n\nid: 13\nevent: end\ndata: {"status":"Succeeded"}\n\n';
        // Split the frame at every possible position; the result must not vary.
        for (let cut = 1; cut < frame.length - 1; cut++) {
            const p = new SseParser();
            const msgs = [
                ...p.feed(frame.slice(0, cut)),
                ...p.feed(frame.slice(cut)),
            ];
            assert.strictEqual(msgs.length, 2, `cut at ${cut}`);
            assert.strictEqual(msgs[0].id, '12');
            assert.strictEqual(msgs[1].event, 'end');
            assert.strictEqual(p.lastEventId, '13');
        }
    });

    it('ignores comment lines (keepalives)', () => {
        const p = new SseParser();
        assert.deepStrictEqual(p.feed(': keepalive\n\n'), []);
        const msgs = p.feed(': another\ndata: x\n\n');
        assert.strictEqual(msgs.length, 1);
        assert.strictEqual(msgs[0].data, 'x');
    });

    it('joins multi-line data with newlines', () => {
        const p = new SseParser();
        const msgs = p.feed('data: line1\ndata: line2\n\n');
        assert.strictEqual(msgs[0].data, 'line1\nline2');
        assert.strictEqual(msgs[0].event, 'message');
    });

    it('handles CRLF line endings, including CR/LF split across chunks', () => {
        const p = new SseParser();
        let msgs = p.feed('data: a\r');
        assert.strictEqual(msgs.length, 0);
        msgs = p.feed('\n\r\n');
        assert.strictEqual(msgs.length, 1);
        assert.strictEqual(msgs[0].data, 'a');
    });

    it('a blank line without data dispatches nothing but resets the type', () => {
        const p = new SseParser();
        assert.deepStrictEqual(p.feed('event: end\n\n'), []);
        const msgs = p.feed('data: x\n\n');
        assert.strictEqual(msgs[0].event, 'message'); // type was reset
    });

    it('strips exactly one leading space from field values', () => {
        const p = new SseParser();
        const msgs = p.feed('data:  two spaces\ndata:none\n\n');
        assert.strictEqual(msgs[0].data, ' two spaces\nnone');
    });

    it('captures the retry field', () => {
        const p = new SseParser();
        p.feed('retry: 2500\n\n');
        assert.strictEqual(p.retryMs, 2500);
    });

    it('preserves lastEventId across messages without an id', () => {
        const p = new SseParser();
        p.feed('id: 5\ndata: a\n\n');
        const msgs = p.feed('data: b\n\n');
        assert.strictEqual(msgs[0].id, undefined);
        assert.strictEqual(p.lastEventId, '5');
    });
});

describe('openEventStream', () => {
    const servers: http.Server[] = [];

    after(async () => {
        for (const s of servers) {
            await new Promise<void>((resolve) => s.close(() => resolve()));
        }
    });

    function listen(
        handler: http.RequestListener,
    ): Promise<{ server: http.Server; url: string }> {
        const server = http.createServer(handler);
        servers.push(server);
        return new Promise((resolve) => {
            server.listen(0, '127.0.0.1', () => {
                const addr = server.address();
                if (addr && typeof addr === 'object') {
                    resolve({
                        server,
                        url: `http://127.0.0.1:${addr.port}/api/v1/jobs/j1/stream`,
                    });
                }
            });
        });
    }

    it('streams events and stops permanently on the end frame', async () => {
        const { url } = await listen((req, res) => {
            assert.strictEqual(req.headers.accept, 'text/event-stream');
            assert.strictEqual(req.headers.authorization, 'Bearer infp_k');
            res.writeHead(200, { 'Content-Type': 'text/event-stream' });
            res.write('id: 1\nevent: job.accepted\ndata: {"seq":1}\n\n');
            res.write(': keepalive\n\n');
            res.write('id: 2\nevent: vm.online\ndata: {"seq":2}\n\n');
            res.write('event: end\ndata: {"status":"Succeeded"}\n\n');
        });

        const messages: SseMessage[] = [];
        const states: StreamState[] = [];
        await new Promise<void>((resolve, reject) => {
            const timeout = setTimeout(
                () => reject(new Error('timed out waiting for end frame')),
                5000,
            );
            const stream = openEventStream({
                url,
                apiKey: 'infp_k',
                onMessage: (m) => {
                    messages.push(m);
                    if (m.event === 'end') {
                        clearTimeout(timeout);
                        stream.dispose();
                        resolve();
                    }
                },
                onStateChange: (s) => states.push(s),
            });
        });

        assert.deepStrictEqual(
            messages.map((m) => m.event),
            ['job.accepted', 'vm.online', 'end'],
        );
        assert.ok(states.includes('live'));
        // 'closed' must fire exactly once even though the consumer's
        // dispose() re-enters from inside the onMessage('end') callback —
        // the same shape JobDetailViewManager uses.
        assert.deepStrictEqual(
            states.filter((s) => s === 'closed'),
            ['closed'],
        );
    });

    it('reconnects with Last-Event-ID after a dropped connection', async () => {
        const seenLastEventIds: Array<string | undefined> = [];
        let connection = 0;
        const { url } = await listen((req, res) => {
            connection += 1;
            seenLastEventIds.push(
                req.headers['last-event-id'] as string | undefined,
            );
            res.writeHead(200, { 'Content-Type': 'text/event-stream' });
            if (connection === 1) {
                res.write('id: 41\nevent: log\ndata: {"seq":41}\n\n');
                res.end(); // server drops the stream mid-job
            } else {
                res.write('id: 42\nevent: end\ndata: {"status":"Failed"}\n\n');
            }
        });

        const events: string[] = [];
        await new Promise<void>((resolve, reject) => {
            const timeout = setTimeout(
                () => reject(new Error('timed out waiting for reconnect')),
                5000,
            );
            const stream = openEventStream({
                url,
                apiKey: 'infp_k',
                backoffMs: [50], // fast reconnect for the test
                onMessage: (m) => {
                    events.push(m.event);
                    if (m.event === 'end') {
                        clearTimeout(timeout);
                        stream.dispose();
                        resolve();
                    }
                },
            });
        });

        assert.deepStrictEqual(events, ['log', 'end']);
        assert.strictEqual(seenLastEventIds[0], undefined);
        // The reconnect resumes from the last seen id.
        assert.strictEqual(seenLastEventIds[1], '41');
    });

    it('resumes from the caller-provided lastEventId', async () => {
        let seen: string | undefined;
        const { url } = await listen((req, res) => {
            seen = req.headers['last-event-id'] as string | undefined;
            res.writeHead(200, { 'Content-Type': 'text/event-stream' });
            res.write('event: end\ndata: {"status":"Succeeded"}\n\n');
        });

        await new Promise<void>((resolve, reject) => {
            const timeout = setTimeout(
                () => reject(new Error('timed out')),
                5000,
            );
            const stream = openEventStream({
                url,
                apiKey: 'infp_k',
                lastEventId: '17',
                onMessage: (m) => {
                    if (m.event === 'end') {
                        clearTimeout(timeout);
                        stream.dispose();
                        resolve();
                    }
                },
            });
        });
        assert.strictEqual(seen, '17');
    });

    it('fails permanently on 404 without retrying', async () => {
        let connections = 0;
        const { url } = await listen((_req, res) => {
            connections += 1;
            res.writeHead(404);
            res.end();
        });

        const state = await new Promise<StreamState>((resolve, reject) => {
            const timeout = setTimeout(
                () => reject(new Error('timed out waiting for failed state')),
                5000,
            );
            openEventStream({
                url,
                apiKey: 'infp_k',
                backoffMs: [10],
                onMessage: () => undefined,
                onStateChange: (s) => {
                    if (s === 'failed') {
                        clearTimeout(timeout);
                        resolve(s);
                    }
                },
            });
        });

        assert.strictEqual(state, 'failed');
        // Give a would-be retry a moment to (incorrectly) fire.
        await new Promise((r) => setTimeout(r, 100));
        assert.strictEqual(connections, 1);
    });

    it('dispose() stops a retrying stream', async () => {
        // No server listening at this port → connection refused → retrying.
        const stream = openEventStream({
            url: 'http://127.0.0.1:1/api/v1/jobs/j1/stream',
            apiKey: 'infp_k',
            backoffMs: [10_000],
            onMessage: () => assert.fail('no messages expected'),
        });
        await new Promise((r) => setTimeout(r, 50));
        stream.dispose(); // must clear the pending reconnect timer
        await new Promise((r) => setTimeout(r, 50));
        // Nothing to assert beyond "the test exits" — a leaked timer would
        // keep the node:test process alive and fail the run.
    });
});
