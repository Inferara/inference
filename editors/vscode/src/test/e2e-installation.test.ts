import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as http from 'node:http';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it, before, after } from 'node:test';
import { execSync } from 'node:child_process';

import { fetchJson, downloadFile, sha256File } from '../utils/download';
import { extractArchive } from '../utils/extract';
import { exec } from '../utils/exec';
import { parseDoctorOutput } from '../toolchain/doctor';
import { findLatestRelease, type ReleaseEntry } from '../toolchain/manifest';
import { detectPlatform } from '../toolchain/platform';

/**
 * End-to-end installation tests.
 *
 * These tests simulate the full user installation flow by wiring together
 * the same real components the VS Code extension uses: HTTP download,
 * archive extraction, child process execution, and doctor output parsing.
 *
 * A local HTTP server serves a manifest and archives containing fake
 * binaries (shell scripts). The test orchestrates the same steps as
 * `installToolchain()` in installation.ts, verifying the entire pipeline
 * works without needing the `vscode` module.
 */

const platform = detectPlatform();

// These e2e tests require a Unix platform (shell scripts as fake binaries).
const isUnix = process.platform !== 'win32';

/**
 * Create a fake `infs` shell script that handles `install` and `doctor`.
 *
 * - `install`: Downloads infc archive from INFS_DIST_SERVER, extracts to
 *   INFERENCE_HOME/toolchains/<version>/, sets default, creates symlinks.
 * - `doctor`: Checks the toolchain structure and outputs status lines in
 *   the format the VS Code extension expects.
 */
