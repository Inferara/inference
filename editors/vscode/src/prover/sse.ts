/**
 * Hand-rolled Server-Sent Events client for `GET /api/v1/jobs/{id}/stream`.
 *
 * The browser `EventSource` cannot send an `Authorization` header and the
 * extension ships zero runtime dependencies, so this implements the essential
 * subset of the WHATWG event-stream format over node `http`/`https`:
 *
 * - lines split on CRLF / LF / CR, safe across chunk boundaries
 * - `:` comment lines (the server's 15 s `: keepalive` frames)
 * - `id:` / `event:` / `data:` / `retry:` fields, multi-line `data`
 * - dispatch on blank line; `Last-Event-ID` resume on reconnect
 *
 * Server specifics (PublicEndpoints.StreamEvents): every job event arrives as
 * `id: <seq>` + `event: <type>` + `data: <envelope json>`; a final
 * `event: end` + `data: {"status":"<terminal>"}` closes the stream once the
 * job reaches a terminal state (except `Lost`, which may still retry).
 */

import * as http from 'http';
import * as https from 'https';
import { StringDecoder } from 'string_decoder';

/** One dispatched SSE message. */
export interface SseMessage {
    /** Event type (`event:` field), defaulting to `message`. */
    event: string;
    /** Concatenated `data:` lines, joined with `\n`. */
    data: string;
    /** `id:` field carried by THIS message, if any. */
    id?: string;
}

/**
 * Incremental parser for the SSE wire format. Pure (no I/O) — feed it decoded
 * text chunks, collect dispatched messages. Tracks `lastEventId` across
 * messages the way EventSource does (only `id:` lines update it).
 */
export class SseParser {
    private buffer = '';
    private dataLines: string[] = [];
    private eventType = '';
    private messageId: string | undefined;
    private _lastEventId = '';
    private _retryMs: number | undefined;

    /** Last seen `id:` value (persists across messages, for Last-Event-ID). */
    get lastEventId(): string {
        return this._lastEventId;
    }

    /** Server-requested reconnection delay (`retry:` field), if any. */
    get retryMs(): number | undefined {
        return this._retryMs;
    }

    /** Feed a decoded text chunk; returns messages completed by this chunk. */
    feed(text: string): SseMessage[] {
        this.buffer += text;
        const messages: SseMessage[] = [];

        // Hold back a trailing CR — it may be the first half of a CRLF pair
        // split across chunks.
        let parseable = this.buffer;
        let held = '';
        if (parseable.endsWith('\r')) {
            held = '\r';
            parseable = parseable.slice(0, -1);
        }

        const lines = parseable.split(/\r\n|\r|\n/);
        // The last element is an incomplete line (no terminator yet).
        this.buffer = lines.pop()! + held;

        for (const line of lines) {
            const msg = this.processLine(line);
            if (msg) {
                messages.push(msg);
            }
        }
        return messages;
    }

    private processLine(line: string): SseMessage | null {
        if (line === '') {
            return this.dispatch();
        }
        if (line.startsWith(':')) {
            return null; // comment (keepalive)
        }

        const colon = line.indexOf(':');
        const field = colon === -1 ? line : line.slice(0, colon);
        let value = colon === -1 ? '' : line.slice(colon + 1);
        if (value.startsWith(' ')) {
            value = value.slice(1);
        }

        switch (field) {
            case 'data':
                this.dataLines.push(value);
                break;
            case 'event':
                this.eventType = value;
                break;
            case 'id':
                if (!value.includes('\0')) {
                    this.messageId = value;
                    this._lastEventId = value;
                }
                break;
            case 'retry': {
                const ms = Number(value);
                if (Number.isInteger(ms) && ms >= 0) {
                    this._retryMs = ms;
                }
                break;
            }
            default:
                break; // unknown field: ignore per spec
        }
        return null;
    }

    private dispatch(): SseMessage | null {
        if (this.dataLines.length === 0) {
            // No data → no event, but the type buffer still resets.
            this.eventType = '';
            this.messageId = undefined;
            return null;
        }
        const msg: SseMessage = {
            event: this.eventType || 'message',
            data: this.dataLines.join('\n'),
            id: this.messageId,
        };
        this.dataLines = [];
        this.eventType = '';
        this.messageId = undefined;
        return msg;
    }
}

/** Connection lifecycle states reported via `onStateChange`. */
export type StreamState =
    /** Attempting the first connection. */
    | 'connecting'
    /** Connected and receiving the stream. */
    | 'live'
    /** Connection lost — waiting to reconnect (poll fallback should run). */
    | 'retrying'
    /** Permanently failed (401/403/404) — no further attempts. */
    | 'failed'
    /** Closed: `event: end` received or `dispose()` called. */
    | 'closed';

export interface EventStreamOptions {
    /** Absolute stream URL (`…/api/v1/jobs/{id}/stream`). */
    url: string;
    /** Bearer key. */
    apiKey: string;
    /** Resume point: highest `seq` already seen, sent as `Last-Event-ID`. */
    lastEventId?: string;
    /** Every dispatched message, including the final `end`. */
    onMessage: (msg: SseMessage) => void;
    /** Lifecycle notifications (drive the poll fallback + UI note). */
    onStateChange?: (state: StreamState, detail?: string) => void;
    /** Reconnect backoff schedule override (tests). */
    backoffMs?: number[];
    /** Silence watchdog override (tests). Default 45 s (keepalives are 15 s). */
    silenceTimeoutMs?: number;
}

