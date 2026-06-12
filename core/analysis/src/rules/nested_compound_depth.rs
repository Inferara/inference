//! A026: Nested compound type depth exceeds maximum.
//!
//! Only one level of compound nesting is supported. A struct field may be
//! another struct or an array, but that inner type must have only scalar
//! fields. Depth-2+ nesting (e.g., struct-in-struct-in-struct) is rejected.
//!
//! This rule operates at definition site: it inspects struct definitions via
//! `TypedContext::lookup_struct()` which provides resolved field types.

use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

use crate::errors::AnalysisDiagnostic;
use crate::walker;

crate::rule! {
    /// Nested compound type depth must not exceed one level.
    #[id = "A026"]
    #[name = "Nested compound depth"]
    #[severity = error]
    pub struct NestedCompoundDepth;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(ctx, arena, &source_file.defs, &mut errors);
        }
        errors
    }
}

fn check_defs(
    ctx: &inference_type_checker::typed_context::TypedContext,
    arena: &inference_ast::arena::AstArena,
    defs: &[DefId],
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Struct { name, fields, .. } => {
                let struct_name = arena[*name].name.clone();
                if let Some(struct_info) = ctx.lookup_struct(&struct_name) {
                    for tc_field in &struct_info.fields {
                        if walker::has_compound_fields(ctx, &tc_field.type_info.kind) {
                            let location = fields
                                .iter()
                                .find(|f| arena[f.name].name == tc_field.name)
                                .map_or_else(|| arena[*name].location, |f| arena[f.ty].location);
                            errors.push(AnalysisDiagnostic::NestedCompoundDepthExceeded {
                                outer: struct_name.clone(),
                                field: tc_field.name.clone(),
                                ty: tc_field.type_info.kind.to_string(),
                                location,
                            });
                        }
                    }
                }
            }
            Def::Spec { defs, .. } => check_defs(ctx, arena, defs, errors),
            _ => {}
        }
    }
}
