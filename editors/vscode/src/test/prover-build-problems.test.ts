import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { BuildProblems } from '../prover/buildProblems';

const id = (message: string) => message;

describe('BuildProblems', () => {
    it("keeps another entry's errors in a shared import when one entry stops importing it", () => {
        const problems = new BuildProblems<string>();
        problems.add('a.inf', 'lib/broken.inf', ['1:14 expected an argument']);
        problems.add('b.inf', 'lib/broken.inf', ['1:14 expected an argument']);
        assert.deepStrictEqual(problems.shown('lib/broken.inf', id), ['1:14 expected an argument'], 'shown once');

        // a.inf no longer imports the file and now builds cleanly.
        assert.deepStrictEqual(problems.start('a.inf'), ['lib/broken.inf']);
        assert.deepStrictEqual(problems.shown('lib/broken.inf', id), ['1:14 expected an argument']);

        problems.start('b.inf');
        assert.deepStrictEqual(problems.shown('lib/broken.inf', id), []);
    });

    it("replaces only the rebuilt entry's errors", () => {
        const problems = new BuildProblems<string>();
        problems.add('a.inf', 'a.inf', ['old']);
        problems.add('b.inf', 'shared.inf', ['from b']);
        problems.start('a.inf');
        problems.add('a.inf', 'a.inf', ['new']);
        problems.add('a.inf', 'shared.inf', ['from a']);
        assert.deepStrictEqual(problems.shown('a.inf', id), ['new']);
        assert.deepStrictEqual(problems.shown('shared.inf', id).sort(), ['from a', 'from b']);
    });

    it('forgets every build\'s errors in an edited file', () => {
        const problems = new BuildProblems<string>();
        problems.add('a.inf', 'shared.inf', ['from a']);
        problems.add('b.inf', 'shared.inf', ['from b']);
        problems.add('b.inf', 'b.inf', ['in b']);
        problems.forget('shared.inf');
        assert.deepStrictEqual(problems.shown('shared.inf', id), []);
        assert.deepStrictEqual(problems.shown('b.inf', id), ['in b']);
        assert.deepStrictEqual(problems.start('a.inf'), []);
    });
});
