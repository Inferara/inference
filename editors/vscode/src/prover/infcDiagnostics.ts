/**
 * Compiler errors from `infs build` output, for the Problems panel. Pure — no
 * `vscode` import.
 *
 * Formats printed by infc (positions are 1-based `line:column`):
 *
 *     Parse error: failed to parse `x.inf`:
 *       2:15: expected an expression
 *     Type checking failed: 2:5: type mismatch …; 3:12: use of undeclared variable `z`
 *     [module::path:]4:1: error[A036]: …
 *     error: <message without a position>
 *
 * Messages in a `Type checking failed:` line are joined by `; `; an optional
 * `module::path:` prefix names the file of a submodule.
 */

export interface InfcDiagnostic {
    /** 1-based. */
    line: number;
    /** 1-based. */
    column: number;
    message: string;
    /** `a::b` when the error is in a submodule file. */
    module?: string;
    /** Analysis code, e.g. `A036`. */
    code?: string;
}

export interface ParsedDiagnostics {
    located: InfcDiagnostic[];
    /** Errors without a position (shown on the first line). */
    unlocated: string[];
}

const MODULE = '[A-Za-z_]\\w*(?:::[A-Za-z_]\\w*)*';
const LOCATED = new RegExp(`^(?:(${MODULE}):)?(\\d+):(\\d+):\\s+(.+)$`);
const ANALYSIS = new RegExp(`^(?:(${MODULE}):)?(\\d+):(\\d+):\\s+(?:error|warning)(?:\\[(\\w+)\\])?:\\s+(.+)$`);
const SPLIT = new RegExp(`;\\s+(?=(?:${MODULE}:)?\\d+:\\d+:\\s)`);

function located(text: string): InfcDiagnostic | null {
    const analysis = ANALYSIS.exec(text);
    if (analysis) {
        const [, module, line, column, code, message] = analysis;
        return {
            line: Number(line),
            column: Number(column),
            message: message.trim(),
            ...(module ? { module } : {}),
            ...(code ? { code } : {}),
        };
    }
    const m = LOCATED.exec(text);
    if (!m) {
        return null;
    }
    const [, module, line, column, message] = m;
    return { line: Number(line), column: Number(column), message: message.trim(), ...(module ? { module } : {}) };
}

export function parseInfcDiagnostics(output: string): ParsedDiagnostics {
    const lines = output.replace(/\r\n?/g, '\n').split('\n');
    const found: InfcDiagnostic[] = [];
    const unlocated: string[] = [];
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (/^Parse error:/.test(line)) {
            for (let j = i + 1; j < lines.length && /^\s+\S/.test(lines[j]); j++) {
                const d = located(lines[j].trim());
                if (d) {
                    found.push(d);
                }
                i = j;
            }
            continue;
        }
        const typeCheck = /^Type checking failed:\s*(.*)$/.exec(line);
        if (typeCheck) {
            for (const part of typeCheck[1].split(SPLIT)) {
                const d = located(part.trim());
                if (d) {
                    found.push(d);
                } else if (part.trim()) {
                    unlocated.push(part.trim());
                }
            }
            continue;
        }
        const d = ANALYSIS.test(line.trim()) ? located(line.trim()) : null;
        if (d) {
            found.push(d);
            continue;
        }
        const error = /^error:\s*(.+)$/.exec(line.trim());
        if (error) {
            unlocated.push(error[1]);
        }
    }
    const seen = new Set<string>();
    const unique = found.filter((d) => {
        const key = `${d.module ?? ''}|${d.line}|${d.column}|${d.message}`;
        if (seen.has(key)) {
            return false;
        }
        seen.add(key);
        return true;
    });
    return { located: unique, unlocated };
}

/** Short summary for a notification: count and the first message. */
export function summarizeDiagnostics(parsed: ParsedDiagnostics): string | null {
    const total = parsed.located.length + parsed.unlocated.length;
    if (total === 0) {
        return null;
    }
    const first = parsed.located[0];
    const head = first ? `${first.line}:${first.column} ${first.message}` : parsed.unlocated[0];
    return `${total === 1 ? '1 compile error' : `${total} compile errors`}. ${head}`;
}
