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

/**
 * Whether a `files.associations` glob matches a path the way VS Code applies
 * it: case-insensitively, against the whole path when the pattern has a `/`,
 * else against the file name. Supports `**`, `*` and `?`.
 */
export function associationMatches(pattern: string, filePath: string): boolean {
    const target = pattern.includes('/') ? filePath : filePath.slice(filePath.lastIndexOf('/') + 1);
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
        } else {
            source += escapeRegExp(c);
        }
    }
    return new RegExp(`^${source}$`, 'i').test(target);
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
