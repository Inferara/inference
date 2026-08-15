//! Shared AST walker with depth tracking for analysis rules.
//!
//! Extracts the traversal logic into a reusable function that any rule can
//! call with its own visitor closure. The walker resolves arena-indexed IDs
//! to access node data.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, NodeId, StmtId};
use inference_ast::nodes::{BlockKind, Def, Expr, Stmt, UnaryOperatorKind};
use inference_type_checker::StructInfo;
use inference_type_checker::type_info::TypeInfoKind;
use inference_type_checker::typed_context::TypedContext;

use crate::errors::NonDetBlockKind;

/// Context passed to visitor callbacks during AST walking.
pub(crate) struct WalkContext {
    pub loop_depth: u32,
    pub nondet_depth: u32,
    pub nondet_block_kind: Option<NonDetBlockKind>,
    /// Module path of the file whose body is currently being walked (empty for
    /// the entry file). A rule pairs each finding with this so the report names
    /// the file it belongs to.
    pub module_path: Vec<String>,
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
        module_path: Vec::new(),
    };

    for source_file in typed_context.source_files() {
        walk_ctx.module_path.clone_from(&source_file.module_path);
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
/// assignments, returns, asserts, if conditions, loop conditions, and
/// constant definitions. Does not recurse into sub-expressions.
pub(crate) fn for_each_stmt_expr(
    stmt: &Stmt,
    arena: &AstArena,
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
        Stmt::ConstDef(def_id) => {
            if let Def::Constant { value, .. } = &arena[*def_id].kind {
                callback(*value);
            }
        }
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

/// Returns the numeric literal a `-` is applied to but written apart from,
/// or `None` when `expr_id` is anything else.
///
/// This is A046's whole predicate, and the exact construct A022 hands over to
/// it: a `Neg` whose operand is *directly* a `NumberLiteral` — no parentheses
/// peeled — where the literal does not begin one byte after the minus. Both
/// rules read it from here so neither can drift into flagging a shape the other
/// has stopped covering.
///
/// Separation is measured on offsets rather than on the source text, which
/// analysis never sees: a `PrefixUnary` node starts at its operator, so the only
/// spelling in which the digits begin at `offset_start + 1` is the glued one.
/// Every kind of gap — a space, several, a newline, a line comment — puts them
/// further along, and none is distinguished from the others.
///
/// A literal whose own text carries a sign is excluded, because that is the
/// grammar's eager lexing of `--42` / `- -42` and belongs to A033: `--42` is
/// already glued and would be excluded by the offsets alone, but the spaced
/// `- -42` is not, and A046's advice there ("write `--42`") would recommend a
/// form A033 rejects. Restricting the predicate to an unsigned literal keeps the
/// two rules disjoint by construction: A033 owns every doubled sign, A046 owns
/// the single detached one.
pub(crate) fn separated_negated_literal(arena: &AstArena, expr_id: ExprId) -> Option<ExprId> {
    let Expr::PrefixUnary {
        op: UnaryOperatorKind::Neg,
        expr: operand,
    } = &arena[expr_id].kind
    else {
        return None;
    };
    let Expr::NumberLiteral { value } = &arena[*operand].kind else {
        return None;
    };
    let separated =
        arena[*operand].location.offset_start > arena[expr_id].location.offset_start + 1;
    (separated && !value.starts_with('-')).then_some(*operand)
}

/// Returns how many array layers deep a type is.
///
/// `[i32; 3]` => 1, `[[i32; 3]; 2]` => 2, `[[[i32; 2]; 3]; 4]` => 3,
/// `i32` / `Point` => 0.
pub(crate) fn array_nesting_depth(kind: &TypeInfoKind) -> u32 {
    match kind {
        TypeInfoKind::Array(elem, _) => 1 + array_nesting_depth(&elem.kind),
        _ => 0,
    }
}

/// Returns true if a type is compound: a struct/custom type, or an array
/// whose innermost element type is compound. Scalar arrays like `[i32; 3]`
/// and multidimensional scalar arrays like `[[i32; 3]; 2]` are not compound.
#[must_use]
fn is_compound_type(ctx: &TypedContext, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Struct(_, _) => true,
        TypeInfoKind::Custom(name) => ctx.lookup_enum(name).is_none(),
        TypeInfoKind::Array(elem, _) => is_compound_type(ctx, &elem.kind),
        _ => false,
    }
}

/// Returns true if a compound type contains fields that are unsupported
/// for struct uzumaki lowering.
///
/// - **Struct/Custom**: looks up the struct definition and checks whether any
///   of its fields are nested structs, arrays of structs, or multidimensional
///   arrays (nesting depth > 1). Fields that are 1D scalar arrays (e.g.
///   `[i32; 3]`) are not considered compound.
/// - **Array**: recurses into the element type.
/// - **Scalars**: returns false.
#[must_use]
pub(crate) fn has_compound_fields(ctx: &TypedContext, kind: &TypeInfoKind) -> bool {
    match kind {
        // A resolved struct carries its canonical, file-qualified key; look it up
        // by that key so a field typed as a cross-file struct reaches the right
        // definition. A same-named struct in another file has a distinct key, so a
        // bare-name lookup would otherwise land on the wrong struct and misjudge
        // its nesting depth. `Custom` is an unresolved (or alias) name with no key,
        // for which the bare name is the only handle.
        TypeInfoKind::Struct(_, key) => ctx
            .lookup_struct(key)
            .is_some_and(|s| struct_has_compound_field(ctx, &s)),
        TypeInfoKind::Custom(name) => ctx
            .lookup_struct(name)
            .is_some_and(|s| struct_has_compound_field(ctx, &s)),
        TypeInfoKind::Array(elem, _) => has_compound_fields(ctx, &elem.kind),
        _ => false,
    }
}

/// Whether any field of `s` is itself a compound type (a struct, an array of
/// structs, or a multidimensional array), which would push nesting past the one
/// supported level.
#[must_use = "this is a pure check with no side effects"]
fn struct_has_compound_field(ctx: &TypedContext, s: &StructInfo) -> bool {
    s.fields.iter().any(|f| match &f.type_info.kind {
        TypeInfoKind::Struct(_, _) => true,
        TypeInfoKind::Custom(n) => ctx.lookup_enum(n).is_none(),
        TypeInfoKind::Array(_, _) => {
            is_compound_type(ctx, &f.type_info.kind) || array_nesting_depth(&f.type_info.kind) > 1
        }
        _ => false,
    })
}

/// Returns the bare name of the field-less struct a type is, or is an array of
/// at any nesting depth — `None` for every other type.
///
/// This is A045's whole predicate. A struct with no fields occupies zero bytes,
/// and an array is zero-sized exactly when its element type is (array lengths are
/// required to be positive), so looking through [`TypeInfoKind::Array`] at every
/// depth and testing `fields.is_empty()` on the leaf covers both. No transitive
/// size computation is needed: A045 also rejects a field-less struct as the type
/// of a struct *field*, so in any accepted program a struct is zero-sized if and
/// only if it has no fields.
///
/// All four type carriers are resolved, because a rule reading raw signature
/// annotations meets each of them: a resolved `Struct` carries its canonical,
/// file-qualified key and is looked up by that key alone — the key a resolved
/// carrier holds is by construction one the struct is registered under, so key
/// lookup is complete, and falling back to the bare name could only add a path
/// to a same-named struct in *another* file, which is exactly what the key
/// exists to distinguish; `Custom` is an unresolved (or alias) name whose only
/// handle is the bare name, resolved against the referencing file; and a
/// `::`-qualified annotation carries an unresolved path resolved against that
/// same file. Enums, scalars, and names that resolve to nothing yield `None`.
///
/// The returned name is the struct's bare name, which is how the source spells
/// it — never the canonical key, which is file-qualified and would read as noise
/// in a diagnostic.
pub(crate) fn fieldless_struct_name(
    ctx: &TypedContext,
    kind: &TypeInfoKind,
    module_path: &[String],
) -> Option<String> {
    let info = match kind {
        TypeInfoKind::Struct(_, key) => ctx.lookup_struct(key),
        TypeInfoKind::Custom(name) => ctx.lookup_struct_in(name, module_path),
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => ctx
            .resolve_struct_by_qualified_path(
                &path
                    .split("::")
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                module_path,
            )
            .map(|(info, _key)| info),
        TypeInfoKind::Array(elem, _) => {
            return fieldless_struct_name(ctx, &elem.kind, module_path);
        }
        _ => None,
    }?;
    info.fields.is_empty().then_some(info.name)
}

/// Classifies the `Custom`-named type carried by an uzumaki (`@`) node as
/// struct-like (`true`) or enum-like (`false`), for the "needs a named frame
/// slot" rules (A038/A039/A040).
///
/// The `debug_assert!` encodes an invariant those rules rely on: a `@` node
/// never reaches classification carrying a bare *enum*-named `Custom`. Every
/// site that types a `@` routes the type through `resolve_custom_type*`, so an
/// enum canonicalizes to `TypeInfoKind::Enum` first and only struct-like (or
/// genuinely unresolved) names remain as `Custom`. A future `@` position that
/// types from a raw, unresolved annotation would trip this. In release builds
/// the `lookup_enum` result is still returned, so behaviour is unchanged.
pub(crate) fn uzumaki_custom_is_struct_like(ctx: &TypedContext, name: &str) -> bool {
    let is_struct_like = ctx.lookup_enum(name).is_none();
    debug_assert!(
        is_struct_like,
        "uzumaki (@) node typed as Custom(`{name}`) that resolves to an enum; enums must \
         canonicalize to TypeInfoKind::Enum before classification (every @-typing site \
         resolves through resolve_custom_type*)"
    );
    is_struct_like
}

/// Returns `true` when `expr_id` is a function call that returns a compound
/// type (array, struct, or custom). Used by multiple rules to detect sret
/// calling convention restrictions.
pub(crate) fn is_compound_returning_call(ctx: &TypedContext, expr_id: ExprId) -> bool {
    if !matches!(ctx.arena()[expr_id].kind, Expr::FunctionCall { .. }) {
        return false;
    }
    if let Some(ti) = ctx.get_node_typeinfo(NodeId::Expr(expr_id)) {
        match &ti.kind {
            TypeInfoKind::Array(_, _) | TypeInfoKind::Struct(_, _) => true,
            TypeInfoKind::Custom(name) => ctx.lookup_enum(name).is_none(),
            _ => false,
        }
    } else {
        false
    }
}

/// Checks whether a loop body contains at least one `break` that targets
/// the current loop (not a nested inner loop).
///
/// This function scans the body recursively but:
/// - Does NOT recurse into nested `Loop` statement bodies (break there targets the nested loop)
/// - Does NOT recurse into non-det block bodies (break inside non-det is prohibited)
/// - DOES recurse into `if/else` arms and regular `Block` statements
pub(crate) fn contains_break_for_this_loop(arena: &AstArena, block_id: BlockId) -> bool {
    let block = &arena[block_id];
    if block.block_kind != BlockKind::Regular {
        return false;
    }
    block
        .stmts
        .iter()
        .any(|&sid| contains_break_in_stmt(arena, sid))
}

fn contains_break_in_stmt(arena: &AstArena, stmt_id: StmtId) -> bool {
    match &arena[stmt_id].kind {
        Stmt::Break => true,
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            contains_break_for_this_loop(arena, *then_block)
                || else_block.is_some_and(|b| contains_break_for_this_loop(arena, b))
        }
        Stmt::Block(block_id) => {
            let block = &arena[*block_id];
            if block.block_kind != BlockKind::Regular {
                return false;
            }
            block
                .stmts
                .iter()
                .any(|&sid| contains_break_in_stmt(arena, sid))
        }
        Stmt::Loop { .. }
        | Stmt::Return { .. }
        | Stmt::Assign { .. }
        | Stmt::Expr(_)
        | Stmt::VarDef { .. }
        | Stmt::TypeDef { .. }
        | Stmt::Assert { .. }
        | Stmt::ConstDef(_) => false,
    }
}

