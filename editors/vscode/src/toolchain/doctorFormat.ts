import { DoctorResult } from './doctor';

const STATUS_TAGS: Record<string, string> = {
    ok: '[OK]  ',
    warn: '[WARN]',
    fail: '[FAIL]',
};

/**
 * Format doctor check results into display lines.
 *
 * Each check is formatted as: `  [TAG] name: message`
 * If a summary is present, it is appended after a blank line.
 * Separator lines wrap the output.
 */
export function formatDoctorChecks(result: DoctorResult): string[] {
    const lines: string[] = [];
    lines.push('--- Doctor Report ---');
    for (const check of result.checks) {
        const tag = STATUS_TAGS[check.status] ?? `[${check.status.toUpperCase()}]`;
        lines.push(`  ${tag} ${check.name}: ${check.message}`);
    }
    if (result.summary) {
        lines.push('');
        lines.push(result.summary);
    }
    lines.push('---------------------');
    return lines;
}
