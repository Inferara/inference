//! A011: Struct definitions must have fields or methods.

use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Struct definitions must have at least one field or method.
    #[id = "A011"]
    #[name = "Empty struct definition"]
    #[severity = warning]
    pub struct EmptyStructDefinition;
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
            Def::Struct { name, fields, methods, .. }
                if fields.is_empty() && methods.is_empty() =>
            {
                warnings.push(AnalysisDiagnostic::EmptyStructDefinition {
                    name: arena[*name].name.clone(),
                    location: arena[def_id].location,
                });
            }
            Def::Spec { defs, .. } => check_defs(arena, defs, warnings),
            _ => {}
        }
    }
}
