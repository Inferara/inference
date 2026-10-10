/**
 * Finding declarations in an Inference (`.inf`) file. Pure — no `vscode`
 * import.
 */

/**
 * The text with comments and string contents replaced by spaces, keeping
 * every line break and column. As in the compiler's lexer, `//` (and `///`)
 * comments run to the end of the line, and a string ends at its closing
 * quote, or unterminated before a line break or backslash.
 */
export function maskInfCommentsAndStrings(text: string): string {
    let out = '';
    let i = 0;
    while (i < text.length) {
        const c = text[i];
        if (c === '/' && text[i + 1] === '/') {
            while (i < text.length && text[i] !== '\n') {
                out += text[i] === '\r' ? '\r' : ' ';
                i++;
            }
        } else if (c === '"') {
            out += c;
            i++;
            while (i < text.length && text[i] !== '"' && text[i] !== '\n' && text[i] !== '\\') {
                out += text[i] === '\r' ? '\r' : ' ';
                i++;
            }
            if (text[i] === '"') {
                out += '"';
                i++;
            }
        } else {
            out += c;
            i++;
        }
    }
    return out;
}

/**
 * Where `fn <name>` is declared: the 0-based line and the columns of the
 * `fn <name>` text, or undefined. Comments and strings are ignored.
 */
export function functionDeclaration(
    text: string,
    name: string,
): { line: number; start: number; end: number } | undefined {
    if (!/^\w+$/.test(name)) {
        return undefined;
    }
    const pattern = new RegExp(`\\bfn\\s+${name}\\b`);
    const lines = maskInfCommentsAndStrings(text).split('\n');
    for (let line = 0; line < lines.length; line++) {
        const m = pattern.exec(lines[line]);
        if (m) {
            return { line, start: m.index, end: m.index + m[0].length };
        }
    }
    return undefined;
}
