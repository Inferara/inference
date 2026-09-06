//! A050: a parameter of a defined function must be written with a name, or
//! with `_`.
//!
//! The grammar admits three ways to write a parameter: `name: T`, `_: T`, and a
//! bare `T`. On a function with a body the third one says strictly less than the
//! second and buys nothing for it.
//!
//! - **A parameter with no name can be neither read in the body nor labelled at
//!   a call site.** That is true of `_: T` as well, which is why it is not on
//!   its own an argument for rejecting the bare form — it is the fact that makes
//!   the two forms comparable at all.
//! - **Given that, the bare form is a second spelling of `_: T` that states
//!   less.** `_: T` is a deliberate declaration: this parameter is present, its
//!   type is part of the signature, and I am not reading it. A bare `T` states
//!   nothing, so the reader cannot tell a considered omission from a forgotten
//!   name.
//! - **It is the grammar's fallback arm**, which is what makes a forgotten type
//!   annotation misparse: `fn f(x)` is not a parameter named `x`, it is a
//!   parameter whose *type* is `x`. That surfaces today as an unknown-type
//!   error, which is the right diagnostic — but the form is what allows the
//!   shape to be built at all.
//!
//! Removing the weaker of two spellings for one concept is the direction this
//! crate has taken twice before: A033 for combined unary operators, A046 for the
//! detached minus.
//!
//! ## What decides the report
//!
//! One finding per parameter written as a bare positional type on a
//! `Def::Function` — a free function, a struct method, a `spec` function, or a
//! method on a struct declared inside a `spec`. The check is a declaration walk:
//! no bodies, no call sites, no type resolution beyond rendering the type for
//! the message.
//!
//! `index` counts the declared parameters from zero, with a `self` receiver
//! excluded, so `fn m(self, i32)` reports parameter 0. That is not the position
//! the argument occupies in the source text; it is the number the type checker
//! already uses when it talks about an argument, because the parameter lists
//! those messages index into are built with the receiver filtered out. Two
//! user-facing messages about one slot must not disagree about which slot it is,
//! and the caret points at the parameter either way.
//!
//! ## `external fn` is outside the rule
//!
//! An extern declares an ABI signature and has no body, so a positional type is
//! genuinely all there is to say about a parameter: there is no body to read it
//! in and nothing about the declaration is underspecified by leaving it unnamed.
//! Code generation and the linker both already read the form, and it is the
//! spelling the corpus uses.
//!
//! It is nonetheless the wrong form on an extern the linked body *writes*
//! through. `mut` is a field of a named parameter alone, so an unnamed one
//! cannot carry the write-set contract A047 checks — an extern that stores
//! through a parameter has to name it. That is a recommendation the rule does
//! not enforce, because an extern that only reads is a perfectly good use of the
//! bare form.
//!
//! ## What stays legal
//!
//! `_: T`, including repeats of it: `_` is not a name the declaration records —
//! the argument carries a type and nothing else — so there is no identifier for
//! a duplicate-name check to compare, and two of them cannot collide. A named
//! parameter, with or without `mut`. A `self` or `mut self` receiver, which is
//! not an unnamed parameter but a parameter that spells its type implicitly.
//!
//! Unlike the two rules it sits beside, this one is not a gate on an
//! unimplemented feature: `_: T` is implemented, and the bare form is rejected
//! because one spelling for the concept is better than two. It does not become
//! obsolete when some later feature lands.

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{ArgData, ArgKind, Def};
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic};

crate::rule! {
    /// A parameter of a defined function must be named, or written `_`.
    #[id = "A050"]
    #[name = "Unnamed parameter on a defined function"]
    #[severity = error]
    pub struct UnnamedParameter;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let mut errors = Vec::new();
        let arena = ctx.arena();
        for source_file in ctx.source_files() {
            scan_defs(arena, &source_file.module_path, &source_file.defs, &mut errors);
        }
        errors
    }
}

/// Walks the declarations of one scope, recursing through `spec` and into the
/// methods of every struct.
fn scan_defs(
    arena: &AstArena,
    module_path: &[String],
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { name, args, .. } => {
                report_unnamed(arena, module_path, &arena[*name].name, args, errors);
            }
            Def::Struct { name, methods, .. } => {
                let struct_name = &arena[*name].name;
                for &method_id in methods {
                    if let Def::Function {
                        name: method_name,
                        args,
                        ..
                    } = &arena[method_id].kind
                    {
                        // A method is named the way a call spells it, so the
                        // message points at a declaration the reader can find
                        // even when two structs declare the same method name.
                        let qualified = format!("{struct_name}::{}", arena[*method_name].name);
                        report_unnamed(arena, module_path, &qualified, args, errors);
                    }
                }
            }
            Def::Spec { defs, .. } => scan_defs(arena, module_path, defs, errors),
            // An extern has no body to read a parameter in, so a bare positional
            // type is a complete statement of its ABI.
            Def::ExternFunction { .. }
            | Def::Enum { .. }
            | Def::Constant { .. }
            | Def::TypeAlias { .. } => {}
        }
    }
}

/// Reports every bare positional parameter of one signature, carrying the index
/// it occupies among the declared parameters.
///
/// A `self` receiver is skipped rather than counted, so the index matches the
/// one the type checker's own argument messages use.
fn report_unnamed(
    arena: &AstArena,
    module_path: &[String],
    function: &str,
    args: &[ArgData],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let mut index = 0;
    for arg in args {
        if matches!(arg.kind, ArgKind::SelfRef { .. }) {
            continue;
        }
        if let ArgKind::TypeOnly(ty) = &arg.kind {
            errors.push(LabeledDiagnostic::new(
                module_path.to_vec(),
                AnalysisDiagnostic::UnnamedParameter {
                    function: function.to_string(),
                    index,
                    ty: render_type(&TypeInfo::from_type_id(arena, *ty).kind),
                    location: arg.location,
                },
            ));
        }
        index += 1;
    }
}

/// The type as the source spells it.
///
/// The message quotes this back inside the `_: {ty}` it recommends, so every
/// part of it has to be a name the author can actually write. A builtin uses its
/// source name, which is why `i32` and `bool` are not the checker's capitalized
/// `Display` renderings; an array is rebuilt from its element so the same holds
/// at every depth, since `Display` would descend into the capitalized form and
/// recommend `_: [Bool; 2]`. Everything else keeps `Display`, which renders a
/// struct or enum by its canonical key.
fn render_type(kind: &TypeInfoKind) -> String {
    if let Some(builtin) = kind.as_builtin_str() {
        return builtin.to_string();
    }
    match kind {
        TypeInfoKind::Array(elem, length) => {
            format!("[{}; {length}]", render_type(&elem.kind))
        }
        _ => kind.to_string(),
    }
}
