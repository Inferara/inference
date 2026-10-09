import * as crypto from 'crypto';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';

import { ApiError, ProverApi, buildUrl } from './api';
import { firstBuildError, type BuildError } from './buildLog';
import { describeError, resolveConfig, type ProverConfig } from './config';
import { cancelJobInteractive, compareProof, openInPortal } from './jobActions';
import { buildJobView, type LiveMode } from './jobDetailModel';
import { REGION_IDS, renderRegions, renderShell, type Regions } from './jobDetailHtml';
import { JobOrigins, type JobOrigin } from './jobOrigins';
import { isStreamTerminalStatus, isTextArtifactKind } from './jobStatus';
import type { ResultState } from './jobVerdict';
import { fetchVerified, safeArtifactName, showAtLine, type ProofDocuments } from './proofDocuments';
import { applyEvent, changesSummary, EVENT_LOG_CAP, newRunModel, type ActivityRow, type RunModel } from './runModel';
import { openEventStream, type SseMessage } from './sse';
import type { ArtifactInfo, EventEnvelope, JobResponse, JobResultResponse } from './types';

/** The panel's webview type, also used to restore panels after a reload. */
export const JOB_PANEL_VIEW_TYPE = 'inference.proofJob';

/** Poll cadence when the SSE stream is unavailable. */
const POLL_MS = 5_000;
/** Safety-net poll cadence while the SSE stream is live (covers reaper-made
 *  transitions that emit no event; the stream's `end` frame covers terminal). */
const SLOW_POLL_MS = 30_000;
/** Debounce for event-driven detail re-fetches. */
const REFRESH_DEBOUNCE_MS = 1_200;
/** Event types that never change the job detail. Agent activity is
 *  high-volume transcript; re-fetching the job per line would flood the server. */
const DETAIL_NEUTRAL_TYPES: ReadonlySet<string> = new Set([
    'heartbeat',
    'usage.tick',
    'log',
    'agent.activity',
]);
/** The events endpoint returns at most this many per call. */
const EVENTS_PAGE = 500;
/** Bound on history pages fetched for a finished job. */
const MAX_HISTORY_PAGES = 40;

/** What the webview saves (`vscode.setState`) and a restored panel gets back. */
interface SavedPanelState {
    jobId?: unknown;
}

interface PanelState {
    panel: vscode.WebviewPanel;
    job: JobResponse;
    origin?: JobOrigin;
    /** Fetch-error banner (null = clean). */
    error: string | null;
    run: RunModel;
    /** Current live-update transport. */
    liveMode: 'sse' | 'poll';
    stream?: { dispose(): void };
    pollTimer?: NodeJS.Timeout;
    refreshDebounce?: NodeJS.Timeout;
    /** A detail fetch is in flight — coalesces timer/reveal/Refresh races. */
    inflight: boolean;
    /** A load was requested while one was in flight — re-run when it ends. */
    reloadRequested: boolean;
    /** The job detail (not just a list row) has been fetched once. */
    detailLoaded: boolean;
    /** False for a restored panel until its first fetch. */
    statusKnown: boolean;
    result: JobResultResponse | null;
    resultState: ResultState;
    /** The finished job's remaining event history has been read. */
    historyFetched: boolean;
    buildError: BuildError | null;
    buildLogFetched: boolean;
    /** The webview script has said `ready` since it last (re)loaded. */
    ready: boolean;
    /** Regions last sent to the webview. */
    sent: Partial<Regions>;
}

/**
 * Manages the "Proof Job" webview panels — one per job, reused on re-open.
 *
 * The page is set once; afterwards the host sends region patches and
 * activity rows, so focus, open sections and the activity filter survive
 * every update. Live updates are SSE-first: while the panel is visible and
 * the job is non-terminal, an event stream feeds the activity list and
 * triggers debounced detail re-fetches; a slow safety poll runs alongside.
 * When the stream is down the view falls back to 5 s polling plus
 * `/events?after=` catch-up. Hidden panels release their stream and timers;
 * when the webview reloads (reveal, window reload) it asks for a snapshot.
 */
