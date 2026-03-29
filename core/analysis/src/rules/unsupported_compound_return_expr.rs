//! A031: Unsupported compound return expression.
//!
//! Functions returning compound types (structs or arrays) use the sret calling
//! convention in WASM, which only supports specific return expression forms:
//! identifiers, literals, function calls, member access, and array index access.
//! Complex expressions like binary operations must be assigned to a temporary
//! variable first.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, StmtId, TypeId};
use inference_ast::nodes::{Def, Expr, Stmt, TypeNode};

use crate::errors::AnalysisDiagnostic;

crate::rule! {
    /// Return expressions in compound-returning functions must be simple forms.
    #[id = "A031"]
    #[name = "Unsupported compound return expression"]
    #[severity = error]
    pub struct UnsupportedCompoundReturnExpr;
    fn check(ctx: &TypedContext) -> Vec<AnalysisDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(ctx, arena, &source_file.defs, &mut errors);
        }
        errors
    }
}

fn has_compound_return_type(
    ctx: &inference_type_checker::typed_context::TypedContext,
    arena: &AstArena,
    returns: Option<TypeId>,
) -> bool {
    let Some(type_id) = returns else {
        return false;
    };
    match &arena[type_id].kind {
        TypeNode::Array { .. } => true,
        TypeNode::Custom(ident_id) => ctx.lookup_struct(&arena[*ident_id].name).is_some(),
        _ => false,
    }
}

fn is_supported_sret_expr(arena: &AstArena, expr_id: ExprId) -> bool {
    matches!(
        &arena[expr_id].kind,
        Expr::Identifier(_)
            | Expr::ArrayLiteral { .. }
            | Expr::StructLiteral { .. }
            | Expr::FunctionCall { .. }
            | Expr::MemberAccess { .. }
            | Expr::ArrayIndexAccess { .. }
    )
}

fn check_defs(
    ctx: &inference_type_checker::typed_context::TypedContext,
    arena: &AstArena,
    defs: &[DefId],
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function {
                returns, body, ..
            } => {
                if has_compound_return_type(ctx, arena, *returns) {
                    check_block_for_returns(arena, *body, errors);
                }
            }
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function {
                        returns, body, ..
                    } = &arena[method_id].kind
                        && has_compound_return_type(ctx, arena, *returns)
                    {
                        check_block_for_returns(arena, *body, errors);
                    }
                }
            }
            Def::Spec { defs, .. } => check_defs(ctx, arena, defs, errors),
            Def::Module {
                defs: Some(inner), ..
            } => check_defs(ctx, arena, inner, errors),
            _ => {}
        }
    }
}

fn check_block_for_returns(
    arena: &AstArena,
    block_id: BlockId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    let block = &arena[block_id];
    for &stmt_id in &block.stmts {
        check_stmt_for_returns(arena, stmt_id, errors);
    }
}

fn check_stmt_for_returns(
    arena: &AstArena,
    stmt_id: StmtId,
    errors: &mut Vec<AnalysisDiagnostic>,
) {
    match &arena[stmt_id].kind {
        Stmt::Return { expr } => {
            if !is_supported_sret_expr(arena, *expr) {
                errors.push(AnalysisDiagnostic::UnsupportedCompoundReturnExpression {
                    location: arena[*expr].location,
                });
            }
        }
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            check_block_for_returns(arena, *then_block, errors);
            if let Some(else_id) = else_block {
                check_block_for_returns(arena, *else_id, errors);
            }
        }
        Stmt::Loop { body, .. } => {
            check_block_for_returns(arena, *body, errors);
        }
        Stmt::Block(block_id) => {
            check_block_for_returns(arena, *block_id, errors);
        }
        _ => {}
    }
}
