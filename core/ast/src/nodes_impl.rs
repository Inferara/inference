//! Convenience methods for AST node types.
//!
//! With the new arena-indexed design, most "constructor" methods are gone —
//! nodes are created by populating plain structs and calling `arena.alloc_*()`.
//! This module provides query helpers that need arena access.

use crate::arena::AstArena;
use crate::ids::{BlockId, DefId, ExprId, StmtId};
use crate::nodes::{Def, Expr, GuardedOp, Stmt};

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
            Stmt::Loop { condition, .. } => condition.is_some_and(|c| self.expr_is_non_det(c)),
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.expr_is_non_det(*condition)
                    || self.block_is_non_det(*then_block)
                    || else_block.is_some_and(|b| self.block_is_non_det(b))
            }
            Stmt::VarDef { value, .. } => value.is_some_and(|v| self.expr_is_non_det(v)),
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
                Stmt::Block(inner_block_id)
                    if !self.block_is_void(*inner_block_id) =>
                {
                    return true;
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
                let returns_unit = returns.is_none_or(|ty_id| self[ty_id].kind.is_unit_type());
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

    /// The expression a *transparent wrapper* encloses, or `None` when
    /// `expr_id` is not one.
    ///
    /// Two node kinds group an expression without contributing a value of their
    /// own: [`Expr::Parenthesized`] and [`Expr::ArithMode`]. Both denote exactly
    /// what they enclose, so a pass asking what an expression *is* — a literal,
    /// a zero, a name, the root of a projection, a constant index, the operand
    /// of a unary operator — has to look through them, and the two must never be
    /// looked through differently: a check that peels one and not the other
    /// changes its answer when an author writes `wrapping(…)` around a spelling
    /// it already accepted.
    ///
    /// This is deliberately *not* a full peel to a non-wrapper node. A caller
    /// that wants that uses [`Self::peel_transparent`]; a caller matching one
    /// level at a time (a `match` arm that recurses, a `while let`) wants this.
    #[must_use]
    pub fn transparent_inner(&self, expr_id: ExprId) -> Option<ExprId> {
        match &self[expr_id].kind {
            Expr::Parenthesized { expr } | Expr::ArithMode { expr, .. } => Some(*expr),
            _ => None,
        }
    }

    /// The expression inside any depth of transparent wrappers, or `expr_id`
    /// itself when it is not wrapped.
    ///
    /// The repeated form of [`Self::transparent_inner`], for the callers that
    /// only want the node underneath.
    #[must_use]
    pub fn peel_transparent(&self, expr_id: ExprId) -> ExprId {
        let mut current = expr_id;
        while let Some(inner) = self.transparent_inner(current) {
            current = inner;
        }
        current
    }

    /// The governed operator written at `expr_id`, paired with the expression
    /// whose recorded type is the width it is performed at, or `None` when the
    /// node is not an operator an arithmetic-mode annotation reaches.
    ///
    /// A binary operator is performed at its *left* operand's type and a
    /// negation at its own, and that choice is what decides both which guard is
    /// emitted and whether a rule calls the annotation meaningful. Two passes
    /// ask, so the pair is produced once here: an operand read differently in
    /// the two places would diagnose a program against arithmetic it does not
    /// have.
    #[must_use]
    pub fn guarded_operator(&self, expr_id: ExprId) -> Option<(GuardedOp, ExprId)> {
        match &self[expr_id].kind {
            Expr::Binary { op, left, .. } => Some((GuardedOp::from_binary(op)?, *left)),
            Expr::PrefixUnary { op, .. } => Some((GuardedOp::from_unary(op)?, expr_id)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{ArithMode, ExprData, Location};

    fn push(arena: &mut AstArena, kind: Expr) -> ExprId {
        arena.exprs.alloc(ExprData {
            location: Location::default(),
            kind,
        })
    }

    fn number(arena: &mut AstArena) -> ExprId {
        push(
            arena,
            Expr::NumberLiteral {
                value: "1".to_string(),
            },
        )
    }

    #[test]
    fn both_wrapper_kinds_are_transparent_and_nothing_else_is() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        let parenthesized = push(&mut arena, Expr::Parenthesized { expr: literal });
        let checked = push(
            &mut arena,
            Expr::ArithMode {
                mode: ArithMode::Checked,
                expr: literal,
            },
        );
        let wrapping = push(
            &mut arena,
            Expr::ArithMode {
                mode: ArithMode::Wrapping,
                expr: literal,
            },
        );
        for wrapper in [parenthesized, checked, wrapping] {
            assert_eq!(arena.transparent_inner(wrapper), Some(literal));
        }
        let uzumaki = push(&mut arena, Expr::Uzumaki);
        for opaque in [literal, uzumaki] {
            assert_eq!(arena.transparent_inner(opaque), None);
        }
    }

    #[test]
    fn peeling_goes_through_mixed_nestings_to_the_same_node() {
        let mut arena = AstArena::default();
        let literal = number(&mut arena);
        let mut current = literal;
        for mode in [ArithMode::Wrapping, ArithMode::Checked] {
            current = push(&mut arena, Expr::Parenthesized { expr: current });
            current = push(
                &mut arena,
                Expr::ArithMode {
                    mode,
                    expr: current,
                },
            );
        }
        assert_eq!(arena.peel_transparent(current), literal);
        assert_eq!(arena.peel_transparent(literal), literal);
    }
}
