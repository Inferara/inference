//! A042: Non-deterministic constructs are only valid inside a `spec` declaration.
//!
//! The non-deterministic block forms — inline `forall`/`exists`/`assume`/`unique`
//! statement blocks and the function-body-modifier form (`fn f() forall { … }`,
//! which records the kind on the function's body block) — describe formal
//! specifications, not executable code. They are legal only lexically inside a
//! `spec { … }` declaration (spec free functions and spec-inner struct methods).
//! Anywhere else — a top-level function, a top-level struct method, or a block
//! nested inside either — they are rejected here.
//!
//! The check is purely lexical (it never inspects types), so it is independent of
//! the compilation mode and runs in both compile and proof modes. The scan only
//! descends into scopes reachable *without* crossing a `spec`; a `spec` and
//! everything under it is skipped, which is exactly the "allowed" region.
//!
//! Uzumaki (`@`) outside a spec is covered transitively: `@` already requires an
//! enclosing non-deterministic block (A006), and this rule rejects that block
//! when it sits outside a spec, so no separate `@` check lives here.
//!
//! Nested non-det blocks are not cascaded: only the outermost non-det block on
//! each path is reported. A `forall { exists { … } }` outside a spec yields one
//! diagnostic (for the `forall`), matching the "one finding per offending
//! construct" intent — the inner blocks are already illegal by virtue of the
//! outer one and repeating the diagnostic would only add noise.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, StmtId};
use inference_ast::nodes::{Def, Stmt};

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker::block_kind_label,
};

crate::rule! {
    /// Non-deterministic blocks and body modifiers (forall/exists/assume/unique)
    /// are only valid inside a `spec` declaration.
    #[id = "A042"]
    #[name = "Non-det construct outside spec"]
    #[severity = error]
    pub struct NonDetOutsideSpec;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            scan_defs(arena, &source_file.module_path, &source_file.defs, &mut errors);
        }
        errors
    }
}

/// Scans definitions that live lexically outside any `spec`, reporting non-det
/// blocks in their function bodies. `Def::Spec` (and everything under it) is the
/// allowed region, so it is deliberately not descended into.
fn scan_defs(
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { body, .. } => scan_child_block(arena, module_path, *body, errors),
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function { body, .. } = &arena[method_id].kind {
                        scan_child_block(arena, module_path, *body, errors);
                    }
                }
            }
            Def::Spec { .. }
            | Def::Enum { .. }
            | Def::Constant { .. }
            | Def::ExternFunction { .. }
            | Def::TypeAlias { .. } => {}
        }
    }
}

/// Inspects one block. If it is a non-det block, reports it (once) and stops —
/// nested non-det blocks under it are not separately flagged. Otherwise recurses
/// into its statements. This one entry point handles both the inline
/// `Stmt::Block` form and the function-body-modifier form (where a function's own
/// body block carries the non-det kind).
fn scan_child_block(
    arena: &AstArena,
    module_path: &[String],
    block_id: BlockId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let block = &arena[block_id];
    if block.block_kind.is_non_det() {
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::NonDetOutsideSpec {
                location: block.location,
                block_kind: block_kind_label(block.block_kind),
            },
        ));
        return;
    }
    for &stmt_id in &block.stmts {
        scan_stmt(arena, module_path, stmt_id, errors);
    }
}

/// Recurses into the child blocks a statement can introduce (`if` arms, loop
/// bodies, and bare `{ … }` blocks), each inspected via [`scan_child_block`].
fn scan_stmt(
    arena: &AstArena,
    module_path: &[String],
    stmt_id: StmtId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    match &arena[stmt_id].kind {
        Stmt::Block(block_id) => scan_child_block(arena, module_path, *block_id, errors),
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            scan_child_block(arena, module_path, *then_block, errors);
            if let Some(else_id) = else_block {
                scan_child_block(arena, module_path, *else_id, errors);
            }
        }
        Stmt::Loop { body, .. } => scan_child_block(arena, module_path, *body, errors),
        Stmt::Assign { .. }
        | Stmt::Return { .. }
        | Stmt::Break
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::TypeDef { .. }
        | Stmt::Assert { .. }
        | Stmt::ConstDef(_) => {}
    }
}
