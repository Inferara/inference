//! Base AST node definitions.
//!
//! Defines the `Node` trait with `Location`.
use std::rc::Rc;

use crate::{ast_enum, ast_enums, ast_nodes, ast_nodes_impl, node::Node};

ast_enums! {

    pub enum Definition {
        Spec(SpecDefinition),
        Struct(StructDefinition),
        Enum(EnumDefinition),
        Constant(ConstantDefinition),
        Function(FunctionDefinition),
        ExternalFunction(ExternalFunctionDefinition),
        Type(TypeDefinition),
    }

    pub enum BlockType {
        Block(Block),
        Assume(Block),
        Forall(Block),
        Exists(Block),
        Unique(Block),
    }

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

    pub enum Literal {
        Array(Rc<ArrayLiteral>),
        Bool(Rc<BoolLiteral>),
        String(Rc<StringLiteral>),
        Number(Rc<NumberLiteral>),
        Unit(Rc<UnitLiteral>),
    }

    pub enum Type {
        Array(Box<TypeArray>),
        Simple(SimpleType),
        Generic(GenericType),
        Function(FunctionType),
        QualifiedName(QualifiedName),
        Qualified(TypeQualifiedName),
        Identifier(Identifier),
    }
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum UnaryOperatorKind {
    Neg,
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
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

ast_nodes! {

    pub struct SourceFile {
        pub use_directives: Vec<UseDirective>,
        pub definitions: Vec<Definition>,
    }

    pub struct UseDirective {
        pub imported_types: Option<Vec<Identifier>>,
        pub segments: Option<Vec<Identifier>>,
        pub from: Option<String>,
    }

    pub struct SpecDefinition {
        pub name: Identifier,
        pub definitions: Vec<Definition>,
    }

    pub struct StructDefinition {
        pub name: Identifier,
        pub fields: Vec<StructField>,
        pub methods: Vec<FunctionDefinition>,
    }

    pub struct StructField {
        pub name: Identifier,
        pub type_: Type,
    }

    pub struct EnumDefinition {
        pub name: Identifier,
        pub variants: Vec<Identifier>,
    }

    pub struct Identifier {
        pub name: String,
    }

    pub struct ConstantDefinition {
        pub name: Identifier,
        pub type_: Type,
        pub value: Literal,
    }

    pub struct FunctionDefinition {
        pub name: Identifier,
        pub parameters: Option<Vec<Parameter>>,
        pub returns: Option<Type>,
        pub body: BlockType,
    }

    pub struct ExternalFunctionDefinition {
        pub name: Identifier,
        pub arguments: Option<Vec<Identifier>>,
        pub returns: Option<Type>,
    }

    pub struct TypeDefinition {
        pub name: Identifier,
        pub type_: Type,
    }

    pub struct Parameter {
        pub name: Identifier,
        pub type_: Type,
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
        pub name: Identifier,
        pub type_: Type,
        pub value: Option<Expression>,
        pub is_undef: bool,
    }

    pub struct TypeDefinitionStatement {
        pub name: Identifier,
        pub type_: Type,
    }

    pub struct AssignExpression {
        pub left: Box<Expression>,
        pub right: Box<Expression>,
    }

    pub struct ArrayIndexAccessExpression {
        pub array: Box<Expression>,
        pub index: Box<Expression>,
    }

    pub struct MemberAccessExpression {
        pub expression: Box<Expression>,
        pub name: Identifier,
    }

    pub struct FunctionCallExpression {
        pub function: Box<Expression>,
        pub arguments: Option<Vec<(Identifier, Expression)>>,
    }

    pub struct UzumakiExpression {}

    pub struct PrefixUnaryExpression {
        pub expression: Box<Expression>,
        pub operator: UnaryOperatorKind,
    }

    pub struct AssertStatement {
        pub expression: Box<Expression>,
    }

    pub struct ParenthesizedExpression {
        pub expression: Box<Expression>,
    }

    pub struct BinaryExpression {
        pub left: Box<Expression>,
        pub operator: OperatorKind,
        pub right: Box<Expression>,
    }

    pub struct ArrayLiteral {
        pub elements: Vec<Expression>,
    }

    pub struct BoolLiteral {
        pub value: bool,
    }

    pub struct StringLiteral {
        pub value: String,
    }

    pub struct NumberLiteral {
        pub value: String,
        pub type_: Type,
    }

    pub struct UnitLiteral {}

    pub struct SimpleType {
        pub name: String,
    }

    pub struct GenericType {
        pub base: Identifier,
        pub parameters: Vec<Type>,
    }

    pub struct FunctionType {
        pub parameters: Option<Vec<Type>>,
        pub returns: Box<Type>,
    }

    pub struct QualifiedName {
        pub qualifier: Identifier,
        pub name: Identifier,
    }

    pub struct TypeQualifiedName {
        pub alias: Identifier,
        pub name: Identifier,
    }

    pub struct TypeArray {
        pub element_type: Box<Type>,
        pub size: Option<Box<Expression>>,
    }

}

ast_nodes_impl! {
    impl Node for SourceFile {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for UseDirective {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for SpecDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for StructDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for StructField {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for EnumDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for Identifier {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO revisit
            vec![]
        }
    }
    impl Node for ConstantDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for FunctionDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ExternalFunctionDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeDefinition {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for Parameter {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for Block {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ExpressionStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ReturnStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for LoopStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for BreakStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO revisit
            vec![]
        }
    }
    impl Node for IfStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for VariableDefinitionStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeDefinitionStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for AssignExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ArrayIndexAccessExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for MemberAccessExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for FunctionCallExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for UzumakiExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for PrefixUnaryExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for AssertStatement {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ParenthesizedExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for BinaryExpression {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ArrayLiteral {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for BoolLiteral {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for StringLiteral {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for NumberLiteral {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for UnitLiteral {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for SimpleType {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for GenericType {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for FunctionType {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for QualifiedName {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeQualifiedName {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeArray {
        fn children(&self) -> Vec<Box<dyn Node>> {
            //TODO implement
            vec![]
        }
    }
}
