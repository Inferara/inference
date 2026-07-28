//! A043: An entry-file top-level `pub fn` may not use a reserved export name.
//!
//! Code generation exports an entry-file top-level (non-method) `pub fn` under
//! its plain source name, and separately appends the synthetic exports `memory`
//! (the module's linear memory) and `__stack_pointer` (its shadow-stack global)
//! whenever the compiled module emits linear memory. Those two names are
//! therefore reserved: a user function claiming either one collides with the
//! compiler's own export surface.
//!
//! The collision has two failure modes depending on codegen-internal state that
//! is not visible from the source alone:
//!
//! - If the program emits linear memory (any struct/array local anywhere is
//!   enough), the module ends up with two exports named `memory` (or
//!   `__stack_pointer`), which is invalid WebAssembly — export names must be
//!   unique across kinds.
//! - If the program uses no memory, the module is valid but exports a *Function*
//!   named `memory`, hijacking the name WebAssembly hosts expect to resolve to
//!   the module's linear memory. That is an ABI hazard.
//!
//! Whether a given program falls in the first or the second case is codegen
//! decision, and adding an unrelated struct local later would silently flip a
//! program from the second case into the first. The rule is therefore
//! unconditional: it rejects the reserved names regardless of whether the
//! program happens to use memory, so the exported ABI surface never depends on
//! that hidden state.
//!
//! The reserved names mirror the synthetic exports emitted in the export-section
//! code of core/wasm-codegen/src/compiler.rs; the two sites must stay in sync if
//! either reserved name ever changes.

use inference_ast::nodes::{Def, Visibility};

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

/// Export names the compiled module reserves for its own synthetic exports.
const RESERVED_EXPORT_NAMES: [&str; 2] = ["memory", "__stack_pointer"];

crate::rule! {
    /// An entry-file top-level `pub fn` must not use a reserved WebAssembly
    /// export name (`memory`, `__stack_pointer`).
    #[id = "A043"]
    #[name = "Reserved export name"]
    #[severity = error]
    pub struct ReservedExportName;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            if !source_file.is_entry() {
                continue;
            }
            // Direct `defs` entries that are `Def::Function` are top-level and
            // non-method by construction: methods nest in `Def::Struct` and spec
            // functions in `Def::Spec`. Only these top-level entry-file `pub fn`s
            // are exported under their plain name, so the check must not recurse
            // into struct or spec definitions.
            for &def_id in &source_file.defs {
                if let Def::Function { name, vis: Visibility::Public, .. } = &arena[def_id].kind {
                    let ident = &arena[*name];
                    if RESERVED_EXPORT_NAMES.contains(&ident.name.as_str()) {
                        errors.push(LabeledDiagnostic::entry(
                            AnalysisDiagnostic::ReservedExportName {
                                name: ident.name.clone(),
                                location: ident.location,
                            },
                        ));
                    }
                }
            }
        }
        errors
    }
}
