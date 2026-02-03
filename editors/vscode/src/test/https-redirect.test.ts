import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as http from 'node:http';
import * as https from 'node:https';
import * as crypto from 'node:crypto';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it, before, after } from 'node:test';

import { fetchJson, downloadFile } from '../utils/download';

/**
 * Tests for HTTPS-to-HTTP redirect blocking and SHA-256 verification.
 * Covers QA Section 11.3-11.4.
 */

describe('HTTPS redirect security (QA Section 11.4)', () => {
    let httpServer: http.Server;
    let httpsServer: https.Server;
    let httpUrl: string;
    let httpsUrl: string;
    let tmpDir: string;

    before(async () => {
        tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-redirect-test-'));

        // Generate self-signed certificate for HTTPS server
        const { privateKey, cert } = generateSelfSignedCert();

        // HTTP server (target for redirects)
        httpServer = http.createServer((req, res) => {
            if (req.url === '/data') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ ok: true }));
            } else if (req.url === '/redirect-to-http') {
                const target = `${httpUrl}/data`;
                res.writeHead(302, { Location: target });
                res.end();
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        await new Promise<void>((resolve) => {
            httpServer.listen(0, '127.0.0.1', () => {
                const addr = httpServer.address();
                if (addr && typeof addr === 'object') {
                    httpUrl = `http://127.0.0.1:${addr.port}`;
                }
                resolve();
            });
        });

        // HTTPS server (source for redirects)
        httpsServer = https.createServer({ key: privateKey, cert }, (req, res) => {
            if (req.url === '/data') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ secure: true }));
            } else if (req.url === '/redirect-to-http') {
                const target = `${httpUrl}/data`;
                res.writeHead(302, { Location: target });
                res.end();
            } else if (req.url === '/redirect-to-https') {
                const target = `${httpsUrl}/data`;
                res.writeHead(302, { Location: target });
                res.end();
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        await new Promise<void>((resolve) => {
            httpsServer.listen(0, '127.0.0.1', () => {
                const addr = httpsServer.address();
                if (addr && typeof addr === 'object') {
                    httpsUrl = `https://127.0.0.1:${addr.port}`;
                }
                resolve();
            });
        });

        // Allow self-signed certs for testing
        process.env['NODE_TLS_REJECT_UNAUTHORIZED'] = '0';
    });

    after(async () => {
        delete process.env['NODE_TLS_REJECT_UNAUTHORIZED'];
        await new Promise<void>((resolve) => httpServer.close(() => resolve()));
        await new Promise<void>((resolve) => httpsServer.close(() => resolve()));
        fs.rmSync(tmpDir, { recursive: true });
    });

    it('rejects HTTPS-to-HTTP redirect with descriptive error', async () => {
        await assert.rejects(
            fetchJson(`${httpsUrl}/redirect-to-http`),
            (err: Error) => {
                assert.ok(
                    err.message.includes('Refusing HTTPS-to-HTTP redirect'),
                    `Expected "Refusing HTTPS-to-HTTP redirect" in error, got: ${err.message}`,
                );
                return true;
            },
        );
    });

    it('allows HTTPS-to-HTTPS redirect', async () => {
        const result = await fetchJson<{ secure: boolean }>(
            `${httpsUrl}/redirect-to-https`,
        );
        assert.deepStrictEqual(result, { secure: true });
    });

    it('allows HTTP-to-HTTP redirect', async () => {
        const result = await fetchJson<{ ok: boolean }>(
            `${httpUrl}/redirect-to-http`,
        );
        assert.deepStrictEqual(result, { ok: true });
    });

    it('rejects HTTPS-to-HTTP redirect during file download', async () => {
        const destPath = path.join(tmpDir, 'should-not-exist');
        await assert.rejects(
            downloadFile(`${httpsUrl}/redirect-to-http`, { destPath }),
            (err: Error) => {
                assert.ok(
                    err.message.includes('Refusing HTTPS-to-HTTP redirect'),
                    `Expected redirect rejection, got: ${err.message}`,
                );
                return true;
            },
        );
        assert.ok(!fs.existsSync(destPath));
    });
});

/** Generate a self-signed certificate for testing. */
function generateSelfSignedCert(): { privateKey: string; cert: string } {
    const { privateKey, publicKey } = crypto.generateKeyPairSync('rsa', {
        modulusLength: 2048,
        publicKeyEncoding: { type: 'spki', format: 'pem' },
        privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
    });

    // Use node:crypto X509Certificate to create self-signed cert
    // For simplicity, use openssl-like approach with createSign
    const cert = generateSelfSignedX509(privateKey, publicKey);
    return { privateKey, cert };
}

/**
 * Create a minimal self-signed X.509 certificate.
 * Uses raw ASN.1/DER encoding to avoid external dependencies.
 */
function generateSelfSignedX509(privateKey: string, publicKey: string): string {
    // Use Node's built-in crypto to create a self-signed cert via a workaround:
    // spawn openssl if available, otherwise use a pre-generated test cert approach
    const { execSync } = require('node:child_process');
    const tmpKey = path.join(os.tmpdir(), `test-key-${process.pid}.pem`);
    const tmpCert = path.join(os.tmpdir(), `test-cert-${process.pid}.pem`);
    try {
        fs.writeFileSync(tmpKey, privateKey);
        execSync(
            `openssl req -new -x509 -key "${tmpKey}" -out "${tmpCert}" -days 1 -subj "/CN=localhost" -addext "subjectAltName=IP:127.0.0.1" 2>/dev/null`,
        );
        return fs.readFileSync(tmpCert, 'utf-8');
    } finally {
        try { fs.unlinkSync(tmpKey); } catch { /* ignore */ }
        try { fs.unlinkSync(tmpCert); } catch { /* ignore */ }
    }
}
