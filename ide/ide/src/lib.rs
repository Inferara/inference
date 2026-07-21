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
//! and ranges in its own coordinates. Feature queries take `&self`: the analysis
//! is still computed lazily and memoized on first use, so a read mutates the
//! `RootDatabase`, but that mutation now runs behind a `RefCell` rather than a
//! `&mut` borrow, which is what lets the query surface be shared. The LSP main
//! loop is single-threaded and never holds one query across another, so the
//! interior borrow is always uncontended.

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

use std::cell::RefCell;
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

// Re-export the cancellation surface so the protocol layer binds a source and
// classifies a caught unwind through `inference-ide` alone, never naming the
// underlying semantic framework.
pub use inference_ide_db::{AnalysisCancelSource, is_cancellation};

/// Owns the editor's open documents and the analyses derived from them.
///
/// Construct with [`AnalysisHost::default`], mirror the editor's lifecycle with
/// [`open_document`](Self::open_document) / [`change_document`](Self::change_document)
/// / [`close_document`](Self::close_document), then take an [`Analysis`] to answer
/// feature queries.
#[derive(Default)]
pub struct AnalysisHost {
    db: RefCell<RootDatabase>,
}

impl AnalysisHost {
    /// Records `text` as the current contents of `path` (an editor `didOpen`).
    pub fn open_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        self.db.get_mut().open_document(path, text);
    }

    /// Replaces the current contents of the open document `path` (a `didChange`).
    pub fn change_document(&mut self, path: &Path, text: impl Into<Arc<str>>) {
        self.db.get_mut().change_document(path, text);
    }

    /// Drops the in-memory contents of `path` (a `didClose`); later analyses read
    /// it from disk again.
    pub fn close_document(&mut self, path: &Path) {
        self.db.get_mut().close_document(path);
    }

    /// Whether an analysis for `path` is currently memoized.
    ///
    /// After a change, an open document whose analysis is no longer memoized is
    /// one the change invalidated; the protocol layer uses this to republish
    /// exactly the affected documents rather than every open one.
    #[must_use = "the analyzed state is the reason to call this"]
    pub fn is_document_analyzed(&self, path: &Path) -> bool {
        self.db.borrow().is_analyzed(path)
    }

    /// Borrows the host to answer feature queries.
    #[must_use = "an Analysis does nothing until a query method is called"]
    pub fn analysis(&self) -> Analysis<'_> {
        Analysis { db: &self.db }
    }

    /// Binds `source` so a cancellation request from another thread interrupts
    /// this host's in-flight analysis at its next checkpoint. Rebind after
    /// replacing the host: the binding is per-database-handle.
    pub fn bind_cancellation(&self, source: &AnalysisCancelSource) {
        self.db.borrow().bind_cancellation(source);
    }
}

/// A shared `&self` view over the host that answers feature queries for a
/// document.
///
/// It holds a `&RefCell<RootDatabase>`, not a `&mut RootDatabase`: each query
/// method takes `&self` and opens a `borrow_mut` scoped to that single call,
/// releasing it before returning, so an `Analysis` holds no live borrow between
/// calls. The read still needs `&mut RootDatabase` underneath — the analysis is
/// computed lazily on first use and a read memoizes the result and drives
/// invalidation in place — but that mutation now runs behind the `RefCell`, which
/// is what makes the query surface shareable.
///
/// This is the shared-`&self` surface, *not* a parallel cloned-handle read
/// model. Salsa's `Storage` is `Clone`, but a genuine handle-clone is deferred
/// to the cancellation work: the read path still bumps a Salsa input on
/// never-opened eviction, and a write from one live handle blocks until every
/// other handle drops, so a cloned reader would not be freely concurrent yet.
///
/// The `RefCell` moves one invariant from compile time to run time: a query
/// method re-entered while another's `borrow_mut` is live panics the cell. It
/// holds under the single-threaded LSP loop because every query's borrow is
/// scoped to its own call. A second property — the host is not mutated while an
/// `Analysis` is live — stays compile-time-enforced: the `Analysis` lifetime is
/// a shared borrow of the host, which the `&mut self` write methods cannot
/// coexist with.
pub struct Analysis<'a> {
    db: &'a RefCell<RootDatabase>,
}

