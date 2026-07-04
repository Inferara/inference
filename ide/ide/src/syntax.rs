//! Shared AST-navigation helpers scoped to a single source file.
//!
//! Feature queries need two things the lower layers do not expose directly: a
//! pre-order walk of every node in one file (for enumerating non-det blocks and
//! uzumaki expressions), and small primitives for reading a definition's name,
//! signature, and by-name lookup.
//!
//! # Why the child enumeration is reproduced here
//!
//! Byte offsets are per-file-local in the merged multi-file arena, so
//! `AstArena::find_source_file_for_node` cannot attribute a `Block` or `Expr` to
//! a file (it answers only for `Def`s and single-file arenas). Any per-file node
//! enumeration must therefore descend structurally from that file's own
//! definitions, exactly as `ide-db`'s hit-test does. That descent is currently
//! private to `ide-db`, so this module keeps its own copy. The exhaustive matches
//! make a *new* AST variant a compile error in both copies, but a semantic edit
//! to one would not error the other; the natural fix is to expose the descent
//! from `ide-db` (its canonical home) and delete this copy — a follow-up left out
//! of this crate's scope.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, IdentId, NodeId, SourceFileId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgData, ArgKind, Def, Directive, Expr, Location, Stmt, TypeNode, Visibility,
};
use inference_ide_db::{FileAnalysis, NodeHit, TextRange, file_defs};

/// Converts a compiler [`Location`] to the byte [`TextRange`] the feature API
/// speaks. Both types are foreign, so this free helper stands in for the
/// `From` impl the orphan rule forbids.
#[must_use]
pub(crate) fn text_range(location: Location) -> TextRange {
    TextRange {
        start: location.offset_start,
        end: location.offset_end,
    }
}

/// Visits every node in `file` in pre-order (a definition before its children),
/// scoped to that file's own definition tree so it never crosses into another
/// file's per-file-local offsets.
pub(crate) fn walk_file(arena: &AstArena, file: SourceFileId, visit: &mut impl FnMut(NodeId)) {
    for &def in &arena[file].defs {
        walk_node(arena, NodeId::Def(def), visit);
    }
}

fn walk_node(arena: &AstArena, node: NodeId, visit: &mut impl FnMut(NodeId)) {
    visit(node);
    for child in children_of(arena, node) {
        walk_node(arena, child, visit);
    }
}

/// The direct child nodes of `node`. Mirrors `ide-db`'s hit-test descent so a
/// walk reaches every node a position query could.
fn children_of(arena: &AstArena, node: NodeId) -> Vec<NodeId> {
    match node {
        NodeId::Def(id) => def_children(arena, id),
        NodeId::Stmt(id) => stmt_children(arena, id),
        NodeId::Expr(id) => expr_children(arena, id),
        NodeId::Type(id) => type_children(arena, id),
        NodeId::Block(id) => arena[id].stmts.iter().map(|&s| NodeId::Stmt(s)).collect(),
        NodeId::Ident(_) | NodeId::SourceFile(_) => Vec::new(),
    }
}

fn def_children(arena: &AstArena, id: DefId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        Def::Function {
            name,
            args,
            returns,
            body,
            ..
        } => {
            out.push(NodeId::Ident(*name));
            arg_children(args, &mut out);
            if let Some(ret) = returns {
                out.push(NodeId::Type(*ret));
            }
            out.push(NodeId::Block(*body));
        }
        Def::ExternFunction {
            name,
            args,
            returns,
            ..
        } => {
            out.push(NodeId::Ident(*name));
            arg_children(args, &mut out);
            if let Some(ret) = returns {
                out.push(NodeId::Type(*ret));
            }
        }
        Def::Struct {
            name,
            fields,
            methods,
            ..
        } => {
            out.push(NodeId::Ident(*name));
            for field in fields {
                out.push(NodeId::Ident(field.name));
                out.push(NodeId::Type(field.ty));
            }
            for &method in methods {
                out.push(NodeId::Def(method));
            }
        }
        Def::Enum { name, variants, .. } => {
            out.push(NodeId::Ident(*name));
            for &variant in variants {
                out.push(NodeId::Ident(variant));
            }
        }
        Def::Spec { name, defs, .. } => {
            out.push(NodeId::Ident(*name));
            for &nested in defs {
                out.push(NodeId::Def(nested));
            }
        }
        Def::Constant {
            name, ty, value, ..
        } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
            out.push(NodeId::Expr(*value));
        }
        Def::TypeAlias { name, ty, .. } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
        }
    }
    out
}

