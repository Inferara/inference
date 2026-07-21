//! The definition hierarchy of one document, for the outline / breadcrumb view.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, IdentId};
use inference_ast::nodes::{Def, Field};
use inference_ide_db::{FileAnalysis, TextRange};

use crate::syntax::{def_name_ident, text_range};

/// The category of a [`DocumentSymbol`], in editor terminology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    EnumVariant,
    Field,
    Method,
    Spec,
    Constant,
    TypeAlias,
}

/// A definition and the definitions nested inside it, forming the document
/// outline. `range` spans the whole definition; `selection_range` is just its
/// name, so an editor can reveal the declaration and highlight the identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: TextRange,
    pub selection_range: TextRange,
    pub children: Vec<DocumentSymbol>,
}

/// Builds the outline of the entry file: top-level definitions, each carrying its
/// nested definitions (struct fields and methods, enum variants, spec-nested
/// defs) as children.
#[must_use]
pub(crate) fn document_symbols(file: &FileAnalysis) -> Vec<DocumentSymbol> {
    let arena = file.arena();
    let Some(entry) = file.source_file_id(&[]) else {
        return Vec::new();
    };
    arena[entry]
        .defs
        .iter()
        .filter_map(|&def| def_symbol(arena, def))
        .collect()
}

/// Whether `name` can be shown as an LSP symbol name. Parse-error recovery emits
/// a zero-width (empty) name identifier, and truly-absent names fall back to
/// `<error>`; LSP 3.17 forbids an empty `DocumentSymbol.name`, so such recovered
/// definitions are dropped from the outline entirely rather than surfaced.
fn is_displayable(name: &str) -> bool {
    !name.trim().is_empty() && name != "<error>"
}

/// A leaf symbol for the identifier `name`, or `None` when the identifier is a
/// parse-error placeholder (see [`is_displayable`]).
fn ident_symbol(arena: &AstArena, name: IdentId, kind: SymbolKind) -> Option<DocumentSymbol> {
    let text = arena.ident_name(name);
    if !is_displayable(text) {
        return None;
    }
    let range = text_range(arena[name].location);
    Some(DocumentSymbol {
        name: text.to_string(),
        kind,
        range,
        selection_range: range,
        children: Vec::new(),
    })
}

fn field_symbol(arena: &AstArena, field: &Field) -> Option<DocumentSymbol> {
    ident_symbol(arena, field.name, SymbolKind::Field)
}

/// A struct method: a `Def::Function` rendered as a `Method` leaf.
fn method_symbol(arena: &AstArena, method: DefId) -> Option<DocumentSymbol> {
    let mut symbol = def_symbol(arena, method)?;
    symbol.kind = SymbolKind::Method;
    Some(symbol)
}

