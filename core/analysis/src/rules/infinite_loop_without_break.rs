//! A004: Infinite loop must contain a reachable break statement.
//!
//! This rule uses its own traversal rather than the shared walker because
//! it needs to inspect loop bodies for `break` with special scoping rules:
//! - Does NOT recurse into nested loops (break there targets the inner loop)
//! - Does NOT recurse into non-det blocks (break inside non-det is prohibited)
//! - DOES recurse into if/else arms and regular blocks

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, StmtId};
use inference_ast::nodes::{BlockKind, Stmt};

use crate::errors::AnalysisError;

crate::rule! {
    /// Infinite loop must contain a reachable break statement.
    #[id = "A004"]
    #[name = "Infinite loop without break"]
    pub struct InfiniteLoopWithoutBreak;
    fn check(ctx: &TypedContext) -> Vec<AnalysisError> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            crate::walker::for_each_function_body(arena, &source_file.defs, &mut |body_id| {
                check_block(arena, body_id, &mut errors);
            });
        }
        errors
    }
}

fn check_block(arena: &AstArena, block_id: BlockId, errors: &mut Vec<AnalysisError>) {
    let block = &arena[block_id];
    check_statements(arena, &block.stmts, errors);
}

fn check_statements(arena: &AstArena, stmt_ids: &[StmtId], errors: &mut Vec<AnalysisError>) {
    for &stmt_id in stmt_ids {
        check_statement(arena, stmt_id, errors);
    }
}

fn check_statement(arena: &AstArena, stmt_id: StmtId, errors: &mut Vec<AnalysisError>) {
    match &arena[stmt_id].kind {
        Stmt::Loop { condition, body } => {
            if condition.is_none() && !contains_break_for_this_loop(arena, *body) {
                errors.push(AnalysisError::InfiniteLoopWithoutBreak {
                    location: arena[stmt_id].location,
                });
            }
            // Continue recursing into the loop body to find nested infinite loops.
            check_block(arena, *body, errors);
        }
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            check_block(arena, *then_block, errors);
            if let Some(else_id) = else_block {
                check_block(arena, *else_id, errors);
            }
        }
        Stmt::Block(block_id) => {
            check_block(arena, *block_id, errors);
        }
        Stmt::Assign { .. }
        | Stmt::Return { .. }
        | Stmt::Break
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::TypeDef { .. }
        | Stmt::Assert { .. }
        | Stmt::ConstDef(_) => {}
    }
}

/// Checks whether a loop body contains at least one `break` that targets
/// the current loop (not a nested inner loop).
///
/// This function scans the body recursively but:
/// - Does NOT recurse into nested `Loop` statement bodies (break there targets the nested loop)
/// - Does NOT recurse into non-det block bodies (break inside non-det is prohibited)
/// - DOES recurse into `if/else` arms and regular `Block` statements
fn contains_break_for_this_loop(arena: &AstArena, block_id: BlockId) -> bool {
    let block = &arena[block_id];
    if block.block_kind != BlockKind::Regular {
        return false;
    }
    contains_break_in_statements(arena, &block.stmts)
}

fn contains_break_in_statements(arena: &AstArena, stmt_ids: &[StmtId]) -> bool {
    for &stmt_id in stmt_ids {
        if contains_break_in_statement(arena, stmt_id) {
            return true;
        }
    }
    false
}

fn contains_break_in_statement(arena: &AstArena, stmt_id: StmtId) -> bool {
    match &arena[stmt_id].kind {
        Stmt::Break => true,
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            contains_break_for_this_loop(arena, *then_block)
                || else_block
                    .is_some_and(|b| contains_break_for_this_loop(arena, b))
        }
        Stmt::Block(block_id) => {
            let block = &arena[*block_id];
            if block.block_kind != BlockKind::Regular {
                return false;
            }
            contains_break_in_statements(arena, &block.stmts)
        }
        // Loop: break inside a nested loop targets that inner loop, not the outer one.
        Stmt::Loop { .. }
        | Stmt::Return { .. }
        | Stmt::Assign { .. }
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::TypeDef { .. }
        | Stmt::Assert { .. }
        | Stmt::ConstDef(_) => false,
    }
}
