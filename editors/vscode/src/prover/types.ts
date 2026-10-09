/**
 * Wire types for the Inference proof server public API (`/api/v1`).
 *
 * Single source of truth: `docs/CONTRACTS.md` (and `docs/api.md`) in
 * Inferara/inference-ai-prover, which lists this file as a client mirror.
 * Change the contract first, then mirror it here. Fields the server may omit
 * are optional so an older or newer server degrades instead of breaking.
 */

/**
 * Job lifecycle status.
 *
 * Non-terminal: Accepted, Queued, Provisioning, Booting, Running, Verifying,
 * Canceling. Terminal: Succeeded, CompileGoals, PartialSuccess, Failed,
 * TimedOut, Lost, Canceled, ProvisionFailed. `CompileGoals` means the input
 * compiled and goals were extracted, but no proof was accepted or returned.
 */
export type JobStatus =
    | 'Accepted'
    | 'Queued'
    | 'Provisioning'
    | 'Booting'
    | 'Running'
    | 'Verifying'
    | 'Canceling'
    | 'Succeeded'
    | 'CompileGoals'
    | 'PartialSuccess'
    | 'Failed'
    | 'TimedOut'
    | 'Lost'
    | 'Canceled'
    | 'ProvisionFailed';

/**
 * Persisted worker execution mode.
 *
 * `unknown` is stored when a worker reports an unsupported value; that job is
 * failed closed rather than being treated as a proof or compile-goals run.
 */
export type RunMode = 'compile-goals' | 'prove' | 'unknown';

/**
 * Proof-obligation kind (ground truth in CONTRACTS.md).
 *
 * - `module`: 1-arg `ValidModule <m>`
 * - `spec`: `ValidSpecFI <m> <payload>` or `ValidSpec <m> <payload>`
 * - `exists_spec`: `ValidExistsSpec <m> <payload>`
 * - `unique_spec`: `ValidUniqueSpec <m> <payload>`
 * - `unknown`: any other theorem carrying a hole marker
 */
export type ObligationKind =
    | 'module'
    | 'spec'
    | 'exists_spec'
    | 'unique_spec'
    | 'unknown';

/** Semantic relationship between an obligation and the compiled module. */
export type ContentClass =
    | 'structural'
    | 'empty'
    | 'tautology'
    | 'grounded'
    | 'unguarded'
    | 'unknown';

/** Strength of the durable claim the control plane permits for a job. */
export type ClaimClass =
    | 'Verified'
    | 'StructuralOnly'
    | 'PartialVerified'
    | 'Unverified';

/**
 * Per-obligation status as reported in job detail / report.json.
 * Mirrors the server's `ObligationStatus` enum, lowercased on the wire
 * (`skipped` = the harness ran out of budget/time before attempting it;
 * `refuted` = a machine-checked refutation of the obligation was accepted).
 */
export type ObligationStatus =
    | 'pending'
    | 'running'
    | 'proved'
    | 'failed'
    | 'skipped'
    | 'refuted';

/** A single proof obligation discovered within a submitted file. */
export interface ObligationDto {
    /** Theorem name, e.g. `valid_main__MySpec`. */
    name: string;
    kind: ObligationKind;
    /** Fail-closed semantic-content classification from inventory analysis. */
    content?: ContentClass | null;
    /** Identifier or `[:: ... ]` payload for spec-family obligations. */
    specList?: string | null;
    status?: ObligationStatus | null;
    /** Number of attempts spent on this obligation. */
    attempts?: number | null;
    durationMs?: number | null;
    lastError?: string | null;
    /** Goal state Rocq shows at this theorem's `Proof.` (compile-goals flow). */
    goal?: string | null;
    /** 1-based line of the theorem in the submitted `.v`; absent on older servers. */
    sourceLine?: number | null;
}

/**
 * Job detail / list element (`GET /api/v1/jobs` and `GET /api/v1/jobs/{id}`).
 *
 * Faithful mirror of the server's `JobResponse` record (Contracts.cs),
 * camelCase on the wire. Everything beyond `id`/`status` is optional so a
 * single shape serves both list and detail.
 */
export interface JobResponse {
    /** UUIDv7. */
    id: string;
    status: JobStatus;
    /** Original uploaded filename, e.g. `main.v`. */
    filename?: string | null;
    /** RFC 3339 submission timestamp. */
    createdAt?: string | null;
    /** Compute provider the job runs on (`kubernetes` | `ec2`). */
    provider?: string | null;
    /** Deployment-selected agent adapter reference, e.g. `claude-code`. */
    agentRef?: string | null;
    agentExecution?: PublicAgentExecution | null;
    maxWallClockSeconds?: number | null;
    holesTotal?: number | null;
    holesClosed?: number | null;
    holesFailed?: number | null;
    /** Highest server-assigned event sequence so far. */
    lastEventSeq?: number | null;
    obligations?: ObligationDto[] | null;
    /** Persisted worker mode; null before completion or for legacy rows. */
    mode?: RunMode | null;
    /** Server-derived outcome (terminal jobs only). */
    outcome?: string | null;
    /** Durable, server-derived strength of the result's behavioral claim. */
    claimClass?: ClaimClass | null;
    errorCode?: string | null;
    errorReason?: string | null;
    /**
     * When the time budget ends (submission + budget; queue time counts).
     * Absent on older servers: `job.queued` carries it, or derive it.
     */
    deadlineUtc?: string | null;
}

