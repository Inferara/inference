#![warn(clippy::pedantic)]
//! The feature layer of the Inference IDE stack: plain-old-data answers to the
//! questions an editor asks about a document.
//!
//! [`AnalysisHost`] owns the open-document state (delegating to `ide-db`'s
//! `RootDatabase`); [`Analysis`] borrows it to answer feature queries —
//! diagnostics, document symbols, hover, goto-definition, completions, and inlay
//! hints. Every result is a plain struct in editor terminology
//! ([`Diagnostic`], [`DocumentSymbol`], [`Hover`], [`NavigationTarget`],
//! [`CompletionItem`], [`InlayHint`]); no compiler type crosses this boundary, so
//! the protocol layer above (`apps/lsp`) maps these straight onto LSP responses.
//!
//! # Coordinates
//!
//! Positions in and out of this crate are **byte offsets** into a document's
//! current text, and ranges are byte ranges. The protocol layer converts them to
//! LSP line/character with the [`LineIndex`] this crate exposes. The open
//! document is addressed by its path; the entry file's module path is the empty
//! slice, which is how a query reaches the document it was asked about.
//!
//! # Single document, single thread
//!
//! Each open file is analyzed as its own project entry (its import closure
//! resolved through the overlay-then-disk loader in `ide-db`), and the resulting
//! analysis answers every query for that document — including goto-definition
//! into an imported file, whose [`NavigationTarget`] carries that file's real path
//! and ranges in its own coordinates. A query borrows the database mutably because
//! the analysis is computed lazily and memoized on first use; the LSP main loop is
//! single-threaded, so this is exactly the access pattern it needs.

mod completions;
mod diagnostics;
mod document_symbols;
mod goto_definition;
mod hover;
mod inlay_hints;
mod nondet_docs;
mod syntax;
mod type_render;

#[cfg(test)]
mod test_utils;

use std::path::Path;
use std::sync::Arc;

use inference_ide_db::RootDatabase;

pub use completions::{CompletionItem, CompletionItemKind};
pub use diagnostics::{Diagnostic, Severity};
pub use document_symbols::{DocumentSymbol, SymbolKind};
pub use goto_definition::NavigationTarget;
pub use hover::Hover;
pub use inlay_hints::{InlayHint, InlayHintKind};

// Re-export the position primitives the protocol layer needs to turn byte
// offsets into LSP positions, so it depends on `inference-ide` alone. The API is
// path-addressed, so the file-id PODs are intentionally not surfaced here.
pub use inference_ide_db::{LineCol, LineIndex, TextRange};

/// Owns the editor's open documents and the analyses derived from them.
///
/// Construct with [`AnalysisHost::default`], mirror the editor's lifecycle with
/// [`open_document`](Self::open_document) / [`change_document`](Self::change_document)
/// / [`close_document`](Self::close_document), then take an [`Analysis`] to answer
/// feature queries.
#[derive(Default)]
pub struct AnalysisHost {
    db: RootDatabase,
}

impl AnalysisHost {
    /// Records `text` as the current contents of `path` (an editor `didOpen`).
    pub fn open_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        self.db.open_document(path, text);
    }

    /// Replaces the current contents of the open document `path` (a `didChange`).
    pub fn change_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        self.db.change_document(path, text);
    }

    /// Drops the in-memory contents of `path` (a `didClose`); later analyses read
    /// it from disk again.
    pub fn close_document(&mut self, path: &Path) {
        self.db.close_document(path);
    }

    /// Borrows the host to answer feature queries.
    #[must_use = "an Analysis does nothing until a query method is called"]
    pub fn analysis(&mut self) -> Analysis<'_> {
        Analysis { db: &mut self.db }
    }
}

/// A borrowed view over the host that answers feature queries for a document.
///
/// Each method names its document by path and takes `&mut self` because the
/// document's analysis is computed lazily and cached on first use.
pub struct Analysis<'a> {
    db: &'a mut RootDatabase,
}

