import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { after, before, describe, it } from 'node:test';
import {
    lspActionForConfigChange,
    lspBinaryName,
    resolveLspBinary,
    ResolveLspBinaryOptions,
} from '../lsp/resolve';

/**
 * Tests for the pure inference-lsp binary resolution module.
 * resolve.ts has no vscode dependency, so it is imported directly.
 *
 * Two styles are used:
 * - Injected fake `isExecutable` callbacks for precedence / separator /
 *   platform-name matrices (fully deterministic, no filesystem).
 * - Real temp directories with an fs-based probe mirroring
 *   `detection.isExecutable` semantics (F_OK on win32, X_OK elsewhere).
 */

const isWin = process.platform === 'win32';

/** Same probe semantics as src/toolchain/detection.ts isExecutable. */
function fsIsExecutable(filePath: string): boolean {
    try {
        const mode = isWin ? fs.constants.F_OK : fs.constants.X_OK;
        fs.accessSync(filePath, mode);
        return true;
    } catch {
        return false;
    }
}

/** Build options with sane defaults for the injected-callback tests. */
function opts(
    overrides: Partial<ResolveLspBinaryOptions>,
): ResolveLspBinaryOptions {
    return {
        configuredPath: '',
        inferenceHome: path.join(path.sep, 'home', 'u', '.inference'),
        isWindows: false,
        envPath: '',
        isExecutable: () => false,
        ...overrides,
    };
}

describe('lspBinaryName', () => {
    it('is inference-lsp on posix', () => {
        assert.strictEqual(lspBinaryName(false), 'inference-lsp');
    });

    it('is inference-lsp.exe on windows', () => {
        assert.strictEqual(lspBinaryName(true), 'inference-lsp.exe');
    });
});

