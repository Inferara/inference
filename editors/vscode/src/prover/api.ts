import * as https from 'https';
import * as http from 'http';
import type {
    ArtifactInfo,
    ArtifactListResponse,
    CancelResponse,
    EventsResponse,
    JobListResponse,
    JobResponse,
    JobResultResponse,
    ListJobsQuery,
    MeResponse,
    MetaResponse,
    SubmitJobOptions,
    SubmitJobRequest,
} from './types';

/** Connection/socket timeout for API requests (15 seconds). */
const REQUEST_TIMEOUT_MS = 15_000;
/** Cap on a JSON response body to avoid unbounded buffering. */
const MAX_JSON_RESPONSE_BYTES = 10 * 1024 * 1024;
/** Cap on a downloaded artifact (build and agent logs can exceed the JSON cap). */
export const MAX_ARTIFACT_BYTES = 64 * 1024 * 1024;
const MAX_REDIRECTS = 5;

/**
 * Raised when the server rejects the request for auth reasons.
 * - 401: missing/invalid key
 * - 403: insufficient scope
 */
export class AuthError extends Error {
    constructor(
        public readonly status: number,
        message: string,
    ) {
        super(message);
        this.name = 'AuthError';
    }
}

/**
 * Raised for any non-2xx, non-auth HTTP response.
 *
 * The server replies with RFC 9457 problem+json carrying a machine-readable
 * `code` extension and a human-actionable `detail` — e.g. submit fail-fast
 * rejections (`PROVIDER_UNAVAILABLE`, `VERIFIER_UNAVAILABLE`, both 503) or
 * `QUOTA_EXCEEDED`. When present they are surfaced here so callers can branch
 * on `code` and show `detail` instead of a bare status line.
 */
export class ApiError extends Error {
    constructor(
        public readonly status: number,
        message: string,
        /** Machine-readable error code from the problem+json body, if any. */
        public readonly code?: string,
        /** Human-readable detail from the problem+json body, if any. */
        public readonly detail?: string,
    ) {
        super(message);
        this.name = 'ApiError';
    }
}

/**
 * Best-effort extraction of an RFC 9457 problem+json body
 * (`{ title?, status?, detail?, code? }`). Anything unparseable → `{}` so the
 * caller falls back to a generic message.
 */
function problemDetails(body: string): { code?: string; detail?: string } {
    try {
        const p: unknown = JSON.parse(body);
        if (typeof p !== 'object' || p === null) {
            return {};
        }
        const rec = p as Record<string, unknown>;
        return {
            code: typeof rec.code === 'string' ? rec.code : undefined,
            detail: typeof rec.detail === 'string' ? rec.detail : undefined,
        };
    } catch {
        return {};
    }
}

interface RequestOptions {
    method: 'GET' | 'POST' | 'DELETE';
    /** Absolute request URL. */
    url: string;
    /** Bearer key (`infp_<keyid>_<secret>`); omitted → no Authorization header. */
    apiKey?: string;
    /** JSON request body for POST. */
    body?: unknown;
    /** Extra request headers (e.g. `Idempotency-Key`). */
    headers?: Record<string, string>;
    /** Response body cap; defaults to the JSON cap. */
    maxBytes?: number;
}

interface RawResponse {
    status: number;
    /** Raw bytes — JSON callers decode UTF-8, artifact downloads keep them. */
    body: Buffer;
    headers: http.IncomingHttpHeaders;
}

/**
 * Perform a single JSON request following redirects.
 *
 * Mirrors `utils/download.ts`: chooses http/https by protocol, rejects
 * HTTPS-to-HTTP downgrades, enforces a socket timeout, and caps the response
 * body. Resolves with the raw status + body so the caller can map status codes
 * to typed errors.
 */
