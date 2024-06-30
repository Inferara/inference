#![allow(dead_code)]

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
    pub definitions: Vec<Definition>,
}

#[derive(Debug)]
pub struct UseDirective {
    pub location: Location,
    pub imported_types: Option<Vec<Identifier>>,
    pub segments: Option<Vec<Identifier>>,
    pub from: Option<String>,
}

#[derive(Debug)]
pub struct ContextDefinition {
    pub location: Location,
    pub name: Identifier,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone)]
pub struct Identifier {
    pub location: Location,
    pub name: String,
}

#[derive(Debug)]
pub enum Definition {
    Context(ContextDefinition),
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

#[derive(Debug)]
pub struct FunctionDefinition {
    pub location: Location,
    pub name: Identifier,
    pub arguments: Option<Vec<Argument>>,
    pub returns: Option<Type>,
    pub body: Block,
}

#[derive(Debug)]
pub struct ExternalFunctionDefinition {
    pub location: Location,
    pub name: Identifier,
    pub arguments: Option<Vec<Identifier>>,
    pub returns: Option<Type>,
}

#[derive(Debug)]
pub struct TypeDefinition {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
}

#[derive(Debug)]
pub struct Argument {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
}

#[derive(Debug)]
pub struct Block {
    pub location: Location,
    pub statements: Vec<Statement>,
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

#[derive(Debug)]
pub struct ReturnStatement {
    pub location: Location,
    pub expression: Expression,
}

#[derive(Debug)]
pub struct FilterStatement {
    pub location: Location,
    pub block: Block,
}

#[derive(Debug)]
pub struct ForStatement {
    pub location: Location,
    pub initializer: Option<VariableDefinitionStatement>,
    pub condition: Option<Expression>,
    pub update: Option<Expression>,
    pub body: Box<Statement>,
}

#[derive(Debug)]
pub struct IfStatement {
    pub location: Location,
    pub condition: Expression,
    pub if_arm: Block,
    pub else_arm: Option<Block>,
}

#[derive(Debug)]
pub struct VariableDefinitionStatement {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
    pub value: Option<Expression>,
}

#[derive(Debug)]
pub struct TypeDefinitionStatement {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
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

#[derive(Debug)]
pub struct MemberAccessExpression {
    pub location: Location,
    pub expression: Box<Expression>,
    pub name: Identifier,
}

#[derive(Debug)]
pub struct FunctionCallExpression {
    pub location: Location,
    pub function: Box<Expression>,
    pub arguments: Option<Vec<Expression>>,
}

#[derive(Debug)]
pub enum UnaryOperatorKind {
    Neg,
}

#[derive(Debug)]
pub struct PrefixUnaryExpression {
    pub location: Location,
    pub expression: Box<Expression>,
    pub operator: UnaryOperatorKind,
}

#[derive(Debug)]
pub struct AssertExpression {
    pub location: Location,
    pub expression: Box<Expression>,
}

#[derive(Debug)]
pub struct ApplyExpression {
    pub location: Location,
    pub function_call: Box<FunctionCallExpression>,
}

#[derive(Debug)]
pub struct ParenthesizedExpression {
    pub location: Location,
    pub expression: Box<Expression>,
}

#[derive(Debug)]
pub struct TypeOfExpression {
    pub location: Location,
    pub typeref: Identifier,
}

#[derive(Debug)]
pub enum OperatorKind {
    Pow,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug)]
pub struct BinaryExpression {
    pub location: Location,
    pub left: Box<Expression>,
    pub operator: OperatorKind,
    pub right: Box<Expression>,
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

#[derive(Debug)]
pub struct StringLiteral {
    pub location: Location,
    pub value: String,
}

#[derive(Debug)]
pub struct NumberLiteral {
    pub location: Location,
    pub value: i64,
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

#[derive(Debug)]
pub struct GenericType {
    pub location: Location,
    pub base: Identifier,
    pub parameters: Vec<Type>,
}

#[derive(Debug)]
pub struct QualifiedType {
    pub location: Location,
    pub qualifier: Identifier,
    pub name: Identifier,
}