impl Analysis<'_> {
    /// The diagnostics for the open document `path`: syntax, import, type, and
    /// analysis-rule findings that belong to it.
    #[must_use = "the diagnostics are the reason to call this"]
    pub fn diagnostics(&mut self, path: &Path) -> Vec<Diagnostic> {
        diagnostics::diagnostics(self.db.analysis(path))
    }

    /// The definition outline of the document `path`.
    #[must_use = "the symbols are the reason to call this"]
    pub fn document_symbols(&mut self, path: &Path) -> Vec<DocumentSymbol> {
        document_symbols::document_symbols(self.db.analysis(path))
    }

    /// The hover for byte `offset` in the document `path`, if anything is there.
    #[must_use = "the hover is the reason to call this"]
    pub fn hover(&mut self, path: &Path, offset: u32) -> Option<Hover> {
        hover::hover(self.db.analysis(path), offset)
    }

    /// The definition(s) of the identifier at byte `offset` in `path`, if any.
    #[must_use = "the navigation targets are the reason to call this"]
    pub fn goto_definition(&mut self, path: &Path, offset: u32) -> Option<Vec<NavigationTarget>> {
        goto_definition::goto_definition(self.db.analysis(path), offset)
    }

    /// The completions for byte `offset` in the document `path`.
    #[must_use = "the completions are the reason to call this"]
    pub fn completions(&mut self, path: &Path, offset: u32) -> Vec<CompletionItem> {
        completions::completions(self.db.analysis(path), offset)
    }

    /// The non-det inlay hints for `path`, optionally clipped to `range`.
    #[must_use = "the inlay hints are the reason to call this"]
    pub fn inlay_hints(&mut self, path: &Path, range: Option<TextRange>) -> Vec<InlayHint> {
        inlay_hints::inlay_hints(self.db.analysis(path), range)
    }

    /// The line index of the document `path`, for byte-offset ↔ line/column
    /// conversion; `None` when the document is not analyzable.
    #[must_use = "the line index is the reason to call this"]
    pub fn line_index(&mut self, path: &Path) -> Option<LineIndex> {
        self.db.analysis(path).line_index(&[]).cloned()
    }

    /// The line index of `target` as it appears in `document`'s analysis closure,
    /// or `None` when `target` is not part of that closure.
    ///
    /// A cross-file [`NavigationTarget`] names a file in `document`'s import
    /// closure; converting its byte ranges to LSP positions needs that file's line
    /// index. This reuses `document`'s already-computed analysis rather than
    /// analyzing `target` as its own entry, which would both duplicate work and,
    /// for a non-entry file, resolve a different closure.
    #[must_use = "the line index is the reason to call this"]
    pub fn closure_line_index(&mut self, document: &Path, target: &Path) -> Option<LineIndex> {
        let analysis = self.db.analysis(document);
        analysis.arena().source_files().find_map(|source_file| {
            let closure_file = analysis.file(&source_file.module_path)?;
            (closure_file.path() == target).then(|| closure_file.line_index().clone())
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]

    use std::path::PathBuf;

    use crate::{AnalysisHost, LineCol};

    fn path() -> PathBuf {
        PathBuf::from("/inf-test/main.inf")
    }

    #[test]
    fn open_then_query_answers_features() {
        let mut host = AnalysisHost::default();
        let source = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        host.open_document(&path(), source);
        let mut analysis = host.analysis();
        assert!(analysis.diagnostics(&path()).is_empty());
        assert_eq!(analysis.document_symbols(&path()).len(), 1);
        let offset = source.find("add").expect("name present") as u32;
        assert!(analysis.hover(&path(), offset).is_some());
    }

    #[test]
    fn a_change_reanalyzes_and_clears_diagnostics() {
        let mut host = AnalysisHost::default();
        host.open_document(&path(), "fn f() -> i32 { return x; }");
        assert!(!host.analysis().diagnostics(&path()).is_empty());
        host.change_document(&path(), "fn f() -> i32 { return 1; }");
        assert!(host.analysis().diagnostics(&path()).is_empty());
    }

    #[test]
    fn close_does_not_panic_and_leaves_the_host_usable() {
        let mut host = AnalysisHost::default();
        host.open_document(&path(), "fn f() -> i32 { return 1; }");
        let _ = host.analysis().diagnostics(&path());
        host.close_document(&path());
        // Re-opening restores a clean analysis.
        host.open_document(&path(), "fn g() -> i32 { return 2; }");
        assert!(host.analysis().diagnostics(&path()).is_empty());
    }

    #[test]
    fn line_index_converts_offsets_to_positions() {
        let mut host = AnalysisHost::default();
        let source = "fn a() {}\nfn b() {}";
        host.open_document(&path(), source);
        let index = host.analysis().line_index(&path()).expect("a line index");
        let offset = source.find('b').expect("b present") as u32;
        assert_eq!(
            index.line_col(offset),
            LineCol {
                line: 1,
                character: 3
            }
        );
    }

    #[test]
    fn closure_line_index_serves_an_imported_file_without_re_analysis() {
        let mut host = AnalysisHost::default();
        let lib_path = PathBuf::from("/inf-test/lib.inf");
        let lib = "pub fn helper() -> i32 { return 7; }";
        host.open_document(&lib_path, lib);
        host.open_document(
            &path(),
            "use lib;\nfn main() -> i32 { return lib::helper(); }",
        );

        let index = host
            .analysis()
            .closure_line_index(&path(), &lib_path)
            .expect("the imported file is in the closure");
        let offset = lib.find("helper").expect("helper present") as u32;
        assert_eq!(
            index.line_col(offset),
            LineCol {
                line: 0,
                character: 7
            }
        );

        // A path outside the closure yields nothing.
        assert!(
            host.analysis()
                .closure_line_index(&path(), &PathBuf::from("/inf-test/nope.inf"))
                .is_none()
        );
    }
}
