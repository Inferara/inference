//! Convenience methods for AST node types.
//!
//! With the new arena-indexed design, most "constructor" methods are gone —
//! nodes are created by populating plain structs and calling `arena.alloc_*()`.
//! This module provides query helpers that need arena access.

use crate::arena::AstArena;
use crate::ids::{BlockId, DefId, ExprId, StmtId};
use crate::nodes::{Def, Expr, GuardedOp, Stmt};

impl AstArena {
    /// Checks whether a block is a non-deterministic block, or holds a statement
    /// [`Self::stmt_is_non_det`] recognizes.
    ///
    /// Transitive over the statement kinds that walker descends and no further:
    /// it is the first step of an approximation, not of a decision procedure,
    /// and [`Self::first_non_det_def_deep`] is the total reading beside it.
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
    /// block, under every statement, inside every operand -- asks
    /// [`Self::first_non_det_def_deep`], which is a decision procedure and exists
    /// beside this rather than replacing it. This one is a cheap
    /// approximation and a `false` from it is not a proof that a body is
    /// deterministic. The analysis walker is a third reading again, and the only
    /// one that reports a source location.
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

    /// Checks whether an expression *is* a non-deterministic uzumaki (`@`).
    ///
    /// The root node and nothing below it: `@ + 1` and `f(@)` both answer
    /// `false`. That is the whole of what the callers of
    /// [`Self::stmt_is_non_det`] want; a caller asking whether an expression
    /// *holds* one asks [`Self::first_non_det_def_deep`] about the definition
    /// around it.
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

