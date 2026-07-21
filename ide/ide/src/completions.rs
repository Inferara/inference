//! Completion suggestions for a position in a document.
//!
//! Several contexts are distinguished, and each offers only names that compile
//! if accepted — accepting a suggestion must never insert code the type checker
//! rejects (issue #246):
//!
//! - Right after a `.` whose receiver has a known struct type, only that
//!   struct's fields and instance methods are offered; a private method is
//!   dropped when the struct is defined in another module, since the checker
//!   forbids calling it there.
//! - Right after a `<module>::` qualifier, that module's `pub` definitions are
//!   offered by their bare name — the one position where a bare member name is
//!   the form that compiles.
//! - Inside a comment or string literal, nothing is offered.
//! - Everywhere else the suggestions are keywords, the locals in scope, the
//!   document's own top-level definitions, and the imports it declares. A plain
//!   `use lib;` binds only the namespace, so its items are offered in their
//!   qualified `lib::item` form; a braced `use lib::{a, b};` binds `a` and `b`
//!   bare, so exactly those names are offered bare.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, NodeId, SourceFileId};
use inference_ast::nodes::{ArgKind, Def, Directive, Stmt};
use inference_ide_db::{FileAnalysis, NodeHit};
use inference_parser::SyntaxKind;
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use rustc_hash::FxHashSet;

use crate::syntax::{
    def_is_public, def_signature, enclosing_function, find_def_by_name, in_scope_locals,
    method_has_self, resolve_plain_import_namespace,
};
use crate::type_render::render_type;

/// The category of a completion, used by the editor to pick an icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompletionItemKind {
    Keyword,
    Function,
    Struct,
    Enum,
    Variable,
    Field,
    Method,
    Constant,
    Module,
    Snippet,
}

/// One completion suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
}

/// Every reserved word the lexer recognizes, offered in keyword position.
///
/// This mirrors `inference_parser`'s `SyntaxKind::from_keyword`; keeping the full
/// set (including the primitive type names) means the completion list never
/// drifts from what the parser actually treats as a keyword.
const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "spec", "struct", "enum", "const", "type", "external", "return", "loop",
    "if", "else", "assert", "break", "use", "from", "self", "pub", "assume", "forall", "exists",
    "unique", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "bool", "true", "false",
];

/// Computes completions for byte `offset` in the entry file.
#[must_use]
pub(crate) fn completions(file: &FileAnalysis, offset: u32) -> Vec<CompletionItem> {
    let arena = file.arena();
    let Some(entry) = file.source_file_id(&[]) else {
        return Vec::new();
    };
    let source = arena[entry].source.as_str();

    // A cursor inside a comment or string literal is not a code position; offer
    // nothing rather than popping the general list into prose an editor
    // auto-triggered on.
    if offset_in_comment_or_string(source, offset) {
        return Vec::new();
    }

    if let Some(items) = member_completions(file, entry, source, offset) {
        return items;
    }
    if let Some(items) = qualified_completions(file, entry, source, offset) {
        return items;
    }

    let mut items = Vec::new();
    push_keywords(&mut items);
    push_locals(file, entry, offset, &mut items);
    push_top_level_defs(file, entry, &mut items);
    push_imported(file, entry, &mut items);
    dedup(items)
}

fn push_keywords(items: &mut Vec<CompletionItem>) {
    for &keyword in KEYWORDS {
        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: CompletionItemKind::Keyword,
            detail: None,
        });
    }
}

/// The locals visible at `offset`: the enclosing function's params, and the `let`
/// bindings declared before the cursor (a later binding is not yet in scope).
fn push_locals(
    file: &FileAnalysis,
    entry: SourceFileId,
    offset: u32,
    items: &mut Vec<CompletionItem>,
) {
    let arena = file.arena();
    let Some(hit) = file.enclosing_hit(entry, offset) else {
        return;
    };
    let Some(function) = enclosing_function(arena, &hit) else {
        return;
    };
    if let Def::Function { args, .. } = &arena[function].kind {
        for arg in args {
            if let ArgKind::Named { name, ty, .. } = &arg.kind {
                items.push(CompletionItem {
                    label: arena.ident_name(*name).to_string(),
                    kind: CompletionItemKind::Variable,
                    detail: Some(render_type(&TypeInfo::from_type_id(arena, *ty))),
                });
            }
        }
    }
    for stmt in in_scope_locals(arena, &hit, offset) {
        if let Stmt::VarDef { name, ty, .. } = &arena[stmt].kind {
            items.push(CompletionItem {
                label: arena.ident_name(*name).to_string(),
                kind: CompletionItemKind::Variable,
                detail: Some(render_type(&TypeInfo::from_type_id(arena, *ty))),
            });
        }
    }
}

