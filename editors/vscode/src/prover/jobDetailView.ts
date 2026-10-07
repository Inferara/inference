import * as crypto from 'crypto';
import * as os from 'os';
import * as vscode from 'vscode';

import { ApiError, ProverApi, buildUrl } from './api';
import { describeError, resolveConfig, type ProverConfig } from './config';
import { formatEventLine } from './eventFormat';
import { cancelJobInteractive, compareProof, openInPortal } from './jobActions';
import {
    EVENT_LOG_CAP,
    isCancelableStatus,
    isStreamTerminalStatus,
    isTextArtifactKind,
    renderJobDetailHtml,
    type LiveMode,
} from './jobDetailHtml';
import { fetchVerified, safeArtifactName, type ProofDocuments } from './proofDocuments';
import { openEventStream, type SseMessage } from './sse';
import type {
    EventEnvelope,
    JobResponse,
    JobResultResponse,
} from './types';

/** Poll cadence when the SSE stream is unavailable. */
const POLL_MS = 5_000;
/** Safety-net poll cadence while the SSE stream is live (covers reaper-made
 *  transitions that emit no event; the stream's `end` frame covers terminal). */
const SLOW_POLL_MS = 30_000;
/** Debounce for event-driven detail re-fetches. */
const REFRESH_DEBOUNCE_MS = 1_200;
/** Event types that never change the rendered job detail. Agent activity is
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

interface PanelState {
    panel: vscode.WebviewPanel;
    job: JobResponse;
    /** Source `.inf` name when this extension compiled the upload. */
    source: string | null;
    /** Last rendered error banner (null = clean render). */
    error: string | null;
    /** Formatted event-log lines, oldest first, capped. */
    log: string[];
    /** Highest event `seq` seen (SSE id / envelope seq / polled events). */
    lastSeq: number;
    /** Current live-update transport. */
    liveMode: 'sse' | 'poll';
    stream?: { dispose(): void };
    pollTimer?: NodeJS.Timeout;
    refreshDebounce?: NodeJS.Timeout;
    /** A detail fetch is in flight — coalesces timer/reveal/Refresh races. */
    inflight: boolean;
    /** A load was requested while one was in flight — re-run when it ends
     *  (e.g. the stream's `end` frame racing an event-debounced fetch). */
    reloadRequested: boolean;
    /** Result metadata + artifacts, fetched once the job is terminal. */
    result: JobResultResponse | null;
    resultFetched: boolean;
    /** The finished job's remaining event history has been read. */
    historyFetched: boolean;
}

/**
 * Manages the "Proof Job" webview panels — one per job, reused on re-open.
 *
 * Live updates are SSE-first: while the panel is visible and the job is
 * non-terminal, an event stream (`/api/v1/jobs/{id}/stream`) feeds the event
 * log incrementally and triggers debounced detail re-fetches; a slow safety
 * poll runs alongside. When the stream is down the view falls back to 5 s
 * polling plus `/events?after=` catch-up, resuming the stream with
 * `Last-Event-ID` once it reconnects — the server replays missed events
 * gap-free from its durable log. Hidden panels release their stream and
 * timers; revealing re-attaches from the saved cursor.
 */
export class JobDetailViewManager implements vscode.Disposable {
    private readonly states = new Map<string, PanelState>();

    constructor(
        private readonly secrets: vscode.SecretStorage,
        private readonly log: vscode.LogOutputChannel,
        private readonly documents: ProofDocuments,
        /** Called after an action here changed a job (the tree refreshes). */
        private readonly onJobChanged: () => void,
    ) {}

    /** Close every panel: the server or the account changed. */
    closeAll(): void {
        for (const state of [...this.states.values()]) {
            state.panel.dispose(); // onDidDispose suspends and forgets it
        }
    }

    /** Open (or reveal) the detail panel for `job`, then fetch fresh data. */
    async open(job: JobResponse, source?: string): Promise<void> {
        const existing = this.states.get(job.id);
        if (existing) {
            existing.source = source ?? existing.source;
            existing.panel.reveal();
            await this.load(job.id);
            return;
        }

        const panel = vscode.window.createWebviewPanel(
            'inference.proofJob',
            this.title(job),
            vscode.ViewColumn.Active,
            {
                enableScripts: true,
                // The view is fully self-contained: no local resources at all.
                localResourceRoots: [],
            },
        );
        const state: PanelState = {
            panel,
            job,
            source: source ?? null,
            error: null,
            log: [],
            lastSeq: 0,
            liveMode: 'poll',
            inflight: false,
            reloadRequested: false,
            result: null,
            resultFetched: false,
            historyFetched: false,
        };
        this.states.set(job.id, state);

        panel.onDidDispose(() => {
            this.suspend(job.id);
            this.states.delete(job.id);
        });
        panel.webview.onDidReceiveMessage(
            (message: { command?: string; artifactId?: string }) => {
                switch (message?.command) {
                    case 'refresh':
                        void this.load(job.id);
                        break;
                    case 'cancel':
                        void this.cancel(job.id);
                        break;
                    case 'downloadArtifact':
                        if (typeof message.artifactId === 'string') {
                            void this.downloadArtifact(
                                job.id,
                                message.artifactId,
                            );
                        }
                        break;
                    case 'openArtifact':
                        if (typeof message.artifactId === 'string') {
                            void this.openArtifact(job.id, message.artifactId);
                        }
                        break;
                    case 'compare':
                        void compareProof(this.secrets, this.log, this.documents, job);
                        break;
                    case 'openCertificate':
                        void openInPortal(this.secrets, job);
                        break;
                }
            },
        );
        // Hidden panels hold no stream or timers; reveal re-attaches and the
        // stream resumes from the saved cursor (missed events are replayed).
        panel.onDidChangeViewState((e) => {
            if (e.webviewPanel.visible) {
                this.render(job.id);
                this.ensureLive(job.id);
                void this.load(job.id);
            } else {
                this.suspend(job.id);
            }
        });

        this.render(job.id);
        this.ensureLive(job.id);
        await this.load(job.id);
    }

