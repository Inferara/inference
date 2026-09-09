//! A051: generic code has no lowering, so neither a type parameter nor a type
//! argument may be written.
//!
//! The type checker handles generics in full: at each call site it infers a type
//! argument from the arguments it was given and checks the body against it.
//! Nothing carries that substitution any further. The compiler does not
//! monomorphize, so a type parameter arrives at code generation still standing
//! for no type at all — there is no layout to size a frame slot with, no
//! WebAssembly value type to pass a value in, and no term for a proof to
//! describe one with. A type *argument* is worse off still: no type declaration
//! in the language accepts one. A struct, an enum and an `external fn` each
//! declare a bare name, and only a function binds a type parameter, so
//! `Q i32'` names no declaration at all.
//!
//! What that produced before this rule was not one failure but three, and only
//! the first was a failure. A binder that appears in a lowered type reached an
//! unsupported-type refusal with no location, because signature lowering sees a
//! `TypeId` and no enclosing type parameters and so cannot tell `T` from a
//! misspelled type name. A binder that appears nowhere lowered — `fn h T'(x:
//! i32)`, `pub fn main T'()` — was emitted, exported, and in proof mode shipped
//! a complete obligation with the binder silently discarded, while being
//! uncallable from Inference: with the binder absent from the parameter list the
//! type checker can infer nothing and refuses every call. And a binder that
//! shadows a declared type was read one way by the type checker, which binds the
//! name to the parameter, and the other way by code generation, which binds it
//! to the struct — a program that exits 0 and emits a module WebAssembly
//! validation rejects.
//!
//! ## What decides the report
//!
//! A finding is pushed for each of these, independently:
//!
//! - a `fn` declaring type parameters — a free function, a struct method, a
//!   `spec` function, or a method of a struct declared inside a `spec`. One
//!   finding per declaration, caret on the first binder;
//! - a type application (`Q i32'`) written as a function, method, or `external
//!   fn` parameter or return type, as a struct field, or as the declared type of
//!   a `let` or of a `const` at function or module scope;
//! - a type application in expression position, at any depth — a bare `Q i32';`
//!   statement and the one wrapped in `(Q i32');` alike.
//!
//! The declaration predicate is the declared type parameters and nothing else:
//! no reachability filter, no entry-point carve-out, and no test of whether a
//! binder is used. Each candidate narrowing leaves one of the three states above
//! standing, and the repair is a two-character edit in the case a narrowing
//! would keep.
//!
//! Type positions look through array nesting at any depth, because an array of a
//! type that has no layout has none either, and an array type is never a value
//! position on its own — it always sits inside one of the annotations above.
//! `[[Q i32'; 2]; 3]` is one finding on the annotation that carries it, not one
//! per layer.
//!
//! Reporting anchors on declarations and annotations, never on call sites. An
//! uncalled generic reaches exactly the same dead end, so the defect is a
//! property of the declaration; anchoring on uses would produce one message per
//! call for a single edit. Where a declaration is both — `fn g T'(p: Q T')` —
//! the two findings are separate, because the binder and the application are two
//! different things to remove.
//!
//! ## The spelling in the message
//!
//! Type applications are rendered from the arena, from the identifiers the
//! source wrote: the base name followed by each argument with its tick, `Q i32'
//! u8'`. The type checker's own rendering of the same node puts a tick on the
//! base as well (`Q' i32'`), which is a spelling the grammar does not accept,
//! and its peel through array nesting discards the arguments entirely — so
//! neither is usable for a message whose fix asks the author to edit what they
//! wrote.
//!
//! ## Why `spec` bodies are covered
//!
//! A `spec` function is lowered to a real WebAssembly function in proof mode, so
//! a generic one fails there exactly as a top-level one does. Compile mode does
//! not emit spec functions at all, which is why a generic spec function
//! *appears* to compile today: the function is dropped from the artifact, not
//! supported in it. The rule is therefore stated on source shape rather than on
//! a mode, and reports the same message whichever mode the program is compiled
//! in.
//!
//! ## What stays legal
//!
//! A non-generic declaration of every kind, including a struct whose name a
//! generic function's type parameter would have shadowed. A bare base name used
//! as a type (`fn g(p: Q)`), and an array of one.
//!
//! One position is outside the predicate, and it is deliberate:
//!
//! - **Function types** (`fn(i32) -> i32`), which have no value representation
//!   with or without a type parameter in them and are not this rule's subject.
//!
//! The declaration half of this rule is a gate on an unimplemented feature and
//! is deleted the day monomorphization lands
//! (<https://github.com/Inferara/inference/issues/76>). **The type-application
//! and expression-position halves survive that day** and must not be deleted
//! with it: no type declaration accepts type arguments whether or not functions
//! can be monomorphized, so `Q i32'` stays meaningless. The architecturally
//! correct home for that half is the type checker's own validation of a written
//! type; this rule is the gate until it moves there.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, IdentId, StmtId, TypeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Expr, Field, Location, Stmt, TypeNode};

