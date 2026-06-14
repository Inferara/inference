//! A032: Top-level `const` declarations are not yet supported.
//!
//! Module-scope (top-level) `const` declarations — both scalar and compound —
//! are silently dropped by codegen today. Any use site then panics with a
//! "Variable not found" error. Until top-level const is implemented (which
//! requires a design decision between WASM globals, a data section, or lazy
//! initialization), this rule converts that panic into a clear diagnostic.
//!
//! Function-scoped `const` (e.g. `const X: i32 = 42` declared inside a function
//! body) is supported and unaffected by this rule — such `const`s appear as
//! `Stmt::ConstDef`, not as `Def::Constant` at module scope.

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

crate::rule! {
    /// Module-scope `const` declarations are not yet supported.
    #[id = "A032"]
    #[name = "Top-level const not supported"]
    #[severity = error]
    pub struct TopLevelConstNotSupported;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.module_path, &source_file.defs, &mut errors);
        }
        errors
    }
}

fn check_defs(
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Constant { name, .. } => {
                errors.push(LabeledDiagnostic::new(module_path.to_vec(), AnalysisDiagnostic::TopLevelConstNotSupported {
                    name: arena[*name].name.clone(),
                    location: arena[def_id].location,
                }));
            }
            Def::Spec { defs, .. } => check_defs(arena, module_path, defs, errors),
            _ => {}
        }
    }
}
