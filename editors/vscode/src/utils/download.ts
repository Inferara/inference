import * as https from 'https';
import * as http from 'http';
import * as fs from 'fs';
import * as crypto from 'crypto';

/** Callback invoked during download with bytes received and total (if known). */
export type ProgressCallback = (
    received: number,
    total: number | undefined,
) => void;

export interface DownloadOptions {
    /** Absolute path where the downloaded file will be saved. */
    destPath: string;
    /** Optional progress callback. */
    onProgress?: ProgressCallback;
    /** Connection timeout in milliseconds (default: 15000). */
    timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 15_000;
const MAX_REDIRECTS = 5;

const SOCKET_TIMEOUT_MS = 15_000;

/**
 * Perform an HTTPS GET request following redirects.
 * Rejects HTTPS-to-HTTP downgrades.
 */
function followRedirects(
    url: string,
    remaining: number,
): Promise<http.IncomingMessage> {
    return new Promise((resolve, reject) => {
        const parsed = new URL(url);
        const requester = parsed.protocol === 'https:' ? https : http;

        const req = requester.get(url, (res) => {
            const status = res.statusCode ?? 0;

            if (status >= 300 && status < 400 && res.headers.location) {
                if (remaining <= 0) {
                    res.resume();
                    reject(new Error(`Too many redirects fetching ${url}`));
                    return;
                }
                const target = new URL(res.headers.location, url).href;
                const targetProtocol = new URL(target).protocol;
                if (parsed.protocol === 'https:' && targetProtocol === 'http:') {
                    res.resume();
                    reject(
                        new Error(
                            `Refusing HTTPS-to-HTTP redirect: ${url} -> ${target}`,
                        ),
                    );
                    return;
                }
                res.resume();
                followRedirects(target, remaining - 1).then(resolve, reject);
                return;
            }

            if (status < 200 || status >= 300) {
                res.resume();
                reject(new Error(`HTTP ${status} fetching ${url}`));
                return;
            }

            resolve(res);
        });

        req.setTimeout(SOCKET_TIMEOUT_MS, () => {
            req.destroy(new Error(`Connection timed out for ${url}`));
        });

        req.on('error', (err) =>
            reject(new Error(`Network error fetching ${url}: ${err.message}`)),
        );
    });
}

/**
 * Fetch a JSON document from a URL via HTTPS GET.
 * Follows up to 5 redirects. Rejects HTTPS-to-HTTP downgrades.
 */
export function fetchJson<T>(url: string): Promise<T> {
    return new Promise((resolve, reject) => {
        followRedirects(url, MAX_REDIRECTS).then(
            (res) => {
                const chunks: Buffer[] = [];
                res.on('data', (chunk: Buffer) => chunks.push(chunk));
                res.on('end', () => {
                    try {
                        const text = Buffer.concat(chunks).toString('utf-8');
                        resolve(JSON.parse(text) as T);
                    } catch (err) {
                        reject(
                            new Error(
                                `Failed to parse JSON from ${url}: ${err instanceof Error ? err.message : err}`,
                            ),
                        );
                    }
                });
                res.on('error', (err) =>
                    reject(
                        new Error(
                            `Error reading response from ${url}: ${err.message}`,
                        ),
                    ),
                );
            },
            (err) => reject(err),
        );
    });
}

/**
 * Download a file from a URL to destPath using streaming.
 * Uses a temp file (.partial suffix) and renames on completion.
 * Follows redirects (GitHub releases redirect to CDN).
 */
export function downloadFile(
    url: string,
    options: DownloadOptions,
): Promise<void> {
    const timeout = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    const partialPath = options.destPath + '.partial';

    return new Promise((resolve, reject) => {
        followRedirects(url, MAX_REDIRECTS).then(
            (res) => {
                const totalStr = res.headers['content-length'];
                const total = totalStr ? parseInt(totalStr, 10) : undefined;
                let received = 0;

                const ws = fs.createWriteStream(partialPath);

                res.on('data', (chunk: Buffer) => {
                    received += chunk.length;
                    options.onProgress?.(received, total);
                });

                res.pipe(ws);

                const cleanup = () => {
                    try {
                        fs.unlinkSync(partialPath);
                    } catch {
                        // ignore
                    }
                };

                ws.on('finish', () => {
                    try {
                        fs.renameSync(partialPath, options.destPath);
                        resolve();
                    } catch (err) {
                        cleanup();
                        reject(
                            new Error(
                                `Failed to save download to ${options.destPath}: ${err instanceof Error ? err.message : err}`,
                            ),
                        );
                    }
                });

                ws.on('error', (err) => {
                    cleanup();
                    reject(
                        new Error(
                            `Failed to write download: ${err.message}`,
                        ),
                    );
                });

                res.on('error', (err) => {
                    ws.destroy();
                    cleanup();
                    reject(
                        new Error(
                            `Download stream error from ${url}: ${err.message}`,
                        ),
                    );
                });

                // No-data timeout: if no bytes arrive for `timeout` ms, abort
                let dataTimer: ReturnType<typeof setTimeout> | undefined;
                const resetTimer = () => {
                    if (dataTimer) {
                        clearTimeout(dataTimer);
                    }
                    dataTimer = setTimeout(() => {
                        res.destroy();
                        ws.destroy();
                        cleanup();
                        reject(new Error(`Download timed out for ${url}`));
                    }, timeout);
                };
                resetTimer();
                res.on('data', resetTimer);
                res.on('end', () => {
                    if (dataTimer) {
                        clearTimeout(dataTimer);
                    }
                });
            },
            (err) => reject(err),
        );
    });
}

/**
 * Compute SHA-256 hash of a file.
 * Returns lowercase hex string.
 */
export function sha256File(filePath: string): Promise<string> {
    return new Promise((resolve, reject) => {
        const hash = crypto.createHash('sha256');
        const stream = fs.createReadStream(filePath);
        stream.on('data', (chunk) => hash.update(chunk));
        stream.on('end', () => resolve(hash.digest('hex')));
        stream.on('error', (err) =>
            reject(
                new Error(
                    `Failed to compute SHA-256 for ${filePath}: ${err.message}`,
                ),
            ),
        );
    });
}