/** Paged list response for `GET /api/v1/jobs`. */
export interface JobListResponse {
    jobs: JobResponse[];
    /** Keyset-paging cursor, when more results exist. */
    nextCursor?: string | null;
}

/**
 * Per-job options accepted by `POST /api/v1/jobs` (`SubmitJobOptions`).
 *
 * The compute provider is internal routing chosen by the server from its config,
 * never by the caller, and there is no caller-chosen agent. Options permit a
 * wall-clock budget and a settings revision precondition; normal submits send none.
 */
export interface SubmitJobOptions {
    /** Wall-clock budget; server clamps to [60, cap]. */
    maxWallClockSeconds?: number;
    /** Positive revision precondition; a settings change rejects new admission. */
    expectedAgentSettingsRevision?: number | null;
}

/**
 * JSON body for `POST /api/v1/jobs` (`SubmitJobRequest`). The server also
 * accepts multipart; this client always submits JSON+base64.
 */
export interface SubmitJobRequest {
    schemaVersion: number;
    filename: string;
    contentBase64: string;
    options?: SubmitJobOptions;
}

/** One stored artifact, as listed by `/result` and `/artifacts` (`ArtifactInfo`). */
export interface ArtifactInfo {
    id: string;
    /**
     * Artifact kind: `InputV` | `CompletedV` | `Report` | `Goals` | `BuildLog` |
     * `AgentLog` (admins only) | `RefutationV` | `RefutationMd`.
     */
    kind: string;
    filename: string;
    sizeBytes: number;
    /** Hex SHA-256 of the content — verify after download. */
    sha256: string;
}

export type UsageCompleteness = 'complete' | 'partial' | 'unknown';

export interface AgentUsage {
    inputTokens: number | null;
    outputTokens: number | null;
    cachedInputTokens: number | null;
    reportedCostUsd: number | null;
    source: 'claude-code-cli' | 'codex-cli';
    completeness: UsageCompleteness;
}

export interface AgentActivityPayload {
    schemaVersion: 1;
    agentRef: 'claude-code' | 'codex-cli';
    attempt: number | null;
    theorem: string | null;
    kind: string;
    detail: string;
    usage: AgentUsage;
}

export interface UsageResponse {
    since: string;
    byStatus: Array<{ status: string; count: number }>;
    note: string;
}

/**
 * Response for `GET /api/v1/jobs/{id}/result` (`JobResultResponse`).
 * Terminal jobs only; non-terminal → 409 `NOT_TERMINAL`.
 */
/** A reviewed canonical comparison, recorded separately from the accepted job. */
export interface ArtifactBindingResponse {
    schemaVersion: 1;
    kind: 'canonical-fixture-runtime-body-identity';
    fixtureId: string;
    catalogId: string;
    catalogSha256: string;
    proof: ArtifactBindingProofResponse;
    comparison: ArtifactBindingComparisonResponse;
    proofWasm: ArtifactBindingFileResponse;
    runtimeWasm: ArtifactBindingFileResponse;
    report: ArtifactBindingFileResponse;
    formalEquivalenceVerified: false;
    comparisonReexecutedForJob: false;
    deploymentVerified: false;
}

export interface ArtifactBindingProofResponse {
    inputArtifactId: string;
    inputSha256: string;
    completedArtifactId: string;
    completedSha256: string;
    verifiedAtUtc: string;
    assumptionPolicyId: string;
}

export interface ArtifactBindingComparisonResponse {
    profile: 'state-independent-i32-v1';
    compilerCommit: string;
    sourceSha256: string;
    createdAtUtc: string;
    runtimeFunctions: number;
    specFunctions: number;
    proofOnlyMemories: number;
    proofOnlyGlobals: number;
}

export interface ArtifactBindingFileResponse {
    asset: 'proof.wasm' | 'runtime.wasm' | 'comparison.json';
    filename: string;
    sizeBytes: number;
    sha256: string;
}