    /// The first definition in `defs`, or in a struct's methods below one, that
    /// holds a non-deterministic construct **anywhere** in the code it ships.
    ///
    /// A caller sweeping a file's top-level `defs` needs the descent into a
    /// struct: a method is an ordinary function that reaches code generation and
    /// appears in no file's list, so asking the top level alone accepts a
    /// program whose non-determinism is one declaration deep.
    ///
    /// Two definitions are deliberately not descended, and neither is an arm
    /// nobody got to. A [`Def::Spec`] is not shipped code — a specification body
    /// is non-deterministic by construction and `compile` mode strips it before
    /// a byte is emitted, so descending would answer `Some` for every program
    /// that writes one. A module-scope [`Def::Constant`] contributes no
    /// instruction either: code generation drops one rather than lowering it,
    /// and a declaration that emits nothing cannot emit something forbidden. A
    /// `const` written *inside* a body is a different node and is descended, by
    /// the [`Stmt::ConstDef`] arm of the statement walk. If module-scope `const`
    /// is ever lowered, a `Def::Constant` arm has to be added here.
    ///
    /// A `false` from the walk under this — `def_contains_non_det_anywhere`
    /// — is a decision rather than an approximation: every block, every
    /// statement and every operand has been looked at. That is what separates it
    /// from [`Self::def_is_non_det`], which matches a statement's root node only
    /// and is what proof-mode translation classifies a body with.
    ///
    /// The returned id names the offending *function*, so a diagnostic can print
    /// its name rather than the name of whatever contains it.
    ///
    /// The match is exhaustive and takes no wildcard. Which definitions nest
    /// further definitions is the whole content of this function, so a variant
    /// added to [`Def`] has to say whether it does rather than inherit "it does
    /// not" from an arm nobody revisits.
    #[must_use]
    pub fn first_non_det_def_deep(&self, defs: &[DefId]) -> Option<DefId> {
        for &def_id in defs {
            match &self[def_id].kind {
                Def::Function { .. } => {
                    if self.def_contains_non_det_anywhere(def_id) {
                        return Some(def_id);
                    }
                }
                Def::Struct { methods, .. } => {
                    if let Some(found) = self.first_non_det_def_deep(methods) {
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

    /// Whether `def_id` holds a non-deterministic construct anywhere below it: in
    /// a nested block of any kind, under any statement, or inside any operand of
    /// any expression.
    ///
    /// The walk behind [`Self::first_non_det_def_deep`], which is the entry point
    /// a caller has and the one that decides which definitions are asked at all.
    ///
    /// "Anywhere below it" rather than "anywhere in the code it ships", because
    /// one arm answers about neither: a [`Def::Constant`] descends its
    /// initializer *expression*, which is what makes a `const` written inside a
    /// function body reachable through the [`Stmt::ConstDef`] arm below, while a
    /// module-scope one ships no instruction at all — which is why the walk above
    /// does not ask about one, and why the two disagree there deliberately.
    ///
    /// A [`Def::Spec`] answers `false` however its body is written, because
    /// `compile` mode strips a specification before emission and a definition
    /// that contributes no instruction cannot contribute a forbidden one. Every
    /// other arm is the honest total answer.
    #[must_use]
    fn def_contains_non_det_anywhere(&self, def_id: DefId) -> bool {
        match &self[def_id].kind {
            Def::Function { body, .. } => self.block_contains_non_det_anywhere(*body),
            Def::Struct { methods, .. } => methods
                .iter()
                .any(|&method| self.def_contains_non_det_anywhere(method)),
            Def::Constant { value, .. } => self.expr_contains_non_det_anywhere(*value),
            // A specification is stripped before emission; an enum declares no
            // code; an `external fn` declares a signature and no body.
            Def::Spec { .. } | Def::Enum { .. } | Def::ExternFunction { .. } => false,
        }
    }

    /// Whether a block is itself a non-deterministic block, or holds one below
    /// it.
    fn block_contains_non_det_anywhere(&self, block_id: BlockId) -> bool {
        let block = &self[block_id];
        block.block_kind.is_non_det()
            || block
                .stmts
                .iter()
                .any(|&stmt| self.stmt_contains_non_det_anywhere(stmt))
    }

    /// Whether a statement holds a non-deterministic construct below it.
    ///
    /// Exhaustive and wildcard-free, like the expression walk under it: an arm
    /// that answers `false` by omission is how this stops being total the next
    /// time a statement kind is added, and the whole value of the walk is that
    /// a `false` from it is a decision.
    ///
    /// Type nodes are not descended, here or below. A type carries no value, and
    /// the one expression inside one — an array's size — is required to be a
    /// constant, which the type checker enforces before code generation reads
    /// anything.
    fn stmt_contains_non_det_anywhere(&self, stmt_id: StmtId) -> bool {
        match &self[stmt_id].kind {
            Stmt::Block(block_id) => self.block_contains_non_det_anywhere(*block_id),
            Stmt::Expr(expr_id)
            | Stmt::Return { expr: expr_id }
            | Stmt::Assert { expr: expr_id } => self.expr_contains_non_det_anywhere(*expr_id),
            Stmt::Assign { left, right } => {
                self.expr_contains_non_det_anywhere(*left)
                    || self.expr_contains_non_det_anywhere(*right)
            }
            Stmt::Loop { condition, body } => {
                condition.is_some_and(|c| self.expr_contains_non_det_anywhere(c))
                    || self.block_contains_non_det_anywhere(*body)
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.expr_contains_non_det_anywhere(*condition)
                    || self.block_contains_non_det_anywhere(*then_block)
                    || else_block.is_some_and(|b| self.block_contains_non_det_anywhere(b))
            }
            Stmt::VarDef { value, .. } => {
                value.is_some_and(|v| self.expr_contains_non_det_anywhere(v))
            }
            Stmt::ConstDef(def_id) => self.def_contains_non_det_anywhere(*def_id),
            // A `break` carries no operand.
            Stmt::Break => false,
        }
    }

    /// Whether an expression is an uzumaki, or holds one below it.
    ///
    /// Exhaustive and wildcard-free for the reason the statement walk is. The
    /// grouping arms are the ones a partial walk loses first: `return x + @` and
    /// `f(@)` are non-deterministic, and a walk that matched only the root node
    /// answers `false` for both.
    fn expr_contains_non_det_anywhere(&self, expr_id: ExprId) -> bool {
        match &self[expr_id].kind {
            Expr::Uzumaki => true,
            Expr::Binary { left, right, .. } => {
                self.expr_contains_non_det_anywhere(*left)
                    || self.expr_contains_non_det_anywhere(*right)
            }
            Expr::PrefixUnary { expr, .. }
            | Expr::Parenthesized { expr }
            | Expr::ArithMode { expr, .. }
            | Expr::MemberAccess { expr, .. }
            | Expr::TypeMemberAccess { expr, .. } => self.expr_contains_non_det_anywhere(*expr),
            Expr::FunctionCall { function, args, .. } => {
                self.expr_contains_non_det_anywhere(*function)
                    || args
                        .iter()
                        .any(|(_, arg)| self.expr_contains_non_det_anywhere(*arg))
            }
            Expr::ArrayIndexAccess { array, index } => {
                self.expr_contains_non_det_anywhere(*array)
                    || self.expr_contains_non_det_anywhere(*index)
            }
            Expr::StructLiteral { fields, .. } => fields
                .iter()
                .any(|(_, value)| self.expr_contains_non_det_anywhere(*value)),
            Expr::ArrayLiteral { elements } => elements
                .iter()
                .any(|&element| self.expr_contains_non_det_anywhere(element)),
            // Leaves, and a type in expression position, which carries no value.
            Expr::Identifier(_)
            | Expr::NumberLiteral { .. }
            | Expr::BoolLiteral { .. }
            | Expr::StringLiteral { .. }
            | Expr::UnitLiteral
            | Expr::Type(_) => false,
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
    use crate::ids::IdentId;
    use crate::nodes::{ArithMode, BlockKind, ExprData, Location};

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

    /// The walk answers for the list it was handed, and answers for it in
    /// order: nothing is reported for a program that holds nothing, the
    /// non-deterministic entry is reported wherever in the list it sits, and
    /// when two offend the first in declaration order is the one named — so the
    /// diagnostic a user sees does not depend on how the arena happened to be
    /// filled.
    ///
    /// Fails if the walk stops consulting every entry of the list, or starts
    /// reporting whichever offender the arena allocated first.
    #[test]
    fn the_walk_answers_for_the_top_level_list_in_order() {
        let mut arena = AstArena::default();
        let plain = function(&mut arena, "plain", false);
        assert_eq!(arena.first_non_det_def_deep(&[plain]), None);

        let nondet = function(&mut arena, "nondet", true);
        assert_eq!(arena.first_non_det_def_deep(&[plain, nondet]), Some(nondet));

        let second = function(&mut arena, "second", true);
        assert_eq!(arena.first_non_det_def_deep(&[nondet, second]), Some(nondet));
        assert_eq!(arena.first_non_det_def_deep(&[second, nondet]), Some(second));
    }

    /// A `pub fn` named `name` whose body is the statements `stmts`.
    fn function_with_stmts(arena: &mut AstArena, name: &str, stmts: Vec<StmtId>) -> DefId {
        use crate::nodes::{BlockData, BlockKind, DefData, Ident, Visibility};

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

    fn stmt(arena: &mut AstArena, kind: Stmt) -> StmtId {
        use crate::nodes::StmtData;

        arena.stmts.alloc(StmtData {
            location: Location::default(),
            kind,
        })
    }

    fn block(arena: &mut AstArena, kind: BlockKind, stmts: Vec<StmtId>) -> BlockId {
        use crate::nodes::BlockData;

        arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: kind,
            stmts,
        })
    }

    /// `@ + 1`, the shape a top-level match on the expression node misses.
    fn uzumaki_under_an_operator(arena: &mut AstArena) -> ExprId {
        use crate::nodes::OperatorKind;

        let left = push(arena, Expr::Uzumaki);
        let right = number(arena);
        push(
            arena,
            Expr::Binary {
                left,
                right,
                op: OperatorKind::Add,
            },
        )
    }

    /// The shallow walk and the deep one disagree about an `@` under an
    /// operator, and both answers are the intended ones.
    ///
    /// Stated as one test because the value of the deep walk is exactly this
    /// difference: were the two to agree here, one of them would be redundant.
    /// The shallow reading is relied on by proof-mode translation, so this is
    /// also what says the new walk was added beside it rather than folded into
    /// it.
    ///
    /// Fails if `expr_is_non_det` is widened to look below the root node, or if
    /// the deep walk stops descending a binary operator's operands.
    #[test]
    fn the_deep_walk_sees_an_uzumaki_the_shallow_one_reports_as_absent() {
        let mut arena = AstArena::default();
        let value = uzumaki_under_an_operator(&mut arena);
        let returned = stmt(&mut arena, Stmt::Return { expr: value });
        let owner = function_with_stmts(&mut arena, "sum", vec![returned]);

        assert!(
            !arena.def_is_non_det(owner),
            "the shallow walk matches the root node only, and the root here is a `+`"
        );
        assert!(
            arena.def_contains_non_det_anywhere(owner),
            "the deep walk descends the operator's operands"
        );
        assert_eq!(arena.first_non_det_def_deep(&[owner]), Some(owner));
    }

    /// One row per statement slot that carries an expression or a block, with
    /// the slot under test filled by `value` or by `nested` and every other slot
    /// by `plain_value` or `plain`.
    ///
    /// A builder rather than a literal table so the same eleven rows can be
    /// driven twice, once with a non-deterministic filler and once with a
    /// deterministic one: a table that only ever asserts `true` is passed by a
    /// walk hard-wired to it.
    fn statement_rows(
        value: ExprId,
        nested: BlockId,
        plain_value: ExprId,
        plain: BlockId,
    ) -> Vec<(&'static str, Stmt)> {
        vec![
            ("a block statement", Stmt::Block(nested)),
            ("an expression statement", Stmt::Expr(value)),
            ("a return", Stmt::Return { expr: value }),
            ("an assertion", Stmt::Assert { expr: value }),
            (
                "the left of an assignment",
                Stmt::Assign {
                    left: value,
                    right: plain_value,
                },
            ),
            (
                "the right of an assignment",
                Stmt::Assign {
                    left: plain_value,
                    right: value,
                },
            ),
            (
                "a loop condition",
                Stmt::Loop {
                    condition: Some(value),
                    body: plain,
                },
            ),
            (
                "a loop body",
                Stmt::Loop {
                    condition: None,
                    body: nested,
                },
            ),
            (
                "an if condition",
                Stmt::If {
                    condition: value,
                    then_block: plain,
                    else_block: None,
                },
            ),
            (
                "a then branch",
                Stmt::If {
                    condition: plain_value,
                    then_block: nested,
                    else_block: None,
                },
            ),
            (
                "an else branch",
                Stmt::If {
                    condition: plain_value,
                    then_block: plain,
                    else_block: Some(nested),
                },
            ),
        ]
    }

    /// Every statement kind that carries an expression or a block is descended,
    /// and none of them answers `true` when what it carries is ordinary code.
    ///
    /// Driven as a table because the failure this guards against is one arm
    /// being forgotten, and an arm forgotten is invisible in a test that
    /// exercises three of eleven. Each row is run twice: once with a
    /// non-deterministic construct in the slot it is about — an `@` under an
    /// operator, or a `forall` one plain block down — and once with ordinary
    /// code in that same slot. Both directions matter, because an arm mutated to
    /// an unconditional `true` passes every positive row there is.
    ///
    /// The block filler is a *nested* one on purpose: a plain block holding a
    /// `forall` exercises the recursion as well as the block kind, which a
    /// `forall` handed directly to the slot would not.
    ///
    /// Fails the moment a statement arm stops descending, or starts answering
    /// without looking — the walk takes no wildcard, so a *new* statement kind
    /// is a compile error here rather than a silent `false`, and this table is
    /// what covers the kinds that already exist.
    #[test]
    fn every_statement_slot_is_descended() {
        let mut arena = AstArena::default();
        let plain = block(&mut arena, BlockKind::Regular, Vec::new());
        let plain_value = number(&mut arena);

        let forall = block(&mut arena, BlockKind::Forall, Vec::new());
        let held = stmt(&mut arena, Stmt::Block(forall));
        let nested = block(&mut arena, BlockKind::Regular, vec![held]);
        let deep = uzumaki_under_an_operator(&mut arena);

        let held = stmt(&mut arena, Stmt::Block(plain));
        let nested_plain = block(&mut arena, BlockKind::Regular, vec![held]);

        for (expected, rows) in [
            (true, statement_rows(deep, nested, plain_value, plain)),
            (
                false,
                statement_rows(plain_value, nested_plain, plain_value, plain),
            ),
        ] {
            for (label, kind) in rows {
                let mut arena = arena.clone();
                let held = stmt(&mut arena, kind);
                let owner = function_with_stmts(&mut arena, "holder", vec![held]);
                assert_eq!(
                    arena.def_contains_non_det_anywhere(owner),
                    expected,
                    "{label} must answer {expected}"
                );
            }
        }
    }

    /// A `let` initializer and a local `const` initializer are both descended,
    /// and neither answers `true` when what it holds is ordinary code.
    ///
    /// The `const` is the one arm that reaches a *definition* from inside a
    /// body, so it is the arm a walk written over statements alone loses.
    ///
    /// Both spellings are driven twice, as every sibling table here is: a pass
    /// that only ever expects `true` is passed by an arm hard-wired to it, and
    /// these two slots are the ones `statement_rows` deliberately leaves out.
    ///
    /// Fails if either initializer stops being read, or starts answering without
    /// reading it.
    #[test]
    fn a_binding_initializer_is_descended_through_either_spelling() {
        use crate::nodes::{DefData, Ident, SimpleTypeKind, TypeData, TypeNode, Visibility};

        let mut arena = AstArena::default();
        let ty = arena.types.alloc(TypeData {
            location: Location::default(),
            kind: TypeNode::Simple(SimpleTypeKind::I32),
        });
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: String::from("n"),
        });

        for (expected, what) in [(true, "an `@` under an operator"), (false, "a literal")] {
            let value = if expected {
                uzumaki_under_an_operator(&mut arena)
            } else {
                number(&mut arena)
            };
            let binding = stmt(
                &mut arena,
                Stmt::VarDef {
                    name,
                    ty,
                    value: Some(value),
                    is_mut: false,
                },
            );
            let owner = function_with_stmts(&mut arena, "lets", vec![binding]);
            assert_eq!(
                arena.def_contains_non_det_anywhere(owner),
                expected,
                "a `let` initializer holding {what} must answer {expected}"
            );

            let value = if expected {
                uzumaki_under_an_operator(&mut arena)
            } else {
                number(&mut arena)
            };
            let constant = arena.defs.alloc(DefData {
                location: Location::default(),
                kind: Def::Constant {
                    name,
                    vis: Visibility::Private,
                    ty,
                    value,
                },
            });
            let held = stmt(&mut arena, Stmt::ConstDef(constant));
            let owner = function_with_stmts(&mut arena, "consts", vec![held]);
            assert_eq!(
                arena.def_contains_non_det_anywhere(owner),
                expected,
                "a local `const` initializer holding {what} must answer {expected}"
            );
        }
    }

    /// Requires the deep walk to answer `expected` for each row's expression,
    /// returned from a function.
    ///
    /// Both directions are driven, from the same row builder, because a table
    /// that only ever expects `true` is passed by an arm mutated to answer
    /// `true` without looking.
    fn each_expression_row_answers(arena: &AstArena, expected: bool, rows: Vec<(&str, Expr)>) {
        for (label, kind) in rows {
            let mut arena = arena.clone();
            let held = push(&mut arena, kind);
            let returned = stmt(&mut arena, Stmt::Return { expr: held });
            let owner = function_with_stmts(&mut arena, "holder", vec![returned]);
            assert_eq!(
                arena.def_contains_non_det_anywhere(owner),
                expected,
                "{label} must answer {expected}"
            );
        }
    }

    /// Every expression node that wraps exactly one other expression is
    /// descended.
    ///
    /// A table for the reason the statement one is: the arms are many, they are
    /// alike, and a walk that lost one would still pass a test written around
    /// the arms somebody happened to think of. These five share a `match` arm,
    /// so what the table really guards is that none of them leaves it.
    ///
    /// Fails if any wrapping arm stops descending, or starts answering without
    /// descending — each row is driven a second time with an ordinary operand
    /// in the same slot, which an arm hard-wired to `true` fails.
    #[test]
    fn every_single_operand_expression_wrapper_is_descended() {
        use crate::nodes::Ident;

        let mut arena = AstArena::default();
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: String::from("f"),
        });
        let inner = push(&mut arena, Expr::Uzumaki);
        let plain = number(&mut arena);

        each_expression_row_answers(&arena, true, single_operand_rows(inner, name));
        each_expression_row_answers(&arena, false, single_operand_rows(plain, name));
    }

    /// One row per expression node that wraps exactly one other expression, each
    /// wrapping `inner`.
    fn single_operand_rows(inner: ExprId, name: IdentId) -> Vec<(&'static str, Expr)> {
        use crate::nodes::UnaryOperatorKind;

        vec![
            (
                "a prefix operator's operand",
                Expr::PrefixUnary {
                    expr: inner,
                    op: UnaryOperatorKind::Neg,
                },
            ),
            ("a parenthesization", Expr::Parenthesized { expr: inner }),
            (
                "an arithmetic-mode annotation",
                Expr::ArithMode {
                    mode: ArithMode::Wrapping,
                    expr: inner,
                },
            ),
            ("a field access", Expr::MemberAccess { expr: inner, name }),
            (
                "a type member access",
                Expr::TypeMemberAccess { expr: inner, name },
            ),
        ]
    }

    /// Every slot of an expression node that holds more than one is descended,
    /// each driven on its own.
    ///
    /// Split from the wrappers above because these arms have to walk *all* of
    /// their operands, which is a second way to be wrong: a `Binary` arm reading
    /// only its left operand, or a call reading only its callee, passes a table
    /// that puts the construct in the first slot every time. Each node kind here
    /// therefore appears once per slot.
    ///
    /// Fails if any slot stops being read, or starts answering without reading
    /// it — the deterministic pass over the same rows is what says the arm
    /// looked.
    #[test]
    fn every_multi_operand_expression_slot_is_descended() {
        use crate::nodes::Ident;

        let mut arena = AstArena::default();
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: String::from("f"),
        });
        let inner = push(&mut arena, Expr::Uzumaki);
        let plain = number(&mut arena);
        let callee = push(&mut arena, Expr::Identifier(name));

        each_expression_row_answers(&arena, true, multi_operand_rows(inner, plain, callee, name));
        each_expression_row_answers(&arena, false, multi_operand_rows(plain, plain, callee, name));
    }

