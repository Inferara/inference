//! A045: A field-less struct has no value representation.
//!
//! A struct with no fields occupies zero bytes, so there is no memory region to
//! hold, copy, or reason about one of its values. Codegen's `compute_frame_layout`
//! allocates a struct frame slot only when the struct's total size is greater
//! than zero, while struct-literal lowering unconditionally requires one — so a
//! field-less struct literal aborts the compiler, and a field-less binding or
//! parameter that survives is lowered under a representation nobody chose (a
//! pointer into a zero-byte region). This rule rejects the *values*, at every
//! position where one can be introduced or consumed:
//!
//! - a struct-literal expression of such a type, in every expression position;
//! - a `let` binding, or a `const` declaration at either function or module
//!   scope, of such a type (the `let` case is also what covers a
//!   non-deterministic draw, since a compound `@` is only reachable as a `let`
//!   initializer — A008/A023/A038/A039/A040 reject the other positions);
//! - a function, method, or `external fn` parameter, including `_: E`;
//! - a function, method, or `external fn` return type;
//! - a struct field;
//! - a `self` / `mut self` receiver declared on a field-less struct.
//!
//! Each position also accepts an array of a field-less struct at any nesting
//! depth: an array is zero-sized exactly when its element type is (array lengths
//! must be positive), and an array type is never a value position on its own — it
//! always sits inside one of the annotations above.
//!
//! ## Why the field position closes the hole
//!
//! Zero-sized types would otherwise compose: a struct all of whose fields are
//! zero-sized is itself zero-sized. Rejecting a field-less struct as the type of a
//! *field* collapses that composition to its base case — in any accepted program a
//! struct is zero-sized if and only if it has no fields. The predicate is therefore
//! the non-recursive `fields.is_empty()` (plus array-element recursion) rather than
//! a transitive size computation, with no visited set and no cycle handling. With
//! every value-introducing position rejected, no value of a zero-sized type exists
//! in an accepted program, so struct-literal lowering can never be entered for one.
//!
//! Assignments between such values (`e = e`), field reads, and method calls on them
//! need no checks of their own: each requires a binding, parameter, or field of the
//! type, all of which are rejected here. Anchoring on declarations and literals is
//! what keeps the report to one diagnostic per offending declaration rather than
//! one per use.
//!
//! A module-scope `const` is checked in its own right rather than left to A032,
//! which rejects *every* top-level `const` as not yet implemented (#171). A032 is
//! a temporary gate on an unimplemented feature; resting the closure on it would
//! make this rule silently incomplete the day that feature lands. Both fire on
//! such a declaration today — the crate has no cross-rule suppression.
//!
//! ## What stays legal
//!
//! Declaring a field-less struct. A struct with no fields but with associated
//! functions is the supported method-namespace idiom (`E::helper()`), and nothing
//! about it needs values, so it compiles unchanged. A011 is untouched: it warns
//! about a struct with no fields *and no methods*, which is a declaration that
//! declares nothing — a disjoint subject from this rule, and deliberately silent
//! on the namespace idiom. Where a bare field-less struct is also given a value,
//! both fire: two facts, two messages, one fix.
//!
//! The `self` receiver is rejected at its declaration rather than left to the call
//! site: a receiver is a parameter that spells its type implicitly, and once no
//! value of the struct can exist the method is uncallable by construction. The fix
//! is to drop the `self` keyword, which turns the method into exactly the
//! associated function the namespace idiom uses — the same advice A010 already
//! gives, escalated to a requirement in the one case where the method is not merely
//! stylistically odd but structurally uncallable.
//!
//! `external fn` signatures are checked for their ABI surface (an emitted import
//! whose parameter is a pointer to a zero-byte region), not for the closure: A024
//! rejects every call to an extern function, so no value can flow through one.
//!
//! Like A042 and A043, the rule is unconditional — it inspects source shape, not
//! compilation mode. A zero-sized value is as undefined for the Rocq translation as
//! it is for WebAssembly.
//!
//! ## Documented non-scope
//!
//! - Generics: a type parameter never resolves to a struct, so a generic
//!   signature (`fn id T'(x: T) -> T`) is outside the predicate. Nothing is missed
//!   by that today: the compiler does not monomorphize — codegen rejects a generic
//!   type outright — so no instantiation at a field-less struct exists to check.
//!   Implementing generic instantiation would introduce value positions this rule
//!   does not yet see, and must revisit it.
//! - Local type aliases: `type X = E;` is not flagged. Aliases are non-transparent
//!   in Inference (`let a: X` does not resolve to the aliased type), so an alias is
//!   a dead end rather than a route to a value.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, IdentId, NodeId, StmtId, TypeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Expr, Field, Location, Stmt};
use inference_type_checker::type_info::TypeInfo;
use inference_type_checker::typed_context::TypedContext;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};
use crate::rules::position::{
    PARAMETER_TYPE, RETURN_TYPE, SELF_RECEIVER_TYPE, STRUCT_FIELD_TYPE, STRUCT_LITERAL,
    VARIABLE_TYPE,
};
use crate::walker;

