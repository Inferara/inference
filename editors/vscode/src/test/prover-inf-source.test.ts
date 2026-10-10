import * as assert from 'node:assert';
import { describe, it } from 'node:test';

import { functionDeclaration, maskInfCommentsAndStrings } from '../prover/infSource';

describe('maskInfCommentsAndStrings', () => {
    it('blanks comments and string contents, keeping lines and columns', () => {
        const text = 'fn a() {} // fn clamp\r\n/// fn clamp docs\nlet s = "fn clamp";\n';
        const masked = maskInfCommentsAndStrings(text);
        assert.strictEqual(masked.length, text.length);
        assert.deepStrictEqual(masked.split('\n').map((l) => l.length), text.split('\n').map((l) => l.length));
        assert.strictEqual(masked, `fn a() {} ${' '.repeat(11)}\r\n${' '.repeat(17)}\nlet s = "${' '.repeat(8)}";\n`);
    });

    it('ends an unterminated string at the line break or backslash, as the lexer does', () => {
        assert.strictEqual(maskInfCommentsAndStrings('"ab\nfn x'), '"  \nfn x');
        assert.strictEqual(maskInfCommentsAndStrings('"ab\\ fn x'), '"  \\ fn x');
    });
});

describe('functionDeclaration', () => {
    it('finds the declaration, not a mention in a comment or string', () => {
        const text = [
            '// fn clamp used to live here',
            '/// see fn clamp below',
            'const NAME = "fn clamp";',
            'pub fn clamp(x: i32) -> i32 {',
        ].join('\n');
        assert.deepStrictEqual(functionDeclaration(text, 'clamp'), { line: 3, start: 4, end: 12 });
    });

    it('matches whole names only', () => {
        assert.strictEqual(functionDeclaration('fn clamp_low() {}', 'clamp'), undefined);
        assert.strictEqual(functionDeclaration('fn clamp() {}', 'cla.mp'), undefined);
        assert.deepStrictEqual(functionDeclaration('x; fn  clamp() {}', 'clamp'), { line: 0, start: 3, end: 12 });
    });
});