    /// One row per slot of an expression node that holds more than one, with
    /// `inner` in the slot the row is about and `plain` in the others.
    fn multi_operand_rows(
        inner: ExprId,
        plain: ExprId,
        callee: ExprId,
        name: IdentId,
    ) -> Vec<(&'static str, Expr)> {
        use crate::nodes::OperatorKind;

        vec![
            (
                "the left of a binary operator",
                Expr::Binary {
                    left: inner,
                    right: plain,
                    op: OperatorKind::Add,
                },
            ),
            (
                "the right of a binary operator",
                Expr::Binary {
                    left: plain,
                    right: inner,
                    op: OperatorKind::Add,
                },
            ),
            (
                "a callee",
                Expr::FunctionCall {
                    function: inner,
                    type_params: Vec::new(),
                    args: Vec::new(),
                },
            ),
            (
                "a call argument",
                Expr::FunctionCall {
                    function: callee,
                    type_params: Vec::new(),
                    args: vec![(None, inner)],
                },
            ),
            (
                "an indexed array",
                Expr::ArrayIndexAccess {
                    array: inner,
                    index: plain,
                },
            ),
            (
                "an index",
                Expr::ArrayIndexAccess {
                    array: plain,
                    index: inner,
                },
            ),
            (
                "a struct literal field",
                Expr::StructLiteral {
                    name,
                    fields: vec![(name, inner)],
                },
            ),
            (
                "an array literal element",
                Expr::ArrayLiteral {
                    elements: vec![plain, inner],
                },
            ),
        ]
    }

