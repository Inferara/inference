/**
 * Proof-server URL rules. No `vscode` import — unit-tested under plain node.
 *
 * HTTPS is required because requests carry the API key. Plain HTTP is
 * allowed only for a loopback host, which is how an offline local
 * deployment (for example the booth bundle on `http://localhost:8088`) runs.
 */

/** The hosted Inference proof service. */
export const DEFAULT_SERVER_URL = 'https://prover-dev.inference-lang.org';

const LOOPBACK_HOSTS: ReadonlySet<string> = new Set(['localhost', '127.0.0.1', '[::1]']);

export type ServerUrlCheck = { ok: true; url: string } | { ok: false; error: string };

/** Validate the `inference.prover.serverUrl` value; empty means the default. */
export function checkServerUrl(raw: string | undefined): ServerUrlCheck {
    const value = (raw ?? '').trim();
    if (!value) {
        return { ok: true, url: DEFAULT_SERVER_URL };
    }
    let parsed: URL;
    try {
        parsed = new URL(value);
    } catch {
        return { ok: false, error: `"${value}" is not a valid URL.` };
    }
    if (parsed.username || parsed.password) {
        return { ok: false, error: 'The proof server URL must not contain credentials.' };
    }
    if (parsed.search || parsed.hash) {
        return { ok: false, error: 'The proof server URL must not have a query or fragment.' };
    }
    if (parsed.protocol === 'http:' && !LOOPBACK_HOSTS.has(parsed.hostname)) {
        return {
            ok: false,
            error: 'The proof server URL must use https; plain http is allowed only for localhost.',
        };
    }
    if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
        return { ok: false, error: 'The proof server URL must use https.' };
    }
    return { ok: true, url: `${parsed.origin}${parsed.pathname.replace(/\/+$/, '')}` };
}

/**
 * The portal page for a job (the portal serves the same origin as the API).
 * The portal routes by hash, so the route goes after `#/`; a plain path gets
 * the portal's fallback route (the jobs list).
 */
export function portalJobUrl(serverUrl: string, jobId: string, tab?: 'certificate'): string {
    const base = serverUrl.replace(/\/+$/, '');
    const page = `${base}/#/job/${encodeURIComponent(jobId)}`;
    return tab ? `${page}/${tab}` : page;
}
