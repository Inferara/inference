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

    it('rejects when the command cannot be spawned', async () => {
        await assert.rejects(run('/nonexistent/infs', []));
    });
});
