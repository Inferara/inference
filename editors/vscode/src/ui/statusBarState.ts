import { DoctorResult } from '../toolchain/doctor';

export type StatusBarIcon = 'loading' | 'dash' | 'check' | 'warning' | 'error';
export type StatusBarBackground = 'none' | 'warning' | 'error';

export interface StatusBarState {
    icon: StatusBarIcon;
    label: string;
    tooltip: string;
    background: StatusBarBackground;
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
