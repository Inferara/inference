//! Inference AST Nodes and enums implementations
#![allow(dead_code)]

use std::rc::Rc;
use std::{fmt::Display, rc::Rc};

use crate::{
    type_info::{TypeInfo, TypeInfoKind},
    types::{ArgumentType, IgnoreArgument, SelfReference},
};

use crate::symbols::SymbolType;

use super::types::{
    Argument, ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignStatement,
    BinaryExpression, Block, BlockType, BoolLiteral, BreakStatement, ConstantDefinition,
    Definition, EnumDefinition, Expression, ExpressionStatement, ExternalFunctionDefinition,
    FunctionCallExpression, FunctionDefinition, FunctionType, GenericType, Identifier, IfStatement,
    Literal, Location, LoopStatement, MemberAccessExpression, NumberLiteral, OperatorKind,
    ParenthesizedExpression, PrefixUnaryExpression, QualifiedName, ReturnStatement, SimpleType,
    SourceFile, SpecDefinition, Statement, StringLiteral, StructDefinition, StructField, Type,
    TypeArray, TypeDefinition, TypeDefinitionStatement, TypeQualifiedName, UnaryOperatorKind,
    UnitLiteral, UseDirective, UzumakiExpression, VariableDefinitionStatement,
};

impl SourceFile {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        SourceFile {
            id,
            location,
            directives: Vec::new(),
            definitions: Vec::new(),
        }
    }
}

impl SourceFile {
    #[must_use]
    pub fn function_definitions(&self) -> Vec<Rc<FunctionDefinition>> {
        self.definitions
            .iter()
            .filter_map(|def| match def {
                Definition::Function(func) => Some(func.clone()),
                _ => None,
            })
            .collect()
    }
}

impl BlockType {
    #[must_use]
    pub fn statements(&self) -> Vec<Statement> {
        match self {
            BlockType::Block(block)
            | BlockType::Forall(block)
            | BlockType::Assume(block)
            | BlockType::Exists(block)
            | BlockType::Unique(block) => block.statements.clone(),
        }
    }
}

impl Expression {
    #[must_use]
    pub fn type_info(&self) -> Option<TypeInfo> {
        match self {
            Expression::ArrayIndexAccess(e) => e.type_info.borrow().clone(),
            Expression::MemberAccess(e) => e.type_info.borrow().clone(),
            Expression::FunctionCall(e) => e.type_info.borrow().clone(),
            Expression::PrefixUnary(e) => e.type_info.borrow().clone(),
            Expression::Parenthesized(e) => e.type_info.borrow().clone(),
            Expression::Binary(e) => e.type_info.borrow().clone(),
            Expression::Literal(l) => l.type_info(),
            Expression::Identifier(e) => e.type_info.borrow().clone(),
            Expression::Type(e) => Some(TypeInfo::new(e)),
            Expression::Uzumaki(e) => e.type_info.borrow().clone(),
        }
    }
}

impl Literal {
    #[must_use]
    pub fn type_info(&self) -> Option<TypeInfo> {
        match self {
            Literal::Bool(_) => Some(TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![],
            }),
            Literal::Number(literal) => literal.type_info.borrow().clone(),
            Literal::String(_) => Some(TypeInfo {
                kind: TypeInfoKind::String,
                type_params: vec![],
            }),
            Literal::Unit(_) => Some(TypeInfo {
                kind: TypeInfoKind::Unit,
                type_params: vec![],
            }),
            Literal::Array(literal) => literal.type_info.borrow().clone(),
        }
    }
}

impl UseDirective {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        imported_types: Option<Vec<Rc<Identifier>>>,
        segments: Option<Vec<Rc<Identifier>>>,
        from: Option<String>,
        location: Location,
    ) -> Self {
        UseDirective {
            id,
            location,
            imported_types,
            segments,
            from,
        }
    }
}

