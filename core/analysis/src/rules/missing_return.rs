//! A007: Non-void functions must return on all code paths.
//!
//! This rule iterates function definitions directly rather than using the
//! shared walker, since it needs the function signature (return type) and
//! performs whole-body analysis rather than per-statement checks.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, StmtId, TypeId};
use inference_ast::nodes::{Def, SimpleTypeKind, Stmt, TypeNode};

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

fn has_non_unit_return_type(arena: &AstArena, returns: Option<TypeId>) -> bool {
    match returns {
        None => false,
        Some(type_id) => match &arena[type_id].kind {
            TypeNode::Simple(SimpleTypeKind::Unit) => false,
            TypeNode::Custom(ident_id) => arena[*ident_id].name != "unit",
            _ => true,
        },
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
                if has_non_unit_return_type(arena, *returns)
                    && !returns_on_all_paths(arena, *body)
                {
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
                        && has_non_unit_return_type(arena, *returns)
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
///
/// Scans all statements in order. If any statement is a definitive terminator
/// (return, if-else where both branches return, infinite loop that returns or
/// has no break), the block returns on all paths. Statements after a terminator
/// are dead code but do not affect the return analysis.
fn returns_on_all_paths(arena: &AstArena, block_id: BlockId) -> bool {
    let block = &arena[block_id];
    for &stmt_id in &block.stmts {
        if stmt_returns_on_all_paths(arena, stmt_id) {
            return true;
        }
    }
    false
}

fn stmt_returns_on_all_paths(arena: &AstArena, stmt_id: StmtId) -> bool {
    match &arena[stmt_id].kind {
        Stmt::Return { .. } => true,
        Stmt::If {
            then_block,
            else_block: Some(else_id),
            ..
        } => returns_on_all_paths(arena, *then_block) && returns_on_all_paths(arena, *else_id),
        Stmt::Loop {
            condition: None,
            body,
        } => {
            returns_on_all_paths(arena, *body)
                || !crate::walker::contains_break_for_this_loop(arena, *body)
        }
        // Conditional loops may never execute, so they cannot guarantee a return
        #[allow(clippy::match_same_arms)]
        Stmt::Loop {
            condition: Some(_), ..
        } => false,
        Stmt::Block(inner) => returns_on_all_paths(arena, *inner),
        _ => false,
    }
}
