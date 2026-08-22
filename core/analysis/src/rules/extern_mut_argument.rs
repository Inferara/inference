//! A047: a compound argument at a `mut` `external fn` parameter must be rooted
//! at a `mut` binding.
//!
//! A linked external shares the caller's single linear memory. A compound
//! argument — a struct or an array — is therefore not copied across the call
//! boundary at all: the caller hands over a raw pointer into its own frame, and
//! the foreign body reads and writes the caller's bytes directly. `mut` on an
//! `external fn` parameter is the declaration that this may happen: it states
//! that the foreign body may store through the address that parameter denotes,
//! and the linker checks the claim against the merged body.
//!
//! That makes this the one place in the language where a write is invisible in
//! Inference source. Every other write to a binding is an assignment the reader
//! can see; this one lives in a `.wasm` the type checker never reads. So the
//! call site has to carry the statement instead: if the callee may write, the
//! argument's binding must be declared `mut`, exactly as it would have to be for
//! an assignment written out in full.
//!
//! ## What decides the report
//!
//! Four conditions, all required:
//!
//! 1. the callee resolves through [`ExternIndex`] to an `external fn`
//!    declaration;
//! 2. the declaration's parameter at that position is `mut`;
//! 3. that parameter's declared type passes a *region* — an array at any depth,
//!    or a name that resolves to a struct. An enum does not: it lowers to a bare
//!    `i32` tag, so the argument is a value and there is nothing for the callee
//!    to write into. Neither does a scalar, which is why the documented
//!    `external fn store_at(mut ptr: i32, ..)` idiom is untouched by this rule;
//! 4. the argument is not rooted at a `mut` binding.
//!
//! Condition 3 deliberately does not mirror codegen's own compound predicate:
//! that one is private to `inference-wasm-codegen`, which this crate does not
//! and must not depend on — the dependency runs the other way. It also carries
//! no field-less-struct carve-out, because A045 already rejects a field-less
//! struct as an `external fn` parameter type at any array depth, so no such
//! parameter survives to be classified here; a future relaxation of A045 must
//! revisit this.
//!
//! Argument-to-parameter correspondence is positional. `call_args[i]` is
//! declaration parameter `i` whether or not the call labels its arguments —
//! nothing in the pipeline reorders by label — and a call whose arity does not
//! match the declaration is a type error reported elsewhere, so a surplus
//! argument is skipped rather than matched against nothing.
//!
//! ## Resolution is scope-aware
//!
//! An `external fn` may be declared at a file's top level or inside a `spec`,
//! and the two may share a name. This rule therefore walks bodies with the
//! enclosing `spec` threaded through, exactly as A024 does, and asks
//! [`ExternIndex`] which declaration each call names in *its* scope. The shared
//! body walker cannot be used: its context carries the file but not the spec, so
//! every spec-inner extern call would be resolved against the top-level scope.
//! Extern calls inside a `spec` are legal when bound — A024 rejects only unbound
//! ones — so those calls are squarely in this rule's subject.
//!
//! ## The `const` root
//!
//! A `const` is a binding like any other here, and it is reported for the same
//! reason: a foreign store would change a value the source says is fixed. What
//! it cannot take is the repair. `mut` is a field of a `let`, a parameter, and a
//! receiver, and of none of them is it a field of a `const` — `const mut P` is a
//! parse error, so advice derived from "not declared `mut`" alone would send the
//! author to a declaration form that does not exist. The rule therefore reads
//! the *three-state* [`BindingMutability`] rather than a yes/no, and hands the
//! diagnostic an [`ImmutableArgumentRoot`] that selects the repair that is
//! actually available: copy the `const` into a `mut` binding and pass that.
//!
//! [`ImmutableArgumentRoot`]: crate::errors::ImmutableArgumentRoot
//!
//! ## The rootless argument
//!
//! An argument that is rooted at no binding at all — a struct or array literal,
//! the result of a call, a draw — is reported too: it is not a `mut` binding, and
//! silently accepting it would leave a hole in the closure. It is never the only
//! diagnostic such a program gets. A012 already rejects a compound literal as an
//! argument, A016 a compound-returning call in that position, and A014/A039 a
//! `@` — so this half of the rule is defense in depth, and no acceptance fixture
//! asserts it alone, because no program can produce it alone.
//!
//! ## What stays legal
//!
//! A `mut` local, a `mut` parameter, and a `mut self` receiver, passed whole or
//! projected (`p`, `p.inner`, `arr[i]`, `(p)`) — a projection of a `mut` binding
//! is memory the binding's own declaration already says may change. Every
//! argument to a parameter the declaration did not mark `mut`, which is the
//! common case and the shape every read-only external has.
//!
//! [`ExternIndex`]: inference_type_checker::ExternIndex

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, TypeId};
use inference_ast::nodes::{ArgKind, Def, Expr};
use inference_type_checker::BindingMutability;
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;