fn push_top_level_defs(file: &FileAnalysis, entry: SourceFileId, items: &mut Vec<CompletionItem>) {
    let arena = file.arena();
    for &def in &arena[entry].defs {
        items.push(def_completion(file, entry, def));
    }
}

/// The completions an entry file's `use` directives contribute, each in the form
/// that compiles when accepted (issue #246).
///
/// A plain `use lib;` (or `use lib::geom;`) binds only the trailing namespace, so
/// its `pub` items are offered *qualified* — label and inserted text `lib::item`,
/// since the LSP layer inserts the label verbatim — alongside the bare namespace
/// name itself, but only once the module resolves: a `use` naming a module that
/// does not exist contributes nothing, since its name would not compile. A braced
/// `use lib::{a, b};` binds `a` and `b` bare, so exactly the
/// braced names are offered bare (an item that names no `pub` definition in the
/// target is dropped rather than offered as code that will not resolve). A
/// `use … from <module>` clause imports an external symbol handled elsewhere and
/// contributes nothing here.
fn push_imported(file: &FileAnalysis, entry: SourceFileId, items: &mut Vec<CompletionItem>) {
    let arena = file.arena();
    for directive in &arena[entry].directives {
        let Directive::Use(use_dir) = directive;
        if use_dir.from.is_some() {
            continue;
        }
        let segments: Vec<String> = use_dir
            .segments
            .iter()
            .map(|&segment| arena.ident_name(segment).to_string())
            .collect();
        if use_dir.braced {
            let Some(sfid) = file.source_file_id(&segments) else {
                continue;
            };
            for &item in &use_dir.imported_types {
                let name = arena.ident_name(item);
                if let Some(def) = public_def_named(file, sfid, name) {
                    items.push(def_completion(file, sfid, def));
                }
            }
        } else {
            let Some(binding) = segments.last() else {
                continue;
            };
            let Some(sfid) = file.source_file_id(&segments) else {
                continue;
            };
            items.push(CompletionItem {
                label: binding.clone(),
                kind: CompletionItemKind::Module,
                detail: None,
            });
            for &def in &arena[sfid].defs {
                if def_is_public(arena, def) {
                    items.push(qualified_def_completion(file, sfid, def, binding));
                }
            }
        }
    }
}

/// The first top-level definition of `sfid` named `name` that is `pub`, or `None`.
/// A braced item import is completed only through this, so a name that is absent
/// or private in the target module is never offered.
fn public_def_named(file: &FileAnalysis, sfid: SourceFileId, name: &str) -> Option<DefId> {
    let arena = file.arena();
    arena[sfid]
        .defs
        .iter()
        .copied()
        .find(|&def| arena.def_name(def) == name && def_is_public(arena, def))
}

fn def_completion(file: &FileAnalysis, sfid: SourceFileId, def: DefId) -> CompletionItem {
    let arena = file.arena();
    CompletionItem {
        label: arena.def_name(def).to_string(),
        kind: def_kind(arena, def),
        detail: def_signature(arena, sfid, def),
    }
}

/// Like [`def_completion`] but labels the definition with a `<prefix>::` qualifier
/// — the form a plain-import item must be written in to compile.
fn qualified_def_completion(
    file: &FileAnalysis,
    sfid: SourceFileId,
    def: DefId,
    prefix: &str,
) -> CompletionItem {
    let arena = file.arena();
    CompletionItem {
        label: format!("{prefix}::{}", arena.def_name(def)),
        kind: def_kind(arena, def),
        detail: def_signature(arena, sfid, def),
    }
}

