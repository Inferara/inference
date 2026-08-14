//! Syntactic scans over a specification body, answering two different questions
//! about it.
//!
//! [`first_claim`] asks *what did the author write that signals an intent to
//! state a property*, and only ever picks a diagnostic's wording. It classifies
//! a non-deterministic block on sight, without looking inside: for a spec
//! function that is the right reading, because the useful remedy names the block
//! the user wrote — telling the author of `fn f() { forall { } }` to move the
//! function out of the `spec` block would be impossible advice, since a
//! non-deterministic block may not live outside one.
//!
//! [`states_an_assertion`] asks *is an assertion actually lost here*, and
//! decides whether a diagnostic is raised at all. It descends *through* a
//! non-deterministic block and reports only an `assert`, because a block that
//! asserts nothing loses nothing. Answering this question with a claim marker is
//! what would make an empty `forall` look like a stated property.
//!
//! Neither scan decides whether an obligation is emitted. That decision belongs
//! to the translated result (`hassert == HAssert::True`), which is exact because
//! the ⊤-absorbing smart constructors collapse every vacuous body to exactly
//! `HA_true`. A mistake in [`first_claim`] can therefore only mis-word a
//! message, never drop a claim.
//!
//! [`first_claim`] is deliberately shallow: it reports the *first* marker in
//! source order and says nothing about whether that marker actually constrains
//! anything. A body that asserts a tautology still reports [`Claim::Assert`],
//! which is what lets the message distinguish "you asserted nothing" from "you
//! asserted nothing useful".

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, StmtId};
use inference_ast::nodes::{BlockKind, Def, Stmt};

/// What in a specification body claims a property.
pub(super) enum Claim {
    /// The body's own quantifier (`fn f() forall { … }`).
    Quantifier(BlockKind),
    /// An inline non-deterministic block inside a plain body.
    NondetBlock(BlockKind),
    /// An `assert` statement.
    Assert,
}

/// What a nested non-deterministic block means to the scan in progress.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Descent {
    /// It is itself a claim, so the scan reports it and stops there.
    StopAtNondet,
    /// It only groups statements, so the scan keeps looking inside it.
    ThroughNondet,
}

/// The first claim marker in the body of `def_id`, in source order, or `None`
/// for a body that only computes.
pub(super) fn first_claim(arena: &AstArena, def_id: DefId) -> Option<Claim> {
    let body = body_of(arena, def_id)?;
    let kind = arena[body].block_kind;
    if kind != BlockKind::Regular {
        return Some(Claim::Quantifier(kind));
    }
    in_stmts(arena, &arena[body].stmts, Descent::StopAtNondet)
}

/// Whether the body of `def_id` contains an `assert` at any depth, including
/// inside a non-deterministic block.
pub(super) fn states_an_assertion(arena: &AstArena, def_id: DefId) -> bool {
    let Some(body) = body_of(arena, def_id) else {
        return false;
    };
    // Descending through every nested block leaves `Assert` as the only marker
    // the walk can reach, so the scan collapses to a yes or no.
    matches!(
        in_stmts(arena, &arena[body].stmts, Descent::ThroughNondet),
        Some(Claim::Assert)
    )
}

fn body_of(arena: &AstArena, def_id: DefId) -> Option<BlockId> {
    let Def::Function { body, .. } = &arena[def_id].kind else {
        return None;
    };
    Some(*body)
}

fn in_stmts(arena: &AstArena, stmts: &[StmtId], descent: Descent) -> Option<Claim> {
    stmts.iter().find_map(|&stmt| in_stmt(arena, stmt, descent))
}

/// The `Stmt` match is exhaustive by design: a new statement kind must decide
/// here whether it can carry a claim rather than defaulting to "cannot".
fn in_stmt(arena: &AstArena, stmt: StmtId, descent: Descent) -> Option<Claim> {
    match &arena[stmt].kind {
        Stmt::Assert { .. } => Some(Claim::Assert),
        Stmt::Block(block) => in_block(arena, *block, descent),
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            // The condition is skipped on purpose: a condition guards a claim,
            // it never is one.
            in_block(arena, *then_block, descent)
                .or_else(|| else_block.and_then(|block| in_block(arena, block, descent)))
        }
        // Unreachable in a specification body (`loop` is `P002`), but keeping
        // the walk total costs one arm.
        Stmt::Loop { body, .. } => in_block(arena, *body, descent),
        Stmt::Return { .. }
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::ConstDef(_)
        | Stmt::TypeDef { .. }
        | Stmt::Assign { .. }
        | Stmt::Break => None,
    }
}

/// A nested block that only groups statements is always walked into; a
/// non-deterministic one is walked into or reported as the claim itself,
/// depending on the question being asked.
fn in_block(arena: &AstArena, block: BlockId, descent: Descent) -> Option<Claim> {
    let kind = arena[block].block_kind;
    if kind != BlockKind::Regular && descent == Descent::StopAtNondet {
        return Some(Claim::NondetBlock(kind));
    }
    in_stmts(arena, &arena[block].stmts, descent)
}