function request(opts: RequestOptions, remaining: number): Promise<RawResponse> {
    return new Promise((resolve, reject) => {
        const parsed = new URL(opts.url);
        const requester = parsed.protocol === 'https:' ? https : http;

        const headers: Record<string, string> = {
            Accept: 'application/json',
            ...opts.headers,
        };
        if (opts.apiKey) {
            headers['Authorization'] = `Bearer ${opts.apiKey}`;
        }
        const payload =
            opts.body !== undefined ? JSON.stringify(opts.body) : undefined;
        if (payload !== undefined) {
            headers['Content-Type'] = 'application/json';
            headers['Content-Length'] = String(Buffer.byteLength(payload));
        }

        const req = requester.request(
            opts.url,
            { method: opts.method, headers },
            (res) => {
                const status = res.statusCode ?? 0;

                // Redirects: re-issue the same method/body at the new location.
                if (status >= 300 && status < 400 && res.headers.location) {
                    if (remaining <= 0) {
                        res.resume();
                        reject(new Error(`Too many redirects fetching ${opts.url}`));
                        return;
                    }
                    const targetUrl = new URL(res.headers.location, opts.url);
                    if (
                        parsed.protocol === 'https:' &&
                        targetUrl.protocol === 'http:'
                    ) {
                        res.resume();
                        reject(
                            new Error(
                                `Refusing HTTPS-to-HTTP redirect: ${opts.url} -> ${targetUrl.href}`,
                            ),
                        );
                        return;
                    }
                    res.resume();
                    // An upload carries the user's program and specifications;
                    // it goes only to the origin they configured.
                    if (targetUrl.origin !== parsed.origin && payload !== undefined) {
                        reject(
                            new Error(
                                `Refusing to send the request body to another origin: ${opts.url} -> ${targetUrl.origin}`,
                            ),
                        );
                        return;
                    }
                    // Never replay credentials (Authorization, Idempotency-Key)
                    // to a different origin — a misdirected redirect must not
                    // leak the bearer token.
                    const next =
                        targetUrl.origin === parsed.origin
                            ? opts
                            : { ...opts, apiKey: undefined, headers: undefined };
                    request({ ...next, url: targetUrl.href }, remaining - 1).then(
                        resolve,
                        reject,
                    );
                    return;
                }

                const chunks: Buffer[] = [];
                let totalBytes = 0;
                const maxBytes = opts.maxBytes ?? MAX_JSON_RESPONSE_BYTES;
                res.on('data', (chunk: Buffer) => {
                    totalBytes += chunk.length;
                    if (totalBytes > maxBytes) {
                        res.destroy();
                        reject(
                            new Error(
                                `Response too large (>${maxBytes} bytes) from ${opts.url}`,
                            ),
                        );
                        return;
                    }
                    chunks.push(chunk);
                });
                res.on('end', () => {
                    resolve({ status, body: Buffer.concat(chunks), headers: res.headers });
                });
                res.on('error', (err) =>
                    reject(
                        new Error(
                            `Error reading response from ${opts.url}: ${err.message}`,
                        ),
                    ),
                );
            },
        );

        req.setTimeout(REQUEST_TIMEOUT_MS, () => {
            req.destroy(new Error(`Connection timed out for ${opts.url}`));
        });
        req.on('error', (err) =>
            reject(new Error(`Network error fetching ${opts.url}: ${err.message}`)),
        );

        if (payload !== undefined) {
            req.write(payload);
        }
        req.end();
    });
}

/** Join a base URL and a path, tolerating a trailing slash on the base. */
export function buildUrl(serverUrl: string, path: string): string {
    const base = serverUrl.replace(/\/+$/, '');
    const suffix = path.startsWith('/') ? path : `/${path}`;
    return `${base}${suffix}`;
}

/** Throw the typed error for a non-2xx response (problem+json aware). */
function throwForStatus(url: string, res: RawResponse): void {
    if (res.status === 401 || res.status === 403) {
        throw new AuthError(
            res.status,
            res.status === 401
                ? 'Unauthorized: missing or invalid API key.'
                : 'Forbidden: API key lacks the required scope.',
        );
    }
    if (res.status < 200 || res.status >= 300) {
        const { code, detail } = problemDetails(res.body.toString('utf-8'));
        const message = detail
            ? `${detail}${code ? ` (${code})` : ''}`
            : `HTTP ${res.status} from ${url}`;
        throw new ApiError(res.status, message, code, detail);
    }
}

