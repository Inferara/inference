/**
 * Which proof-build errors the Problems panel shows for each file. Pure — no
 * `vscode` import.
 *
 * Two entry files can import the same file, so a file shows what the latest
 * build of each entry reported for it, and a build replaces only its own
 * entry's errors.
 */
export class BuildProblems<T> {
    /** Entry → file → the errors its latest build reported there. */
    private readonly builds = new Map<string, Map<string, T[]>>();

    /** A build of `entry` starts: forget its previous build. Returns the files that changed. */
    start(entry: string): string[] {
        const previous = this.builds.get(entry);
        this.builds.delete(entry);
        return [...(previous?.keys() ?? [])];
    }

    /** `entry`'s build reported `items` in `file`. */
    add(entry: string, file: string, items: readonly T[]): void {
        const files = this.builds.get(entry) ?? new Map<string, T[]>();
        files.set(file, [...(files.get(file) ?? []), ...items]);
        this.builds.set(entry, files);
    }

    /** `file` was edited: every build's errors there describe old text. */
    forget(file: string): void {
        for (const files of this.builds.values()) {
            files.delete(file);
        }
    }

    /** Every entry's errors in `file`, each once (`identity` tells repeats apart). */
    shown(file: string, identity: (item: T) => string): T[] {
        const seen = new Set<string>();
        const items: T[] = [];
        for (const files of this.builds.values()) {
            for (const item of files.get(file) ?? []) {
                const id = identity(item);
                if (!seen.has(id)) {
                    seen.add(id);
                    items.push(item);
                }
            }
        }
        return items;
    }
}