describe('resolveLspBinary (injected callbacks)', () => {
    const home = path.join(path.sep, 'home', 'u', '.inference');
    const managedPosix = path.join(home, 'bin', 'inference-lsp');
    const managedWindows = path.join(home, 'bin', 'inference-lsp.exe');

    describe('settings path', () => {
        it('returns the configured path when executable', () => {
            const configured = path.join(path.sep, 'opt', 'inference-lsp');
            const result = resolveLspBinary(
                opts({
                    configuredPath: configured,
                    isExecutable: (p) => p === configured,
                }),
            );
            assert.deepStrictEqual(result, {
                path: configured,
                source: 'settings',
            });
        });

        it('returns null when set but not executable, WITHOUT fallback to managed or PATH', () => {
            const dir = path.join(path.sep, 'usr', 'bin');
            const result = resolveLspBinary(
                opts({
                    configuredPath: path.join(path.sep, 'bad', 'inference-lsp'),
                    envPath: dir,
                    // Everything except the configured path is executable.
                    isExecutable: (p) =>
                        p !== path.join(path.sep, 'bad', 'inference-lsp'),
                }),
            );
            assert.strictEqual(result, null);
        });

        it('is used verbatim: no .exe suffix appended on windows', () => {
            const configured = 'C:\\tools\\my-lsp';
            const probed: string[] = [];
            resolveLspBinary(
                opts({
                    configuredPath: configured,
                    isWindows: true,
                    isExecutable: (p) => {
                        probed.push(p);
                        return true;
                    },
                }),
            );
            assert.deepStrictEqual(probed, [configured]);
        });
    });

    describe('managed location', () => {
        it('finds INFERENCE_HOME/bin/inference-lsp on posix', () => {
            const result = resolveLspBinary(
                opts({
                    inferenceHome: home,
                    isExecutable: (p) => p === managedPosix,
                }),
            );
            assert.deepStrictEqual(result, {
                path: managedPosix,
                source: 'managed',
            });
        });

        it('finds INFERENCE_HOME/bin/inference-lsp.exe on windows', () => {
            const result = resolveLspBinary(
                opts({
                    inferenceHome: home,
                    isWindows: true,
                    isExecutable: (p) => p === managedWindows,
                }),
            );
            assert.deepStrictEqual(result, {
                path: managedWindows,
                source: 'managed',
            });
        });
    });

    describe('PATH lookup', () => {
        it('finds the binary in a PATH directory', () => {
            const dir = path.join(path.sep, 'usr', 'local', 'bin');
            const candidate = path.join(dir, 'inference-lsp');
            const result = resolveLspBinary(
                opts({
                    envPath: dir,
                    isExecutable: (p) => p === candidate,
                }),
            );
            assert.deepStrictEqual(result, { path: candidate, source: 'path' });
        });

        it('returns the FIRST matching PATH directory', () => {
            const first = path.join(path.sep, 'a');
            const second = path.join(path.sep, 'b');
            const candidates = [
                path.join(first, 'inference-lsp'),
                path.join(second, 'inference-lsp'),
            ];
            const result = resolveLspBinary(
                opts({
                    envPath: [first, second].join(':'),
                    // Both PATH directories contain an executable candidate.
                    isExecutable: (p) => candidates.includes(p),
                }),
            );
            assert.deepStrictEqual(result, {
                path: candidates[0],
                source: 'path',
            });
        });

        it('splits PATH on ":" on posix', () => {
            const dir = path.join(path.sep, 'x');
            const candidate = path.join(dir, 'inference-lsp');
            const result = resolveLspBinary(
                opts({
                    envPath: `${path.join(path.sep, 'nope')}:${dir}`,
                    isExecutable: (p) => p === candidate,
                }),
            );
            assert.deepStrictEqual(result, { path: candidate, source: 'path' });
        });

        it('splits PATH on ";" on windows', () => {
            const dir = 'C:\\tools';
            const candidate = path.join(dir, 'inference-lsp.exe');
            const result = resolveLspBinary(
                opts({
                    isWindows: true,
                    envPath: `C:\\nope;${dir}`,
                    isExecutable: (p) => p === candidate,
                }),
            );
            assert.deepStrictEqual(result, { path: candidate, source: 'path' });
        });

        it('does NOT split on ";" on posix (semicolon is part of the dir name)', () => {
            const probed: string[] = [];
            resolveLspBinary(
                opts({
                    envPath: '/a;/b',
                    isExecutable: (p) => {
                        probed.push(p);
                        return false;
                    },
                }),
            );
            // Managed probe + one single PATH entry '/a;/b'.
            assert.strictEqual(probed.length, 2);
            assert.strictEqual(probed[1], path.join('/a;/b', 'inference-lsp'));
        });

        it('skips empty PATH segments', () => {
            const dir = path.join(path.sep, 'real');
            const candidate = path.join(dir, 'inference-lsp');
            const result = resolveLspBinary(
                opts({
                    envPath: `::${dir}::`,
                    isExecutable: (p) => p === candidate,
                }),
            );
            assert.deepStrictEqual(result, { path: candidate, source: 'path' });
        });

        it('returns null when PATH is empty', () => {
            const result = resolveLspBinary(
                opts({ envPath: '', isExecutable: () => false }),
            );
            assert.strictEqual(result, null);
        });
    });

    describe('precedence', () => {
        it('settings wins over managed and PATH', () => {
            const configured = path.join(path.sep, 'custom', 'lsp');
            const result = resolveLspBinary(
                opts({
                    configuredPath: configured,
                    inferenceHome: home,
                    envPath: path.join(path.sep, 'usr', 'bin'),
                    isExecutable: () => true,
                }),
            );
            assert.deepStrictEqual(result, {
                path: configured,
                source: 'settings',
            });
        });

        it('managed wins over PATH', () => {
            const result = resolveLspBinary(
                opts({
                    inferenceHome: home,
                    envPath: path.join(path.sep, 'usr', 'bin'),
                    isExecutable: () => true,
                }),
            );
            assert.deepStrictEqual(result, {
                path: managedPosix,
                source: 'managed',
            });
        });

        it('returns null when nothing matches anywhere', () => {
            const result = resolveLspBinary(
                opts({
                    inferenceHome: home,
                    envPath: [
                        path.join(path.sep, 'a'),
                        path.join(path.sep, 'b'),
                    ].join(':'),
                    isExecutable: () => false,
                }),
            );
            assert.strictEqual(result, null);
        });
    });
});

