//! Shared AST walker with depth tracking for analysis rules.
//!
//! Extracts the traversal logic into a reusable function that any rule can
//! call with its own visitor closure. The walker resolves arena-indexed IDs
//! to access node data.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId, StmtId};
use inference_ast::nodes::{BlockKind, Def, Expr, Stmt};
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

/// Context passed to visitor callbacks during AST walking.
pub(crate) struct WalkContext {
    pub loop_depth: u32,
    pub nondet_depth: u32,
    pub nondet_block_kind: Option<&'static str>,
}

fn block_kind_label(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Forall => "forall",
        BlockKind::Exists => "exists",
        BlockKind::Assume => "assume",
        BlockKind::Unique => "unique",
        BlockKind::Regular => unreachable!("called only for non-det blocks"),
    }
}

/// Walks all function bodies and calls `visitor` for every statement.
///
/// Uses `dyn FnMut` (not `impl FnMut`) to avoid monomorphization
/// bloat when called from hundreds of rules.
pub(crate) fn walk_function_bodies(
    typed_context: &TypedContext,
    visitor: &mut dyn FnMut(StmtId, &WalkContext),
) {
    let arena = typed_context.arena();
    let mut walk_ctx = WalkContext {
        loop_depth: 0,
        nondet_depth: 0,
        nondet_block_kind: None,
    };

    for source_file in typed_context.source_files() {
        for_each_function_body(arena, &source_file.defs, &mut |body_id| {
            assert_eq!(walk_ctx.loop_depth, 0, "loop_depth leaked");
            assert_eq!(walk_ctx.nondet_depth, 0, "nondet_depth leaked");
            assert!(walk_ctx.nondet_block_kind.is_none(), "nondet_block_kind leaked");
            walk_block(arena, body_id, &mut walk_ctx, visitor);
        });
    }
}

/// Extracts top-level expressions from a statement and calls the callback
/// for each one. Covers variable definitions, expression statements,
/// assignments, returns, asserts, if conditions, and loop conditions.
/// Does not recurse into sub-expressions.
pub(crate) fn for_each_stmt_expr(
    stmt: &Stmt,
    callback: &mut dyn FnMut(ExprId),
) {
    match stmt {
        Stmt::VarDef {
            value: Some(expr_id),
            ..
        }
        | Stmt::Expr(expr_id) => callback(*expr_id),
        Stmt::Assign { left, right } => {
            callback(*left);
            callback(*right);
        }
        Stmt::Return { expr } | Stmt::Assert { expr } => callback(*expr),
        Stmt::If { condition, .. } => callback(*condition),
        Stmt::Loop {
            condition: Some(cond_expr),
            ..
        } => callback(*cond_expr),
        _ => {}
    }
}

/// Recursively visits all sub-expressions in pre-order, calling `visitor`
/// for every node including the root.
pub(crate) fn walk_expr(
    arena: &AstArena,
    expr_id: ExprId,
    visitor: &mut dyn FnMut(ExprId),
) {
    visitor(expr_id);
    match &arena[expr_id].kind {
        Expr::FunctionCall {
            function, args, ..
        } => {
            walk_expr(arena, *function, visitor);
            for (_, arg_expr) in args {
                walk_expr(arena, *arg_expr, visitor);
            }
        }
        Expr::Binary { left, right, .. } => {
            walk_expr(arena, *left, visitor);
            walk_expr(arena, *right, visitor);
        }
        Expr::PrefixUnary { expr, .. }
        | Expr::Parenthesized { expr }
        | Expr::MemberAccess { expr, .. }
        | Expr::TypeMemberAccess { expr, .. } => walk_expr(arena, *expr, visitor),
        Expr::ArrayIndexAccess { array, index } => {
            walk_expr(arena, *array, visitor);
            walk_expr(arena, *index, visitor);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, field_expr) in fields {
                walk_expr(arena, *field_expr, visitor);
            }
        }
        Expr::ArrayLiteral { elements } => {
            for elem in elements {
                walk_expr(arena, *elem, visitor);
            }
        }
        Expr::Identifier(_)
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki
        | Expr::Type(_) => {}
    }
}

/// Returns `true` when `expr_id` is a function call that returns a compound
/// type (array, struct, or custom). Used by multiple rules to detect sret
/// calling convention restrictions.
pub(crate) fn is_compound_returning_call(ctx: &TypedContext, expr_id: ExprId) -> bool {
    if !matches!(ctx.arena()[expr_id].kind, Expr::FunctionCall { .. }) {
        return false;
    }
    if let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
        matches!(
            ti.kind,
            TypeInfoKind::Array(_, _) | TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_)
        )
    } else {
        false
    }
}