/** Reconnect backoff; the server's `retry:` field, when present, wins. */
const DEFAULT_BACKOFF_MS = [1_000, 2_000, 5_000, 10_000, 30_000];
/** Reconnect if the wire is silent this long (server keepalives every 15 s). */
const DEFAULT_SILENCE_MS = 45_000;

/**
 * Open a resilient SSE connection. Reconnects with backoff on any transport
 * error or silence, resuming from the last seen event id (the server replays
 * missed events from the durable `job_events` table — gap-free). Stops
 * permanently on `event: end`, on auth/404 responses, or on `dispose()`.
 */
export function openEventStream(opts: EventStreamOptions): { dispose(): void } {
    const backoff = opts.backoffMs ?? DEFAULT_BACKOFF_MS;
    const silenceMs = opts.silenceTimeoutMs ?? DEFAULT_SILENCE_MS;

    let disposed = false;
    let permanent = false;
    let attempts = 0;
    let lastEventId = opts.lastEventId ?? '';
    let req: http.ClientRequest | undefined;
    let reconnectTimer: NodeJS.Timeout | undefined;
    let watchdog: NodeJS.Timeout | undefined;
    let retryMsFromServer: number | undefined;

    let closedNotified = false;
    const state = (s: StreamState, detail?: string) => {
        // 'closed' is delivered exactly once, even when the consumer's
        // dispose() re-enters from inside an onMessage('end') callback.
        if (s === 'closed') {
            if (closedNotified) {
                return;
            }
            closedNotified = true;
        } else if (disposed) {
            return;
        }
        opts.onStateChange?.(s, detail);
    };

    const stopTimers = () => {
        if (watchdog) {
            clearTimeout(watchdog);
            watchdog = undefined;
        }
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = undefined;
        }
    };

    const armWatchdog = () => {
        if (watchdog) {
            clearTimeout(watchdog);
        }
        watchdog = setTimeout(() => {
            req?.destroy(new Error('SSE silence timeout'));
        }, silenceMs);
    };

    const scheduleReconnect = (reason: string) => {
        if (disposed || permanent || reconnectTimer) {
            return;
        }
        const delay =
            retryMsFromServer ??
            backoff[Math.min(attempts, backoff.length - 1)];
        attempts += 1;
        state('retrying', reason);
        reconnectTimer = setTimeout(() => {
            reconnectTimer = undefined;
            connect();
        }, delay);
    };

    const connect = () => {
        if (disposed || permanent) {
            return;
        }
        state(attempts === 0 ? 'connecting' : 'retrying', 'connecting');

        const parsed = new URL(opts.url);
        const requester = parsed.protocol === 'https:' ? https : http;
        const headers: Record<string, string> = {
            Accept: 'text/event-stream',
            'Cache-Control': 'no-cache',
            Authorization: `Bearer ${opts.apiKey}`,
        };
        if (lastEventId) {
            headers['Last-Event-ID'] = lastEventId;
        }

        req = requester.request(opts.url, { method: 'GET', headers }, (res) => {
            const status = res.statusCode ?? 0;
            if (status === 401 || status === 403 || status === 404) {
                permanent = true;
                res.resume();
                stopTimers();
                state(
                    'failed',
                    status === 404
                        ? 'job not found (removed by retention?)'
                        : 'authentication failed',
                );
                return;
            }
            if (status !== 200) {
                res.resume();
                scheduleReconnect(`HTTP ${status}`);
                return;
            }

            attempts = 0; // healthy connection: reset the backoff ladder
            const parser = new SseParser();
            const decoder = new StringDecoder('utf8');
            state('live');
            armWatchdog();

            res.on('data', (chunk: Buffer) => {
                armWatchdog();
                for (const msg of parser.feed(decoder.write(chunk))) {
                    lastEventId = parser.lastEventId || lastEventId;
                    retryMsFromServer = parser.retryMs ?? retryMsFromServer;
                    if (msg.event === 'end') {
                        permanent = true;
                    }
                    if (!disposed) {
                        opts.onMessage(msg);
                    }
                    if (permanent) {
                        stopTimers();
                        req?.destroy();
                        state('closed', 'job reached a terminal state');
                        return;
                    }
                }
            });
            res.on('end', () => scheduleReconnect('server closed the stream'));
            res.on('error', (err) => scheduleReconnect(err.message));
        });

        // Covers both connection establishment and mid-stream silence: the
        // watchdog destroys the request, which lands in the error handler.
        armWatchdog();
        req.on('error', (err) => scheduleReconnect(err.message));
        req.end();
    };

    connect();

    return {
        dispose(): void {
            if (disposed) {
                return;
            }
            disposed = true;
            stopTimers();
            req?.destroy();
            state('closed', 'disposed');
        },
    };
}
