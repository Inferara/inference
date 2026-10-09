/**
 * Job status sets mirrored from the server's state machine. Pure — no
 * `vscode` import.
 */

/** Statuses the control plane treats as terminal (mirrors the server). */
const TERMINAL_STATUSES: ReadonlySet<string> = new Set([
    'Succeeded',
    'CompileGoals',
    'PartialSuccess',
    'Failed',
    'TimedOut',
    'Lost',
    'Canceled',
    'ProvisionFailed',
]);

/**
 * Statuses `POST /jobs/{id}/cancel` accepts (mirrors the server's switch);
 * everything else — including Verifying and Canceling — is 409 NOT_CANCELABLE.
 */
const CANCELABLE_STATUSES: ReadonlySet<string> = new Set([
    'Accepted',
    'Queued',
    'Provisioning',
    'Booting',
    'Running',
]);

/** Artifact kinds that are text and open as read-only documents. */
const TEXT_ARTIFACT_KINDS: ReadonlySet<string> = new Set([
    'InputV',
    'CompletedV',
    'Report',
    'Goals',
    'BuildLog',
    'AgentLog',
    'RefutationV',
    'RefutationMd',
]);

export function isTerminalStatus(status: string): boolean {
    return TERMINAL_STATUSES.has(status);
}

/**
 * Statuses that end the server's event stream. `Lost` is soft-terminal: the
 * reaper re-queues it (bounded retry, Lost → Queued) and `StreamEvents`
 * deliberately keeps the SSE stream open without an `end` frame — so the
 * client must keep its transport (stream + polls) alive on Lost too, or it
 * freezes on a job that is actively being rerun.
 */
export function isStreamTerminalStatus(status: string): boolean {
    return TERMINAL_STATUSES.has(status) && status !== 'Lost';
}

export function isCancelableStatus(status: string): boolean {
    return CANCELABLE_STATUSES.has(status);
}

export function isTextArtifactKind(kind: string): boolean {
    return TEXT_ARTIFACT_KINDS.has(kind);
}
