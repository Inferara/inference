use inference_ast::nodes::{AstNode, Expression, Statement};
use inference_type_checker::typed_context::TypedContext;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("{location}: `uzumaki` can only be used in variable declaration statements")]
    UzumakiMisuse {
        location: inference_ast::nodes::Location,
    },
    #[error("Semantic analysis failed: {0}")]
    General(String),
}

pub fn analyze(ctx: &TypedContext) -> anyhow::Result<()> {
    let mut errors = Vec::new();

    check_uzumaki_usage(ctx, &mut errors);

    if !errors.is_empty() {
        let error_messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        anyhow::bail!(error_messages.join("; "));
    }

    Ok(())
}

fn check_uzumaki_usage(ctx: &TypedContext, errors: &mut Vec<SemanticError>) {
    let uzumaki_nodes =
        ctx.filter_nodes(|node| matches!(node, AstNode::Expression(Expression::Uzumaki(_))));

    for node in uzumaki_nodes {
        let uzumaki = match node {
            AstNode::Expression(Expression::Uzumaki(u)) => u,
            _ => unreachable!(),
        };

        if let Some(parent) = ctx.get_parent_node(uzumaki.id) {
            match parent {
                AstNode::Statement(Statement::VariableDefinition(_)) => {
                    // Valid usage
                }
                _ => {
                    errors.push(SemanticError::UzumakiMisuse {
                        location: uzumaki.location,
                    });
                }
            }
        } else {
            // Root uzumaki or no parent found
            errors.push(SemanticError::UzumakiMisuse {
                location: uzumaki.location,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_ast::{arena::Arena, builder::Builder};
    use inference_type_checker::TypeCheckerBuilder;

    fn parse(source_code: &str) -> Arena {
        let inference_language = tree_sitter_inference::language();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&inference_language)
            .expect("Error loading Inference grammar");
        let tree = parser.parse(source_code, None).unwrap();
        let code = source_code.as_bytes();
        let root_node = tree.root_node();
        let mut builder = Builder::new();
        builder.add_source_code(root_node, code);
        builder.build_ast().unwrap()
    }

    fn check(source: &str) -> anyhow::Result<()> {
        let arena = parse(source);
        let typed_context = TypeCheckerBuilder::build_typed_context(arena)?.typed_context();
        analyze(&typed_context)
    }

    #[test]
    fn test_uzumaki_valid() {
        let source = "fn main() { let x: i32 = @; }";
        assert!(check(source).is_ok());
    }

    #[test]
    fn test_uzumaki_in_assignment() {
        let source = "fn main() { let x: i32 = 0; x = @; }";
        assert!(check(source).is_err());
    }

    #[test]
    fn test_uzumaki_in_return() {
        let source = "fn main() -> i32 { return @; }";
        assert!(check(source).is_err());
    }

    #[test]
    fn test_uzumaki_in_expression() {
        let source = "fn main() { let x: i32 = 1 + @; }";
        assert!(check(source).is_err());
    }
}
