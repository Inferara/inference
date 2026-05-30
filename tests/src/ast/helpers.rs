//! Deep structural verification helpers for parsed ASTs.
//!
//! Each assertion function destructures the expected AST node variant, verifies
//! all scalar fields, and returns child IDs for chaining. Panics on mismatch
//! with context-rich messages identifying the failing node.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgData, ArgKind, BlockKind, Def, Expr, Field, Location, OperatorKind, SimpleTypeKind, Stmt,
    TypeNode, UnaryOperatorKind, Visibility,
};

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

/// Create a [`Location`] for assertions. Line and column values are 1-indexed
/// to match the parser's location convention. Byte offsets are set to zero — use
/// [`assert_location`] which ignores them by default.
#[must_use]
pub(crate) fn loc(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> Location {
    Location {
        offset_start: 0,
        offset_end: 0,
        start_line,
        start_column: start_col,
        end_line,
        end_column: end_col,
    }
}

/// Assert that `actual` matches `expected` on line/column fields.
/// Byte offsets are ignored unless `expected` has non-zero values.
pub(crate) fn assert_location(actual: &Location, expected: &Location, context: &str) {
    assert_eq!(
        actual.start_line, expected.start_line,
        "{context}: start_line"
    );
    assert_eq!(
        actual.start_column, expected.start_column,
        "{context}: start_column"
    );
    assert_eq!(actual.end_line, expected.end_line, "{context}: end_line");
    assert_eq!(
        actual.end_column, expected.end_column,
        "{context}: end_column"
    );
    if expected.offset_start != 0 || expected.offset_end != 0 {
        assert_eq!(
            actual.offset_start, expected.offset_start,
            "{context}: offset_start"
        );
        assert_eq!(
            actual.offset_end, expected.offset_end,
            "{context}: offset_end"
        );
    }
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

/// Assert that `def_id` points to a `Def::Function` with the given name,
/// visibility, parameter count, presence of a return type, and body statement
/// count. Returns `(args, returns, body)` for further drill-down.
#[must_use]
pub(crate) fn assert_function_def(
    arena: &AstArena,
    def_id: DefId,
    name: &str,
    vis: Visibility,
    param_count: usize,
    has_return: bool,
    body_stmt_count: usize,
) -> (Vec<ArgData>, Option<TypeId>, BlockId) {
    let def = &arena[def_id];
    let Def::Function {
        name: name_id,
        vis: actual_vis,
        args,
        returns,
        body,
        ..
    } = &def.kind
    else {
        panic!("expected Def::Function for '{name}', got {:?}", def.kind);
    };

    assert_eq!(arena[*name_id].name, name, "function name");
    assert_eq!(*actual_vis, vis, "function '{name}' visibility");
    assert_eq!(args.len(), param_count, "function '{name}' param count");
    assert_eq!(
        returns.is_some(),
        has_return,
        "function '{name}' has_return"
    );

    let block = &arena[*body];
    assert_eq!(
        block.stmts.len(),
        body_stmt_count,
        "function '{name}' body statement count"
    );

    (args.clone(), *returns, *body)
}

/// Assert that `def_id` points to a `Def::Struct` with the given name,
/// visibility, field count, and method count. Returns `(fields, methods)`.
#[must_use]
pub(crate) fn assert_struct_def(
    arena: &AstArena,
    def_id: DefId,
    name: &str,
    vis: Visibility,
    field_count: usize,
    method_count: usize,
) -> (Vec<Field>, Vec<DefId>) {
    let def = &arena[def_id];
    let Def::Struct {
        name: name_id,
        vis: actual_vis,
        fields,
        methods,
    } = &def.kind
    else {
        panic!("expected Def::Struct for '{name}', got {:?}", def.kind);
    };

    assert_eq!(arena[*name_id].name, name, "struct name");
    assert_eq!(*actual_vis, vis, "struct '{name}' visibility");
    assert_eq!(fields.len(), field_count, "struct '{name}' field count");
    assert_eq!(methods.len(), method_count, "struct '{name}' method count");

    (fields.clone(), methods.clone())
}

/// Assert that `def_id` points to a `Def::Enum` with the given name,
/// visibility, and variant names (in order).
pub(crate) fn assert_enum_def(
    arena: &AstArena,
    def_id: DefId,
    name: &str,
    vis: Visibility,
    variant_names: &[&str],
) {
    let def = &arena[def_id];
    let Def::Enum {
        name: name_id,
        vis: actual_vis,
        variants,
    } = &def.kind
    else {
        panic!("expected Def::Enum for '{name}', got {:?}", def.kind);
    };

    assert_eq!(arena[*name_id].name, name, "enum name");
    assert_eq!(*actual_vis, vis, "enum '{name}' visibility");
    assert_eq!(
        variants.len(),
        variant_names.len(),
        "enum '{name}' variant count"
    );

    for (i, (&variant_id, &expected_name)) in variants.iter().zip(variant_names).enumerate() {
        assert_eq!(
            arena[variant_id].name, expected_name,
            "enum '{name}' variant {i}"
        );
    }
}

/// Assert that `def_id` points to a `Def::Constant` with the given name and
/// visibility. Returns `(ty, value)`.
#[must_use]
pub(crate) fn assert_const_def(
    arena: &AstArena,
    def_id: DefId,
    name: &str,
    vis: Visibility,
) -> (TypeId, ExprId) {
    let def = &arena[def_id];
    let Def::Constant {
        name: name_id,
        vis: actual_vis,
        ty,
        value,
    } = &def.kind
    else {
        panic!("expected Def::Constant for '{name}', got {:?}", def.kind);
    };

    assert_eq!(arena[*name_id].name, name, "constant name");
    assert_eq!(*actual_vis, vis, "constant '{name}' visibility");

    (*ty, *value)
}

/// Assert that `def_id` points to a `Def::TypeAlias` with the given name and
/// visibility. Returns the aliased type ID.
#[must_use]
pub(crate) fn assert_type_alias_def(
    arena: &AstArena,
    def_id: DefId,
    name: &str,
    vis: Visibility,
) -> TypeId {
    let def = &arena[def_id];
    let Def::TypeAlias {
        name: name_id,
        vis: actual_vis,
        ty,
    } = &def.kind
    else {
        panic!("expected Def::TypeAlias for '{name}', got {:?}", def.kind);
    };

    assert_eq!(arena[*name_id].name, name, "type alias name");
    assert_eq!(*actual_vis, vis, "type alias '{name}' visibility");

    *ty
}

/// Assert that `def_id` points to a `Def::ExternFunction` with the given name,
/// visibility, parameter count, and return type presence. Returns `(args, returns)`.
#[must_use]
pub(crate) fn assert_extern_function_def(
    arena: &AstArena,
    def_id: DefId,
    name: &str,
    vis: Visibility,
    param_count: usize,
    has_return: bool,
) -> (Vec<ArgData>, Option<TypeId>) {
    let def = &arena[def_id];
    let Def::ExternFunction {
        name: name_id,
        vis: actual_vis,
        args,
        returns,
    } = &def.kind
    else {
        panic!(
            "expected Def::ExternFunction for '{name}', got {:?}",
            def.kind
        );
    };

    assert_eq!(arena[*name_id].name, name, "extern function name");
    assert_eq!(*actual_vis, vis, "extern function '{name}' visibility");
    assert_eq!(
        args.len(),
        param_count,
        "extern function '{name}' param count"
    );
    assert_eq!(
        returns.is_some(),
        has_return,
        "extern function '{name}' has_return"
    );

    (args.clone(), *returns)
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

/// Assert that `stmt_id` is a `Stmt::VarDef` with the given name, mutability,
/// and initializer presence. Returns `(ty, value)`.
#[must_use]
pub(crate) fn assert_var_def(
    arena: &AstArena,
    stmt_id: StmtId,
    name: &str,
    is_mut: bool,
    has_init: bool,
) -> (TypeId, Option<ExprId>) {
    let stmt = &arena[stmt_id];
    let Stmt::VarDef {
        name: name_id,
        ty,
        value,
        is_mut: actual_mut,
    } = &stmt.kind
    else {
        panic!("expected Stmt::VarDef for '{name}', got {:?}", stmt.kind);
    };

    assert_eq!(arena[*name_id].name, name, "var def name");
    assert_eq!(*actual_mut, is_mut, "var '{name}' is_mut");
    assert_eq!(value.is_some(), has_init, "var '{name}' has initializer");

    (*ty, *value)
}

/// Assert that `stmt_id` is a `Stmt::Return`. Returns the returned expression.
#[must_use]
pub(crate) fn assert_return(arena: &AstArena, stmt_id: StmtId) -> ExprId {
    let stmt = &arena[stmt_id];
    let Stmt::Return { expr } = &stmt.kind else {
        panic!("expected Stmt::Return, got {:?}", stmt.kind);
    };
    *expr
}

/// Assert that `stmt_id` is a `Stmt::Assign`. Returns `(left, right)`.
#[must_use]
pub(crate) fn assert_assign(arena: &AstArena, stmt_id: StmtId) -> (ExprId, ExprId) {
    let stmt = &arena[stmt_id];
    let Stmt::Assign { left, right } = &stmt.kind else {
        panic!("expected Stmt::Assign, got {:?}", stmt.kind);
    };
    (*left, *right)
}

/// Assert that `stmt_id` is a `Stmt::If` with optional else. Returns
/// `(condition, then_block, else_block)`.
#[must_use]
pub(crate) fn assert_if(
    arena: &AstArena,
    stmt_id: StmtId,
    has_else: bool,
) -> (ExprId, BlockId, Option<BlockId>) {
    let stmt = &arena[stmt_id];
    let Stmt::If {
        condition,
        then_block,
        else_block,
    } = &stmt.kind
    else {
        panic!("expected Stmt::If, got {:?}", stmt.kind);
    };

    assert_eq!(else_block.is_some(), has_else, "if statement has_else");

    (*condition, *then_block, *else_block)
}

/// Assert that `stmt_id` is a `Stmt::Loop` with optional condition. Returns
/// `(condition, body)`.
#[must_use]
pub(crate) fn assert_loop(
    arena: &AstArena,
    stmt_id: StmtId,
    has_condition: bool,
) -> (Option<ExprId>, BlockId) {
    let stmt = &arena[stmt_id];
    let Stmt::Loop { condition, body } = &stmt.kind else {
        panic!("expected Stmt::Loop, got {:?}", stmt.kind);
    };

    assert_eq!(condition.is_some(), has_condition, "loop has_condition");

    (*condition, *body)
}

/// Assert that `stmt_id` is a `Stmt::Break`.
pub(crate) fn assert_break(arena: &AstArena, stmt_id: StmtId) {
    let stmt = &arena[stmt_id];
    assert!(
        matches!(stmt.kind, Stmt::Break),
        "expected Stmt::Break, got {:?}",
        stmt.kind
    );
}

/// Assert that `stmt_id` is a `Stmt::Assert`. Returns the asserted expression.
#[must_use]
pub(crate) fn assert_assert_stmt(arena: &AstArena, stmt_id: StmtId) -> ExprId {
    let stmt = &arena[stmt_id];
    let Stmt::Assert { expr } = &stmt.kind else {
        panic!("expected Stmt::Assert, got {:?}", stmt.kind);
    };
    *expr
}

/// Assert that `stmt_id` is a `Stmt::Block` with the given block kind and
/// statement count. Returns the inner [`BlockId`].
#[must_use]
pub(crate) fn assert_block_stmt(
    arena: &AstArena,
    stmt_id: StmtId,
    kind: BlockKind,
    stmt_count: usize,
) -> BlockId {
    let stmt = &arena[stmt_id];
    let Stmt::Block(block_id) = &stmt.kind else {
        panic!("expected Stmt::Block, got {:?}", stmt.kind);
    };

    let block = &arena[*block_id];
    assert_eq!(block.block_kind, kind, "block kind");
    assert_eq!(block.stmts.len(), stmt_count, "block statement count");

    *block_id
}

/// Assert that `stmt_id` is a `Stmt::Expr`. Returns the expression.
#[must_use]
pub(crate) fn assert_expr_stmt(arena: &AstArena, stmt_id: StmtId) -> ExprId {
    let stmt = &arena[stmt_id];
    let Stmt::Expr(expr_id) = &stmt.kind else {
        panic!("expected Stmt::Expr, got {:?}", stmt.kind);
    };
    *expr_id
}

/// Assert that `stmt_id` is a `Stmt::TypeDef`. Returns `(name_ident, type_id)`.
#[must_use]
pub(crate) fn assert_type_def_stmt(arena: &AstArena, stmt_id: StmtId) -> (IdentId, TypeId) {
    let stmt = &arena[stmt_id];
    let Stmt::TypeDef { name, ty } = &stmt.kind else {
        panic!("expected Stmt::TypeDef, got {:?}", stmt.kind);
    };
    (*name, *ty)
}

/// Assert that `stmt_id` is a `Stmt::ConstDef`. Returns the inner [`DefId`].
#[must_use]
pub(crate) fn assert_const_def_stmt(arena: &AstArena, stmt_id: StmtId) -> DefId {
    let stmt = &arena[stmt_id];
    let Stmt::ConstDef(def_id) = &stmt.kind else {
        panic!("expected Stmt::ConstDef, got {:?}", stmt.kind);
    };
    *def_id
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

/// Assert that `expr_id` is an `Expr::Identifier` with the given name.
pub(crate) fn assert_ident_expr(arena: &AstArena, expr_id: ExprId, name: &str) {
    let expr = &arena[expr_id];
    let Expr::Identifier(ident_id) = &expr.kind else {
        panic!("expected Expr::Identifier('{name}'), got {:?}", expr.kind);
    };
    assert_eq!(arena[*ident_id].name, name, "identifier name");
}

/// Assert that `expr_id` is an `Expr::NumberLiteral` with the given value
/// string.
pub(crate) fn assert_number(arena: &AstArena, expr_id: ExprId, value: &str) {
    let expr = &arena[expr_id];
    let Expr::NumberLiteral { value: actual } = &expr.kind else {
        panic!(
            "expected Expr::NumberLiteral('{value}'), got {:?}",
            expr.kind
        );
    };
    assert_eq!(actual, value, "number literal value");
}

/// Assert that `expr_id` is an `Expr::BoolLiteral` with the given value.
pub(crate) fn assert_bool(arena: &AstArena, expr_id: ExprId, value: bool) {
    let expr = &arena[expr_id];
    let Expr::BoolLiteral { value: actual } = &expr.kind else {
        panic!("expected Expr::BoolLiteral({value}), got {:?}", expr.kind);
    };
    assert_eq!(*actual, value, "bool literal value");
}

/// Assert that `expr_id` is an `Expr::StringLiteral` with the given value.
pub(crate) fn assert_string_literal(arena: &AstArena, expr_id: ExprId, value: &str) {
    let expr = &arena[expr_id];
    let Expr::StringLiteral { value: actual } = &expr.kind else {
        panic!(
            "expected Expr::StringLiteral(\"{value}\"), got {:?}",
            expr.kind
        );
    };
    assert_eq!(actual, value, "string literal value");
}

/// Assert that `expr_id` is an `Expr::UnitLiteral`.
pub(crate) fn assert_unit_literal(arena: &AstArena, expr_id: ExprId) {
    let expr = &arena[expr_id];
    assert!(
        matches!(expr.kind, Expr::UnitLiteral),
        "expected Expr::UnitLiteral, got {:?}",
        expr.kind
    );
}

/// Assert that `expr_id` is an `Expr::Binary` with the given operator.
/// Returns `(left, right)`.
#[must_use]
pub(crate) fn assert_binary(
    arena: &AstArena,
    expr_id: ExprId,
    op: OperatorKind,
) -> (ExprId, ExprId) {
    let expr = &arena[expr_id];
    let Expr::Binary {
        left,
        right,
        op: actual_op,
    } = &expr.kind
    else {
        panic!("expected Expr::Binary({op:?}), got {:?}", expr.kind);
    };
    assert_eq!(*actual_op, op, "binary operator");
    (*left, *right)
}

/// Assert that `expr_id` is an `Expr::PrefixUnary` with the given operator.
/// Returns the inner expression.
#[must_use]
pub(crate) fn assert_prefix_unary(
    arena: &AstArena,
    expr_id: ExprId,
    op: UnaryOperatorKind,
) -> ExprId {
    let expr = &arena[expr_id];
    let Expr::PrefixUnary {
        expr: inner,
        op: actual_op,
    } = &expr.kind
    else {
        panic!("expected Expr::PrefixUnary({op:?}), got {:?}", expr.kind);
    };
    assert_eq!(*actual_op, op, "prefix unary operator");
    *inner
}

/// Assert that `expr_id` is an `Expr::FunctionCall` where the function
/// expression is an `Identifier` with the given name and the given argument
/// count. Returns the argument expression IDs.
#[must_use]
pub(crate) fn assert_fn_call(
    arena: &AstArena,
    expr_id: ExprId,
    name: &str,
    arg_count: usize,
) -> Vec<ExprId> {
    let expr = &arena[expr_id];
    let Expr::FunctionCall { function, args, .. } = &expr.kind else {
        panic!("expected Expr::FunctionCall('{name}'), got {:?}", expr.kind);
    };

    let Expr::Identifier(fn_ident) = &arena[*function].kind else {
        panic!(
            "expected function callee to be Identifier('{name}'), got {:?}",
            arena[*function].kind
        );
    };
    assert_eq!(arena[*fn_ident].name, name, "function call name");
    assert_eq!(args.len(), arg_count, "function call '{name}' arg count");

    args.iter().map(|(_, expr_id)| *expr_id).collect()
}

/// Assert that `expr_id` is an `Expr::FunctionCall`. Returns
/// `(function_expr, args)` without constraining the callee shape, so the
/// caller can inspect method-call chains or complex callees.
#[must_use]
pub(crate) fn assert_fn_call_raw(
    arena: &AstArena,
    expr_id: ExprId,
    arg_count: usize,
) -> (ExprId, Vec<(Option<IdentId>, ExprId)>) {
    let expr = &arena[expr_id];
    let Expr::FunctionCall { function, args, .. } = &expr.kind else {
        panic!("expected Expr::FunctionCall, got {:?}", expr.kind);
    };
    assert_eq!(args.len(), arg_count, "function call arg count");
    (*function, args.clone())
}

/// Assert that `expr_id` is an `Expr::ArrayIndexAccess`. Returns
/// `(array, index)`.
#[must_use]
pub(crate) fn assert_array_index(arena: &AstArena, expr_id: ExprId) -> (ExprId, ExprId) {
    let expr = &arena[expr_id];
    let Expr::ArrayIndexAccess { array, index } = &expr.kind else {
        panic!("expected Expr::ArrayIndexAccess, got {:?}", expr.kind);
    };
    (*array, *index)
}

/// Assert that `expr_id` is an `Expr::MemberAccess` with the given field
/// name. Returns the base expression.
#[must_use]
pub(crate) fn assert_member_access(arena: &AstArena, expr_id: ExprId, field: &str) -> ExprId {
    let expr = &arena[expr_id];
    let Expr::MemberAccess {
        expr: base,
        name: name_id,
    } = &expr.kind
    else {
        panic!(
            "expected Expr::MemberAccess('.{field}'), got {:?}",
            expr.kind
        );
    };
    assert_eq!(arena[*name_id].name, field, "member access field name");
    *base
}

/// Assert that `expr_id` is an `Expr::TypeMemberAccess` with the given member
/// name. Returns the type expression (left-hand side of `::`).
#[must_use]
pub(crate) fn assert_type_member_access(arena: &AstArena, expr_id: ExprId, member: &str) -> ExprId {
    let expr = &arena[expr_id];
    let Expr::TypeMemberAccess {
        expr: base,
        name: name_id,
    } = &expr.kind
    else {
        panic!(
            "expected Expr::TypeMemberAccess('::{member}'), got {:?}",
            expr.kind
        );
    };
    assert_eq!(arena[*name_id].name, member, "type member access name");
    *base
}

/// Assert that `expr_id` is an `Expr::StructLiteral` with the given struct
/// name and field count. Returns `Vec<(field_name, field_value_expr)>`.
#[must_use]
pub(crate) fn assert_struct_literal(
    arena: &AstArena,
    expr_id: ExprId,
    name: &str,
    field_count: usize,
) -> Vec<(String, ExprId)> {
    let expr = &arena[expr_id];
    let Expr::StructLiteral {
        name: name_id,
        fields,
    } = &expr.kind
    else {
        panic!(
            "expected Expr::StructLiteral('{name}'), got {:?}",
            expr.kind
        );
    };
    assert_eq!(arena[*name_id].name, name, "struct literal name");
    assert_eq!(
        fields.len(),
        field_count,
        "struct literal '{name}' field count"
    );

    fields
        .iter()
        .map(|(ident_id, value_id)| (arena[*ident_id].name.clone(), *value_id))
        .collect()
}

/// Assert that `expr_id` is an `Expr::ArrayLiteral` with the given element
/// count. Returns the element expressions.
#[must_use]
pub(crate) fn assert_array_literal(
    arena: &AstArena,
    expr_id: ExprId,
    element_count: usize,
) -> Vec<ExprId> {
    let expr = &arena[expr_id];
    let Expr::ArrayLiteral { elements } = &expr.kind else {
        panic!("expected Expr::ArrayLiteral, got {:?}", expr.kind);
    };
    assert_eq!(elements.len(), element_count, "array literal element count");
    elements.clone()
}

/// Assert that `expr_id` is an `Expr::Uzumaki` (@).
pub(crate) fn assert_uzumaki(arena: &AstArena, expr_id: ExprId) {
    let expr = &arena[expr_id];
    assert!(
        matches!(expr.kind, Expr::Uzumaki),
        "expected Expr::Uzumaki, got {:?}",
        expr.kind
    );
}

/// Assert that `expr_id` is an `Expr::Parenthesized`. Returns the inner
/// expression.
#[must_use]
pub(crate) fn assert_parens(arena: &AstArena, expr_id: ExprId) -> ExprId {
    let expr = &arena[expr_id];
    let Expr::Parenthesized { expr: inner } = &expr.kind else {
        panic!("expected Expr::Parenthesized, got {:?}", expr.kind);
    };
    *inner
}

/// Assert that `expr_id` is an `Expr::Type`. Returns the inner type ID.
#[must_use]
pub(crate) fn assert_type_expr(arena: &AstArena, expr_id: ExprId) -> TypeId {
    let expr = &arena[expr_id];
    let Expr::Type(type_id) = &expr.kind else {
        panic!("expected Expr::Type, got {:?}", expr.kind);
    };
    *type_id
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Assert that `type_id` points to a `TypeNode::Simple` of the expected kind.
pub(crate) fn assert_simple_type(arena: &AstArena, type_id: TypeId, expected: SimpleTypeKind) {
    let ty = &arena[type_id];
    let TypeNode::Simple(actual) = &ty.kind else {
        panic!("expected TypeNode::Simple({expected:?}), got {:?}", ty.kind);
    };
    assert_eq!(*actual, expected, "simple type kind");
}

/// Assert that `type_id` points to a `TypeNode::Array`. Returns
/// `(element_type, size_expr)`.
#[must_use]
pub(crate) fn assert_array_type(arena: &AstArena, type_id: TypeId) -> (TypeId, ExprId) {
    let ty = &arena[type_id];
    let TypeNode::Array { element, size } = &ty.kind else {
        panic!("expected TypeNode::Array, got {:?}", ty.kind);
    };
    (*element, *size)
}

/// Assert that `type_id` points to a `TypeNode::Custom` with the given name.
pub(crate) fn assert_custom_type(arena: &AstArena, type_id: TypeId, name: &str) {
    let ty = &arena[type_id];
    let TypeNode::Custom(ident_id) = &ty.kind else {
        panic!("expected TypeNode::Custom('{name}'), got {:?}", ty.kind);
    };
    assert_eq!(arena[*ident_id].name, name, "custom type name");
}

/// Assert that `type_id` points to a `TypeNode::Generic` with the given base
/// name and parameter count. Returns the type parameter ident IDs.
#[must_use]
pub(crate) fn assert_generic_type(
    arena: &AstArena,
    type_id: TypeId,
    base: &str,
    param_count: usize,
) -> Vec<IdentId> {
    let ty = &arena[type_id];
    let TypeNode::Generic {
        base: base_id,
        params,
    } = &ty.kind
    else {
        panic!("expected TypeNode::Generic('{base}'), got {:?}", ty.kind);
    };
    assert_eq!(arena[*base_id].name, base, "generic type base name");
    assert_eq!(
        params.len(),
        param_count,
        "generic type '{base}' param count"
    );
    params.clone()
}

/// Assert that `type_id` points to a `TypeNode::Function` with the given
/// parameter count and return type presence. Returns `(param_types, ret_type)`.
#[must_use]
pub(crate) fn assert_function_type(
    arena: &AstArena,
    type_id: TypeId,
    param_count: usize,
    has_return: bool,
) -> (Vec<TypeId>, Option<TypeId>) {
    let ty = &arena[type_id];
    let TypeNode::Function { params, ret } = &ty.kind else {
        panic!("expected TypeNode::Function, got {:?}", ty.kind);
    };
    assert_eq!(params.len(), param_count, "function type param count");
    assert_eq!(ret.is_some(), has_return, "function type has_return");
    (params.clone(), *ret)
}

// ---------------------------------------------------------------------------
// Block / Arg helpers
// ---------------------------------------------------------------------------

/// Assert block properties. Returns the statement IDs for further assertions.
#[must_use]
pub(crate) fn assert_block(
    arena: &AstArena,
    block_id: BlockId,
    kind: BlockKind,
    stmt_count: usize,
) -> Vec<StmtId> {
    let block = &arena[block_id];
    assert_eq!(block.block_kind, kind, "block kind");
    assert_eq!(block.stmts.len(), stmt_count, "block statement count");
    block.stmts.clone()
}

/// Assert that `arg` is an `ArgKind::Named` with the given name and
/// mutability. Returns the type ID.
#[must_use]
pub(crate) fn assert_named_arg(
    arena: &AstArena,
    arg: &ArgData,
    name: &str,
    is_mut: bool,
) -> TypeId {
    let ArgKind::Named {
        name: name_id,
        ty,
        is_mut: actual_mut,
    } = &arg.kind
    else {
        panic!("expected ArgKind::Named('{name}'), got {:?}", arg.kind);
    };
    assert_eq!(arena[*name_id].name, name, "arg name");
    assert_eq!(*actual_mut, is_mut, "arg '{name}' is_mut");
    *ty
}

/// Assert that `arg` is an `ArgKind::SelfRef` with the given mutability.
pub(crate) fn assert_self_arg(arg: &ArgData, is_mut: bool) {
    let ArgKind::SelfRef { is_mut: actual_mut } = &arg.kind else {
        panic!("expected ArgKind::SelfRef, got {:?}", arg.kind);
    };
    assert_eq!(*actual_mut, is_mut, "self arg is_mut");
}

/// Assert that `arg` is an `ArgKind::Ignored`. Returns the type ID.
#[must_use]
pub(crate) fn assert_ignored_arg(arena: &AstArena, arg: &ArgData) -> TypeId {
    let ArgKind::Ignored { ty } = &arg.kind else {
        panic!("expected ArgKind::Ignored, got {:?}", arg.kind);
    };
    let _ = arena; // future-proof: may need arena for type resolution
    *ty
}

/// Assert that `arg` is an `ArgKind::TypeOnly`. Returns the type ID.
#[must_use]
pub(crate) fn assert_type_only_arg(arena: &AstArena, arg: &ArgData) -> TypeId {
    let ArgKind::TypeOnly(ty) = &arg.kind else {
        panic!("expected ArgKind::TypeOnly, got {:?}", arg.kind);
    };
    let _ = arena;
    *ty
}

// ---------------------------------------------------------------------------
// Compound convenience
// ---------------------------------------------------------------------------

/// Parse source, assert exactly one source file, and return the arena.
#[must_use]
pub(crate) fn parse_one(source: &str) -> AstArena {
    let arena = crate::utils::build_ast(source.to_string());
    assert_eq!(
        arena.source_files().len(),
        1,
        "expected exactly 1 source file"
    );
    arena
}

/// Parse source and return the `DefId`s from the single source file.
#[must_use]
pub(crate) fn parse_defs(source: &str) -> (AstArena, Vec<DefId>) {
    let arena = parse_one(source);
    let defs: Vec<DefId> = arena
        .source_files()
        .next()
        .expect("must have source file")
        .defs
        .clone();
    (arena, defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loc_creates_location_with_zero_offsets() {
        let l = loc(1, 0, 1, 5);
        assert_eq!(l.start_line, 1);
        assert_eq!(l.start_column, 0);
        assert_eq!(l.end_line, 1);
        assert_eq!(l.end_column, 5);
        assert_eq!(l.offset_start, 0);
        assert_eq!(l.offset_end, 0);
    }

    #[test]
    fn test_assert_location_ignores_offsets_when_expected_is_zero() {
        let actual = Location::new(10, 20, 1, 0, 1, 5);
        let expected = loc(1, 0, 1, 5);
        assert_location(&actual, &expected, "test");
    }

    #[test]
    #[should_panic(expected = "start_line")]
    fn test_assert_location_panics_on_line_mismatch() {
        let actual = Location::new(0, 5, 2, 0, 2, 5);
        let expected = loc(1, 0, 1, 5);
        assert_location(&actual, &expected, "test");
    }

    #[test]
    fn test_parse_one_returns_arena() {
        let arena = parse_one("fn f() {}");
        assert_eq!(arena.function_def_ids().len(), 1);
    }

    #[test]
    fn test_parse_defs_returns_defs() {
        let (arena, defs) = parse_defs("fn a() {} fn b() {}");
        assert_eq!(defs.len(), 2);
        let _ = assert_function_def(&arena, defs[0], "a", Visibility::Private, 0, false, 0);
        let _ = assert_function_def(&arena, defs[1], "b", Visibility::Private, 0, false, 0);
    }

    #[test]
    fn test_assert_function_def_with_params_and_return() {
        let (arena, defs) = parse_defs("fn add(a: i32, b: i32) -> i32 { return a + b; }");
        let (args, returns, body) =
            assert_function_def(&arena, defs[0], "add", Visibility::Private, 2, true, 1);

        let ty_a = assert_named_arg(&arena, &args[0], "a", false);
        assert_simple_type(&arena, ty_a, SimpleTypeKind::I32);

        let ty_b = assert_named_arg(&arena, &args[1], "b", false);
        assert_simple_type(&arena, ty_b, SimpleTypeKind::I32);

        assert_simple_type(&arena, returns.unwrap(), SimpleTypeKind::I32);

        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Add);
        assert_ident_expr(&arena, left, "a");
        assert_ident_expr(&arena, right, "b");
    }

    #[test]
    fn test_assert_struct_def_with_fields() {
        let (arena, defs) = parse_defs("struct Point { x: i32; y: i32; }");
        let (fields, methods) =
            assert_struct_def(&arena, defs[0], "Point", Visibility::Private, 2, 0);
        assert!(methods.is_empty());

        assert_eq!(arena[fields[0].name].name, "x");
        assert_simple_type(&arena, fields[0].ty, SimpleTypeKind::I32);
        assert_eq!(arena[fields[1].name].name, "y");
        assert_simple_type(&arena, fields[1].ty, SimpleTypeKind::I32);
    }

    #[test]
    fn test_assert_enum_def_with_variants() {
        let (arena, defs) = parse_defs("enum Color { Red, Green, Blue }");
        assert_enum_def(
            &arena,
            defs[0],
            "Color",
            Visibility::Private,
            &["Red", "Green", "Blue"],
        );
    }

    #[test]
    fn test_assert_const_def_number() {
        let (arena, defs) = parse_defs("const X: i32 = 42;");
        let (ty, value) = assert_const_def(&arena, defs[0], "X", Visibility::Private);
        assert_simple_type(&arena, ty, SimpleTypeKind::I32);
        assert_number(&arena, value, "42");
    }

    #[test]
    fn test_assert_type_alias() {
        let (arena, defs) = parse_defs("type MyInt = i32;");
        let ty = assert_type_alias_def(&arena, defs[0], "MyInt", Visibility::Private);
        assert_simple_type(&arena, ty, SimpleTypeKind::I32);
    }

    #[test]
    fn test_assert_var_def_mutable_with_init() {
        let (arena, defs) = parse_defs("fn f() { let mut x: i32 = 5; }");
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let (ty, value) = assert_var_def(&arena, stmts[0], "x", true, true);
        assert_simple_type(&arena, ty, SimpleTypeKind::I32);
        assert_number(&arena, value.unwrap(), "5");
    }

    #[test]
    fn test_assert_if_with_else() {
        let source = "fn f(x: i32) -> i32 { if x > 0 { return 1; } else { return 0; } }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 1, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let (cond, then_block, else_block) = assert_if(&arena, stmts[0], true);

        let (left, right) = assert_binary(&arena, cond, OperatorKind::Gt);
        assert_ident_expr(&arena, left, "x");
        assert_number(&arena, right, "0");

        let then_stmts = assert_block(&arena, then_block, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, then_stmts[0]);
        assert_number(&arena, ret_expr, "1");

        let else_stmts = assert_block(&arena, else_block.unwrap(), BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, else_stmts[0]);
        assert_number(&arena, ret_expr, "0");
    }

    #[test]
    fn test_assert_loop_with_condition() {
        let source = "fn f() { let mut i: i32 = 0; loop i < 10 { i = i + 1; } }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, false, 2);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 2);
        let (cond, loop_body) = assert_loop(&arena, stmts[1], true);

        let cond = cond.unwrap();
        let (left, right) = assert_binary(&arena, cond, OperatorKind::Lt);
        assert_ident_expr(&arena, left, "i");
        assert_number(&arena, right, "10");

        let body_stmts = assert_block(&arena, loop_body, BlockKind::Regular, 1);
        let (left, right) = assert_assign(&arena, body_stmts[0]);
        assert_ident_expr(&arena, left, "i");
        let (add_left, add_right) = assert_binary(&arena, right, OperatorKind::Add);
        assert_ident_expr(&arena, add_left, "i");
        assert_number(&arena, add_right, "1");
    }

    #[test]
    fn test_assert_break_in_loop() {
        let source = "fn f() { loop { break; } }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let (_, loop_body) = assert_loop(&arena, stmts[0], false);
        let body_stmts = assert_block(&arena, loop_body, BlockKind::Regular, 1);
        assert_break(&arena, body_stmts[0]);
    }

    #[test]
    fn test_assert_fn_call_expr() {
        let source = "fn f() -> i32 { return add(1, 2); }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        let args = assert_fn_call(&arena, ret_expr, "add", 2);
        assert_number(&arena, args[0], "1");
        assert_number(&arena, args[1], "2");
    }

    #[test]
    fn test_assert_member_access_expr() {
        let source = "fn f() -> i32 { return p.x; }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        let base = assert_member_access(&arena, ret_expr, "x");
        assert_ident_expr(&arena, base, "p");
    }

    #[test]
    fn test_assert_array_literal_and_index() {
        let source = "fn f() { let a: [i32; 3] = [1, 2, 3]; }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let (ty, value) = assert_var_def(&arena, stmts[0], "a", false, true);

        let (elem_ty, size_expr) = assert_array_type(&arena, ty);
        assert_simple_type(&arena, elem_ty, SimpleTypeKind::I32);
        assert_number(&arena, size_expr, "3");

        let elems = assert_array_literal(&arena, value.unwrap(), 3);
        assert_number(&arena, elems[0], "1");
        assert_number(&arena, elems[1], "2");
        assert_number(&arena, elems[2], "3");
    }

    #[test]
    fn test_assert_prefix_unary_negation() {
        let source = "fn f() -> i32 { return -x; }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        let inner = assert_prefix_unary(&arena, ret_expr, UnaryOperatorKind::Neg);
        assert_ident_expr(&arena, inner, "x");
    }

    #[test]
    fn test_assert_bool_literal() {
        let source = "fn f() -> bool { return true; }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        assert_bool(&arena, ret_expr, true);
    }

    #[test]
    fn test_assert_struct_literal_expr() {
        let source = r#"
            struct Point { x: i32; y: i32; }
            fn f() { let p: Point = Point { x: 1, y: 2 }; }
        "#;
        let (arena, defs) = parse_defs(source);
        let _ = assert_struct_def(&arena, defs[0], "Point", Visibility::Private, 2, 0);
        let (_, _, body) =
            assert_function_def(&arena, defs[1], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let (ty, value) = assert_var_def(&arena, stmts[0], "p", false, true);
        assert_custom_type(&arena, ty, "Point");
        let fields = assert_struct_literal(&arena, value.unwrap(), "Point", 2);
        assert_eq!(fields[0].0, "x");
        assert_number(&arena, fields[0].1, "1");
        assert_eq!(fields[1].0, "y");
        assert_number(&arena, fields[1].1, "2");
    }

    #[test]
    fn test_assert_self_arg_in_method() {
        let source = r#"
            struct Counter {
                value: i32;
                fn inc(mut self) { self.value = self.value + 1; }
            }
        "#;
        let (arena, defs) = parse_defs(source);
        let (_, methods) = assert_struct_def(&arena, defs[0], "Counter", Visibility::Private, 1, 1);
        let (args, _, _) =
            assert_function_def(&arena, methods[0], "inc", Visibility::Private, 1, false, 1);
        assert_self_arg(&args[0], true);
    }

    #[test]
    fn test_assert_public_function() {
        let (arena, defs) = parse_defs("pub fn main() -> i32 { return 0; }");
        let _ = assert_function_def(&arena, defs[0], "main", Visibility::Public, 0, true, 1);
    }

    #[test]
    fn test_assert_parens_expr() {
        let source = "fn f() -> i32 { return (42); }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        let inner = assert_parens(&arena, ret_expr);
        assert_number(&arena, inner, "42");
    }

    #[test]
    fn test_assert_custom_type_on_variable() {
        let source = r#"
            struct Foo { x: i32; }
            fn f() { let v: Foo = Foo { x: 0 }; }
        "#;
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[1], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let (ty, _) = assert_var_def(&arena, stmts[0], "v", false, true);
        assert_custom_type(&arena, ty, "Foo");
    }

    #[test]
    fn test_assert_array_index_access() {
        let source = "fn f() -> i32 { return a[0]; }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);
        let (arr, idx) = assert_array_index(&arena, ret_expr);
        assert_ident_expr(&arena, arr, "a");
        assert_number(&arena, idx, "0");
    }

    #[test]
    fn test_assert_forall_block() {
        let source = "fn f() { forall { let x: i32 = @; } }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let forall_id = assert_block_stmt(&arena, stmts[0], BlockKind::Forall, 1);
        let inner_stmts = assert_block(&arena, forall_id, BlockKind::Forall, 1);
        let (_, value) = assert_var_def(&arena, inner_stmts[0], "x", false, true);
        assert_uzumaki(&arena, value.unwrap());
    }

    #[test]
    fn test_assert_expr_stmt() {
        let source = "fn f() { foo(); }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, false, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let call_expr = assert_expr_stmt(&arena, stmts[0]);
        let _ = assert_fn_call(&arena, call_expr, "foo", 0);
    }

    #[test]
    fn test_deep_chain_binary_arithmetic() {
        let source = "fn f() -> i32 { return 1 + 2 * 3; }";
        let (arena, defs) = parse_defs(source);
        let (_, _, body) =
            assert_function_def(&arena, defs[0], "f", Visibility::Private, 0, true, 1);
        let stmts = assert_block(&arena, body, BlockKind::Regular, 1);
        let ret_expr = assert_return(&arena, stmts[0]);

        let (left, right) = assert_binary(&arena, ret_expr, OperatorKind::Add);
        assert_number(&arena, left, "1");
        let (mul_left, mul_right) = assert_binary(&arena, right, OperatorKind::Mul);
        assert_number(&arena, mul_left, "2");
        assert_number(&arena, mul_right, "3");
    }
}
