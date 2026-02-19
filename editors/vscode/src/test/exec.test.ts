import * as assert from 'node:assert';
import { describe, it } from 'node:test';

// exec.ts uses child_process.spawn; we test it by running real simple commands.
// This avoids mocking complexity while still verifying the contract.

import { exec } from '../utils/exec';

describe('exec', () => {
    it('captures stdout from a successful command', async () => {
        const result = await exec('echo', ['hello']);
        assert.strictEqual(result.exitCode, 0);
        assert.strictEqual(result.stdout.trim(), 'hello');
        assert.strictEqual(result.stderr, '');
    });

    it('captures exit code from a failing command', async () => {
        const result = await exec('node', ['-e', 'process.exit(42)']);
        assert.strictEqual(result.exitCode, 42);
    });

    it('captures stderr', async () => {
        const result = await exec('node', [
            '-e',
            'process.stderr.write("err"); process.exit(1)',
        ]);
        assert.strictEqual(result.exitCode, 1);
        assert.strictEqual(result.stderr, 'err');
    });

    it('rejects on spawn failure for nonexistent binary', async () => {
        await assert.rejects(
            exec('nonexistent_binary_xyz_123', []),
            (err: NodeJS.ErrnoException) => {
                assert.strictEqual(err.code, 'ENOENT');
                return true;
            },
        );
    });

    it('respects custom timeout', async () => {
        const start = Date.now();
        const result = await exec('node', ['-e', 'setTimeout(() => {}, 10000)'], {
            timeoutMs: 500,
        });
        const elapsed = Date.now() - start;
        assert.ok(elapsed < 5000, `Expected fast timeout, took ${elapsed}ms`);
        assert.notStrictEqual(result.exitCode, 0);
    });

    it('respects cwd option', async () => {
        const result = await exec('pwd', [], { cwd: '/tmp' });
        assert.strictEqual(result.exitCode, 0);
        assert.ok(
            result.stdout.trim().startsWith('/tmp'),
            `Expected /tmp, got ${result.stdout.trim()}`,
        );
    });

    it('captures both stdout and stderr from single command', async () => {
        const result = await exec('node', [
            '-e',
            'process.stdout.write("out"); process.stderr.write("err"); process.exit(0)',
        ]);
        assert.strictEqual(result.exitCode, 0);
        assert.strictEqual(result.stdout, 'out');
        assert.strictEqual(result.stderr, 'err');
    });

    it('handles empty output', async () => {
        const result = await exec('node', ['-e', 'process.exit(0)']);
        assert.strictEqual(result.exitCode, 0);
        assert.strictEqual(result.stdout, '');
        assert.strictEqual(result.stderr, '');
    });
});