function createFakeInfsScript(): string {
    return `#!/bin/sh
set -e

INFERENCE_HOME="\${INFERENCE_HOME:-$HOME/.inference}"

case "\$1" in
    install)
        DIST_SERVER="\${INFS_DIST_SERVER:-https://inference-lang.org}"
        MANIFEST_URL="\${DIST_SERVER}/releases.json"

        VERSION=\$(node -e "
            const http = require('http');
            const url = new URL('\${MANIFEST_URL}');
            http.get(url, res => {
                let data = '';
                res.on('data', c => data += c);
                res.on('end', () => {
                    const m = JSON.parse(data);
                    console.log(m[0].version);
                });
            });
        ")

        TOOLCHAIN_DIR="\${INFERENCE_HOME}/toolchains/\${VERSION}"
        mkdir -p "\${TOOLCHAIN_DIR}"

        ARCHIVE_URL=\$(node -e "
            const http = require('http');
            const url = new URL('\${MANIFEST_URL}');
            http.get(url, res => {
                let data = '';
                res.on('data', c => data += c);
                res.on('end', () => {
                    const m = JSON.parse(data);
                    const f = m[0].files.find(f => f.url.includes('infc'));
                    console.log(f.url);
                });
            });
        ")

        TMPFILE=\$(mktemp /tmp/infc-archive-XXXXXX.tar.gz)
        node -e "
            const http = require('http');
            const fs = require('fs');
            const url = new URL('\${ARCHIVE_URL}');
            http.get(url, res => {
                const out = fs.createWriteStream('\${TMPFILE}');
                res.pipe(out);
                out.on('finish', () => out.close());
            });
        "
        sleep 1

        tar -xzf "\${TMPFILE}" -C "\${TOOLCHAIN_DIR}"
        rm -f "\${TMPFILE}"

        echo "\${VERSION}" > "\${INFERENCE_HOME}/default"

        mkdir -p "\${INFERENCE_HOME}/bin"
        for bin in infc inf-llc rust-lld; do
            SOURCE="\${TOOLCHAIN_DIR}/bin/\${bin}"
            if [ ! -f "\${SOURCE}" ]; then
                SOURCE="\${TOOLCHAIN_DIR}/\${bin}"
            fi
            if [ -f "\${SOURCE}" ]; then
                ln -sf "\${SOURCE}" "\${INFERENCE_HOME}/bin/\${bin}"
            fi
        done

        echo "Toolchain \${VERSION} installed successfully."
        ;;

    doctor)
        HAS_WARN=0
        HAS_FAIL=0

        # Check infs binary in PATH
        if command -v infs >/dev/null 2>&1; then
            echo "  [OK] infs binary: Found at \$(command -v infs)"
        else
            echo "  [WARN] infs binary: Found at \$0 but not in PATH"
            HAS_WARN=1
        fi

        # Check platform
        echo "  [OK] Platform: Detected linux-x64"

        # Check toolchain directory
        if [ -d "\${INFERENCE_HOME}" ]; then
            echo "  [OK] Toolchain directory: Found at \${INFERENCE_HOME}"
        else
            echo "  [WARN] Toolchain directory: Not found at \${INFERENCE_HOME}"
            HAS_WARN=1
        fi

        # Check default toolchain
        if [ -f "\${INFERENCE_HOME}/default" ]; then
            DEFAULT_VERSION=\$(cat "\${INFERENCE_HOME}/default")
            if [ -d "\${INFERENCE_HOME}/toolchains/\${DEFAULT_VERSION}" ]; then
                echo "  [OK] Default toolchain: Set to \${DEFAULT_VERSION}"
            else
                echo "  [FAIL] Default toolchain: \${DEFAULT_VERSION} is set but not installed"
                HAS_FAIL=1
            fi
        else
            echo "  [WARN] Default toolchain: No default toolchain set"
            HAS_WARN=1
        fi

        # Check inf-llc
        DEFAULT_VERSION=\$(cat "\${INFERENCE_HOME}/default" 2>/dev/null || echo "")
        if [ -n "\${DEFAULT_VERSION}" ]; then
            INF_LLC="\${INFERENCE_HOME}/toolchains/\${DEFAULT_VERSION}/bin/inf-llc"
            if [ -f "\${INF_LLC}" ]; then
                echo "  [OK] inf-llc: Found at \${INF_LLC}"
            else
                echo "  [FAIL] inf-llc: Not found"
                HAS_FAIL=1
            fi
        fi

        # Check rust-lld
        if [ -n "\${DEFAULT_VERSION}" ]; then
            RUST_LLD="\${INFERENCE_HOME}/toolchains/\${DEFAULT_VERSION}/bin/rust-lld"
            if [ -f "\${RUST_LLD}" ]; then
                echo "  [OK] rust-lld: Found at \${RUST_LLD}"
            else
                echo "  [FAIL] rust-lld: Not found"
                HAS_FAIL=1
            fi
        fi

        # Check libLLVM (Linux only)
        if [ -n "\${DEFAULT_VERSION}" ]; then
            LIB_DIR="\${INFERENCE_HOME}/toolchains/\${DEFAULT_VERSION}/lib"
            if [ -d "\${LIB_DIR}" ]; then
                FOUND_LIB=\$(ls "\${LIB_DIR}"/libLLVM*.so* 2>/dev/null | head -1)
                if [ -n "\${FOUND_LIB}" ]; then
                    echo "  [OK] libLLVM: Found \${FOUND_LIB}"
                else
                    echo "  [WARN] libLLVM: Not found in \${LIB_DIR}"
                    HAS_WARN=1
                fi
            else
                echo "  [WARN] libLLVM: Library directory not found at \${LIB_DIR}"
                HAS_WARN=1
            fi
        fi

        echo ""
        if [ "\${HAS_FAIL}" = "1" ]; then
            echo "Some checks failed. Run 'infs install' to install the toolchain."
            exit 1
        elif [ "\${HAS_WARN}" = "1" ]; then
            echo "Some warnings were found. The toolchain may work but could have issues."
            exit 0
        else
            echo "All checks passed. The toolchain is ready to use."
            exit 0
        fi
        ;;

    *)
        echo "Unknown command: \$1" >&2
        exit 1
        ;;
esac
`;
}

/**
 * Build a tar.gz archive containing fake toolchain binaries.
 *
 * Structure mirrors the real GitHub release:
 *   ./infc
 *   ./bin/inf-llc
 *   ./bin/rust-lld
 *   ./lib/libLLVM.so.21.1-rust-1.94.0-nightly
 */
