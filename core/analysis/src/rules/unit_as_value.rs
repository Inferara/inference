//! A049: the unit type has no value representation, so no value of it may be
//! introduced.
//!
//! `()` is the language's way of saying *there is nothing here*, and it is
//! legitimate in exactly that role: a function whose return type is `()` — or
//! `unit`, or omitted — returns nothing, and code generation implements that
//! directly by giving the WebAssembly function an empty result list. What has
//! no implementation is unit in a *carrier*. A unit value carries no
//! information, so it occupies no bytes and has no WebAssembly type: a
//! parameter declared `()` is given no argument slot to arrive in, a binding of
//! it has nothing to store, an array of it has no element size for frame layout
//! to compute, and a struct field of it has no offset that means anything.
//!
//! So the line this rule draws is between the absence of a value and a value of
//! nothing. The first is the point of the type; the second is a declaration
//! that cannot be honoured: most of those positions aborted code generation
//! outright, and a binding of it was worse still — the frame pre-scan gives
//! every non-i64 binding an i32 local, so the store into that local consumed a
//! value the unit initializer never pushed, and the malformed body would have
//! assembled silently behind the abort at the literal.
//!
//! ## What decides the report
//!
//! A finding is pushed for each of these, independently:
//!
//! - a `UnitLiteral` expression, *except* as the whole expression of a `return`
//!   or of an expression statement (below);
//! - the recorded type of a `let` binding or of a `const`, at function or
//!   module scope;
//! - a function, method, or `external fn` parameter — `Named`, `_: ()`, and a
//!   bare positional `()` alike;
//! - a struct field.
//!
//! Both spellings are covered without a special case: `()` lowers to a simple
//! unit type node and `unit` to a name the builtin table maps to the same kind,
//! so a single test on the resolved kind sees both. Array nesting is looked
//! through at any depth ([`walker::innermost_element`]), because an array of
//! unit has no element size at any nesting depth and an array type is never a
//! value position on its own.
//!
//! The message names `()` rather than both spellings, because that is the
//! canonical one; the alias is pinned by a test instead of by the text.
//!
//! ## The two exempt statement forms, and why they are load-bearing
//!
//! `return;` does not parse to "a return with no expression": the parser
//! synthesizes a `UnitLiteral` for the missing one, so the literal is present in
//! the tree of every bare `return` in the language. Rejecting `UnitLiteral`
//! unconditionally would therefore reject every void function ever written.
//!
//! The exemption is stated on the *root* of a `return` statement and of an
//! expression statement, after peeling parentheses — so `return;`, `return ();`,
//! `();`, `return (());` and `(());` are all silent, and the four spellings are
//! not made to differ by a pair of brackets. Peeling here is deliberately more
//! permissive than A046's decision not to peel: A046 removes a redundant
//! spelling, while this rule must not manufacture one.
//!
//! The exemption reaches the root and nothing below it. `f(())` is an argument
//! that has to arrive somewhere, so it is reported whether it stands in a
//! `return` or in a statement of its own.
//!
//! ## The return type is not covered
//!
//! `fn f() -> ()`, `fn f() -> unit` and an omitted return type are the one place
//! unit means something, and it is implemented. Covering the position would
//! reject every void function in every program.
//!
//! ## Why `spec` bodies are covered
//!
//! A `spec` function is lowered to a real WebAssembly function in proof mode,
//! so a unit carrier in a spec body reaches the same lowering a top-level one
//! does, and aborted there in the same way. Compile mode does not emit spec
//! functions at all, so nothing in a spec body reaches code generation there —
//! which is why the rule is stated on source shape rather than on a mode. One
//! message, whichever mode the program is compiled in.
//!
//! The proof translation's own rejection is not a substitute: it covers a unit
//! literal appearing in an *assertion term*, which is a different path from the
//! function body's WebAssembly lowering. Where both apply the program is
//! reported twice — the crate has no cross-rule suppression.
//!
//! ## Prior art this generalizes
//!
//! The linker already rejects exactly the parameter position, on the extern
//! path alone: lowering an `external fn` signature fails when a parameter's
//! value type comes back empty, and renders as "`unit` cannot appear as an
//! external function parameter". This rule generalizes that judgement from
//! `external fn` to every function and to the other carrier positions, and moves
//! it from link time to analysis. The link-time check stays where it is as
//! defence in depth for a caller that reaches the linker without running
//! analysis; the two must not contradict each other, which is why "has no value
//! representation" is the shared phrase.
//!
//! ## Reading the recorded type, not the annotation
//!
//! The binding half reads the type the checker recorded for the statement,
//! which is the resolved one, rather than the raw annotation node. Two facts
//! about the parser make this worth stating rather than simplifying away.
//! Lowering is total, so a `let` or an argument with no type child at all is
//! given a synthesized unit type node; and a `return` or an initializer with no
//! expression is given a synthesized unit *literal*. Neither placeholder is
//! reachable from a clean parse — the grammar requires `: type` on a `let`, on a
//! `const` and on a named argument, so a missing one is a parse error and the
//! compiler stops before analysis — but a rule that read the raw annotation
//! would be one grammar relaxation away from rejecting every binding in the
//! language, and would gain nothing for it.
//!
//! ## What stays legal
//!
//! Every void function and every way of returning from one. Type aliases,
//! whether the item form or the statement form: aliases are nominal in
//! Inference, so an alias names a type at which no value can be produced, and
//! every position that could produce one is covered above. The `self` receiver,
//! whose type is the enclosing struct.
//!
//! The rule is unconditional — it inspects source shape, not compilation mode.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, NodeId, StmtId, TypeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Expr, Field, Location, Stmt};
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};
use crate::rules::position::{PARAMETER_TYPE, STRUCT_FIELD_TYPE, VALUE, VARIABLE_TYPE};
use crate::walker;