impl Analysis<'_> {
    /// The diagnostics for the open document `path`: syntax, import, type, and
    /// analysis-rule findings that belong to it.
    #[must_use = "the diagnostics are the reason to call this"]
    pub fn diagnostics(&self, path: &Path) -> Vec<Diagnostic> {
        diagnostics::diagnostics(self.db.borrow_mut().analysis(path))
    }

    /// The definition outline of the document `path`.
    #[must_use = "the symbols are the reason to call this"]
    pub fn document_symbols(&self, path: &Path) -> Vec<DocumentSymbol> {
        document_symbols::document_symbols(self.db.borrow_mut().analysis(path))
    }

    /// The hover for byte `offset` in the document `path`, if anything is there.
    #[must_use = "the hover is the reason to call this"]
    pub fn hover(&self, path: &Path, offset: u32) -> Option<Hover> {
        hover::hover(self.db.borrow_mut().analysis(path), offset)
    }

    /// The definition(s) of the identifier at byte `offset` in `path`, if any.
    #[must_use = "the navigation targets are the reason to call this"]
    pub fn goto_definition(&self, path: &Path, offset: u32) -> Option<Vec<NavigationTarget>> {
        goto_definition::goto_definition(self.db.borrow_mut().analysis(path), offset)
    }

    /// The completions for byte `offset` in the document `path`.
    #[must_use = "the completions are the reason to call this"]
    pub fn completions(&self, path: &Path, offset: u32) -> Vec<CompletionItem> {
        completions::completions(self.db.borrow_mut().analysis(path), offset)
    }

    /// The non-det inlay hints for `path`, optionally clipped to `range`.
    #[must_use = "the inlay hints are the reason to call this"]
    pub fn inlay_hints(&self, path: &Path, range: Option<TextRange>) -> Vec<InlayHint> {
        inlay_hints::inlay_hints(self.db.borrow_mut().analysis(path), range)
    }

    /// The line index of the document `path`, for byte-offset ↔ line/column
    /// conversion; `None` when the document is not analyzable.
    ///
    /// Returned as a shared [`Arc`] handle so repeated position queries against
    /// the same open document share one index rather than each copying the whole
    /// document's text.
    #[must_use = "the line index is the reason to call this"]
    pub fn line_index(&self, path: &Path) -> Option<Arc<LineIndex>> {
        self.db.borrow_mut().analysis(path).line_index_arc(&[])
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
    pub fn closure_line_index(&self, document: &Path, target: &Path) -> Option<Arc<LineIndex>> {
        let mut db = self.db.borrow_mut();
        let analysis = db.analysis(document);
        analysis.arena().source_files().find_map(|source_file| {
            let closure_file = analysis.file(&source_file.module_path)?;
            (closure_file.path() == target).then(|| closure_file.line_index_arc())
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
        let analysis = host.analysis();
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

    #[test]
    fn queries_answer_through_a_shared_borrow() {
        // The document is opened inside `single`'s own scoped `&mut` borrow;
        // here the host is bound without `mut`, so every call below reaches it
        // through a shared `&host`. Two `Analysis` values are held live from the
        // same shared borrow and each answers a query — this only type-checks
        // because `analysis` and the query methods take `&self`; a regression to
        // `&mut self` would fail to compile.
        let (host, path) =
            crate::test_utils::single("fn add(a: i32, b: i32) -> i32 { return a + b; }");
        let first = host.analysis();
        let second = host.analysis();
        assert!(first.diagnostics(&path).is_empty());
        assert_eq!(second.document_symbols(&path).len(), 1);
        assert!(host.is_document_analyzed(&path));
    }
}
