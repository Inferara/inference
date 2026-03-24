//! A004: Infinite loop must contain a reachable break statement.
//!
//! Uses its own block traversal to find nested infinite loops, and delegates
//! break-reachability checking to `walker::contains_break_for_this_loop`.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, StmtId};
use inference_ast::nodes::Stmt;

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Infinite loop must contain a reachable break statement.
    #[id = "A004"]
    #[name = "Infinite loop without break"]
    #[severity = error]
    pub struct InfiniteLoopWithoutBreak;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
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

fn check_block(arena: &AstArena, block_id: BlockId, errors: &mut Vec<AnalysisDiagnostic>) {
    let block = &arena[block_id];
    check_statements(arena, &block.stmts, errors);
}

fn check_statements(arena: &AstArena, stmt_ids: &[StmtId], errors: &mut Vec<AnalysisDiagnostic>) {
    for &stmt_id in stmt_ids {
        check_statement(arena, stmt_id, errors);
    }
}

fn check_statement(arena: &AstArena, stmt_id: StmtId, errors: &mut Vec<AnalysisDiagnostic>) {
    match &arena[stmt_id].kind {
        Stmt::Loop { condition, body } => {
            if condition.is_none() && !crate::walker::contains_break_for_this_loop(arena, *body) {
                errors.push(AnalysisDiagnostic::InfiniteLoopWithoutBreak {
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
