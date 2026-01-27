#![warn(clippy::pedantic)]
//! Semantic Analysis Crate
//!
//! Performs semantic checks on the typed AST that are beyond the scope of type checking.
//! These are scope-bounded checks for language-level constraints that are not type errors.
//!
//! ## Current Checks
//!
//! - **Combined unary operators**: Prohibits chained/unparenthesized unary operator
//!   combinations such as `--x`, `-~x`, `!!x`, and parenthesized variants like `-(~x)`.
//!
//! ## Diagnostics
//!
//! The analysis produces diagnostics with three severity levels:
//! - `Error` — semantic violations that must be fixed
//! - `Warning` — suspicious patterns that may indicate bugs
//! - `Info` — informational notes about code style or usage

pub mod diagnostics;

use diagnostics::{SemanticDiagnostic, SemanticResult, Severity};
use inference_ast::nodes::{AstNode, Expression, Literal};
use inference_type_checker::typed_context::TypedContext;

/// Runs all semantic analysis passes on the typed AST.
///
/// Returns a [`SemanticResult`] containing any diagnostics found.
#[must_use]
pub fn analyze(ctx: &TypedContext) -> SemanticResult {
    let mut result = SemanticResult::default();
    check_combined_unary_operators(ctx, &mut result);
    result
}

/// Checks for prohibited combined unary operators in expressions.
///
/// Detects chained prefix unary operators like `--x`, `!!x`, `-~x`,
/// and parenthesized variants like `-(~x)`, `~(-x)`.
fn check_combined_unary_operators(ctx: &TypedContext, result: &mut SemanticResult) {
    let prefix_unary_nodes = ctx.filter_nodes(|node| {
        matches!(node, AstNode::Expression(Expression::PrefixUnary(_)))
    });

    for node in prefix_unary_nodes {
        if let AstNode::Expression(Expression::PrefixUnary(ref prefix_expr)) = node {
            if is_combined_unary(&prefix_expr.expression.borrow()) {
                result.diagnostics.push(SemanticDiagnostic {
                    severity: Severity::Error,
                    message: "combined unary operators are prohibited".to_string(),
                    location: node.location(),
                });
            }
        }
    }
}

/// Returns `true` if the expression is itself a unary operator (directly or
/// through parentheses), or a negative numeric literal — indicating a
/// combined/chained unary operator usage.
fn is_combined_unary(expr: &Expression) -> bool {
    match expr {
        Expression::PrefixUnary(_) => true,
        Expression::Parenthesized(inner) => is_combined_unary(&inner.expression.borrow()),
        Expression::Literal(Literal::Number(num)) => num.value.starts_with('-'),
        _ => false,
    }
}
