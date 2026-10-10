/**
 * Compiler errors from `infs build` output, for the Problems panel. Pure — no
 * `vscode` import.
 *
 * Formats printed by infc (positions are 1-based `line:column`):
 *
 *     Parse error: failed to parse `x.inf`:
 *       2:15: expected an expression
 *     Parse error: failed to parse imported file `a::b`:
 *       3:1: expected an item
 *     Type checking failed: 2:5: type mismatch …; 3:12: use of undeclared variable `z`
 *     [module::path:]4:1: error[A036]: …     (or warning[…], kept as a warning)
 *     error: <message without a position>
 *
 * Messages in a `Type checking failed:` line are joined by `; `; an optional
 * `module::path:` prefix names the file of a submodule. A message can go on
 * over `note:` lines, and the next message then follows the last of them:
 *
 *     Type checking failed: 3:13: cannot apply operator `Add` …
 *     note: Inference has no implicit widening …; 5:5: use of undeclared variable `z`
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
    /** Set for an analysis warning; everything else is an error. */
    severity?: 'warning';
}

export interface ParsedDiagnostics {
    located: InfcDiagnostic[];
    /** Errors without a position (shown on the first line). */
    unlocated: string[];
}

const MODULE = '[A-Za-z_]\\w*(?:::[A-Za-z_]\\w*)*';
// `s`: a message keeps its note lines.
const LOCATED = new RegExp(`^(?:(${MODULE}):)?(\\d+):(\\d+):\\s+(.+)$`, 's');
const ANALYSIS = new RegExp(`^(?:(${MODULE}):)?(\\d+):(\\d+):\\s+(error|warning)(?:\\[(\\w+)\\])?:\\s+(.+)$`, 's');
const NOTE = /^\s*(?:note|help):\s/;
const SPLIT = new RegExp(`;\\s+(?=(?:${MODULE}:)?\\d+:\\d+:\\s)`);

function located(text: string): InfcDiagnostic | null {
    const analysis = ANALYSIS.exec(text);
    if (analysis) {
        const [, module, line, column, severity, code, message] = analysis;
        return {
            line: Number(line),
            column: Number(column),
            message: message.trim(),
            ...(module ? { module } : {}),
            ...(code ? { code } : {}),
            ...(severity === 'warning' ? { severity } : {}),
        };
    }
    const m = LOCATED.exec(text);
    if (!m) {
        return null;
    }
    const [, module, line, column, message] = m;
    return { line: Number(line), column: Number(column), message: message.trim(), ...(module ? { module } : {}) };
}

/** Line `i` (trimmed) with the note lines that continue it; `end` is the last line read. */
function withNotes(lines: readonly string[], i: number): { text: string; end: number } {
    let text = lines[i].trim();
    let end = i;
    while (end + 1 < lines.length && NOTE.test(lines[end + 1])) {
        end++;
        text += `\n${lines[end].trim()}`;
    }
    return { text, end };
}

export function parseInfcDiagnostics(output: string): ParsedDiagnostics {
    const lines = output.replace(/\r\n?/g, '\n').split('\n');
    const found: InfcDiagnostic[] = [];
    const unlocated: string[] = [];
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (/^Parse error:/.test(line)) {
            // An imported file is named by its module path in the header only.
            const imported = new RegExp(`failed to parse imported file \`(${MODULE})\``).exec(line);
            for (let j = i + 1; j < lines.length && /^\s+\S/.test(lines[j]); j++) {
                const d = located(lines[j].trim());
                if (d) {
                    found.push(imported && !d.module ? { ...d, module: imported[1] } : d);
                }
                i = j;
            }
            continue;
        }
        if (/^Type checking failed:/.test(line)) {
            const { text, end } = withNotes(lines, i);
            i = end;
            for (const part of text.replace(/^Type checking failed:\s*/, '').split(SPLIT)) {
                const d = located(part.trim());
                if (d) {
                    found.push(d);
                } else if (part.trim()) {
                    unlocated.push(part.trim());
                }
            }
            continue;
        }
        if (ANALYSIS.test(line.trim())) {
            const { text, end } = withNotes(lines, i);
            i = end;
            const d = located(text);
            if (d) {
                found.push(d);
            }
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

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** Short summary for a notification: the counts and the first error. */
export function summarizeDiagnostics(parsed: ParsedDiagnostics): string | null {
    const errors = parsed.located.filter((d) => d.severity !== 'warning');
    const warnings = parsed.located.length - errors.length;
    const total = errors.length + parsed.unlocated.length;
    if (total === 0) {
        return null;
    }
    const first = errors[0];
    const head = first ? `${first.line}:${first.column} ${first.message.split('\n')[0]}` : parsed.unlocated[0];
    return `${plural(total, 'compile error')}${warnings ? ` and ${plural(warnings, 'warning')}` : ''}. ${head}`;
}
