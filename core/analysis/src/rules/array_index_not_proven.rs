//! A056: a dynamic array index must be proven in bounds.
//!
//! Every array access whose index is not a bare number literal carries a
//! runtime guard that traps on an out-of-range index. A trap is a halt, not a
//! recovery, so this rule requires the program to make that guard unreachable
//! itself: the range analysis in `src/range.rs` has to prove, from the code
//! the programmer wrote, that the index lies in `0..length` on every run that
//! reaches the access. An out-of-range index is then a path the program
//! handles — an `else`, an early `return`, a loop's exit — never an implicit
//! trap.
//!
//! What proves an access is whatever the analysis can follow: the condition of
//! an enclosing `if` or `loop`, the left operand of a short-circuiting `&&`, an
//! early exit, an initializer and the arithmetic applied to it since, or the
//! index's type alone (a `u8` index into a `[T; 256]`). An `assert` proves
//! nothing: it establishes its condition only by halting the program when it
//! does not hold, which is the outcome this rule exists to rule out. Only a
//! plain local is narrowed by a condition, so an index read from a field or an
//! element is proven only by its type, and the fix is to bind it to a local
//! first.
//!
//! The rule is intraprocedural, since the language has no preconditions: a
//! parameter can hold any value of its type at entry, and a function indexing
//! with one guards it itself. A specification envelope does not stand in for
//! that guard — it describes only the calls the specification makes, while any
//! other caller can still pass an index the guard traps on.
//!
//! The runtime guard stays: this rule proves it unreachable, and the guard is
//! still what an analysis bug would fall back to.
//!
//! A literal index is A037's (it folds and range-checks it) and a 64-bit index
//! is A019's; neither is reported here. The bodies examined are the ones code
//! generation lowers, since a `forall` body never runs.

use inference_ast::arena::AstArena;
use inference_ast::ids::{ExprId, NodeId};
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    range::{self, BodyRanges, IndexReach, Interval},
    walker,
};

crate::rule! {
    /// A dynamic array index must be proven to lie within the array's bounds.
    #[id = "A056"]
    #[name = "Array index not proven in bounds"]
    #[severity = error]
    pub struct ArrayIndexNotProven;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            let module_path = &source_file.module_path;
            walker::for_each_lowered_function_body(arena, &source_file.defs, &mut |body_id| {
                let ranges = range::analyze_body(ctx, body_id);
                walker::walk_block_stmts(arena, body_id, &mut |stmt_id| {
                    walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
                        walker::walk_expr(arena, expr_id, &mut |node| {
                            if let Some(diagnostic) = unproven_access(ctx, &ranges, node) {
                                errors.push(LabeledDiagnostic::new(module_path.clone(), diagnostic));
                            }
                        });
                    });
                });
            });
        }
        errors
    }
}

/// The diagnostic for `node` when it is an array access this rule covers and
/// the analysis did not prove its index in bounds.
///
/// The accesses are enumerated here, independently of the analysis, so an
/// access the analysis never reached is reported rather than silently passed.
fn unproven_access(
    ctx: &TypedContext,
    ranges: &BodyRanges,
    node: ExprId,
) -> Option<AnalysisDiagnostic> {
    let arena = ctx.arena();
    let Expr::ArrayIndexAccess { array, index } = &arena[node].kind else {
        return None;
    };
    if matches!(arena[*index].kind, Expr::NumberLiteral { .. }) {
        return None;
    }
    let TypeInfoKind::Array(_, length) = ctx.get_node_typeinfo(NodeId::Expr(*array))?.kind else {
        return None;
    };
    // Zero is the length the type checker records for a size it rejected.
    if length == 0 {
        return None;
    }
    let TypeInfoKind::Number(number) = ctx.get_node_typeinfo(NodeId::Expr(*index))?.kind else {
        return None;
    };
    if matches!(number, NumberType::I64 | NumberType::U64) {
        return None;
    }
    let range = match ranges.indices.get(&node) {
        Some(IndexReach::Unreachable) => return None,
        Some(IndexReach::Within(range)) => *range,
        None => Interval::of_type(number),
    };
    let in_bounds = range.lo >= 0 && range.hi < i128::from(length);
    (!in_bounds).then(|| AnalysisDiagnostic::ArrayIndexNotProvenInBounds {
        index: local_name(arena, *index),
        number,
        length,
        lo: range.lo,
        hi: range.hi,
        location: arena[node].location,
    })
}

/// The name of the local `index` reads, looking through parentheses, or `None`
/// when it is any other expression.
fn local_name(arena: &AstArena, index: ExprId) -> Option<String> {
    match &arena[arena.peel_transparent(index)].kind {
        Expr::Identifier(ident) => Some(arena[*ident].name.clone()),
        _ => None,
    }
}
