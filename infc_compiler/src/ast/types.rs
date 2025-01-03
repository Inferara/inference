#![allow(dead_code)]

use core::fmt;
use std::fmt::{Display, Formatter};

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Position {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Location {
    pub start: Position,
    pub end: Position,
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.start.row, self.start.column)
    }
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
pub struct SpecDefinition {
    pub location: Location,
    pub name: Identifier,
    pub definitions: Vec<Definition>,
}

#[derive(Debug)]
pub struct StructDefinition {
    pub location: Location,
    pub name: Identifier,
    pub fields: Vec<StructField>,
    pub methods: Vec<FunctionDefinition>,
}

#[derive(Debug)]
pub struct StructField {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
}

#[derive(Debug)]
pub struct EnumDefinition {
    pub location: Location,
    pub name: Identifier,
    pub variants: Vec<Identifier>,
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Identifier {
    pub location: Location,
    pub name: String,
}

#[derive(Debug)]
pub enum Definition {
    Spec(SpecDefinition),
    Struct(StructDefinition),
    Enum(EnumDefinition),
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
    pub parameters: Option<Vec<Parameter>>,
    pub returns: Option<Type>,
    pub body: BlockType,
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
pub struct Parameter {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
}

#[derive(Debug)]
pub enum BlockType {
    Block(Block),
    Assume(Block),
    Forall(Block),
    Exists(Block),
    Unique(Block),
}

#[derive(Debug)]
pub struct Block {
    pub location: Location,
    pub statements: Vec<Statement>,
}

#[derive(Debug)]
pub enum Statement {
    Block(BlockType),
    Expression(ExpressionStatement),
    Return(ReturnStatement),
    Loop(LoopStatement),
    Break(BreakStatement),
    If(IfStatement),
    VariableDefinition(VariableDefinitionStatement),
    TypeDefinition(TypeDefinitionStatement),
    Assert(AssertStatement),
    ConstantDefinition(ConstantDefinition),
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
pub struct LoopStatement {
    pub location: Location,
    pub condition: Option<Expression>,
    pub body: BlockType,
}

#[derive(Debug)]
pub struct BreakStatement {
    pub location: Location,
}

#[derive(Debug)]
pub struct IfStatement {
    pub location: Location,
    pub condition: Expression,
    pub if_arm: BlockType,
    pub else_arm: Option<BlockType>,
}

#[derive(Debug)]
pub struct VariableDefinitionStatement {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
    pub value: Option<Expression>,
    pub is_undef: bool,
}

#[derive(Debug)]
pub struct TypeDefinitionStatement {
    pub location: Location,
    pub name: Identifier,
    pub type_: Type,
}

#[derive(Debug)]
pub enum Expression {
    Assign(Box<AssignExpression>),
    ArrayIndexAccess(Box<ArrayIndexAccessExpression>),
    MemberAccess(Box<MemberAccessExpression>),
    FunctionCall(Box<FunctionCallExpression>),
    PrefixUnary(Box<PrefixUnaryExpression>),
    Parenthesized(Box<ParenthesizedExpression>),
    Binary(Box<BinaryExpression>),
    Literal(Literal),
    Identifier(Identifier),
    Type(Box<Type>),
    Uzumaki(UzumakiExpression),
}

#[derive(Debug)]
pub struct AssignExpression {
    pub location: Location,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
}

#[derive(Debug)]
pub struct ArrayIndexAccessExpression {
    pub location: Location,
    pub array: Box<Expression>,
    pub index: Box<Expression>,
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
    pub arguments: Option<Vec<(Identifier, Expression)>>,
}

#[derive(Debug)]
pub struct UzumakiExpression {
    pub location: Location,
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
pub struct AssertStatement {
    pub location: Location,
    pub expression: Box<Expression>,
}

#[derive(Debug)]
pub struct ParenthesizedExpression {
    pub location: Location,
    pub expression: Box<Expression>,
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
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,
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
    Array(ArrayLiteral),
    Bool(BoolLiteral),
    String(StringLiteral),
    Number(NumberLiteral),
    Unit(UnitLiteral),
}

#[derive(Debug)]
pub struct ArrayLiteral {
    pub location: Location,
    pub elements: Vec<Expression>,
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
    pub value: String,
    pub type_: Type,
}

#[derive(Debug)]
pub struct UnitLiteral {
    pub location: Location,
}

#[derive(Debug)]
pub enum Type {
    Array(Box<TypeArray>),
    Simple(SimpleType),
    Generic(GenericType),
    Function(FunctionType),
    QualifiedName(QualifiedName),
    Qualified(TypeQualifiedName),
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
pub struct FunctionType {
    pub location: Location,
    pub parameters: Option<Vec<Type>>,
    pub returns: Box<Type>,
}

#[derive(Debug)]
pub struct QualifiedName {
    pub location: Location,
    pub qualifier: Identifier,
    pub name: Identifier,
}

#[derive(Debug)]
pub struct TypeQualifiedName {
    pub location: Location,
    pub alias: Identifier,
    pub name: Identifier,
}

#[derive(Debug)]
pub struct TypeArray {
    pub location: Location,
    pub element_type: Box<Type>,
    pub size: Option<Box<Expression>>,
}