export class JobDetailViewManager implements vscode.Disposable, vscode.WebviewPanelSerializer {
    private readonly states = new Map<string, PanelState>();

    constructor(
        private readonly extensionUri: vscode.Uri,
        private readonly secrets: vscode.SecretStorage,
        private readonly log: vscode.LogOutputChannel,
        private readonly documents: ProofDocuments,
        private readonly origins: JobOrigins,
        /** Called after an action here changed a job (the tree refreshes). */
        private readonly onJobChanged: () => void,
        /** Run a finished job again (the caller opens the new job). */
        private readonly onRunAgain: (job: JobResponse) => Promise<void>,
    ) {}

    /** Close every panel: the server or the account changed. */
    closeAll(): void {
        for (const state of [...this.states.values()]) {
            state.panel.dispose(); // onDidDispose suspends and forgets it
        }
    }

    /** Open (or reveal) the detail panel for `job`, then fetch fresh data. */
    async open(job: JobResponse, origin?: JobOrigin): Promise<void> {
        const existing = this.states.get(job.id);
        if (existing) {
            existing.origin = origin ?? existing.origin;
            existing.panel.reveal();
            await this.load(job.id);
            return;
        }
        const panel = vscode.window.createWebviewPanel(
            JOB_PANEL_VIEW_TYPE,
            this.title(job, origin),
            vscode.ViewColumn.Active,
            this.webviewOptions(),
        );
        await this.adopt(panel, job, origin);
    }

    /** Restore a panel VS Code kept across a window reload. */
    async deserializeWebviewPanel(panel: vscode.WebviewPanel, saved: unknown): Promise<void> {
        const jobId = (saved as SavedPanelState | undefined)?.jobId;
        if (typeof jobId !== 'string' || !jobId || this.states.has(jobId)) {
            panel.dispose();
            return;
        }
        panel.webview.options = this.webviewOptions();
        await this.adopt(panel, { id: jobId, status: 'Accepted' }, undefined, false);
    }

    private webviewOptions(): vscode.WebviewOptions & vscode.WebviewPanelOptions {
        // The page is self-contained: inline styles, inline SVG, one nonce'd
        // script. No local resources are loaded.
        return { enableScripts: true, localResourceRoots: [] };
    }

    private async adopt(
        panel: vscode.WebviewPanel,
        job: JobResponse,
        origin?: JobOrigin,
        known = true,
    ): Promise<void> {
        const id = job.id;
        const state: PanelState = {
            panel,
            job,
            origin,
            error: null,
            run: newRunModel(),
            liveMode: 'poll',
            inflight: false,
            reloadRequested: false,
            detailLoaded: false,
            statusKnown: known,
            result: null,
            resultState: 'n/a',
            historyFetched: false,
            buildError: null,
            buildLogFetched: false,
            ready: false,
            sent: {},
        };
        this.states.set(id, state);
        panel.iconPath = vscode.Uri.joinPath(this.extensionUri, 'icons', 'file_icon.svg');
        if (!origin) {
            const config = await this.config();
            state.origin = config ? this.origins.get(config.serverUrl, id) : undefined;
        }
        panel.title = this.title(job, state.origin);

        panel.onDidDispose(() => {
            this.suspend(id);
            this.states.delete(id);
        });
        panel.webview.onDidReceiveMessage((message: unknown) => this.onMessage(id, message));
        // Hidden panels hold no stream or timers; the webview reloads on reveal
        // and asks for a snapshot (`ready`).
        panel.onDidChangeViewState((e) => {
            const fresh = this.states.get(id);
            if (!fresh) {
                return;
            }
            if (e.webviewPanel.visible) {
                this.ensureLive(id);
                void this.load(id);
            } else {
                fresh.ready = false;
                this.suspend(id);
            }
        });

        panel.webview.html = renderShell({
            nonce: crypto.randomBytes(16).toString('hex'),
            cspSource: panel.webview.cspSource,
        });
        if (known) {
            this.ensureLive(id);
        }
        await this.load(id);
    }