impl SpecDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        definitions: Vec<Definition>,
        location: Location,
    ) -> Self {
        SpecDefinition {
            id,
            location,
            name,
            definitions,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl StructDefinition {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        fields: Vec<Rc<StructField>>,
        methods: Vec<Rc<FunctionDefinition>>,
        location: Location,
    ) -> Self {
        StructDefinition {
            id,
            location,
            name,
            fields,
            methods,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl StructField {
    #[must_use]
    pub fn new(id: u32, name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        StructField {
            id,
            location,
            name,
            type_,
        }
    }
}

impl EnumDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        variants: Vec<Rc<Identifier>>,
        location: Location,
    ) -> Self {
        EnumDefinition {
            id,
            location,
            name,
            variants,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl Identifier {
    #[must_use]
    pub fn new(id: u32, name: String, location: Location) -> Self {
        Identifier {
            id,
            location,
            name,
            type_info: RefCell::new(None),
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

impl ConstantDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        type_: Type,
        value: Literal,
        location: Location,
    ) -> Self {
        ConstantDefinition {
            id,
            location,
            name,
            ty: type_,
            value,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl FunctionDefinition {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        type_parameters: Option<Vec<Rc<Identifier>>>,
        arguments: Option<Vec<ArgumentType>>,
        returns: Option<Type>,
        body: BlockType,
        location: Location,
    ) -> Self {
        FunctionDefinition {
            id,
            location,
            name,
            type_parameters,
            arguments,
            returns,
            body,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }

    #[must_use]
    #[must_use]
    pub fn has_parameters(&self) -> bool {
        if let Some(arguments) = &self.arguments {
            return !arguments.is_empty();
        }
        false
    }

    #[must_use]
    #[must_use]
    pub fn is_void(&self) -> bool {
        self.returns.is_none()
    }
}

impl ExternalFunctionDefinition {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        arguments: Option<Vec<ArgumentType>>,
        returns: Option<Type>,
        location: Location,
    ) -> Self {
        ExternalFunctionDefinition {
            id,
            location,
            name,
            arguments,
            returns,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl TypeDefinition {
    #[must_use]
    pub fn new(id: u32, name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        TypeDefinition {
            id,
            location,
            name,
            ty: type_,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl Argument {
    #[must_use]
    pub fn new(id: u32, location: Location, name: Rc<Identifier>, is_mut: bool, ty: Type) -> Self {
        Argument {
            id,
            location,
            name,
            is_mut,
            ty,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl SelfReference {
    #[must_use]
    pub fn new(id: u32, location: Location, is_mut: bool) -> Self {
        SelfReference {
            id,
            location,
            is_mut,
        }
    }
}

impl IgnoreArgument {
    #[must_use]
    pub fn new(id: u32, location: Location, ty: Type) -> Self {
        IgnoreArgument { id, location, ty }
    }
}

impl Block {
    #[must_use]
    pub fn new(id: u32, location: Location, statements: Vec<Statement>) -> Self {
        Block {
            id,
            location,
            statements,
        }
    }
}

impl ExpressionStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ExpressionStatement {
            id,
            location,
            expression,
        }
    }
}

impl ReturnStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ReturnStatement {
            id,
            location,
            expression: RefCell::new(expression),
        }
    }
}

impl LoopStatement {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        condition: Option<Expression>,
        body: BlockType,
    ) -> Self {
        LoopStatement {
            id,
            location,
            condition: RefCell::new(condition),
            body,
        }
    }
}

impl BreakStatement {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        BreakStatement { id, location }
    }
}

impl IfStatement {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        condition: Expression,
        if_arm: BlockType,
        else_arm: Option<BlockType>,
    ) -> Self {
        IfStatement {
            id,
            location,
            condition: RefCell::new(condition),
            if_arm,
            else_arm,
        }
    }
}

impl VariableDefinitionStatement {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        name: Rc<Identifier>,
        type_: Type,
        value: Option<Expression>,
        is_uzumaki: bool,
    ) -> Self {
        VariableDefinitionStatement {
            id,
            location,
            name,
            ty: type_,
            value: value.map(RefCell::new),
            is_uzumaki,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl TypeDefinitionStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, name: Rc<Identifier>, type_: Type) -> Self {
        TypeDefinitionStatement {
            id,
            location,
            name,
            ty: type_,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl AssignStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, left: Expression, right: Expression) -> Self {
        AssignStatement {
            id,
            location,
            left: RefCell::new(left),
            right: RefCell::new(right),
        }
    }
}

impl ArrayIndexAccessExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, array: Expression, index: Expression) -> Self {
        ArrayIndexAccessExpression {
            id,
            location,
            array: RefCell::new(array),
            index: RefCell::new(index),
            type_info: RefCell::new(None),
        }
    }
}

impl MemberAccessExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression, name: Rc<Identifier>) -> Self {
        MemberAccessExpression {
            id,
            location,
            expression: RefCell::new(expression),
            name,
            type_info: RefCell::new(None),
        }
    }
}

impl FunctionCallExpression {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        function: Expression,
        arguments: Option<Vec<(Option<Rc<Identifier>>, Expression)>>,
    ) -> Self {
        let arguments = arguments.map(|args| {
            args.into_iter()
                .map(|(name, expr)| (name, RefCell::new(expr)))
                .collect()
        });
        FunctionCallExpression {
            id,
            location,
            function,
            arguments,
            type_info: RefCell::new(None),
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        if let Expression::Identifier(identifier) = &self.function {
            identifier.name()
        } else if let Expression::MemberAccess(member_access) = &self.function {
            member_access.name.name()
        } else {
            String::new()
        }
    }
}

impl PrefixUnaryExpression {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        expression: Expression,
        operator: UnaryOperatorKind,
    ) -> Self {
        PrefixUnaryExpression {
            id,
            location,
            expression: RefCell::new(expression),
            operator,
            type_info: RefCell::new(None),
        }
    }
}

impl UzumakiExpression {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        UzumakiExpression {
            id,
            location,
            type_info: RefCell::new(None),
        }
    }
}

impl AssertStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        AssertStatement {
            id,
            location,
            expression: RefCell::new(expression),
        }
    }
}

impl ParenthesizedExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ParenthesizedExpression {
            id,
            location,
            expression: RefCell::new(expression),
            type_info: RefCell::new(None),
        }
    }
}

impl BinaryExpression {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        left: Expression,
        operator: OperatorKind,
        right: Expression,
    ) -> Self {
        BinaryExpression {
            id,
            location,
            left: RefCell::new(left),
            operator,
            right: RefCell::new(right),
            type_info: RefCell::new(None),
        }
    }
}

