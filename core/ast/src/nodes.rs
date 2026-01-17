use core::fmt;
use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    rc::Rc,
};

/// Source location information for AST nodes.
///
/// Stores byte offsets and line/column positions.
/// Source text should be retrieved from the `SourceFile` using the offset range.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Location {
    pub offset_start: u32,
    pub offset_end: u32,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
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
    ) -> Self {
        Self {
            offset_start,
            offset_end,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.start_line, self.start_column)
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
        #[derive(Clone, PartialEq, Eq, Debug)]
        $struct_vis struct $name {
            pub id: u32,
            pub location: $crate::nodes::Location,
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
        #[derive(Clone, PartialEq, Eq, Debug)]
        $enum_vis enum $name {
            $(
                $(#[$arm_attr])*
                $arm $( ( $($tuple)* ) )? $( { $($struct)* } )? ,
            )*
        }

        impl $name {

            #[must_use]
            #[allow(unused_variables)]
            pub fn id(&self) -> u32 {
                match self {
                    $(
                        $name::$arm(n, ..) => { ast_enum!(@id_arm n, $($conv)?) }
                    )*
                }
            }

            #[must_use]
            #[allow(unused_variables)]
            pub fn location(&self) -> Location {
                match self {
                    $(
                        $name::$arm(n, ..) => { ast_enum!(@location_arm n, $($conv)?) }
                    )*
                }
            }
        }
    };

    // Variants marked with `skip` (e.g., `SimpleTypeKind`) do not correspond to
    // heap-allocated AST nodes and therefore have no stable ID. For these cases
    // we return `u32::MAX` as a sentinel "no ID" value. Code that performs
    // ID-based lookups must treat `u32::MAX` as invalid and never assign it to
    // any real node.
    (@id_arm $inner:ident, skip) => {
        u32::MAX
    };

    (@id_arm $inner:ident, inner_enum) => {
        $inner.id()
    };

    (@id_arm $inner:ident, ) => {
        $inner.id
    };

    (@location_arm $inner:ident, skip) => {
        Location::default()
    };

    (@location_arm $inner:ident, inner_enum) => {
        $inner.location()
    };

    (@location_arm $inner:ident, ) => {
        $inner.location
    };
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

        #[derive(Clone, Debug)]
        pub enum AstNode {
            $(
                $name($name),
            )+
        }

        impl AstNode {
            #[must_use]
            pub fn id(&self) -> u32 {
                match self {
                    $(
                        AstNode::$name(node) => node.id(),
                    )+
                }
            }

            #[must_use]
            pub fn location(&self) -> Location {
                match self {
                    $(
                        AstNode::$name(node) => node.location(),
                    )+
                }
            }

            #[must_use]
            pub fn start_line(&self) -> u32 {
                match self {
                    $(
                        AstNode::$name(node) => node.location().start_line,
                    )+
                }
            }
        }
    };
}

ast_enums! {

    pub enum Ast {
        SourceFile(Rc<SourceFile>),
    }

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
        Module(Rc<ModuleDefinition>),
    }

    pub enum BlockType {
        Block(Rc<Block>),
        Assume(Rc<Block>),
        Forall(Rc<Block>),
        Exists(Rc<Block>),
        Unique(Rc<Block>),
    }

    pub enum Statement {
        @inner_enum Block(BlockType),
        @inner_enum Expression(Expression),
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
        @inner_enum Literal(Literal),
        Identifier(Rc<Identifier>),
        @inner_enum Type(Type),
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
        @skip Simple(SimpleTypeKind),
        Generic(Rc<GenericType>),
        Function(Rc<FunctionType>),
        QualifiedName(Rc<QualifiedName>),
        Qualified(Rc<TypeQualifiedName>),
        Custom(Rc<Identifier>),
    }

    pub enum ArgumentType {
        SelfReference(Rc<SelfReference>),
        IgnoreArgument(Rc<IgnoreArgument>),
        Argument(Rc<Argument>),
        @inner_enum Type(Type),
    }

    pub enum Misc {
        StructField(Rc<StructField>),
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UnaryOperatorKind {
    Not,
    Neg,
    BitNot,
}

/// Simple type kinds for primitive built-in types.
///
/// Primitive types have dedicated variants for efficient pattern matching
/// without string comparison. User-defined types use `Type::Custom` instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SimpleTypeKind {
    Unit,
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl SimpleTypeKind {
    /// Returns the canonical lowercase source-code representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            SimpleTypeKind::Unit => "unit",
            SimpleTypeKind::Bool => "bool",
            SimpleTypeKind::I8 => "i8",
            SimpleTypeKind::I16 => "i16",
            SimpleTypeKind::I32 => "i32",
            SimpleTypeKind::I64 => "i64",
            SimpleTypeKind::U8 => "u8",
            SimpleTypeKind::U16 => "u16",
            SimpleTypeKind::U32 => "u32",
            SimpleTypeKind::U64 => "u64",
        }
    }
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