crate::rule! {
    /// A field-less struct has no value representation and may not be used as
    /// one.
    #[id = "A045"]
    #[name = "Field-less struct value"]
    #[severity = error]
    pub struct FieldLessStructValue;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(ctx, arena, &source_file.module_path, &source_file.defs, &mut errors);
        }
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            check_stmt(ctx, &walk_ctx.module_path, stmt_id, &mut errors);
        });
        errors
    }
}

/// Checks the declaration surface of a file: function, method, and `external fn`
/// signatures, struct fields, `self` receivers, and module-scope `const`
/// declarations, recursing through `spec`.
fn check_defs(
    ctx: &TypedContext,
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { args, returns, .. } | Def::ExternFunction { args, returns, .. } => {
                check_signature(ctx, arena, module_path, args, *returns, None, errors);
            }
            Def::Struct {
                name,
                fields,
                methods,
                ..
            } => {
                let struct_name = arena[*name].name.clone();
                check_struct_fields(ctx, arena, module_path, *name, fields, errors);
                // A `self` receiver is a parameter that spells its type — the
                // enclosing struct — implicitly, so it is reported exactly when
                // that struct is the field-less one.
                let receiver = fields.is_empty().then_some(struct_name.as_str());
                for &method_id in methods {
                    if let Def::Function { args, returns, .. } = &arena[method_id].kind {
                        check_signature(ctx, arena, module_path, args, *returns, receiver, errors);
                    }
                }
            }
            // A module-scope `const` is the twin of the function-local one the
            // body walk checks: the annotation is a value position and the
            // initializer may build a literal. It is checked here rather than
            // left to A032's blanket rejection of top-level `const`, which is a
            // gate on an unimplemented feature and not part of this closure.
            Def::Constant { ty, value, .. } => {
                check_annotation(
                    ctx,
                    arena,
                    module_path,
                    *ty,
                    VARIABLE_TYPE,
                    arena[def_id].location,
                    errors,
                );
                check_struct_literals(ctx, module_path, *value, errors);
            }
            Def::Spec { defs, .. } => check_defs(ctx, arena, module_path, defs, errors),
            Def::Enum { .. } | Def::TypeAlias { .. } => {}
        }
    }
}

/// Reports each field whose type is a field-less struct, or an array of one.
///
/// Field types are read from the resolved [`StructInfo`], whose `type_info`
/// already carries canonical keys, and the caret is recovered by matching the
/// resolved field back to the AST field it was declared by.
///
/// [`StructInfo`]: inference_type_checker::StructInfo
fn check_struct_fields(
    ctx: &TypedContext,
    arena: &AstArena,
    module_path: &[String],
    name: IdentId,
    fields: &[Field],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    // Resolve by the struct's own file so a same-named struct in another file is
    // not picked up by its bare name.
    let Some(struct_info) = ctx.lookup_struct_in(&arena[name].name, module_path) else {
        return;
    };
    for tc_field in &struct_info.fields {
        if let Some(fieldless) =
            walker::fieldless_struct_name(ctx, &tc_field.type_info.kind, module_path)
        {
            let location = fields
                .iter()
                .find(|f| arena[f.name].name == tc_field.name)
                .map_or_else(|| arena[name].location, |f| arena[f.ty].location);
            push(errors, module_path, fieldless, STRUCT_FIELD_TYPE, location);
        }
    }
}

