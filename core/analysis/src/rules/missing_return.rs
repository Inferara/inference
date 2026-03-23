//! A007: Non-void functions must return on all code paths.
//!
//! This rule iterates function definitions directly rather than using the
//! shared walker, since it needs the function signature (return type) and
//! performs whole-body analysis rather than per-statement checks.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId};
use inference_ast::nodes::{Def, Stmt};

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Non-void functions must return on all code paths.
    #[id = "A007"]
    #[name = "Missing return"]
    #[severity = error]
    pub struct MissingReturn;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.defs, &mut errors);
        }
        errors
    }
}

fn check_defs(arena: &AstArena, def_ids: &[DefId], errors: &mut Vec<AnalysisDiagnostic>) {
    for &def_id in def_ids {
        match &arena[def_id].kind {
            Def::Function {
                name,
                returns,
                body,
                ..
            } => {
                if returns.is_some() && !returns_on_all_paths(arena, *body) {
                    errors.push(AnalysisDiagnostic::MissingReturn {
                        function_name: arena[*name].name.clone(),
                        location: arena[def_id].location,
                    });
                }
            }
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function {
                        name,
                        returns,
                        body,
                        ..
                    } = &arena[method_id].kind
                        && returns.is_some()
                        && !returns_on_all_paths(arena, *body)
                    {
                        errors.push(AnalysisDiagnostic::MissingReturn {
                            function_name: arena[*name].name.clone(),
                            location: arena[method_id].location,
                        });
                    }
                }
            }
            Def::Spec { defs, .. } => {
                check_defs(arena, defs, errors);
            }
            Def::Module { defs: Some(d), .. } => {
                check_defs(arena, d, errors);
            }
            Def::Enum { .. }
            | Def::Constant { .. }
            | Def::ExternFunction { .. }
            | Def::TypeAlias { .. }
            | Def::Module { defs: None, .. } => {}
        }
    }
}

/// Checks whether a block returns on all code paths.
fn returns_on_all_paths(arena: &AstArena, block_id: BlockId) -> bool {
    let block = &arena[block_id];
    let Some(last_stmt_id) = block.stmts.last() else {
        return false;
    };
    match &arena[*last_stmt_id].kind {
        Stmt::If {
            then_block,
            else_block: Some(else_id),
            ..
        } => returns_on_all_paths(arena, *then_block) && returns_on_all_paths(arena, *else_id),
        // Explicit return — control never falls through
        Stmt::Return { .. } => true,
        Stmt::Block(inner) => returns_on_all_paths(arena, *inner),
        _ => false,
    }
}
