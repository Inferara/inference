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
});
