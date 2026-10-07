/**
 * Compare the local `infc` with the compiler the proof server accepts.
 *
 * No `vscode` import — unit-tested under plain node. The accepted identity
 * comes from `GET /api/v1/meta` (`acceptedToolchain`); the local one from
 * `infc --commit-hash`, `infc --version` and `infc --abi-version`, run on the
 * exact binary `infs build` will use (the doctor's "Resolved infc" line).
 */

import type { DoctorResult } from '../toolchain/doctor';
import type { AcceptedToolchain } from './types';

/** What the local compiler reports about itself (null = unreadable). */
export interface InfcIdentity {
    /** Short or full commit hash; null when infc was built outside git. */
    commit: string | null;
    /** Full `infc --version` line, e.g. `infc 0.0.6`. */
    version: string | null;
    /** `MAJOR.MINOR`, e.g. `1.8`. */
    abiVersion: string | null;
}

export type IdentityVerdict =
    | { kind: 'match' }
    | { kind: 'mismatch'; reasons: string[] }
    /** The server does not publish an accepted compiler (older server). */
    | { kind: 'unchecked' };

/** Shortest local hash accepted as a prefix match. */
const MIN_COMMIT_CHARS = 7;

function firstLine(stdout: string): string {
    return stdout.split(/\r?\n/, 1)[0]?.trim() ?? '';
}

export function parseCommitHash(stdout: string): string | null {
    const line = firstLine(stdout).toLowerCase();
    return /^[0-9a-f]{4,40}$/.test(line) ? line : null;
}

export function parseInfcVersion(stdout: string): string | null {
    const line = firstLine(stdout);
    return /^infc \S+$/.test(line) ? line : null;
}

export function parseAbiVersion(stdout: string): string | null {
    const line = firstLine(stdout);
    return /^\d+\.\d+$/.test(line) ? line : null;
}

/**
 * Path of the compiler `infs build` will spawn, from `infs doctor`.
 * Only an `[OK]` line names a usable binary; its message is
 * `<path> (source: <label>)`.
 */
export function resolvedInfcPath(doctor: DoctorResult | null): string | null {
    const check = doctor?.checks.find((c) => c.name === 'Resolved infc');
    if (!check || check.status !== 'ok') {
        return null;
    }
    const path = check.message.replace(/\s+\(source: [^)]*\)\s*$/, '').trim();
    return path || null;
}

/**
 * Match by commit (prefix either way — `--commit-hash` prints a short hash)
 * and ABI version. The version string is informational: a matching commit
 * already pins it.
 */
export function compareIdentity(
    local: InfcIdentity,
    accepted: AcceptedToolchain | null | undefined,
): IdentityVerdict {
    if (!accepted) {
        return { kind: 'unchecked' };
    }
    const reasons: string[] = [];
    const want = accepted.commit.toLowerCase();
    const have = local.commit?.toLowerCase() ?? null;
    if (!have) {
        reasons.push('infc reports no commit hash, so it cannot be matched to the accepted compiler');
    } else if (have.length < MIN_COMMIT_CHARS) {
        reasons.push(`infc commit ${have} is too short to identify a build`);
    } else if (!want.startsWith(have) && !have.startsWith(want)) {
        reasons.push(`commit ${have} is not the accepted ${want.slice(0, 12)}`);
    }
    if (local.abiVersion !== accepted.abiVersion) {
        reasons.push(`ABI ${local.abiVersion ?? 'unknown'} is not the accepted ${accepted.abiVersion}`);
    }
    return reasons.length === 0 ? { kind: 'match' } : { kind: 'mismatch', reasons };
}

/**
 * The `infs install` version for an accepted release (`v0.0.6` → `0.0.6`),
 * or null when the accepted compiler is not a published release.
 */
export function installableVersion(accepted: AcceptedToolchain | null | undefined): string | null {
    const tag = accepted?.releaseTag?.trim();
    if (!tag) {
        return null;
    }
    const version = tag.replace(/^v/, '');
    return /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version) ? version : null;
}
