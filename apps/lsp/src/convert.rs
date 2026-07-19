//! Conversions from `ide`'s plain-old-data answers to `lsp-types` protocol
//! values.
//!
//! The IDE layer speaks byte offsets; the protocol speaks 0-based UTF-16
//! line/character. Every position crosses that boundary here, using a
//! [`LineIndex`](inference_ide::LineIndex) built for the *right* file — a
//! cross-file goto-definition target is converted with the target file's index,
//! not the requested document's. The severity and kind mappings match every
//! `ide` variant explicitly, so a new one is a compile error here rather than a
//! silent default on the wire.

use inference_ide as ide;
use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol, Hover,
    HoverContents, InlayHint, InlayHintKind, InlayHintLabel, Location, MarkupContent, MarkupKind,
    NumberOrString, Position, Range, SymbolInformation, SymbolKind, Uri,
};

/// The LSP position of a byte `offset` within the file `index` describes.
pub(crate) fn position(index: &ide::LineIndex, offset: u32) -> Position {
    let line_col = index.line_col(offset);
    Position {
        line: line_col.line,
        character: line_col.character,
    }
}

/// The LSP range of an `ide` byte range within the file `index` describes.
pub(crate) fn range(index: &ide::LineIndex, range: ide::TextRange) -> Range {
    Range {
        start: position(index, range.start),
        end: position(index, range.end),
    }
}

/// The byte offset of an LSP position, or `None` when the position's line is out
/// of range for the file `index` describes.
pub(crate) fn offset(index: &ide::LineIndex, position: Position) -> Option<u32> {
    index.offset(ide::LineCol {
        line: position.line,
        character: position.character,
    })
}

/// The `ide` byte range of an LSP range, clamping an endpoint whose line is past
/// the end of the file to the file's end.
///
/// Used for the inlay-hint request window: an out-of-range endpoint (an end one
/// line past EOF is a common client spelling for "to the end") must still yield a
/// bounded range so the window is honored, never widened to the whole file by a
/// single unmappable position.
pub(crate) fn text_range_clamped(index: &ide::LineIndex, range: Range) -> ide::TextRange {
    ide::TextRange {
        start: offset_clamped(index, range.start),
        end: offset_clamped(index, range.end),
    }
}

/// The `ide` byte offset of an LSP position, clamping a line past EOF to the end
/// of the file (`offset` clamps a past-EOL character within a line; this extends
/// the same clamp to the line).
fn offset_clamped(index: &ide::LineIndex, position: Position) -> u32 {
    index.offset_clamped(ide::LineCol {
        line: position.line,
        character: position.character,
    })
}

pub(crate) fn severity(severity: ide::Severity) -> DiagnosticSeverity {
    match severity {
        ide::Severity::Error => DiagnosticSeverity::ERROR,
        ide::Severity::Warning => DiagnosticSeverity::WARNING,
        ide::Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

pub(crate) fn diagnostic(index: &ide::LineIndex, diagnostic: ide::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: range(index, diagnostic.range),
        severity: Some(severity(diagnostic.severity)),
        code: diagnostic.code.map(NumberOrString::String),
        source: Some("inference".to_owned()),
        message: diagnostic.message,
        ..Diagnostic::default()
    }
}

pub(crate) fn symbol_kind(kind: ide::SymbolKind) -> SymbolKind {
    match kind {
        ide::SymbolKind::Function => SymbolKind::FUNCTION,
        ide::SymbolKind::Struct => SymbolKind::STRUCT,
        ide::SymbolKind::Enum => SymbolKind::ENUM,
        ide::SymbolKind::EnumVariant => SymbolKind::ENUM_MEMBER,
        ide::SymbolKind::Field => SymbolKind::FIELD,
        ide::SymbolKind::Method => SymbolKind::METHOD,
        // A spec is a set of laws over a type, closest to an interface/trait.
        ide::SymbolKind::Spec => SymbolKind::INTERFACE,
        ide::SymbolKind::Constant => SymbolKind::CONSTANT,
        ide::SymbolKind::TypeAlias => SymbolKind::TYPE_PARAMETER,
    }
}

/// A hierarchical document symbol and its nested children.
pub(crate) fn document_symbol(
    index: &ide::LineIndex,
    symbol: ide::DocumentSymbol,
) -> DocumentSymbol {
    let children = if symbol.children.is_empty() {
        None
    } else {
        Some(
            symbol
                .children
                .into_iter()
                .map(|child| document_symbol(index, child))
                .collect(),
        )
    };
    #[allow(deprecated)] // `deprecated` is a required field; we always leave it unset.
    DocumentSymbol {
        name: symbol.name,
        detail: None,
        kind: symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        range: range(index, symbol.range),
        selection_range: range(index, symbol.selection_range),
        children,
    }
}

/// A flat symbol record for a client that does not support the hierarchical
/// document-symbol response. `container` names the enclosing symbol, if any.
pub(crate) fn symbol_information(
    index: &ide::LineIndex,
    uri: &Uri,
    container: Option<&str>,
    symbol: &ide::DocumentSymbol,
) -> SymbolInformation {
    #[allow(deprecated)] // `deprecated` is a required field; we always leave it unset.
    SymbolInformation {
        name: symbol.name.clone(),
        kind: symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range: range(index, symbol.range),
        },
        container_name: container.map(ToOwned::to_owned),
    }
}

