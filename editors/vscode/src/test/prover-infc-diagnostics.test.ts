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

    it('attributes parse errors in an imported file to its module (real infc output)', () => {
        const parsed = parseInfcDiagnostics([
            'Parse error: failed to parse imported file `lib::broken`:',
            '  1:14: expected an argument',
            '  1:14: expected RParen',
        ].join('\n'));
        assert.deepStrictEqual(parsed.located, [
            { line: 1, column: 14, message: 'expected an argument', module: 'lib::broken' },
            { line: 1, column: 14, message: 'expected RParen', module: 'lib::broken' },
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

    it('ignores comments and strings, and keeps module namespaces apart', () => {
        assert.deepStrictEqual(duplicateDefinitions('(*\nDefinition helper := 0.\n*)\nDefinition helper := 1.\n'), []);
        assert.deepStrictEqual(duplicateDefinitions('Definition s := "\nDefinition s := 2.".\n'), []);
        const modules = 'Module A.\nDefinition x := 1.\nEnd A.\nModule B.\nDefinition x := 2.\nEnd B.\nDefinition x := 3.\n';
        assert.deepStrictEqual(duplicateDefinitions(modules), []);
        assert.deepStrictEqual(
            duplicateDefinitions('Module A.\nDefinition x := 1.\nDefinition x := 2.\nEnd A.\n'),
            [{ name: 'A.x', lines: [2, 3] }],
        );
        // An alias opens no namespace; a section shares the enclosing one.
        assert.deepStrictEqual(
            duplicateDefinitions('Module C := B.\nDefinition y := 1.\nSection S.\nDefinition y := 2.\nEnd S.\n'),
            [{ name: 'y', lines: [2, 4] }],
        );
    });

    it('blocks a generated upload with a fix, and leaves hand-written files to the server', () => {
        const checked = preflight('clamp.v', new TextEncoder().encode(generated), undefined, true);
        assert.ok(!checked.ok && checked.problem.includes('names the module after the file'));
        assert.ok(!checked.ok && checked.duplicate?.name === 'clamp');
        assert.ok(preflight('clamp.v', new TextEncoder().encode(generated)).ok);
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