use crate::errors::{AnalysisDiagnostic, GenericSite, LabeledDiagnostic};
use crate::rules::position::{
    PARAMETER_TYPE, RETURN_TYPE, STRUCT_FIELD_TYPE, VALUE, VARIABLE_TYPE,
};
use crate::walker;

crate::rule! {
    /// Generic code has no lowering and may not be written.
    #[id = "A051"]
    #[name = "Generic code not supported"]
    #[severity = error]
    pub struct GenericNotSupported;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            check_defs(arena, &source_file.module_path, &source_file.defs, &mut errors);
        }
        walker::walk_function_bodies(ctx, &mut |stmt_id, walk_ctx| {
            check_stmt(arena, &walk_ctx.module_path, stmt_id, &mut errors);
        });
        errors
    }
}

/// Checks the declaration surface of a file: type parameters and the signatures
/// of functions, methods and `external fn`s, struct fields, and module-scope
/// `const` declarations, recursing through `spec`.
fn check_defs(
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function {
                name,
                type_params,
                args,
                returns,
                ..
            } => {
                check_type_params(arena, module_path, &arena[*name].name, type_params, errors);
                check_signature(arena, module_path, args, *returns, errors);
            }
            Def::ExternFunction { args, returns, .. } => {
                check_signature(arena, module_path, args, *returns, errors);
            }
            Def::Struct {
                name,
                fields,
                methods,
                ..
            } => {
                check_struct_fields(arena, module_path, fields, errors);
                let struct_name = &arena[*name].name;
                for &method_id in methods {
                    if let Def::Function {
                        name: method_name,
                        type_params,
                        args,
                        returns,
                        ..
                    } = &arena[method_id].kind
                    {
                        // A method is named the way a call spells it, so the
                        // message points at a declaration the reader can find
                        // even when two structs declare the same method name.
                        let qualified = format!("{struct_name}::{}", arena[*method_name].name);
                        check_type_params(arena, module_path, &qualified, type_params, errors);
                        check_signature(arena, module_path, args, *returns, errors);
                    }
                }
            }
            // A module-scope `const` is the twin of the function-local one the
            // body walk checks. Its value is checked here too, because the body
            // walk reaches no expression of a module-scope declaration.
            Def::Constant { ty, value, .. } => {
                check_annotation(
                    arena,
                    module_path,
                    *ty,
                    VARIABLE_TYPE,
                    arena[def_id].location,
                    errors,
                );
                check_type_expressions(arena, module_path, *value, errors);
            }
            Def::Spec { defs, .. } => check_defs(arena, module_path, defs, errors),
            Def::Enum { .. } => {}
        }
    }
}