    /// A program with nothing non-deterministic in it answers `false`, however
    /// deeply it nests.
    ///
    /// The tables above drive each arm in both directions one node at a time.
    /// This is the composed shape they do not build: a loop around a block
    /// around a return around a parenthesized sum, which is the arrangement an
    /// ordinary function actually has.
    ///
    /// Fails if the walk ever answers `true` for a deterministic body.
    #[test]
    fn a_deterministic_body_is_not_reported_however_deep() {
        use crate::nodes::OperatorKind;

        let mut arena = AstArena::default();
        let left = number(&mut arena);
        let right = number(&mut arena);
        let sum = push(
            &mut arena,
            Expr::Binary {
                left,
                right,
                op: OperatorKind::Add,
            },
        );
        let wrapped = push(&mut arena, Expr::Parenthesized { expr: sum });
        let returned = stmt(&mut arena, Stmt::Return { expr: wrapped });
        let inner = block(&mut arena, BlockKind::Regular, vec![returned]);
        let nested = stmt(&mut arena, Stmt::Block(inner));
        let body = block(&mut arena, BlockKind::Regular, vec![nested]);
        let looped = stmt(
            &mut arena,
            Stmt::Loop {
                condition: None,
                body,
            },
        );
        let owner = function_with_stmts(&mut arena, "arithmetic", vec![looped]);

        assert!(!arena.def_contains_non_det_anywhere(owner));
        assert_eq!(arena.first_non_det_def_deep(&[owner]), None);
    }

