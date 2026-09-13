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
    ///
    /// The walk is not total over the statement tree: a `loop` is inspected for
    /// a non-deterministic *condition* and its body is not descended, and
    /// `Stmt::Assign`, `Stmt::Assert` and `Stmt::ConstDef` answer `false`
    /// without looking at their operands (`Stmt::Break` carries none).
    ///
    /// It is not total *within* a statement either, which is the wider gap. Every
    /// arm that inspects an expression asks [`Self::expr_is_non_det`], and that
    /// recognizes a bare `@` and nothing that merely contains one -- so
    /// `return x + @;` answers `false` through the `Return` arm as surely as a
    /// construct in a loop body does, and so do `let n: i32 = f(@);` and
    /// `if @ == 1 { }`.
    ///
    /// A caller that needs the total answer -- every construct in every nested
    /// block -- uses the analysis walker, which descends all of them; this one is
    /// a cheap approximation and a `false` from it is not a proof that a body is
    /// deterministic.
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

    /// The first definition in `defs`, or in a struct's methods below one, whose
    /// body is non-deterministic.
    ///
    /// [`Self::def_is_non_det`] answers for a single definition, which is the
    /// whole answer only for a caller that already holds every function. A
    /// caller sweeping a file's top-level `defs` does not: a struct's methods
    /// are ordinary functions that reach code generation and appear in no file's
    /// list, so asking the top level alone accepts a program whose
    /// non-determinism is one `impl`-block deep.
    ///
    /// The descent stops at [`Def::Spec`], deliberately and not for want of a
    /// second arm. A specification body is non-deterministic by construction and
    /// `compile` mode strips it before a byte is emitted, so a caller asking
    /// this question about executable code would, by descending, answer `Some`
    /// for every program that writes a specification at all.
    ///
    /// The returned id names the offending *function*, so a diagnostic can print
    /// its name rather than the name of whatever contains it.
    ///
    /// Whether a function's own body counts is [`Self::def_is_non_det`]'s
    /// answer, and that walk is an approximation rather than a decision
    /// procedure, so `None` here means "no offender this walk can see".
    ///
    /// The match is exhaustive and takes no wildcard. Which definitions nest
    /// further definitions is the whole content of this function, so a variant
    /// added to [`Def`] has to say whether it does rather than inherit "it does
    /// not" from an arm nobody revisits.
    #[must_use]
    pub fn first_non_det_def(&self, defs: &[DefId]) -> Option<DefId> {
        for &def_id in defs {
            match &self[def_id].kind {
                Def::Function { .. } => {
                    if self.def_is_non_det(def_id) {
                        return Some(def_id);
                    }
                }
                Def::Struct { methods, .. } => {
                    if let Some(found) = self.first_non_det_def(methods) {
                        return Some(found);
                    }
                }
                Def::Spec { .. }
                | Def::Enum { .. }
                | Def::Constant { .. }
                | Def::ExternFunction { .. } => {}
            }
        }
        None
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

    /// A `pub fn` named `name` whose body is `@`-free or holds one uzumaki.
    fn function(arena: &mut AstArena, name: &str, non_det: bool) -> DefId {
        use crate::nodes::{BlockData, BlockKind, DefData, Ident, StmtData, Visibility};

        let value = if non_det {
            push(arena, Expr::Uzumaki)
        } else {
            number(arena)
        };
        let stmts = vec![arena.stmts.alloc(StmtData {
            location: Location::default(),
            kind: Stmt::Return { expr: value },
        })];
        let body = arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts,
        });
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: name.to_string(),
        });
        arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Function {
                name,
                vis: Visibility::Public,
                type_params: Vec::new(),
                args: Vec::new(),
                returns: None,
                body,
            },
        })
    }

    /// A field-less `struct` carrying `methods`.
    fn struct_with_methods(arena: &mut AstArena, name: &str, methods: Vec<DefId>) -> DefId {
        use crate::nodes::{DefData, Ident, Visibility};

        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: name.to_string(),
        });
        arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Struct {
                name,
                vis: Visibility::Public,
                fields: Vec::new(),
                methods,
            },
        })
    }

    /// A `spec` block carrying `defs`.
    fn spec_with_defs(arena: &mut AstArena, name: &str, defs: Vec<DefId>) -> DefId {
        use crate::nodes::{DefData, Ident, Visibility};

        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: name.to_string(),
        });
        arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Spec {
                name,
                vis: Visibility::Public,
                defs,
            },
        })
    }

    /// The walk finds nothing in a program that holds nothing, and finds the
    /// top-level function when it is the one that is non-deterministic.
    ///
    /// Fails if the walk ever stops consulting [`AstArena::def_is_non_det`] for
    /// the entries of the list it was handed.
    #[test]
    fn the_walk_answers_for_the_top_level_list_itself() {
        let mut arena = AstArena::default();
        let plain = function(&mut arena, "plain", false);
        assert_eq!(arena.first_non_det_def(&[plain]), None);

        let nondet = function(&mut arena, "nondet", true);
        assert_eq!(arena.first_non_det_def(&[plain, nondet]), Some(nondet));
    }

    /// A method is an ordinary function that no file's `defs` list names, so a
    /// walk that does not descend a struct accepts a program whose
    /// non-determinism is one declaration deep.
    ///
    /// Fails if the `Def::Struct` arm is removed: the method would be invisible
    /// and this would read `None`.
    #[test]
    fn the_walk_descends_into_a_struct_s_methods() {
        let mut arena = AstArena::default();
        let method = function(&mut arena, "method", true);
        let owner = struct_with_methods(&mut arena, "Owner", vec![method]);

        assert_eq!(
            arena.first_non_det_def(&[owner]),
            Some(method),
            "the offending method is what a diagnostic has to name, not its struct"
        );
    }

    /// The one descent the walk must *not* make. A `spec` body is
    /// non-deterministic by construction, so descending would make every
    /// specification-bearing program answer `Some`.
    ///
    /// Fails the moment a `Def::Spec` arm is added, including one copied from
    /// `def_in_list`, which descends specs because it is answering a different
    /// question.
    #[test]
    fn the_walk_stops_at_a_spec_block() {
        let mut arena = AstArena::default();
        let inside = function(&mut arena, "inside", true);
        let spec = spec_with_defs(&mut arena, "properties", vec![inside]);
        assert_eq!(arena.first_non_det_def(&[spec]), None);

        let method_spec = spec_with_defs(&mut arena, "nested", vec![inside]);
        let owner = struct_with_methods(&mut arena, "Owner", vec![method_spec]);
        assert_eq!(
            arena.first_non_det_def(&[owner]),
            None,
            "a spec reached through a struct is still a spec"
        );
    }

    /// The first offender in declaration order is the one reported, so the
    /// diagnostic a user sees does not depend on how the arena happened to be
    /// filled.
    #[test]
    fn the_walk_reports_the_first_offender_in_order() {
        let mut arena = AstArena::default();
        let second = function(&mut arena, "second", true);
        let first = function(&mut arena, "first", true);
        assert_eq!(arena.first_non_det_def(&[first, second]), Some(first));
        assert_eq!(arena.first_non_det_def(&[second, first]), Some(second));
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
