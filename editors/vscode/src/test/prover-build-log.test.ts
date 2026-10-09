import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { duplicateNameHint, fileStem, firstBuildError } from '../prover/buildLog';
import { pruneOrigins } from '../prover/jobOrigins';

describe('firstBuildError', () => {
    it('reads the located error Rocq prints (real server build.log)', () => {
        const log = 'File "/work/compiled.v", line 53, characters 11-16:\nError: clamp already exists.\n\n';
        assert.deepStrictEqual(firstBuildError(log), {
            line: 53, startChar: 11, endChar: 16, message: 'clamp already exists.',
        });
    });

    it('keeps a multi-line message and stops at the next location', () => {
        const log = [
            'File "/work/compiled.v", line 9, characters 2-10:',
            'Error:',
            'In environment',
            'x : nat',
            'File "/work/compiled.v", line 12, characters 0-1:',
            'Error: later',
        ].join('\n');
        const error = firstBuildError(log);
        assert.strictEqual(error?.line, 9);
        assert.strictEqual(error?.message, 'In environment\nx : nat');
    });

    it('falls back to an unlocated Error block, and to null', () => {
        assert.deepStrictEqual(firstBuildError('noise\nError: Cannot find library Foo.\n'), { message: 'Cannot find library Foo.' });
        assert.strictEqual(firstBuildError('all good\n'), null);
    });
});

describe('duplicateNameHint', () => {
    it('explains the module-named-after-the-file clash', () => {
        assert.ok(duplicateNameHint('clamp already exists.', ['clamp'])?.includes('names the module after the file'));
        assert.ok(duplicateNameHint('foo already exists.', ['clamp'])?.includes('defined twice'));
        assert.strictEqual(duplicateNameHint('Syntax error', ['clamp']), null);
    });

    it('takes stems from paths and names', () => {
        assert.strictEqual(fileStem('/a/b/clamp.inf'), 'clamp');
        assert.strictEqual(fileStem('clamp.v'), 'clamp');
        assert.strictEqual(fileStem(null), null);
    });
});

describe('pruneOrigins', () => {
    it('keeps the newest entries', () => {
        const kept = pruneOrigins({ a: { at: 1 }, b: { at: 3 }, c: { at: 2 } }, 2);
        assert.deepStrictEqual(Object.keys(kept).sort(), ['b', 'c']);
    });
});
