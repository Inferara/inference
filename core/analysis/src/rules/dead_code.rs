//! A020: Unreachable code after `return`, `break`, or infinite loop.
//!
//! Statements that appear after an unconditional `return`, `break`, or an
//! infinite loop without `break` within the same block are unreachable and
//! indicate a likely mistake.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, StmtId};
use inference_ast::nodes::{Def, Stmt};

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Unreachable code after `return`, `break`, or infinite loop.
    #[id = "A020"]
    #[name = "Dead code"]
    #[severity = warning]
    pub struct DeadCode;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut warnings = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.defs, &mut warnings);
        }
        warnings
    }
}

fn check_defs(arena: &AstArena, def_ids: &[DefId], warnings: &mut Vec<AnalysisDiagnostic>) {
    for &def_id in def_ids {
        match &arena[def_id].kind {
            Def::Function { body, .. } => check_block(arena, *body, warnings),
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function { body, .. } = &arena[method_id].kind {
                        check_block(arena, *body, warnings);
                    }
                }
            }
            Def::Spec { defs, .. } => check_defs(arena, defs, warnings),
            _ => {}
        }
    }
}

fn check_block(arena: &AstArena, block_id: BlockId, warnings: &mut Vec<AnalysisDiagnostic>) {
    let block = &arena[block_id];
    let stmts = &block.stmts;

    for (i, &stmt_id) in stmts.iter().enumerate() {
        recurse_into_sub_blocks(arena, stmt_id, warnings);

        if let Some(terminator) = stmt_terminator_kind(arena, stmt_id) {
            for &dead_stmt_id in &stmts[i + 1..] {
                warnings.push(AnalysisDiagnostic::DeadCode {
                    terminator,
                    location: arena[dead_stmt_id].location,
                });
            }
            break;
        }
    }
}

fn recurse_into_sub_blocks(
    arena: &AstArena,
    stmt_id: StmtId,
    warnings: &mut Vec<AnalysisDiagnostic>,
) {
    match &arena[stmt_id].kind {
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            check_block(arena, *then_block, warnings);
            if let Some(else_id) = else_block {
                check_block(arena, *else_id, warnings);
            }
        }
        Stmt::Loop { body, .. } => {
            check_block(arena, *body, warnings);
        }
        Stmt::Block(block_id) => {
            check_block(arena, *block_id, warnings);
        }
        _ => {}
    }
}

/// Returns the terminator kind if the statement is an unconditional terminator.
fn stmt_terminator_kind(arena: &AstArena, stmt_id: StmtId) -> Option<&'static str> {
    match &arena[stmt_id].kind {
        Stmt::Return { .. } => Some("return"),
        Stmt::Break => Some("break"),
        Stmt::Loop {
            condition: None,
            body,
        } if !crate::walker::contains_break_for_this_loop(arena, *body) => Some("loop"),
        Stmt::If {
            then_block,
            else_block: Some(else_id),
            ..
        } => {
            let then_term = block_terminates(arena, *then_block);
            let else_term = block_terminates(arena, *else_id);
            match (then_term, else_term) {
                (Some(k1), Some(k2)) if k1 == k2 => Some(k1),
                (Some(_), Some(_)) => Some("conditional"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns the terminator kind if a block unconditionally terminates.
fn block_terminates(arena: &AstArena, block_id: BlockId) -> Option<&'static str> {
    let block = &arena[block_id];
    for &stmt_id in &block.stmts {
        if let Some(k) = stmt_terminator_kind(arena, stmt_id) {
            return Some(k);
        }
    }
    None
}
