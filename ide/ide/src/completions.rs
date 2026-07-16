//! Completion suggestions for a position in a document.
//!
//! Two contexts are distinguished. Right after a `.` whose receiver has a known
//! struct type, only that struct's fields and instance methods are offered.
//! Everywhere else the suggestions are keywords, the locals in scope, the
//! document's own top-level definitions, and the modules it imports.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, NodeId, SourceFileId};
use inference_ast::nodes::{ArgKind, Def, Stmt};
use inference_ide_db::{FileAnalysis, NodeHit};
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use rustc_hash::FxHashSet;

use crate::syntax::{
    def_signature, enclosing_function, find_def_by_name, imported_module_paths, in_scope_locals,
    method_has_self,
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

    if let Some(items) = member_completions(file, entry, source, offset) {
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
    let Some(hit) = enclosing_hit(file, entry, offset) else {
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

/// Modules imported by the entry file plus each module's `pub` top-level defs, so
/// a name from an imported module can be completed.
fn push_imported(file: &FileAnalysis, entry: SourceFileId, items: &mut Vec<CompletionItem>) {
    let arena = file.arena();
    for module_path in imported_module_paths(arena, entry) {
        if let Some(name) = module_path.last() {
            items.push(CompletionItem {
                label: name.clone(),
                kind: CompletionItemKind::Module,
                detail: None,
            });
        }
        let Some(sfid) = file.source_file_id(&module_path) else {
            continue;
        };
        for &def in &arena[sfid].defs {
            if crate::syntax::def_is_public(arena, def) {
                items.push(def_completion(file, sfid, def));
            }
        }
    }
}

fn def_completion(file: &FileAnalysis, sfid: SourceFileId, def: DefId) -> CompletionItem {
    let arena = file.arena();
    CompletionItem {
        label: arena.def_name(def).to_string(),
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
    Some(struct_members(file, bare, key))
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

/// The fields and instance methods of the struct identified by `key`.
fn struct_members(file: &FileAnalysis, bare: &str, key: &str) -> Vec<CompletionItem> {
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
    if let Def::Struct { methods, .. } = &arena[struct_def].kind {
        for &method in methods {
            if method_has_self(arena, method) {
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

/// The hit that locates `offset` within a definition, tolerating a cursor one
/// byte past the token it is completing.
///
/// The one-byte fallback exists only to complete a just-typed identifier, so it
/// fires only when the byte at `offset - 1` is part of an identifier. A
/// punctuation byte such as a closing `}` must not pull the cursor back inside
/// the preceding definition, which would leak that function's params and locals
/// at file scope.
fn enclosing_hit(file: &FileAnalysis, entry: SourceFileId, offset: u32) -> Option<NodeHit> {
    if let Some(hit) = file.hit_test(entry, offset) {
        return Some(hit);
    }
    let back = offset.checked_sub(1)?;
    let source = file.arena()[entry].source.as_str();
    if source
        .as_bytes()
        .get(back as usize)
        .copied()
        .is_some_and(is_ident_byte)
    {
        file.hit_test(entry, back)
    } else {
        None
    }
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
    use crate::test_utils::{at, single, with_lib};

    fn complete(source: &str, offset: u32) -> Vec<CompletionItem> {
        let (mut host, path) = single(source);
        host.analysis().completions(&path, offset)
    }

    fn has(items: &[CompletionItem], label: &str, kind: CompletionItemKind) -> bool {
        items
            .iter()
            .any(|item| item.label == label && item.kind == kind)
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

    #[test]
    fn imported_module_and_its_public_defs_are_offered() {
        let entry = "use lib;\nfn main() -> i32 { return 0; }";
        let lib = "pub fn exported() -> i32 { return 1; }";
        let (mut host, path) = with_lib(entry, lib);
        let items = host.analysis().completions(&path, at(entry, "return 0"));
        assert!(has(&items, "lib", CompletionItemKind::Module));
        assert!(has(&items, "exported", CompletionItemKind::Function));
    }
}
