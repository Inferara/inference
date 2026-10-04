//! A032: A `const` declared inside a `spec` block is not yet supported.
//!
//! A module-scope `const` is computed by the type checker and emitted by code
//! generation — inline for a scalar, as bytes of the static data region for an
//! array or a struct. A `const` inside a `spec` block takes none of that path:
//! it registers in the spec's own scope, where the initializer check, the
//! evaluation and the static data placement do not reach, so a use of one could
//! only reach code generation as a name with no value. This rule turns that into
//! a diagnostic at the declaration, which names the fix: a module-scope `const`
//! is visible to the spec's functions too.
//!
//! The rule once refused every module-scope `const`, before module constants
//! had a representation; it keeps its ID for the part that still has none.
//!
//! Function-scoped `const` (e.g. `const X: i32 = 42` declared inside a function
//! body) is supported and unaffected by this rule — such `const`s appear as
//! `Stmt::ConstDef`, not as `Def::Constant` among a spec's definitions.

use inference_ast::arena::AstArena;
use inference_ast::nodes::Def;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

crate::rule! {
    /// A `const` inside a `spec` block is not yet supported.
    #[id = "A032"]
    #[name = "Const inside spec not supported"]
    #[severity = error]
    pub struct ConstInSpecNotSupported;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            for &def_id in &source_file.defs {
                if let Def::Spec { name, defs, .. } = &arena[def_id].kind {
                    let spec_name = &arena[*name].name;
                    check_spec(arena, &source_file.module_path, spec_name, defs, &mut errors);
                }
            }
        }
        errors
    }
}

fn check_spec(
    arena: &AstArena,
    module_path: &[String],
    spec_name: &str,
    defs: &[inference_ast::ids::DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        if let Def::Constant { name, .. } = &arena[def_id].kind {
            errors.push(LabeledDiagnostic::new(
                module_path.to_vec(),
                AnalysisDiagnostic::ConstInSpecNotSupported {
                    name: arena[*name].name.clone(),
                    spec_name: spec_name.to_string(),
                    location: arena[def_id].location,
                },
            ));
        }
    }
}
