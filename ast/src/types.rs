use core::fmt;
use std::{
    fmt::{Display, Formatter},
    rc::Rc,
};

#[derive(Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Location {
    pub offset_start: u32,
    pub offset_end: u32,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub source: String,
}

impl Location {
    #[must_use]
    pub fn new(
        offset_start: u32,
        offset_end: u32,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        source: String,
    ) -> Self {
        Self {
            offset_start,
            offset_end,
            start_line,
            start_column,
            end_line,
            end_column,
            source,
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Location {{ offset_start: {}, offset_end: {}, start_line: {}, start_column: {}, end_line: {}, end_column: {}, source: {} }}",
            self.offset_start, self.offset_end, self.start_line, self.start_column, self.end_line, self.end_column, self.source
        )
    }
}

#[macro_export]
macro_rules! ast_node {
    (
        $(#[$outer:meta])*
        $struct_vis:vis struct $name:ident {
            $(
                $(#[$field_attr:meta])*
                $field_vis:vis $field_name:ident : $field_ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$outer])*
        #[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
        $struct_vis struct $name {
            pub id: u32,
            pub location: $crate::types::Location,
            $(
                $(#[$field_attr])*
                $field_vis $field_name : $field_ty,
            )*
        }
    };
}

macro_rules! ast_nodes {
    (
        $(
            $(#[$outer:meta])*
            $struct_vis:vis struct $name:ident { $($fields:tt)* }
        )+
    ) => {
        $(
            ast_node! {
                $(#[$outer])*
                $struct_vis struct $name { $($fields)* }
            }
        )+

        #[derive(Clone, Debug)]
        pub enum AstNode {
            $(
                $name(std::rc::Rc<$name>),
            )+
        }

        impl AstNode {
            #[must_use]
            pub fn id(&self) -> u32 {
                match self {
                    $(
                        AstNode::$name(node) => node.id,
                    )+
                }
            }
        }
    };
}

macro_rules! ast_enum {
    (
        $(#[$outer:meta])*
        $enum_vis:vis enum $name:ident {
            $(
                $(#[$arm_attr:meta])*
                $(@$conv:ident)? $arm:ident $( ( $($tuple:tt)* ) )? $( { $($struct:tt)* } )? ,
            )*
        }
    ) => {
        $(#[$outer])*
        #[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
        $enum_vis enum $name {
            $(
                $(#[$arm_attr])*
                $arm $( ( $($tuple)* ) )? $( { $($struct)* } )? ,
            )*
        }
    }
}

macro_rules! ast_enums {
    (
        $(
            $(#[$outer:meta])*
            $enum_vis:vis enum $name:ident { $($arms:tt)* }
        )+
    ) => {
        $(
            ast_enum! {
                $(#[$outer])*
                $enum_vis enum $name { $($arms)* }
            }
        )+
    };
}

ast_enums! {

    pub enum Definition {
        Spec(Rc<SpecDefinition>),
        Struct(Rc<StructDefinition>),
        Enum(Rc<EnumDefinition>),
        Constant(Rc<ConstantDefinition>),
        Function(Rc<FunctionDefinition>),
        ExternalFunction(Rc<ExternalFunctionDefinition>),
        Type(Rc<TypeDefinition>),
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
        Assign(Rc<AssignExpression>),
        Block(BlockType),
        Expression(Expression),
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
        Assign(Rc<AssignExpression>),//TODO add type
        ArrayIndexAccess(Rc<ArrayIndexAccessExpression>),//TODO add type
        MemberAccess(Rc<MemberAccessExpression>),//TODO add type
        FunctionCall(Rc<FunctionCallExpression>),//TODO add type
        PrefixUnary(Rc<PrefixUnaryExpression>),//TODO add type
        Parenthesized(Rc<ParenthesizedExpression>),//TODO add type
        Binary(Rc<BinaryExpression>),//TODO add type
        Literal(Literal),//TODO add type
        Identifier(Rc<Identifier>),//TODO add type
        Type(Type),//TODO add type
        Uzumaki(Rc<UzumakiExpression>),//TODO add type
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
pub enum BlockType {
    Block(Rc<Block>),
    Assume(Rc<Block>),
    Forall(Rc<Block>),
    Exists(Rc<Block>),
    Unique(Rc<Block>),
}

impl BlockType {
    #[must_use]
    pub fn id(&self) -> u32 {
        match self {
            BlockType::Block(b)
            | BlockType::Assume(b)
            | BlockType::Forall(b)
            | BlockType::Exists(b)
            | BlockType::Unique(b) => b.id(),
        }
    }

    #[must_use]
    pub fn location(&self) -> crate::node::Location {
        match self {
            BlockType::Block(b)
            | BlockType::Assume(b)
            | BlockType::Forall(b)
            | BlockType::Exists(b)
            | BlockType::Unique(b) => b.location(),
        }
    }

    #[must_use]
    pub fn children(&self) -> Vec<NodeKind> {
        match self {
            BlockType::Block(b)
            | BlockType::Assume(b)
            | BlockType::Forall(b)
            | BlockType::Exists(b)
            | BlockType::Unique(b) => b.children(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum UnaryOperatorKind {
    Neg,
}

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
        Custom(Rc<Identifier>),
    }
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
        pub name: Rc<Identifier>,
        pub definitions: Vec<Definition>,
    }

    pub struct StructDefinition {
        pub name: Rc<Identifier>,
        pub fields: Vec<Rc<StructField>>,
        pub methods: Vec<Rc<FunctionDefinition>>,
        pub name: Rc<Identifier>,
        pub fields: Vec<Rc<StructField>>,
        pub methods: Vec<Rc<FunctionDefinition>>,
    }

    pub struct StructField {
        pub name: Rc<Identifier>,
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct EnumDefinition {
        pub name: Rc<Identifier>,
        pub variants: Vec<Rc<Identifier>>,
        pub name: Rc<Identifier>,
        pub variants: Vec<Rc<Identifier>>,
    }

    pub struct Identifier {
        pub name: String,
    }

    pub struct ConstantDefinition {
        pub name: Rc<Identifier>,
        pub name: Rc<Identifier>,
        pub type_: Type,
        pub value: Literal,
    }

    pub struct FunctionDefinition {
        pub name: Rc<Identifier>,
        pub arguments: Option<Vec<Rc<Parameter>>>,
        pub returns: Option<Type>,
        pub body: BlockType,
    }

    pub struct ExternalFunctionDefinition {
        pub name: Rc<Identifier>,
        pub arguments: Option<Vec<Rc<Identifier>>>,
        pub name: Rc<Identifier>,
        pub arguments: Option<Vec<Rc<Identifier>>>,
        pub returns: Option<Type>,
    }

    pub struct TypeDefinition {
        pub name: Rc<Identifier>,
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct Parameter {
        pub name: Rc<Identifier>,
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
        pub name: Rc<Identifier>,
        pub type_: Type,
        pub value: Option<Expression>,
        pub is_uzumaki: bool,
    }

    pub struct TypeDefinitionStatement {
        pub name: Rc<Identifier>,
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct AssignExpression {
        pub left: Expression,
        pub right: Expression,
        pub left: Expression,
        pub right: Expression,
    }

    pub struct ArrayIndexAccessExpression {
        pub array: Expression,
        pub index: Expression,
        pub array: Expression,
        pub index: Expression,
    }

    pub struct MemberAccessExpression {
        pub expression: Expression,
        pub name: Rc<Identifier>,
    }

    pub struct TypeMemberAccessExpression {
        pub expression: Expression,
        pub name: Rc<Identifier>,
    }

    pub struct FunctionCallExpression {
        pub function: Expression,
        pub arguments: Option<Vec<(Option<Rc<Identifier>>, Expression)>>,
    }

    pub struct UzumakiExpression {
        pub ty: Type,
    }

    pub struct PrefixUnaryExpression {
        pub expression: Expression,
        pub expression: Expression,
        pub operator: UnaryOperatorKind,
    }

    pub struct AssertStatement {
        pub expression: Expression,
        pub expression: Expression,
    }

    pub struct ParenthesizedExpression {
        pub expression: Expression,
        pub expression: Expression,
    }

    pub struct BinaryExpression {
        pub left: Expression,
        pub left: Expression,
        pub operator: OperatorKind,
        pub right: Expression,
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
        pub qualifier: Rc<Identifier>,
        pub name: Rc<Identifier>,
    }

    pub struct TypeQualifiedName {
        pub alias: Rc<Identifier>,
        pub name: Rc<Identifier>,
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
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for UseDirective {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for SpecDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for StructDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for StructField {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for EnumDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for Identifier {
        fn children(&self) -> Vec<NodeKind> {
            //TODO revisit
            vec![]
        }
    }
    impl Node for ConstantDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for FunctionDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ExternalFunctionDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeDefinition {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for Parameter {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for Block {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ExpressionStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ReturnStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for LoopStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for BreakStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO revisit
            vec![]
        }
    }
    impl Node for IfStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for VariableDefinitionStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeDefinitionStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for AssignExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ArrayIndexAccessExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for MemberAccessExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeMemberAccessExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for FunctionCallExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for UzumakiExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for PrefixUnaryExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for AssertStatement {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ParenthesizedExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for BinaryExpression {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for ArrayLiteral {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for BoolLiteral {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for StringLiteral {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for NumberLiteral {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for UnitLiteral {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for SimpleType {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for GenericType {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for FunctionType {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for QualifiedName {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeQualifiedName {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
    impl Node for TypeArray {
        fn children(&self) -> Vec<NodeKind> {
            //TODO implement
            vec![]
        }
    }
}