/// Recursively walks all `Def` variants and calls `callback` for each
/// function body found. Handles struct methods, spec definitions (recursive),
/// and module definitions (recursive).
pub(crate) fn for_each_function_body(
    arena: &AstArena,
    def_ids: &[DefId],
    callback: &mut dyn FnMut(BlockId),
) {
    for &def_id in def_ids {
        match &arena[def_id].kind {
            Def::Function { body, .. } => {
                callback(*body);
            }
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function { body, .. } = &arena[method_id].kind {
                        callback(*body);
                    }
                }
            }
            Def::Spec { defs, .. } => {
                for_each_function_body(arena, defs, callback);
            }
            Def::Module { defs, .. } => {
                if let Some(body_defs) = defs {
                    for_each_function_body(arena, body_defs, callback);
                }
            }
            Def::Enum { .. }
            | Def::Constant { .. }
            | Def::ExternFunction { .. }
            | Def::TypeAlias { .. } => {}
        }
    }
}

fn walk_block(
    arena: &AstArena,
    block_id: BlockId,
    ctx: &mut WalkContext,
    visitor: &mut dyn FnMut(StmtId, &WalkContext),
) {
    let block = &arena[block_id];
    if block.block_kind.is_non_det() {
        let prev_kind = ctx.nondet_block_kind;
        ctx.nondet_block_kind = Some(block_kind_label(block.block_kind));
        ctx.nondet_depth += 1;
        walk_statements(arena, &block.stmts, ctx, visitor);
        ctx.nondet_depth -= 1;
        ctx.nondet_block_kind = prev_kind;
    } else {
        walk_statements(arena, &block.stmts, ctx, visitor);
    }
}

fn walk_statements(
    arena: &AstArena,
    stmt_ids: &[StmtId],
    ctx: &mut WalkContext,
    visitor: &mut dyn FnMut(StmtId, &WalkContext),
) {
    for &stmt_id in stmt_ids {
        walk_statement(arena, stmt_id, ctx, visitor);
    }
}

