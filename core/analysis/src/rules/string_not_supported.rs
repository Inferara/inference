//! A048: `string` has no value representation, so no value of it may be
//! introduced.
//!
//! `string` and `String` are registered as root-scope builtin type names, so
//! every annotation that spells one type-checks: `let s: string`, `fn f(s:
//! string)`, `struct S { s: string; }`, `[string; 2]`. Nothing after the type
//! checker can act on that. There is no layout for a string in linear memory,
//! so frame layout has no byte size to give one; there is no WebAssembly value
//! type to pass one in, so a signature carrying one has nothing to lower to;
//! and there is no term for a proof to describe one with, so the Rocq
//! translation has nothing to say about it either. A string literal was
//! therefore an expression code generation could only abort on; a string in a
//! frame or a struct aborted one layer earlier, in the byte-size computation
//! that lays those out; and a `string` signature failed with an unsupported-type
//! error. Three failure modes for one missing feature, none of them a
//! diagnostic anyone could act on.
//!
//! The type name is kept and the *values* are rejected, which is what makes the
//! diagnostic worth reading: an author who writes `string` is told the feature
//! is not implemented and what to model text with instead, rather than being
//! told `string` is an unknown type — which would be a worse answer to the same
//! question, and would still leave the literal to reject.
//!
//! ## What decides the report
//!
//! A finding is pushed for each of these, independently:
//!
//! - a `StringLiteral` expression, in every expression position;
//! - the recorded type of a `let` binding or of a `const`, at function or
//!   module scope;
//! - a function, method, or `external fn` parameter — `Named`, `_: string`, and
//!   a bare positional `string` alike;
//! - a function, method, or `external fn` return type;
//! - a struct field.
//!
//! Every type position looks through array nesting at any depth
//! ([`walker::innermost_element`]), because an array of strings is exactly as
//! unrepresentable as a string and an array type is never a value position on
//! its own — it always sits inside one of the annotations above. `[[string; 2];
//! 3]` is one finding on the annotation that carries it, not one per layer.
//!
//! Reporting is per offending construct rather than per declaration, so `let s:
//! string = "hi";` reports twice: the annotation and the literal are two
//! separate things to remove, and a reader repairing one still has to see the
//! other.
//!
//! Type positions are read from the annotation as written rather than from the
//! resolved struct table, because the predicate is a builtin type kind, which
//! `TypeInfo::from_type_id` decides on its own. That keeps the caret on the type
//! node the author wrote and cannot miss a declaration whose *other* types
//! failed to resolve. The binding half is the exception: it reads the type the
//! checker recorded for the statement, which is the resolved one, so an
//! initializer-typed binding is judged by what it was actually given.
//!
//! ## Why `spec` bodies are covered
//!
//! A `spec` function is lowered to a real WebAssembly function in proof mode,
//! so a string literal in a spec body reaches expression lowering exactly as a
//! top-level one does, and aborted there in exactly the same way. Compile mode
//! does not emit spec functions at all, so nothing in a spec body reaches code
//! generation there — which is why the rule is stated on source shape rather
//! than on a mode. One message, whichever mode the program is compiled in.
//!
//! The proof translation's own rejection is not a substitute. It covers a
//! string literal appearing in an *assertion term*, which is a different path
//! from the function body's WebAssembly lowering and leaves the rest of the body
//! open. Where both apply the program is reported twice — the crate has no
//! cross-rule suppression, and two statements of the same fact are better than a
//! position in which neither is made.
//!
//! ## What stays legal
//!
//! Nothing about `string` is legal as a value, and the rule says so uniformly;
//! there is no position where a string is accepted and no partial support to
//! keep track of. Two positions are nonetheless outside the predicate, and both
//! are deliberate:
//!
//! - **Type aliases**, whether the item form `type S = string;` or the
//!   statement form inside a body. Aliases are nominal in Inference — a
//!   binding annotated with the alias does not resolve to the aliased type — so
//!   an alias declares a name at which no value can be produced. Every position
//!   that could produce one is covered above. This is the same non-scope A045
//!   records for the same reason.
//! - **The `self` receiver**, whose type is the enclosing struct and so is
//!   never `string`.
//!
//! The rule is unconditional: it inspects source shape, not compilation mode,
//! because a string is as undefined for the Rocq translation as it is for
//! WebAssembly.
//!
//! The day strings are implemented — a layout, a data segment, and a proof term
//! — this rule is deleted whole. It gates an unimplemented feature; it does not
//! describe a language decision.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, NodeId, StmtId, TypeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Expr, Field, Location, Stmt};
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};
use crate::rules::position::{
    PARAMETER_TYPE, RETURN_TYPE, STRING_LITERAL, STRUCT_FIELD_TYPE, VARIABLE_TYPE,
};
use crate::walker;