use crate::{
    errors::{AnalysisDiagnostic, ImmutableArgumentRoot, LabeledDiagnostic},
    walker,
};

crate::rule! {
    /// A compound argument at a `mut` `external fn` parameter must be rooted at
    /// a `mut` binding.
    #[id = "A047"]
    #[name = "Extern write through immutable argument"]
    #[severity = error]
    pub struct ExternMutArgument;
    fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
        let arena = ctx.arena();
        let mut errors = Vec::new();
        for source_file in ctx.source_files() {
            check_defs(
                arena,
                ctx,
                &source_file.module_path,
                None,
                &source_file.defs,
                &mut errors,
            );
        }
        errors
    }
}

/// What an argument expression bottoms out at.
enum ArgumentRoot {
    /// A projection of one binding, down to the identifier naming it. `expr` is
    /// that identifier's expression id, the key the recorded mutability is
    /// under; `name` is how the source spells the binding.
    Binding { expr: ExprId, name: String },
    /// A value the argument introduces itself, with no binding behind it,
    /// rendered as a short shape for the message.
    Temporary(String),
}

/// Walks the definitions of one scope, checking every call in their bodies
/// against the `external fn` declaration it resolves to.
///
/// A file's top level and a `spec` inside it are the only two places an
/// `external fn` can be declared, so a body's scope is fully described by the
/// file's `module_path` plus `spec` — `None` at the top level, the spec's name
/// inside one. Specs do not nest, so the recursion descends at most one level
/// and sibling specs stay isolated by construction.
fn check_defs(
    arena: &AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    spec: Option<&str>,
    defs: &[DefId],
    errors: &mut Vec<LabeledDiagnostic>,
) {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::Function { body, .. } => {
                check_function_body(arena, ctx, module_path, spec, *body, errors);
            }
            Def::Struct { methods, .. } => {
                for &method_id in methods {
                    if let Def::Function { body, .. } = &arena[method_id].kind {
                        check_function_body(arena, ctx, module_path, spec, *body, errors);
                    }
                }
            }
            Def::Spec { name, defs, .. } => {
                check_defs(
                    arena,
                    ctx,
                    module_path,
                    Some(arena[*name].name.as_str()),
                    defs,
                    errors,
                );
            }
            _ => {}
        }
    }
}

fn check_function_body(
    arena: &AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    spec: Option<&str>,
    body: BlockId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    walker::walk_block_stmts(arena, body, &mut |stmt_id| {
        walker::for_each_stmt_expr(&arena[stmt_id].kind, arena, &mut |expr_id| {
            walker::walk_expr(arena, expr_id, &mut |sub_id| {
                check_call(arena, ctx, module_path, spec, sub_id, errors);
            });
        });
    });
}

/// Reports each argument of one call that a `mut` compound parameter would be
/// allowed to write through, and whose binding does not say so.
fn check_call(
    arena: &AstArena,
    ctx: &TypedContext,
    module_path: &[String],
    spec: Option<&str>,
    expr_id: ExprId,
    errors: &mut Vec<LabeledDiagnostic>,
) {
    let Expr::FunctionCall { function, args, .. } = &arena[expr_id].kind else {
        return;
    };
    let Expr::Identifier(callee_id) = &arena[*function].kind else {
        return;
    };
    let callee = &arena[*callee_id].name;
    let Some(decl) = ctx.extern_index().lookup(module_path, spec, callee) else {
        return;
    };
    let Def::ExternFunction {
        name: decl_name,
        args: params,
        ..
    } = &arena[decl].kind
    else {
        return;
    };
    for (position, (_label, arg_id)) in args.iter().enumerate() {
        let Some(param) = params.get(position) else {
            continue;
        };
        let ArgKind::Named {
            name: param_name,
            ty,
            is_mut: true,
        } = &param.kind
        else {
            continue;
        };
        if !param_passes_a_region(ctx, *ty, module_path) {
            continue;
        }
        let (arg, root) = match argument_root(arena, *arg_id) {
            ArgumentRoot::Binding { expr, name } => match ctx.binding_mutability(expr) {
                Some(BindingMutability::Mutable) => continue,
                Some(BindingMutability::Constant) => (name, ImmutableArgumentRoot::Constant),
                Some(BindingMutability::Immutable) | None => (name, ImmutableArgumentRoot::Binding),
            },
            ArgumentRoot::Temporary(shape) => (shape, ImmutableArgumentRoot::Binding),
        };
        errors.push(LabeledDiagnostic::new(
            module_path.to_vec(),
            AnalysisDiagnostic::ExternWriteThroughImmutableArgument {
                arg,
                param: arena[*param_name].name.clone(),
                callee: arena[*decl_name].name.clone(),
                ty: TypeInfo::from_type_id(arena, *ty).to_string(),
                root,
                location: arena[*arg_id].location,
            },
        ));
    }
}