fn walk_statement(
    arena: &AstArena,
    stmt_id: StmtId,
    ctx: &mut WalkContext,
    visitor: &mut dyn FnMut(StmtId, &WalkContext),
) {
    // Pre-order: call visitor BEFORE recursing into children.
    visitor(stmt_id, ctx);

    match &arena[stmt_id].kind {
        Stmt::Loop { body, .. } => {
            ctx.loop_depth += 1;
            walk_block(arena, *body, ctx, visitor);
            ctx.loop_depth -= 1;
        }
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            walk_block(arena, *then_block, ctx, visitor);
            if let Some(else_id) = else_block {
                walk_block(arena, *else_id, ctx, visitor);
            }
        }
        Stmt::Block(block_id) => {
            walk_block(arena, *block_id, ctx, visitor);
        }
        Stmt::Assign { .. }
        | Stmt::Return { .. }
        | Stmt::Break
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::TypeDef { .. }
        | Stmt::Assert { .. }
        | Stmt::ConstDef(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_ast::arena::AstArena;
    use inference_ast::ids::*;
    use inference_ast::nodes::*;

    fn dummy_location() -> Location {
        Location::default()
    }

    fn alloc_ident(arena: &mut AstArena, name: &str) -> IdentId {
        arena.idents.alloc(Ident {
            location: dummy_location(),
            name: name.to_string(),
        })
    }

    fn alloc_break_block(arena: &mut AstArena) -> BlockId {
        let break_stmt = arena.stmts.alloc(StmtData {
            location: dummy_location(),
            kind: Stmt::Break,
        });
        arena.blocks.alloc(BlockData {
            location: dummy_location(),
            block_kind: BlockKind::Regular,
            stmts: vec![break_stmt],
        })
    }

    fn alloc_unit_type(arena: &mut AstArena) -> TypeId {
        arena.types.alloc(TypeData {
            location: dummy_location(),
            kind: TypeNode::Simple(SimpleTypeKind::Unit),
        })
    }

    fn alloc_function_with_break(arena: &mut AstArena, name: &str) -> DefId {
        let name_id = alloc_ident(arena, name);
        let body_id = alloc_break_block(arena);
        arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Function {
                name: name_id,
                vis: Visibility::default(),
                type_params: vec![],
                args: vec![],
                returns: None,
                body: body_id,
            },
        })
    }

    #[test]
    fn for_each_function_body_visits_free_function() {
        let mut arena = AstArena::default();
        let def_id = alloc_function_with_break(&mut arena, "free_fn");
        let mut count = 0;
        for_each_function_body(&arena, &[def_id], &mut |_body| {
            count += 1;
        });
        assert_eq!(count, 1, "should visit 1 free function body");
    }

    #[test]
    fn for_each_function_body_visits_struct_methods() {
        let mut arena = AstArena::default();
        let method_a = alloc_function_with_break(&mut arena, "method_a");
        let method_b = alloc_function_with_break(&mut arena, "method_b");
        let struct_name = alloc_ident(&mut arena, "Foo");
        let struct_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Struct {
                name: struct_name,
                vis: Visibility::default(),
                fields: vec![],
                methods: vec![method_a, method_b],
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[struct_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(count, 2, "should visit 2 struct method bodies");
    }

    #[test]
    fn for_each_function_body_visits_spec_functions() {
        let mut arena = AstArena::default();
        let check_fn = alloc_function_with_break(&mut arena, "check");
        let spec_name = alloc_ident(&mut arena, "MySpec");
        let spec_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Spec {
                name: spec_name,
                vis: Visibility::default(),
                defs: vec![check_fn],
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[spec_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(count, 1, "should visit 1 spec function body");
    }

    #[test]
    fn for_each_function_body_visits_spec_nested_struct_method() {
        let mut arena = AstArena::default();
        let method = alloc_function_with_break(&mut arena, "method");
        let inner_struct_name = alloc_ident(&mut arena, "Inner");
        let inner_struct = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Struct {
                name: inner_struct_name,
                vis: Visibility::default(),
                fields: vec![],
                methods: vec![method],
            },
        });
        let spec_name = alloc_ident(&mut arena, "MySpec");
        let spec_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Spec {
                name: spec_name,
                vis: Visibility::default(),
                defs: vec![inner_struct],
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[spec_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(
            count, 1,
            "should visit struct method inside spec definition"
        );
    }

    #[test]
    fn for_each_function_body_visits_module_function() {
        let mut arena = AstArena::default();
        let helper = alloc_function_with_break(&mut arena, "helper");
        let module_name = alloc_ident(&mut arena, "utils");
        let module_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Module {
                name: module_name,
                vis: Visibility::default(),
                defs: Some(vec![helper]),
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[module_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(count, 1, "should visit function inside module body");
    }

    #[test]
    fn for_each_function_body_visits_module_struct_method() {
        let mut arena = AstArena::default();
        let method = alloc_function_with_break(&mut arena, "method");
        let bar_name = alloc_ident(&mut arena, "Bar");
        let inner_struct = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Struct {
                name: bar_name,
                vis: Visibility::default(),
                fields: vec![],
                methods: vec![method],
            },
        });
        let module_name = alloc_ident(&mut arena, "utils");
        let module_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Module {
                name: module_name,
                vis: Visibility::default(),
                defs: Some(vec![inner_struct]),
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[module_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(
            count, 1,
            "should visit struct method inside module definition"
        );
    }

    #[test]
    fn for_each_function_body_skips_module_without_body() {
        let mut arena = AstArena::default();
        let module_name = alloc_ident(&mut arena, "external_mod");
        let module_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Module {
                name: module_name,
                vis: Visibility::default(),
                defs: None,
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[module_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(count, 0, "should skip module with no body (external mod)");
    }

    #[test]
    fn for_each_function_body_skips_non_function_definitions() {
        let mut arena = AstArena::default();
        let color_name = alloc_ident(&mut arena, "Color");
        let enum_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Enum {
                name: color_name,
                vis: Visibility::default(),
                variants: vec![],
            },
        });
        let max_name = alloc_ident(&mut arena, "MAX");
        let unit_type = alloc_unit_type(&mut arena);
        let value_expr = arena.exprs.alloc(ExprData {
            location: dummy_location(),
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        let const_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Constant {
                name: max_name,
                vis: Visibility::default(),
                ty: unit_type,
                value: value_expr,
            },
        });
        let alias_name = alloc_ident(&mut arena, "Alias");
        let alias_type = alloc_unit_type(&mut arena);
        let type_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::TypeAlias {
                name: alias_name,
                vis: Visibility::default(),
                ty: alias_type,
            },
        });
        let mut count = 0;
        for_each_function_body(&arena, &[enum_def, const_def, type_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(
            count, 0,
            "should not visit bodies for enum, constant, or type alias definitions"
        );
    }

    #[test]
    fn for_each_function_body_mixed_definitions() {
        let mut arena = AstArena::default();
        let free_fn = alloc_function_with_break(&mut arena, "free_fn");

        let struct_method = alloc_function_with_break(&mut arena, "method");
        let foo_name = alloc_ident(&mut arena, "Foo");
        let struct_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Struct {
                name: foo_name,
                vis: Visibility::default(),
                fields: vec![],
                methods: vec![struct_method],
            },
        });

        let spec_check = alloc_function_with_break(&mut arena, "check");
        let spec_name = alloc_ident(&mut arena, "MySpec");
        let spec_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Spec {
                name: spec_name,
                vis: Visibility::default(),
                defs: vec![spec_check],
            },
        });

        let mod_helper = alloc_function_with_break(&mut arena, "helper");
        let utils_name = alloc_ident(&mut arena, "utils");
        let module_def = arena.defs.alloc(DefData {
            location: dummy_location(),
            kind: Def::Module {
                name: utils_name,
                vis: Visibility::default(),
                defs: Some(vec![mod_helper]),
            },
        });

        let mut count = 0;
        for_each_function_body(
            &arena,
            &[free_fn, struct_def, spec_def, module_def],
            &mut |_body| {
                count += 1;
            },
        );
        assert_eq!(
            count, 4,
            "should visit: 1 free fn + 1 struct method + 1 spec fn + 1 module fn = 4"
        );
    }
}