    private title(job: JobResponse, origin?: JobOrigin): string {
        return `Proof: ${JobOrigins.displayName(origin) ?? job.filename ?? job.id}`;
    }

    /** Resolved config, or null (missing key or invalid URL) — never throws. */
    private async config(): Promise<ProverConfig | null> {
        try {
            return await resolveConfig(this.secrets);
        } catch {
            return null;
        }
    }

    // ── Webview messages ────────────────────────────────────────────────────

    private onMessage(id: string, raw: unknown): void {
        const state = this.states.get(id);
        const message = (raw ?? {}) as { command?: unknown; artifactId?: unknown; line?: unknown };
        if (!state || typeof message.command !== 'string') {
            return;
        }
        const artifactId = typeof message.artifactId === 'string' ? message.artifactId : undefined;
        const line = typeof message.line === 'number' && Number.isInteger(message.line) ? message.line : undefined;
        switch (message.command) {
            case 'ready':
                state.ready = true;
                this.sendSnapshot(id);
                break;
            case 'refresh':
                void this.load(id);
                break;
            case 'cancel':
                void this.cancel(id);
                break;
            case 'downloadArtifact':
                if (artifactId) void this.downloadArtifact(id, artifactId);
                break;
            case 'openArtifact':
                if (artifactId) void this.openArtifact(id, artifactId);
                break;
            case 'gotoLine':
                if (line !== undefined) void this.gotoLine(id, line, artifactId);
                break;
            case 'compare':
                void compareProof(this.secrets, this.log, this.documents, state.job);
                break;
            case 'openCertificate':
                void openInPortal(this.secrets, state.job, 'certificate');
                break;
            case 'openPortal':
                void openInPortal(this.secrets, state.job, undefined);
                break;
            case 'copyJobId':
                void vscode.env.clipboard.writeText(id).then(() =>
                    vscode.window.setStatusBarMessage(`Copied job ID ${id}`, 3000),
                );
                break;
            case 'resubmit':
                void this.onRunAgain(state.job);
                break;
        }
    }

    // ── Live stream ─────────────────────────────────────────────────────────

    /** Start the SSE stream if the panel is visible and the job can move. */
    private ensureLive(id: string): void {
        const state = this.states.get(id);
        if (
            !state ||
            state.stream ||
            !state.panel.visible ||
            isStreamTerminalStatus(state.job.status)
        ) {
            return;
        }
        void this.config().then((config) => {
            const fresh = this.states.get(id);
            if (!config || !fresh || fresh.stream || !fresh.panel.visible) {
                return;
            }
            fresh.stream = openEventStream({
                url: buildUrl(
                    config.serverUrl,
                    `/api/v1/jobs/${encodeURIComponent(id)}/stream`,
                ),
                apiKey: config.apiKey,
                // A fresh panel connects from seq 0 so the durable history
                // replays into the activity list (bounded — 200/batch on the
                // server, EVENT_LOG_CAP rows kept). Hide/reveal resumes from
                // the cursor whose events the model already holds.
                lastEventId:
                    fresh.run.lastSeq > 0 ? String(fresh.run.lastSeq) : undefined,
                onMessage: (msg) => this.onStreamMessage(id, msg),
                onStateChange: (s, detail) => {
                    const st = this.states.get(id);
                    if (!st) {
                        return;
                    }
                    if (s === 'live') {
                        this.setLiveMode(id, 'sse');
                    } else if (s === 'retrying' || s === 'failed') {
                        if (detail && s === 'failed') {
                            this.log.warn(`Job ${id} stream failed: ${detail}`);
                        }
                        this.setLiveMode(id, 'poll');
                    }
                },
            });
        });
    }

