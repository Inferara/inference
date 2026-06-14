//! A011: Struct definitions must have fields or methods.

use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

crate::rule! {
    /// Struct definitions must have at least one field or method.
    #[id = "A011"]
    #[name = "Empty struct definition"]
    #[severity = warning]
    pub struct EmptyStructDefinition;
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
            Def::Struct { name, fields, methods, .. }
                if fields.is_empty() && methods.is_empty() =>
            {
                warnings.push(LabeledDiagnostic::new(
                    module_path.to_vec(),
                    AnalysisDiagnostic::EmptyStructDefinition {
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
