/**
 * Fail-closed checks over the independent verifier's evidence. Pure — no
 * `vscode` import. A positive claim needs every check here to pass; the
 * wording built on top of them lives in ./jobVerdict.ts.
 */

import type { AssumptionReport, JobResponse, JobResultResponse } from './types';

/** The verifier's outcome, lowercased; null when the verifier did not report. */
export function normalizedVerifierOutcome(
    result: JobResultResponse | null | undefined,
): string | null {
    const value = result?.verificationOutcome?.trim().toLowerCase();
    return value || null;
}

/** Well-formed per-target reports, or null when any is malformed. */
export function normalizedAssumptionReports(
    result: JobResultResponse,
): AssumptionReport[] | null {
    const reports = result.assumptionReports;
    if (!Array.isArray(reports)) return null;
    for (const report of reports) {
        if (!report || typeof report.target !== 'string' || !report.target ||
            typeof report.kernelOutput !== 'string' || !report.kernelOutput ||
            typeof report.policyPassed !== 'boolean' ||
            !Array.isArray(report.assumptions)) {
            return null;
        }
        for (const assumption of report.assumptions) {
            if (!assumption || typeof assumption.name !== 'string' || !assumption.name ||
                typeof assumption.type !== 'string' || !assumption.type ||
                (assumption.kind !== 'axiom' && assumption.kind !== 'sectionVariable')) {
                return null;
            }
        }
    }
    return reports;
}

/** One passing report per closed target, and only for the expected targets. */
export function hasCompleteAssumptionEvidence(
    result: JobResultResponse,
    expectedTargets: string[],
): boolean {
    const reports = normalizedAssumptionReports(result);
    if (!result.assumptionPolicyId?.trim() || !reports) return false;
    const names = new Set(reports.map((report) => report.target));
    const expected = new Set(expectedTargets);
    return reports.length === result.holesClosed
        && names.size === reports.length
        && expected.size === expectedTargets.length
        && expected.size === result.holesClosed
        && reports.every((report) => expected.has(report.target))
        && reports.every((report) => report.policyPassed);
}

export function verifierAcceptedFull(
    result: JobResultResponse,
    expectedTargets: string[],
): boolean {
    return (
        normalizedVerifierOutcome(result) === 'succeeded' &&
        result.compileOk === true &&
        result.assumptionsClosed === true &&
        result.verifiedClean === true &&
        result.admitsDetected === false &&
        result.statementsImmutable === true &&
        result.markersRemaining === 0 &&
        hasCompleteAssumptionEvidence(result, expectedTargets)
    );
}

export function verifierAcceptedPartial(
    result: JobResultResponse,
    expectedTargets: string[],
): boolean {
    // Partial verification intentionally leaves the unfilled obligations
    // admitted, so verifiedClean=false/admitsDetected=true are expected here.
    return (
        normalizedVerifierOutcome(result) === 'partial' &&
        result.compileOk === true &&
        result.assumptionsClosed === true &&
        result.verifiedClean === false &&
        result.admitsDetected === true &&
        result.statementsImmutable === true &&
        result.markersRemaining === 0 &&
        hasCompleteAssumptionEvidence(result, expectedTargets)
    );
}

/** The job detail and its result describe the same run. */
export function resultMatchesJob(job: JobResponse, result: JobResultResponse): boolean {
    return (
        result.id === job.id &&
        result.status === job.status &&
        result.mode === job.mode &&
        result.holesTotal === job.holesTotal &&
        result.holesClosed === job.holesClosed &&
        result.claimClass === job.claimClass
    );
}
