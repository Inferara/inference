//! A009: Enum definitions must have at least one variant.

use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

crate::rule! {
    /// Enum definitions must have at least one variant.
    #[id = "A009"]
    #[name = "Empty enum definition"]
    #[severity = warning]
    pub struct EmptyEnumDefinition;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut warnings = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.module_path, &source_file.defs, &mut warnings);
        }
        warnings
    }
}

fn check_defs(
    arena: &inference_ast::arena::AstArena,
    module_path: &[String],
    defs: &[DefId],
    warnings: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Enum { name, variants, .. } if variants.is_empty() => {
                warnings.push(LabeledDiagnostic::new(
                    module_path.to_vec(),
                    AnalysisDiagnostic::EmptyEnumDefinition {
                        name: arena[*name].name.clone(),
                        location: arena[def_id].location,
                    },
                ));
            }
            Def::Spec { defs, .. } => check_defs(arena, module_path, defs, warnings),
            _ => {}
        }
    }
}