/// The outline symbol for `def`, or `None` when the definition is a parse-error
/// placeholder with no displayable name (see [`is_displayable`]).
fn def_symbol(arena: &AstArena, def: DefId) -> Option<DocumentSymbol> {
    let text = arena.def_name(def);
    if !is_displayable(text) {
        return None;
    }
    let range = text_range(arena[def].location);
    let selection_range = text_range(arena[def_name_ident(arena, def)].location);
    let name = text.to_string();
    let (kind, children) = match &arena[def].kind {
        Def::Function { .. } | Def::ExternFunction { .. } => (SymbolKind::Function, Vec::new()),
        Def::Struct {
            fields, methods, ..
        } => {
            let mut children: Vec<DocumentSymbol> = fields
                .iter()
                .filter_map(|field| field_symbol(arena, field))
                .collect();
            children.extend(
                methods
                    .iter()
                    .filter_map(|&method| method_symbol(arena, method)),
            );
            (SymbolKind::Struct, children)
        }
        Def::Enum { variants, .. } => {
            let children = variants
                .iter()
                .filter_map(|&variant| ident_symbol(arena, variant, SymbolKind::EnumVariant))
                .collect();
            (SymbolKind::Enum, children)
        }
        Def::Spec { defs, .. } => {
            let children = defs
                .iter()
                .filter_map(|&nested| def_symbol(arena, nested))
                .collect();
            (SymbolKind::Spec, children)
        }
        Def::Constant { .. } => (SymbolKind::Constant, Vec::new()),
        Def::TypeAlias { .. } => (SymbolKind::TypeAlias, Vec::new()),
    };
    Some(DocumentSymbol {
        name,
        kind,
        range,
        selection_range,
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::{DocumentSymbol, SymbolKind};
    use crate::test_utils::{at, single};

    const SOURCE: &str = "const MAX: i32 = 1;\n\
type Handle = i32;\n\
enum Color { Red, Green }\n\
struct Point { px: i32; py: i32; fn getx(self) -> i32 { return self.px; } }\n\
spec Laws { fn commutes() {} }\n\
fn entry() { return; }";

    fn symbols(source: &str) -> Vec<DocumentSymbol> {
        let (host, path) = single(source);
        host.analysis().document_symbols(&path)
    }

    fn child<'a>(symbol: &'a DocumentSymbol, name: &str) -> &'a DocumentSymbol {
        symbol
            .children
            .iter()
            .find(|child| child.name == name)
            .unwrap_or_else(|| panic!("child `{name}` present"))
    }

    fn top<'a>(symbols: &'a [DocumentSymbol], name: &str) -> &'a DocumentSymbol {
        symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("top-level `{name}` present"))
    }

    #[test]
    fn top_level_order_and_kinds() {
        let symbols = symbols(SOURCE);
        let summary: Vec<(&str, SymbolKind)> = symbols
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("MAX", SymbolKind::Constant),
                ("Handle", SymbolKind::TypeAlias),
                ("Color", SymbolKind::Enum),
                ("Point", SymbolKind::Struct),
                ("Laws", SymbolKind::Spec),
                ("entry", SymbolKind::Function),
            ]
        );
    }

    #[test]
    fn selection_ranges_point_at_the_name() {
        let symbols = symbols(SOURCE);
        for name in ["MAX", "Handle", "Color", "Point", "Laws", "entry"] {
            let symbol = top(&symbols, name);
            assert_eq!(
                symbol.selection_range.start,
                at(SOURCE, name),
                "selection range of `{name}` is its identifier"
            );
            assert!(
                symbol.range.start <= symbol.selection_range.start
                    && symbol.range.end >= symbol.selection_range.end,
                "whole-definition range of `{name}` contains its name"
            );
        }
    }

    #[test]
    fn struct_children_are_fields_then_methods() {
        let symbols = symbols(SOURCE);
        let point = top(&symbols, "Point");
        let children: Vec<(&str, SymbolKind)> = point
            .children
            .iter()
            .map(|c| (c.name.as_str(), c.kind))
            .collect();
        assert_eq!(
            children,
            vec![
                ("px", SymbolKind::Field),
                ("py", SymbolKind::Field),
                ("getx", SymbolKind::Method),
            ]
        );
        assert_eq!(
            child(point, "getx").selection_range.start,
            at(SOURCE, "getx")
        );
    }

    #[test]
    fn enum_children_are_variants() {
        let symbols = symbols(SOURCE);
        let color = top(&symbols, "Color");
        let children: Vec<(&str, SymbolKind)> = color
            .children
            .iter()
            .map(|c| (c.name.as_str(), c.kind))
            .collect();
        assert_eq!(
            children,
            vec![
                ("Red", SymbolKind::EnumVariant),
                ("Green", SymbolKind::EnumVariant),
            ]
        );
    }

    #[test]
    fn spec_children_are_its_nested_functions() {
        let symbols = symbols(SOURCE);
        let laws = top(&symbols, "Laws");
        assert_eq!(laws.children.len(), 1);
        assert_eq!(child(laws, "commutes").kind, SymbolKind::Function);
    }

    #[test]
    fn empty_file_has_no_symbols() {
        assert!(symbols("").is_empty());
    }

    #[test]
    fn a_nameless_recovered_definition_is_skipped() {
        // `enum { ... }` recovers with a zero-width (empty) name identifier; LSP
        // 3.17 forbids an empty `DocumentSymbol.name`, so the whole recovered
        // definition is dropped from the outline rather than surfaced.
        let symbols = symbols("enum { Red, Green }\nfn f() {}\n");
        assert!(
            symbols
                .iter()
                .all(|symbol| !symbol.name.trim().is_empty() && symbol.name != "<error>"),
            "no empty or `<error>` symbol names: {symbols:?}"
        );
        let names: Vec<&str> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
        assert_eq!(names, vec!["f"], "only the named function survives");
    }

    #[test]
    fn only_the_entry_files_symbols_are_returned() {
        use crate::test_utils::with_lib;
        let entry = "use lib;\nfn only_me() -> i32 { return 0; }";
        let lib = "pub fn hidden() -> i32 { return 1; }";
        let (host, path) = with_lib(entry, lib);
        let names: Vec<String> = host
            .analysis()
            .document_symbols(&path)
            .into_iter()
            .map(|symbol| symbol.name)
            .collect();
        assert_eq!(names, vec!["only_me".to_string()]);
    }
}