export interface JobResultResponse {
    id: string;
    status: JobStatus;
    /** Persisted worker mode; null for legacy rows. */
    mode?: RunMode | null;
    /** Durable, server-derived strength of the result's behavioral claim. */
    claimClass?: ClaimClass | null;
    /** Explicit independent-verifier outcome; null for legacy worker-only rows. */
    verificationOutcome?: string | null;
    /** Control-plane timestamp for the persisted independent verdict. */
    verifiedAt?: string | null;
    /** Independent-verifier facts; null when that verifier did not run. */
    compileOk?: boolean | null;
    assumptionsClosed?: boolean | null;
    verifiedClean?: boolean | null;
    admitsDetected?: boolean | null;
    statementsImmutable?: boolean | null;
    markersRemaining?: number | null;
    holesTotal: number;
    holesClosed: number;
    /** Versioned reviewed policy used for Print Assumptions. */
    assumptionPolicyId?: string | null;
    /** Per-closed-target kernel dependency evidence; null for legacy rows. */
    assumptionReports?: AssumptionReport[] | null;
    artifacts: ArtifactInfo[];
    /** JSON-encoded WorkerImageManifest captured from the worker, if reported. */
    imageManifestJson?: string | null;
    /** Null/absent unless exact input bytes and the frozen proof qualify. */
    artifactBinding?: ArtifactBindingResponse | null;
}

/**
 * Safe fields projected from a worker report's IMAGE_MANIFEST.json.
 * Optional agent fields describe installed worker contents; they are not an
 * independent trusted-verifier attestation and remain absent for legacy rows.
 */
export interface WorkerImageManifest {
    coq: string;
    libraryCommit: string;
    coqWasm: string;
    coqWasmCommit: string;
    infcCommit: string;
    harness: string;
    agentProtocolVersion?: 2;
    agentProtocolPolicy?: string;
    agentProtocolSha256?: string;
    agentCatalogSha256?: string;
    agentToolchainSha256?: string;
    claudeCode?: string;
    codexCli?: string;
    node?: string;
    [key: string]: unknown;
}

export interface KernelAssumption {
    name: string;
    type: string;
    kind: 'axiom' | 'sectionVariable';
}

export interface AssumptionReport {
    target: string;
    assumptions: KernelAssumption[];
    kernelOutput: string;
    policyPassed: boolean;
}

/** Response for `POST /api/v1/jobs/{id}/cancel`. */
export interface CancelResponse {
    status: string;
}

/**
 * Event envelope as delivered by SSE (`/stream`) and polling (`/events`).
 *
 * `seq` is the gap-free server-assigned sequence; `vmSeq` is the worker's local
 * counter (null for server-originated events). `payload` shape depends on
 * `type` (see CONTRACTS.md "Event types").
 */
export interface EventEnvelope<P = unknown> {
    schemaVersion: number;
    jobId: string;
    seq: number;
    vmSeq?: number | null;
    type: string;
    ts: string;
    phase?: string | null;
    level?: string | null;
    payload: P;
}

/**
 * Response shape for the polling endpoint `GET /api/v1/jobs/{id}/events`.
 * The server returns `{ events }` only — the caller tracks its own cursor
 * from the highest `seq` it has seen.
 */
export interface EventsResponse {
    events: EventEnvelope[];
}

/** Agent settings pinned at admission (`JobResponse.agentExecution`). */
export interface PublicAgentExecution {
    settingsRevision: number;
    agentRef: string;
    model: string | null;
}

/** The infc identity a deployment's worker accepts (`MetaResponse.acceptedToolchain`). */
export interface AcceptedToolchain {
    repository: string;
    /** Full 40-hex commit; compare a local `infc --commit-hash` by prefix. */
    commit: string;
    /** Exact `infc --version` output, e.g. `infc 0.0.6`. */
    version: string;
    /** Exact `infc --abi-version` output, e.g. `1.8`. */
    abiVersion: string;
    /** Published release tag when the accepted commit is a release, else null. */
    releaseTag: string | null;
}

/** Response for `GET /api/v1/meta`. */
export interface MetaResponse {
    schemaVersion: number;
    maxUploadBytes: number;
    defaultMaxWallClockSeconds: number;
    maxWallClockSecondsCap: number;
    providers?: string[];
    executionMode?: 'standard' | 'local-deterministic';
    /** Absent on servers that predate it; clients then skip the identity check. */
    acceptedToolchain?: AcceptedToolchain | null;
}

/** Response for `GET /api/v1/me`: who the API key belongs to. */
export interface MeResponse {
    schemaVersion: number;
    userId: string;
    role: string;
    keyId?: string | null;
}

/** Response for `GET /api/v1/jobs/{id}/artifacts`. */
export interface ArtifactListResponse {
    artifacts: ArtifactInfo[];
}

/** Query for `GET /api/v1/jobs`: newest first, keyset-paged by `createdAt`. */
export interface ListJobsQuery {
    status?: JobStatus;
    /** 1–100; the server defaults to 50. */
    limit?: number;
    /** RFC 3339; only jobs created strictly before it. */
    before?: string;
}
