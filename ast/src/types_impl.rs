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

#[allow(clippy::cast_possible_truncation)]
fn get_node_id() -> u32 {
    uuid::Uuid::new_v4().as_u128() as u32
}

impl SourceFile {
    #[must_use]
    pub fn new(location: Location) -> Self {
        SourceFile {
            id: get_node_id(),
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
    pub fn new(
        imported_types: Option<Vec<Rc<Identifier>>>,
        segments: Option<Vec<Rc<Identifier>>>,
        from: Option<String>,
        location: Location,
    ) -> Self {
        UseDirective {
            id: get_node_id(),
            location,
            imported_types,
            segments,
            from,
        }
    }
}

impl SpecDefinition {
    #[must_use]
    pub fn new(name: Rc<Identifier>, definitions: Vec<Definition>, location: Location) -> Self {
        SpecDefinition {
            id: get_node_id(),
            location,
            name,
            definitions,
        }
    }
}

impl StructDefinition {
    #[must_use]
    pub fn new(
        name: Rc<Identifier>,
        fields: Vec<Rc<StructField>>,
        methods: Vec<Rc<FunctionDefinition>>,
        location: Location,
    ) -> Self {
        StructDefinition {
            id: get_node_id(),
            location,
            name,
            fields,
            methods,
        }
    }
}

impl StructField {
    #[must_use]
    pub fn new(name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        StructField {
            id: get_node_id(),
            location,
            name,
            type_,
        }
    }
}

impl EnumDefinition {
    #[must_use]
    pub fn new(name: Rc<Identifier>, variants: Vec<Rc<Identifier>>, location: Location) -> Self {
        EnumDefinition {
            id: get_node_id(),
            location,
            name,
            variants,
        }
    }
}

impl Identifier {
    #[must_use]
    pub fn new(name: String, location: Location) -> Self {
        Identifier {
            id: get_node_id(),
            location,
            name,
        }
    }
}

impl Default for Identifier {
    fn default() -> Self {
        Identifier {
            id: get_node_id(),
            location: Location::default(),
            name: String::new(),
        }
    }
}

impl ConstantDefinition {
    #[must_use]
    pub fn new(name: Rc<Identifier>, type_: Type, value: Literal, location: Location) -> Self {
        ConstantDefinition {
            id: get_node_id(),
            location,
            name,
            type_,
            value,
        }
    }
}

impl FunctionDefinition {
    #[must_use]
    pub fn new(
        name: Rc<Identifier>,
        arguments: Option<Vec<Rc<Parameter>>>,
        returns: Option<Type>,
        body: BlockType,
        location: Location,
    ) -> Self {
        FunctionDefinition {
            id: get_node_id(),
            location,
            name,
            arguments,
            returns,
            body,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name.name
    }

    #[must_use]
    pub fn has_parameters(&self) -> bool {
        match self.parameters {
            Some(ref params) => !params.is_empty(),
            None => false,
        }
    }

    #[must_use]
    pub fn is_void(&self) -> bool {
        self.returns.is_none()
    }
}

impl ExternalFunctionDefinition {
    #[must_use]
    pub fn new(
        name: Rc<Identifier>,
        arguments: Option<Vec<Rc<Identifier>>>,
        returns: Option<Type>,
        location: Location,
    ) -> Self {
        ExternalFunctionDefinition {
            id: get_node_id(),
            location,
            name,
            arguments,
            returns,
        }
    }
}

impl TypeDefinition {
    #[must_use]
    pub fn new(name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        TypeDefinition {
            id: get_node_id(),
            location,
            name,
            type_,
        }
    }
}

impl Parameter {
    #[must_use]
    pub fn new(name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        Parameter {
            id: get_node_id(),
            location,
            name,
            type_,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name.name
    }
}

impl Block {
    #[must_use]
    pub fn new(location: Location, statements: Vec<Statement>) -> Self {
        Block {
            id: get_node_id(),
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
    pub fn new(location: Location, expression: Expression) -> Self {
        ExpressionStatement {
            id: get_node_id(),
            location,
            expression,
        }
    }
}

impl ReturnStatement {
    #[must_use]
    pub fn new(location: Location, expression: Expression) -> Self {
        ReturnStatement {
            id: get_node_id(),
            location,
            expression,
        }
    }
}

impl LoopStatement {
    #[must_use]
    pub fn new(location: Location, condition: Option<Expression>, body: BlockType) -> Self {
        LoopStatement {
            id: get_node_id(),
            location,
            condition,
            body,
        }
    }
}

impl BreakStatement {
    #[must_use]
    pub fn new(location: Location) -> Self {
        BreakStatement {
            id: get_node_id(),
            location,
        }
    }
}

impl IfStatement {
    #[must_use]
    pub fn new(
        location: Location,
        condition: Expression,
        if_arm: BlockType,
        else_arm: Option<BlockType>,
    ) -> Self {
        IfStatement {
            id: get_node_id(),
            location,
            condition,
            if_arm,
            else_arm,
        }
    }
}

impl VariableDefinitionStatement {
    #[must_use]
    pub fn new(
        location: Location,
        name: Rc<Identifier>,
        type_: Type,
        value: Option<Expression>,
        is_uzumaki: bool,
    ) -> Self {
        VariableDefinitionStatement {
            id: get_node_id(),
            location,
            name,
            type_,
            value,
            is_uzumaki,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name.name
    }
}

impl TypeDefinitionStatement {
    #[must_use]
    pub fn new(location: Location, name: Rc<Identifier>, type_: Type) -> Self {
        TypeDefinitionStatement {
            id: get_node_id(),
            location,
            name,
            type_,
        }
    }
}

impl AssignExpression {
    #[must_use]
    pub fn new(location: Location, left: Expression, right: Expression) -> Self {
        AssignExpression {
            id: get_node_id(),
            location,
            left,
            right,
        }
    }
}

impl ArrayIndexAccessExpression {
    #[must_use]
    pub fn new(location: Location, array: Expression, index: Expression) -> Self {
        ArrayIndexAccessExpression {
            id: get_node_id(),
            location,
            array,
            index,
        }
    }
}

impl MemberAccessExpression {
    #[must_use]
    pub fn new(location: Location, expression: Expression, name: Rc<Identifier>) -> Self {
        MemberAccessExpression {
            id: get_node_id(),
            location,
            expression,
            name,
        }
    }
}

impl TypeMemberAccessExpression {
    #[must_use]
    pub fn new(location: Location, expression: Expression, name: Rc<Identifier>) -> Self {
        TypeMemberAccessExpression {
            id: get_node_id(),
            location,
            expression,
            name,
        }
    }
}

impl FunctionCallExpression {
    #[must_use]
    pub fn new(
        location: Location,
        function: Expression,
        arguments: Option<Vec<(Rc<Identifier>, Expression)>>,
    ) -> Self {
        FunctionCallExpression {
            id: get_node_id(),
            location,
            function,
            arguments,
        }
    }
}

impl PrefixUnaryExpression {
    #[must_use]
    pub fn new(location: Location, expression: Expression, operator: UnaryOperatorKind) -> Self {
        PrefixUnaryExpression {
            id: get_node_id(),
            location,
            expression,
            operator,
        }
    }
}

impl UzumakiExpression {
    #[must_use]
    pub fn new(location: Location) -> Self {
        UzumakiExpression {
            id: get_node_id(),
            location,
        }
    }
}

impl AssertStatement {
    #[must_use]
    pub fn new(location: Location, expression: Expression) -> Self {
        AssertStatement {
            id: get_node_id(),
            location,
            expression,
        }
    }
}

impl ParenthesizedExpression {
    #[must_use]
    pub fn new(location: Location, expression: Expression) -> Self {
        ParenthesizedExpression {
            id: get_node_id(),
            location,
            expression,
        }
    }
}

impl BinaryExpression {
    #[must_use]
    pub fn new(
        location: Location,
        left: Expression,
        operator: OperatorKind,
        right: Expression,
    ) -> Self {
        BinaryExpression {
            id: get_node_id(),
            location,
            left,
            operator,
            right,
        }
    }
}

impl BoolLiteral {
    #[must_use]
    pub fn new(location: Location, value: bool) -> Self {
        BoolLiteral {
            id: get_node_id(),
            location,
            value,
        }
    }
}

impl ArrayLiteral {
    #[must_use]
    pub fn new(location: Location, elements: Vec<Expression>) -> Self {
        ArrayLiteral {
            id: get_node_id(),
            location,
            elements,
        }
    }
}

impl StringLiteral {
    #[must_use]
    pub fn new(location: Location, value: String) -> Self {
        StringLiteral {
            id: get_node_id(),
            location,
            value,
        }
    }
}

impl NumberLiteral {
    #[must_use]
    pub fn new(location: Location, value: String, type_: Type) -> Self {
        NumberLiteral {
            id: get_node_id(),
            location,
            value,
            type_,
        }
    }
}

impl UnitLiteral {
    #[must_use]
    pub fn new(location: Location) -> Self {
        UnitLiteral {
            id: get_node_id(),
            location,
        }
    }
}

impl SimpleType {
    #[must_use]
    pub fn new(location: Location, name: String) -> Self {
        SimpleType {
            id: get_node_id(),
            location,
            name,
        }
    }
}

impl GenericType {
    #[must_use]
    pub fn new(location: Location, base: Rc<Identifier>, parameters: Vec<Type>) -> Self {
        GenericType {
            id: get_node_id(),
            location,
            base,
            parameters,
        }
    }
}

impl FunctionType {
    #[must_use]
    pub fn new(location: Location, parameters: Option<Vec<Type>>, returns: Type) -> Self {
        FunctionType {
            id: get_node_id(),
            location,
            parameters,
            returns,
        }
    }
}

impl QualifiedName {
    #[must_use]
    pub fn new(location: Location, qualifier: Rc<Identifier>, name: Rc<Identifier>) -> Self {
        QualifiedName {
            id: get_node_id(),
            location,
            qualifier,
            name,
        }
    }
}

impl TypeQualifiedName {
    #[must_use]
    pub fn new(location: Location, alias: Rc<Identifier>, name: Rc<Identifier>) -> Self {
        TypeQualifiedName {
            id: get_node_id(),
            location,
            alias,
            name,
        }
    }
}

impl TypeArray {
    #[must_use]
    pub fn new(location: Location, element_type: Type, size: Option<Expression>) -> Self {
        TypeArray {
            id: get_node_id(),
            location,
            element_type,
            size,
        }
    }
}
