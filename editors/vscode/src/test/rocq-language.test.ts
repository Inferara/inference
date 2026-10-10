import * as assert from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { describe, it } from 'node:test';

import { associationMatches, ROCQ_LANGUAGE_ID, shouldShowAsRocq } from '../rocq/rocqDocuments';

const root = path.resolve(__dirname, '..', '..');
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf-8'));

describe('shouldShowAsRocq', () => {
    const plain = (p: string) => ({ languageId: 'plaintext', path: p });

    it('shows a .v file that opened as plain text as Rocq', () => {
        assert.strictEqual(shouldShowAsRocq(plain('/w/out/bounds.v'), []), true);
        assert.strictEqual(shouldShowAsRocq(plain('/job/artifact/completed.V'), []), true);
    });

    it('leaves files another extension or setting gave a language', () => {
        assert.strictEqual(shouldShowAsRocq({ languageId: 'coq', path: '/w/a.v' }, []), false);
        assert.strictEqual(shouldShowAsRocq({ languageId: 'verilog', path: '/w/a.v' }, []), false);
        assert.strictEqual(shouldShowAsRocq({ languageId: ROCQ_LANGUAGE_ID, path: '/w/a.v' }, []), false);
    });

    it('leaves other plain-text files', () => {
        assert.strictEqual(shouldShowAsRocq(plain('/w/notes.txt'), []), false);
        assert.strictEqual(shouldShowAsRocq(plain('/w/a.vo'), []), false);
        assert.strictEqual(shouldShowAsRocq(plain('/w/v'), []), false);
    });

    it('respects files.associations entries that keep .v files plain text', () => {
        assert.strictEqual(shouldShowAsRocq(plain('/w/a.v'), ['*.v']), false);
        assert.strictEqual(shouldShowAsRocq(plain('/w/rtl/top.v'), ['**/rtl/*.v']), false);
        assert.strictEqual(shouldShowAsRocq(plain('/w/proofs/top.v'), ['**/rtl/*.v']), true);
        assert.strictEqual(shouldShowAsRocq(plain('/w/a.v'), ['*.txt']), true);
    });
});

describe('associationMatches', () => {
    it('matches a pattern without a slash against the file name', () => {
        assert.strictEqual(associationMatches('*.v', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('x.v', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('?.v', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('*.V', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('y.v', '/a/b/x.v'), false);
    });

    it('matches a pattern with a slash against the whole path', () => {
        assert.strictEqual(associationMatches('**/*.v', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('**/b/*.v', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('/a/**', '/a/b/x.v'), true);
        assert.strictEqual(associationMatches('b/*.v', '/a/b/x.v'), false);
        assert.strictEqual(associationMatches('**/c/*.v', '/a/b/x.v'), false);
    });

    it('supports brace alternatives and character classes', () => {
        assert.strictEqual(associationMatches('*.{v,txt}', '/w/a.v'), true);
        assert.strictEqual(associationMatches('*.{v,txt}', '/w/a.txt'), true);
        assert.strictEqual(associationMatches('*.{v,txt}', '/w/a.vo'), false);
        assert.strictEqual(associationMatches('{**/rtl/*.v,*.sv}', '/w/rtl/top.v'), true);
        assert.strictEqual(associationMatches('*.[vV]', '/w/a.v'), true);
        assert.strictEqual(associationMatches('[!a].v', '/w/b.v'), true);
        assert.strictEqual(associationMatches('[!a].v', '/w/a.v'), false);
        assert.strictEqual(associationMatches('[a-c].v', '/w/b.v'), true);
        assert.strictEqual(shouldShowAsRocq({ languageId: 'plaintext', path: '/w/a.v' }, ['*.{v,txt}']), false);
    });

    it('treats unclosed groups literally', () => {
        assert.strictEqual(associationMatches('{a.v', '/w/{a.v'), true);
        assert.strictEqual(associationMatches('[a.v', '/w/[a.v'), true);
    });

    it('treats other characters literally', () => {
        assert.strictEqual(associationMatches('a+b.v', '/w/a+b.v'), true);
        assert.strictEqual(associationMatches('a+b.v', '/w/aab.v'), false);
        assert.strictEqual(associationMatches('(x).v', '/w/(x).v'), true);
    });
});

describe('Rocq language contribution', () => {
    const language = pkg.contributes.languages.find((l: { id: string }) => l.id === ROCQ_LANGUAGE_ID);
    const grammar = pkg.contributes.grammars.find((g: { language: string }) => g.language === ROCQ_LANGUAGE_ID);

    it('declares the language without claiming .v (VsCoq, coq-lsp and Verilog extensions own it)', () => {
        assert.ok(language);
        for (const key of ['extensions', 'filenames', 'filenamePatterns', 'firstLine', 'mimetypes']) {
            assert.strictEqual(language[key], undefined, `${key} would claim files`);
        }
        assert.deepStrictEqual(language.aliases, ['Rocq (Inference)']);
        assert.ok(fs.existsSync(path.join(root, language.configuration)));
    });

    it('uses its own grammar scope, so it never replaces another Rocq grammar', () => {
        assert.ok(grammar);
        assert.strictEqual(grammar.scopeName, 'source.inference-rocq');
        const file = JSON.parse(fs.readFileSync(path.join(root, grammar.path), 'utf-8'));
        assert.strictEqual(file.scopeName, grammar.scopeName);
    });

    it('does not activate the extension for every plain-text file', () => {
        assert.ok(!pkg.activationEvents.some((e: string) => /onLanguage:(plaintext|inference-rocq)|\*\.v\b/.test(e)));
    });
});