/// A hover rendered in `format`. A client that did not advertise Markdown in its
/// `textDocument.hover.contentFormat` gets [`MarkupKind::PlainText`], for which
/// the `ide` layer's Markdown is reduced to plain text (see [`plain_text`]) so
/// fence delimiters and inline-code markers are not rendered literally.
pub(crate) fn hover(index: &ide::LineIndex, hover: ide::Hover, format: MarkupKind) -> Hover {
    let value = match format {
        MarkupKind::PlainText => plain_text(&hover.contents_markdown),
        MarkupKind::Markdown => hover.contents_markdown,
    };
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: format,
            value,
        }),
        range: Some(range(index, hover.range)),
    }
}

/// Reduces the limited Markdown the `ide` layer emits — fenced code blocks and
/// inline code — to plain text for a client that accepts only plain text.
///
/// Fence delimiter lines (`` ``` ``) are dropped. Outside a fence, inline-code
/// backticks are removed; inside one, the body is emitted verbatim, so a
/// backtick that is part of the code example itself survives. Every other
/// character is preserved either way, so a `*` inside a code example (a
/// multiplication, say) survives rather than being mistaken for emphasis and
/// stripped.
fn plain_text(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut wrote_line = false;
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if wrote_line {
            out.push('\n');
        }
        wrote_line = true;
        if in_fence {
            out.push_str(line);
        } else {
            out.extend(line.chars().filter(|&ch| ch != '`'));
        }
    }
    out
}

pub(crate) fn completion_item_kind(kind: ide::CompletionItemKind) -> CompletionItemKind {
    match kind {
        ide::CompletionItemKind::Keyword => CompletionItemKind::KEYWORD,
        ide::CompletionItemKind::Function => CompletionItemKind::FUNCTION,
        ide::CompletionItemKind::Struct => CompletionItemKind::STRUCT,
        ide::CompletionItemKind::Enum => CompletionItemKind::ENUM,
        ide::CompletionItemKind::Variable => CompletionItemKind::VARIABLE,
        ide::CompletionItemKind::Field => CompletionItemKind::FIELD,
        ide::CompletionItemKind::Method => CompletionItemKind::METHOD,
        ide::CompletionItemKind::Constant => CompletionItemKind::CONSTANT,
        ide::CompletionItemKind::Module => CompletionItemKind::MODULE,
        ide::CompletionItemKind::Snippet => CompletionItemKind::SNIPPET,
    }
}

pub(crate) fn completion_item(item: ide::CompletionItem) -> CompletionItem {
    CompletionItem {
        label: item.label,
        kind: Some(completion_item_kind(item.kind)),
        detail: item.detail,
        ..CompletionItem::default()
    }
}

pub(crate) fn inlay_hint_kind(kind: ide::InlayHintKind) -> InlayHintKind {
    match kind {
        // Both non-det annotations describe the meaning of a construct; the LSP
        // vocabulary offers only Type and Parameter, and Type is the closer fit.
        ide::InlayHintKind::NonDetBlock | ide::InlayHintKind::Uzumaki => InlayHintKind::TYPE,
    }
}

