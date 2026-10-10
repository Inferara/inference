import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { declarationLine, maskCommentsAndStrings } from '../prover/rocqSource';

// The shape the compiler emits (command_receiver.input.v): each obligation is a
// theorem whose proof is a hole marker two lines below the statement.
const INPUT = [
    'Section Host.',
    'Context `{ho: host}.',
    '',
    'Theorem valid_command_receiver : ValidModule command_receiver.',
    'Proof.',
    '  (* TODO: fill the proof *)',
    'Qed.',
    '',
    'Theorem valid_command_receiver__AcceptedShutdown : ValidSpec command_receiver command_receiver__AcceptedShutdown_specs.',
    'Proof.',
    '  (* TODO: fill the proof *)',
    'Qed.',
].join('\n');

describe('declarationLine', () => {
    it('finds the theorem statement, not the proof hole', () => {
        assert.strictEqual(declarationLine(INPUT, 'valid_command_receiver__AcceptedShutdown'), 9);
        assert.strictEqual(declarationLine(INPUT, 'valid_command_receiver'), 4);
    });

    it('does not match a longer name with the same prefix', () => {
        assert.strictEqual(declarationLine('Lemma t_helper : True.\nTheorem t : True.', 't'), 2);
        assert.strictEqual(declarationLine("Lemma t' : True.", 't'), undefined);
    });

    it('accepts other declaration keywords, attributes and modifiers, and CRLF', () => {
        assert.strictEqual(declarationLine('x\r\n  Lemma t : True.', 't'), 2);
        assert.strictEqual(declarationLine('#[local] Instance t : C.', 't'), 1);
        assert.strictEqual(declarationLine('Local Definition t := 1.', 't'), 1);
        assert.strictEqual(declarationLine('Program Fixpoint t n := n.', 't'), 1);
    });

    it('falls back to the last segment of a qualified name', () => {
        assert.strictEqual(declarationLine(INPUT, 'Host.valid_command_receiver__AcceptedShutdown'), 9);
    });

    it('ignores mentions that are not declarations', () => {
        const text = '(* Theorem t is proved below *)\nexact t.\nCheck t.';
        assert.strictEqual(declarationLine(text, 't'), undefined);
    });

    it('skips declarations inside comments and strings', () => {
        assert.strictEqual(declarationLine('(*\nTheorem t : True.\n*)\nTheorem t : True.', 't'), 4);
        assert.strictEqual(declarationLine('(* (* nested *)\nTheorem t : True. *)\nLemma t : True.', 't'), 3);
        assert.strictEqual(declarationLine('Definition s := "\nTheorem t".\nTheorem t : True.', 't'), 3);
        assert.strictEqual(declarationLine('(* "*)" *)\nTheorem t : True.', 't'), 2);
    });

    it('masks comments and strings without moving lines or columns', () => {
        const text = 'a (* b\r\n c *) "d""e" (* "f*)" *) g';
        const masked = maskCommentsAndStrings(text);
        assert.strictEqual(masked.length, text.length);
        assert.strictEqual(masked, `a${' '.repeat(5)}\r\n${' '.repeat(6)}"${' '.repeat(4)}"${' '.repeat(13)}g`);
    });

    it('treats the name literally', () => {
        assert.strictEqual(declarationLine('Theorem a_b : True.', 'a.b'), undefined);
        assert.strictEqual(declarationLine('Theorem axb : True.', 'a.b'), undefined);
    });
});