/// Checks every parameter and the return type of one signature.
///
/// `receiver` names the enclosing struct when it is field-less, which is the only
/// case in which a `self` receiver is reported.
fn check_signature(
    ctx: &TypedContext,
    arena: &AstArena,
    module_path: &[String],
    args: &[ArgData],
    returns: Option<TypeId>,
    receiver: Option<&str>,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for arg in args {
        match &arg.kind {
            ArgKind::Named { ty, .. } | ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => {
                check_annotation(
                    ctx,
                    arena,
                    module_path,
                    *ty,
                    PARAMETER_TYPE,
                    arg.location,
                    errors,
                );
            }
            ArgKind::SelfRef { .. } => {
                if let Some(struct_name) = receiver {
                    push(
                        errors,
                        module_path,
                        struct_name.to_string(),
                        SELF_RECEIVER_TYPE,
                        arg.location,
                    );
                }
            }
        }
    }
    if let Some(ty) = returns {
        check_annotation(
            ctx,
            arena,
            module_path,
            ty,
            RETURN_TYPE,
            arena[ty].location,
            errors,
        );
    }
}

/// Reports `location` when the raw type annotation `ty` names a field-less
/// struct, or an array of one at any depth.
fn check_annotation(
    ctx: &TypedContext,
    arena: &AstArena,
    module_path: &[String],
    ty: TypeId,
    position: &'static str,
    location: Location,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let type_info = TypeInfo::from_type_id(arena, ty);
    if let Some(fieldless) = walker::fieldless_struct_name(ctx, &type_info.kind, module_path) {
        push(errors, module_path, fieldless, position, location);
    }
}

/// Checks one statement of a function body: the declared type of a `let` or a
/// function-local `const`, and every struct literal reachable from it.
fn check_stmt(
    ctx: &TypedContext,
    module_path: &[String],
    stmt_id: StmtId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    // The recorded statement type is where declared and inferred binding types
    // arrive uniformly; the caret is the declaration, since an un-annotated
    // binding has no type node to point at.
    if matches!(
        &arena[stmt_id].kind,
        Stmt::VarDef { .. } | Stmt::ConstDef(_)
    ) && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id))
        && let Some(fieldless) = walker::fieldless_struct_name(ctx, &type_info.kind, module_path)
    {
        push(
            errors,
            module_path,
            fieldless,
            VARIABLE_TYPE,
            arena[stmt_id].location,
        );
    }
    walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
        check_struct_literals(ctx, module_path, expr_id, errors);
    });
}

/// Reports every field-less struct literal reachable from `expr_id`, including
/// `expr_id` itself.
fn check_struct_literals(
    ctx: &TypedContext,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    walker::walk_expr(arena, expr_id, &mut |sub_id| {
        if matches!(&arena[sub_id].kind, Expr::StructLiteral { .. })
            && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Expr(sub_id))
            && let Some(fieldless) =
                walker::fieldless_struct_name(ctx, &type_info.kind, module_path)
        {
            push(
                errors,
                module_path,
                fieldless,
                STRUCT_LITERAL,
                arena[sub_id].location,
            );
        }
    });
}

fn push(
    errors: &mut Vec<LabeledDiagnostic>,
    module_path: &[String],
    name: String,
    position: &'static str,
    location: Location,
) {
    errors.push(LabeledDiagnostic::new(
        module_path.to_vec(),
        AnalysisDiagnostic::FieldLessStructValue {
            name,
            position,
            location,
        },
    ));
}
