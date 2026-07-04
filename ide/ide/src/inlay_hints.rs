//! Inline hints that restate what each non-det construct means, at a glance.

use inference_ast::ids::{ExprId, NodeId, TypeId};
use inference_ast::nodes::{BlockKind, Expr, Stmt};
use inference_ide_db::{FileAnalysis, TextRange};
use inference_type_checker::type_info::TypeInfo;
use rustc_hash::FxHashMap;

use crate::nondet_docs::{UZUMAKI_INLAY, block_inlay, block_keyword};
use crate::syntax::walk_file;
use crate::type_render::render_type;

/// What a non-det [`InlayHint`] annotates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InlayHintKind {
    /// The header of a `forall` / `exists` / `unique` / `assume` block.
    NonDetBlock,
    /// A `@` (uzumaki) binding.
    Uzumaki,
}

/// One inline hint placed at a byte `offset` in the open document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    pub offset: u32,
    pub label: String,
    pub kind: InlayHintKind,
}

/// Produces the non-det inlay hints for the entry file, optionally clipped to
/// `range` (used to fetch only the hints for the viewport an editor is showing).
///
/// A block-header hint sits just after the opening keyword; a uzumaki hint sits
/// just after the `@`, with the binding's concrete type appended when known.
#[must_use]
pub(crate) fn inlay_hints(file: &FileAnalysis, range: Option<TextRange>) -> Vec<InlayHint> {
    let arena = file.arena();
    let Some(entry) = file.source_file_id(&[]) else {
        return Vec::new();
    };
    let source = arena[entry].source.as_str();

    let mut uzumaki_types: FxHashMap<ExprId, TypeId> = FxHashMap::default();
    let mut hints = Vec::new();
    walk_file(arena, entry, &mut |node| match node {
        NodeId::Stmt(stmt) => {
            if let Stmt::VarDef {
                ty,
                value: Some(value),
                ..
            } = &arena[stmt].kind
                && matches!(arena[*value].kind, Expr::Uzumaki)
            {
                uzumaki_types.insert(*value, *ty);
            }
        }
        NodeId::Block(block) => {
            let data = &arena[block];
            if let Some(hint) =
                nondet_block_hint(source, data.block_kind, data.location.offset_start)
            {
                hints.push(hint);
            }
        }
        NodeId::Expr(expr) => {
            if matches!(arena[expr].kind, Expr::Uzumaki) {
                let declared = uzumaki_types
                    .get(&expr)
                    .map(|&ty| TypeInfo::from_type_id(arena, ty));
                hints.push(InlayHint {
                    offset: arena[expr].location.offset_end,
                    label: uzumaki_label(declared.as_ref()),
                    kind: InlayHintKind::Uzumaki,
                });
            }
        }
        _ => {}
    });

    if let Some(range) = range {
        hints.retain(|hint| hint.offset >= range.start && hint.offset < range.end);
    }
    hints.sort_by_key(|hint| hint.offset);
    hints
}

/// A block-header hint, or `None` for a regular block or when the keyword is not
/// where the location says it should be (a defensive guard against a stray span).
fn nondet_block_hint(source: &str, kind: BlockKind, start: u32) -> Option<InlayHint> {
    let keyword = block_keyword(kind)?;
    let label = block_inlay(kind)?;
    let end = start.checked_add(u32::try_from(keyword.len()).ok()?)?;
    if source.get(start as usize..end as usize) != Some(keyword) {
        return None;
    }
    Some(InlayHint {
        offset: end,
        label: label.to_string(),
        kind: InlayHintKind::NonDetBlock,
    })
}

