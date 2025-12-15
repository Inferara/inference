use std::rc::Rc;

use crate::type_info::TypeInfo;

pub enum Directive {
    Use(Rc<UseDirective>),
}

pub enum Definition {
    Spec(Rc<SpecDefinition>),
    Struct(Rc<StructDefinition>),
    Enum(Rc<EnumDefinition>),
    Constant(Rc<ConstantDefinition>),
    Function(Rc<FunctionDefinition>),
    ExternalFunction(Rc<ExternalFunctionDefinition>),
    Type(Rc<TypeDefinition>),
}

pub enum BlockType {
    Block(Rc<Block>),
    Assume(Rc<Block>),
    Forall(Rc<Block>),
    Exists(Rc<Block>),
    Unique(Rc<Block>),
}

pub enum Statement {
    Block(BlockType),
    Expression(Expression),
    Assign(Rc<AssignStatement>),
    Return(Rc<ReturnStatement>),
    Loop(Rc<LoopStatement>),
    Break(Rc<BreakStatement>),
    If(Rc<IfStatement>),
    VariableDefinition(Rc<VariableDefinitionStatement>),
    TypeDefinition(Rc<TypeDefinitionStatement>),
    Assert(Rc<AssertStatement>),
    ConstantDefinition(Rc<ConstantDefinition>),
}

pub enum Expression {
    ArrayIndexAccess(Rc<ArrayIndexAccessExpression>),
    Binary(Rc<BinaryExpression>),
    MemberAccess(Rc<MemberAccessExpression>),
    TypeMemberAccess(Rc<TypeMemberAccessExpression>),
    FunctionCall(Rc<FunctionCallExpression>),
    Struct(Rc<StructExpression>),
    PrefixUnary(Rc<PrefixUnaryExpression>),
    Parenthesized(Rc<ParenthesizedExpression>),
    Literal(Literal),
    TypeInfo(TypeInfo), //TODO: need it
    Uzumaki(Rc<UzumakiExpression>),
}

pub enum Literal {
    Array(Rc<ArrayLiteral>),
    Bool(Rc<BoolLiteral>),
    String(Rc<StringLiteral>),
    Number(Rc<NumberLiteral>),
    Unit(Rc<UnitLiteral>),
}

pub enum ArgumentType {
    SelfReference(Rc<SelfReference>),
    IgnoreArgument(Rc<IgnoreArgument>),
    Argument(Rc<Argument>),
    TypeInfo(TypeInfo),
}

pub enum Misc {
    StructField(Rc<StructField>),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UnaryOperatorKind {
    Neg,
}

#[derive(Clone, PartialEq, Eq, Debug)]
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

pub struct SourceFile {
    pub directives: Vec<Directive>,
    pub definitions: Vec<Definition>,
}

pub struct UseDirective {
    pub imported_types: Option<Vec<String>>,
    pub segments: Option<Vec<String>>,
    pub from: Option<String>,
}

pub struct SpecDefinition {
    pub name: String,
    pub definitions: Vec<Definition>,
}

pub struct StructDefinition {
    pub name: String,
    pub fields: Vec<Rc<StructField>>,
    pub methods: Vec<Rc<FunctionDefinition>>,
}

pub struct StructField {
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct EnumDefinition {
    pub name: String,
    pub variants: Vec<String>,
}

pub struct ConstantDefinition {
    pub name: String,
    pub type_info: TypeInfo,
    pub value: Literal,
}

pub struct FunctionDefinition {
    pub name: String,
    pub type_parameters: Option<Vec<String>>,
    pub arguments: Option<Vec<ArgumentType>>,
    pub returns: Option<TypeInfo>,
    pub body: BlockType,
}

pub struct ExternalFunctionDefinition {
    pub name: String,
    pub arguments: Option<Vec<ArgumentType>>,
    pub returns: Option<TypeInfo>,
}

pub struct TypeDefinition {
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct Argument {
    pub name: String,
    pub is_mut: bool,
    pub type_info: TypeInfo,
}

pub struct SelfReference {
    pub is_mut: bool,
}

pub struct IgnoreArgument {
    pub type_info: TypeInfo,
}

pub struct Block {
    pub statements: Vec<Statement>,
}

pub struct ExpressionStatement {
    pub expression: Expression,
}

pub struct ReturnStatement {
    pub expression: Expression,
}

pub struct LoopStatement {
    pub condition: Option<Expression>,
    pub body: BlockType,
}

pub struct BreakStatement {}

pub struct IfStatement {
    pub condition: Expression,
    pub if_arm: BlockType,
    pub else_arm: Option<BlockType>,
}

pub struct VariableDefinitionStatement {
    pub name: String,
    pub type_info: TypeInfo,
    pub value: Option<Expression>, //TODO: revisit
    pub is_uzumaki: bool,
}

pub struct TypeDefinitionStatement {
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct AssignStatement {
    pub left: Expression,
    pub right: Expression,
}

pub struct ArrayIndexAccessExpression {
    pub array: Expression,
    pub index: Expression,
    pub type_info: TypeInfo,
}

pub struct MemberAccessExpression {
    pub expression: Expression,
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct TypeMemberAccessExpression {
    pub expression: Expression,
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct FunctionCallExpression {
    pub name: String,
    pub function: Expression,
    pub type_parameters: Option<Vec<String>>,
    pub arguments: Option<Vec<(Option<String>, Expression)>>,
    pub type_info: TypeInfo,
}

pub struct StructExpression {
    pub name: String,
    pub fields: Option<Vec<(String, Expression)>>,
    pub type_info: TypeInfo,
}

pub struct UzumakiExpression {
    pub type_info: TypeInfo,
}

pub struct PrefixUnaryExpression {
    pub expression: Expression,
    pub operator: UnaryOperatorKind,
    pub type_info: TypeInfo,
}

pub struct AssertStatement {
    pub expression: Expression,
}

pub struct ParenthesizedExpression {
    pub expression: Expression,
    pub type_info: TypeInfo,
}

pub struct BinaryExpression {
    pub left: Expression,
    pub operator: OperatorKind,
    pub right: Expression,
    pub type_info: TypeInfo,
}

pub struct ArrayLiteral {
    pub elements: Option<Vec<Expression>>,
    pub type_info: TypeInfo,
}

pub struct BoolLiteral {
    pub value: bool,
}

pub struct StringLiteral {
    pub value: String,
}

pub struct NumberLiteral {
    pub value: String,
    pub type_info: TypeInfo,
}

pub struct UnitLiteral {}

pub struct SimpleType {
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct GenericType {
    pub base: String,
    pub parameters: Vec<String>,
    pub type_info: TypeInfo,
}

pub struct FunctionType {
    pub parameters: Option<Vec<TypeInfo>>,
    pub returns: Option<TypeInfo>,
}

pub struct QualifiedName {
    pub qualifier: String,
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct TypeQualifiedName {
    pub alias: String,
    pub name: String,
    pub type_info: TypeInfo,
}

pub struct TypeArray {
    pub element_type: TypeInfo,
    pub size: Option<Expression>,
}
