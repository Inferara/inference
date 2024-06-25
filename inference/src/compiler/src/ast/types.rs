#[derive(Debug, Clone)]
pub struct Position {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug)]
pub struct SourceFile {
    pub location: Location,
    pub use_directives: Vec<UseDirective>,
    pub context_definitions: Vec<ContextDefinition>,
    pub definitions: Vec<Definition>,
}

impl SourceFile {
    pub fn new(location: Location) -> Self {
        SourceFile {
            location,
            use_directives: Vec::new(),
            context_definitions: Vec::new(),
            definitions: Vec::new(),
        }
    }

    pub fn add_use_directive(&mut self, use_directive: UseDirective) {
        self.use_directives.push(use_directive);
    }

    pub fn add_context_definition(&mut self, context_definition: ContextDefinition) {
        self.context_definitions.push(context_definition);
    }

    pub fn add_definition(&mut self, definition: Definition) {
        self.definitions.push(definition);
    }
}

#[derive(Debug)]
pub struct UseDirective {
    pub location: Location,
    pub imported_types: Option<Vec<Identifier>>,
    pub segments: Option<Vec<Identifier>>,
    pub from: Option<String>,
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

#[derive(Debug)]
pub struct ContextDefinition {
    pub location: Location,
    pub name: Identifier,
    pub definitions: Vec<Definition>,
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

#[derive(Debug, Clone)]
pub struct Identifier {
    pub location: Location,
    pub name: String,
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

#[derive(Debug)]
pub enum Definition {
    Constant(ConstantDefinition),
    Function(FunctionDefinition),
    ExternalFunction(ExternalFunctionDefinition),
    Type(TypeDefinition),
}

#[derive(Debug)]
pub struct ConstantDefinition {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
    pub value: Literal,
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

#[derive(Debug)]
pub struct FunctionDefinition {
    pub location: Location,
    pub name: Identifier,
    pub arguments: Option<Vec<Argument>>,
    pub returns: Option<Type>,
    pub body: Block,
}

impl FunctionDefinition {
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

#[derive(Debug)]
pub struct ExternalFunctionDefinition {
    pub location: Location,
    pub name: Identifier,
    pub arguments: Option<Vec<Identifier>>,
    pub returns: Option<Type>,
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

#[derive(Debug)]
pub struct TypeDefinition {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
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

#[derive(Debug)]
pub struct Argument {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
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

#[derive(Debug)]
pub struct Block {
    pub location: Location,
    pub statements: Vec<Statement>,
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

#[derive(Debug)]
pub enum Statement {
    Block(Block),
    Expression(ExpressionStatement),
    Return(ReturnStatement),
    Filter(FilterStatement),
    For(ForStatement),
    If(IfStatement),
    VariableDefinition(VariableDefinitionStatement),
    TypeDefinition(TypeDefinitionStatement),
}

#[derive(Debug)]
pub struct ExpressionStatement {
    pub location: Location,
    pub expression: Expression,
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

#[derive(Debug)]
pub struct ReturnStatement {
    pub location: Location,
    pub expression: Expression,
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

#[derive(Debug)]
pub struct FilterStatement {
    pub location: Location,
    pub block: Block,
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

#[derive(Debug)]
pub struct ForStatement {
    pub location: Location,
    pub initializer: Option<VariableDefinitionStatement>,
    pub condition: Option<Expression>,
    pub update: Option<Expression>,
    pub body: Box<Statement>,
}

impl ForStatement {
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

#[derive(Debug)]
pub struct IfStatement {
    pub location: Location,
    pub condition: Expression,
    pub if_arm: Block,
    pub else_arm: Option<Block>,
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

#[derive(Debug)]
pub struct VariableDefinitionStatement {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
    pub value: Option<Expression>,
}

impl VariableDefinitionStatement {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        name: Identifier,
        type_: Type,
        value: Option<Expression>,
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
        }
    }
}

#[derive(Debug)]
pub struct TypeDefinitionStatement {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
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

#[derive(Debug)]
pub enum Expression {
    Assign(AssignExpression),
    MemberAccess(MemberAccessExpression),
    FunctionCall(FunctionCallExpression),
    PrefixUnary(PrefixUnaryExpression),
    Assert(AssertExpression),
    Apply(ApplyExpression),
    Parenthesized(ParenthesizedExpression),
    TypeOf(TypeOfExpression),
    Binary(BinaryExpression),
    Literal(Literal),
    Identifier(Identifier),
    Type(Type),
}

#[derive(Debug)]
pub struct AssignExpression {
    pub location: Location,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
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

#[derive(Debug)]
pub struct MemberAccessExpression {
    pub location: Location,
    pub expression: Box<Expression>,
    pub name: Identifier,
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

#[derive(Debug)]
pub struct FunctionCallExpression {
    pub location: Location,
    pub function: Box<Expression>,
    pub arguments: Vec<Expression>,
}

impl FunctionCallExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        function: Expression,
        arguments: Vec<Expression>,
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

#[derive(Debug)]
pub struct PrefixUnaryExpression {
    pub location: Location,
    pub expression: Box<Expression>,
}

impl PrefixUnaryExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
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
        }
    }
}

#[derive(Debug)]
pub struct AssertExpression {
    pub location: Location,
    pub expression: Box<Expression>,
}

impl AssertExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        expression: Expression,
    ) -> Self {
        AssertExpression {
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

#[derive(Debug)]
pub struct ApplyExpression {
    pub location: Location,
    pub function_call: Box<FunctionCallExpression>,
}

impl ApplyExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        function_call: FunctionCallExpression,
    ) -> Self {
        ApplyExpression {
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

#[derive(Debug)]
pub struct ParenthesizedExpression {
    pub location: Location,
    pub expression: Box<Expression>,
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

#[derive(Debug)]
pub struct TypeOfExpression {
    pub location: Location,
    pub typeref: Identifier,
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

#[derive(Debug)]
pub struct BinaryExpression {
    pub location: Location,
    pub left: Box<Expression>,
    pub operator: String,
    pub right: Box<Expression>,
}

impl BinaryExpression {
    pub fn new(
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        left: Expression,
        operator: String,
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

#[derive(Debug)]
pub enum Literal {
    Bool(BoolLiteral),
    String(StringLiteral),
    Number(NumberLiteral),
}

#[derive(Debug)]
pub struct BoolLiteral {
    pub location: Location,
    pub value: bool,
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

#[derive(Debug)]
pub struct StringLiteral {
    pub location: Location,
    pub value: String,
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

#[derive(Debug)]
pub struct NumberLiteral {
    pub location: Location,
    pub value: i64,
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

#[derive(Debug)]
pub enum Type {
    Simple(SimpleType),
    Generic(GenericType),
    Qualified(QualifiedType),
    Identifier(Identifier),
}

#[derive(Debug)]
pub struct SimpleType {
    pub location: Location,
    pub name: String,
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

#[derive(Debug)]
pub struct GenericType {
    pub location: Location,
    pub base: Identifier,
    pub parameters: Vec<Type>,
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

#[derive(Debug)]
pub struct QualifiedType {
    pub location: Location,
    pub qualifier: Identifier,
    pub name: Identifier,
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
