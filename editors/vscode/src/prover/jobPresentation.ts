import type { ClaimClass, JobStatus, RunMode } from './types';

/**
 * An unqualified behavioral-proof claim requires durable proof mode, valid
 * complete counts, and the control plane's strongest claim classification.
 */
export function isProvedSuccess(
    status: JobStatus,
    mode: RunMode | null | undefined,
    holesTotal: number | null | undefined,
    holesClosed: number | null | undefined,
    claimClass: ClaimClass | null | undefined,
): boolean {
    return (
        status === 'Succeeded' &&
        mode === 'prove' &&
        typeof holesTotal === 'number' &&
        holesTotal > 0 &&
        typeof holesClosed === 'number' &&
        holesClosed === holesTotal &&
        claimClass === 'Verified'
    );
}

/** Status text that does not leave an unqualified legacy Succeeded claim. */
export function jobStatusLabel(
    status: JobStatus,
    mode: RunMode | null | undefined,
    holesTotal: number | null | undefined,
    holesClosed: number | null | undefined,
    claimClass: ClaimClass | null | undefined,
): string {
    if (status !== 'Succeeded') {
        return status;
    }
    if (
        isProvedSuccess(
            status,
            mode,
            holesTotal,
            holesClosed,
            claimClass,
        )
    ) {
        return status;
    }
    if (mode === 'prove') {
        if (
            typeof holesTotal !== 'number' ||
            holesTotal <= 0 ||
            typeof holesClosed !== 'number' ||
            holesClosed !== holesTotal
        ) {
            return `${status} (counts invalid)`;
        }
        if (claimClass === 'StructuralOnly') {
            return `${status} (structural only)`;
        }
        return `${status} (claim unverified)`;
    }
    return `${status} (${mode === 'compile-goals' ? mode : 'mode unknown'})`;
}

/** Pure icon-id mapping so proof-success behavior is testable without VS Code. */
export function jobStatusIconId(
    status: JobStatus,
    mode: RunMode | null | undefined,
    holesTotal: number | null | undefined,
    holesClosed: number | null | undefined,
    claimClass: ClaimClass | null | undefined,
): string {
    switch (status) {
        case 'Queued':
        case 'Accepted':
        case 'Provisioning':
        case 'Booting':
            return 'clock';
        case 'Running':
            return 'sync~spin';
        case 'Verifying':
            return 'beaker';
        case 'Succeeded':
            return isProvedSuccess(
                status,
                mode,
                holesTotal,
                holesClosed,
                claimClass,
            )
                ? 'check'
                : 'warning';
        case 'CompileGoals':
        case 'PartialSuccess':
            return 'warning';
        case 'Failed':
        case 'TimedOut':
        case 'Lost':
        case 'ProvisionFailed':
            return 'x';
        case 'Canceling':
        case 'Canceled':
            return 'circle-slash';
        default:
            return 'question';
    }
}