    private onStreamMessage(id: string, msg: SseMessage): void {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        if (msg.event === 'end') {
            // Terminal: drop the stream and fetch the final detail + result.
            state.stream?.dispose();
            state.stream = undefined;
            void this.load(id);
            return;
        }
        let envelope: EventEnvelope;
        try {
            envelope = JSON.parse(msg.data) as EventEnvelope;
        } catch {
            this.log.warn(`Job ${id}: unparseable SSE frame (${msg.event})`);
            return;
        }
        this.ingest(id, [envelope]);
        if (!DETAIL_NEUTRAL_TYPES.has(envelope.type)) {
            this.scheduleDetailRefresh(id);
        }
    }

    /**
     * Apply events to the run model (it dedupes replays: the stream and the
     * poll fallback overlap), send the new activity rows, and re-render the
     * summary when an event can change it.
     */
    private ingest(id: string, events: readonly EventEnvelope[]): void {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        const rows: ActivityRow[] = [];
        let summaryChanged = false;
        for (const env of events) {
            const { applied, row } = applyEvent(state.run, env);
            if (!applied) {
                continue;
            }
            if (row) {
                rows.push(row);
            }
            summaryChanged ||= changesSummary(env.type);
        }
        if (rows.length > 0 && state.ready) {
            void state.panel.webview.postMessage({ command: 'activity', rows });
        }
        if (summaryChanged) {
            this.render(id);
        }
    }

    private setLiveMode(id: string, mode: 'sse' | 'poll'): void {
        const state = this.states.get(id);
        if (!state || state.liveMode === mode) {
            return;
        }
        state.liveMode = mode;
        this.render(id);
        this.schedule(id); // re-arm the poll at the cadence the mode implies
    }

    /** Debounced detail re-fetch driven by stream events. */
    private scheduleDetailRefresh(id: string): void {
        const state = this.states.get(id);
        if (!state || state.refreshDebounce) {
            return;
        }
        state.refreshDebounce = setTimeout(() => {
            const fresh = this.states.get(id);
            if (fresh) {
                fresh.refreshDebounce = undefined;
            }
            void this.load(id);
        }, REFRESH_DEBOUNCE_MS);
    }

    // ── Detail fetch / poll ─────────────────────────────────────────────────

    /** Fetch fresh detail and re-render; on failure keep stale data + banner. */
    private async load(id: string): Promise<void> {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        if (state.inflight) {
            // Latch instead of dropping: an `end` frame (or Refresh) arriving
            // mid-fetch must still produce a post-fetch reload, or the final
            // verdict waits for the 30 s safety poll.
            state.reloadRequested = true;
            return;
        }
        state.inflight = true;
        try {
            const config = await resolveConfig(this.secrets);
            if (!config) {
                this.renderError(id, 'The proof server API key is not set. Run “Inference: Set Proof Server API Key”.');
                return;
            }
            const api = new ProverApi(config.serverUrl, config.apiKey);
            const fresh = await api.getJob(id);
            const current = this.states.get(id);
            if (!current) {
                return; // panel closed while the fetch was in flight
            }
            const wasKnown = current.detailLoaded;
            current.job = fresh;
            current.detailLoaded = true;
            current.statusKnown = true;
            current.error = null;
            current.panel.title = this.title(fresh, current.origin);
            if (!wasKnown) {
                this.ensureLive(id); // a restored panel learns its status here
            }

            // Stream-terminal, not display-terminal: on `Lost` the server
            // keeps the stream open and may re-queue the job — keep ours too.
            if (isStreamTerminalStatus(fresh.status)) {
                current.stream?.dispose();
                current.stream = undefined;
                if (current.resultState !== 'loaded') {
                    current.resultState = 'loading';
                }
                this.render(id);
                await this.fetchResultOnce(id, api);
                await this.catchUpHistoryOnce(id, api);
                await this.fetchBuildErrorOnce(id, api);
            } else if (current.liveMode !== 'sse') {
                await this.pollEvents(id, api);
            }
            const changed = this.render(id);
            if (!changed && state.ready) {
                void state.panel.webview.postMessage({
                    command: 'checked',
                    at: new Date().toLocaleTimeString(),
                });
            }
        } catch (err) {
            const message =
                err instanceof ApiError && err.status === 404
                    ? 'Job not found on the server. It may have been deleted or removed by retention.'
                    : describeError(err);
            this.log.error(
                `Proof job detail (${id}): ${err instanceof Error ? err.message : err}`,
            );
            this.renderError(id, message);
        } finally {
            const fresh = this.states.get(id);
            if (fresh) {
                fresh.inflight = false;
                if (fresh.reloadRequested) {
                    fresh.reloadRequested = false;
                    void this.load(id); // schedules in its own finally
                } else {
                    this.schedule(id);
                }
            }
        }
    }