/// Recursively visits every statement in a single block, calling `visitor`
/// for each one. Unlike [`walk_function_bodies`], this walks one body in
/// isolation and tracks no loop/non-det depth — for rules that maintain their
/// own per-body context (e.g. an enclosing-scope stack) across the traversal.
pub(crate) fn walk_block_stmts(
    arena: &AstArena,
    block_id: BlockId,
    visitor: &mut dyn FnMut(StmtId),
) {
    for &stmt_id in &arena[block_id].stmts {
        walk_stmt_recursive(arena, stmt_id, visitor);
    }
}

fn walk_stmt_recursive(
    arena: &AstArena,
    stmt_id: StmtId,
    visitor: &mut dyn FnMut(StmtId),
) {
    visitor(stmt_id);
    match &arena[stmt_id].kind {
        Stmt::Loop { body, .. } | Stmt::Block(body) => {
            walk_block_stmts(arena, *body, visitor);
        }
        Stmt::If {
            then_block,
            else_block,
            ..
        } => {
            walk_block_stmts(arena, *then_block, visitor);
            if let Some(else_id) = else_block {
                walk_block_stmts(arena, *else_id, visitor);
            }
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
    if let Some(kind) = NonDetBlockKind::from_block_kind(block.block_kind) {
        let prev_kind = ctx.nondet_block_kind;
        ctx.nondet_block_kind = Some(kind);
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
    use inference_type_checker::type_info::{NumberType, TypeInfo};

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

    /// The entry file's (empty) module path: `register_test_struct` keys a
    /// struct by its bare name, which is its canonical key in a single file.
    const NO_PATH: &[String] = &[];

    /// Builds a `TypedContext` with the given structs and enums registered,
    /// mirroring the `register_test_struct` pattern used by other rule tests.
    fn ctx_with_types(
        structs: &[(&str, &[(&str, TypeInfoKind)])],
        enums: &[(&str, &[&str])],
    ) -> TypedContext {
        let mut ctx = TypedContext::default();
        for (name, fields) in structs {
            let field_specs: Vec<_> = fields
                .iter()
                .map(|(field_name, kind)| {
                    (
                        (*field_name).to_string(),
                        TypeInfo {
                            kind: kind.clone(),
                            type_params: vec![],
                        },
                    )
                })
                .collect();
            ctx.register_test_struct(name, &field_specs).unwrap();
        }
        for (name, variants) in enums {
            ctx.register_test_enum(name, variants).unwrap();
        }
        ctx
    }

    fn array_of(kind: TypeInfoKind, length: u32) -> TypeInfoKind {
        TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind,
                type_params: vec![],
            }),
            length,
        )
    }

    #[test]
    fn fieldless_struct_name_detects_struct_with_no_fields() {
        let ctx = ctx_with_types(&[("E", &[])], &[]);
        assert_eq!(
            fieldless_struct_name(&ctx, &TypeInfoKind::Custom("E".to_string()), NO_PATH),
            Some("E".to_string()),
            "a bare name resolving to a field-less struct must be detected"
        );
        assert_eq!(
            fieldless_struct_name(
                &ctx,
                &TypeInfoKind::Struct("E".to_string(), "E".to_string()),
                NO_PATH
            ),
            Some("E".to_string()),
            "a resolved struct type carrying a canonical key must be detected"
        );
    }

    #[test]
    fn fieldless_struct_name_ignores_struct_with_fields() {
        let ctx = ctx_with_types(
            &[("P", &[("x", TypeInfoKind::Number(NumberType::I32))])],
            &[],
        );
        assert_eq!(
            fieldless_struct_name(&ctx, &TypeInfoKind::Custom("P".to_string()), NO_PATH),
            None,
            "a struct with one field is not zero-sized"
        );
    }

    #[test]
    fn fieldless_struct_name_sees_through_array_nesting() {
        let ctx = ctx_with_types(&[("E", &[])], &[]);
        let e = || TypeInfoKind::Custom("E".to_string());
        assert_eq!(
            fieldless_struct_name(&ctx, &array_of(e(), 3), NO_PATH),
            Some("E".to_string()),
            "`[E; 3]` is zero-sized because its element type is"
        );
        assert_eq!(
            fieldless_struct_name(&ctx, &array_of(array_of(e(), 2), 3), NO_PATH),
            Some("E".to_string()),
            "the predicate must recurse past one array layer"
        );
        assert_eq!(
            fieldless_struct_name(
                &ctx,
                &array_of(TypeInfoKind::Number(NumberType::I32), 3),
                NO_PATH
            ),
            None,
            "an array of a scalar is never zero-sized"
        );
    }

    #[test]
    fn fieldless_struct_name_returns_none_for_scalars_bool_and_enum() {
        let ctx = ctx_with_types(
            &[("E", &[])],
            &[("Color", &["Red", "Green"]), ("Never", &[])],
        );
        for number_type in [
            NumberType::I8,
            NumberType::U8,
            NumberType::I16,
            NumberType::U16,
            NumberType::I32,
            NumberType::U32,
            NumberType::I64,
            NumberType::U64,
        ] {
            assert_eq!(
                fieldless_struct_name(&ctx, &TypeInfoKind::Number(number_type), NO_PATH),
                None,
                "{number_type:?} is a scalar, never zero-sized"
            );
        }
        assert_eq!(
            fieldless_struct_name(&ctx, &TypeInfoKind::Bool, NO_PATH),
            None
        );
        // An enum lowers to a 4-byte tag regardless of variant count, so even a
        // variantless one is not zero-sized — through either carrier.
        for kind in [
            TypeInfoKind::Custom("Color".to_string()),
            TypeInfoKind::Custom("Never".to_string()),
            TypeInfoKind::Enum("Never".to_string(), "Never".to_string()),
        ] {
            assert_eq!(
                fieldless_struct_name(&ctx, &kind, NO_PATH),
                None,
                "an enum is never zero-sized, got a hit for {kind:?}"
            );
        }
    }

    #[test]
    fn fieldless_struct_name_returns_none_for_unresolved_custom_name() {
        let ctx = ctx_with_types(&[("E", &[])], &[]);
        assert_eq!(
            fieldless_struct_name(&ctx, &TypeInfoKind::Custom("Nope".to_string()), NO_PATH),
            None,
            "an unresolved name must yield None rather than panicking"
        );
        assert_eq!(
            fieldless_struct_name(&ctx, &TypeInfoKind::Generic("T".to_string()), NO_PATH),
            None,
            "a generic type parameter never names a struct"
        );
    }

    /// Builds `<op> <literal>` with the operator at `op_start` and the literal
    /// at `literal_start` — the two offsets are the whole of what the predicate
    /// reads. Returns the operator node and its operand.
    fn alloc_negation(
        arena: &mut AstArena,
        op: UnaryOperatorKind,
        op_start: u32,
        literal: &str,
        literal_start: u32,
    ) -> (ExprId, ExprId) {
        let width = u32::try_from(literal.len()).expect("test literal fits a u32 span");
        let operand = arena.exprs.alloc(ExprData {
            location: Location::new(
                literal_start,
                literal_start + width,
                1,
                literal_start + 1,
                1,
                literal_start + 1 + width,
            ),
            kind: Expr::NumberLiteral {
                value: literal.to_string(),
            },
        });
        let negation = arena.exprs.alloc(ExprData {
            location: Location::new(op_start, literal_start, 1, op_start + 1, 1, op_start + 2),
            kind: Expr::PrefixUnary { expr: operand, op },
        });
        (negation, operand)
    }

    #[test]
    fn separated_negated_literal_measures_the_gap_at_one_byte() {
        // `-42` never reaches the predicate as a negation (the lexer folds it
        // into one token), but `--42` does, glued: the digits begin exactly one
        // byte after the minus. That boundary is what separates A033's subject
        // from A046's, so it is pinned on both sides.
        let mut arena = AstArena::default();
        let (glued, _) = alloc_negation(&mut arena, UnaryOperatorKind::Neg, 0, "42", 1);
        assert_eq!(
            separated_negated_literal(&arena, glued),
            None,
            "a literal beginning one byte after the minus is written glued"
        );
        let (spaced, spaced_literal) =
            alloc_negation(&mut arena, UnaryOperatorKind::Neg, 0, "42", 2);
        assert_eq!(
            separated_negated_literal(&arena, spaced),
            Some(spaced_literal),
            "one space is already a separation, and the operand is what A022 skips"
        );
        let (far, far_literal) = alloc_negation(&mut arena, UnaryOperatorKind::Neg, 0, "42", 40);
        assert_eq!(
            separated_negated_literal(&arena, far),
            Some(far_literal),
            "a newline or a comment is no different from a space"
        );
    }

    #[test]
    fn separated_negated_literal_ignores_a_signed_literal() {
        // `- -42` is separated, but the operand carries its own sign: that is
        // A033's doubled-operator subject, and A046's advice there would spell
        // out a form A033 rejects.
        let mut arena = AstArena::default();
        let (doubled, _) = alloc_negation(&mut arena, UnaryOperatorKind::Neg, 0, "-42", 2);
        assert_eq!(separated_negated_literal(&arena, doubled), None);
    }

    #[test]
    fn separated_negated_literal_ignores_other_operators() {
        // Only `-` is folded into a literal by the lexer, so only `-` has a
        // second spelling to remove.
        let mut arena = AstArena::default();
        for op in [UnaryOperatorKind::Not, UnaryOperatorKind::BitNot] {
            let (expr, _) = alloc_negation(&mut arena, op.clone(), 0, "42", 2);
            assert_eq!(
                separated_negated_literal(&arena, expr),
                None,
                "`{op:?}` is out of scope"
            );
        }
    }

    #[test]
    fn separated_negated_literal_ignores_a_non_literal_operand() {
        // Negating a value has no glued spelling to prefer.
        let mut arena = AstArena::default();
        let name = alloc_ident(&mut arena, "x");
        let operand = arena.exprs.alloc(ExprData {
            location: Location::new(2, 3, 1, 3, 1, 4),
            kind: Expr::Identifier(name),
        });
        let expr = arena.exprs.alloc(ExprData {
            location: Location::new(0, 3, 1, 1, 1, 2),
            kind: Expr::PrefixUnary {
                expr: operand,
                op: UnaryOperatorKind::Neg,
            },
        });
        assert_eq!(separated_negated_literal(&arena, expr), None);
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

        let mut count = 0;
        for_each_function_body(&arena, &[free_fn, struct_def, spec_def], &mut |_body| {
            count += 1;
        });
        assert_eq!(
            count, 3,
            "should visit: 1 free fn + 1 struct method + 1 spec fn = 3"
        );
    }
}
