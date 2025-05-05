use core::fmt;
use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    rc::Rc,
};

#[derive(Clone, PartialEq, Eq, Debug, Default)]
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct TypeInfo {
    pub name: String,
    pub type_params: Vec<String>,
    // (Field type information could be added here if needed for struct field checking.)
}

impl TypeInfo {
    #[must_use]
    pub fn new(ty: &Type) -> Self {
        match ty {
            Type::Simple(simple) => Self {
                name: simple.name.clone(),
                type_params: vec![],
            },
            Type::Generic(generic) => Self {
                name: generic.base.name.clone(),
                type_params: generic.parameters.iter().map(|p| p.name.clone()).collect(),
            },
            Type::QualifiedName(qualified_name) => Self {
                name: qualified_name.qualifier.name.clone(),
                type_params: vec![],
            },
            Type::Qualified(qualified) => Self {
                name: qualified.alias.name.clone(),
                type_params: vec![],
            },
            Type::Array(array) => Self {
                name: format!("Array<{}>", TypeInfo::new(&array.element_type).name),
                type_params: vec![],
            },
            Type::Function(func) => {
                //REVISIT
                let param_types = func
                    .parameters
                    .as_ref()
                    .map(|params| params.iter().map(TypeInfo::new).collect::<Vec<_>>())
                    .unwrap_or_default();
                let return_type = if func.returns.is_some() {
                    Self::new(func.returns.as_ref().unwrap())
                } else {
                    Self {
                        name: "Unit".to_string(),
                        type_params: vec![],
                    }
                };
                Self {
                    name: format!("Function<{}, {}>", param_types.len(), return_type.name),
                    type_params: vec![],
                }
            }
            Type::Custom(custom) => Self {
                name: custom.name.clone(),
                type_params: vec![],
            },
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
        #[derive(Clone, PartialEq, Eq, Debug)]
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
            pub fn id(&self) -> u32 {
                match self {
                    $(
                        $name::$arm(n, ..) => { ast_enum!(@id_arm n, $($conv)?) }
                    )*
                }
            }

            #[must_use]
            pub fn location(&self) -> Location {
                match self {
                    $(
                        $name::$arm(n, ..) => { ast_enum!(@location_arm n, $($conv)?) }
                    )*
                }
            }
        }
    };

    (@id_arm $inner:ident, inner_enum) => {
        $inner.id()
    };

    (@id_arm $inner:ident, ) => {
        $inner.id
    };

    (@location_arm $inner:ident, inner_enum) => {
        $inner.location()
    };

    (@location_arm $inner:ident, ) => {
        $inner.location.clone()
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
        MemberAccess(Rc<MemberAccessExpression>),
        FunctionCall(Rc<FunctionCallExpression>),
        PrefixUnary(Rc<PrefixUnaryExpression>),
        Parenthesized(Rc<ParenthesizedExpression>),
        Binary(Rc<BinaryExpression>),
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
        Simple(Rc<SimpleType>),
        Generic(Rc<GenericType>),
        Function(Rc<FunctionType>),
        QualifiedName(Rc<QualifiedName>),
        Qualified(Rc<TypeQualifiedName>),
        Custom(Rc<Identifier>),
    }

    pub enum Misc {
        StructField(Rc<StructField>),
        Parameter(Rc<Parameter>),
    }
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

ast_nodes! {

    pub struct SourceFile {
        pub directives: Vec<Directive>,
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
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct ConstantDefinition {
        pub name: Rc<Identifier>,
        pub ty: Type,
        pub value: Literal,
    }

    pub struct FunctionDefinition {
        pub name: Rc<Identifier>,
        pub type_parameters: Option<Vec<Rc<Identifier>>>,
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
        pub ty: Type,
    }

    pub struct Parameter {
        pub name: Rc<Identifier>,
        pub ty: Type,
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
        pub array: Expression,
        pub index: Expression,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct MemberAccessExpression {
        pub expression: Expression,
        pub name: Rc<Identifier>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct TypeMemberAccessExpression {
        pub expression: Expression,
        pub name: Rc<Identifier>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct FunctionCallExpression {
        pub function: Expression,
        pub arguments: Option<Vec<(Option<Rc<Identifier>>, Expression)>>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct UzumakiExpression {
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct PrefixUnaryExpression {
        pub expression: Expression,
        pub expression: Expression,
        pub operator: UnaryOperatorKind,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct AssertStatement {
        pub expression: Expression,
        pub expression: Expression,
    }

    pub struct ParenthesizedExpression {
        pub expression: RefCell<Expression>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct BinaryExpression {
        pub left: Expression,
        pub left: Expression,
        pub operator: OperatorKind,
        pub right: Expression,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct ArrayLiteral {
        pub elements: Vec<Expression>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct BoolLiteral {
        pub value: bool,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct StringLiteral {
        pub value: String,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct NumberLiteral {
        pub value: String,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct UnitLiteral {
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct SimpleType {
        pub name: String,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct GenericType {
        pub base: Rc<Identifier>,
        pub parameters: Vec<Rc<Identifier>>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct FunctionType {
        pub parameters: Option<Vec<Type>>,
        pub returns: Option<Type>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct QualifiedName {
        pub qualifier: Rc<Identifier>,
        pub name: Rc<Identifier>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct TypeQualifiedName {
        pub alias: Rc<Identifier>,
        pub name: Rc<Identifier>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

    pub struct TypeArray {
        pub element_type: Type,
        pub size: Option<Expression>,
        pub type_info: RefCell<Option<TypeInfo>>
    }

}