fn arg_children(args: &[ArgData], out: &mut Vec<NodeId>) {
    for arg in args {
        match &arg.kind {
            ArgKind::Named { name, ty, .. } => {
                out.push(NodeId::Ident(*name));
                out.push(NodeId::Type(*ty));
            }
            ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => out.push(NodeId::Type(*ty)),
            ArgKind::SelfRef { .. } => {}
        }
    }
}

fn stmt_children(arena: &AstArena, id: StmtId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        Stmt::Block(block) => out.push(NodeId::Block(*block)),
        Stmt::Expr(expr) | Stmt::Return { expr } | Stmt::Assert { expr } => {
            out.push(NodeId::Expr(*expr));
        }
        Stmt::Assign { left, right } => {
            out.push(NodeId::Expr(*left));
            out.push(NodeId::Expr(*right));
        }
        Stmt::Loop { condition, body } => {
            if let Some(condition) = condition {
                out.push(NodeId::Expr(*condition));
            }
            out.push(NodeId::Block(*body));
        }
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            out.push(NodeId::Expr(*condition));
            out.push(NodeId::Block(*then_block));
            if let Some(else_block) = else_block {
                out.push(NodeId::Block(*else_block));
            }
        }
        Stmt::VarDef {
            name, ty, value, ..
        } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
            if let Some(value) = value {
                out.push(NodeId::Expr(*value));
            }
        }
        Stmt::TypeDef { name, ty } => {
            out.push(NodeId::Ident(*name));
            out.push(NodeId::Type(*ty));
        }
        Stmt::ConstDef(def) => out.push(NodeId::Def(*def)),
        Stmt::Break => {}
    }
    out
}

fn expr_children(arena: &AstArena, id: ExprId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        Expr::Binary { left, right, .. } => {
            out.push(NodeId::Expr(*left));
            out.push(NodeId::Expr(*right));
        }
        Expr::PrefixUnary { expr, .. } | Expr::Parenthesized { expr } => {
            out.push(NodeId::Expr(*expr));
        }
        Expr::FunctionCall { function, args, .. } => {
            out.push(NodeId::Expr(*function));
            for (name, arg) in args {
                if let Some(name) = name {
                    out.push(NodeId::Ident(*name));
                }
                out.push(NodeId::Expr(*arg));
            }
        }
        Expr::ArrayIndexAccess { array, index } => {
            out.push(NodeId::Expr(*array));
            out.push(NodeId::Expr(*index));
        }
        Expr::MemberAccess { expr, name } | Expr::TypeMemberAccess { expr, name } => {
            out.push(NodeId::Expr(*expr));
            out.push(NodeId::Ident(*name));
        }
        Expr::StructLiteral { name, fields } => {
            out.push(NodeId::Ident(*name));
            for (field, value) in fields {
                out.push(NodeId::Ident(*field));
                out.push(NodeId::Expr(*value));
            }
        }
        Expr::Identifier(ident) => out.push(NodeId::Ident(*ident)),
        Expr::ArrayLiteral { elements } => {
            for &element in elements {
                out.push(NodeId::Expr(element));
            }
        }
        Expr::Type(ty) => out.push(NodeId::Type(*ty)),
        Expr::NumberLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::UnitLiteral
        | Expr::Uzumaki => {}
    }
    out
}

