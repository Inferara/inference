//! Base AST node definitions.
//!
//! Defines the `Node` trait with `Location`.
use std::rc::Rc;

use crate::{ast_enum, ast_enums, ast_nodes, ast_nodes_impl, node::Node};

ast_enums! {

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
        Expression(Rc<ExpressionStatement>),
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
        Assign(Rc<AssignExpression>),
        ArrayIndexAccess(Rc<ArrayIndexAccessExpression>),
        MemberAccess(Rc<MemberAccessExpression>),
        FunctionCall(Rc<FunctionCallExpression>),
        PrefixUnary(Rc<PrefixUnaryExpression>),
        Parenthesized(Rc<ParenthesizedExpression>),
        Binary(Rc<BinaryExpression>),
        Literal(Literal),
        Identifier(Rc<Identifier>),
        Type(Type),
        Uzumaki(Rc<UzumakiExpression>),
    }

    pub enum Literal {
        Array(Rc<ArrayLiteral>),
        Bool(Rc<BoolLiteral>),
        String(Rc<StringLiteral>),
        Number(Rc<NumberLiteral>),
        Unit(Rc<UnitLiteral>),
    }

    pub enum Type {
        Array(Rc<TypeArray>),
        Simple(Rc<SimpleType>),
        Generic(Rc<GenericType>),
        Function(Rc<FunctionType>),
        QualifiedName(Rc<QualifiedName>),
        Qualified(Rc<TypeQualifiedName>),
        Identifier(Rc<Identifier>),
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
        pub use_directives: Vec<Rc<UseDirective>>,
        pub definitions: Vec<Definition>,
    }

    pub struct UseDirective {
        pub imported_types: Option<Vec<Rc<Identifier>>>,
        pub segments: Option<Vec<Rc<Identifier>>>,
        pub from: Option<String>,
    }

    pub struct SpecDefinition {
        pub name: Rc<Identifier>,
        pub definitions: Vec<Definition>,
    }

    pub struct StructDefinition {
        pub name: Rc<Identifier>,
        pub fields: Vec<Rc<StructField>>,
        pub methods: Vec<Rc<FunctionDefinition>>,
    }

    pub struct StructField {
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct EnumDefinition {
        pub name: Rc<Identifier>,
        pub variants: Vec<Rc<Identifier>>,
    }

    pub struct Identifier {
        pub name: String,
    }

    pub struct ConstantDefinition {
        pub name: Rc<Identifier>,
        pub type_: Type,
        pub value: Literal,
    }

    pub struct FunctionDefinition {
        pub name: Rc<Identifier>,
        pub parameters: Option<Vec<Rc<Parameter>>>,
        pub returns: Option<Type>,
        pub body: BlockType,
    }

    pub struct ExternalFunctionDefinition {
        pub name: Rc<Identifier>,
        pub arguments: Option<Vec<Rc<Identifier>>>,
        pub returns: Option<Type>,
    }

    pub struct TypeDefinition {
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct Parameter {
        pub name: Rc<Identifier>,
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
        pub name: Rc<Identifier>,
        pub type_: Type,
        pub value: Option<Expression>,
        pub is_undef: bool,
    }

    pub struct TypeDefinitionStatement {
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct AssignExpression {
        pub left: Expression,
        pub right: Expression,
    }

    pub struct ArrayIndexAccessExpression {
        pub array: Expression,
        pub index: Expression,
    }

    pub struct MemberAccessExpression {
        pub expression: Expression,
        pub name: Rc<Identifier>,
    }

    pub struct FunctionCallExpression {
        pub function: Expression,
        pub arguments: Option<Vec<(Rc<Identifier>, Expression)>>,
    }

    pub struct UzumakiExpression {}

    pub struct PrefixUnaryExpression {
        pub expression: Expression,
        pub operator: UnaryOperatorKind,
    }

    pub struct AssertStatement {
        pub expression: Expression,
    }

    pub struct ParenthesizedExpression {
        pub expression: Expression,
    }

    pub struct BinaryExpression {
        pub left: Expression,
        pub operator: OperatorKind,
        pub right: Expression,
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
        pub base: Rc<Identifier>,
        pub parameters: Vec<Type>,
    }

    pub struct FunctionType {
        pub parameters: Option<Vec<Type>>,
        pub returns: Type,
    }

    pub struct QualifiedName {
        pub qualifier: Rc<Identifier>,
        pub name: Rc<Identifier>,
    }

    pub struct TypeQualifiedName {
        pub alias: Rc<Identifier>,
        pub name: Rc<Identifier>,
    }

    pub struct TypeArray {
        pub element_type: Type,
        pub size: Option<Expression>,
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
