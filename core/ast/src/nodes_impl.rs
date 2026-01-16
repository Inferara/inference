use std::{cell::RefCell, rc::Rc};

use crate::nodes::{
    ArgumentType, IgnoreArgument, ModuleDefinition, SelfReference, StructExpression,
    TypeMemberAccessExpression, Visibility,
};

use super::nodes::{
    Argument, ArrayIndexAccessExpression, ArrayLiteral, AssertStatement, AssignStatement,
    BinaryExpression, Block, BlockType, BoolLiteral, BreakStatement, ConstantDefinition,
    Definition, EnumDefinition, Expression, ExpressionStatement, ExternalFunctionDefinition,
    FunctionCallExpression, FunctionDefinition, FunctionType, GenericType, Identifier, IfStatement,
    Literal, Location, LoopStatement, MemberAccessExpression, NumberLiteral, OperatorKind,
    ParenthesizedExpression, PrefixUnaryExpression, QualifiedName, ReturnStatement, SourceFile,
    SpecDefinition, Statement, StringLiteral, StructDefinition, StructField, Type, TypeArray,
    TypeDefinition, TypeDefinitionStatement, TypeQualifiedName, UnaryOperatorKind, UnitLiteral,
    UseDirective, UzumakiExpression, VariableDefinitionStatement,
};

