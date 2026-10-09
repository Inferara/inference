/**
 * The first error in a server build log (`build.log`, Rocq output). Pure — no
 * `vscode` import.
 *
 * Rocq reports `File "<path>", line N, characters A-B:` followed by the
 * message. The server compiles the uploaded file in place (only the proof
 * terminators change), so line N is line N of the submitted `.v`.
 */

export interface BuildError {
    /** 1-based line in the compiled file, when the log names one. */
    line?: number;
    /** 0-based character range on that line. */
    startChar?: number;
    endChar?: number;
    /** The message without its `Error:` prefix, at most 600 characters. */
    message: string;
}

const LOCATION = /^File "[^"]*", line (\d+), characters (\d+)-(\d+):\s*$/;

function clean(lines: string[]): string {
    const text = lines
        .join('\n')
        .replace(/^\s*Error:\s*/, '')
        .trim();
    return text.length > 600 ? `${text.slice(0, 600)}…` : text;
}

/** First located error, else the first `Error:` block, else null. */
export function firstBuildError(log: string): BuildError | null {
    const lines = log.replace(/\r\n?/g, '\n').split('\n');
    for (let i = 0; i < lines.length; i++) {
        const m = LOCATION.exec(lines[i]);
        if (!m) {
            continue;
        }
        const body: string[] = [];
        for (let j = i + 1; j < lines.length; j++) {
            if (LOCATION.test(lines[j]) || (lines[j].trim() === '' && body.length > 0)) {
                break;
            }
            body.push(lines[j]);
        }
        const message = clean(body);
        if (message) {
            return {
                line: Number(m[1]),
                startChar: Number(m[2]),
                endChar: Number(m[3]),
                message,
            };
        }
    }
    const errorAt = lines.findIndex((l) => /^\s*Error:/.test(l));
    if (errorAt >= 0) {
        const body: string[] = [];
        for (let j = errorAt; j < lines.length && (j === errorAt || lines[j].trim() !== ''); j++) {
            body.push(lines[j]);
        }
        const message = clean(body);
        return message ? { message } : null;
    }
    return null;
}

/**
 * A hint for `<name> already exists`: the compiler names the module after the
 * file, so a function with the file's name defines it twice.
 */
export function duplicateNameHint(message: string, fileStems: readonly string[]): string | null {
    const m = /^(\S+) already exists\.?$/.exec(message.split('\n')[0].trim());
    if (!m) {
        return null;
    }
    const name = m[1];
    if (fileStems.includes(name)) {
        return `The compiler names the module after the file, so a function also named “${name}” defines “${name}” twice. Rename the file or the function, then prove again.`;
    }
    return `“${name}” is defined twice in the generated file.`;
}

/** File name without directory and final extension. */
export function fileStem(name: string | null | undefined): string | null {
    if (!name) {
        return null;
    }
    const base = name.split(/[\\/]/).pop() ?? name;
    const dot = base.lastIndexOf('.');
    return dot > 0 ? base.slice(0, dot) : base;
}
