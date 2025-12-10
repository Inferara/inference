use serde::{Deserialize, Serialize};

use crate::{
    node::Location,
    types::{Definition, Expression, Literal, Statement, Type},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NodeKind {
    Definition(Definition),
    Statement(Statement),
    Expression(Expression),
    Literal(Literal),
    Type(Type),
}

impl NodeKind {
    #[must_use]
    pub fn id(&self) -> u32 {
        match self {
            NodeKind::Definition(d) => d.id(),
            NodeKind::Statement(s) => s.id(),
            NodeKind::Expression(e) => e.id(),
            NodeKind::Literal(l) => l.id(),
            NodeKind::Type(t) => t.id(),
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn location(&self) -> Location {
        match self {
            NodeKind::Definition(d) => d.location().clone(),
            NodeKind::Statement(s) => s.location(),
            NodeKind::Expression(e) => e.location(),
            NodeKind::Literal(l) => l.location(),
            NodeKind::Type(t) => t.location(),
        }
    }

    #[must_use]
    pub fn children(&self) -> Vec<NodeKind> {
        match self {
            NodeKind::Definition(definition) => definition.children(),
            NodeKind::Statement(statement) => statement.children(),
            NodeKind::Expression(expression) => expression.children(),
            NodeKind::Literal(literal) => literal.children(),
            NodeKind::Type(ty) => ty.children(),
        }
    }
}
