import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as http from 'node:http';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it, before, after } from 'node:test';
import { execSync } from 'node:child_process';

import { installToolchain } from '../toolchain/installation';
import type { PlatformInfo } from '../toolchain/platform';

/**
 * Tests for installation error paths.
 * Covers QA Sections 9.5-9.8 and 4.1.10.
 */

const isUnix = process.platform !== 'win32';

const linuxPlatform: PlatformInfo = {
    id: 'linux-x64',
    archiveExtension: '.tar.gz',
    binaryName: 'infs',
};

describe('installation failure paths (QA Section 9.5-9.8)', { skip: !isUnix ? 'Unix-only tests' : undefined }, () => {
    let server: http.Server;
    let baseUrl: string;
    let tmpDir: string;
    let originalDistServer: string | undefined;
    let originalInferenceHome: string | undefined;

    before(async () => {
        tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-fail-test-'));
        originalDistServer = process.env['INFS_DIST_SERVER'];
        originalInferenceHome = process.env['INFERENCE_HOME'];

        const archiveDir = path.join(tmpDir, 'server');
        fs.mkdirSync(archiveDir, { recursive: true });

        // Build a valid archive with an infs binary
        const archiveSourceDir = path.join(tmpDir, 'archive-source');
        fs.mkdirSync(archiveSourceDir, { recursive: true });
        fs.writeFileSync(
            path.join(archiveSourceDir, 'infs'),
            '#!/bin/sh\necho "infs install ran" && exit 0\n',
        );
        fs.chmodSync(path.join(archiveSourceDir, 'infs'), 0o755);
        const validArchive = path.join(archiveDir, 'infs-linux-x64.tar.gz');
        execSync(`tar -czf "${validArchive}" -C "${archiveSourceDir}" .`);
        fs.rmSync(archiveSourceDir, { recursive: true });

        const { sha256File } = await import('../utils/download');
        const validHash = await sha256File(validArchive);

        // Build an archive without the infs binary (only a dummy file)
        const noBinSourceDir = path.join(tmpDir, 'nobin-source');
        fs.mkdirSync(noBinSourceDir, { recursive: true });
        fs.writeFileSync(path.join(noBinSourceDir, 'README'), 'no binary here');
        const noBinArchive = path.join(archiveDir, 'infs-linux-x64-nobin.tar.gz');
        execSync(`tar -czf "${noBinArchive}" -C "${noBinSourceDir}" .`);
        fs.rmSync(noBinSourceDir, { recursive: true });
        const noBinHash = await sha256File(noBinArchive);

        // Build an archive with infs that exits non-zero on install
        const failBinSourceDir = path.join(tmpDir, 'failbin-source');
        fs.mkdirSync(failBinSourceDir, { recursive: true });
        fs.writeFileSync(
            path.join(failBinSourceDir, 'infs'),
            '#!/bin/sh\necho "installation error" >&2 && exit 1\n',
        );
        fs.chmodSync(path.join(failBinSourceDir, 'infs'), 0o755);
        const failBinArchive = path.join(archiveDir, 'infs-linux-x64-failbin.tar.gz');
        execSync(`tar -czf "${failBinArchive}" -C "${failBinSourceDir}" .`);
        fs.rmSync(failBinSourceDir, { recursive: true });
        const failBinHash = await sha256File(failBinArchive);

        server = http.createServer((req, res) => {
            if (req.url === '/releases.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-linux-x64.tar.gz`, sha256: validHash },
                        ],
                    },
                ]));
            } else if (req.url === '/releases-bad-hash.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-linux-x64.tar.gz`, sha256: 'deadbeef' },
                        ],
                    },
                ]));
            } else if (req.url === '/releases-nobin.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-linux-x64-nobin.tar.gz`, sha256: noBinHash },
                        ],
                    },
                ]));
            } else if (req.url === '/releases-failbin.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-linux-x64-failbin.tar.gz`, sha256: failBinHash },
                        ],
                    },
                ]));
            } else if (req.url === '/releases-no-platform.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-macos-arm64.tar.gz`, sha256: 'abc' },
                        ],
                    },
                ]));
            } else if (req.url === '/releases-network-error.json') {
                // Destroy the socket to simulate a network error
                req.socket.destroy();
            } else {
                const filePath = path.join(archiveDir, path.basename(req.url ?? ''));
                if (fs.existsSync(filePath)) {
                    const stat = fs.statSync(filePath);
                    res.writeHead(200, {
                        'Content-Type': 'application/octet-stream',
                        'Content-Length': String(stat.size),
                    });
                    fs.createReadStream(filePath).pipe(res);
                } else {
                    res.writeHead(404);
                    res.end('Not Found');
                }
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
        if (originalDistServer !== undefined) {
            process.env['INFS_DIST_SERVER'] = originalDistServer;
        } else {
            delete process.env['INFS_DIST_SERVER'];
        }
        if (originalInferenceHome !== undefined) {
            process.env['INFERENCE_HOME'] = originalInferenceHome;
        } else {
            delete process.env['INFERENCE_HOME'];
        }
        await new Promise<void>((resolve) => server.close(() => resolve()));
        fs.rmSync(tmpDir, { recursive: true });
    });

    it('network error during manifest fetch produces descriptive error (QA 9.5)', async () => {
        const home = path.join(tmpDir, 'net-error-home');
        fs.mkdirSync(home, { recursive: true });
        process.env['INFS_DIST_SERVER'] = `${baseUrl.replace('/releases.json', '')}/`;
        process.env['INFERENCE_HOME'] = home;

        // Point to the endpoint that destroys the socket
        process.env['INFS_DIST_SERVER'] = baseUrl;
        // Override manifest URL by setting INFS_DIST_SERVER to a server that will error
        const badUrl = baseUrl.replace(/:\d+/, ':1');
        process.env['INFS_DIST_SERVER'] = badUrl;

        await assert.rejects(
            installToolchain(linuxPlatform),
            (err: Error) => {
                assert.ok(err.message.length > 0, 'Error should have a message');
                return true;
            },
        );
    });

    it('SHA-256 mismatch throws "SHA-256 verification failed" (QA 9.6)', async () => {
        const home = path.join(tmpDir, 'sha-mismatch-home');
        fs.mkdirSync(home, { recursive: true });
        process.env['INFS_DIST_SERVER'] = baseUrl;
        process.env['INFERENCE_HOME'] = home;

        // Temporarily replace the manifest URL to one with a bad hash
        const originalServer = process.env['INFS_DIST_SERVER'];
        // We need to make installToolchain use the bad-hash manifest.
        // Since it uses INFS_DIST_SERVER + /releases.json, we'll set up a separate port
        // Instead, let's use a custom server for this test
        const badHashServer = http.createServer((req, res) => {
            if (req.url === '/releases.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-linux-x64.tar.gz`, sha256: 'deadbeef0000' },
                        ],
                    },
                ]));
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        const badHashUrl = await new Promise<string>((resolve) => {
            badHashServer.listen(0, '127.0.0.1', () => {
                const addr = badHashServer.address();
                if (addr && typeof addr === 'object') {
                    resolve(`http://127.0.0.1:${addr.port}`);
                }
            });
        });

        process.env['INFS_DIST_SERVER'] = badHashUrl;

        try {
            await assert.rejects(
                installToolchain(linuxPlatform),
                (err: Error) => {
                    assert.ok(
                        err.message.includes('SHA-256 verification failed'),
                        `Expected "SHA-256 verification failed", got: ${err.message}`,
                    );
                    return true;
                },
            );
        } finally {
            process.env['INFS_DIST_SERVER'] = originalServer;
            await new Promise<void>((resolve) => badHashServer.close(() => resolve()));
        }
    });

    it('archive without infs binary throws error about missing binary (QA 9.7)', async () => {
        const home = path.join(tmpDir, 'nobin-home');
        fs.mkdirSync(home, { recursive: true });
        process.env['INFERENCE_HOME'] = home;

        const noBinServer = http.createServer((req, res) => {
            if (req.url === '/releases.json') {
                const { sha256File: sha256 } = require('../utils/download');
                const archivePath = path.join(tmpDir, 'server', 'infs-linux-x64-nobin.tar.gz');
                sha256(archivePath).then((hash: string) => {
                    res.writeHead(200, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify([
                        {
                            version: '0.0.1-test',
                            stable: true,
                            files: [
                                { url: `${baseUrl}/infs-linux-x64-nobin.tar.gz`, sha256: hash },
                            ],
                        },
                    ]));
                });
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        const noBinUrl = await new Promise<string>((resolve) => {
            noBinServer.listen(0, '127.0.0.1', () => {
                const addr = noBinServer.address();
                if (addr && typeof addr === 'object') {
                    resolve(`http://127.0.0.1:${addr.port}`);
                }
            });
        });

        process.env['INFS_DIST_SERVER'] = noBinUrl;

        try {
            await assert.rejects(
                installToolchain(linuxPlatform),
                (err: Error) => {
                    assert.ok(
                        err.message.includes('not found') || err.message.includes('binary'),
                        `Expected error about missing binary, got: ${err.message}`,
                    );
                    return true;
                },
            );
        } finally {
            await new Promise<void>((resolve) => noBinServer.close(() => resolve()));
        }
    });

    it('infs install returning non-zero exit produces error with stderr (QA 9.8)', async () => {
        const home = path.join(tmpDir, 'fail-install-home');
        fs.mkdirSync(home, { recursive: true });
        process.env['INFERENCE_HOME'] = home;

        const failServer = http.createServer((req, res) => {
            if (req.url === '/releases.json') {
                const { sha256File: sha256 } = require('../utils/download');
                const archivePath = path.join(tmpDir, 'server', 'infs-linux-x64-failbin.tar.gz');
                sha256(archivePath).then((hash: string) => {
                    res.writeHead(200, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify([
                        {
                            version: '0.0.1-test',
                            stable: true,
                            files: [
                                { url: `${baseUrl}/infs-linux-x64-failbin.tar.gz`, sha256: hash },
                            ],
                        },
                    ]));
                });
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        const failUrl = await new Promise<string>((resolve) => {
            failServer.listen(0, '127.0.0.1', () => {
                const addr = failServer.address();
                if (addr && typeof addr === 'object') {
                    resolve(`http://127.0.0.1:${addr.port}`);
                }
            });
        });

        process.env['INFS_DIST_SERVER'] = failUrl;

        try {
            await assert.rejects(
                installToolchain(linuxPlatform),
                (err: Error) => {
                    assert.ok(
                        err.message.includes('infs install failed'),
                        `Expected "infs install failed" in error, got: ${err.message}`,
                    );
                    return true;
                },
            );
        } finally {
            await new Promise<void>((resolve) => failServer.close(() => resolve()));
        }
    });

    it('no matching platform in manifest throws "No compatible infs release" (QA 4.1.10)', async () => {
        const home = path.join(tmpDir, 'no-platform-home');
        fs.mkdirSync(home, { recursive: true });
        process.env['INFERENCE_HOME'] = home;

        const noPlatformServer = http.createServer((req, res) => {
            if (req.url === '/releases.json') {
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify([
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infs-macos-arm64.tar.gz`, sha256: 'abc' },
                        ],
                    },
                ]));
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        const noPlatformUrl = await new Promise<string>((resolve) => {
            noPlatformServer.listen(0, '127.0.0.1', () => {
                const addr = noPlatformServer.address();
                if (addr && typeof addr === 'object') {
                    resolve(`http://127.0.0.1:${addr.port}`);
                }
            });
        });

        process.env['INFS_DIST_SERVER'] = noPlatformUrl;

        try {
            await assert.rejects(
                installToolchain(linuxPlatform),
                (err: Error) => {
                    assert.ok(
                        err.message.includes('No compatible infs release'),
                        `Expected "No compatible infs release", got: ${err.message}`,
                    );
                    return true;
                },
            );
        } finally {
            await new Promise<void>((resolve) => noPlatformServer.close(() => resolve()));
        }
    });
});