crate::rule! {
    /// The unit type has no value representation and may not be used as one.
    #[id = "A049"]
    #[name = "Unit used as a value"]
    #[severity = error]
    pub struct UnitAsValue;
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

/// Whether a type is unit, or an array of unit at any nesting depth.
fn is_unit(kind: &TypeInfoKind) -> bool {
    matches!(walker::innermost_element(kind), TypeInfoKind::Unit)
}

/// Checks the declaration surface of a file: function, method, and `external fn`
/// parameters, struct fields, and module-scope `const` declarations, recursing
/// through `spec`. Return types are outside the rule.
fn check_defs(
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { args, .. } | Def::ExternFunction { args, .. } => {
                check_parameters(arena, module_path, args, errors);
            }
            Def::Struct { fields, methods, .. } => {
                check_struct_fields(arena, module_path, fields, errors);
                for &method_id in methods {
                    if let Def::Function { args, .. } = &arena[method_id].kind {
                        check_parameters(arena, module_path, args, errors);
                    }
                }
            }
            // A module-scope `const` is the twin of the function-local one the
            // body walk checks. It is checked here in its own right rather than
            // left to A032's blanket rejection of top-level `const`, which is a
            // gate on an unimplemented feature and not part of this closure;
            // both fire on such a declaration. The initializer is a value
            // position with no exempt form, so its literal is reported too.
            Def::Constant { ty, value, .. } => {
                check_annotation(
                    arena,
                    module_path,
                    *ty,
                    VARIABLE_TYPE,
                    arena[def_id].location,
                    errors,
                );
                check_unit_literals(arena, module_path, *value, errors);
            }
            Def::Spec { defs, .. } => check_defs(arena, module_path, defs, errors),
            Def::Enum { .. } | Def::TypeAlias { .. } => {}
        }
    }
}

/// Reports each field declared unit, or an array of it.
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

/// Checks every parameter of one signature.
///
/// A `self` receiver spells no type of its own — its type is the enclosing
/// struct — so it is never in scope here.
fn check_parameters(
    arena: &AstArena,
    module_path: &[String],
    args: &[ArgData],
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
}

/// Reports `location` when the type annotation `ty` is unit, or an array of it
/// at any depth.
fn check_annotation(
    arena: &AstArena,
    module_path: &[String],
    ty: TypeId,
    position: &'static str,
    location: Location,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    if is_unit(&TypeInfo::from_type_id(arena, ty).kind) {
        push(errors, module_path, position, location);
    }
}

/// Checks one statement of a function body: the declared type of a `let` or a
/// function-local `const`, and every unit literal reachable from it that is not
/// one of the two exempt statement roots.
fn check_stmt(
    ctx: &TypedContext,
    module_path: &[String],
    stmt_id: StmtId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let arena = ctx.arena();
    let stmt = &arena[stmt_id].kind;
    if matches!(stmt, Stmt::VarDef { .. } | Stmt::ConstDef(_))
        && let Some(type_info) = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id))
        && is_unit(&type_info.kind)
    {
        push(errors, module_path, VARIABLE_TYPE, arena[stmt_id].location);
    }
    // `return ();` and `();` are the two forms in which a unit literal states
    // that nothing is being produced, and `return;` is spelled with a
    // synthesized one. Their root is skipped whole: an exempt root is a literal,
    // so it has no sub-expressions that would be missed by not descending.
    if is_exempt_unit_statement(arena, stmt) {
        return;
    }
    walker::for_each_stmt_expr(stmt, arena, &mut |expr_id| {
        check_unit_literals(arena, module_path, expr_id, errors);
    });
}

/// Whether `stmt` is a `return` or an expression statement whose whole
/// expression is a unit literal, parentheses peeled.
fn is_exempt_unit_statement(arena: &AstArena, stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { expr } | Stmt::Expr(expr) => {
            matches!(&arena[peel_parens(arena, *expr)].kind, Expr::UnitLiteral)
        }
        _ => false,
    }
}

/// The expression inside any depth of parentheses.
fn peel_parens(arena: &AstArena, expr_id: ExprId) -> ExprId {
    let mut current = expr_id;
    while let Expr::Parenthesized { expr } = &arena[current].kind {
        current = *expr;
    }
    current
}

/// Reports every unit literal reachable from `expr_id`, including `expr_id`
/// itself.
fn check_unit_literals(
    arena: &AstArena,
    module_path: &[String],
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    walker::walk_expr(arena, expr_id, &mut |sub_id| {
        if matches!(&arena[sub_id].kind, Expr::UnitLiteral) {
            push(errors, module_path, VALUE, arena[sub_id].location);
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
        AnalysisDiagnostic::UnitAsValue { position, location },
    ));
}