    /** Fetch result metadata + artifacts once (terminal jobs only). */
    private async fetchResultOnce(id: string, api: ProverApi): Promise<void> {
        const state = this.states.get(id);
        if (!state || state.resultState === 'loaded') {
            return;
        }
        try {
            const result = await api.getResult(id);
            const fresh = this.states.get(id);
            if (fresh) {
                fresh.result = result;
                fresh.resultState = 'loaded';
            }
        } catch (err) {
            // 409 NOT_TERMINAL = raced a transition; retry on next refresh.
            const fresh = this.states.get(id);
            if (fresh && !(err instanceof ApiError && err.status === 409)) {
                fresh.resultState = 'failed';
                this.log.warn(`Job ${id} result: ${err instanceof Error ? err.message : err}`);
            }
        }
    }

    /**
     * Once a job has finished, read every event past the model's cursor: the
     * whole history for a panel opened afterwards, or the tail a polling panel
     * had not fetched yet. Reads until a short page (bounded).
     */
    private async catchUpHistoryOnce(id: string, api: ProverApi): Promise<void> {
        const state = this.states.get(id);
        if (!state || state.historyFetched) {
            return;
        }
        try {
            const events: EventEnvelope[] = [];
            let cursor = state.run.lastSeq;
            for (let page = 0; page < MAX_HISTORY_PAGES; page++) {
                const res = await api.getEvents(id, cursor);
                for (const env of res.events) {
                    events.push(env);
                    if (typeof env.seq === 'number' && env.seq > cursor) {
                        cursor = env.seq;
                    }
                }
                if (res.events.length < EVENTS_PAGE) {
                    break;
                }
            }
            const fresh = this.states.get(id);
            if (!fresh) {
                return;
            }
            fresh.historyFetched = true;
            const before = fresh.run.rows.length;
            for (const env of events) {
                applyEvent(fresh.run, env);
            }
            if (fresh.ready && fresh.run.rows.length !== before) {
                void fresh.panel.webview.postMessage({ command: 'activity', rows: fresh.run.rows, reset: true });
            }
        } catch (err) {
            this.log.warn(`Job ${id} event history: ${err instanceof Error ? err.message : err}`);
        }
    }

    /** For a compile failure, read the build log once and keep its first error. */
    private async fetchBuildErrorOnce(id: string, api: ProverApi): Promise<void> {
        const state = this.states.get(id);
        const code = state?.job.errorCode;
        if (!state || state.buildLogFetched || (code !== 'compile_failed' && code !== 'completed_compile_failed')) {
            return;
        }
        const info = state.result?.artifacts.find((a) => a.kind === 'BuildLog');
        if (!info) {
            return;
        }
        state.buildLogFetched = true;
        try {
            const text = await this.documents.text(api, id, info);
            const fresh = this.states.get(id);
            if (fresh) {
                fresh.buildError = firstBuildError(text);
            }
        } catch (err) {
            this.log.warn(`Job ${id} build log: ${err instanceof Error ? err.message : err}`);
        }
    }