/** Map a raw response to a parsed JSON body or a typed error. */
function parse<T>(url: string, res: RawResponse): T {
    throwForStatus(url, res);
    try {
        return JSON.parse(res.body.toString('utf-8')) as T;
    } catch (err) {
        throw new Error(
            `Failed to parse JSON from ${url}: ${err instanceof Error ? err.message : err}`,
        );
    }
}

/**
 * Typed REST client for the proof server public API.
 *
 * Stateless aside from `serverUrl`/`apiKey`; create one per request batch with
 * the current configuration. All methods throw {@link AuthError} on 401/403 and
 * {@link ApiError} on other non-2xx responses.
 */
export class ProverApi {
    constructor(
        private readonly serverUrl: string,
        private readonly apiKey: string,
    ) {}

    private async get<T>(path: string): Promise<T> {
        const url = buildUrl(this.serverUrl, path);
        const res = await request(
            { method: 'GET', url, apiKey: this.apiKey },
            MAX_REDIRECTS,
        );
        return parse<T>(url, res);
    }

    private async post<T>(
        path: string,
        body?: unknown,
        headers?: Record<string, string>,
    ): Promise<T> {
        const url = buildUrl(this.serverUrl, path);
        const res = await request(
            { method: 'POST', url, apiKey: this.apiKey, body, headers },
            MAX_REDIRECTS,
        );
        return parse<T>(url, res);
    }

    /** `GET /api/v1/meta` — upload cap, budgets, and the accepted toolchain. */
    async getMeta(): Promise<MetaResponse> {
        return this.get<MetaResponse>('/api/v1/meta');
    }

    /** `GET /api/v1/me` — the account and key the API key authenticates as. */
    async getMe(): Promise<MeResponse> {
        return this.get<MeResponse>('/api/v1/me');
    }

    /**
     * `GET /api/v1/jobs` — the caller's jobs, newest first. Without a query
     * the server returns its default page (50); `before` pages backwards.
     */
    async listJobs(query: ListJobsQuery = {}): Promise<JobResponse[]> {
        const params = new URLSearchParams();
        if (query.status) {
            params.set('status', query.status);
        }
        if (query.limit !== undefined) {
            params.set('limit', String(query.limit));
        }
        if (query.before) {
            params.set('before', query.before);
        }
        const qs = params.toString();
        const res = await this.get<JobListResponse | JobResponse[]>(
            qs ? `/api/v1/jobs?${qs}` : '/api/v1/jobs',
        );
        // Tolerate either a paged envelope or a bare array.
        return Array.isArray(res) ? res : (res.jobs ?? []);
    }

    /** `GET /api/v1/jobs/{id}` — fetch a single job's detail. */
    async getJob(id: string): Promise<JobResponse> {
        return this.get<JobResponse>(
            `/api/v1/jobs/${encodeURIComponent(id)}`,
        );
    }

    /**
     * `POST /api/v1/jobs` — submit a `.v` file (JSON + base64) → 202.
     *
     * `idempotencyKey` must be a UUID (the server parses the header as a
     * Guid); resubmitting with the same key replays the original 202 instead
     * of creating a second job.
     *
     * `options` is empty for normal use. The server always picks provider and
     * agent from deployment config; callers may set only the optional
     * wall-clock budget.
     */
    async submitJob(
        filename: string,
        content: Uint8Array,
        options: SubmitJobOptions = {},
        idempotencyKey?: string,
    ): Promise<JobResponse> {
        return (await this.submitJobWithMeta(filename, content, options, idempotencyKey)).job;
    }

