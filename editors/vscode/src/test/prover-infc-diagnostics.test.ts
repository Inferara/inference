import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { parseInfcDiagnostics, summarizeDiagnostics } from '../prover/infcDiagnostics';
import { classifyReplay, duplicateDefinitions, preflight } from '../prover/submission';

describe('parseInfcDiagnostics (output of the accepted infc, d71a9c3e)', () => {
    it('reads parse errors listed under the header', () => {
        const parsed = parseInfcDiagnostics([
            'Parse error: failed to parse `parse.inf`:',
            '  2:15: expected an expression',
            '  3:1: expected Semi',
            '  2:15: expected an expression',
        ].join('\n'));
        assert.deepStrictEqual(parsed.located, [
            { line: 2, column: 15, message: 'expected an expression' },
            { line: 3, column: 1, message: 'expected Semi' },
        ]);
    });

    it('splits type errors joined on one line, keeping colons and backticks in messages', () => {
        const parsed = parseInfcDiagnostics(
            'Parsed: types.inf\nType checking failed: 2:5: type mismatch in variable definition: expected `Bool`, found `i32`; 3:12: use of undeclared variable `z`; math::ops:3:5: type mismatch in return statement: expected `i32`, found `Unit`',
        );
        assert.deepStrictEqual(parsed.located, [
            { line: 2, column: 5, message: 'type mismatch in variable definition: expected `Bool`, found `i32`' },
            { line: 3, column: 12, message: 'use of undeclared variable `z`' },
            { line: 3, column: 5, message: 'type mismatch in return statement: expected `i32`, found `Unit`', module: 'math::ops' },
        ]);
        assert.strictEqual(summarizeDiagnostics(parsed), '3 compile errors. 2:5 type mismatch in variable definition: expected `Bool`, found `i32`');
    });

    it('reads analysis findings with codes and unlocated errors', () => {
        const parsed = parseInfcDiagnostics('4:1: error[A036]: something is wrong\nerror: cannot read file\n');
        assert.deepStrictEqual(parsed.located, [{ line: 4, column: 1, message: 'something is wrong', code: 'A036' }]);
        assert.deepStrictEqual(parsed.unlocated, ['cannot read file']);
        assert.strictEqual(summarizeDiagnostics({ located: [], unlocated: [] }), null);
    });
});

describe('duplicate definitions', () => {
    const generated = 'Definition clamp : module_func := {|\n|}.\n\nDefinition clamp : module := {|\n|}.\nTheorem valid_clamp : ValidModule clamp.\n(* TODO: fill the proof *)\n';

    it('finds the module-named-after-the-file clash', () => {
        assert.deepStrictEqual(duplicateDefinitions(generated), [{ name: 'clamp', lines: [1, 4] }]);
        assert.deepStrictEqual(duplicateDefinitions('Definition a := 1.\n  Definition a := 2.\n'), [], 'column 0 only');
    });

    it('blocks the upload with a fix', () => {
        const checked = preflight('clamp.v', new TextEncoder().encode(generated));
        assert.ok(!checked.ok && checked.problem.includes('names the module after the file'));
        assert.ok(!checked.ok && checked.duplicate?.name === 'clamp');
    });
});

describe('classifyReplay', () => {
    const now = Date.parse('2026-10-09T12:00:00Z');
    it('trusts the server header when present', () => {
        assert.strictEqual(classifyReplay({ status: 'Queued', createdAt: '2026-10-09T12:00:00Z' }, true, now), true);
        assert.strictEqual(classifyReplay({ status: 'Succeeded', createdAt: '2026-10-01T00:00:00Z' }, false, now), false);
    });

    it('falls back to status and age on older servers', () => {
        assert.strictEqual(classifyReplay({ status: 'Queued', createdAt: '2026-10-09T11:59:59Z' }, null, now), false);
        assert.strictEqual(classifyReplay({ status: 'Succeeded', createdAt: '2026-10-09T11:59:59Z' }, null, now), true);
        assert.strictEqual(classifyReplay({ status: 'Queued', createdAt: '2026-10-07T08:29:37Z' }, null, now), true);
    });
});
