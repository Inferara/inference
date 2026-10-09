/**
 * Pure formatting of job event envelopes into single log lines for the
 * detail view. No `vscode` import — unit-tested under plain node.
 *
 * Event types and payload shapes per docs/CONTRACTS.md ("Event types"); the
 * server caps and sanitizes worker-originated strings, but lines are capped
 * here too — render-side HTML escaping happens in jobDetailHtml.
 */

import type { EventEnvelope } from './types';

/** Hard cap on a rendered log line. */
const MAX_LINE_CHARS = 400;
/** Cap on a stringified unknown payload. */
const MAX_PAYLOAD_CHARS = 160;
/** Cap on one agent-activity detail (thinking and tool output are long). */
const MAX_ACTIVITY_CHARS = 240;

/** Server-originated agent limits (`agent.limited.code`), as the portal words them. */
export const AGENT_LIMIT_MESSAGES: Readonly<Record<string, string>> = {
    AGENT_REQUEST_LIMIT:
        'Agent request limit reached. No further model requests are allowed for this job.',
    AGENT_PROVIDER_RATE_LIMITED:
        'The model provider rate-limited a request. Ask the operator to check provider limits.',
    AGENT_CODEX_ALLOWANCE_EXHAUSTED:
        'Codex allowance exhausted. Further attempts use the OpenAI API; API charges apply.',
};

function asRecord(payload: unknown): Record<string, unknown> {
    return typeof payload === 'object' && payload !== null
        ? (payload as Record<string, unknown>)
        : {};
}

function str(v: unknown): string | undefined {
    return typeof v === 'string' && v.length > 0 ? v : undefined;
}

function num(v: unknown): number | undefined {
    return typeof v === 'number' && Number.isFinite(v) ? v : undefined;
}

