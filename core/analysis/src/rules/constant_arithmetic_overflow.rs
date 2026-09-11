//! A052: arithmetic whose operands are constants must fit the type it is
//! performed at.
//!
//! `+`, `-`, `*` and unary `-` trap when their result leaves the operand type,
//! so an operation whose operands are known before the program runs and whose
//! result does not fit is not a value at all — it is a trap taken on every run
//! that reaches it, and no `assume`, envelope or specification can recover a
//! result the type cannot hold. Reporting it at compile time is the difference
//! between a diagnostic naming the expression and a program that stops at a
//! machine instruction.
//!
//! An operator inside a `wrapping(...)` is exempt and folds modularly, because
//! there the wrapped value is the answer the author asked for. After this rule
//! `wrapping(2147483647 + 1)` is the only way to write a constant that wraps,
//! which is why the fold looks *through* the annotation rather than refusing to
//! enter it.
//!
//! ## What folds
//!
//! Literals, `const` bindings of the same function body, parentheses,
//! arithmetic-mode annotations and nested arithmetic. A `let` binding does not:
//! it is a variable, and a rule that folded one would be reporting a value the
//! language does not promise stays put. Neither does a call — the annotation
//! does not reach into a callee and neither does this.
//!
//! A body's `const` bindings are collected into one flat map because they share
//! one flat namespace: A041 rejects a body that declares a name twice, and the
//! type checker rejects a body local that shadows a parameter, so a name in that
//! map names exactly one binding wherever it is read.
//!
//! ## The bodies with no run to trap
//!
//! A `forall`-quantified specification function's body, and an unquantified
//! one's, are turned into obligation *terms* and never lowered: the translator
//! emits no instruction for either, in either compilation mode, and a term's
//! `+` is the machine's modular operator with no trap in it. The finding this
//! rule states — a trap taken on every run that reaches the operation — is
//! therefore false of them, so they are skipped. The line is the body's own
//! quantifier, keyed the way the proof-mode rules key theirs and never on
//! lexical containment in `spec { }`: an `exists`/`unique` body *is* compiled
//! and reduced, so it is examined like any other, and so is a function outside
//! the `spec` block that a specification merely calls. Marking the arithmetic
//! of a retained body `wrapping(...)` — which the reachability rules require of
//! it anyway — makes it fold modularly and silences this rule for the same
//! reason it silences it anywhere else.
//!
//! ## Where the line with A022 falls
//!
//! A022 owns a literal that does not fit the type it is measured at. This rule
//! folds nothing that contains one, so `let x: u8 = 300 + 1;` is one finding,
//! A022's, on `300`. The handoff reads
//! [`walker::literal_leaves_range`](crate::walker::literal_leaves_range) rather
//! than restating the comparison, so the rule that steps aside cannot start
//! stepping aside for a shape the other has stopped covering. Everything the
//! fold does produce is therefore in range, which is what lets the message tell
//! a reader that both operands fit and the operation does not.
//!
//! The reproducer this rule is often expected to catch — a fixed-point multiply
//! whose product leaves `i64` — is not one of its findings: its operands are
//! parameters, and nothing folds. That overflow is the guard's to catch at run
//! time.