fn type_children(arena: &AstArena, id: TypeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    match &arena[id].kind {
        TypeNode::Simple(_) => {}
        TypeNode::Array { element, size } => {
            out.push(NodeId::Type(*element));
            out.push(NodeId::Expr(*size));
        }
        TypeNode::Generic { base, params } => {
            out.push(NodeId::Ident(*base));
            for &param in params {
                out.push(NodeId::Ident(param));
            }
        }
        TypeNode::Function { params, ret } => {
            for &param in params {
                out.push(NodeId::Type(param));
            }
            if let Some(ret) = ret {
                out.push(NodeId::Type(*ret));
            }
        }
        TypeNode::QualifiedName { qualifier, name } => {
            out.push(NodeId::Ident(*qualifier));
            out.push(NodeId::Ident(*name));
        }
        TypeNode::Qualified { qualifier, name } => {
            for &segment in qualifier {
                out.push(NodeId::Ident(segment));
            }
            out.push(NodeId::Ident(*name));
        }
        TypeNode::Custom(ident) => out.push(NodeId::Ident(*ident)),
    }
    out
}

/// The name identifier of a definition, whatever its kind.
#[must_use]
pub(crate) fn def_name_ident(arena: &AstArena, def: DefId) -> IdentId {
    match &arena[def].kind {
        Def::Function { name, .. }
        | Def::ExternFunction { name, .. }
        | Def::Struct { name, .. }
        | Def::Enum { name, .. }
        | Def::Spec { name, .. }
        | Def::Constant { name, .. }
        | Def::TypeAlias { name, .. } => *name,
    }
}

/// The first top-level or nested definition in `file` named `name`, or `None`.
///
/// The search covers struct methods and spec-nested defs (via `file_defs`), so a
/// method or a spec-inner function is found by its bare name. `file_defs` is a
/// pre-order flatten with no scope discriminator, so when a top-level and a
/// spec-inner definition share a name the first in source order wins; goto/hover
/// can then land on the wrong same-named definition. This is an IDE-convenience
/// mis-navigation only — never a compile or codegen path — and is accepted in v1.
#[must_use]
pub(crate) fn find_def_by_name(arena: &AstArena, file: SourceFileId, name: &str) -> Option<DefId> {
    file_defs(arena, file)
        .into_iter()
        .find(|&def| arena.def_name(def) == name)
}

/// A method named `name` defined directly on the struct `struct_def`, or `None`.
#[must_use]
pub(crate) fn find_method(arena: &AstArena, struct_def: DefId, name: &str) -> Option<DefId> {
    let Def::Struct { methods, .. } = &arena[struct_def].kind else {
        return None;
    };
    methods
        .iter()
        .copied()
        .find(|&method| arena.def_name(method) == name)
}