#[macro_export]
macro_rules! ast_node_impl {
    (
        $(#[$outer:meta])*
        impl Node for $name:ident {
            $(
                $(#[$method_attr:meta])*
                fn $method:ident ( $($args:tt)* ) -> $ret:ty $body:block
            )*
        }
    ) => {
        $(#[$outer])*
        impl Node for $name {
            fn id(&self) -> u32 {
                self.id
            }

            fn location(&self) -> $crate::node::Location {
                self.location.clone()
            }

            $(
                $(#[$method_attr])*
                fn $method ( $($args)* ) -> $ret $body
            )*
        }
    };
}

#[macro_export]
macro_rules! ast_nodes_impl {
    (
        $(
            $(#[$outer:meta])*
            impl Node for $name:ident {
                $(
                    $(#[$method_attr:meta])*
                    fn $method:ident ( $($args:tt)* ) -> $ret:ty $body:block
                )*
            }
        )+
    ) => {
        $(
            $crate::ast_node_impl! {
                $(#[$outer])*
                impl Node for $name {
                    $(
                        $(#[$method_attr])*
                        fn $method ( $($args)* ) -> $ret $body
                    )*
                }
            }
        )+
    };
}

impl SourceFile {
    #[must_use]
    pub fn new(id: u32, location: Location, source: String) -> Self {
        SourceFile {
            id,
            location,
            source,
            directives: Vec::new(),
            definitions: Vec::new(),
        }
    }
}
impl SourceFile {
    #[must_use]
    pub fn specs(&self) -> Vec<Rc<SpecDefinition>> {
        self.definitions
            .iter()
            .filter_map(|def| match def {
                Definition::Spec(spec) => Some(spec.clone()),
                _ => None,
            })
            .collect()
    }
    #[must_use]
    pub fn function_definitions(&self) -> Vec<Rc<FunctionDefinition>> {
        self.definitions
            .iter()
            .filter_map(|def| match def {
                Definition::Function(func) => Some(func.clone()),
                _ => None,
            })
            .collect()
    }
}

impl BlockType {
    #[must_use]
    pub fn statements(&self) -> Vec<Statement> {
        match self {
            BlockType::Block(block)
            | BlockType::Forall(block)
            | BlockType::Assume(block)
            | BlockType::Exists(block)
            | BlockType::Unique(block) => block.statements.clone(),
        }
    }
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        match self {
            BlockType::Block(block) => block
                .statements
                .iter()
                .any(super::nodes::Statement::is_non_det),
            _ => true,
        }
    }
    #[must_use]
    pub fn is_void(&self) -> bool {
        let fn_find_ret_stmt = |statements: &Vec<Statement>| -> bool {
            for stmt in statements {
                match stmt {
                    Statement::Return(_) => return true,
                    Statement::Block(block_type) => {
                        if block_type.is_void() {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        };
        !fn_find_ret_stmt(&self.statements())
    }
}

impl Statement {
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        match self {
            Statement::Block(block_type) => !matches!(block_type, BlockType::Block(_)),
            Statement::Expression(expr_stmt) => expr_stmt.is_non_det(),
            Statement::Return(ret_stmt) => ret_stmt.expression.borrow().is_non_det(),
            Statement::Loop(loop_stmt) => loop_stmt
                .condition
                .borrow()
                .as_ref()
                .is_some_and(super::nodes::Expression::is_non_det),
            Statement::If(if_stmt) => {
                if_stmt.condition.borrow().is_non_det()
                    || if_stmt.if_arm.is_non_det()
                    || if_stmt
                        .else_arm
                        .as_ref()
                        .is_some_and(super::nodes::BlockType::is_non_det)
            }
            Statement::VariableDefinition(var_def) => var_def
                .value
                .as_ref()
                .is_some_and(|value| value.borrow().is_non_det()),
            _ => false,
        }
    }
}

impl Expression {
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        matches!(self, Expression::Uzumaki(_))
    }
}

impl UseDirective {
    #[must_use]
    pub fn new(
        id: u32,
        imported_types: Option<Vec<Rc<Identifier>>>,
        segments: Option<Vec<Rc<Identifier>>>,
        from: Option<String>,
        location: Location,
    ) -> Self {
        UseDirective {
            id,
            location,
            imported_types,
            segments,
            from,
        }
    }
}

impl SpecDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        definitions: Vec<Definition>,
        location: Location,
    ) -> Self {
        SpecDefinition {
            id,
            location,
            visibility,
            name,
            definitions,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl StructDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        fields: Vec<Rc<StructField>>,
        methods: Vec<Rc<FunctionDefinition>>,
        location: Location,
    ) -> Self {
        StructDefinition {
            id,
            location,
            visibility,
            name,
            fields,
            methods,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl StructField {
    #[must_use]
    pub fn new(id: u32, name: Rc<Identifier>, type_: Type, location: Location) -> Self {
        StructField {
            id,
            location,
            name,
            type_,
        }
    }
}

impl EnumDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        variants: Vec<Rc<Identifier>>,
        location: Location,
    ) -> Self {
        EnumDefinition {
            id,
            location,
            visibility,
            name,
            variants,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl Identifier {
    #[must_use]
    pub fn new(id: u32, name: String, location: Location) -> Self {
        Identifier { id, location, name }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

impl ConstantDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        type_: Type,
        value: Literal,
        location: Location,
    ) -> Self {
        ConstantDefinition {
            id,
            location,
            visibility,
            name,
            ty: type_,
            value,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl FunctionDefinition {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        type_parameters: Option<Vec<Rc<Identifier>>>,
        arguments: Option<Vec<ArgumentType>>,
        returns: Option<Type>,
        body: BlockType,
        location: Location,
    ) -> Self {
        FunctionDefinition {
            id,
            location,
            visibility,
            name,
            type_parameters,
            arguments,
            returns,
            body,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }

    #[must_use]
    pub fn has_parameters(&self) -> bool {
        if let Some(arguments) = &self.arguments {
            return !arguments.is_empty();
        }
        false
    }

    #[must_use]
    pub fn is_void(&self) -> bool {
        self.returns
            .as_ref()
            .is_none_or(super::nodes::Type::is_unit_type)
    }

    #[must_use]
    pub fn is_non_det(&self) -> bool {
        self.body.is_non_det()
    }
}

impl ExternalFunctionDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        arguments: Option<Vec<ArgumentType>>,
        returns: Option<Type>,
        location: Location,
    ) -> Self {
        ExternalFunctionDefinition {
            id,
            location,
            visibility,
            name,
            arguments,
            returns,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl TypeDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        type_: Type,
        location: Location,
    ) -> Self {
        TypeDefinition {
            id,
            location,
            visibility,
            name,
            ty: type_,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl ModuleDefinition {
    #[must_use]
    pub fn new(
        id: u32,
        visibility: Visibility,
        name: Rc<Identifier>,
        body: Option<Vec<Definition>>,
        location: Location,
    ) -> Self {
        ModuleDefinition {
            id,
            location,
            visibility,
            name,
            body,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl Argument {
    #[must_use]
    pub fn new(id: u32, location: Location, name: Rc<Identifier>, is_mut: bool, ty: Type) -> Self {
        Argument {
            id,
            location,
            name,
            is_mut,
            ty,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl SelfReference {
    #[must_use]
    pub fn new(id: u32, location: Location, is_mut: bool) -> Self {
        SelfReference {
            id,
            location,
            is_mut,
        }
    }
}

impl IgnoreArgument {
    #[must_use]
    pub fn new(id: u32, location: Location, ty: Type) -> Self {
        IgnoreArgument { id, location, ty }
    }
}

impl Block {
    #[must_use]
    pub fn new(id: u32, location: Location, statements: Vec<Statement>) -> Self {
        Block {
            id,
            location,
            statements,
        }
    }
}

impl ExpressionStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ExpressionStatement {
            id,
            location,
            expression,
        }
    }
}

impl ReturnStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ReturnStatement {
            id,
            location,
            expression: RefCell::new(expression),
        }
    }
}

impl LoopStatement {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        condition: Option<Expression>,
        body: BlockType,
    ) -> Self {
        LoopStatement {
            id,
            location,
            condition: RefCell::new(condition),
            body,
        }
    }
}

impl BreakStatement {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        BreakStatement { id, location }
    }
}

impl IfStatement {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        condition: Expression,
        if_arm: BlockType,
        else_arm: Option<BlockType>,
    ) -> Self {
        IfStatement {
            id,
            location,
            condition: RefCell::new(condition),
            if_arm,
            else_arm,
        }
    }
}

impl VariableDefinitionStatement {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        name: Rc<Identifier>,
        type_: Type,
        value: Option<Expression>,
        is_uzumaki: bool,
    ) -> Self {
        VariableDefinitionStatement {
            id,
            location,
            name,
            ty: type_,
            value: value.map(RefCell::new),
            is_uzumaki,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl TypeDefinitionStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, name: Rc<Identifier>, type_: Type) -> Self {
        TypeDefinitionStatement {
            id,
            location,
            name,
            ty: type_,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name.clone()
    }
}

impl AssignStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, left: Expression, right: Expression) -> Self {
        AssignStatement {
            id,
            location,
            left: RefCell::new(left),
            right: RefCell::new(right),
        }
    }
}

impl ArrayIndexAccessExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, array: Expression, index: Expression) -> Self {
        ArrayIndexAccessExpression {
            id,
            location,
            array: RefCell::new(array),
            index: RefCell::new(index),
        }
    }
}

impl MemberAccessExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression, name: Rc<Identifier>) -> Self {
        MemberAccessExpression {
            id,
            location,
            expression: RefCell::new(expression),
            name,
        }
    }
}

impl TypeMemberAccessExpression {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        type_expression: Expression,
        name: Rc<Identifier>,
    ) -> Self {
        TypeMemberAccessExpression {
            id,
            location,
            expression: RefCell::new(type_expression),
            name,
        }
    }
}

impl FunctionCallExpression {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        function: Expression,
        type_parameters: Option<Vec<Rc<Identifier>>>,
        arguments: Option<Vec<(Option<Rc<Identifier>>, Expression)>>,
    ) -> Self {
        let arguments = arguments.map(|args| {
            args.into_iter()
                .map(|(name, expr)| (name, RefCell::new(expr)))
                .collect()
        });
        FunctionCallExpression {
            id,
            location,
            function,
            type_parameters,
            arguments,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        if let Expression::Identifier(identifier) = &self.function {
            identifier.name()
        } else if let Expression::MemberAccess(member_access) = &self.function {
            member_access.name.name()
        } else {
            String::new()
        }
    }
}

impl StructExpression {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        name: Rc<Identifier>,
        fields: Option<Vec<(Rc<Identifier>, Expression)>>,
    ) -> Self {
        let fields = fields.map(|vec| {
            vec.into_iter()
                .map(|(name, expr)| (name, RefCell::new(expr)))
                .collect()
        });
        StructExpression {
            id,
            location,
            name,
            fields,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }
}

impl PrefixUnaryExpression {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        expression: Expression,
        operator: UnaryOperatorKind,
    ) -> Self {
        PrefixUnaryExpression {
            id,
            location,
            expression: RefCell::new(expression),
            operator,
        }
    }
}

impl UzumakiExpression {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        UzumakiExpression { id, location }
    }
}

impl AssertStatement {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        AssertStatement {
            id,
            location,
            expression: RefCell::new(expression),
        }
    }
}

impl ParenthesizedExpression {
    #[must_use]
    pub fn new(id: u32, location: Location, expression: Expression) -> Self {
        ParenthesizedExpression {
            id,
            location,
            expression: RefCell::new(expression),
        }
    }
}

impl BinaryExpression {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        left: Expression,
        operator: OperatorKind,
        right: Expression,
    ) -> Self {
        BinaryExpression {
            id,
            location,
            left: RefCell::new(left),
            operator,
            right: RefCell::new(right),
        }
    }
}

impl BoolLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, value: bool) -> Self {
        BoolLiteral {
            id,
            location,
            value,
        }
    }
}

impl ArrayLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, elements: Option<Vec<Expression>>) -> Self {
        ArrayLiteral {
            id,
            location,
            elements: elements.map(|vec| vec.into_iter().map(RefCell::new).collect()),
        }
    }
}

impl StringLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, value: String) -> Self {
        StringLiteral {
            id,
            location,
            value,
        }
    }
}

