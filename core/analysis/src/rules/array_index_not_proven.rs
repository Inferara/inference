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
//! The accesses this rule proves are also what code generation reads to omit
//! the guard, in a build that chose `BoundsChecks::OmitProven`:
//! `check_and_prove` keeps each access whose index the analysis bounded within
//! `0..length` from the same pass, and the same `verdict`, this rule reports
//! from, so the set a build elides and the set this rule accepts cannot drift
//! apart. An access
//! proven only by never being reached keeps its guard; it costs nothing at run
//! time.
//!
//! A literal index is A037's (it folds and range-checks it) and a 64-bit index
//! is A019's; neither is reported here. The bodies examined are the ones code
//! generation lowers, since a `forall` body never runs.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, ExprId, NodeId};
use inference_ast::nodes::Expr;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashSet;

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
        check_and_prove(ctx).0
    }
}

/// The array accesses whose index the analysis proved to lie within
/// `0..length` on every run that reaches them, keyed by the access expression.
///
/// Such an access's runtime bounds guard can never fire, which is what lets a
/// build that chose `BoundsChecks::OmitProven` omit it. The set is produced
/// only by an analysis that passed under that policy (see
/// [`AnalysisResult::proven_in_bounds`]), so a program that reaches code
/// generation with any access unproven carries none of them.
///
/// [`AnalysisResult::proven_in_bounds`]: crate::errors::AnalysisResult::proven_in_bounds
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProvenInBounds {
    accesses: FxHashSet<ExprId>,
}

impl ProvenInBounds {
    /// Whether `access`, an `ArrayIndexAccess` expression, was proven in bounds.
    #[must_use = "returns whether the access was proven in bounds"]
    pub fn contains(&self, access: ExprId) -> bool {
        self.accesses.contains(&access)
    }

    /// Every access proven in bounds, as the set code generation takes.
    #[must_use = "returns the proven accesses"]
    pub fn accesses(&self) -> &FxHashSet<ExprId> {
        &self.accesses
    }

    /// How many accesses were proven in bounds.
    #[must_use = "returns the number of proven accesses"]
    pub fn len(&self) -> usize {
        self.accesses.len()
    }

    /// Whether no access was proven in bounds.
    #[must_use = "returns whether no access was proven in bounds"]
    pub fn is_empty(&self) -> bool {
        self.accesses.is_empty()
    }
}

/// The rule's findings and every covered access its analysis proves in
/// bounds, from one run of the range analysis over each body with a covered
/// access.
///
/// The rule's own `check` is this with the proofs dropped, which keeps it
/// callable on its own, as the editor calls it; a build that omits proven
/// guards calls this in its place, so it analyzes each body once rather than
/// once for the findings and again for the proofs.
pub(crate) fn check_and_prove(ctx: &TypedContext) -> (Vec<LabeledDiagnostic>, ProvenInBounds) {
    let mut errors = Vec::new();
    let mut accesses = FxHashSet::default();
    let arena = ctx.arena();
    for_each_verdict(ctx, &mut |module_path, access, verdict| match verdict {
        Verdict::InBounds => {
            accesses.insert(access.node);
        }
        Verdict::Unreached => {}
        Verdict::Unproven(range) => errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            unproven(arena, access, range),
        )),
    });
    (errors, ProvenInBounds { accesses })
}

/// What the analysis established about one covered access.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verdict {
    /// Every run that reaches the access indexes within `0..length`.
    InBounds,
    /// No run reaches the access.
    Unreached,
    /// A run that reaches the access may index with a value in this interval,
    /// which leaves `0..length`.
    Unproven(Interval),
}

/// Calls `f` with every covered access in every lowered body, the file it is
/// in, and what the analysis established about it.
fn for_each_verdict(ctx: &TypedContext, f: &mut dyn FnMut(&[String], &CoveredAccess, Verdict)) {
    let arena = ctx.arena();
    for source_file in ctx.source_files() {
        let module_path = &source_file.module_path;
        walker::for_each_lowered_function_body(arena, &source_file.defs, &mut |body_id| {
            let accesses = covered_accesses(ctx, body_id);
            // A body with nothing to prove is not analyzed at all.
            if accesses.is_empty() {
                return;
            }
            let ranges = range::analyze_body(ctx, body_id);
            for access in &accesses {
                f(module_path, access, verdict(&ranges, access));
            }
        });
    }
}

/// An array access this rule covers: its index is neither a bare literal
/// (A037's) nor 64-bit (A019's), and its array has a length the type checker
/// accepted.
struct CoveredAccess {
    node: ExprId,
    index: ExprId,
    number: NumberType,
    length: u32,
}

/// Every access in `body` this rule covers, in source order.
///
/// The accesses are enumerated here, independently of the analysis, so an
/// access the analysis never reached is reported rather than silently passed.
fn covered_accesses(ctx: &TypedContext, body: BlockId) -> Vec<CoveredAccess> {
    let arena = ctx.arena();
    let mut accesses = Vec::new();
    walker::walk_block_stmts(arena, body, &mut |stmt_id| {
        walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
            walker::walk_expr(arena, expr_id, &mut |node| {
                accesses.extend(covered_access(ctx, node));
            });
        });
    });
    accesses
}

fn covered_access(ctx: &TypedContext, node: ExprId) -> Option<CoveredAccess> {
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
    Some(CoveredAccess {
        node,
        index: *index,
        number,
        length,
    })
}

/// What `ranges` establishes about `access`. An access the analysis never
/// met can hold any value of its index type.
fn verdict(ranges: &BodyRanges, access: &CoveredAccess) -> Verdict {
    let range = match ranges.indices.get(&access.node) {
        Some(IndexReach::Unreachable) => return Verdict::Unreached,
        Some(IndexReach::Within(range)) => *range,
        None => Interval::of_type(access.number),
    };
    if range.lo >= 0 && range.hi < i128::from(access.length) {
        Verdict::InBounds
    } else {
        Verdict::Unproven(range)
    }
}

/// The diagnostic for `access`, whose index the analysis bounded only to
/// `range`.
fn unproven(arena: &AstArena, access: &CoveredAccess, range: Interval) -> AnalysisDiagnostic {
    AnalysisDiagnostic::ArrayIndexNotProvenInBounds {
        index: local_name(arena, access.index),
        number: access.number,
        length: access.length,
        lo: range.lo,
        hi: range.hi,
        location: arena[access.node].location,
    }
}

/// The name of the local `index` reads, looking through parentheses, or `None`
/// when it is any other expression.
fn local_name(arena: &AstArena, index: ExprId) -> Option<String> {
    match &arena[arena.peel_transparent(index)].kind {
        Expr::Identifier(ident) => Some(arena[*ident].name.clone()),
        _ => None,
    }
}
