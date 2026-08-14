//! A syntactic scan for the construct in a body that *marks* an intent to state
//! a property.
//!
//! This exists to pick the right diagnostic wording — it never decides whether
//! an obligation is emitted. That decision belongs to the translated result
//! (`hassert == HAssert::True`), which is exact because the ⊤-absorbing smart
//! constructors collapse every vacuous body to exactly `HA_true`. A mistake
//! here can therefore only mis-word a message, never drop a claim.
//!
//! The scan is deliberately shallow: it reports the *first* marker in source
//! order and says nothing about whether that marker actually constrains
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

/// The first claim marker in the body of `def_id`, in source order, or `None`
/// for a body that only computes.
pub(super) fn first_claim(arena: &AstArena, def_id: DefId) -> Option<Claim> {
    let Def::Function { body, .. } = &arena[def_id].kind else {
        return None;
    };
    let kind = arena[*body].block_kind;
    if kind != BlockKind::Regular {
        return Some(Claim::Quantifier(kind));
    }
    in_stmts(arena, &arena[*body].stmts)
}

fn in_stmts(arena: &AstArena, stmts: &[StmtId]) -> Option<Claim> {
    stmts.iter().find_map(|&stmt| in_stmt(arena, stmt))
}

/// The `Stmt` match is exhaustive by design: a new statement kind must decide
/// here whether it can carry a claim rather than defaulting to "cannot".
fn in_stmt(arena: &AstArena, stmt: StmtId) -> Option<Claim> {
    match &arena[stmt].kind {
        Stmt::Assert { .. } => Some(Claim::Assert),
        Stmt::Block(block) => in_block(arena, *block),
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            // The condition is skipped on purpose: a condition guards a claim,
            // it never is one.
            in_block(arena, *then_block)
                .or_else(|| else_block.and_then(|block| in_block(arena, block)))
        }
        // Unreachable in a specification body (`loop` is `P002`), but keeping
        // the walk total costs one arm.
        Stmt::Loop { body, .. } => in_block(arena, *body),
        Stmt::Return { .. }
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::ConstDef(_)
        | Stmt::TypeDef { .. }
        | Stmt::Assign { .. }
        | Stmt::Break => None,
    }
}

/// A nested block is itself a claim when it is non-deterministic; a `Regular`
/// one only groups statements, so the scan descends into it.
fn in_block(arena: &AstArena, block: BlockId) -> Option<Claim> {
    let kind = arena[block].block_kind;
    if kind != BlockKind::Regular {
        return Some(Claim::NondetBlock(kind));
    }
    in_stmts(arena, &arena[block].stmts)
}
