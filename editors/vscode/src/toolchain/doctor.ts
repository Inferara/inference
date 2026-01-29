import { exec } from '../utils/exec';

export type DoctorCheckStatus = 'ok' | 'warn' | 'fail';

export interface DoctorCheck {
    name: string;
    status: DoctorCheckStatus;
    message: string;
}

export interface DoctorResult {
    checks: DoctorCheck[];
    hasErrors: boolean;
    hasWarnings: boolean;
    summary: string;
}

const STATUS_MAP: Record<string, DoctorCheckStatus> = {
    OK: 'ok',
    WARN: 'warn',
    FAIL: 'fail',
};

/**
 * Check line format: `  [OK|WARN|FAIL] <name>: <message>`
 *
 * PATH conflict continuation lines (indented without a status prefix)
 * are intentionally not captured as individual checks.
 */
const CHECK_PATTERN = /^\s+\[(OK|WARN|FAIL)]\s+(.+?):\s+(.*)/;

/**
 * Parse the stdout of `infs doctor` into a structured result.
 *
 * The output format is a public contract defined in
 * `apps/infs/src/commands/doctor.rs`.
 */
export function parseDoctorOutput(stdout: string): DoctorResult {
    const checks: DoctorCheck[] = [];
    const lines = stdout.split('\n');

    for (const line of lines) {
        const match = line.match(CHECK_PATTERN);
        if (match) {
            checks.push({
                status: STATUS_MAP[match[1]],
                name: match[2].trim(),
                message: match[3].trim(),
            });
        }
    }

    let summary = '';
    for (let i = lines.length - 1; i >= 0; i--) {
        const trimmed = lines[i].trim();
        if (trimmed.length > 0 && !CHECK_PATTERN.test(lines[i])) {
            summary = trimmed;
            break;
        }
    }

    return {
        checks,
        hasErrors: checks.some((c) => c.status === 'fail'),
        hasWarnings: checks.some((c) => c.status === 'warn'),
        summary,
    };
}

/**
 * Execute `infs doctor` and return the parsed result.
 * Returns null if execution fails (e.g., binary not found or crash).
 */
export async function runDoctor(
    infsPath: string,
): Promise<DoctorResult | null> {
    try {
        const result = await exec(infsPath, ['doctor']);
        return parseDoctorOutput(result.stdout);
    } catch {
        return null;
    }
}
