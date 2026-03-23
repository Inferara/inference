//! A024: Calls to external functions are not yet supported in codegen.
//!
//! External functions are declared with `external fn` but the WebAssembly code
//! generator does not yet emit WASM imports for them. Calling an external
//! function would panic during code generation, so the analysis pass rejects
//! such calls with a clear error message.

use std::collections::HashSet;

use inference_ast::arena::AstArena;
use inference_ast::ids::ExprId;
use inference_ast::nodes::{Def, Expr, Stmt};

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
            check_stmt(arena, &arena[stmt_id].kind, &extern_names, &mut errors);
        });
        errors
    }
}

fn collect_extern_function_names(
    arena: &AstArena,
    ctx: &crate::rule::TypedContext,
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

fn check_stmt(
    arena: &AstArena,
    stmt: &Stmt,
    extern_names: &HashSet<String>,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match stmt {
        Stmt::VarDef { value: Some(expr_id), .. } | Stmt::Expr(expr_id) => {
            check_expr(arena, *expr_id, extern_names, errors);
        }
        Stmt::Assign { left, right } => {
            check_expr(arena, *left, extern_names, errors);
            check_expr(arena, *right, extern_names, errors);
        }
        Stmt::Return { expr } | Stmt::Assert { expr } => {
            check_expr(arena, *expr, extern_names, errors);
        }
        Stmt::If { condition, .. } => {
            check_expr(arena, *condition, extern_names, errors);
        }
        Stmt::Loop { condition: Some(cond_expr), .. } => {
            check_expr(arena, *cond_expr, extern_names, errors);
        }
        _ => {}
    }
}

fn check_expr(
    arena: &AstArena,
    expr_id: ExprId,
    extern_names: &HashSet<String>,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match &arena[expr_id].kind {
        Expr::FunctionCall { function, args, .. } => {
            if let Expr::Identifier(ident_id) = &arena[*function].kind {
                let callee_name = &arena[*ident_id].name;
                if extern_names.contains(callee_name) {
                    errors.push(AnalysisDiagnostic::ExternFunctionCall {
                        name: callee_name.clone(),
                        location: arena[expr_id].location,
                    });
                }
            }
            check_expr(arena, *function, extern_names, errors);
            for (_, arg_expr) in args {
                check_expr(arena, *arg_expr, extern_names, errors);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr(arena, *left, extern_names, errors);
            check_expr(arena, *right, extern_names, errors);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => {
            check_expr(arena, *expr, extern_names, errors);
        }
        Expr::ArrayIndexAccess { array, index } => {
            check_expr(arena, *array, extern_names, errors);
            check_expr(arena, *index, extern_names, errors);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                check_expr(arena, *field_expr, extern_names, errors);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                check_expr(arena, *elem, extern_names, errors);
            }
        }
        Expr::Identifier(_)
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => {}
    }
}