/// The uzumaki hint label: the verbatim text, with the binding's concrete type in
/// parentheses when the declaration named one.
fn uzumaki_label(declared: Option<&TypeInfo>) -> String {
    match declared {
        Some(ty) => format!("{UZUMAKI_INLAY} ({})", render_type(ty)),
        None => UZUMAKI_INLAY.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]

    use super::{InlayHint, InlayHintKind};
    use crate::TextRange;
    use crate::nondet_docs::{
        ASSUME_INLAY, EXISTS_INLAY, FORALL_INLAY, UNIQUE_INLAY, UZUMAKI_INLAY,
    };
    use crate::test_utils::{at, nth, single};

    const NONDET: &str = "fn f() {\n\
    forall { let a: i32 = @; assert(a == a); }\n\
    exists { let b: i32 = @; assert(b == b); }\n\
    unique { assert(true); }\n\
    assume { assert(true); }\n\
}";

    fn hints(source: &str, range: Option<TextRange>) -> Vec<InlayHint> {
        let (mut host, path) = single(source);
        host.analysis().inlay_hints(&path, range)
    }

    fn nondet_hint<'a>(hints: &'a [InlayHint], keyword: &str) -> &'a InlayHint {
        let offset = at(NONDET, keyword) + keyword.len() as u32;
        hints
            .iter()
            .find(|hint| hint.kind == InlayHintKind::NonDetBlock && hint.offset == offset)
            .unwrap_or_else(|| panic!("a hint after `{keyword}`"))
    }

    #[test]
    fn every_block_kind_gets_a_hint_after_its_keyword() {
        let hints = hints(NONDET, None);
        for (keyword, label) in [
            ("forall", FORALL_INLAY),
            ("exists", EXISTS_INLAY),
            ("unique", UNIQUE_INLAY),
            ("assume", ASSUME_INLAY),
        ] {
            assert_eq!(nondet_hint(&hints, keyword).label, label);
        }
    }

    #[test]
    fn each_uzumaki_gets_a_typed_hint_after_the_at() {
        let hints = hints(NONDET, None);
        let mut uzumaki: Vec<&InlayHint> = hints
            .iter()
            .filter(|hint| hint.kind == InlayHintKind::Uzumaki)
            .collect();
        uzumaki.sort_by_key(|hint| hint.offset);
        assert_eq!(uzumaki.len(), 2, "one per `@`");
        for hint in &uzumaki {
            assert_eq!(hint.label, format!("{UZUMAKI_INLAY} (i32)"));
        }
        // Each hint sits just past its own `@`. Asserting both offsets (not only
        // the first) catches a regression that collapses the second uzumaki hint
        // onto the first's position.
        let offsets: Vec<u32> = uzumaki.iter().map(|hint| hint.offset).collect();
        assert_eq!(
            offsets,
            vec![nth(NONDET, "@", 0) + 1, nth(NONDET, "@", 1) + 1]
        );
    }

    #[test]
    fn hints_are_ordered_by_offset() {
        let hints = hints(NONDET, None);
        let offsets: Vec<u32> = hints.iter().map(|hint| hint.offset).collect();
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        assert_eq!(offsets, sorted);
    }

    #[test]
    fn a_range_filter_keeps_only_hints_inside_it() {
        let start = at(NONDET, "forall");
        let range = TextRange {
            start,
            end: start + "forall".len() as u32 + 1,
        };
        let filtered = hints(NONDET, Some(range));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].kind, InlayHintKind::NonDetBlock);
        assert_eq!(filtered[0].label, FORALL_INLAY);
    }

    #[test]
    fn an_uzumaki_without_a_typed_binding_omits_the_type() {
        // The second `@` is not the value of a typed `let`, so its hint is the
        // bare verbatim text with no parenthetical type.
        let source = "fn f() { forall { let x: i32 = @; assert(@ == x); } }";
        let hints = hints(source, None);
        let bare = hints
            .iter()
            .filter(|hint| hint.kind == InlayHintKind::Uzumaki)
            .find(|hint| hint.label == UZUMAKI_INLAY);
        assert!(
            bare.is_some(),
            "a standalone `@` gets the untyped label: {hints:?}"
        );
    }

    #[test]
    fn the_range_filter_is_half_open() {
        // A hint exactly at `range.start` is kept; one exactly at `range.end` is
        // excluded. Build a window whose start is the forall hint's offset and
        // whose end is the first uzumaki hint's offset.
        let all = hints(NONDET, None);
        let forall_offset = at(NONDET, "forall") + "forall".len() as u32;
        let uzumaki_offset = all
            .iter()
            .find(|hint| hint.kind == InlayHintKind::Uzumaki)
            .expect("a uzumaki hint")
            .offset;
        let window = TextRange {
            start: forall_offset,
            end: uzumaki_offset,
        };
        let filtered = hints(NONDET, Some(window));
        assert!(
            filtered.iter().any(|hint| hint.offset == forall_offset),
            "the hint at range.start is kept"
        );
        assert!(
            filtered.iter().all(|hint| hint.offset != uzumaki_offset),
            "the hint at range.end is excluded"
        );
    }

    #[test]
    fn a_forall_marked_function_body_gets_a_header_hint() {
        // The non-det kind lives on the body block even when it marks the whole
        // function signature (`fn f() forall { … }`), so the hint still appears.
        let source = "fn prop(a: i32) forall { assert(a == a); }";
        let hints = hints(source, None);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].kind, InlayHintKind::NonDetBlock);
        assert_eq!(hints[0].label, FORALL_INLAY);
        assert_eq!(
            hints[0].offset,
            at(source, "forall") + "forall".len() as u32
        );
    }

    #[test]
    fn a_file_without_nondet_has_no_hints() {
        assert!(hints("fn f() -> i32 { return 1; }", None).is_empty());
    }
}