fn def_kind(arena: &AstArena, def: DefId) -> CompletionItemKind {
    match &arena[def].kind {
        Def::Function { .. } | Def::ExternFunction { .. } => CompletionItemKind::Function,
        Def::Enum { .. } => CompletionItemKind::Enum,
        Def::Constant { .. } => CompletionItemKind::Constant,
        Def::Spec { .. } => CompletionItemKind::Module,
        // A type alias names a type, so it shares the struct icon in the list.
        Def::Struct { .. } | Def::TypeAlias { .. } => CompletionItemKind::Struct,
    }
}

/// The completions after a `.`, or `None` when the cursor is not in member
/// position. An in-member cursor whose receiver is not a known struct yields an
/// empty list — no fields to offer — rather than falling back to the general set.
fn member_completions(
    file: &FileAnalysis,
    entry: SourceFileId,
    source: &str,
    offset: u32,
) -> Option<Vec<CompletionItem>> {
    let receiver_end = member_receiver_end(source, offset)?;
    let receiver = receiver_expr(file, entry, receiver_end)?;
    let Some(type_info) = file
        .typed_context()
        .get_node_typeinfo(NodeId::Expr(receiver))
    else {
        return Some(Vec::new());
    };
    let TypeInfoKind::Struct(bare, key) = &type_info.kind else {
        return Some(Vec::new());
    };
    Some(struct_members(file, entry, bare, key))
}

/// The byte offset at which the receiver expression ends (just before the `.`),
/// or `None` when `offset` is not in member position.
fn member_receiver_end(source: &str, offset: u32) -> Option<u32> {
    let bytes = source.as_bytes();
    let mut cursor = (offset as usize).min(bytes.len());
    while cursor > 0 && is_ident_byte(bytes[cursor - 1]) {
        cursor -= 1;
    }
    if cursor == 0 || bytes[cursor - 1] != b'.' {
        return None;
    }
    let mut end = cursor - 1;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    u32::try_from(end).ok()
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The outermost expression that ends exactly at `receiver_end` — the whole
/// receiver of the member access being typed.
fn receiver_expr(file: &FileAnalysis, entry: SourceFileId, receiver_end: u32) -> Option<ExprId> {
    let arena = file.arena();
    let hit = file.hit_test(entry, receiver_end - 1)?;
    outermost_first(&hit).find_map(|node| match node {
        NodeId::Expr(expr) if arena[expr].location.offset_end == receiver_end => Some(expr),
        _ => None,
    })
}

fn outermost_first(hit: &NodeHit) -> impl Iterator<Item = NodeId> + '_ {
    hit.ancestors
        .iter()
        .copied()
        .chain(std::iter::once(hit.node))
}

/// The fields and instance methods of the struct identified by `key`, accessible
/// from `entry`.
///
/// A private instance method is dropped when the struct is defined in another
/// module (`sfid != entry`): the checker rejects `receiver.method()` there, so
/// offering it would suggest code that does not compile (issue #246). A same-file
/// struct keeps its private methods, which are callable from within that file.
/// Fields carry no per-field visibility — they are accessible exactly when the
/// struct is — so a receiver typed here already grants access to every field.
fn struct_members(
    file: &FileAnalysis,
    entry: SourceFileId,
    bare: &str,
    key: &str,
) -> Vec<CompletionItem> {
    let arena = file.arena();
    let ctx = file.typed_context();
    let mut items = Vec::new();
    let Some(info) = ctx.lookup_struct(key) else {
        return items;
    };
    for field in &info.fields {
        items.push(CompletionItem {
            label: field.name.clone(),
            kind: CompletionItemKind::Field,
            detail: Some(render_type(&field.type_info)),
        });
    }
    let Some(module_path) = ctx.module_path_of_struct_key(key) else {
        return items;
    };
    let Some(sfid) = file.source_file_id(&module_path) else {
        return items;
    };
    let Some(struct_def) = find_def_by_name(arena, sfid, bare) else {
        return items;
    };
    let cross_module = sfid != entry;
    if let Def::Struct { methods, .. } = &arena[struct_def].kind {
        for &method in methods {
            if method_has_self(arena, method) && (!cross_module || def_is_public(arena, method)) {
                items.push(CompletionItem {
                    label: arena.def_name(method).to_string(),
                    kind: CompletionItemKind::Method,
                    detail: def_signature(arena, sfid, method),
                });
            }
        }
    }
    items
}