    /** Poll-mode event catch-up: replay anything past the cursor. */
    private async pollEvents(id: string, api: ProverApi): Promise<void> {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        try {
            const res = await api.getEvents(id, state.run.lastSeq);
            this.ingest(id, res.events);
        } catch (err) {
            this.log.warn(`Job ${id} event poll: ${err instanceof Error ? err.message : err}`);
        }
    }

    private schedule(id: string): void {
        this.clearPollTimer(id);
        const state = this.states.get(id);
        if (
            !state ||
            !state.panel.visible ||
            (state.detailLoaded && isStreamTerminalStatus(state.job.status))
        ) {
            return;
        }
        const delay = state.liveMode === 'sse' ? SLOW_POLL_MS : POLL_MS;
        state.pollTimer = setTimeout(() => void this.load(id), delay);
    }

    private clearPollTimer(id: string): void {
        const state = this.states.get(id);
        if (state?.pollTimer) {
            clearTimeout(state.pollTimer);
            state.pollTimer = undefined;
        }
    }

    /** Release the stream and all timers (panel hidden or disposed). */
    private suspend(id: string): void {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        state.stream?.dispose();
        state.stream = undefined;
        if (state.refreshDebounce) {
            clearTimeout(state.refreshDebounce);
            state.refreshDebounce = undefined;
        }
        this.clearPollTimer(id);
    }

    // ── User actions ────────────────────────────────────────────────────────

    private async cancel(id: string): Promise<void> {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        const acted = await cancelJobInteractive(this.secrets, this.log, {
            id,
            filename: JobOrigins.displayName(state.origin) ?? state.job.filename,
        });
        if (acted) {
            this.onJobChanged();
            await this.load(id);
        }
    }

    /** The artifact list: the result's when loaded, else fetched. */
    private async artifacts(id: string, api: ProverApi): Promise<ArtifactInfo[]> {
        return this.states.get(id)?.result?.artifacts ?? (await api.listArtifacts(id));
    }

    private async downloadArtifact(id: string, artifactId: string): Promise<void> {
        const config = await this.config();
        if (!config) {
            return;
        }
        const api = new ProverApi(config.serverUrl, config.apiKey);
        try {
            // The artifact id from the webview is only a lookup key into the
            // server-provided list.
            const info = (await this.artifacts(id, api)).find((a) => a.id === artifactId);
            if (!info) {
                return;
            }
            const content = await vscode.window.withProgress(
                {
                    location: vscode.ProgressLocation.Notification,
                    title: `Downloading ${info.filename}…`,
                },
                () => fetchVerified(api, id, info),
            );
            const target = await vscode.window.showSaveDialog({
                defaultUri: vscode.Uri.joinPath(
                    vscode.workspace.workspaceFolders?.[0]?.uri ??
                        vscode.Uri.file(os.homedir()),
                    safeArtifactName(info.filename),
                ),
            });
            if (!target) {
                return;
            }
            await vscode.workspace.fs.writeFile(target, content);
            const openable = /\.(v|json|log|txt|md)$/i.test(target.path);
            const action = await vscode.window.showInformationMessage(
                `Inference: saved ${path.basename(target.fsPath)}.`,
                ...(openable ? ['Open File'] : []),
            );
            if (action === 'Open File') {
                await vscode.window.showTextDocument(
                    await vscode.workspace.openTextDocument(target),
                );
            }
        } catch (err) {
            this.log.error(
                `Artifact ${artifactId} of job ${id}: ${err instanceof Error ? err.message : err}`,
            );
            vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
        }
    }

    /** Show a text artifact as a read-only document (SHA-256-checked). */
    private async openArtifact(id: string, artifactId: string, line?: number): Promise<void> {
        const config = await this.config();
        if (!config) {
            return;
        }
        const api = new ProverApi(config.serverUrl, config.apiKey);
        try {
            const info = (await this.artifacts(id, api)).find((a) => a.id === artifactId);
            if (!info || !isTextArtifactKind(info.kind)) {
                return;
            }
            await this.documents.open(api, id, info, line);
        } catch (err) {
            this.log.error(
                `Artifact ${artifactId} of job ${id}: ${err instanceof Error ? err.message : err}`,
            );
            vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
        }
    }

