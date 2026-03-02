//! Convenience methods for AST node types.
//!
//! With the new arena-indexed design, most "constructor" methods are gone —
//! nodes are created by populating plain structs and calling `arena.alloc_*()`.
//! This module provides query helpers that need arena access.

use crate::arena::AstArena;
use crate::ids::*;
use crate::nodes::*;

impl AstArena {
    /// Checks whether a block (and its transitive children) contains
    /// any non-deterministic constructs.
    #[must_use]
    pub fn block_is_non_det(&self, block_id: BlockId) -> bool {
        let block = &self[block_id];
        if block.block_kind.is_non_det() {
            return true;
        }
        block.stmts.iter().any(|&s| self.stmt_is_non_det(s))
    }

    /// Checks whether a statement contains any non-deterministic constructs.
    #[must_use]
    pub fn stmt_is_non_det(&self, stmt_id: StmtId) -> bool {
        match &self[stmt_id].kind {
            Stmt::Block(block_id) => self.block_is_non_det(*block_id),
            Stmt::Expr(expr_id) => self.expr_is_non_det(*expr_id),
            Stmt::Return { expr } => self.expr_is_non_det(*expr),
            Stmt::Loop { condition, .. } => condition
                .map_or(false, |c| self.expr_is_non_det(c)),
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.expr_is_non_det(*condition)
                    || self.block_is_non_det(*then_block)
                    || else_block.map_or(false, |b| self.block_is_non_det(b))
            }
            Stmt::VarDef { value, .. } => value
                .map_or(false, |v| self.expr_is_non_det(v)),
            _ => false,
        }
    }

    /// Checks whether an expression is a non-deterministic uzumaki (`@`).
    #[must_use]
    pub fn expr_is_non_det(&self, expr_id: ExprId) -> bool {
        matches!(self[expr_id].kind, Expr::Uzumaki)
    }

    /// Returns `true` if the function body has no explicit `return` on any path.
    #[must_use]
    pub fn block_is_void(&self, block_id: BlockId) -> bool {
        let block = &self[block_id];
        !self.block_stmts_have_return(&block.stmts)
    }

    fn block_stmts_have_return(&self, stmts: &[StmtId]) -> bool {
        for &stmt_id in stmts {
            match &self[stmt_id].kind {
                Stmt::Return { .. } => return true,
                Stmt::Block(inner_block_id) => {
                    if !self.block_is_void(*inner_block_id) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Returns `true` if the definition is a function that is non-void.
    #[must_use]
    pub fn def_is_void_function(&self, def_id: DefId) -> bool {
        match &self[def_id].kind {
            Def::Function { returns, body, .. } => {
                let returns_unit = returns
                    .map_or(true, |ty_id| self[ty_id].kind.is_unit_type());
                returns_unit || self.block_is_void(*body)
            }
            _ => true,
        }
    }

    /// Returns `true` if a function definition body is non-deterministic.
    #[must_use]
    pub fn def_is_non_det(&self, def_id: DefId) -> bool {
        match &self[def_id].kind {
            Def::Function { body, .. } => self.block_is_non_det(*body),
            _ => false,
        }
    }
}
