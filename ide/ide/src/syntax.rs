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

/// The local bindings in scope at `offset`: every `let` or local `const` that is
/// a direct statement of a block enclosing the offset — a block on the hit's
/// ancestor chain, or the covering node itself when the offset falls in a block's
/// own gap — and whose name ends at or before the offset.
///
/// Scoping is lexical: a binding in a sibling block that has already closed is
/// not in scope (its block is absent from the ancestor chain), and one declared
/// later in an enclosing block is not yet visible (its name ends after the
/// offset). A local `const` scopes exactly like a `let` — the type checker
/// registers it in statement order — so both are gated on the same name-end
/// bound. Inference forbids shadowing, so a name resolves to at most one binding
/// here. Sharing this walk keeps goto/hover and completions in agreement on what
/// is in scope.
#[must_use]
pub(crate) fn in_scope_locals(arena: &AstArena, hit: &NodeHit, offset: u32) -> Vec<StmtId> {
    let mut locals = Vec::new();
    for &node in hit.ancestors.iter().chain(std::iter::once(&hit.node)) {
        let NodeId::Block(block) = node else {
            continue;
        };
        for &stmt in &arena[block].stmts {
            let name_end = match &arena[stmt].kind {
                Stmt::VarDef { name, .. } => arena[*name].location.offset_end,
                Stmt::ConstDef(def) => arena[def_name_ident(arena, *def)].location.offset_end,
                _ => continue,
            };
            if name_end <= offset {
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

/// The source-root-relative path segments of each **plain** (namespace-binding)
/// `use` directive in `entry`: a file import `use a::b;` yields `["a", "b"]`.
///
/// Item imports (`use a::b::{c};`) and `from`-clause extern imports are excluded,
/// because neither binds a namespace: the first binds `c` bare, the second binds
/// an external symbol. Only a plain import makes its trailing segment usable as a
/// `::` qualifier, so only these paths anchor namespace resolution.
fn plain_import_paths(arena: &AstArena, entry: SourceFileId) -> Vec<Vec<String>> {
    arena[entry]
        .directives
        .iter()
        .filter_map(|directive| {
            let Directive::Use(use_directive) = directive;
            if use_directive.braced || use_directive.from.is_some() {
                return None;
            }
            Some(
                use_directive
                    .segments
                    .iter()
                    .map(|&segment| arena.ident_name(segment).to_string())
                    .collect(),
            )
        })
        .collect()
}

/// Resolves the `::`-qualifier `segments` typed before a completion cursor to the
/// module they name, trusting only **plain** namespace-binding imports.
///
/// A qualifier resolves two ways, both requiring a plain import so the result is
/// code the type checker accepts:
///
/// - by *binding*: its head names a plain import's trailing segment (`use a::b;`
///   binds `b`, so `b::…` resolves), and any further segments descend into
///   submodules;
/// - by *anchored full path*: the qualifier is itself a source-root path that a
///   plain import is a prefix of (`use a::b;` anchors `a::b::…`), so it is
///   addressable as written.
///
/// An item import `use a::b::{c};` binds `c` bare and no namespace, so it never
/// anchors a `::` qualifier — offering `b::…` through it would suggest code that
/// does not compile.
#[must_use]
pub(crate) fn resolve_plain_import_namespace(
    file: &FileAnalysis,
    entry: SourceFileId,
    segments: &[String],
) -> Option<SourceFileId> {
    let (head, rest) = segments.split_first()?;
    let plain = plain_import_paths(file.arena(), entry);
    for path in &plain {
        if path.last().map(String::as_str) == Some(head.as_str()) {
            let mut full = path.clone();
            full.extend(rest.iter().cloned());
            if let Some(sfid) = file.source_file_id(&full) {
                return Some(sfid);
            }
        }
    }
    if plain.iter().any(|path| segments.starts_with(path.as_slice())) {
        return file.source_file_id(segments);
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use inference_ast::arena::AstArena;
    use inference_ast::ids::{IdentId, NodeId, SourceFileId};
    use inference_ast::nodes::{Ident, Location, SimpleTypeKind, TypeData, TypeNode};
    use inference_ide_db::{FileAnalysis, RootDatabase};

    use super::{
        children_of, def_is_public, def_name_ident, find_def_by_name, find_method, method_has_self,
        resolve_qualified_module, type_children, walk_file,
    };
    use crate::test_utils::module_path;

    /// Analyzes `source` as a single entry document and hands back an owned clone
    /// of the merged arena plus its entry file id. Type checking never mutates the
    /// arena, so what the syntax helpers read here is exactly the parser's output.
    fn analyze(source: &str) -> (AstArena, SourceFileId) {
        let mut db = RootDatabase::default();
        let path = module_path("main");
        db.open_document(&path, source);
        let arena = db.analysis(&path).arena().clone();
        let entry = arena
            .source_file_ids()
            .next()
            .expect("the entry produces one source file");
        (arena, entry)
    }

    /// The absolute path of a nested module file: the last segment names the file
    /// (`<leaf>.inf`), the earlier segments its parent directories under the test
    /// root, so `["lib", "geom"]` is `<root>/lib/geom.inf`.
    fn nested_module_path(segments: &[&str]) -> PathBuf {
        let (leaf, dirs) = segments.split_last().expect("at least one segment");
        let mut path = module_path("main");
        path.pop();
        for dir in dirs {
            path.push(dir);
        }
        path.push(format!("{leaf}.inf"));
        path
    }

    /// The name of every identifier node a full pre-order walk of `file` visits.
    /// A descent arm that fails to push one of its children drops that child's
    /// name from this list, so asserting a name is present proves the arm reached
    /// it.
    fn walked_idents(arena: &AstArena, file: SourceFileId) -> Vec<String> {
        let mut names = Vec::new();
        walk_file(arena, file, &mut |node| {
            if let NodeId::Ident(id) = node {
                names.push(arena.ident_name(id).to_string());
            }
        });
        names
    }

    fn assert_visits(visited: &[String], expected: &[&str]) {
        for want in expected {
            assert!(
                visited.iter().any(|name| name == want),
                "the walk did not visit `{want}`; visited: {visited:?}"
            );
        }
    }

    /// The qualifier segments of the first `TypeNode::Qualified` reachable in the
    /// entry file — the shape `resolve_qualified_module` consumes.
    fn first_qualifier(file: &FileAnalysis, entry: SourceFileId) -> Vec<IdentId> {
        let arena = file.arena();
        let mut found: Option<Vec<IdentId>> = None;
        walk_file(arena, entry, &mut |node| {
            if let NodeId::Type(ty) = node
                && let TypeNode::Qualified { qualifier, .. } = &arena[ty].kind
                && found.is_none()
            {
                found = Some(qualifier.clone());
            }
        });
        found.expect("a qualified type in the entry file")
    }

    // walk_file / children_of descent

    #[test]
    fn walk_reaches_every_definition_kind_and_argument_form() {
        // One file exercising each `Def` arm plus the argument forms: a type-only
        // extern argument, an ignored argument, a `self` receiver, and named
        // arguments throughout.
        let source = "external fn ext_probe(WidgetArg) -> GadgetRet;\n\
struct StructProbe { field_p: i32; fn method_p(self) -> i32 { return self.field_p; } }\n\
enum ColorProbe { RedV, GreenV, BlueV }\n\
spec RulesProbe { fn law_probe() -> i32 { return 1; } }\n\
const LIMIT_PROBE: GaugeTy = REF_PROBE;\n\
type AliasProbe = TargetTy;\n\
fn with_ignored(_: IgnoredTy) -> i32 { return 1; }";
        let (arena, entry) = analyze(source);
        let idents = walked_idents(&arena, entry);
        assert_visits(
            &idents,
            &[
                "ext_probe",
                "WidgetArg",
                "GadgetRet", // extern fn: name, type-only arg, return
                "StructProbe",
                "field_p",
                "method_p", // struct: name, field, self-method
                "ColorProbe",
                "RedV",
                "GreenV",
                "BlueV", // enum: name + variants
                "RulesProbe",
                "law_probe", // spec: name + nested def
                "LIMIT_PROBE",
                "GaugeTy",
                "REF_PROBE", // constant: name, type, value
                "AliasProbe",
                "TargetTy",  // type alias: name + aliased type
                "IgnoredTy", // ignored-argument type
            ],
        );
    }

    #[test]
    fn walk_descends_into_assign_loop_if_typedef_and_constdef_statements() {
        // Each probe identifier appears in exactly one syntactic position, so its
        // presence pins the arm that had to descend to reach it.
        let source = "fn stmt_probes() -> i32 {\n\
assign_l = assign_r;\n\
loop loop_cond() { loop_body(); }\n\
loop { plain_loop_body(); break; }\n\
if if_cond() { then_probe(); } else { else_probe(); }\n\
if bare_cond() { bare_then(); }\n\
type LocalAlias = LocalTarget;\n\
const LOCAL_K: i32 = local_const_val;\n\
return 1;\n\
}";
        let (arena, entry) = analyze(source);
        let idents = walked_idents(&arena, entry);
        assert_visits(
            &idents,
            &[
                "assign_l",
                "assign_r", // assign: left + right
                "loop_cond",
                "loop_body",       // loop with condition + body
                "plain_loop_body", // loop without condition: body only
                "if_cond",
                "then_probe",
                "else_probe", // if/else: condition, then, else
                "bare_cond",
                "bare_then", // if without an else block
                "LocalAlias",
                "LocalTarget", // local type def: name + aliased type
                "LOCAL_K",
                "local_const_val", // local const def: name + value
            ],
        );
    }

    #[test]
    fn walk_descends_into_call_index_struct_array_and_type_expressions() {
        let source = "fn expr_probes() -> i32 {\n\
call_probe(arg_name: arg_value);\n\
idx_array[idx_index];\n\
let s: StructTy = StructTy { field_probe: field_value };\n\
let arr: [i32; 2] = [elem_a, elem_b];\n\
type_expr_probe i32';\n\
return 1;\n\
}";
        let (arena, entry) = analyze(source);
        let idents = walked_idents(&arena, entry);
        assert_visits(
            &idents,
            &[
                "call_probe",
                "arg_name",
                "arg_value", // call: callee, named-arg name, arg value
                "idx_array",
                "idx_index", // array index: array + index
                "StructTy",
                "field_probe",
                "field_value", // struct literal: name, field, value
                "elem_a",
                "elem_b", // array literal elements
                "type_expr_probe",
                "i32", // generic name in expr position (Expr::Type + generic)
            ],
        );
    }

    #[test]
    fn walk_descends_into_array_generic_function_qualified_and_custom_types() {
        // A function type appears both with a return (`fn() -> FnRetTy`) and
        // without (`fn()`), exercising the return arm's `Some` and `None` sides.
        let source = "fn type_probes(\n\
arr_p: [WidgetElem; 4],\n\
gen_p: GenBase i32',\n\
fn_ret_p: fn() -> FnRetTy,\n\
fn_noret_p: fn(),\n\
qual_p: qmod::qsub::QLeaf,\n\
custom_p: CustomTy,\n\
) -> i32 { return 1; }";
        let (arena, entry) = analyze(source);
        let idents = walked_idents(&arena, entry);
        assert_visits(
            &idents,
            &[
                "WidgetElem", // array type: element (its literal size has no ident)
                "GenBase",
                "i32",     // generic type: base + parameter
                "FnRetTy", // function type: return type
                "qmod",
                "qsub",
                "QLeaf",    // qualified type: qualifier segments + leaf
                "CustomTy", // custom (bare) type
            ],
        );

        // The array size is a bare literal with no identifier, so confirm the
        // Array arm pushed both the element type and the size expression by
        // inspecting the node's children directly.
        let array = arena
            .types
            .iter()
            .find_map(|(id, data)| matches!(data.kind, TypeNode::Array { .. }).then_some(id))
            .expect("an array type node");
        assert!(matches!(
            type_children(&arena, array).as_slice(),
            [NodeId::Type(_), NodeId::Expr(_)]
        ));
    }

    #[test]
    fn children_of_a_source_file_or_identifier_are_empty() {
        let (arena, entry) = analyze("fn f() -> i32 { return 1; }");
        assert!(children_of(&arena, NodeId::SourceFile(entry)).is_empty());
        let name = def_name_ident(&arena, arena[entry].defs[0]);
        assert!(children_of(&arena, NodeId::Ident(name)).is_empty());
    }

    #[test]
    fn type_children_of_a_qualified_name_yields_qualifier_then_name() {
        // The parser lowers `a::B` to `TypeNode::Qualified`, never
        // `TypeNode::QualifiedName`, so this arm is reachable only by building the
        // node directly.
        let mut arena = AstArena::default();
        let loc = Location::default();
        let qualifier = arena.idents.alloc(Ident {
            location: loc,
            name: "modx".to_string(),
        });
        let name = arena.idents.alloc(Ident {
            location: loc,
            name: "Leaf".to_string(),
        });
        let ty = arena.types.alloc(TypeData {
            location: loc,
            kind: TypeNode::QualifiedName { qualifier, name },
        });
        assert_eq!(
            type_children(&arena, ty),
            vec![NodeId::Ident(qualifier), NodeId::Ident(name)]
        );
    }

    #[test]
    fn type_children_of_a_function_type_yields_params_then_return() {
        // `fn(...)` always lowers to empty parameters (a parity quirk), so the
        // parameter-descent arm is reachable only by building the node directly.
        let mut arena = AstArena::default();
        let loc = Location::default();
        let p0 = arena.types.alloc(TypeData {
            location: loc,
            kind: TypeNode::Simple(SimpleTypeKind::I32),
        });
        let p1 = arena.types.alloc(TypeData {
            location: loc,
            kind: TypeNode::Simple(SimpleTypeKind::Bool),
        });
        let ret = arena.types.alloc(TypeData {
            location: loc,
            kind: TypeNode::Simple(SimpleTypeKind::U8),
        });
        let ty = arena.types.alloc(TypeData {
            location: loc,
            kind: TypeNode::Function {
                params: vec![p0, p1],
                ret: Some(ret),
            },
        });
        assert_eq!(
            type_children(&arena, ty),
            vec![NodeId::Type(p0), NodeId::Type(p1), NodeId::Type(ret)]
        );
    }

    // Small definition helpers

    #[test]
    fn def_name_ident_names_every_definition_kind() {
        let source = "fn fn_def() -> i32 { return 1; }\n\
external fn extern_def(i32) -> i32;\n\
struct struct_def { f: i32; }\n\
enum enum_def { Va }\n\
spec spec_def { fn nested_def() -> i32 { return 1; } }\n\
const const_def: i32 = 1;\n\
type type_def = i32;";
        let (arena, entry) = analyze(source);
        let names: Vec<&str> = arena[entry]
            .defs
            .iter()
            .map(|&def| arena.ident_name(def_name_ident(&arena, def)))
            .collect();
        assert_eq!(
            names,
            vec![
                "fn_def",
                "extern_def",
                "struct_def",
                "enum_def",
                "spec_def",
                "const_def",
                "type_def",
            ]
        );
    }

    #[test]
    fn def_is_public_reflects_visibility_across_definition_kinds() {
        let source = "pub fn pub_fn() -> i32 { return 1; }\n\
fn priv_fn() -> i32 { return 1; }\n\
pub struct PubStruct { f: i32; }\n\
struct PrivStruct { f: i32; }\n\
pub enum PubEnum { Va }\n\
enum PrivEnum { Vb }\n\
pub const PUB_C: i32 = 1;\n\
const PRIV_C: i32 = 1;\n\
pub type PubT = i32;\n\
type PrivT = i32;\n\
external fn extern_priv(i32) -> i32;\n\
spec spec_priv { fn spec_fn() -> i32 { return 1; } }";
        let (arena, entry) = analyze(source);
        let is_public = |name: &str| {
            let def =
                find_def_by_name(&arena, entry, name).unwrap_or_else(|| panic!("no def `{name}`"));
            def_is_public(&arena, def)
        };
        for name in ["pub_fn", "PubStruct", "PubEnum", "PUB_C", "PubT"] {
            assert!(is_public(name), "`{name}` is declared pub");
        }
        for name in [
            "priv_fn",
            "PrivStruct",
            "PrivEnum",
            "PRIV_C",
            "PrivT",
            "extern_priv",
            "spec_priv",
        ] {
            assert!(!is_public(name), "`{name}` is not pub");
        }
    }

    #[test]
    fn find_method_finds_struct_methods_and_declines_non_structs() {
        let source = "struct MethHost { fx: i32; fn get_fx(self) -> i32 { return self.fx; } }\n\
fn free_fn() -> i32 { return 1; }";
        let (arena, entry) = analyze(source);
        let host = find_def_by_name(&arena, entry, "MethHost").expect("struct present");
        let free = find_def_by_name(&arena, entry, "free_fn").expect("function present");
        assert!(
            find_method(&arena, host, "get_fx").is_some(),
            "the declared method resolves"
        );
        assert!(
            find_method(&arena, host, "absent").is_none(),
            "an unknown method name does not resolve"
        );
        assert!(
            find_method(&arena, free, "get_fx").is_none(),
            "a non-struct definition has no methods"
        );
    }

    #[test]
    fn method_has_self_distinguishes_instance_methods_from_the_rest() {
        let source = "struct SelfHost { fn inst_method(self) -> i32 { return 1; } fn assoc_method() -> i32 { return 2; } }";
        let (arena, entry) = analyze(source);
        let inst = find_def_by_name(&arena, entry, "inst_method").expect("instance method present");
        let assoc = find_def_by_name(&arena, entry, "assoc_method").expect("associated fn present");
        let host = find_def_by_name(&arena, entry, "SelfHost").expect("struct present");
        assert!(
            method_has_self(&arena, inst),
            "an instance method takes self"
        );
        assert!(
            !method_has_self(&arena, assoc),
            "an associated fn does not take self"
        );
        assert!(
            !method_has_self(&arena, host),
            "a non-function definition is not a self-method"
        );
    }

    // resolve_qualified_module

    #[test]
    fn resolve_qualified_module_follows_an_import_binding() {
        // `lib::T`: the head `lib` matches the `use lib;` binding and the full path
        // resolves — the common in-loop success.
        let mut db = RootDatabase::default();
        let lib = module_path("lib");
        let main = module_path("main");
        db.open_document(&lib, "pub struct T { pub v: i32; }");
        db.open_document(&main, "use lib;\nfn f(x: lib::T) -> i32 { return 0; }");
        let file = db.analysis(&main);
        let entry = file.source_file_id(&[]).expect("entry file");
        let qualifier = first_qualifier(file, entry);
        assert_eq!(
            resolve_qualified_module(file, entry, &qualifier),
            file.source_file_id(&["lib".to_string()])
        );
    }

    #[test]
    fn resolve_qualified_module_returns_none_when_an_extended_path_names_no_module() {
        // `lib::missing::T`: the head `lib` matches the import, but the extended
        // path names no file, so the in-loop lookup fails and the whole-path
        // fallback fails too.
        let mut db = RootDatabase::default();
        let lib = module_path("lib");
        let main = module_path("main");
        db.open_document(&lib, "pub struct T { pub v: i32; }");
        db.open_document(
            &main,
            "use lib;\nfn f(x: lib::missing::T) -> i32 { return 0; }",
        );
        let file = db.analysis(&main);
        let entry = file.source_file_id(&[]).expect("entry file");
        let qualifier = first_qualifier(file, entry);
        assert!(resolve_qualified_module(file, entry, &qualifier).is_none());
    }

    #[test]
    fn resolve_qualified_module_falls_back_to_a_full_source_root_path() {
        // `lib::geom::Point` written while only `use lib::geom;` is imported: the
        // import binds `geom`, so the head `lib` matches no binding, yet the whole
        // qualifier is a real module path — the fallback resolves it.
        let mut db = RootDatabase::default();
        let geom = nested_module_path(&["lib", "geom"]);
        let main = module_path("main");
        db.open_document(&geom, "pub struct Point { pub v: i32; }");
        db.open_document(
            &main,
            "use lib::geom;\nfn f(x: lib::geom::Point) -> i32 { return 0; }",
        );
        let file = db.analysis(&main);
        let entry = file.source_file_id(&[]).expect("entry file");
        let qualifier = first_qualifier(file, entry);
        assert_eq!(
            resolve_qualified_module(file, entry, &qualifier),
            file.source_file_id(&["lib".to_string(), "geom".to_string()])
        );
    }

    #[test]
    fn resolve_qualified_module_returns_none_for_an_unknown_qualifier() {
        // No import matches the head and the qualifier names no file: the import
        // loop never runs and the fallback declines.
        let mut db = RootDatabase::default();
        let main = module_path("main");
        db.open_document(&main, "fn f(x: ghost::T) -> i32 { return 0; }");
        let file = db.analysis(&main);
        let entry = file.source_file_id(&[]).expect("entry file");
        let qualifier = first_qualifier(file, entry);
        assert!(resolve_qualified_module(file, entry, &qualifier).is_none());
    }

    #[test]
    fn resolve_qualified_module_rejects_an_empty_qualifier() {
        // An empty qualifier has no head segment, so resolution declines at once.
        let mut db = RootDatabase::default();
        let main = module_path("main");
        db.open_document(&main, "fn f() -> i32 { return 0; }");
        let file = db.analysis(&main);
        let entry = file.source_file_id(&[]).expect("entry file");
        assert!(resolve_qualified_module(file, entry, &[]).is_none());
    }
}
