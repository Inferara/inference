//! A034: Visibility modifier on definitions inside a `spec` body has no effect.
//!
//! Inside a `spec { ... }` block, the spec itself is the unit of visibility:
//! marking inner definitions with `pub` is meaningless and misleading. This
//! rule emits a warning for every such occurrence so the modifier can be
//! removed.

use inference_ast::ids::DefId;
use inference_ast::nodes::{Def, Visibility};

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Visibility modifiers on definitions inside a `spec` body are
    /// meaningless because the spec is the visibility unit.
    #[id = "A034"]
    #[name = "Visibility modifier inside spec body"]
    #[severity = warning]
    pub struct VisibilityInsideSpec;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut warnings = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            scan_for_specs(arena, &source_file.defs, &mut warnings);
        }
        warnings
    }
}

fn scan_for_specs(
    arena: &inference_ast::arena::AstArena,
    defs: &[DefId],
    warnings: &mut Vec<AnalysisDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Spec { name, defs: inner, .. } => {
                let spec_name = arena[*name].name.clone();
                for &inner_id in inner {
                    check_inner_def(arena, inner_id, &spec_name, warnings);
                }
                // Defensively recurse so nested specs (should they ever
                // become reachable) are still inspected.
                scan_for_specs(arena, inner, warnings);
            }
            Def::Module { defs: Some(inner), .. } => {
                scan_for_specs(arena, inner, warnings);
            }
            _ => {}
        }
    }
}

fn check_inner_def(
    arena: &inference_ast::arena::AstArena,
    def_id: DefId,
    spec_name: &str,
    warnings: &mut Vec<AnalysisDiagnostic>,
) {
    let (vis, name_id, kind): (&Visibility, _, &'static str) = match &arena[def_id].kind {
        Def::Function { vis, name, .. } => (vis, *name, "fn"),
        Def::ExternFunction { vis, name, .. } => (vis, *name, "extern fn"),
        Def::Struct { vis, name, .. } => (vis, *name, "struct"),
        Def::Enum { vis, name, .. } => (vis, *name, "enum"),
        Def::Constant { vis, name, .. } => (vis, *name, "const"),
        Def::TypeAlias { vis, name, .. } => (vis, *name, "type"),
        // Nested spec / module inside a spec body is not currently
        // grammatically reachable; the outer scan_for_specs handles
        // any future reachability.
        Def::Spec { .. } | Def::Module { .. } => return,
    };
    if matches!(vis, Visibility::Public) {
        warnings.push(AnalysisDiagnostic::VisibilityInsideSpec {
            spec_name: spec_name.to_string(),
            def_name: arena[name_id].name.clone(),
            def_kind: kind,
            location: arena[def_id].location,
        });
    }
}
