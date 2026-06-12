//! A010: Methods that declare `self` should actually reference it.
//!
//! If a method takes `self` but never accesses it in the body, the method
//! should be an associated function instead.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, ExprId, StmtId};
use inference_ast::nodes::{ArgKind, Def, Expr, Stmt};

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Methods that declare `self` should actually reference it.
    #[id = "A010"]
    #[name = "Method never accesses self"]
    #[severity = warning]
    pub struct MethodNeverAccessesSelf;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut warnings = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            for &def_id in &source_file.defs {
                check_def(arena, def_id, &mut warnings);
            }
        }
        warnings
    }
}

fn check_def(arena: &AstArena, def_id: inference_ast::ids::DefId, warnings: &mut Vec<AnalysisDiagnostic>) {
    match &arena[def_id].kind {
        Def::Struct { name, methods, .. } => {
            let struct_name = arena[*name].name.clone();
            for &method_id in methods {
                if let Def::Function {
                    name: method_name,
                    args,
                    body,
                    ..
                } = &arena[method_id].kind
                {
                    let has_self = args
                        .iter()
                        .any(|a| matches!(a.kind, ArgKind::SelfRef { .. }));
                    if has_self && !body_references_self(arena, *body) {
                        warnings.push(AnalysisDiagnostic::MethodNeverAccessesSelf {
                            struct_name: struct_name.clone(),
                            method_name: arena[*method_name].name.clone(),
                            location: arena[method_id].location,
                        });
                    }
                }
            }
        }
        Def::Spec { defs, .. } => {
            for &inner_def_id in defs {
                check_def(arena, inner_def_id, warnings);
            }
        }
        _ => {}
    }
}

fn body_references_self(arena: &AstArena, block_id: BlockId) -> bool {
    arena[block_id]
        .stmts
        .iter()
        .any(|&stmt_id| stmt_references_self(arena, stmt_id))
}

fn stmt_references_self(arena: &AstArena, stmt_id: StmtId) -> bool {
    match &arena[stmt_id].kind {
        Stmt::Block(block_id) => body_references_self(arena, *block_id),
        Stmt::Expr(expr_id) => expr_references_self(arena, *expr_id),
        Stmt::Assign { left, right } => {
            expr_references_self(arena, *left) || expr_references_self(arena, *right)
        }
        Stmt::Return { expr } | Stmt::Assert { expr } => expr_references_self(arena, *expr),
        Stmt::Loop { condition, body } => {
            condition
                .as_ref()
                .is_some_and(|c| expr_references_self(arena, *c))
                || body_references_self(arena, *body)
        }
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_references_self(arena, *condition)
                || body_references_self(arena, *then_block)
                || else_block.is_some_and(|b| body_references_self(arena, b))
        }
        Stmt::VarDef { value, .. } => {
            value.is_some_and(|v| expr_references_self(arena, v))
        }
        Stmt::Break | Stmt::TypeDef { .. } | Stmt::ConstDef(_) => false,
    }
}

fn expr_references_self(arena: &AstArena, expr_id: ExprId) -> bool {
    match &arena[expr_id].kind {
        Expr::Identifier(ident_id) => arena[*ident_id].name == "self",
        Expr::Binary { left, right, .. } => {
            expr_references_self(arena, *left) || expr_references_self(arena, *right)
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => expr_references_self(arena, *expr),
        Expr::FunctionCall {
            function, args, ..
        } => {
            expr_references_self(arena, *function)
                || args
                    .iter()
                    .any(|(_, arg_expr)| expr_references_self(arena, *arg_expr))
        }
        Expr::ArrayIndexAccess { array, index } => {
            expr_references_self(arena, *array) || expr_references_self(arena, *index)
        }
        Expr::StructLiteral { fields, .. } => fields
            .iter()
            .any(|(_, field_expr)| expr_references_self(arena, *field_expr)),
        Expr::ArrayLiteral { elements } => elements
            .iter()
            .any(|elem| expr_references_self(arena, *elem)),
        Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => false,
    }
}