ast_nodes! {

    /// Root AST node representing a parsed source file.
    ///
    /// Stores the complete source text, enabling any node to retrieve its source
    /// via `Location::offset_start..Location::offset_end` slicing on this field.
    pub struct SourceFile {
        pub source: String,
        pub directives: Vec<Directive>,
        pub definitions: Vec<Definition>,
    }

    pub struct UseDirective {
        pub imported_types: Option<Vec<Rc<Identifier>>>,
        pub segments: Option<Vec<Rc<Identifier>>>,
        pub from: Option<String>,
    }

    pub struct SpecDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub definitions: Vec<Definition>,
    }

    pub struct StructDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub fields: Vec<Rc<StructField>>,
        pub methods: Vec<Rc<FunctionDefinition>>,
    }

    pub struct StructField {
        pub name: Rc<Identifier>,
        pub type_: Type,
    }

    pub struct EnumDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub variants: Vec<Rc<Identifier>>,
    }

    pub struct Identifier {
        pub name: String,
    }

    pub struct ConstantDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub ty: Type,
        pub value: Literal,
    }

    pub struct FunctionDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub type_parameters: Option<Vec<Rc<Identifier>>>,
        pub arguments: Option<Vec<ArgumentType>>,
        pub returns: Option<Type>,
        pub body: BlockType,
    }

    pub struct ExternalFunctionDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub arguments: Option<Vec<ArgumentType>>,
        pub returns: Option<Type>,
    }

    pub struct TypeDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub ty: Type,
    }

    pub struct ModuleDefinition {
        pub visibility: Visibility,
        pub name: Rc<Identifier>,
        pub body: Option<Vec<Definition>>,
    }

    pub struct Argument {
        pub name: Rc<Identifier>,
        pub is_mut: bool,
        pub ty: Type,
    }

    pub struct SelfReference {
        pub is_mut: bool,
    }

    pub struct IgnoreArgument {
        pub ty: Type,
    }

    pub struct Block {
        pub statements: Vec<Statement>,
    }

    pub struct ExpressionStatement {
        pub expression: Expression,
    }

    pub struct ReturnStatement {
        pub expression: RefCell<Expression>,
    }

    pub struct LoopStatement {
        pub condition: RefCell<Option<Expression>>,
        pub body: BlockType,
    }

    pub struct BreakStatement {}

    pub struct IfStatement {
        pub condition: RefCell<Expression>,
        pub if_arm: BlockType,
        pub else_arm: Option<BlockType>,
    }

    pub struct VariableDefinitionStatement {
        pub name: Rc<Identifier>,
        pub ty: Type,
        pub value: Option<RefCell<Expression>>,
        pub is_uzumaki: bool,
    }

    pub struct TypeDefinitionStatement {
        pub name: Rc<Identifier>,
        pub ty: Type,
    }

    pub struct AssignStatement {
        pub left: RefCell<Expression>,
        pub right: RefCell<Expression>,
    }

    pub struct ArrayIndexAccessExpression {
        pub array: RefCell<Expression>,
        pub index: RefCell<Expression>,
    }

    pub struct MemberAccessExpression {
        pub expression: RefCell<Expression>,
        pub name: Rc<Identifier>,
    }

    pub struct TypeMemberAccessExpression {
        pub expression: RefCell<Expression>,
        pub name: Rc<Identifier>,
    }

    pub struct FunctionCallExpression {
        pub function: Expression,
        pub type_parameters: Option<Vec<Rc<Identifier>>>,
        pub arguments: Option<Vec<(Option<Rc<Identifier>>, RefCell<Expression>)>>,
    }

    pub struct StructExpression {
        pub name: Rc<Identifier>,
        pub fields: Option<Vec<(Rc<Identifier>, RefCell<Expression>)>>,
    }

    pub struct UzumakiExpression {}

    pub struct PrefixUnaryExpression {
        pub expression: RefCell<Expression>,
        pub operator: UnaryOperatorKind,
    }

    pub struct AssertStatement {
        pub expression: RefCell<Expression>,
    }

    pub struct ParenthesizedExpression {
        pub expression: RefCell<Expression>,
    }

    pub struct BinaryExpression {
        pub left: RefCell<Expression>,
        pub operator: OperatorKind,
        pub right: RefCell<Expression>,
    }

    pub struct ArrayLiteral {
        pub elements: Option<Vec<RefCell<Expression>>>,
    }

    pub struct BoolLiteral {
        pub value: bool
    }

    pub struct StringLiteral {
        pub value: String
    }

    pub struct NumberLiteral {
        pub value: String,
    }

    pub struct UnitLiteral {
    }

    pub struct GenericType {
        pub base: Rc<Identifier>,
        pub parameters: Vec<Rc<Identifier>>,
    }

    pub struct FunctionType {
        pub parameters: Option<Vec<Type>>,
        pub returns: Option<Type>,
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
        pub size: Expression,
    }

}
