use crate::{
    arena::Arena,
    types::{AstNode, Expression, Location, SourceFile, Type},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolType {
    Global(String),
    Inner(String),
    Untyped,
}

pub enum SymbolScope {}

pub struct Symbol {
    name: String,
    location: Location,
    ty: Type,
    expr: Expression,
}

#[derive(Clone, Default)]
pub struct SymbolTable {}

impl SymbolTable {
    pub fn build(source_files: &Vec<SourceFile>, types: &Vec<SymbolType>, arena: &Arena) -> Self {
        // for source_file in source_files {}
        // for node in arena.nodes.values() {
        //     match node {
        //         AstNode::AssignExpression(e) |
        //         AstNode::ArrayIndexAccessExpression(e) |
        //         AstNode::MemberAccessExpression(e) |
        //         AstNode::FunctionCallExpression(e) |
        //         AstNode::PrefixUnaryExpression(e) |
        //         AstNode::ParenthesizedExpression(e) |
        //         AstNode::BinaryExpression(e) |
        //         AstNode::UzumakiExpression(e) |

        //         AstNode::Identifier(ie) |

        //         AstNode::ArrayLiteral(le) |
        //         AstNode::BoolLiteral(le) |
        //         AstNode::StringLiteral(le)
        //         AstNode::NumberLiteral(le) |
        //         AstNode::UnitLiteral(le) =>

        //         AstNode::TypeArray(te) |
        //         AstNode::SimpleType(te) |
        //         AstNode::GenericType(te) |
        //         &AstNode::FunctionType(te) |
        //         &AstNode::QualifiedName(te) |
        //         AstNode::TypeQualifiedName(te) |
        //     }
        // }
        Self {}
    }

    fn infer_for_expression() {}
}