    /// The deep walk descends a struct's methods and stops at a `spec`, exactly
    /// as the shallow one does.
    ///
    /// The two carve-outs are the reason the walk is not simply "look
    /// everywhere": a method is shipped code in no file's `defs` list, and a
    /// specification is not shipped code at all. A deep walk that descended a
    /// `spec` would refuse every program that writes one — and it is deeper than
    /// the shallow walk, so it would do so more often.
    ///
    /// A third definition is passed over for a reason of its own, and the last
    /// row is the one place the two walks disagree deliberately: a module-scope
    /// `const` whose initializer holds an `@` is `None` from the walk the gate
    /// calls and `true` from the walk under it. Code generation drops such a
    /// declaration rather than lowering it, so it ships no instruction to be
    /// wrong about; what would make the asymmetry wrong is emission learning to
    /// lower one, and this row is where the decision is written down so a reader
    /// unifying the two has to come past it.
    ///
    /// Fails if the `Def::Struct` arm is dropped, if a `Def::Spec` arm is added,
    /// or if a `Def::Constant` arm is added to the walk the gate calls without
    /// the emission change that would justify it.
    #[test]
    fn the_deep_walk_keeps_both_of_the_shallow_walk_s_carve_outs() {
        use crate::nodes::{DefData, Ident, SimpleTypeKind, TypeData, TypeNode, Visibility};

        let mut arena = AstArena::default();

        let deep = uzumaki_under_an_operator(&mut arena);
        let returned = stmt(&mut arena, Stmt::Return { expr: deep });
        let method = function_with_stmts(&mut arena, "hidden", vec![returned]);
        let owner = struct_with_methods(&mut arena, "Holder", vec![method]);
        assert_eq!(
            arena.first_non_det_def_deep(&[owner]),
            Some(method),
            "the offending method is what a diagnostic has to name, not its struct"
        );

        let inside = function(&mut arena, "inside", true);
        let spec = spec_with_defs(&mut arena, "properties", vec![inside]);
        assert_eq!(arena.first_non_det_def_deep(&[spec]), None);
        assert!(
            !arena.def_contains_non_det_anywhere(spec),
            "a specification contributes no instruction, so it contributes no forbidden one"
        );

        let method_spec = spec_with_defs(&mut arena, "nested", vec![inside]);
        let owner = struct_with_methods(&mut arena, "Owner", vec![method_spec]);
        assert_eq!(
            arena.first_non_det_def_deep(&[owner]),
            None,
            "a spec reached through a struct is still a spec"
        );

        let ty = arena.types.alloc(TypeData {
            location: Location::default(),
            kind: TypeNode::Simple(SimpleTypeKind::I32),
        });
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: String::from("K"),
        });
        let value = uzumaki_under_an_operator(&mut arena);
        let konst = arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Constant {
                name,
                vis: Visibility::Public,
                ty,
                value,
            },
        });
        assert_eq!(
            arena.first_non_det_def_deep(&[konst]),
            None,
            "a module-scope `const` is dropped by emission, so the gate has nothing to refuse"
        );
        assert!(
            arena.def_contains_non_det_anywhere(konst),
            "the walk under it answers about the initializer expression, and this \
             disagreement is what the carve-out above is"
        );
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