/// The one-line signature of a definition: its source up to the opening brace (or
/// the whole declaration when it has none), with surrounding whitespace trimmed.
///
/// `file` must be the definition's own file, because the location's offsets are
/// local to it.
#[must_use]
pub(crate) fn def_signature(arena: &AstArena, file: SourceFileId, def: DefId) -> Option<String> {
    let source = arena.node_source_in_file(file, arena[def].location)?;
    let head = source.split('{').next().unwrap_or(source);
    Some(head.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Whether a method takes a `self` receiver (an instance method, reachable via
/// `receiver.method()`), as opposed to an associated function.
#[must_use]
pub(crate) fn method_has_self(arena: &AstArena, method: DefId) -> bool {
    let Def::Function { args, .. } = &arena[method].kind else {
        return false;
    };
    args.iter()
        .any(|arg| matches!(arg.kind, ArgKind::SelfRef { .. }))
}

/// Whether a definition is `pub`, i.e. visible to importing files.
#[must_use]
pub(crate) fn def_is_public(arena: &AstArena, def: DefId) -> bool {
    let vis = match &arena[def].kind {
        Def::Function { vis, .. }
        | Def::ExternFunction { vis, .. }
        | Def::Struct { vis, .. }
        | Def::Enum { vis, .. }
        | Def::Spec { vis, .. }
        | Def::Constant { vis, .. }
        | Def::TypeAlias { vis, .. } => vis,
    };
    matches!(vis, Visibility::Public)
}

/// The innermost function definition enclosing a hit, or `None` when the hit is
/// not inside any function body (e.g. a top-level type annotation).
#[must_use]
pub(crate) fn enclosing_function(arena: &AstArena, hit: &NodeHit) -> Option<DefId> {
    hit.ancestors.iter().rev().find_map(|&node| match node {
        NodeId::Def(def) if matches!(arena[def].kind, Def::Function { .. }) => Some(def),
        _ => None,
    })
}

/// Whether `callee` is the function position of the `FunctionCall` named by
/// `grandparent` — the test that tells a call target apart from a plain value use
/// of the same identifier.
#[must_use]
pub(crate) fn is_call_callee(
    arena: &AstArena,
    grandparent: Option<NodeId>,
    callee: ExprId,
) -> bool {
    matches!(
        grandparent,
        Some(NodeId::Expr(call)) if matches!(
            &arena[call].kind,
            Expr::FunctionCall { function, .. } if *function == callee
        )
    )
}

/// The source-root-relative module paths the file `entry` imports with a `use`
/// directive, each as its `::`-split segments.
#[must_use]
pub(crate) fn imported_module_paths(arena: &AstArena, entry: SourceFileId) -> Vec<Vec<String>> {
    arena[entry]
        .directives
        .iter()
        .map(|directive| {
            let Directive::Use(use_directive) = directive;
            use_directive
                .segments
                .iter()
                .map(|&segment| arena.ident_name(segment).to_string())
                .collect()
        })
        .collect()
}

/// The `let` bindings in scope at `offset`: every `VarDef` that is a direct
/// statement of a block enclosing the offset — a block on the hit's ancestor
/// chain, or the covering node itself when the offset falls in a block's own gap
/// — and whose name ends at or before the offset.
///
/// Scoping is lexical: a binding in a sibling block that has already closed is
/// not in scope (its block is absent from the ancestor chain), and one declared
/// later in an enclosing block is not yet visible (its name ends after the
/// offset). Inference forbids shadowing, so a name resolves to at most one
/// binding here. Sharing this walk keeps goto/hover and completions in agreement
/// on what is in scope.
#[must_use]
pub(crate) fn in_scope_locals(arena: &AstArena, hit: &NodeHit, offset: u32) -> Vec<StmtId> {
    let mut locals = Vec::new();
    for &node in hit.ancestors.iter().chain(std::iter::once(&hit.node)) {
        let NodeId::Block(block) = node else {
            continue;
        };
        for &stmt in &arena[block].stmts {
            if let Stmt::VarDef { name, .. } = &arena[stmt].kind
                && arena[*name].location.offset_end <= offset
            {
                locals.push(stmt);
            }
        }
    }
    locals
}

/// Resolves the `::`-qualifier segments of a cross-module reference (the leading
/// `lib` of `lib::T`, or `lib::geom` of `lib::geom::Point`) to the
/// [`SourceFileId`] of the module they name, as imported by `entry`.
///
/// A qualifier's head names an imported module by its binding — the last segment
/// of its `use` path (`use lib::geom;` binds `geom`) — and any further segments
/// descend into submodules. A qualifier that is already a full
/// source-root-relative path resolves directly as a fallback.
#[must_use]
pub(crate) fn resolve_qualified_module(
    file: &FileAnalysis,
    entry: SourceFileId,
    qualifier: &[IdentId],
) -> Option<SourceFileId> {
    let arena = file.arena();
    let segments: Vec<String> = qualifier
        .iter()
        .map(|&id| arena.ident_name(id).to_string())
        .collect();
    let (head, rest) = segments.split_first()?;
    for import in imported_module_paths(arena, entry) {
        if import.last().map(String::as_str) == Some(head.as_str()) {
            let mut full = import.clone();
            full.extend(rest.iter().cloned());
            if let Some(sfid) = file.source_file_id(&full) {
                return Some(sfid);
            }
        }
    }
    file.source_file_id(&segments)
}
