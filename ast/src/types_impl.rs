//! Inference AST Nodes and enums implementations
#![allow(dead_code)]

use std::rc::Rc;

use crate::{node::Location, types::TypeMemberAccessExpression};

use super::types::{
    ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignExpression, BinaryExpression,
    Block, BlockType, BoolLiteral, BreakStatement, ConstantDefinition, Definition, EnumDefinition,
    Expression, ExpressionStatement, ExternalFunctionDefinition, FunctionCallExpression,
    FunctionDefinition, FunctionType, GenericType, Identifier, IfStatement, Literal, LoopStatement,
    MemberAccessExpression, NumberLiteral, OperatorKind, Parameter, ParenthesizedExpression,
    PrefixUnaryExpression, QualifiedName, ReturnStatement, SimpleType, SourceFile, SpecDefinition,
    Statement, StringLiteral, StructDefinition, StructField, Type, TypeArray, TypeDefinition,
    TypeDefinitionStatement, TypeQualifiedName, UnaryOperatorKind, UnitLiteral, UseDirective,
    UzumakiExpression, VariableDefinitionStatement,
};

impl SourceFile {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        SourceFile {
            id,
            location,
            use_directives: Vec::new(),
            definitions: Vec::new(),
        }
    }

    pub fn add_use_directive(&mut self, use_directive: Rc<UseDirective>) {
    pub fn add_use_directive(&mut self, use_directive: Rc<UseDirective>) {
        self.use_directives.push(use_directive);
    }

    pub fn add_definition(&mut self, definition: Definition) {
        self.definitions.push(definition);
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
}

impl Identifier {
    #[must_use]
    pub fn new(id: u32, name: String, location: Location) -> Self {
        Identifier { id, location, name }
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
            type_,
            value,
        }
    }
}

impl FunctionDefinition {
    #[must_use]
    #[must_use]
    pub fn new(
        id: u32,
        name: Rc<Identifier>,
        arguments: Option<Vec<Rc<Parameter>>>,
        returns: Option<Type>,
        body: BlockType,
        location: Location,
    ) -> Self {
        FunctionDefinition {
            id,
            location,
            name,
            arguments,
            returns,
            body,
        }
    }

    #[must_use]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name.name
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
        arguments: Option<Vec<Rc<Identifier>>>,
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
}

impl TypeDefinition {
    #[must_use]
    pub fn new(id: u32, name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        TypeDefinition {
            id,
            location,
            name,
            type_,
        }
    }
}

impl Parameter {
    #[must_use]
    pub fn new(id: u32, location: Location, name: Rc<Identifier>, type_: Type) -> Self {
        Parameter {
            id,
            location,
            name,
            type_,
        }
    }

    #[must_use]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name.name
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

    pub fn add_statement(&mut self, statement: Statement) {
        self.statements.push(statement);
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
            expression,
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
            condition,
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
            condition,
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
            type_,
            value,
            is_uzumaki,
        }
    }

    #[must_use]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name.name
    }
}

impl TypeDefinitionStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, name: Rc<Identifier>, type_: Type) -> Self {
        TypeDefinitionStatement {
            id,
            location,
            name,
            type_,
        }
    }
}

impl AssignExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, left: Expression, right: Expression) -> Self {
        AssignExpression {
            id,
            location,
            left,
            right,
        }
    }
}

impl ArrayIndexAccessExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, array: Expression, index: Expression) -> Self {
        ArrayIndexAccessExpression {
            id,
            location,
            array,
            index,
        }
    }
}

impl MemberAccessExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression, name: Rc<Identifier>) -> Self {
        MemberAccessExpression {
            id,
            location,
            expression,
            name,
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
        FunctionCallExpression {
            id,
            location,
            function,
            arguments,
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
            expression,
            operator,
        }
    }
}

impl UzumakiExpression {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        UzumakiExpression { id, location }
    }
}

impl AssertStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        AssertStatement {
            id,
            location,
            expression,
        }
    }
}

impl ParenthesizedExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ParenthesizedExpression {
            id,
            location,
            expression,
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
            left,
            operator,
            right,
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
    pub fn new(id: u32, location: Location, value: String, type_: Type) -> Self {
        NumberLiteral {
            id,
            location,
            value,
            type_,
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
        SimpleType { id, location, name }
    }
}

impl GenericType {
    #[must_use]
    pub fn new(id: u32, location: Location, base: Rc<Identifier>, parameters: Vec<Type>) -> Self {
        GenericType {
            id,
            location,
            base,
            parameters,
        }
    }
}

impl FunctionType {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        parameters: Option<Vec<Type>>,
        returns: Box<Type>,
    ) -> Self {
        FunctionType {
            id,
            location,
            parameters,
            returns,
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
        }
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
        }
    }
}

impl TypeArray {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        element_type: Box<Type>,
        size: Option<Box<Expression>>,
    ) -> Self {
        TypeArray {
            id,
            location,
            element_type,
            size,
        }
    }
}
