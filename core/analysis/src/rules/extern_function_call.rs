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
//! NOTE: This rule only matches direct calls by name (`foo()`). External
//! functions cannot currently be struct members or passed as values, so
//! name-based matching within a scope is sufficient. If the language later
//! allows extern functions in structs or as first-class values, this rule
//! will need to be extended.

use std::collections::HashMap;

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
        let mut scopes: Vec<HashMap<&str, DefId>> = Vec::new();
        for source_file in ctx.source_files() {
            check_defs(
                arena,
                ctx,
                &source_file.module_path,
                &source_file.defs,
                &mut scopes,
                &mut errors,
            );
        }
        errors
    }
}

/// Walks the definition tree, maintaining a stack of extern declarations in
/// scope, and flags every call that resolves to an *unbound* extern.
///
/// A file's top level and a `spec` inside it are the only two places an
/// `external fn` can be declared, and each pushes the declarations it introduces
/// as a new scope layer before the bodies at that level are checked, so a
/// spec-inner `external fn` shadows a same-named top-level one for calls inside
/// that spec. The layer is popped on exit, keeping sibling specs isolated from
/// one another. Specs do not nest, so the stack is at most two deep.
fn check_defs<'a>(
    arena: &'a AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    defs: &[DefId],
    scopes: &mut Vec<HashMap<&'a str, DefId>>,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    scopes.push(collect_extern_decls(arena, defs));
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { body, .. } => {
                check_function_body(arena, ctx, module_path, *body, scopes, errors);
            }
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function { body, .. } = &arena[method_id].kind {
                        check_function_body(arena, ctx, module_path, *body, scopes, errors);
                    }
                }
            }
            Def::Spec { defs, .. } => {
                check_defs(arena, ctx, module_path, defs, scopes, errors);
            }
            _ => {}
        }
    }
    scopes.pop();
}

/// Records the `external fn` declarations introduced directly by `defs`,
/// mapping each extern name to its declaring [`DefId`]. Keeps the first
/// declaration for a name; a same-name redeclaration in one scope is a type
/// error caught earlier, so the choice is immaterial to a valid program.
fn collect_extern_decls<'a>(arena: &'a AstArena, defs: &[DefId]) -> HashMap<&'a str, DefId> {
    let mut decls = HashMap::default();
    for &def_id in defs {
        if let Def::ExternFunction { name, .. } = &arena[def_id].kind {
            decls.entry(arena[*name].name.as_str()).or_insert(def_id);
        }
    }
    decls
}

/// Resolves a callee name against the scope stack, innermost first, returning
/// the declaring [`DefId`] of the nearest `external fn` of that name, or `None`
/// if the name does not resolve to any extern in scope (a regular function).
fn resolve_extern_decl(scopes: &[HashMap<&str, DefId>], name: &str) -> Option<DefId> {
    scopes.iter().rev().find_map(|scope| scope.get(name).copied())
}

fn check_function_body(
    arena: &AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    body: inference_ast::ids::BlockId,
    scopes: &[HashMap<&str, DefId>],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    walker::walk_block_stmts(arena, body, &mut |stmt_id| {
        walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
            walker::walk_expr(arena, expr_id, &mut |sub_id| {
                if let Expr::FunctionCall { function, .. } = &arena[sub_id].kind
                    && let Expr::Identifier(ident_id) = &arena[*function].kind
                {
                    let callee_name = &arena[*ident_id].name;
                    if let Some(decl) = resolve_extern_decl(scopes, callee_name)
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