use std::ops::RangeInclusive;

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, ExprId, NodeId};
use inference_ast::nodes::{ArithMode, Def, Expr, GuardedOp, Stmt};
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    errors::{AnalysisDiagnostic, ExactValue, FoldedOperands, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Constant arithmetic must fit the type it is performed at.
    #[id = "A052"]
    #[name = "Constant arithmetic overflow"]
    #[severity = error]
    pub struct ConstantArithmeticOverflow;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            let module_path = &source_file.module_path;
            walker::for_each_lowered_function_body(arena, &source_file.defs, &mut |body_id| {
                let constants = body_constants(arena, body_id);
                walker::walk_block_stmts(arena, body_id, &mut |stmt_id| {
                    walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                        walker::walk_expr_with_arith_mode(
                            arena,
                            expr_id,
                            None,
                            &mut |node, enclosing| {
                                if walker::arith_mode_under(arena, enclosing)
                                    != ArithMode::Checked
                                {
                                    return;
                                }
                                let mut folder = Folder::new(ctx, &constants);
                                let Some(operation) = folder.overflowing_operation(node) else {
                                    return;
                                };
                                errors.push(LabeledDiagnostic::new(
                                    module_path.clone(),
                                    AnalysisDiagnostic::ConstantArithmeticOverflow {
                                        expression: operation.source,
                                        operands: operation.operands,
                                        op: operation.op,
                                        number: operation.number,
                                        exact: operation.exact,
                                        wrapped: operation.wrapped,
                                        location: arena[node].location,
                                    },
                                ));
                            },
                        );
                    });
                });
            });
        }
        errors
    }
}

/// Every `const` a function body declares, by name.
///
/// Flat and order-blind, which is exact here: A041 rejects a body that declares
/// one name twice — in disjoint blocks included — and the type checker rejects
/// a local that shadows a parameter, so a name resolves to one binding wherever
/// it is read. The descent mirrors A041's own, since the map is only as sound as
/// the walk that rule holds the body to.
fn body_constants(arena: &AstArena, body_id: BlockId) -> FxHashMap<String, ExprId> {
    let mut constants = FxHashMap::default();
    walker::walk_block_stmts(arena, body_id, &mut |stmt_id| {
        if let Stmt::ConstDef(def_id) = &arena[stmt_id].kind
            && let Def::Constant { name, value, .. } = &arena[*def_id].kind
        {
            constants.insert(arena[*name].name.clone(), *value);
        }
    });
    constants
}

/// A constant an expression folds to, with the expression as the source spells
/// it.
///
/// The two travel together because a message has to name both: the value is
/// what the operation computed with, and the spelling is what the author has to
/// find in the file. Rendering only the folded form would show an expression
/// nobody wrote.
struct Folded {
    value: i128,
    source: String,
}

/// A governed operator whose operands folded, before its result is measured.
struct Operation {
    op: GuardedOp,
    number: NumberType,
    operands: FoldedOperands,
    source: String,
    /// The result in the mathematical integers, which is the point: it is exact
    /// whatever the width, and the range question comes afterwards.
    exact: ExactValue,
}

/// A governed operator whose result leaves its type, with both answers.
struct Overflow {
    op: GuardedOp,
    number: NumberType,
    operands: FoldedOperands,
    source: String,
    exact: ExactValue,
    wrapped: i128,
}

/// Folds constant arithmetic within one function body.
///
/// `visiting` holds the `const` initializers currently being folded, so a cycle
/// among them ends the recursion instead of the stack. The type checker refuses
/// a circular definition, but a rule that reads the arena directly must not
/// depend on an earlier pass to stay finite.
struct Folder<'a> {
    ctx: &'a TypedContext,
    constants: &'a FxHashMap<String, ExprId>,
    visiting: FxHashSet<ExprId>,
}

impl<'a> Folder<'a> {
    fn new(ctx: &'a TypedContext, constants: &'a FxHashMap<String, ExprId>) -> Self {
        Self {
            ctx,
            constants,
            visiting: FxHashSet::default(),
        }
    }

    /// The finding at `expr_id`, or `None` when there is none to report.
    ///
    /// Effectively-checked mode is the caller's question; this one is whether
    /// the operands are constants and the result leaves the type. An operand
    /// that itself overflows does not fold, so a chain of operators reports its
    /// innermost overflow and nothing above it: the outer operation has no
    /// constant to compute with, which is exactly true — the program never
    /// reaches it.
    fn overflowing_operation(&mut self, expr_id: ExprId) -> Option<Overflow> {
        let operation = self.operation(expr_id, ArithMode::Checked)?;
        let range = operation.number.range();
        if value_in_range(&range, operation.exact).is_some() {
            return None;
        }
        Some(Overflow {
            op: operation.op,
            number: operation.number,
            operands: operation.operands,
            source: operation.source,
            exact: operation.exact,
            wrapped: wrap_exact_into(&range, operation.exact),
        })
    }