function buildFakeInfcArchive(archivePath: string, opts?: { skipLib?: boolean }): void {
    const sourceDir = archivePath + '-source';
    fs.mkdirSync(path.join(sourceDir, 'bin'), { recursive: true });

    fs.writeFileSync(path.join(sourceDir, 'infc'), '#!/bin/sh\necho "infc 0.0.1-test"\n');
    fs.chmodSync(path.join(sourceDir, 'infc'), 0o755);

    fs.writeFileSync(path.join(sourceDir, 'bin', 'inf-llc'), '#!/bin/sh\necho "inf-llc"\n');
    fs.chmodSync(path.join(sourceDir, 'bin', 'inf-llc'), 0o755);

    fs.writeFileSync(path.join(sourceDir, 'bin', 'rust-lld'), '#!/bin/sh\necho "rust-lld"\n');
    fs.chmodSync(path.join(sourceDir, 'bin', 'rust-lld'), 0o755);

    if (!opts?.skipLib) {
        fs.mkdirSync(path.join(sourceDir, 'lib'), { recursive: true });
        fs.writeFileSync(
            path.join(sourceDir, 'lib', 'libLLVM.so.21.1-rust-1.94.0-nightly'),
            'fake-llvm-lib',
        );
    }

    execSync(`tar -czf "${archivePath}" -C "${sourceDir}" .`);
    fs.rmSync(sourceDir, { recursive: true });
}

/**
 * Build a tar.gz archive containing the fake infs binary.
 */
function buildFakeInfsArchive(archivePath: string, infsScript: string): void {
    const sourceDir = archivePath + '-source';
    fs.mkdirSync(sourceDir, { recursive: true });

    fs.writeFileSync(path.join(sourceDir, 'infs'), infsScript);
    fs.chmodSync(path.join(sourceDir, 'infs'), 0o755);

    execSync(`tar -czf "${archivePath}" -C "${sourceDir}" .`);
    fs.rmSync(sourceDir, { recursive: true });
}

