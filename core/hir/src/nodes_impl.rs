use std::rc::Rc;

use crate::{
    nodes::{Definition, Expression, FunctionDefinition, Literal, SourceFile, UzumakiExpression},
    type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind},
};

impl Expression {
    #[must_use]
    pub fn type_info(&self) -> TypeInfo {
        match self {
            Expression::ArrayIndexAccess(e) => e.type_info.clone(),
            Expression::MemberAccess(e) => e.type_info.clone(),
            Expression::TypeMemberAccess(e) => e.type_info.clone(),
            Expression::FunctionCall(e) => e.type_info.clone(),
            Expression::Struct(e) => e.type_info.clone(),
            Expression::PrefixUnary(e) => e.type_info.clone(),
            Expression::Parenthesized(e) => e.type_info.clone(),
            Expression::Binary(e) => e.type_info.clone(),
            Expression::Literal(l) => l.type_info(),
            Expression::TypeInfo(e) => e.clone(),
            Expression::Uzumaki(e) => e.type_info.clone(),
        }
    }
}

impl Literal {
    #[must_use]
    pub fn type_info(&self) -> TypeInfo {
        match self {
            Literal::Bool(_) => TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            },
            Literal::Number(literal) => literal.type_info.clone(),
            Literal::String(_) => TypeInfo {
                kind: TypeInfoKind::String,
                type_params: vec![],
            },
            Literal::Unit(_) => TypeInfo {
                kind: TypeInfoKind::Unit,
                type_params: vec![],
            },
            Literal::Array(literal) => literal.type_info.clone(),
        }
    }
}

impl UzumakiExpression {
    #[must_use]
    pub fn is_i32(&self) -> bool {
        matches!(
            self.type_info.kind,
            TypeInfoKind::Number(NumberTypeKindNumberType::I32)
        )
    }
    #[must_use]
    pub fn is_i64(&self) -> bool {
        matches!(
            self.type_info.kind,
            TypeInfoKind::Number(NumberTypeKindNumberType::I64)
        )
    }
}

impl SourceFile {
    #[must_use]
    pub fn function_definitions(&self) -> Vec<Rc<FunctionDefinition>> {
        self.definitions
            .iter()
            .filter_map(|item| {
                if let Definition::Function(func_def) = item {
                    Some(func_def.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