pub(crate) fn inlay_hint(index: &ide::LineIndex, hint: ide::InlayHint) -> InlayHint {
    InlayHint {
        position: position(index, hint.offset),
        label: InlayHintLabel::String(hint.label),
        kind: Some(inlay_hint_kind(hint.kind)),
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: None,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use inference_ide as ide;
    use lsp_types::{
        CompletionItemKind, DiagnosticSeverity, HoverContents, InlayHintKind, InlayHintLabel,
        MarkupKind, NumberOrString, Position, SymbolKind, Uri,
    };

    use super::{
        completion_item, completion_item_kind, diagnostic, document_symbol, hover, inlay_hint,
        inlay_hint_kind, offset, position, range, severity, symbol_information, symbol_kind,
        text_range_clamped,
    };

    fn index(text: &str) -> ide::LineIndex {
        ide::LineIndex::new(text)
    }

    #[test]
    fn position_and_offset_round_trip_on_a_multibyte_line() {
        // "é∀\n😀b": the astral emoji is a surrogate pair (two UTF-16 units).
        let index = index("é∀\n😀b");
        // 'b' is byte offset 10, on line 1 after the surrogate pair -> column 2.
        assert_eq!(
            position(&index, 10),
            Position {
                line: 1,
                character: 2
            }
        );
        assert_eq!(
            offset(
                &index,
                Position {
                    line: 1,
                    character: 2
                }
            ),
            Some(10)
        );
    }

    #[test]
    fn range_maps_both_endpoints() {
        let index = index("ab\ncd");
        let converted = range(&index, ide::TextRange { start: 0, end: 4 });
        assert_eq!(
            converted.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            converted.end,
            Position {
                line: 1,
                character: 1
            }
        );
    }

    #[test]
    fn offset_past_end_of_line_clamps_but_out_of_range_line_is_none() {
        let index = index("ab\ncd");
        assert_eq!(
            offset(
                &index,
                Position {
                    line: 0,
                    character: 99
                }
            ),
            Some(2)
        );
        assert_eq!(
            offset(
                &index,
                Position {
                    line: 9,
                    character: 0
                }
            ),
            None
        );
    }

    #[test]
    fn text_range_clamped_maps_an_in_range_window_exactly() {
        let index = index("ab\ncd");
        let good = lsp_types::Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 1,
            },
        };
        assert_eq!(
            text_range_clamped(&index, good),
            ide::TextRange { start: 0, end: 4 }
        );
    }

    #[test]
    fn text_range_clamped_clamps_an_out_of_range_end_to_the_file_end() {
        // The bug this guards: an end one line past EOF used to make the whole
        // conversion `None`, disabling the inlay clip and returning hints outside
        // the requested window. It must clamp to the file end instead, keeping a
        // valid start intact so the window is honored.
        let index = index("ab\ncd");
        let past_eof_end = lsp_types::Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 9,
                character: 0,
            },
        };
        assert_eq!(
            text_range_clamped(&index, past_eof_end),
            ide::TextRange { start: 3, end: 5 }
        );
    }

    #[test]
    fn severity_maps_every_variant() {
        assert_eq!(severity(ide::Severity::Error), DiagnosticSeverity::ERROR);
        assert_eq!(
            severity(ide::Severity::Warning),
            DiagnosticSeverity::WARNING
        );
        assert_eq!(
            severity(ide::Severity::Info),
            DiagnosticSeverity::INFORMATION
        );
    }

    #[test]
    fn diagnostic_carries_code_source_and_stripped_message() {
        let index = index("fn f() {}");
        let converted = diagnostic(
            &index,
            ide::Diagnostic {
                range: ide::TextRange { start: 3, end: 4 },
                severity: ide::Severity::Error,
                code: Some("A041".to_owned()),
                message: "already declared".to_owned(),
            },
        );
        assert_eq!(converted.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            converted.code,
            Some(NumberOrString::String("A041".to_owned()))
        );
        assert_eq!(converted.source.as_deref(), Some("inference"));
        assert_eq!(converted.message, "already declared");
    }

    #[test]
    fn symbol_kind_maps_every_variant() {
        assert_eq!(symbol_kind(ide::SymbolKind::Function), SymbolKind::FUNCTION);
        assert_eq!(symbol_kind(ide::SymbolKind::Struct), SymbolKind::STRUCT);
        assert_eq!(symbol_kind(ide::SymbolKind::Enum), SymbolKind::ENUM);
        assert_eq!(
            symbol_kind(ide::SymbolKind::EnumVariant),
            SymbolKind::ENUM_MEMBER
        );
        assert_eq!(symbol_kind(ide::SymbolKind::Field), SymbolKind::FIELD);
        assert_eq!(symbol_kind(ide::SymbolKind::Method), SymbolKind::METHOD);
        assert_eq!(symbol_kind(ide::SymbolKind::Spec), SymbolKind::INTERFACE);
        assert_eq!(symbol_kind(ide::SymbolKind::Constant), SymbolKind::CONSTANT);
        assert_eq!(
            symbol_kind(ide::SymbolKind::TypeAlias),
            SymbolKind::TYPE_PARAMETER
        );
    }

    #[test]
    fn completion_item_kind_maps_every_variant() {
        let cases = [
            (
                ide::CompletionItemKind::Keyword,
                CompletionItemKind::KEYWORD,
            ),
            (
                ide::CompletionItemKind::Function,
                CompletionItemKind::FUNCTION,
            ),
            (ide::CompletionItemKind::Struct, CompletionItemKind::STRUCT),
            (ide::CompletionItemKind::Enum, CompletionItemKind::ENUM),
            (
                ide::CompletionItemKind::Variable,
                CompletionItemKind::VARIABLE,
            ),
            (ide::CompletionItemKind::Field, CompletionItemKind::FIELD),
            (ide::CompletionItemKind::Method, CompletionItemKind::METHOD),
            (
                ide::CompletionItemKind::Constant,
                CompletionItemKind::CONSTANT,
            ),
            (ide::CompletionItemKind::Module, CompletionItemKind::MODULE),
            (
                ide::CompletionItemKind::Snippet,
                CompletionItemKind::SNIPPET,
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(completion_item_kind(input), expected);
        }
    }

    #[test]
    fn inlay_hint_kind_maps_every_variant() {
        assert_eq!(
            inlay_hint_kind(ide::InlayHintKind::NonDetBlock),
            InlayHintKind::TYPE
        );
        assert_eq!(
            inlay_hint_kind(ide::InlayHintKind::Uzumaki),
            InlayHintKind::TYPE
        );
    }

    #[test]
    fn hover_is_markdown_with_a_range() {
        let index = index("fn f() {}");
        let converted = hover(
            &index,
            ide::Hover {
                contents_markdown: "```inference\nfn f()\n```".to_owned(),
                range: ide::TextRange { start: 3, end: 4 },
            },
            MarkupKind::Markdown,
        );
        let HoverContents::Markup(markup) = converted.contents else {
            panic!("hover contents are markdown");
        };
        assert_eq!(markup.kind, MarkupKind::Markdown);
        assert_eq!(markup.value, "```inference\nfn f()\n```");
        assert!(converted.range.is_some());
    }

    #[test]
    fn hover_as_plaintext_strips_fences_and_backticks() {
        let index = index("fn f() {}");
        let converted = hover(
            &index,
            ide::Hover {
                contents_markdown: "```inference\nfn f() -> i32\n```".to_owned(),
                range: ide::TextRange { start: 3, end: 4 },
            },
            MarkupKind::PlainText,
        );
        let HoverContents::Markup(markup) = converted.contents else {
            panic!("hover contents are markup");
        };
        assert_eq!(markup.kind, MarkupKind::PlainText);
        // The fence delimiter lines are gone and the signature stands alone.
        assert_eq!(markup.value, "fn f() -> i32");
    }

    #[test]
    fn plaintext_hover_drops_inline_code_but_keeps_a_code_example_intact() {
        let index = index("fn f() {}");
        // Prose with inline code and a fenced example whose body contains a `*`
        // that must survive (it is a multiplication, not emphasis).
        let markdown = "**`exists`** holds when\n\n```inference\nassert(n * n == 25);\n```";
        let converted = hover(
            &index,
            ide::Hover {
                contents_markdown: markdown.to_owned(),
                range: ide::TextRange { start: 0, end: 1 },
            },
            MarkupKind::PlainText,
        );
        let HoverContents::Markup(markup) = converted.contents else {
            panic!("hover contents are markup");
        };
        assert!(
            !markup.value.contains('`'),
            "no backticks remain: {:?}",
            markup.value
        );
        assert!(
            !markup.value.contains("```"),
            "no fence delimiters remain: {:?}",
            markup.value
        );
        assert!(
            markup.value.contains("assert(n * n == 25);"),
            "the code example's `*` survives: {:?}",
            markup.value
        );
        assert!(
            markup.value.contains("**exists** holds when"),
            "inline-code backticks are removed, emphasis asterisks preserved: {:?}",
            markup.value
        );
    }

    #[test]
    fn plaintext_hover_keeps_a_backtick_that_belongs_to_the_code_example() {
        let index = index("fn f() {}");
        // Inline code in the prose, and a fenced body whose own backtick is part
        // of the example rather than a Markdown marker.
        let markdown = "Compare with `eq`\n\n```inference\nlet tick = \"`\";\n```";
        let converted = hover(
            &index,
            ide::Hover {
                contents_markdown: markdown.to_owned(),
                range: ide::TextRange { start: 0, end: 1 },
            },
            MarkupKind::PlainText,
        );
        let HoverContents::Markup(markup) = converted.contents else {
            panic!("hover contents are markup");
        };
        // Fence delimiters gone, the blank line between prose and example kept,
        // inline-code backticks stripped, the fenced body verbatim.
        assert_eq!(markup.value, "Compare with eq\n\nlet tick = \"`\";");
    }

    #[test]
    fn completion_item_preserves_label_and_detail() {
        let converted = completion_item(ide::CompletionItem {
            label: "helper".to_owned(),
            kind: ide::CompletionItemKind::Function,
            detail: Some("fn helper() -> i32".to_owned()),
        });
        assert_eq!(converted.label, "helper");
        assert_eq!(converted.kind, Some(CompletionItemKind::FUNCTION));
        assert_eq!(converted.detail.as_deref(), Some("fn helper() -> i32"));
    }

    #[test]
    fn inlay_hint_carries_label_kind_and_position() {
        let index = index("fn f() { forall { assert(true); } }");
        let converted = inlay_hint(
            &index,
            ide::InlayHint {
                offset: 15,
                label: "for all values".to_owned(),
                kind: ide::InlayHintKind::NonDetBlock,
            },
        );
        let InlayHintLabel::String(label) = converted.label else {
            panic!("the inlay label is a plain string");
        };
        assert_eq!(label, "for all values");
        assert_eq!(converted.kind, Some(InlayHintKind::TYPE));
        assert_eq!(
            converted.position,
            Position {
                line: 0,
                character: 15
            }
        );
    }

    #[test]
    fn symbol_information_records_container_and_location() {
        let index = index("struct P { x: i32; }");
        let uri = Uri::from_str("file:///main.inf").expect("uri");
        let symbol = ide::DocumentSymbol {
            name: "x".to_owned(),
            kind: ide::SymbolKind::Field,
            range: ide::TextRange { start: 11, end: 12 },
            selection_range: ide::TextRange { start: 11, end: 12 },
            children: Vec::new(),
        };
        let info = symbol_information(&index, &uri, Some("P"), &symbol);
        assert_eq!(info.name, "x");
        assert_eq!(info.kind, SymbolKind::FIELD);
        assert_eq!(info.container_name.as_deref(), Some("P"));
        assert_eq!(info.location.uri, uri);
    }

    #[test]
    fn document_symbol_nests_children() {
        let index = index("struct P { x: i32; }");
        let symbol = ide::DocumentSymbol {
            name: "P".to_owned(),
            kind: ide::SymbolKind::Struct,
            range: ide::TextRange { start: 0, end: 20 },
            selection_range: ide::TextRange { start: 7, end: 8 },
            children: vec![ide::DocumentSymbol {
                name: "x".to_owned(),
                kind: ide::SymbolKind::Field,
                range: ide::TextRange { start: 11, end: 12 },
                selection_range: ide::TextRange { start: 11, end: 12 },
                children: Vec::new(),
            }],
        };
        let converted = document_symbol(&index, symbol);
        assert_eq!(converted.kind, SymbolKind::STRUCT);
        let children = converted.children.expect("a struct has field children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].kind, SymbolKind::FIELD);
    }
}
