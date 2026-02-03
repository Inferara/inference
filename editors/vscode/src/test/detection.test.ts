import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { describe, it, before, after } from 'node:test';

// We test the detection helpers directly rather than importing from detection.ts,
// because detection.ts transitively depends on vscode (via settings.ts).
// The helpers below are the same logic extracted for testability.

function inferenceHome(): string {
    return process.env['INFERENCE_HOME'] || path.join(os.homedir(), '.inference');
}

function isExecutable(filePath: string): boolean {
    try {
        fs.accessSync(filePath, fs.constants.X_OK);
        return true;
    } catch {
        return false;
    }
}

function findInPath(binaryName: string): string | null {
    const envPath = process.env['PATH'] || '';
    const sep = process.platform === 'win32' ? ';' : ':';
    const dirs = envPath.split(sep).filter(Boolean);
    for (const dir of dirs) {
        const candidate = path.join(dir, binaryName);
        if (isExecutable(candidate)) {
            return candidate;
        }
    }
    return null;
}

describe('detection helpers', () => {
    describe('inferenceHome', () => {
        const originalEnv = process.env['INFERENCE_HOME'];

        after(() => {
            if (originalEnv === undefined) {
                delete process.env['INFERENCE_HOME'];
            } else {
                process.env['INFERENCE_HOME'] = originalEnv;
            }
        });

        it('returns INFERENCE_HOME when set', () => {
            process.env['INFERENCE_HOME'] = '/custom/inference';
            assert.strictEqual(inferenceHome(), '/custom/inference');
        });

        it('returns ~/.inference when INFERENCE_HOME is not set', () => {
            delete process.env['INFERENCE_HOME'];
            assert.strictEqual(
                inferenceHome(),
                path.join(os.homedir(), '.inference'),
            );
        });
    });

    describe('isExecutable', () => {
        let tmpDir: string;
        let execFile: string;
        let nonExecFile: string;

        before(() => {
            tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-test-'));
            execFile = path.join(tmpDir, 'test-bin');
            fs.writeFileSync(execFile, '#!/bin/sh\n');
            fs.chmodSync(execFile, 0o755);

            nonExecFile = path.join(tmpDir, 'test-noexec');
            fs.writeFileSync(nonExecFile, 'not executable\n');
            fs.chmodSync(nonExecFile, 0o644);
        });

        after(() => {
            fs.rmSync(tmpDir, { recursive: true });
        });

        it('returns true for an executable file', () => {
            assert.strictEqual(isExecutable(execFile), true);
        });

        it('returns false for a nonexistent file', () => {
            assert.strictEqual(isExecutable('/nonexistent/path/xyz'), false);
        });

        it('returns false for a non-executable file on Unix', () => {
            if (process.platform === 'win32') {
                return;
            }
            assert.strictEqual(isExecutable(nonExecFile), false);
        });

        it('returns true for a directory (X_OK is set on dirs)', () => {
            if (process.platform === 'win32') {
                return;
            }
            assert.strictEqual(isExecutable(tmpDir), true);
        });
    });

    describe('findInPath', () => {
        const originalPath = process.env['PATH'];
        let tmpDir: string;

        before(() => {
            tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-path-'));
            const binPath = path.join(tmpDir, 'test-infs-bin');
            fs.writeFileSync(binPath, '#!/bin/sh\n');
            fs.chmodSync(binPath, 0o755);
        });

        after(() => {
            process.env['PATH'] = originalPath;
            fs.rmSync(tmpDir, { recursive: true });
        });

        it('finds a binary in PATH', () => {
            process.env['PATH'] = tmpDir + ':' + (originalPath || '');
            const result = findInPath('test-infs-bin');
            assert.strictEqual(result, path.join(tmpDir, 'test-infs-bin'));
        });

        it('returns null for a binary not in PATH', () => {
            process.env['PATH'] = originalPath;
            const result = findInPath('nonexistent-binary-xyz-456');
            assert.strictEqual(result, null);
        });

        it('returns null when PATH is empty', () => {
            process.env['PATH'] = '';
            const result = findInPath('test-infs-bin');
            assert.strictEqual(result, null);
        });

        it('handles PATH with multiple directories', () => {
            const otherDir = fs.mkdtempSync(path.join(os.tmpdir(), 'infs-path2-'));
            process.env['PATH'] = otherDir + ':' + tmpDir + ':' + (originalPath || '');
            const result = findInPath('test-infs-bin');
            assert.strictEqual(result, path.join(tmpDir, 'test-infs-bin'));
            fs.rmSync(otherDir, { recursive: true });
        });
    });
});
