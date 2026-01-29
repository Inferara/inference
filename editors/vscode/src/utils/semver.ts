/**
 * Compare two semver strings. Returns negative if a < b, 0 if equal, positive if a > b.
 * Only handles numeric major.minor.patch; ignores pre-release tags.
 */
export function compareSemver(a: string, b: string): number {
    const pa = a.split('-')[0].split('.').map(Number);
    const pb = b.split('-')[0].split('.').map(Number);
    for (let i = 0; i < 3; i++) {
        const diff = (pa[i] || 0) - (pb[i] || 0);
        if (diff !== 0) {
            return diff;
        }
    }
    return 0;
}