crate::rule! {
    /// `string` has no value representation and may not be used as one.
    #[id = "A048"]
    #[name = "String value not supported"]
    #[severity = error]
    pub struct StringNotSupported;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.module_path, &source_file.defs, &mut errors);
        }
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            check_stmt(ctx, &walk_ctx.module_path, stmt_id, &mut errors);
        });
        errors
    }
}

/// Whether a type is `string`, or an array of `string` at any nesting depth.
fn is_string(kind: &TypeInfoKind) -> bool {
    matches!(walker::innermost_element(kind), TypeInfoKind::String)
}

/// Checks the declaration surface of a file: function, method, and `external fn`
/// signatures, struct fields, and module-scope `const` declarations, recursing
/// through `spec`.
fn check_defs(
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { args, returns, .. } | Def::ExternFunction { args, returns, .. } => {
                check_signature(arena, module_path, args, *returns, errors);
            }
            Def::Struct { fields, methods, .. } => {
                check_struct_fields(arena, module_path, fields, errors);
                for &method_id in methods {
                    if let Def::Function { args, returns, .. } = &arena[method_id].kind {
                        check_signature(arena, module_path, args, *returns, errors);
                    }
                }
            }
            // A module-scope `const` is the twin of the function-local one the
            // body walk checks. It is checked here in its own right rather than
            // left to A032's blanket rejection of top-level `const`, which is a
            // gate on an unimplemented feature and not part of this closure;
            // both fire on such a declaration.
            Def::Constant { ty, value, .. } => {
                check_annotation(
                    arena,
                    module_path,
                    *ty,
                    VARIABLE_TYPE,
                    arena[def_id].location,
                    errors,
                );
                check_string_literals(arena, module_path, *value, errors);
            }
            Def::Spec { defs, .. } => check_defs(arena, module_path, defs, errors),
            Def::Enum { .. } | Def::TypeAlias { .. } => {}
        }
    }
}

/// Reports each field declared `string`, or an array of it.
fn check_struct_fields(
    arena: &AstArena,
    module_path: &[String],
    fields: &[Field],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for field in fields {
        check_annotation(
            arena,
            module_path,
            field.ty,
            STRUCT_FIELD_TYPE,
            arena[field.ty].location,
            errors,
        );
    }
}

/// Checks every parameter and the return type of one signature.
///
/// A `self` receiver spells no type of its own — its type is the enclosing
/// struct — so it is never in scope here.
fn check_signature(
    arena: &AstArena,
    module_path: &[String],
    args: &[ArgData],
    returns: Option<TypeId>,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for arg in args {
        match &arg.kind {
            ArgKind::Named { ty, .. } | ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => {
                check_annotation(arena, module_path, *ty, PARAMETER_TYPE, arg.location, errors);
            }
            ArgKind::SelfRef { .. } => {}
        }
    }
    if let Some(ty) = returns {
        check_annotation(
            arena,
            module_path,
            ty,
            RETURN_TYPE,
            arena[ty].location,
            errors,
        );
    }
}

/// Reports `location` when the type annotation `ty` names `string`, or an array
/// of it at any depth.
fn check_annotation(
    arena: &AstArena,
    module_path: &[String],
    ty: TypeId,
    position: &'static str,
    location: Location,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    if is_string(&TypeInfo::from_type_id(arena, ty).kind) {
        push(errors, module_path, position, location);
    }
}

/// Checks one statement of a function body: the declared type of a `let` or a
/// function-local `const`, and every string literal reachable from it.
fn check_stmt(
    ctx: &TypedContext,
    module_path: &[String],
    stmt_id: StmtId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    // The recorded statement type is the resolved type the binding was given;
    // the caret is the declaration, which is where the annotation stands.
    if matches!(
        &arena[stmt_id].kind,
        Stmt::VarDef { .. } | Stmt::ConstDef(_)
    ) && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id))
        && is_string(&type_info.kind)
    {
        push(errors, module_path, VARIABLE_TYPE, arena[stmt_id].location);
    }
    walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
        check_string_literals(arena, module_path, expr_id, errors);
    });
}

/// Reports every string literal reachable from `expr_id`, including `expr_id`
/// itself.
fn check_string_literals(
    arena: &AstArena,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    walker::walk_expr(arena, expr_id, &mut |sub_id| {
        if matches!(&arena[sub_id].kind, Expr::StringLiteral { .. }) {
            push(errors, module_path, STRING_LITERAL, arena[sub_id].location);
        }
    });
}

fn push(
    errors: &mut Vec<LabeledDiagnostic>,
    module_path: &[String],
    position: &'static str,
    location: Location,
) {
    errors.push(LabeledDiagnostic::new(
        module_path.to_vec(),
        AnalysisDiagnostic::StringNotSupported { position, location },
    ));
}
