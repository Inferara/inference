import { DoctorResult } from '../toolchain/doctor';

export type StatusBarIcon = 'loading' | 'dash' | 'check' | 'warning' | 'error';
export type StatusBarBackground = 'none' | 'warning' | 'error';

export interface StatusBarState {
    icon: StatusBarIcon;
    label: string;
    tooltip: string;
    background: StatusBarBackground;
}

export type ToolchainHealth = 'missing' | 'healthy' | 'warnings' | 'degraded' | 'errors';

/**
 * Overall toolchain health. `degraded`: some doctor check failed, but `infs`
 * resolved a working `infc` — compiling (and proving) works. A custom
 * `inference.path` binary, for example, can fail the Platform check.
 */
export function toolchainHealth(result: DoctorResult | null): ToolchainHealth {
    if (result === null) {
        return 'missing';
    }
    if (result.hasErrors) {
        const compiler = result.checks.find((c) => c.name === 'Resolved infc');
        return compiler?.status === 'ok' ? 'degraded' : 'errors';
    }
    return result.hasWarnings ? 'warnings' : 'healthy';
}

/** The failing checks, as `Name — message`, for tooltips and logs. */
export function failingChecks(result: DoctorResult | null): string[] {
    return (result?.checks ?? [])
        .filter((c) => c.status === 'fail')
        .map((c) => `${c.name} — ${c.message}`);
}

/** One-word status for the Configuration view and the log. */
export function toolchainHealthLabel(health: ToolchainHealth): string {
    switch (health) {
        case 'missing':
            return 'unknown';
        case 'degraded':
            return 'works with issues';
        default:
            return health;
    }
}

/**
 * Determine the status bar display state from a doctor result.
 *
 * - null: toolchain not found (dash icon)
 * - hasErrors: red error icon
 * - hasWarnings: yellow warning icon
 * - all OK: green check icon
 */
export function determineStatusBarState(result: DoctorResult | null): StatusBarState {
    if (result === null) {
        return {
            icon: 'dash',
            label: 'Inference',
            tooltip: 'Inference: Toolchain not found. Click to run doctor.',
            background: 'none',
        };
    }

    if (toolchainHealth(result) === 'degraded') {
        return {
            icon: 'warning',
            label: 'Inference',
            tooltip: `Inference: the compiler works (infc found), but some toolchain checks failed: ${failingChecks(result).join('; ')}. Click to run the doctor.`,
            background: 'none',
        };
    }

    if (result.hasErrors) {
        return {
            icon: 'error',
            label: 'Inference',
            tooltip: `Inference: ${result.summary || 'Toolchain errors detected'}`,
            background: 'error',
        };
    }

    if (result.hasWarnings) {
        return {
            icon: 'warning',
            label: 'Inference',
            tooltip: `Inference: ${result.summary || 'Toolchain warnings detected'}`,
            background: 'warning',
        };
    }

    return {
        icon: 'check',
        label: 'Inference',
        tooltip: 'Inference: Toolchain healthy',
        background: 'none',
    };
}