    /**
     * Jump to a line of the submitted file: the local `.v` when it is still
     * byte-identical to the upload, else the uploaded copy (read-only). An
     * explicit artifact (e.g. the returned proof) opens that one instead.
     */
    private async gotoLine(id: string, line: number, artifactId?: string): Promise<void> {
        const state = this.states.get(id);
        const config = await this.config();
        if (!state || !config) {
            return;
        }
        const api = new ProverApi(config.serverUrl, config.apiKey);
        try {
            const artifacts = await this.artifacts(id, api);
            const target = artifactId
                ? artifacts.find((a) => a.id === artifactId)
                : artifacts.find((a) => a.kind === 'InputV');
            if (target?.kind === 'InputV' && state.origin) {
                const local = vscode.Uri.file(state.origin.vPath);
                try {
                    const bytes = await vscode.workspace.fs.readFile(local);
                    const digest = crypto.createHash('sha256').update(bytes).digest('hex');
                    if (digest === target.sha256.toLowerCase()) {
                        await showAtLine(await vscode.workspace.openTextDocument(local), line);
                        return;
                    }
                } catch {
                    // The local file is gone; fall back to the uploaded copy.
                }
            }
            if (target) {
                await this.openArtifact(id, target.id, line);
            }
        } catch (err) {
            this.log.error(`Job ${id} go to line ${line}: ${err instanceof Error ? err.message : err}`);
            vscode.window.showErrorMessage(`Inference: ${describeError(err)}`);
        }
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    private renderError(id: string, message: string): void {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        state.error = message;
        this.render(id);
    }

    private liveMode(state: PanelState): LiveMode {
        return state.detailLoaded && isStreamTerminalStatus(state.job.status) ? 'terminal' : state.liveMode;
    }

    private regions(state: PanelState): Regions {
        const source = state.origin?.infPath ? path.basename(state.origin.infPath) : null;
        return renderRegions(
            buildJobView({
                job: state.job,
                result: state.result,
                resultState: state.resultState,
                detailLoaded: state.detailLoaded,
                statusKnown: state.statusKnown,
                run: state.run,
                buildError: state.buildError,
                source,
                error: state.error,
                live: this.liveMode(state),
            }),
        );
    }

    private clock(state: PanelState): unknown {
        const view = buildJobView({
            job: state.job,
            detailLoaded: state.detailLoaded,
            run: state.run,
            live: this.liveMode(state),
        });
        return view.clock;
    }

    /** Send the regions that changed; returns whether anything did. */
    private render(id: string): boolean {
        const state = this.states.get(id);
        if (!state || !state.ready) {
            return false;
        }
        const regions = this.regions(state);
        const changed: Partial<Regions> = {};
        for (const key of REGION_IDS) {
            if (state.sent[key] !== regions[key]) {
                changed[key] = regions[key];
            }
        }
        if (Object.keys(changed).length === 0) {
            return false;
        }
        state.sent = { ...state.sent, ...changed };
        void state.panel.webview.postMessage({ command: 'regions', regions: changed });
        void state.panel.webview.postMessage({ command: 'clock', clock: this.clock(state) });
        return true;
    }

    /** Full state for a webview that (re)loaded. */
    private sendSnapshot(id: string): void {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        const regions = this.regions(state);
        state.sent = regions;
        void state.panel.webview.postMessage({
            command: 'snapshot',
            jobId: id,
            regions,
            rows: state.run.rows.slice(-EVENT_LOG_CAP),
            clock: this.clock(state),
        });
    }

    dispose(): void {
        for (const id of [...this.states.keys()]) {
            this.suspend(id);
        }
        for (const state of this.states.values()) {
            state.panel.dispose();
        }
        this.states.clear();
    }
}