/// Whether a parameter declared with type `ty` receives a region of the caller's
/// memory rather than a value.
///
/// An array does, at any element type: it is passed as a pointer to its first
/// element. So does a name that resolves to a struct, through any of the three
/// spellings a raw annotation can use — a bare name, a `::`-qualified path, or
/// (should the annotation ever arrive resolved) a canonical key. An enum does
/// not, because it lowers to a bare `i32` tag; neither does any scalar.
///
/// Resolving a bare or qualified name against the *call site's* module path is
/// sound because [`ExternIndex`] only ever returns a declaration in the call
/// site's own file or a `spec` within it, so declaration and call site always
/// share a file.
///
/// [`ExternIndex`]: inference_type_checker::ExternIndex
fn param_passes_a_region(ctx: &TypedContext, ty: TypeId, module_path: &[String]) -> bool {
    match &TypeInfo::from_type_id(ctx.arena(), ty).kind {
        TypeInfoKind::Array(..) | TypeInfoKind::Struct(..) => true,
        TypeInfoKind::Custom(name) => ctx.lookup_struct_in(name, module_path).is_some(),
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => ctx
            .resolve_struct_by_qualified_path(
                &path
                    .split("::")
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                module_path,
            )
            .is_some(),
        TypeInfoKind::Unit
        | TypeInfoKind::Bool
        | TypeInfoKind::String
        | TypeInfoKind::Number(_)
        | TypeInfoKind::Enum(..)
        | TypeInfoKind::Generic(_)
        | TypeInfoKind::Function(_)
        | TypeInfoKind::Spec(_) => false,
    }
}

/// Reduces an argument to the binding whose memory it denotes, or reports that
/// it denotes none.
///
/// `p`, `p.inner`, `arr[i]`, `(p)` and any nesting of those all address bytes
/// inside `p`'s or `arr`'s own region, so the binding at the bottom is the one a
/// foreign store would reach. Every remaining shape is enumerated rather than
/// swept by a wildcard, so a new expression form must be classified here instead
/// of silently defaulting to a temporary — the direction that changes what the
/// message says.
fn argument_root(arena: &AstArena, expr_id: ExprId) -> ArgumentRoot {
    match &arena[expr_id].kind {
        Expr::Identifier(ident_id) => ArgumentRoot::Binding {
            expr: expr_id,
            name: arena[*ident_id].name.clone(),
        },
        Expr::MemberAccess { expr, .. }
        | Expr::ArrayIndexAccess { array: expr, .. }
        | Expr::Parenthesized { expr } => argument_root(arena, *expr),
        Expr::StructLiteral { name, .. } => {
            ArgumentRoot::Temporary(format!("{} {{ … }}", arena[*name].name))
        }
        Expr::ArrayLiteral { .. } => ArgumentRoot::Temporary("[…]".to_string()),
        Expr::FunctionCall { function, .. } => {
            ArgumentRoot::Temporary(match &arena[*function].kind {
                Expr::Identifier(ident_id) => format!("{}(…)", arena[*ident_id].name),
                _ => "…".to_string(),
            })
        }
        Expr::Uzumaki => ArgumentRoot::Temporary("@".to_string()),
        Expr::Binary { .. }
        | Expr::PrefixUnary { .. }
        | Expr::TypeMemberAccess { .. }
        | Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Type(_) => ArgumentRoot::Temporary("…".to_string()),
    }
}
