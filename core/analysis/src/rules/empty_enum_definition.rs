//! A009: Enum definitions must have at least one variant.

use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Enum definitions must have at least one variant.
    #[id = "A009"]
    #[name = "Empty enum definition"]
    #[severity = warning]
    pub struct EmptyEnumDefinition;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut warnings = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.defs, &mut warnings);
        }
        warnings
    }
}

fn check_defs(
    arena: &inference_ast::arena::AstArena,
    defs: &[DefId],
    warnings: &mut Vec<AnalysisDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Enum { name, variants, .. } if variants.is_empty() => {
                warnings.push(AnalysisDiagnostic::EmptyEnumDefinition {
                    name: arena[*name].name.clone(),
                    location: arena[def_id].location,
                });
            }
            Def::Spec { defs, .. } => check_defs(arena, defs, warnings),
            _ => {}
        }
    }
}