    /**
     * {@link submitJob}, plus whether the server replayed an existing job for
     * this key (`Idempotent-Replayed`; null when the server does not say).
     */
    async submitJobWithMeta(
        filename: string,
        content: Uint8Array,
        options: SubmitJobOptions = {},
        idempotencyKey?: string,
    ): Promise<{ job: JobResponse; replayHeader: boolean | null }> {
        const body: SubmitJobRequest = {
            schemaVersion: 1,
            filename,
            contentBase64: Buffer.from(content).toString('base64'),
            options,
        };
        const url = buildUrl(this.serverUrl, '/api/v1/jobs');
        const res = await request(
            {
                method: 'POST',
                url,
                apiKey: this.apiKey,
                body,
                headers: idempotencyKey ? { 'Idempotency-Key': idempotencyKey } : undefined,
            },
            MAX_REDIRECTS,
        );
        const job = parse<JobResponse>(url, res);
        const raw = res.headers['idempotent-replayed'];
        const value = (Array.isArray(raw) ? raw[0] : raw)?.trim().toLowerCase();
        return { job, replayHeader: value === 'true' ? true : value === 'false' ? false : null };
    }

    /**
     * `POST /api/v1/jobs/{id}/cancel`. Accepted/Queued cancel immediately;
     * active states go through `Canceling` (the worker acknowledges via its
     * heartbeat). Already-terminal or Verifying → 409 `NOT_CANCELABLE`.
     */
    async cancelJob(id: string): Promise<CancelResponse> {
        return this.post<CancelResponse>(
            `/api/v1/jobs/${encodeURIComponent(id)}/cancel`,
        );
    }

    /**
     * `DELETE /api/v1/jobs/{id}` — hide a terminal job (purged by retention
     * later). Non-terminal → 409 `NOT_TERMINAL`.
     */
    async deleteJob(id: string): Promise<void> {
        const url = buildUrl(this.serverUrl, `/api/v1/jobs/${encodeURIComponent(id)}`);
        const res = await request(
            { method: 'DELETE', url, apiKey: this.apiKey },
            MAX_REDIRECTS,
        );
        throwForStatus(url, res);
    }

    /** `GET /api/v1/jobs/{id}/artifacts` — the artifacts visible to the caller. */
    async listArtifacts(id: string): Promise<ArtifactInfo[]> {
        const res = await this.get<ArtifactListResponse>(
            `/api/v1/jobs/${encodeURIComponent(id)}/artifacts`,
        );
        return res.artifacts ?? [];
    }

    /**
     * `GET /api/v1/jobs/{id}/result` — terminal result metadata + artifacts.
     * Terminal jobs only; non-terminal → 409 `NOT_TERMINAL`.
     */
    async getResult(id: string): Promise<JobResultResponse> {
        return this.get<JobResultResponse>(
            `/api/v1/jobs/${encodeURIComponent(id)}/result`,
        );
    }

    /**
     * `GET /api/v1/jobs/{id}/events?after={seq}` — durable event replay
     * (polling fallback when SSE is unavailable). Batches of ≤500.
     */
    async getEvents(id: string, after: number, wait = 0): Promise<EventsResponse> {
        return this.get<EventsResponse>(
            `/api/v1/jobs/${encodeURIComponent(id)}/events?after=${after}&wait=${wait}`,
        );
    }

    /**
     * `GET /api/v1/jobs/{id}/artifacts/{artifactId}` — raw artifact bytes
     * streamed from the server's Postgres-backed store. Verify the returned
     * content against `ArtifactInfo.sha256` before trusting it.
     */
    async downloadArtifact(jobId: string, artifactId: string): Promise<Buffer> {
        const url = buildUrl(
            this.serverUrl,
            `/api/v1/jobs/${encodeURIComponent(jobId)}/artifacts/${encodeURIComponent(artifactId)}`,
        );
        const res = await request(
            { method: 'GET', url, apiKey: this.apiKey, maxBytes: MAX_ARTIFACT_BYTES },
            MAX_REDIRECTS,
        );
        throwForStatus(url, res);
        return res.body;
    }
}
