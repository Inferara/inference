/**
 * Compare two semver strings. Returns negative if a < b, 0 if equal, positive if a > b.
 * Handles numeric major.minor.patch and pre-release tags per the SemVer 2.0.0 spec.
 *
 * Pre-release versions have lower precedence than the associated normal version:
 *   1.0.0-alpha < 1.0.0
 *
 * Pre-release identifiers are compared left-to-right:
 *   - Numeric identifiers are compared as integers.
 *   - Alphanumeric identifiers are compared lexically (ASCII order).
 *   - Numeric identifiers always have lower precedence than alphanumeric.
 *   - A shorter set of identifiers has lower precedence if all preceding are equal.
 */
export function compareSemver(a: string, b: string): number {
    const clean = (v: string) => v.replace(/^v/i, '');
    const [coreA, preA] = clean(a).split('-', 2);
    const [coreB, preB] = clean(b).split('-', 2);

    const pa = coreA.split('.').map(Number);
    const pb = coreB.split('.').map(Number);
    for (let i = 0; i < 3; i++) {
        const diff = (pa[i] || 0) - (pb[i] || 0);
        if (diff !== 0) {
            return diff;
        }
    }

    if (!preA && !preB) {
        return 0;
    }
    if (preA && !preB) {
        return -1;
    }
    if (!preA && preB) {
        return 1;
    }

    const partsA = preA!.split('.');
    const partsB = preB!.split('.');
    const len = Math.max(partsA.length, partsB.length);
    for (let i = 0; i < len; i++) {
        if (i >= partsA.length) {
            return -1;
        }
        if (i >= partsB.length) {
            return 1;
        }
        const numA = Number(partsA[i]);
        const numB = Number(partsB[i]);
        const aIsNum = !Number.isNaN(numA);
        const bIsNum = !Number.isNaN(numB);
        if (aIsNum && bIsNum) {
            if (numA !== numB) {
                return numA - numB;
            }
        } else if (aIsNum) {
            return -1;
        } else if (bIsNum) {
            return 1;
        } else {
            if (partsA[i] < partsB[i]) {
                return -1;
            }
            if (partsA[i] > partsB[i]) {
                return 1;
            }
        }
    }
    return 0;
}
