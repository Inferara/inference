//! A041: A function-local name is declared at most once per function body.
//!
//! Every `let` and `const` binding in a function body shares a single flat
//! namespace. Two declarations of the same name — even in disjoint sibling
//! blocks that never coexist at runtime (the two arms of an `if`, two
//! sequential `if`s, a loop body and a later block, or two non-deterministic
//! blocks) — are rejected here.
//!
//! The type checker treats each lexical block as its own scope, so reusing a
//! name across sibling blocks is *not* shadowing and it is accepted there. Code
//! generation, however, flattens every body local into one name-keyed map (one
//! WebAssembly local per source name), where a repeated name would collide.
//! This rule enforces the flat-namespace contract as a diagnostic instead of a
//! codegen crash.
//!
//! The rationale is simplicity and auditability, **not** proof soundness: the
//! Rocq backend addresses locals by numeric index and is unaffected by names,
//! so either policy yields a sound proof term. What a single flat namespace
//! buys is a 1:1 source-name to WebAssembly-local to proof-index mapping per
//! function, which keeps proofs and traces legible to a human reading them.
//! See issue #217.
//!
//! ## Invariant
//!
//! A041 must flag *exactly* the body-local duplicates that would trip the
//! defense-in-depth collision asserts in wasm-codegen's `pre_scan_locals`. The
//! two walks must agree: if they diverge, either a panic leaks through (A041
//! misses a duplicate) or legal code is rejected (A041 over-flags). To hold
//! that agreement, the descent here mirrors `pre_scan_locals`' pre-order DFS —
//! it recurses through `Stmt::Block`, both arms of `Stmt::If`, and `Stmt::Loop`
//! bodies, treating `VarDef`/`ConstDef` as leaves. Non-deterministic blocks
//! (`forall`/`exists`/`unique`/`assume`) surface as `Stmt::Block` carrying a
//! kind and are descended unconditionally, exactly as codegen descends them.
//!
//! Parameters are out of scope: a body local that reuses a parameter name is
//! already rejected by the type checker (`VariableShadowed`, since parameters
//! sit in an enclosing scope), and analysis only runs on type-checked programs.
//! The accumulator is therefore a flat per-body map with no scope tree — it
//! makes no ancestor/sibling distinction because the type checker has already
//! removed every ancestor collision.

use inference_ast::arena::AstArena;
use inference_ast::ids::{IdentId, StmtId};
use inference_ast::nodes::{Def, Location, Stmt};
use rustc_hash::FxHashMap;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// A function-local name may be declared at most once per function body.
    #[id = "A041"]
    #[name = "Duplicate local name"]
    #[severity = error]
    pub struct DuplicateLocalName;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            let module_path = &source_file.module_path;
            walker::for_each_function_body(arena, &source_file.defs, &mut |body_id| {
                // Fresh per body: names in different functions never interact.
                let mut first_seen: FxHashMap<String, Location> = FxHashMap::default();
                walker::walk_block_stmts(arena, body_id, &mut |stmt_id| {
                    let Some((name_id, location)) = local_declaration(arena, stmt_id) else {
                        return;
                    };
                    let name = arena[name_id].name.clone();
                    if let Some(&first_location) = first_seen.get(&name) {
                        errors.push(LabeledDiagnostic::new(
                            module_path.clone(),
                            AnalysisDiagnostic::DuplicateLocalName {
                                name,
                                location,
                                first_location,
                            },
                        ));
                    } else {
                        first_seen.insert(name, location);
                    }
                });
            });
        }
        errors
    }
}

/// The declared name and the statement's own location for a `let` or `const`
/// statement, or `None` for any other statement.
///
/// Both arms cite `arena[stmt_id].location` so the caret points at the whole
/// statement uniformly — for a `ConstDef` this is the statement's location, not
/// the inner `Def::Constant` node's.
#[must_use = "the extracted declaration drives duplicate detection"]
fn local_declaration(arena: &AstArena, stmt_id: StmtId) -> Option<(IdentId, Location)> {
    let location = arena[stmt_id].location;
    match &arena[stmt_id].kind {
        Stmt::VarDef { name, .. } => Some((*name, location)),
        Stmt::ConstDef(def_id) => match &arena[*def_id].kind {
            Def::Constant { name, .. } => Some((*name, location)),
            _ => None,
        },
        _ => None,
    }
}