/// The completions after a `<module>::` qualifier, or `None` when the cursor is
/// not in a qualified position.
///
/// The target module's `pub` definitions are offered by their bare name — the one
/// position where a bare member name is exactly what compiles. When the qualifier
/// names no plain-imported module (an item import binds bare names, not a
/// namespace; a struct or enum qualifier is not a module) the result is an empty
/// list rather than `None`: a `::` position must never fall back to the general
/// keyword-and-local list, which does not compile after `::`.
fn qualified_completions(
    file: &FileAnalysis,
    entry: SourceFileId,
    source: &str,
    offset: u32,
) -> Option<Vec<CompletionItem>> {
    let segments = qualifier_before(source, offset)?;
    let Some(sfid) = resolve_plain_import_namespace(file, entry, &segments) else {
        return Some(Vec::new());
    };
    let arena = file.arena();
    let items = arena[sfid]
        .defs
        .iter()
        .filter(|&&def| def_is_public(arena, def))
        .map(|&def| def_completion(file, sfid, def))
        .collect();
    Some(items)
}

/// The `::`-separated qualifier segments immediately before `offset`, or `None`
/// when the cursor is not after a `<qualifier>::` (optionally followed by a
/// partial name being typed).
///
/// `lib::` and `lib::exp` both yield `["lib"]`; `lib::geom::Point` yields
/// `["lib", "geom"]`. A trailing partial name is skipped so the module is offered
/// while its member name is still being typed, and the editor filters by the
/// prefix. A `::` not preceded by an identifier segment (`::x`) yields `None`.
fn qualifier_before(source: &str, offset: u32) -> Option<Vec<String>> {
    let bytes = source.as_bytes();
    let mut cursor = (offset as usize).min(bytes.len());
    while cursor > 0 && is_ident_byte(bytes[cursor - 1]) {
        cursor -= 1;
    }
    if cursor < 2 || bytes[cursor - 1] != b':' || bytes[cursor - 2] != b':' {
        return None;
    }
    cursor -= 2;
    let mut segments = Vec::new();
    loop {
        let end = cursor;
        while cursor > 0 && is_ident_byte(bytes[cursor - 1]) {
            cursor -= 1;
        }
        if cursor == end {
            return None;
        }
        segments.push(std::str::from_utf8(&bytes[cursor..end]).ok()?.to_string());
        if cursor >= 2 && bytes[cursor - 1] == b':' && bytes[cursor - 2] == b':' {
            cursor -= 2;
            continue;
        }
        break;
    }
    segments.reverse();
    Some(segments)
}