    private title(job: JobResponse): string {
        return `Proof: ${job.filename ?? job.id}`;
    }

    /** Resolved config, or null (missing key or invalid URL) — never throws. */
    private async config(): Promise<ProverConfig | null> {
        try {
            return await resolveConfig(this.secrets);
        } catch {
            return null;
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
                // Deliberately not seeded from JobResponse.lastEventSeq: a
                // fresh panel connects from seq 0 so the durable history
                // replays into the event log (bounded — 200/batch on the
                // server, EVENT_LOG_CAP lines kept). Hide/reveal resumes
                // from the cursor whose events the log already holds.
                lastEventId:
                    fresh.lastSeq > 0 ? String(fresh.lastSeq) : undefined,
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
        // Advance-then-append: state.lastSeq is the single dedupe authority.
        // The stream and the poll fallback can overlap (the SSE client
        // reconnects with its own Last-Event-ID while a poll already moved
        // the cursor) — anything at or below the cursor is a replay.
        if (typeof envelope.seq === 'number') {
            if (envelope.seq <= state.lastSeq) {
                return;
            }
            state.lastSeq = envelope.seq;
        }
        this.appendLogLine(state, formatEventLine(envelope));
        if (!DETAIL_NEUTRAL_TYPES.has(envelope.type)) {
            this.scheduleDetailRefresh(id);
        }
    }

    private appendLogLine(state: PanelState, line: string): void {
        state.log.push(line);
        if (state.log.length > EVENT_LOG_CAP) {
            state.log.splice(0, state.log.length - EVENT_LOG_CAP);
        }
        void state.panel.webview.postMessage({ command: 'event', line });
    }

    private setLiveMode(id: string, mode: 'sse' | 'poll'): void {
        const state = this.states.get(id);
        if (!state || state.liveMode === mode) {
            return;
        }
        state.liveMode = mode;
        const uiMode: LiveMode = isStreamTerminalStatus(state.job.status)
            ? 'terminal'
            : mode;
        void state.panel.webview.postMessage({ command: 'mode', mode: uiMode });
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
                this.renderError(id, 'Proof server API key is not set.');
                return;
            }
            const api = new ProverApi(config.serverUrl, config.apiKey);
            const fresh = await api.getJob(id);
            if (!this.states.get(id)) {
                return; // panel closed while the fetch was in flight
            }

            // Stream-terminal, not display-terminal: on `Lost` the server
            // keeps the stream open and may re-queue the job — keep ours too.
            const terminal = isStreamTerminalStatus(fresh.status);
            let auxChanged = false;
            if (terminal) {
                // No more movement: release the stream, fetch the verdict and
                // (when opened post-mortem) backfill the event history.
                state.stream?.dispose();
                state.stream = undefined;
                auxChanged = await this.fetchResultOnce(id, api);
                auxChanged =
                    (await this.catchUpHistoryOnce(id, api)) || auxChanged;
            } else if (state.liveMode !== 'sse') {
                await this.pollEvents(id, api);
            }

            const changed =
                JSON.stringify(fresh) !== JSON.stringify(state.job);
            const hadError = state.error !== null;
            state.error = null;
            state.job = fresh;
            if (changed || hadError || auxChanged) {
                this.render(id);
            } else {
                // Acknowledge a no-op refresh without an HTML swap.
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

    /**
     * Fetch result metadata + artifacts once (terminal jobs only).
     * Returns true when new data landed (the caller must re-render).
     */
    private async fetchResultOnce(id: string, api: ProverApi): Promise<boolean> {
        const state = this.states.get(id);
        if (!state || state.resultFetched) {
            return false;
        }
        try {
            state.result = await api.getResult(id);
            state.resultFetched = true;
            return true;
        } catch (err) {
            // 409 NOT_TERMINAL = raced a transition; retry on next refresh.
            if (!(err instanceof ApiError && err.status === 409)) {
                this.log.warn(
                    `Job ${id} result: ${err instanceof Error ? err.message : err}`,
                );
            }
            return false;
        }
    }

    /**
     * Once a job has finished, read every event past the panel's cursor: the
     * whole history for a panel opened afterwards, or the tail a polling
     * panel had not fetched yet when the job ended. Reads until a short page
     * (bounded) and keeps the newest {@link EVENT_LOG_CAP} lines — the tail
     * holds the verdict. Returns true when lines were added.
     */
    private async catchUpHistoryOnce(
        id: string,
        api: ProverApi,
    ): Promise<boolean> {
        const state = this.states.get(id);
        if (!state || state.historyFetched) {
            return false;
        }
        try {
            const lines: string[] = [];
            let cursor = state.lastSeq;
            for (let page = 0; page < MAX_HISTORY_PAGES; page++) {
                const res = await api.getEvents(id, cursor);
                for (const env of res.events) {
                    if (typeof env.seq === 'number') {
                        if (env.seq <= cursor) {
                            continue;
                        }
                        cursor = env.seq;
                    }
                    lines.push(formatEventLine(env));
                }
                if (lines.length > EVENT_LOG_CAP) {
                    lines.splice(0, lines.length - EVENT_LOG_CAP);
                }
                if (res.events.length < EVENTS_PAGE) {
                    break;
                }
            }
            const fresh = this.states.get(id);
            if (!fresh) {
                return false;
            }
            fresh.log.push(...lines);
            if (fresh.log.length > EVENT_LOG_CAP) {
                fresh.log.splice(0, fresh.log.length - EVENT_LOG_CAP);
            }
            fresh.lastSeq = Math.max(fresh.lastSeq, cursor);
            fresh.historyFetched = true;
            return lines.length > 0;
        } catch (err) {
            this.log.warn(
                `Job ${id} event history: ${err instanceof Error ? err.message : err}`,
            );
            return false;
        }
    }

    /** Poll-mode event catch-up: replay anything past the cursor. */
    private async pollEvents(id: string, api: ProverApi): Promise<void> {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        try {
            const res = await api.getEvents(id, state.lastSeq);
            const fresh = this.states.get(id);
            if (!fresh) {
                return;
            }
            for (const env of res.events) {
                // Same advance-then-append dedupe as the stream path: a
                // reconnecting stream may already have shown these.
                if (typeof env.seq === 'number') {
                    if (env.seq <= fresh.lastSeq) {
                        continue;
                    }
                    fresh.lastSeq = env.seq;
                }
                this.appendLogLine(fresh, formatEventLine(env));
            }
        } catch (err) {
            this.log.warn(
                `Job ${id} event poll: ${err instanceof Error ? err.message : err}`,
            );
        }
    }

    private schedule(id: string): void {
        this.clearPollTimer(id);
        const state = this.states.get(id);
        if (
            !state ||
            !state.panel.visible ||
            isStreamTerminalStatus(state.job.status)
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
            filename: state.job.filename,
        });
        if (acted) {
            this.onJobChanged();
            await this.load(id);
        }
    }

    private async downloadArtifact(
        id: string,
        artifactId: string,
    ): Promise<void> {
        const state = this.states.get(id);
        // The artifact id from the webview is only trusted as a lookup key
        // into the server-provided list.
        const info = state?.result?.artifacts.find((a) => a.id === artifactId);
        const config = await this.config();
        if (!state || !info || !config) {
            return;
        }
        try {
            const content = await vscode.window.withProgress(
                {
                    location: vscode.ProgressLocation.Notification,
                    title: `Downloading ${info.filename}…`,
                },
                () => fetchVerified(new ProverApi(config.serverUrl, config.apiKey), id, info),
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
                `Inference: saved ${info.filename}.`,
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
    private async openArtifact(id: string, artifactId: string): Promise<void> {
        const info = this.states.get(id)?.result?.artifacts.find((a) => a.id === artifactId);
        const config = await this.config();
        if (!info || !config || !isTextArtifactKind(info.kind)) {
            return;
        }
        try {
            await this.documents.open(new ProverApi(config.serverUrl, config.apiKey), id, info);
        } catch (err) {
            this.log.error(
                `Artifact ${artifactId} of job ${id}: ${err instanceof Error ? err.message : err}`,
            );
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

    private render(id: string): void {
        const state = this.states.get(id);
        if (!state) {
            return;
        }
        state.panel.title = this.title(state.job);
        const terminal = isStreamTerminalStatus(state.job.status);
        state.panel.webview.html = renderJobDetailHtml(state.job, {
            nonce: crypto.randomBytes(16).toString('hex'),
            cspSource: state.panel.webview.cspSource,
            error: state.error,
            live: terminal ? 'terminal' : state.liveMode,
            eventLog: state.log,
            canCancel: isCancelableStatus(state.job.status),
            result: state.result,
            source: state.source,
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