/// Reports one finding for a declaration that binds type parameters, carrying
/// every binder it declares.
///
/// One finding per declaration rather than one per binder: the binders are a
/// single list, removed together, and a reader repairing `fn pick T' U'` has one
/// edit to make. The caret sits on the first binder, which is the construct the
/// message is about.
fn check_type_params(
    arena: &AstArena,
    module_path: &[String],
    function: &str,
    type_params: &[IdentId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let Some(&first) = type_params.first() else {
        return;
    };
    push(
        errors,
        module_path,
        GenericSite::Declaration {
            function: function.to_string(),
            params: render_type_params(arena, type_params),
        },
        arena[first].location,
    );
}

/// Reports each field whose type is a type application, or an array of one.
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

/// Checks one statement of a function body: the declared type of a `let` or of a
/// function-local `const`, and every type application reachable from its
/// expressions.
fn check_stmt(
    arena: &AstArena,
    module_path: &[String],
    stmt_id: StmtId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    // The caret is the declaration, which is where the annotation stands. The
    // annotation is read from the arena rather than from the type the checker
    // recorded, because the recorded rendering is not a spelling the grammar
    // accepts and drops the type arguments the message quotes back.
    let declared = match &arena[stmt_id].kind {
        Stmt::VarDef { ty, .. } => Some(*ty),
        Stmt::ConstDef(def_id) => match &arena[*def_id].kind {
            Def::Constant { ty, .. } => Some(*ty),
            _ => None,
        },
        _ => None,
    };
    if let Some(ty) = declared {
        check_annotation(
            arena,
            module_path,
            ty,
            VARIABLE_TYPE,
            arena[stmt_id].location,
            errors,
        );
    }
    walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
        check_type_expressions(arena, module_path, expr_id, errors);
    });
}

/// Reports every type application standing in expression position within
/// `expr_id`, including `expr_id` itself.
///
/// The descent is what reaches a nested one: a statement walk yields only the
/// root expression of each statement, while `(Q i32');` puts the type inside a
/// parenthesization.
fn check_type_expressions(
    arena: &AstArena,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    walker::walk_expr(arena, expr_id, &mut |sub_id| {
        if let Expr::Type(ty) = &arena[sub_id].kind {
            check_annotation(
                arena,
                module_path,
                *ty,
                VALUE,
                arena[sub_id].location,
                errors,
            );
        }
    });
}

/// Reports `location` when the type annotation `ty` is a type application, or an
/// array of one at any depth.
fn check_annotation(
    arena: &AstArena,
    module_path: &[String],
    ty: TypeId,
    position: &'static str,
    location: Location,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    if let TypeNode::Generic { base, params } = &arena[innermost_element_type(arena, ty)].kind {
        push(
            errors,
            module_path,
            GenericSite::TypeApplication {
                rendered: render_application(arena, *base, params),
                position,
            },
            location,
        );
    }
}

/// The element type an array type ultimately holds, or `ty` itself when it is
/// not an array. `[[Q i32'; 2]; 3]` reaches the `Q i32'` node.
fn innermost_element_type(arena: &AstArena, ty: TypeId) -> TypeId {
    let mut current = ty;
    while let TypeNode::Array { element, .. } = &arena[current].kind {
        current = *element;
    }
    current
}

/// A type application as the source spells it: the base name, then each argument
/// with the tick that marks it, `Q i32' u8'`.
fn render_application(arena: &AstArena, base: IdentId, params: &[IdentId]) -> String {
    std::iter::once(arena[base].name.clone())
        .chain(params.iter().map(|p| format!("{}'", arena[*p].name)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The type parameters of a declaration as the source spells them, `T' U'`.
fn render_type_params(arena: &AstArena, type_params: &[IdentId]) -> String {
    type_params
        .iter()
        .map(|p| format!("{}'", arena[*p].name))
        .collect::<Vec<_>>()
        .join(" ")
}

fn push(
    errors: &mut Vec<LabeledDiagnostic>,
    module_path: &[String],
    site: GenericSite,
    location: Location,
) {
    errors.push(LabeledDiagnostic::new(
        module_path.to_vec(),
        AnalysisDiagnostic::GenericNotSupported { site, location },
    ));
}