impl NumberLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location, value: String) -> Self {
        NumberLiteral {
            id,
            location,
            value,
        }
    }
}

impl UnitLiteral {
    #[must_use]
    pub fn new(id: u32, location: Location) -> Self {
        UnitLiteral { id, location }
    }
}

impl GenericType {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        base: Rc<Identifier>,
        parameters: Vec<Rc<Identifier>>,
    ) -> Self {
        GenericType {
            id,
            location,
            base,
            parameters,
        }
    }
}

impl FunctionType {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        parameters: Option<Vec<Type>>,
        returns: Option<Type>,
    ) -> Self {
        FunctionType {
            id,
            location,
            parameters,
            returns,
        }
    }
}

impl QualifiedName {
    #[must_use]
    pub fn new(
        id: u32,
        location: Location,
        qualifier: Rc<Identifier>,
        name: Rc<Identifier>,
    ) -> Self {
        QualifiedName {
            id,
            location,
            qualifier,
            name,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }

    #[must_use]
    pub fn qualifier(&self) -> String {
        self.qualifier.name()
    }
}

impl TypeQualifiedName {
    #[must_use]
    pub fn new(id: u32, location: Location, alias: Rc<Identifier>, name: Rc<Identifier>) -> Self {
        TypeQualifiedName {
            id,
            location,
            alias,
            name,
        }
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.name.name()
    }

    #[must_use]
    pub fn alias(&self) -> String {
        self.alias.name()
    }
}

impl TypeArray {
    #[must_use]
    pub fn new(id: u32, location: Location, element_type: Type, size: Expression) -> Self {
        TypeArray {
            id,
            location,
            element_type,
            size,
        }
    }
}
