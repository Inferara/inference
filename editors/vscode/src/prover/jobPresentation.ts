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
