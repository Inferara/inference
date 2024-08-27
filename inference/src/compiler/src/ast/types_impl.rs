#![allow(dead_code)]

use super::types::{
    Argument, ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignExpression,
    BinaryExpression, Block, BoolLiteral, ConstantDefinition, ContextDefinition, Definition,
    EnumDefinition, Expression, ExpressionStatement, ExternalFunctionDefinition, FilterStatement,
    ForStatement, FunctionCallExpression, FunctionDefinition, GenericType, Identifier, IfStatement,
    Literal, Location, MemberAccessExpression, NumberLiteral, OperatorKind,
    ParenthesizedExpression, Position, PrefixUnaryExpression, QualifiedType, ReturnStatement,
    SimpleType, SourceFile, Statement, StringLiteral, StructDefinition, StructField, Type,
    TypeArray, TypeDefinition, TypeDefinitionStatement, TypeOfExpression, UnaryOperatorKind,
    UseDirective, VariableDefinitionStatement, VerifyStatement,
};

impl SourceFile {
    pub fn new(location: Location) -> Self {
        SourceFile {
            location,
            use_directives: Vec::new(),
            definitions: Vec::new(),
        }
    }

    pub fn add_use_directive(&mut self, use_directive: UseDirective) {
        self.use_directives.push(use_directive);
    }

    pub fn add_definition(&mut self, definition: Definition) {
        self.definitions.push(definition);
    }
}

impl UseDirective {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        imported_types: Option<Vec<Identifier>>,
        segments: Option<Vec<Identifier>>,
        from: Option<String>,
    ) -> Self {
        UseDirective {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            imported_types,
            segments,
            from,
        }
    }
}

impl ContextDefinition {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        definitions: Vec<Definition>,
    ) -> Self {
        ContextDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            definitions,
        }
    }
}

impl StructDefinition {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        fields: Vec<StructField>,
        methods: Vec<FunctionDefinition>,
    ) -> Self {
        StructDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            fields,
            methods,
        }
    }
}

impl StructField {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
    ) -> Self {
        StructField {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            type_,
        }
    }
}

impl EnumDefinition {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        variants: Vec<Identifier>,
    ) -> Self {
        EnumDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            variants,
        }
    }
}

impl Identifier {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: String,
    ) -> Self {
        Identifier {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
        }
    }
}

impl ConstantDefinition {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
        value: Literal,
    ) -> Self {
        ConstantDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            type_,
            value,
        }
    }
}

impl FunctionDefinition {
    #![allow(clippy::too_many_arguments)]
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        arguments: Option<Vec<Argument>>,
        returns: Option<Type>,
        body: Block,
    ) -> Self {
        FunctionDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            arguments,
            returns,
            body,
        }
    }
}

impl ExternalFunctionDefinition {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        arguments: Option<Vec<Identifier>>,
        returns: Option<Type>,
    ) -> Self {
        ExternalFunctionDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            arguments,
            returns,
        }
    }
}

impl TypeDefinition {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
    ) -> Self {
        TypeDefinition {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            type_,
        }
    }
}

impl Argument {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
    ) -> Self {
        Argument {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            type_,
        }
    }
}

impl Block {
    pub fn new(start_row: usize, start_column: usize, end_row: usize, end_column: usize) -> Self {
        Block {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            statements: Vec::new(),
        }
    }

    pub fn add_statement(&mut self, statement: Statement) {
        self.statements.push(statement);
    }
}

impl ExpressionStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
    ) -> Self {
        ExpressionStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            expression,
        }
    }
}

impl ReturnStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
    ) -> Self {
        ReturnStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            expression,
        }
    }
}

impl FilterStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        block: Block,
    ) -> Self {
        FilterStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            block,
        }
    }
}

impl ForStatement {
    #![allow(clippy::too_many_arguments)]
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        initializer: Option<VariableDefinitionStatement>,
        condition: Option<Expression>,
        update: Option<Expression>,
        body: Statement,
    ) -> Self {
        ForStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            initializer,
            condition,
            update,
            body: Box::new(body),
        }
    }
}

impl IfStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        condition: Expression,
        if_arm: Block,
        else_arm: Option<Block>,
    ) -> Self {
        IfStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            condition,
            if_arm,
            else_arm,
        }
    }
}

#[allow(clippy::too_many_arguments)]
impl VariableDefinitionStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
        value: Option<Expression>,
        is_undef: bool,
    ) -> Self {
        VariableDefinitionStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            type_,
            value,
            is_undef,
        }
    }
}

impl TypeDefinitionStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
    ) -> Self {
        TypeDefinitionStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
            type_,
        }
    }
}

impl AssignExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        left: Expression,
        right: Expression,
    ) -> Self {
        AssignExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

impl ArrayIndexAccessExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        array: Expression,
        index: Expression,
    ) -> Self {
        ArrayIndexAccessExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            array: Box::new(array),
            index: Box::new(index),
        }
    }
}

impl MemberAccessExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
        name: Identifier,
    ) -> Self {
        MemberAccessExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            expression: Box::new(expression),
            name,
        }
    }
}

impl FunctionCallExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        function: Expression,
        arguments: Option<Vec<Expression>>,
    ) -> Self {
        FunctionCallExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            function: Box::new(function),
            arguments,
        }
    }
}

impl PrefixUnaryExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
        operator: UnaryOperatorKind,
    ) -> Self {
        PrefixUnaryExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            expression: Box::new(expression),
            operator,
        }
    }
}

impl AssertStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
    ) -> Self {
        AssertStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            expression: Box::new(expression),
        }
    }
}

impl VerifyStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        function_call: FunctionCallExpression,
    ) -> Self {
        VerifyStatement {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            function_call: Box::new(function_call),
        }
    }
}
impl ParenthesizedExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
    ) -> Self {
        ParenthesizedExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            expression: Box::new(expression),
        }
    }
}

impl TypeOfExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        typeref: Identifier,
    ) -> Self {
        TypeOfExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            typeref,
        }
    }
}

impl BinaryExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        left: Expression,
        operator: OperatorKind,
        right: Expression,
    ) -> Self {
        BinaryExpression {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }
}

impl BoolLiteral {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        value: bool,
    ) -> Self {
        BoolLiteral {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            value,
        }
    }
}

impl ArrayLiteral {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        elements: Vec<Expression>,
    ) -> Self {
        ArrayLiteral {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            elements,
        }
    }
}

impl StringLiteral {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        value: String,
    ) -> Self {
        StringLiteral {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            value,
        }
    }
}

impl NumberLiteral {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        value: i64,
    ) -> Self {
        NumberLiteral {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            value,
        }
    }
}

impl SimpleType {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: String,
    ) -> Self {
        SimpleType {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            name,
        }
    }
}

impl GenericType {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        base: Identifier,
        parameters: Vec<Type>,
    ) -> Self {
        GenericType {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            base,
            parameters,
        }
    }
}

impl QualifiedType {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        qualifier: Identifier,
        name: Identifier,
    ) -> Self {
        QualifiedType {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            qualifier,
            name,
        }
    }
}

impl TypeArray {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        element_type: Box<Type>,
        size: Option<NumberLiteral>,
    ) -> Self {
        TypeArray {
            location: Location {
                start: Position {
                    row: start_row,
                    column: start_column,
                },
                end: Position {
                    row: end_row,
                    column: end_column,
                },
            },
            element_type,
            size,
        }
    }
}