function fmtMs(ms: number): string {
    if (ms < 1000) {
        return `${ms} ms`;
    }
    const s = ms / 1000;
    return s < 60
        ? `${s.toFixed(1)} s`
        : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`;
}

/** `HH:MM:SS` in local time; falls back to the raw string when unparseable. */
function fmtClock(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) {
        return iso.slice(0, 19);
    }
    const p = (n: number) => String(n).padStart(2, '0');
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function clip(text: string, max: number): string {
    const oneLine = text.replace(/\s+/g, ' ').trim();
    return oneLine.length > max ? `${oneLine.slice(0, max)}…` : oneLine;
}

/**
 * One `agent.activity` payload as a readable line. The kinds mirror the
 * portal's transcript (`portal/src/lib/store.ts::applyActivity`).
 */
export function summarizeActivity(p: Record<string, unknown>): string {
    const kind = str(p.kind) ?? '';
    const detail = str(p.detail) ?? '';
    if (kind === 'truncated') {
        return `activity stream capped${detail ? ` — ${clip(detail, MAX_ACTIVITY_CHARS)}` : ''}`;
    }
    if (kind === 'thinking') {
        return `thinking — ${clip(detail, MAX_ACTIVITY_CHARS)}`;
    }
    if (kind === 'text') {
        return clip(detail, MAX_ACTIVITY_CHARS);
    }
    if (kind.startsWith('tool:')) {
        return `${kind.slice(5)}: ${clip(detail, MAX_ACTIVITY_CHARS)}`;
    }
    if (kind === 'tool_result') {
        return `result: ${detail ? clip(detail, MAX_ACTIVITY_CHARS) : '(empty)'}`;
    }
    if (kind.startsWith('result')) {
        const usage = asRecord(p.usage);
        const cost = num(usage.reportedCostUsd);
        const failed = /is_error=true/i.test(kind);
        return `attempt finished — ${cost !== undefined && cost >= 0 ? `$${cost.toFixed(3)}` : 'cost unknown'}${failed ? ' (agent error)' : ''}`;
    }
    if (kind.startsWith('system/')) {
        return `${kind.slice(7)}${detail ? ` — ${clip(detail, MAX_ACTIVITY_CHARS)}` : ''}`;
    }
    return clip(`${kind} ${detail}`, MAX_ACTIVITY_CHARS);
}

/** `theorem #attempt` scope for an activity line, when the payload names one. */
function activityScope(p: Record<string, unknown>): string {
    const theorem = str(p.theorem);
    const attempt = num(p.attempt);
    if (!theorem) {
        return '';
    }
    return attempt !== undefined ? `[${theorem} #${attempt}] ` : `[${theorem}] `;
}

/** Per-type human summary of an event payload ('' = type alone suffices). */
export function summarize(env: EventEnvelope): string {
    const p = asRecord(env.payload);
    const name = str(p.name);
    const attempt = num(p.attempt);
    const duration = num(p.durationMs);

    switch (env.type) {
        case 'job.accepted': {
            const holes = num(p.holes);
            return [str(p.filename), holes !== undefined ? `${holes} holes` : undefined]
                .filter(Boolean)
                .join(', ');
        }
        case 'job.queued':
            return 'waiting for a worker';
        case 'obligations.discovered': {
            const obligations = p.obligations;
            return Array.isArray(obligations)
                ? `${obligations.length} obligation(s)`
                : '';
        }
        case 'obligation.started':
            return name ?? '';
        case 'obligation.attempt':
            return [name, attempt !== undefined ? `attempt ${attempt}` : undefined]
                .filter(Boolean)
                .join(' — ');
        case 'obligation.proved':
            return [
                name,
                attempt !== undefined ? `attempt ${attempt}` : undefined,
                duration !== undefined ? fmtMs(duration) : undefined,
            ]
                .filter(Boolean)
                .join(' — ');
        case 'obligation.failed':
            return [name, str(p.error)].filter(Boolean).join(' — ');
        case 'obligation.refuted':
            return [name, str(p.error) ?? 'machine-checked refutation accepted']
                .filter(Boolean)
                .join(' — ');
        case 'agent.activity':
            return activityScope(p) + summarizeActivity(p);
        case 'agent.limited': {
            const code = str(p.code);
            return (code && AGENT_LIMIT_MESSAGES[code]) ?? code ?? '';
        }
        // Phase transitions: the type is the message. Their payloads carry
        // deployment-internal routing (provider, instance), never shown.
        case 'job.provisioning':
        case 'vm.booting':
        case 'vm.online':
        case 'job.compiling':
        case 'job.proving':
        case 'job.verifying':
        case 'job.cancelling':
        case 'job.cancelled':
            return str(p.message) ?? '';
        case 'setup.progress':
        case 'log':
            return str(p.message) ?? str(p.step) ?? '';
        case 'job.completed':
            return str(p.outcome) ?? '';
        case 'job.errored':
            return [str(p.errorCode), str(p.errorReason)]
                .filter(Boolean)
                .join(': ');
        case 'integrity.warning':
            return str(p.detail) ?? str(p.message) ?? compactJson(env.payload);
        case 'heartbeat':
        case 'usage.tick':
            return '';
        default:
            return compactJson(env.payload);
    }
}

function compactJson(payload: unknown): string {
    if (payload === null || payload === undefined) {
        return '';
    }
    try {
        const s = JSON.stringify(payload);
        if (s === '{}' || s === 'null') {
            return '';
        }
        return s.length > MAX_PAYLOAD_CHARS
            ? `${s.slice(0, MAX_PAYLOAD_CHARS)}…`
            : s;
    } catch {
        return '';
    }
}

/**
 * One log line: `HH:MM:SS type — summary` (summary omitted when empty).
 * Plain text; the caller escapes for HTML.
 */
export function formatEventLine(env: EventEnvelope): string {
    const summary = summarize(env);
    // Agent activity is the transcript; a short label keeps it readable.
    const label = env.type === 'agent.activity' ? 'agent' : env.type;
    const separator = env.type === 'agent.activity' ? ' ' : ' — ';
    const line = `${fmtClock(env.ts)}  ${label}${summary ? `${separator}${summary}` : ''}`;
    return line.length > MAX_LINE_CHARS
        ? `${line.slice(0, MAX_LINE_CHARS)}…`
        : line;
}
