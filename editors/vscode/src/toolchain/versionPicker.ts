import { VersionInfo } from './versions';
import { compareSemver } from '../utils/semver';

export interface PickItem {
    label: string;
    description?: string;
}

/**
 * Build QuickPick items from available versions.
 *
 * - Filters to `available_for_current` versions only
 * - Sorts descending by semver
 * - Tags the current version with "(current)" and stable versions with "(stable)"
 * - Moves the current version to the top of the list
 */
export function buildVersionPickItems(
    versions: VersionInfo[],
    currentVersion: string | null,
): PickItem[] {
    const available = versions
        .filter((v) => v.available_for_current)
        .sort((a, b) => compareSemver(b.version, a.version));

    const items: PickItem[] = available.map((v) => {
        const tags: string[] = [];
        if (v.version === currentVersion) {
            tags.push('current');
        }
        if (v.stable) {
            tags.push('stable');
        }
        return {
            label: v.version,
            description: tags.length > 0 ? `(${tags.join(', ')})` : undefined,
        };
    });

    if (currentVersion) {
        const idx = items.findIndex((i) => i.label === currentVersion);
        if (idx > 0) {
            const [item] = items.splice(idx, 1);
            items.unshift(item);
        }
    }

    return items;
}
