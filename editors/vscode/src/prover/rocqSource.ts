/**
 * Finding declarations in a Rocq file. Pure — no `vscode` import.
 *
 * Obligation names are theorem names (`valid_m__Spec`). Their line differs
 * between the submitted file and the returned proof (proofs and helper lemmas
 * are added), so links search the text instead of trusting a line number.
 */

const KEYWORDS = 'Theorem|Lemma|Corollary|Proposition|Fact|Remark|Example|Definition|Fixpoint|CoFixpoint|Instance';

function escapeRegExp(text: string): string {
    return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * The 1-based line declaring `name` (or, for a qualified name, its last
 * segment), or undefined when the text has no such declaration.
 */
export function declarationLine(text: string, name: string): number | undefined {
    const last = name.split('.').pop() ?? name;
    const names = last && last !== name ? [name, last] : [name];
    const lines = text.split(/\r\n?|\n/);
    for (const candidate of names) {
        if (!candidate) {
            continue;
        }
        const declaration = new RegExp(
            `^\\s*(?:#\\[[^\\]]*\\]\\s*)?(?:(?:Local|Global|Polymorphic|Program)\\s+)*(?:${KEYWORDS})\\s+${escapeRegExp(candidate)}(?![\\w'])`,
        );
        const index = lines.findIndex((line) => declaration.test(line));
        if (index >= 0) {
            return index + 1;
        }
    }
    return undefined;
}
