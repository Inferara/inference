import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as http from 'node:http';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it, before, after } from 'node:test';

import { fetchJson, downloadFile, sha256File } from '../utils/download';

describe('download', () => {
    let server: http.Server;
    let baseUrl: string;
    let tmpDir: string;

    before(async () => {
        tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-download-test-'));

        server = http.createServer((req, res) => {
            if (req.url === '/json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ name: 'inference', version: '1.0.0' }));
            } else if (req.url === '/not-found') {
                res.writeHead(404);
                res.end('Not Found');
            } else if (req.url === '/redirect') {
                res.writeHead(302, { Location: '/json' });
                res.end();
            } else if (req.url === '/redirect-chain-1') {
                res.writeHead(302, { Location: '/redirect-chain-2' });
                res.end();
            } else if (req.url === '/redirect-chain-2') {
                res.writeHead(302, { Location: '/json' });
                res.end();
            } else if (req.url === '/binary') {
                const data = Buffer.from('hello world binary content');
                res.writeHead(200, {
                    'Content-Type': 'application/octet-stream',
                    'Content-Length': String(data.length),
                });
                res.end(data);
            } else if (req.url === '/binary-no-length') {
                const data = Buffer.from('no content length');
                res.writeHead(200, {
                    'Content-Type': 'application/octet-stream',
                });
                res.end(data);
            } else if (req.url === '/invalid-json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end('not valid json{{{');
            } else if (req.url === '/empty-json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end('');
            } else if (req.url === '/json-array') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([1, 2, 3]));
            } else {
                res.writeHead(500);
                res.end('Internal Server Error');
            }
        });

        await new Promise<void>((resolve) => {
            server.listen(0, '127.0.0.1', () => {
                const addr = server.address();
                if (addr && typeof addr === 'object') {
                    baseUrl = `http://127.0.0.1:${addr.port}`;
                }
                resolve();
            });
        });
    });

    after(async () => {
        await new Promise<void>((resolve) => server.close(() => resolve()));
        fs.rmSync(tmpDir, { recursive: true });
    });

    describe('fetchJson', () => {
        it('parses valid JSON from a local HTTP server', async () => {
            const result = await fetchJson<{ name: string; version: string }>(
                `${baseUrl}/json`,
            );
            assert.deepStrictEqual(result, {
                name: 'inference',
                version: '1.0.0',
            });
        });

        it('rejects on non-200 status', async () => {
            await assert.rejects(
                fetchJson(`${baseUrl}/not-found`),
                (err: Error) => {
                    assert.ok(err.message.includes('404'));
                    return true;
                },
            );
        });

        it('follows redirects', async () => {
            const result = await fetchJson<{ name: string }>(
                `${baseUrl}/redirect`,
            );
            assert.strictEqual(result.name, 'inference');
        });

        it('rejects on invalid JSON', async () => {
            await assert.rejects(
                fetchJson(`${baseUrl}/invalid-json`),
                (err: Error) => {
                    assert.ok(err.message.includes('Failed to parse JSON'));
                    return true;
                },
            );
        });

        it('follows multi-hop redirect chain', async () => {
            const result = await fetchJson<{ name: string }>(
                `${baseUrl}/redirect-chain-1`,
            );
            assert.strictEqual(result.name, 'inference');
        });

        it('rejects on empty response body', async () => {
            await assert.rejects(
                fetchJson(`${baseUrl}/empty-json`),
                (err: Error) => {
                    assert.ok(err.message.includes('Failed to parse JSON'));
                    return true;
                },
            );
        });

        it('parses JSON arrays', async () => {
            const result = await fetchJson<number[]>(`${baseUrl}/json-array`);
            assert.deepStrictEqual(result, [1, 2, 3]);
        });

        it('rejects on server error (500)', async () => {
            await assert.rejects(
                fetchJson(`${baseUrl}/server-error`),
                (err: Error) => {
                    assert.ok(err.message.includes('500'));
                    return true;
                },
            );
        });
    });

    describe('downloadFile', () => {
        it('downloads to the correct path', async () => {
            const destPath = path.join(tmpDir, 'downloaded-file');
            await downloadFile(`${baseUrl}/binary`, { destPath });
            assert.ok(fs.existsSync(destPath));
            const content = fs.readFileSync(destPath, 'utf-8');
            assert.strictEqual(content, 'hello world binary content');
        });

        it('calls progress callback with received/total bytes', async () => {
            const destPath = path.join(tmpDir, 'downloaded-progress');
            const progressCalls: Array<{
                received: number;
                total: number | undefined;
            }> = [];

            await downloadFile(`${baseUrl}/binary`, {
                destPath,
                onProgress: (received, total) => {
                    progressCalls.push({ received, total });
                },
            });

            assert.ok(progressCalls.length > 0);
            const lastCall = progressCalls[progressCalls.length - 1];
            assert.strictEqual(
                lastCall.received,
                Buffer.from('hello world binary content').length,
            );
            assert.strictEqual(lastCall.total, lastCall.received);
        });

        it('calls progress callback without total when content-length is absent', async () => {
            const destPath = path.join(tmpDir, 'downloaded-no-length');
            const progressCalls: Array<{
                received: number;
                total: number | undefined;
            }> = [];

            await downloadFile(`${baseUrl}/binary-no-length`, {
                destPath,
                onProgress: (received, total) => {
                    progressCalls.push({ received, total });
                },
            });

            assert.ok(progressCalls.length > 0);
            const lastCall = progressCalls[progressCalls.length - 1];
            assert.strictEqual(lastCall.total, undefined);
        });

        it('cleans up .partial file on error', async () => {
            const destPath = path.join(tmpDir, 'should-not-exist');
            const partialPath = destPath + '.partial';

            await assert.rejects(
                downloadFile(`${baseUrl}/not-found`, { destPath }),
            );

            assert.ok(!fs.existsSync(partialPath));
            assert.ok(!fs.existsSync(destPath));
        });
    });

    describe('downloadFile edge cases', () => {
        it('overwrites existing file at destPath', async () => {
            const destPath = path.join(tmpDir, 'overwrite-test');
            fs.writeFileSync(destPath, 'old content');
            await downloadFile(`${baseUrl}/binary`, { destPath });
            const content = fs.readFileSync(destPath, 'utf-8');
            assert.strictEqual(content, 'hello world binary content');
        });

        it('rejects on server error', async () => {
            const destPath = path.join(tmpDir, 'server-error-download');
            await assert.rejects(
                downloadFile(`${baseUrl}/server-error`, { destPath }),
            );
            assert.ok(!fs.existsSync(destPath));
        });

        it('follows redirects when downloading files', async () => {
            const destPath = path.join(tmpDir, 'redirect-download');
            await downloadFile(`${baseUrl}/redirect-chain-1`, { destPath });
            assert.ok(fs.existsSync(destPath));
            const content = fs.readFileSync(destPath, 'utf-8');
            assert.ok(content.includes('inference'));
        });
    });

    describe('sha256File', () => {
        it('returns correct hex hash for a known file', async () => {
            const filePath = path.join(tmpDir, 'hash-test');
            fs.writeFileSync(filePath, 'hello');

            const hash = await sha256File(filePath);

            assert.strictEqual(
                hash,
                '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824',
            );
        });

        it('rejects for a nonexistent file', async () => {
            await assert.rejects(
                sha256File(path.join(tmpDir, 'nonexistent-file')),
                (err: Error) => {
                    assert.ok(err.message.includes('Failed to compute SHA-256'));
                    return true;
                },
            );
        });

        it('returns correct hash for empty file', async () => {
            const filePath = path.join(tmpDir, 'empty-hash-test');
            fs.writeFileSync(filePath, '');
            const hash = await sha256File(filePath);
            assert.strictEqual(
                hash,
                'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
            );
        });

        it('returns consistent hash for same content', async () => {
            const path1 = path.join(tmpDir, 'hash-consistent-1');
            const path2 = path.join(tmpDir, 'hash-consistent-2');
            fs.writeFileSync(path1, 'same content');
            fs.writeFileSync(path2, 'same content');

            const hash1 = await sha256File(path1);
            const hash2 = await sha256File(path2);
            assert.strictEqual(hash1, hash2);
        });

        it('returns different hashes for different content', async () => {
            const path1 = path.join(tmpDir, 'hash-diff-1');
            const path2 = path.join(tmpDir, 'hash-diff-2');
            fs.writeFileSync(path1, 'content A');
            fs.writeFileSync(path2, 'content B');

            const hash1 = await sha256File(path1);
            const hash2 = await sha256File(path2);
            assert.notStrictEqual(hash1, hash2);
        });
    });
});
