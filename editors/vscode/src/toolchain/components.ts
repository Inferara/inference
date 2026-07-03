import { DoctorResult } from './doctor';

/**
 * Toolchain components the CLI can provision via `infs component add`.
 *
 * Mirrors the `KNOWN_COMPONENTS` list in the `infs component` command
 * (`apps/infs/src/commands/component.rs`); keep the two in sync.
 */
export const KNOWN_COMPONENTS = ['wasm-opt'] as const;

/** A component name understood by `infs component`. */
export type ComponentName = (typeof KNOWN_COMPONENTS)[number];

/** Build the argv for `infs component add <component>`. */
export function componentAddArgs(component: ComponentName): string[] {
    return ['component', 'add', component];
}

/**
 * Whether a doctor result indicates the wasm-opt component needs attention,
 * i.e. any check named `wasm-opt` reported a warning or failure.
 */
export function wasmOptNeedsAttention(result: DoctorResult): boolean {
    return result.checks.some(
        (check) =>
            check.name === 'wasm-opt' &&
            (check.status === 'warn' || check.status === 'fail'),
    );
}
