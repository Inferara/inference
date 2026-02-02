import { VersionInfo } from './versions';
import { compareSemver } from '../utils/semver';

export type UpdateCheckResult =
    | { status: 'up-to-date'; version: string }
    | { status: 'update-available'; current: string; latest: string }
    | { status: 'no-versions' }
    | { status: 'no-current-version' };

/**
 * Determine whether an update is available based on current version and available versions.
 *
 * Filters to `available_for_current` versions and compares the highest against current.
 */
export function checkUpdateAvailable(
    currentVersion: string | null,
    versions: VersionInfo[] | null,
): UpdateCheckResult {
    if (!currentVersion) {
        return { status: 'no-current-version' };
    }

    if (!versions) {
        return { status: 'no-versions' };
    }

    const candidates = versions.filter((v) => v.available_for_current);

    if (candidates.length === 0) {
        return { status: 'no-versions' };
    }

    const sorted = [...candidates].sort((a, b) =>
        compareSemver(b.version, a.version),
    );
    const latest = sorted[0];

    if (compareSemver(currentVersion, latest.version) >= 0) {
        return { status: 'up-to-date', version: currentVersion };
    }

    return {
        status: 'update-available',
        current: currentVersion,
        latest: latest.version,
    };
}