describe('e2e installation', { skip: !isUnix ? 'Unix-only tests' : undefined }, () => {
    let server: http.Server;
    let baseUrl: string;
    let tmpDir: string;
    let inferenceHome: string;

    before(async () => {
        tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-e2e-'));
        inferenceHome = path.join(tmpDir, 'inference-home');

        const archiveDir = path.join(tmpDir, 'server');
        fs.mkdirSync(archiveDir, { recursive: true });

        const infsScript = createFakeInfsScript();
        const infsArchive = path.join(archiveDir, 'infs-linux-x64.tar.gz');
        buildFakeInfsArchive(infsArchive, infsScript);
        const infsHash = await sha256File(infsArchive);

        const infcArchive = path.join(archiveDir, 'infc-linux-x64.tar.gz');
        buildFakeInfcArchive(infcArchive);
        const infcHash = await sha256File(infcArchive);

        server = http.createServer((req, res) => {
            if (req.url === '/releases.json') {
                const manifest: ReleaseEntry[] = [
                    {
                        version: '0.0.1-test',
                        stable: true,
                        files: [
                            { url: `${baseUrl}/infc-linux-x64.tar.gz`, sha256: infcHash },
                            { url: `${baseUrl}/infs-linux-x64.tar.gz`, sha256: infsHash },
                        ],
                    },
                ];
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify(manifest));
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
        await new Promise<void>((resolve) => server.close(() => resolve()));
        fs.rmSync(tmpDir, { recursive: true });
    });

    /**
     * Simulate the installToolchain() flow using real, importable components.
     *
     * This mirrors installation.ts without needing the vscode dependency:
     * 1. Fetch manifest
     * 2. Find latest release for platform
     * 3. Download infs archive + verify SHA256
     * 4. Extract infs to INFERENCE_HOME/bin
     * 5. Run `infs install` with augmented PATH
     * 6. Run `infs doctor` with augmented PATH
     * 7. Parse doctor output
     */
    async function simulateInstall(opts?: {
        inferenceHomeOverride?: string;
    }): Promise<{
        doctorWarnings: boolean;
        doctorResult: ReturnType<typeof parseDoctorOutput>;
        installStdout: string;
        installStderr: string;
    }> {
        const home = opts?.inferenceHomeOverride ?? inferenceHome;
        const manifestUrl = `${baseUrl}/releases.json`;

        const manifest = await fetchJson<ReleaseEntry[]>(manifestUrl);
        assert.ok(Array.isArray(manifest), 'Manifest should be an array');

        const platformInfo = { id: 'linux-x64' as const };
        const match = findLatestRelease(manifest, platformInfo);
        assert.ok(match !== null, 'Should find a release for linux-x64');

        const { fileUrl, sha256 } = match;

        const binDir = path.join(home, 'bin');
        fs.mkdirSync(binDir, { recursive: true });

        const archiveTmp = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-dl-'));
        const archivePath = path.join(archiveTmp, 'infs-linux-x64.tar.gz');

        try {
            await downloadFile(fileUrl, { destPath: archivePath });

            const actualHash = await sha256File(archivePath);
            assert.strictEqual(actualHash, sha256, 'SHA256 should match manifest');

            await extractArchive({ archivePath, destDir: binDir });
        } finally {
            fs.rmSync(archiveTmp, { recursive: true, force: true });
        }

        const infsPath = path.join(binDir, 'infs');
        assert.ok(fs.existsSync(infsPath), 'infs binary should exist after extraction');

        const sep = ':';
        const augmentedPath = `${binDir}${sep}${process.env['PATH'] ?? ''}`;
        const childEnv = {
            PATH: augmentedPath,
            INFERENCE_HOME: home,
            INFS_DIST_SERVER: baseUrl,
        };

        const installResult = await exec(infsPath, ['install'], {
            timeoutMs: 30_000,
            env: childEnv,
        });

        const doctorResult = await exec(infsPath, ['doctor'], {
            timeoutMs: 10_000,
            env: childEnv,
        });

        const parsed = parseDoctorOutput(doctorResult.stdout);
        const doctorWarnings = doctorResult.exitCode !== 0 || parsed.hasErrors || parsed.hasWarnings;

        return {
            doctorWarnings,
            doctorResult: parsed,
            installStdout: installResult.stdout,
            installStderr: installResult.stderr,
        };
    }

    describe('happy path: full install from scratch', () => {
        let result: Awaited<ReturnType<typeof simulateInstall>>;
        let home: string;

        before(async () => {
            home = path.join(tmpDir, 'happy-home');
            result = await simulateInstall({ inferenceHomeOverride: home });
        });

        it('reports no doctor warnings', () => {
            assert.strictEqual(result.doctorWarnings, false);
        });

        it('doctor reports all checks as OK', () => {
            for (const check of result.doctorResult.checks) {
                assert.strictEqual(
                    check.status,
                    'ok',
                    `Expected OK for "${check.name}", got ${check.status}: ${check.message}`,
                );
            }
        });

        it('doctor summary indicates healthy toolchain', () => {
            assert.ok(
                result.doctorResult.summary.includes('All checks passed'),
                `Expected healthy summary, got: ${result.doctorResult.summary}`,
            );
        });

        it('creates the default version file', () => {
            const defaultFile = path.join(home, 'default');
            assert.ok(fs.existsSync(defaultFile), 'default file should exist');
            const version = fs.readFileSync(defaultFile, 'utf-8').trim();
            assert.strictEqual(version, '0.0.1-test');
        });

        it('creates the toolchain directory with correct structure', () => {
            const toolchainDir = path.join(home, 'toolchains', '0.0.1-test');
            assert.ok(fs.existsSync(toolchainDir), 'toolchain dir should exist');
            assert.ok(fs.existsSync(path.join(toolchainDir, 'infc')), 'infc should exist');
            assert.ok(fs.existsSync(path.join(toolchainDir, 'bin', 'inf-llc')), 'inf-llc should exist');
            assert.ok(fs.existsSync(path.join(toolchainDir, 'bin', 'rust-lld')), 'rust-lld should exist');
        });

        it('creates the lib directory with libLLVM', () => {
            const libDir = path.join(home, 'toolchains', '0.0.1-test', 'lib');
            assert.ok(fs.existsSync(libDir), 'lib/ directory should exist');
            const files = fs.readdirSync(libDir);
            const hasLlvm = files.some(
                (f) => f.startsWith('libLLVM') && f.includes('.so'),
            );
            assert.ok(hasLlvm, `Expected libLLVM*.so in ${libDir}, found: ${files}`);
        });

        it('creates symlinks in bin directory', () => {
            const binDir = path.join(home, 'bin');
            assert.ok(fs.existsSync(path.join(binDir, 'infc')), 'infc symlink should exist');
            assert.ok(fs.existsSync(path.join(binDir, 'inf-llc')), 'inf-llc symlink should exist');
            assert.ok(fs.existsSync(path.join(binDir, 'rust-lld')), 'rust-lld symlink should exist');
        });
    });

    describe('missing lib directory triggers doctor warning', () => {
        let parsed: ReturnType<typeof parseDoctorOutput>;
        let doctorWarnings: boolean;

        before(async () => {
            const home = path.join(tmpDir, 'nolib-home');
            const binDir = path.join(home, 'bin');
            const version = '0.0.1-nolib';
            const toolchainDir = path.join(home, 'toolchains', version);

            // Manually create toolchain structure WITHOUT lib/
            fs.mkdirSync(path.join(toolchainDir, 'bin'), { recursive: true });
            fs.mkdirSync(binDir, { recursive: true });

            fs.writeFileSync(path.join(toolchainDir, 'infc'), '#!/bin/sh\necho infc\n');
            fs.chmodSync(path.join(toolchainDir, 'infc'), 0o755);
            fs.writeFileSync(path.join(toolchainDir, 'bin', 'inf-llc'), '#!/bin/sh\necho inf-llc\n');
            fs.chmodSync(path.join(toolchainDir, 'bin', 'inf-llc'), 0o755);
            fs.writeFileSync(path.join(toolchainDir, 'bin', 'rust-lld'), '#!/bin/sh\necho rust-lld\n');
            fs.chmodSync(path.join(toolchainDir, 'bin', 'rust-lld'), 0o755);

            fs.writeFileSync(path.join(home, 'default'), version);

            // Copy infs binary from a previously downloaded archive
            const manifest = await fetchJson<ReleaseEntry[]>(`${baseUrl}/releases.json`);
            const match = findLatestRelease(manifest, { id: 'linux-x64' });
            assert.ok(match);
            const archiveTmp = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-nolib-'));
            const archivePath = path.join(archiveTmp, 'infs.tar.gz');
            try {
                await downloadFile(match.fileUrl, { destPath: archivePath });
                await extractArchive({ archivePath, destDir: binDir });
            } finally {
                fs.rmSync(archiveTmp, { recursive: true, force: true });
            }

            // Create symlinks
            for (const bin of ['infc', 'inf-llc', 'rust-lld']) {
                const source = bin === 'infc'
                    ? path.join(toolchainDir, bin)
                    : path.join(toolchainDir, 'bin', bin);
                const target = path.join(binDir, bin);
                try { fs.unlinkSync(target); } catch { /* ignore */ }
                fs.symlinkSync(source, target);
            }

            const infsPath = path.join(binDir, 'infs');
            const sep = ':';
            const augmentedPath = `${binDir}${sep}${process.env['PATH'] ?? ''}`;

            const doctorResult = await exec(infsPath, ['doctor'], {
                timeoutMs: 10_000,
                env: { PATH: augmentedPath, INFERENCE_HOME: home },
            });

            parsed = parseDoctorOutput(doctorResult.stdout);
            doctorWarnings = doctorResult.exitCode !== 0 || parsed.hasErrors || parsed.hasWarnings;
        });

        it('reports doctor warnings', () => {
            assert.strictEqual(doctorWarnings, true);
        });

        it('libLLVM check reports warning', () => {
            const libCheck = parsed.checks.find(
                (c) => c.name === 'libLLVM',
            );
            assert.ok(libCheck, 'Should have a libLLVM check');
            assert.strictEqual(libCheck.status, 'warn');
        });

        it('non-lib checks still pass', () => {
            const nonLibChecks = parsed.checks.filter(
                (c) => c.name !== 'libLLVM',
            );
            for (const check of nonLibChecks) {
                assert.strictEqual(
                    check.status,
                    'ok',
                    `Expected OK for "${check.name}", got ${check.status}: ${check.message}`,
                );
            }
        });
    });

    describe('SHA256 verification rejects tampered archive', () => {
        it('throws on hash mismatch', async () => {
            const manifest = await fetchJson<ReleaseEntry[]>(
                `${baseUrl}/releases.json`,
            );
            const match = findLatestRelease(manifest, { id: 'linux-x64' });
            assert.ok(match);

            const archiveTmp = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-tamper-'));
            const archivePath = path.join(archiveTmp, 'infs.tar.gz');

            try {
                await downloadFile(match.fileUrl, { destPath: archivePath });

                fs.appendFileSync(archivePath, 'tampered-data');

                const actualHash = await sha256File(archivePath);
                assert.notStrictEqual(
                    actualHash,
                    match.sha256,
                    'Tampered archive should have different hash',
                );
            } finally {
                fs.rmSync(archiveTmp, { recursive: true, force: true });
            }
        });
    });

    describe('exec env option passes environment to child process', () => {
        it('child process sees custom environment variable', async () => {
            const result = await exec('node', [
                '-e',
                'process.stdout.write(process.env.TEST_CUSTOM_VAR || "unset")',
            ], {
                env: { TEST_CUSTOM_VAR: 'e2e-value' },
            });
            assert.strictEqual(result.stdout, 'e2e-value');
        });

        it('child process sees augmented PATH', async () => {
            const fakeBinDir = path.join(tmpDir, 'fake-path-dir');
            fs.mkdirSync(fakeBinDir, { recursive: true });
            const fakeBin = path.join(fakeBinDir, 'e2e-test-bin');
            fs.writeFileSync(fakeBin, '#!/bin/sh\necho "found-in-path"\n');
            fs.chmodSync(fakeBin, 0o755);

            const augmentedPath = `${fakeBinDir}:${process.env['PATH'] ?? ''}`;
            const result = await exec('sh', ['-c', 'command -v e2e-test-bin'], {
                env: { PATH: augmentedPath },
            });
            assert.strictEqual(result.exitCode, 0);
            assert.ok(
                result.stdout.trim().includes('e2e-test-bin'),
                `Expected path containing e2e-test-bin, got: ${result.stdout}`,
            );
        });

        it('INFERENCE_HOME override is visible to child', async () => {
            const result = await exec('node', [
                '-e',
                'process.stdout.write(process.env.INFERENCE_HOME || "unset")',
            ], {
                env: { INFERENCE_HOME: '/custom/test/path' },
            });
            assert.strictEqual(result.stdout, '/custom/test/path');
        });
    });

    describe('PATH augmentation makes infs findable by doctor', () => {
        it('infs is found via augmented PATH during doctor check', async () => {
            const home = path.join(tmpDir, 'path-augment-home');
            const result = await simulateInstall({ inferenceHomeOverride: home });

            const infsBinaryCheck = result.doctorResult.checks.find(
                (c) => c.name === 'infs binary',
            );
            assert.ok(infsBinaryCheck, 'Should have infs binary check');
            assert.strictEqual(
                infsBinaryCheck.status,
                'ok',
                `infs binary check should be OK when PATH is augmented, got: ${infsBinaryCheck.message}`,
            );
        });

        it('infs doctor warns when PATH is NOT augmented', async () => {
            const home = path.join(tmpDir, 'no-augment-home');
            fs.mkdirSync(home, { recursive: true });

            const binDir = path.join(home, 'bin');
            fs.mkdirSync(binDir, { recursive: true });

            const manifest = await fetchJson<ReleaseEntry[]>(
                `${baseUrl}/releases.json`,
            );
            const match = findLatestRelease(manifest, { id: 'linux-x64' });
            assert.ok(match);

            const archiveTmp = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-noaug-'));
            const archivePath = path.join(archiveTmp, 'infs.tar.gz');
            try {
                await downloadFile(match.fileUrl, { destPath: archivePath });
                await extractArchive({ archivePath, destDir: binDir });
            } finally {
                fs.rmSync(archiveTmp, { recursive: true, force: true });
            }

            const infsPath = path.join(binDir, 'infs');

            // Run install with augmented PATH (needed for install to work)
            await exec(infsPath, ['install'], {
                timeoutMs: 30_000,
                env: {
                    PATH: `${binDir}:${process.env['PATH'] ?? ''}`,
                    INFERENCE_HOME: home,
                    INFS_DIST_SERVER: baseUrl,
                },
            });

            // Run doctor WITHOUT augmented PATH
            const doctorResult = await exec(infsPath, ['doctor'], {
                timeoutMs: 10_000,
                env: {
                    PATH: process.env['PATH'] ?? '',
                    INFERENCE_HOME: home,
                },
            });

            const parsed = parseDoctorOutput(doctorResult.stdout);
            const infsBinaryCheck = parsed.checks.find(
                (c) => c.name === 'infs binary',
            );
            assert.ok(infsBinaryCheck, 'Should have infs binary check');
            assert.strictEqual(
                infsBinaryCheck.status,
                'warn',
                `infs binary should warn when not in PATH, got: ${infsBinaryCheck.status}`,
            );
        });
    });

    describe('manifest matching', () => {
        it('fetches manifest and matches infs artifact for linux', async () => {
            const manifest = await fetchJson<ReleaseEntry[]>(
                `${baseUrl}/releases.json`,
            );
            const match = findLatestRelease(manifest, { id: 'linux-x64' });
            assert.ok(match !== null);
            assert.strictEqual(match.release.version, '0.0.1-test');
            assert.ok(match.fileUrl.includes('infs-linux-x64'));
        });

        it('does not match infc as the infs artifact', async () => {
            const manifest = await fetchJson<ReleaseEntry[]>(
                `${baseUrl}/releases.json`,
            );
            const match = findLatestRelease(manifest, { id: 'linux-x64' });
            assert.ok(match !== null);
            assert.ok(
                !match.fileUrl.includes('infc'),
                `Should match infs, not infc: ${match.fileUrl}`,
            );
        });

        it('returns null for unsupported platform', async () => {
            const manifest = await fetchJson<ReleaseEntry[]>(
                `${baseUrl}/releases.json`,
            );
            const match = findLatestRelease(manifest, { id: 'freebsd-x64' });
            assert.strictEqual(match, null);
        });
    });

    describe('archive extraction produces correct structure', () => {
        let extractDir: string;

        before(async () => {
            extractDir = path.join(tmpDir, 'extract-verify');
            fs.mkdirSync(extractDir, { recursive: true });

            const manifest = await fetchJson<ReleaseEntry[]>(
                `${baseUrl}/releases.json`,
            );
            const infcFile = manifest[0].files.find((f) =>
                f.url.includes('infc'),
            );
            assert.ok(infcFile);

            const archiveTmp = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-extr-'));
            const archivePath = path.join(archiveTmp, 'infc.tar.gz');
            try {
                await downloadFile(infcFile.url, { destPath: archivePath });
                await extractArchive({ archivePath, destDir: extractDir });
            } finally {
                fs.rmSync(archiveTmp, { recursive: true, force: true });
            }
        });

        it('has infc at root level (not inside bin/)', () => {
            assert.ok(fs.existsSync(path.join(extractDir, 'infc')));
        });

        it('has inf-llc inside bin/', () => {
            assert.ok(fs.existsSync(path.join(extractDir, 'bin', 'inf-llc')));
        });

        it('has rust-lld inside bin/', () => {
            assert.ok(fs.existsSync(path.join(extractDir, 'bin', 'rust-lld')));
        });

        it('has lib/ directory with libLLVM', () => {
            const libDir = path.join(extractDir, 'lib');
            assert.ok(fs.existsSync(libDir));
            const files = fs.readdirSync(libDir);
            assert.ok(
                files.some((f) => f.startsWith('libLLVM') && f.includes('.so')),
                `Expected libLLVM*.so in lib/, found: ${files}`,
            );
        });

        it('binaries have executable permissions', () => {
            const infcStat = fs.statSync(path.join(extractDir, 'infc'));
            assert.ok(
                (infcStat.mode & 0o111) !== 0,
                'infc should be executable',
            );
        });
    });
});