describe('resolveLspBinary (real filesystem)', () => {
    let homeDir: string;
    let pathDir: string;
    let managedBinary: string;
    let pathBinary: string;
    let nonExecFile: string;

    const binaryName = lspBinaryName(isWin);

    before(() => {
        homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'lsp-home-'));
        fs.mkdirSync(path.join(homeDir, 'bin'));
        managedBinary = path.join(homeDir, 'bin', binaryName);

        pathDir = fs.mkdtempSync(path.join(os.tmpdir(), 'lsp-path-'));
        pathBinary = path.join(pathDir, binaryName);

        nonExecFile = path.join(homeDir, 'not-executable');
        fs.writeFileSync(nonExecFile, 'plain file\n');
        fs.chmodSync(nonExecFile, 0o644);
    });

    after(() => {
        fs.rmSync(homeDir, { recursive: true });
        fs.rmSync(pathDir, { recursive: true });
    });

    function fsOpts(
        overrides: Partial<ResolveLspBinaryOptions>,
    ): ResolveLspBinaryOptions {
        return {
            configuredPath: '',
            inferenceHome: homeDir,
            isWindows: isWin,
            envPath: '',
            isExecutable: fsIsExecutable,
            ...overrides,
        };
    }

    function createExecutable(filePath: string): void {
        fs.writeFileSync(filePath, '#!/bin/sh\n');
        fs.chmodSync(filePath, 0o755);
    }

    it('finds a real managed binary', () => {
        createExecutable(managedBinary);
        try {
            const result = resolveLspBinary(fsOpts({}));
            assert.deepStrictEqual(result, {
                path: managedBinary,
                source: 'managed',
            });
        } finally {
            fs.rmSync(managedBinary);
        }
    });

    it('finds a real binary via PATH when managed location is empty', () => {
        createExecutable(pathBinary);
        try {
            const sep = isWin ? ';' : ':';
            const result = resolveLspBinary(
                fsOpts({ envPath: [os.tmpdir(), pathDir].join(sep) }),
            );
            assert.deepStrictEqual(result, {
                path: pathBinary,
                source: 'path',
            });
        } finally {
            fs.rmSync(pathBinary);
        }
    });

    it('resolves a real executable via configured settings path', () => {
        createExecutable(managedBinary);
        try {
            const result = resolveLspBinary(
                fsOpts({ configuredPath: managedBinary }),
            );
            assert.deepStrictEqual(result, {
                path: managedBinary,
                source: 'settings',
            });
        } finally {
            fs.rmSync(managedBinary);
        }
    });

    it('returns null for a configured path pointing at a missing file, even when managed exists', () => {
        createExecutable(managedBinary);
        try {
            const result = resolveLspBinary(
                fsOpts({
                    configuredPath: path.join(homeDir, 'does-not-exist'),
                }),
            );
            assert.strictEqual(result, null);
        } finally {
            fs.rmSync(managedBinary);
        }
    });

    it('returns null for a configured non-executable file on unix, even when managed exists', () => {
        if (isWin) {
            // On windows the probe is F_OK: existence implies usable.
            return;
        }
        createExecutable(managedBinary);
        try {
            const result = resolveLspBinary(
                fsOpts({ configuredPath: nonExecFile }),
            );
            assert.strictEqual(result, null);
        } finally {
            fs.rmSync(managedBinary);
        }
    });

    it('ignores a non-executable managed binary on unix and falls through to PATH', () => {
        if (isWin) {
            return;
        }
        fs.writeFileSync(managedBinary, 'not executable\n');
        fs.chmodSync(managedBinary, 0o644);
        createExecutable(pathBinary);
        try {
            const result = resolveLspBinary(fsOpts({ envPath: pathDir }));
            assert.deepStrictEqual(result, {
                path: pathBinary,
                source: 'path',
            });
        } finally {
            fs.rmSync(managedBinary);
            fs.rmSync(pathBinary);
        }
    });

    it('returns null when no binary exists anywhere', () => {
        const result = resolveLspBinary(fsOpts({ envPath: pathDir }));
        assert.strictEqual(result, null);
    });
});

describe('lspActionForConfigChange', () => {
    it('ignores changes outside inference.lsp.*', () => {
        for (const enabled of [true, false]) {
            for (const running of [true, false]) {
                assert.strictEqual(
                    lspActionForConfigChange({
                        affectsLsp: false,
                        enabled,
                        running,
                    }),
                    'none',
                );
            }
        }
    });

    it('stops a running client when disabled', () => {
        assert.strictEqual(
            lspActionForConfigChange({
                affectsLsp: true,
                enabled: false,
                running: true,
            }),
            'stop',
        );
    });

    it('does nothing when disabled and already stopped', () => {
        assert.strictEqual(
            lspActionForConfigChange({
                affectsLsp: true,
                enabled: false,
                running: false,
            }),
            'none',
        );
    });

    it('restarts a running client on lsp config change', () => {
        assert.strictEqual(
            lspActionForConfigChange({
                affectsLsp: true,
                enabled: true,
                running: true,
            }),
            'restart',
        );
    });

    it('restarts (i.e., starts) a stopped client when enabled', () => {
        assert.strictEqual(
            lspActionForConfigChange({
                affectsLsp: true,
                enabled: true,
                running: false,
            }),
            'restart',
        );
    });
});