/// Whether `offset` falls inside a comment or string literal, where a completion
/// would land in prose rather than code.
///
/// The lexer's token spans decide this, so the boundaries are exact. A comment
/// suppresses `(start, end]` — the whole body up to and including its end, since a
/// comment reaches to the line's end. A string suppresses `(start, end)` — its
/// interior only, so the cursor at the opening quote (still code) or just past the
/// closing quote (code again) is not suppressed. An unterminated string lexes as
/// an `Error` token beginning with `"`; its interior is suppressed the same way, so
/// suggestions stay quiet while the literal is still open.
fn offset_in_comment_or_string(source: &str, offset: u32) -> bool {
    for token in inference_parser::tokenize(source) {
        let start = token.loc.offset_start;
        let end = token.loc.offset_end;
        // Tokens are ordered by ascending start and every suppressing case needs
        // `start < offset`, so nothing past here can match.
        if start >= offset {
            break;
        }
        match token.kind {
            SyntaxKind::Comment | SyntaxKind::DocComment if offset <= end => {
                return true;
            }
            SyntaxKind::String if offset < end => return true,
            SyntaxKind::Error
                if offset <= end && source.as_bytes().get(start as usize) == Some(&b'"') =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Removes exact duplicates while preserving first-seen order (a local and a
/// same-named top-level def keep distinct entries by kind).
fn dedup(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let mut seen: FxHashSet<(String, CompletionItemKind)> = FxHashSet::default();
    items
        .into_iter()
        .filter(|item| seen.insert((item.label.clone(), item.kind)))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]

    use super::{CompletionItem, CompletionItemKind};
    use crate::test_utils::{after, at, project, single, with_lib};

    fn complete(source: &str, offset: u32) -> Vec<CompletionItem> {
        let (host, path) = single(source);
        host.analysis().completions(&path, offset)
    }

    fn has(items: &[CompletionItem], label: &str, kind: CompletionItemKind) -> bool {
        items
            .iter()
            .any(|item| item.label == label && item.kind == kind)
    }

    /// Whether any offered item carries `label`, regardless of kind — the test of
    /// "would accepting this insert this exact text".
    fn has_label(items: &[CompletionItem], label: &str) -> bool {
        items.iter().any(|item| item.label == label)
    }

    #[test]
    fn top_level_context_offers_keywords_and_definitions() {
        let source = "struct Widget { w: i32; }\nfn compute() -> i32 { return 1; }";
        let items = complete(source, 0);
        assert!(has(&items, "fn", CompletionItemKind::Keyword));
        assert!(has(&items, "forall", CompletionItemKind::Keyword));
        assert!(has(&items, "Widget", CompletionItemKind::Struct));
        assert!(has(&items, "compute", CompletionItemKind::Function));
    }

    #[test]
    fn locals_declared_before_the_cursor_are_offered_later_ones_are_not() {
        let source = "fn f() -> i32 { let early: i32 = 1; let later: i32 = 2; return early; }";
        let items = complete(source, at(source, "let later"));
        assert!(has(&items, "early", CompletionItemKind::Variable));
        assert!(
            !items.iter().any(|item| item.label == "later"),
            "a not-yet-declared local is out of scope"
        );
        // The enclosing function and keywords are still available.
        assert!(has(&items, "f", CompletionItemKind::Function));
        assert!(has(&items, "let", CompletionItemKind::Keyword));
    }

    #[test]
    fn params_are_offered_as_locals() {
        let source = "fn f(seed: i32) -> i32 { return 0; }";
        let items = complete(source, at(source, "return"));
        assert!(has(&items, "seed", CompletionItemKind::Variable));
    }

    #[test]
    fn a_local_from_a_closed_sibling_block_is_not_offered() {
        // Valid code (zero diagnostics): `inner`'s `if` block closes before the
        // cursor, so it is out of scope and must not be offered — accepting it
        // would produce an undeclared-variable error.
        let source = "fn f(c: bool) -> i32 {\n\
    if c {\n\
        let inner: i32 = 1;\n\
        assert(inner > 0);\n\
    }\n\
    let z: i32 = 0;\n\
    return z;\n\
}";
        let items = complete(source, at(source, "z;"));
        assert!(
            has(&items, "z", CompletionItemKind::Variable),
            "the in-scope local is offered"
        );
        assert!(
            !items.iter().any(|item| item.label == "inner"),
            "a local from an already-closed sibling block is out of scope: {items:?}"
        );
    }

    #[test]
    fn completions_after_the_closing_brace_do_not_leak_function_scope() {
        // The cursor is one byte past `}`, at top-level file scope. The one-byte
        // fallback must not pull it back inside the function and offer that
        // function's params and locals, none of which are usable here.
        let source =
            "fn f(secret_param: i32) -> i32 { let secret_local: i32 = 1; return secret_local; }\n";
        let brace = source.rfind('}').expect("a closing brace") as u32;
        let items = complete(source, brace + 1);
        assert!(
            !items
                .iter()
                .any(|item| item.kind == CompletionItemKind::Variable),
            "no function-scoped names leak at top level after `}}`: {items:?}"
        );
        // Top-level definitions are still offered at this position.
        assert!(has(&items, "f", CompletionItemKind::Function));
    }

    #[test]
    fn after_dot_on_a_struct_offers_only_fields_and_methods() {
        let source = "struct P { x: i32; fn get(self) -> i32 { return self.x; } }\n\
fn m(p: P) -> i32 { return p.; }";
        let items = complete(source, at(source, "p.;") + "p.".len() as u32);
        let mut labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        labels.sort_unstable();
        assert_eq!(labels, vec!["get", "x"]);
        assert!(has(&items, "x", CompletionItemKind::Field));
        assert!(has(&items, "get", CompletionItemKind::Method));
        assert!(
            items
                .iter()
                .all(|item| item.kind != CompletionItemKind::Keyword),
            "member context excludes keywords"
        );
    }

    #[test]
    fn after_dot_on_a_scalar_is_empty() {
        let source = "fn m() -> i32 { let n: i32 = 1; return n.; }";
        let items = complete(source, at(source, "n.;") + "n.".len() as u32);
        assert!(
            items.is_empty(),
            "a scalar receiver has no members: {items:?}"
        );
    }

    #[test]
    fn after_dot_offers_instance_methods_but_not_associated_functions() {
        // `inst` takes `self` (callable as `p.inst()`); `make` does not, so it is
        // reached as `P::make()` and must not appear after `.`.
        let source = "struct P { x: i32; \
fn inst(self) -> i32 { return self.x; } \
fn make() -> i32 { return 1; } }\n\
fn m(p: P) -> i32 { return p.; }";
        let items = complete(source, at(source, "p.;") + "p.".len() as u32);
        assert!(has(&items, "inst", CompletionItemKind::Method));
        assert!(
            !items.iter().any(|item| item.label == "make"),
            "an associated function is not offered after `.`: {items:?}"
        );
    }

    // --- plain module import: `use lib;` binds only the namespace ---

    #[test]
    fn a_plain_import_offers_the_module_and_its_defs_qualified_not_bare() {
        // `use lib;` binds the namespace `lib`, so `exported()` fails to compile
        // while `lib::exported()` works. The general list must therefore offer the
        // qualified `lib::exported` (label = inserted text) and never bare
        // `exported`, which the checker rejects (issue #246).
        let entry = "use lib;\nfn main() -> i32 { return 0; }";
        let lib = "pub fn exported() -> i32 { return 1; }";
        let (host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, at(entry, "return 0"));
        assert!(has(&items, "lib", CompletionItemKind::Module));
        assert!(has(&items, "lib::exported", CompletionItemKind::Function));
        assert!(
            !has_label(&items, "exported"),
            "the bare name does not compile under a plain import: {items:?}"
        );
    }

    #[test]
    fn a_plain_import_qualifies_with_its_trailing_namespace_segment() {
        // `use lib::geom;` binds the namespace `geom` (its last segment), so items
        // are written `geom::Point`, not `lib::geom::Point` and not bare `Point`.
        let entry = "use lib::geom;\nfn main() -> i32 { return 0; }";
        let geom = "pub struct Point { x: i32; }";
        let (host, path) = project(&[
            (&["main"], entry),
            (&["lib", "geom"], geom),
        ]);
        let items = host.analysis().completions(&path, at(entry, "return 0"));
        assert!(has(&items, "geom", CompletionItemKind::Module));
        assert!(has(&items, "geom::Point", CompletionItemKind::Struct));
        assert!(
            !has_label(&items, "Point") && !has_label(&items, "lib::geom::Point"),
            "only the binding-qualified form is offered: {items:?}"
        );
    }

    #[test]
    fn a_plain_import_does_not_offer_private_defs() {
        let entry = "use lib;\nfn main() -> i32 { return 0; }";
        let lib = "pub fn shown() -> i32 { return 1; }\nfn hidden() -> i32 { return 2; }";
        let (host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, at(entry, "return 0"));
        assert!(has(&items, "lib::shown", CompletionItemKind::Function));
        assert!(
            !has_label(&items, "lib::hidden") && !has_label(&items, "hidden"),
            "a private def is not importable, so it is not offered: {items:?}"
        );
    }

    #[test]
    fn a_plain_import_of_a_nonexistent_module_offers_nothing() {
        // `use ghost;` names a module that is not on disk, so the namespace never
        // resolves. Neither its bare name nor any qualified item may be offered —
        // accepting `ghost` would insert a name the checker rejects (issue #246).
        let source = "use ghost;\nfn main() -> i32 { return 0; }";
        let items = complete(source, at(source, "return 0"));
        assert!(
            !has_label(&items, "ghost"),
            "a nonexistent module contributes no completion: {items:?}"
        );
    }

    // --- braced item import: `use lib::{a, b};` binds exactly a and b bare ---

    #[test]
    fn a_braced_import_offers_only_the_braced_names_bare() {
        // `use arith::{add};` binds `add` bare; `sub` is public in arith but not
        // braced, so it must not be offered, and no `arith` namespace is bound.
        let entry = "use arith::{add};\nfn main() -> i32 { return 0; }";
        let arith = "pub fn add() -> i32 { return 1; }\npub fn sub() -> i32 { return 2; }";
        let (host, path) = project(&[(&["main"], entry), (&["arith"], arith)]);
        let items = host.analysis().completions(&path, at(entry, "return 0"));
        assert!(has(&items, "add", CompletionItemKind::Function));
        assert!(
            !has_label(&items, "sub"),
            "a public-but-unbraced def is not bound bare: {items:?}"
        );
        assert!(
            !has_label(&items, "arith") && !has_label(&items, "arith::add"),
            "an item import binds no namespace: {items:?}"
        );
    }

    #[test]
    fn a_braced_import_skips_names_that_are_absent_or_private() {
        // Only `good` resolves to a public def; `absent` names nothing and `secret`
        // is private, so neither is offered (accepting either would not compile).
        let entry = "use lib::{good, absent, secret};\nfn main() -> i32 { return 0; }";
        let lib = "pub fn good() -> i32 { return 1; }\nfn secret() -> i32 { return 2; }";
        let (host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, at(entry, "return 0"));
        assert!(has(&items, "good", CompletionItemKind::Function));
        assert!(!has_label(&items, "absent"), "a missing item is not offered");
        assert!(
            !has_label(&items, "secret"),
            "a private item is not offered: {items:?}"
        );
    }

    // --- `::`-qualified context: bare pub defs of the named module ---

    #[test]
    fn after_a_module_qualifier_offers_its_pub_defs_bare() {
        let entry = "use lib;\nfn main() -> i32 { return lib::; }";
        let lib = "pub fn shown() -> i32 { return 1; }\nfn hidden() -> i32 { return 2; }";
        let (host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, after(entry, "lib::"));
        assert!(
            has(&items, "shown", CompletionItemKind::Function),
            "the module's pub def is offered bare: {items:?}"
        );
        assert!(
            !has_label(&items, "hidden"),
            "a private def is not offered even after `::`: {items:?}"
        );
        assert!(
            !has_label(&items, "fn") && !has_label(&items, "main"),
            "keywords and locals are wrong after `::`: {items:?}"
        );
    }

    #[test]
    fn after_a_nested_module_qualifier_binding_offers_its_defs_bare() {
        // `use lib::geom;` binds `geom`; both `geom::` and the full `lib::geom::`
        // resolve to the same module.
        let entry = "use lib::geom;\nfn f() -> i32 { return 0; }\nfn g() -> i32 { return 0; }";
        let geom = "pub struct Point { x: i32; }";
        let (mut host, path) = project(&[(&["main"], entry), (&["lib", "geom"], geom)]);

        let mut binding = entry.to_string();
        binding.push_str("\nfn h() -> i32 { return geom::; }");
        host.change_document(&path, binding.clone());
        let items = host.analysis().completions(&path, after(&binding, "geom::"));
        assert!(
            has(&items, "Point", CompletionItemKind::Struct),
            "the nested module's def via its binding: {items:?}"
        );

        let mut full = entry.to_string();
        full.push_str("\nfn h() -> i32 { return lib::geom::; }");
        host.change_document(&path, full.clone());
        let items = host.analysis().completions(&path, after(&full, "lib::geom::"));
        assert!(
            has(&items, "Point", CompletionItemKind::Struct),
            "the nested module's def via its full path: {items:?}"
        );
    }

    #[test]
    fn a_partial_name_after_a_qualifier_still_offers_the_module_defs() {
        let entry = "use lib;\nfn main() -> i32 { return lib::sho; }";
        let lib = "pub fn shown() -> i32 { return 1; }";
        let (host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, after(entry, "lib::sho"));
        assert!(
            has(&items, "shown", CompletionItemKind::Function),
            "the module resolves while the member name is still being typed: {items:?}"
        );
    }

    #[test]
    fn an_item_import_does_not_make_its_module_a_qualifier() {
        // `use arith::{add};` binds `add` bare but no `arith` namespace, so
        // `arith::` names nothing importable — it must offer nothing, never
        // arith's defs (which `arith::x` could not call).
        let entry = "use arith::{add};\nfn main() -> i32 { return arith::; }";
        let arith = "pub fn add() -> i32 { return 1; }\npub fn sub() -> i32 { return 2; }";
        let (host, path) = project(&[(&["main"], entry), (&["arith"], arith)]);
        let items = host.analysis().completions(&path, after(entry, "arith::"));
        assert!(
            items.is_empty(),
            "an item-import module is not a `::` namespace: {items:?}"
        );
    }

    #[test]
    fn an_unknown_qualifier_offers_nothing_rather_than_the_general_list() {
        let source = "fn main() -> i32 { return ghost::; }";
        let items = complete(source, after(source, "ghost::"));
        assert!(
            items.is_empty(),
            "a `::` position never falls back to keywords/locals: {items:?}"
        );
    }

    // --- cross-module method visibility after `.` ---

    #[test]
    fn a_cross_module_receiver_hides_private_methods() {
        // `p: lib::P` from another module: `p.shown()` compiles but `p.hidden()`
        // is rejected as a private method, so only `shown` is offered. Fields are
        // accessible whenever the struct is, so `x` stays.
        let entry = "use lib;\nfn m(p: lib::P) -> i32 { return p.; }";
        let lib = "pub struct P { x: i32; \
pub fn shown(self) -> i32 { return self.x; } \
fn hidden(self) -> i32 { return self.x; } }";
        let (host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, after(entry, "p."));
        assert!(has(&items, "shown", CompletionItemKind::Method));
        assert!(has(&items, "x", CompletionItemKind::Field));
        assert!(
            !has_label(&items, "hidden"),
            "a private cross-module method is not callable, so not offered: {items:?}"
        );
    }

    #[test]
    fn a_same_file_receiver_keeps_private_methods() {
        // The struct is defined in the same file, so its private method is callable
        // and must still be offered — the visibility filter is cross-module only.
        let source = "struct P { x: i32; fn hidden(self) -> i32 { return self.x; } }\n\
fn m(p: P) -> i32 { return p.; }";
        let items = complete(source, after(source, "p."));
        assert!(
            has(&items, "hidden", CompletionItemKind::Method),
            "a same-file private method stays offered: {items:?}"
        );
    }

    // --- suppression inside comments and string literals ---

    #[test]
    fn a_line_comment_suppresses_completions() {
        let source = "fn f() -> i32 { return 0; }\n// note here";
        let inside = complete(source, after(source, "// no"));
        assert!(inside.is_empty(), "no completions inside a comment: {inside:?}");
        let end = complete(source, after(source, "// note here"));
        assert!(end.is_empty(), "the comment's tail is still suppressed");
    }

    #[test]
    fn a_doc_comment_suppresses_completions() {
        let source = "/// doc note\nfn f() -> i32 { return 0; }";
        let items = complete(source, after(source, "/// do"));
        assert!(items.is_empty(), "no completions inside a doc comment: {items:?}");
    }

    #[test]
    fn a_dot_inside_a_comment_is_not_a_member_context() {
        // Suppression runs before the member context, so `p.` in a comment offers
        // nothing rather than the fields of some `p`.
        let source = "fn m() -> i32 { return 0; }\n// p.";
        let items = complete(source, after(source, "// p."));
        assert!(items.is_empty(), "a comment is not member position: {items:?}");
    }

    #[test]
    fn code_after_a_comment_line_still_completes() {
        let source = "// header\nfn f() -> i32 { return 0; }";
        let items = complete(source, at(source, "fn f"));
        assert!(
            has(&items, "fn", CompletionItemKind::Keyword),
            "the next line is code again: {items:?}"
        );
    }

    #[test]
    fn a_string_literal_suppresses_completions_only_in_its_interior() {
        // `"abc"` spans the quotes inclusive; the interior is suppressed but the
        // quote boundaries are code positions on either side.
        let source = "fn f() -> i32 { let s: i32 = \"abc\"; return 0; }";
        let open = at(source, "\"abc\"");
        assert!(
            !complete(source, open).is_empty(),
            "the opening-quote position is still code"
        );
        assert!(
            complete(source, open + 1).is_empty(),
            "just inside the opening quote is suppressed"
        );
        assert!(
            complete(source, open + 4).is_empty(),
            "just before the closing quote is suppressed"
        );
        assert!(
            !complete(source, open + 5).is_empty(),
            "past the closing quote is code again"
        );
    }
}
