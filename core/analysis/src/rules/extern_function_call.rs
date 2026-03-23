//! A024: Calls to external functions are not yet supported in codegen.
//!
//! External functions are declared with `external fn` but the WebAssembly code
//! generator does not yet emit WASM imports for them. Calling an external
//! function would panic during code generation, so the analysis pass rejects
//! such calls with a clear error message.

use std::collections::HashSet;

use inference_ast::arena::AstArena;
use inference_ast::nodes::{Def, Expr};
use inference_type_checker::typed_context::TypedContext;

use crate::{errors::AnalysisDiagnostic, walker};

crate::rule! {
    /// Calls to external functions are not yet supported in codegen.
    #[id = "A024"]
    #[name = "External function call"]
    #[severity = error]
    pub struct ExternFunctionCall;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let arena = ctx.arena();
        let extern_names = collect_extern_function_names(arena, ctx);
        if extern_names.is_empty() {
            return Vec::new();
        }
        let mut errors = Vec::new();
        walker::walk_function_bodies(ctx, &mut |stmt_id, _walk_ctx| {
            walker::for_each_stmt_expr(&arena[stmt_id].kind, &mut |expr_id| {
                walker::walk_expr(arena, expr_id, &mut |sub_id| {
                    if let Expr::FunctionCall { function, .. } = &arena[sub_id].kind
                        && let Expr::Identifier(ident_id) = &arena[*function].kind
                    {
                        let callee_name = &arena[*ident_id].name;
                        if extern_names.contains(callee_name) {
                            errors.push(AnalysisDiagnostic::ExternFunctionCall {
                                name: callee_name.clone(),
                                location: arena[sub_id].location,
                            });
                        }
                    }
                });
            });
        });
        errors
    }
}

fn collect_extern_function_names(
    arena: &AstArena,
    ctx: &TypedContext,
) -> HashSet<String> {
    let mut names = HashSet::default();
    for source_file in ctx.source_files() {
        collect_extern_names_from_defs(arena, &source_file.defs, &mut names);
    }
    names
}

fn collect_extern_names_from_defs(
    arena: &AstArena,
    defs: &[inference_ast::ids::DefId],
    names: &mut HashSet<String>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::ExternFunction { name, .. } => {
                names.insert(arena[*name].name.clone());
            }
            Def::Spec { defs, .. } | Def::Module { defs: Some(defs), .. } => {
                collect_extern_names_from_defs(arena, defs, names);
            }
            _ => {}
        }
    }
}
