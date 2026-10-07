import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { run } from '../utils/spawn';

const node = process.execPath;

describe('run', () => {
    it('streams complete lines from both streams and returns the full output', async () => {
        const lines: string[] = [];
        const result = await run(
            node,
            ['-e', 'process.stdout.write("a\\nb"); process.stderr.write("e\\n"); process.exit(3)'],
            { onLine: (line, stream) => lines.push(`${stream}:${line}`) },
        );
        assert.strictEqual(result.exitCode, 3);
        assert.strictEqual(result.stdout, 'a\nb');
        assert.deepStrictEqual(lines.sort(), ['stderr:e', 'stdout:a', 'stdout:b']);
        assert.ok(!result.timedOut && !result.aborted);
    });

    it('merges env over the parent environment and honours cwd', async () => {
        const result = await run(node, ['-e', 'console.log(process.env.INFC_PATH, process.cwd())'], {
            env: { INFC_PATH: '/x/infc' },
            cwd: '/',
        });
        assert.strictEqual(result.stdout.trim(), '/x/infc /');
    });

    it('kills on timeout and on abort', async () => {
        const slow = ['-e', 'setTimeout(() => {}, 60000)'];
        const timed = await run(node, slow, { timeoutMs: 100 });
        assert.ok(timed.timedOut && timed.exitCode !== 0);

        const controller = new AbortController();
        setTimeout(() => controller.abort(), 100);
        const aborted = await run(node, slow, { signal: controller.signal });
        assert.ok(aborted.aborted);
    });

    it('stops the whole process tree, so a grandchild holding the pipes cannot hang it', { skip: process.platform === 'win32' }, async () => {
        // The child starts a grandchild that inherits stdout, as infs starts infc.
        const script = [
            "const cp = require('child_process');",
            "const g = cp.spawn(process.execPath, ['-e', 'setTimeout(() => {}, 60000)'], { stdio: 'inherit' });",
            'console.log(g.pid);',
            'setTimeout(() => {}, 60000);',
        ].join(' ');
        let grandchild = 0;
        const controller = new AbortController();
        const started = Date.now();
        const result = await run(node, ['-e', script], {
            signal: controller.signal,
            onLine: (line) => {
                grandchild = Number(line);
                controller.abort();
            },
        });
        assert.ok(result.aborted);
        assert.ok(Date.now() - started < 10_000, 'run returned promptly');
        assert.ok(grandchild > 0);
        assert.throws(() => process.kill(grandchild, 0), 'the grandchild was stopped too');
    });

    it('rejects when the command cannot be spawned', async () => {
        await assert.rejects(run('/nonexistent/infs', []));
    });
});
