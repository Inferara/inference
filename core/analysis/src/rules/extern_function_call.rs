//! A024: Calls to *unbound* external functions are not supported in codegen.
//!
//! An `external fn` bound to a source module via `use { f } from <module>;`
//! lowers to a WASM import that the static-merge linker later satisfies (issue
//! #9), so calling it is fully supported. An *unbound* bare extern — declared
//! `external fn` with no binding `use` — has no source module to merge, would
//! emit no import, and so cannot be compiled. This rule rejects calls to those
//! unbound externs only.
//!
//! Resolution is *scope-aware*, not name-keyed. Two distinct `external fn f`
//! declarations — a bound top-level `f` and an unbound spec-inner `f` — share
//! a name but bind differently: a call inside the spec resolves to the
//! spec-inner declaration, while a call at the top level resolves to the
//! top-level one. The rule resolves each call to the specific `external fn`
//! declaration visible in its enclosing scope and flags it only when *that*
//! declaration is unbound. A purely name-keyed check would let an unbound
//! same-named declaration poison every call to a bound extern (round-2 H-1).
//!
//! Resolution itself is not implemented here. The walk below tracks only
//! *where* each body sits — the file it belongs to and the `spec` enclosing it,
//! if any — and hands that scope to
//! [`ExternIndex`](inference_type_checker::ExternIndex), the whole-program
//! index type checking already built. Sharing that index is what keeps this
//! rule and the specification translator from disagreeing about which
//! declaration a call names.
//!
//! NOTE: This rule only matches direct calls by name (`foo()`). External
//! functions cannot currently be struct members or passed as values, so
//! name-based matching within a scope is sufficient. If the language later
//! allows extern functions in structs or as first-class values, this rule
//! will need to be extended.

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{Def, Expr};
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// Calls to unbound external functions are not supported in codegen.
    #[id = "A024"]
    #[name = "External function call"]
    #[severity = error]
    pub struct ExternFunctionCall;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let arena = ctx.arena();
        let mut errors = Vec::new();
        for source_file in ctx.source_files() {
            check_defs(
                arena,
                ctx,
                &source_file.module_path,
                None,
                &source_file.defs,
                &mut errors,
            );
        }
        errors
    }
}

/// Walks the definitions of one scope, flagging every call in their bodies that
/// resolves to an *unbound* extern.
///
/// A file's top level and a `spec` inside it are the only two places an
/// `external fn` can be declared, so a body's scope is fully described by the
/// file's `module_path` plus `spec` — `None` at the top level, the spec's name
/// inside one. Specs do not nest, so the recursion descends at most one level
/// and sibling specs stay isolated by construction.
fn check_defs(
    arena: &AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    spec: Option<&str>,
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { body, .. } => {
                check_function_body(arena, ctx, module_path, spec, *body, errors);
            }
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function { body, .. } = &arena[method_id].kind {
                        check_function_body(arena, ctx, module_path, spec, *body, errors);
                    }
                }
            }
            Def::Spec { name, defs, .. } => {
                check_defs(
                    arena,
                    ctx,
                    module_path,
                    Some(arena[*name].name.as_str()),
                    defs,
                    errors,
                );
            }
            _ => {}
        }
    }
}

fn check_function_body(
    arena: &AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    spec: Option<&str>,
    body: inference_ast::ids::BlockId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    walker::walk_block_stmts(arena, body, &mut |stmt_id| {
        walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
            walker::walk_expr(arena, expr_id, &mut |sub_id| {
                if let Expr::FunctionCall { function, .. } = &arena[sub_id].kind
                    && let Expr::Identifier(ident_id) = &arena[*function].kind
                {
                    let callee_name = &arena[*ident_id].name;
                    if let Some(decl) = ctx.extern_index().lookup(module_path, spec, callee_name)
                        && ctx.extern_origin_by_decl(decl).is_none()
                    {
                        errors.push(LabeledDiagnostic::new(
                            module_path.to_vec(),
                            AnalysisDiagnostic::ExternFunctionCall {
                                name: callee_name.clone(),
                                location: arena[sub_id].location,
                            },
                        ));
                    }
                }
            });
        });
    });
}