    /// The governed operator at `expr_id` with its operands folded under `mode`,
    /// or `None` when it is not one or an operand does not fold.
    fn operation(&mut self, expr_id: ExprId, mode: ArithMode) -> Option<Operation> {
        let (op, number) = walker::governed_operator_at(self.ctx, expr_id)?;
        let arena = self.ctx.arena();
        match &arena[expr_id].kind {
            Expr::Binary { left, right, .. } => {
                let (left, right) = (*left, *right);
                let left = self.fold(left, mode)?;
                let right = self.fold(right, mode)?;
                let exact = match op {
                    GuardedOp::Add => left.value.checked_add(right.value).map(ExactValue::Narrow),
                    GuardedOp::Sub => left.value.checked_sub(right.value).map(ExactValue::Narrow),
                    GuardedOp::Mul => Some(exact_product(left.value, right.value)),
                    // The operator was read off this same node, so reaching here
                    // would mean a binary expression writing a unary operator.
                    GuardedOp::Neg => None,
                }?;
                Some(Operation {
                    op,
                    number,
                    operands: FoldedOperands::Binary(left.value, right.value),
                    source: format!("{} {} {}", left.source, op.spelling(), right.source),
                    exact,
                })
            }
            Expr::PrefixUnary { expr, .. } => {
                let operand = self.fold(*expr, mode)?;
                let exact = ExactValue::Narrow(operand.value.checked_neg()?);
                Some(Operation {
                    op,
                    number,
                    operands: FoldedOperands::Unary(operand.value),
                    source: format!("{}{}", op.spelling(), operand.source),
                    exact,
                })
            }
            _ => None,
        }
    }

    /// The constant `expr_id` folds to under `mode`, or `None` when it is not
    /// constant.
    ///
    /// `mode` governs the arithmetic *inside* the expression: a `wrapping(...)`
    /// region computes the wrapped value and a checked one computes nothing at
    /// all when its result leaves the type, since there is no such value for an
    /// enclosing operation to use.
    fn fold(&mut self, expr_id: ExprId, mode: ArithMode) -> Option<Folded> {
        let arena = self.ctx.arena();
        match &arena[expr_id].kind {
            Expr::NumberLiteral { value } => {
                let TypeInfoKind::Number(number) =
                    self.ctx.get_node_typeinfo(NodeId::Expr(expr_id))?.kind
                else {
                    return None;
                };
                if walker::literal_leaves_range(value, number) {
                    return None;
                }
                Some(Folded {
                    value: value.parse().ok()?,
                    source: value.clone(),
                })
            }
            Expr::Identifier(ident_id) => {
                let initializer = *self.constants.get(&arena[*ident_id].name)?;
                let source = arena[*ident_id].name.clone();
                if !self.visiting.insert(initializer) {
                    return None;
                }
                // A `const` initializer is a statement of its own: the
                // annotation at a use site is lexical over its own parentheses
                // and does not reach back into the declaration.
                let folded = self.fold(initializer, ArithMode::DEFAULT);
                self.visiting.remove(&initializer);
                Some(Folded {
                    value: folded?.value,
                    source,
                })
            }
            Expr::Parenthesized { expr } => {
                let inner = self.fold(*expr, mode)?;
                Some(Folded {
                    value: inner.value,
                    source: format!("({})", inner.source),
                })
            }
            Expr::ArithMode { mode: written, expr } => {
                let (written, expr) = (*written, *expr);
                let inner = self.fold(expr, written)?;
                Some(Folded {
                    value: inner.value,
                    source: format!("{}({})", written.spelling(), inner.source),
                })
            }
            Expr::Binary { .. } | Expr::PrefixUnary { .. } => {
                let operation = self.operation(expr_id, mode)?;
                let range = operation.number.range();
                let value = match value_in_range(&range, operation.exact) {
                    Some(value) => value,
                    None if mode == ArithMode::Wrapping => {
                        wrap_exact_into(&range, operation.exact)
                    }
                    None => return None,
                };
                Some(Folded {
                    value,
                    source: operation.source,
                })
            }
            _ => None,
        }
    }
}