impl BoolLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, value: bool) -> Self {
        BoolLiteral {
            id,
            location,
            value,
        }
    }
}

impl ArrayLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, elements: Vec<Expression>) -> Self {
        ArrayLiteral {
            id,
            location,
            elements,
            type_info: RefCell::new(None),
        }
    }
}

impl StringLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, value: String) -> Self {
        StringLiteral {
            id,
            location,
            value,
        }
    }
}

impl NumberLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, value: String) -> Self {
        NumberLiteral {
            id,
            location,
            value,
            type_info: RefCell::new(None),
        }
    }
}

impl UnitLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        UnitLiteral { id, location }
    }
}

impl SimpleType {
    #[must_use]
    pub fn new(id: u32, location: Location, name: String) -> Self {
        SimpleType {
            id,
            location,
            name,
            type_info: RefCell::new(None),
        }
    }
}

impl GenericType {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        base: Rc<Identifier>,
        parameters: Vec<Rc<Identifier>>,
    ) -> Self {
        GenericType {
            id,
            location,
            base,
            parameters,
            type_info: RefCell::new(None),
        }
    }
}

impl FunctionType {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        parameters: Option<Vec<Type>>,
        returns: Option<Type>,
    ) -> Self {
        FunctionType {
            id,
            location,
            parameters,
            returns,
            type_info: RefCell::new(None),
        }
    }
}

impl QualifiedName {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        qualifier: Rc<Identifier>,
        name: Rc<Identifier>,
    ) -> Self {
        QualifiedName {
            id,
            location,
            qualifier,
            name,
            type_info: RefCell::new(None),
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }

    #[must_use]
    pub fn qualifier(&self) -> String {
        self.qualifier.name()
    }
}

impl TypeQualifiedName {
    #[must_use]
    pub fn new(id: u32, location: Location, alias: Rc<Identifier>, name: Rc<Identifier>) -> Self {
        TypeQualifiedName {
            id,
            location,
            alias,
            name,
            type_info: RefCell::new(None),
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }

    #[must_use]
    pub fn alias(&self) -> String {
        self.alias.name()
    }
}

impl TypeArray {
    #[must_use]
    pub fn new(id: u32, location: Location, element_type: Type, size: Option<Expression>) -> Self {
        TypeArray {
            id,
            location,
            element_type,
            size,
            type_info: RefCell::new(None),
        }
    }
}
