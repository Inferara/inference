/**
 * Finding declarations in a Rocq file. Pure — no `vscode` import.
 *
 * Obligation names are theorem names (`valid_m__Spec`). Their line differs
 * between the submitted file and the returned proof (proofs and helper lemmas
 * are added), so links search the text instead of trusting a line number.
 */

const KEYWORDS = 'Theorem|Lemma|Corollary|Proposition|Fact|Remark|Example|Definition|Fixpoint|CoFixpoint|Instance';

function escapeRegExp(text: string): string {
    return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * The text with comments (nested `(* *)`) and string contents replaced by
 * spaces, keeping every line break, so line numbers stay valid. As in Rocq,
 * a string inside a comment is skipped as a whole.
 */
export function maskCommentsAndStrings(text: string): string {
    let out = '';
    let depth = 0;
    let inString = false;
    const blank = (c: string) => (c === '\n' || c === '\r' ? c : ' ');
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        const next = text[i + 1];
        if (inString) {
            if (c === '"' && next === '"') {
                out += '  ';
                i++;
            } else if (c === '"') {
                inString = false;
                out += depth > 0 ? ' ' : c;
            } else {
                out += blank(c);
            }
        } else if (c === '(' && next === '*') {
            depth++;
            out += '  ';
            i++;
        } else if (depth > 0 && c === '*' && next === ')') {
            depth--;
            out += '  ';
            i++;
        } else if (c === '"') {
            inString = true;
            out += depth > 0 ? ' ' : c;
        } else {
            out += depth > 0 ? blank(c) : c;
        }
    }
    return out;
}

/**
 * The 1-based line declaring `name` (or, for a qualified name, its last
 * segment), or undefined when the text has no such declaration. Comments and
 * strings are ignored.
 */
export function declarationLine(text: string, name: string): number | undefined {
    const last = name.split('.').pop() ?? name;
    const names = last && last !== name ? [name, last] : [name];
    const lines = maskCommentsAndStrings(text).split(/\r\n?|\n/);
    for (const candidate of names) {
        if (!candidate) {
            continue;
        }
        const declaration = new RegExp(
            `^\\s*(?:#\\[[^\\]]*\\]\\s*)?(?:(?:Local|Global|Polymorphic|Program)\\s+)*(?:${KEYWORDS})\\s+${escapeRegExp(candidate)}(?![\\w'])`,
        );
        const index = lines.findIndex((line) => declaration.test(line));
        if (index >= 0) {
            return index + 1;
        }
    }
    return undefined;
}