/// The exact product of two folded operands.
///
/// A product `i128` cannot hold is not an operation this rule cannot measure: it
/// is proof of one that overflows every width there is. Both operands are inside
/// a machine width, so a product that large needs both magnitudes above `2^63`,
/// and `u64` is the only width that reaches there — which also means both
/// operands are non-negative, so the `u128` below holds the product exactly and
/// the message can name it. The same bound is why nothing is lost on the way:
/// two values under `2^64` have a product under `2^128`, so the multiplication
/// has nothing to saturate at and there is no sign for the magnitudes to drop.
fn exact_product(left: i128, right: i128) -> ExactValue {
    match left.checked_mul(right) {
        Some(product) => ExactValue::Narrow(product),
        None => ExactValue::Wide(left.unsigned_abs().saturating_mul(right.unsigned_abs())),
    }
}

/// `exact` when `range` holds it, which is this rule's one question about a
/// result.
///
/// A wide result is above `i128::MAX` and the widest range here ends at
/// `2^64 - 1`, so no range holds one: it is an overflow wherever it is measured,
/// and there is no value for an enclosing checked operation to fold with.
fn value_in_range(range: &RangeInclusive<i128>, exact: ExactValue) -> Option<i128> {
    match exact {
        ExactValue::Narrow(value) => range.contains(&value).then_some(value),
        ExactValue::Wide(_) => None,
    }
}

/// `exact` reduced into `range` the way the machine's own operator reduces it.
///
/// A wide result is reduced modulo the range's span first, which leaves a
/// non-negative value below `2^64` for [`wrap_into`]: reducing modulo the span
/// twice is reducing once, so the answer is the representative every other
/// result gets, and the conversion that follows is exact rather than a
/// narrowing, which is what lets it be written as an addition that cannot
/// overflow.
fn wrap_exact_into(range: &RangeInclusive<i128>, exact: ExactValue) -> i128 {
    match exact {
        ExactValue::Narrow(value) => wrap_into(range, value),
        ExactValue::Wide(value) => {
            let modulus = range.end().abs_diff(*range.start()) + 1;
            wrap_into(range, 0_i128.wrapping_add_unsigned(value % modulus))
        }
    }
}

/// `value` reduced into `range` the way the machine's own operator reduces it:
/// modulo the number of values the width has, counted from its minimum.
///
/// One formula for both signednesses — an unsigned width counts from zero and a
/// signed one from its negative minimum, and the arithmetic is the same once the
/// minimum is subtracted out.
///
/// Total, and returning `i128` rather than an `Option` says so: a fallible
/// signature here would make a width this function cannot reduce look possible,
/// and the caller would have to decide what to do with a `None` that means
/// nothing. `range` is one of the eight machine widths, so its span is at most
/// `2^64` and its minimum at least `-2^63`; `value` arrives from
/// [`wrap_exact_into`] as either a `checked_*` result over two operands each
/// already inside such a range, whose magnitude is at most `2^126`, or a
/// remainder below `2^64`. Every intermediate below therefore stays inside
/// `i128`, and the modulus is positive, so the remainder cannot divide by zero
/// either.
fn wrap_into(range: &RangeInclusive<i128>, value: i128) -> i128 {
    let min = *range.start();
    let modulus = range.end() - min + 1;
    (value - min).rem_euclid(modulus) + min
}
