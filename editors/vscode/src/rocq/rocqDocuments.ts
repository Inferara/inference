/**
 * Which documents get this extension's Rocq highlighting. Pure — no `vscode`
 * import.
 *
 * `.v` is claimed by Rocq extensions (VsCoq, coq-lsp) and by Verilog ones, so
 * the Rocq language here declares no file extension: it would take files away
 * from those extensions or override their grammars. Instead a `.v` document
 * that opened as plain text (nothing else claimed it) is switched to it.
 */

/** Language id of the Rocq highlighting this extension contributes. */
export const ROCQ_LANGUAGE_ID = 'inference-rocq';

export interface DocumentFacts {
    languageId: string;
    /** The URI path (`/a/b/x.v`), whatever the scheme. */
    path: string;
}

function escapeRegExp(text: string): string {
    return text.replace(/[.+^${}()|[\]\\]/g, '\\$&');
}

/** Index of the `close` that ends the group opened at `open`, or -1. */
function closing(pattern: string, open: number, close: string): number {
    let depth = 0;
    for (let i = open; i < pattern.length; i++) {
        if (pattern[i] === pattern[open]) depth++;
        else if (pattern[i] === close && --depth === 0) return i;
    }
    return -1;
}

/** Split a `{a,b}` group's inside at its top-level commas. */
function alternatives(inside: string): string[] {
    const parts: string[] = [];
    let depth = 0;
    let start = 0;
    for (let i = 0; i < inside.length; i++) {
        if (inside[i] === '{') depth++;
        else if (inside[i] === '}') depth--;
        else if (inside[i] === ',' && depth === 0) {
            parts.push(inside.slice(start, i));
            start = i + 1;
        }
    }
    parts.push(inside.slice(start));
    return parts;
}

/** A glob as a regular expression source (VS Code syntax: `**`, `*`, `?`, `{a,b}`, `[abc]`, `[!a]`). */
function globSource(pattern: string): string {
    let source = '';
    for (let i = 0; i < pattern.length; i++) {
        const c = pattern[i];
        if (c === '*' && pattern[i + 1] === '*') {
            // `**/` is any number of directories (none included), `**` anything.
            if (pattern[i + 2] === '/') {
                source += '(?:.*/)?';
                i += 2;
            } else {
                source += '.*';
                i += 1;
            }
        } else if (c === '*') {
            source += '[^/]*';
        } else if (c === '?') {
            source += '[^/]';
        } else if (c === '{' && closing(pattern, i, '}') > i) {
            const end = closing(pattern, i, '}');
            source += `(?:${alternatives(pattern.slice(i + 1, end)).map(globSource).join('|')})`;
            i = end;
        } else if (c === '[' && pattern.indexOf(']', i + 2) > i) {
            const end = pattern.indexOf(']', i + 2);
            let set = pattern.slice(i + 1, end);
            const negated = set.startsWith('!') || set.startsWith('^');
            if (negated) set = set.slice(1);
            source += `[${negated ? '^/' : ''}${set.replace(/[\\\]^]/g, '\\$&')}]`;
            i = end;
        } else {
            source += escapeRegExp(c);
        }
    }
    return source;
}

/**
 * Whether a `files.associations` glob matches a path the way VS Code applies
 * it: case-insensitively, against the whole path when the pattern has a `/`,
 * else against the file name.
 */
export function associationMatches(pattern: string, filePath: string): boolean {
    const target = pattern.includes('/') ? filePath : filePath.slice(filePath.lastIndexOf('/') + 1);
    try {
        return new RegExp(`^${globSource(pattern)}$`, 'i').test(target);
    } catch {
        return false; // a malformed pattern matches nothing
    }
}

/**
 * Show a document as Rocq when it is a `.v` file that opened as plain text
 * and no `files.associations` entry keeps it plain text on purpose.
 */
export function shouldShowAsRocq(doc: DocumentFacts, plainTextPatterns: readonly string[]): boolean {
    if (doc.languageId !== 'plaintext' || !/\.v$/i.test(doc.path)) {
        return false;
    }
    return !plainTextPatterns.some((pattern) => associationMatches(pattern, doc.path));
}
