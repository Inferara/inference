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
//! # Query surfaces
//!
//! Each open file is analyzed as its own project entry (its import closure
//! resolved through the overlay-then-disk loader in `ide-db`), and the resulting
//! analysis answers every query for that document — including goto-definition
//! into an imported file, whose [`NavigationTarget`] carries that file's real path
//! and ranges in its own coordinates. There are two ways to reach that analysis:
//!
//! * The **worker surface** — [`AnalysisHost::analysis`] hands out an [`Analysis`]
//!   whose feature queries take `&self`, computing lazily and memoizing on first
//!   use behind a `RefCell`. The worker thread drives it and never holds one query
//!   across another, so the interior borrow is always uncontended.
//! * The **snapshot surface** (#292) — [`AnalysisHost::plan_concurrent_read`]
//!   decides whether a request can be served off the worker. When it can, it mints
//!   an [`AnalysisSnapshot`] (a `Send` per-request handle) a pool thread serves
//!   into a [`DocumentAnalysis`] carrying the same plain-old-data answers; the
//!   worker later folds the result back with [`AnalysisHost::apply_concurrent_read`].
//!   This surface is purely additive — it does not touch the `RefCell` interior,
//!   and the worker surface above is unchanged.

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

use inference_ide_db::{ConcurrentReadPlan, FileAnalysis, ReadServe, ReadSnapshot, RootDatabase};

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

    /// Decides whether a feature request for `path` can be served off the worker on
    /// a pool thread (#292), returning a [`ReadPlan`].
    ///
    /// Borrows the `RefCell` immutably; the returned [`AnalysisSnapshot`] carries a
    /// cloned database handle and is `Send`, so a pool thread can serve it while the
    /// worker moves on.
    #[must_use = "the plan decides where the read runs"]
    pub fn plan_concurrent_read(&self, path: &Path, source: &AnalysisCancelSource) -> ReadPlan {
        match self.db.borrow().plan_concurrent_read(path, source) {
            ConcurrentReadPlan::Serial => ReadPlan::Serial,
            ConcurrentReadPlan::Concurrent(snapshot) => {
                ReadPlan::Concurrent(AnalysisSnapshot(snapshot))
            }
        }
    }

    /// Folds a pool-served analysis back into the worker's entry mirror (#292),
    /// guarded against any write that superseded the read (see
    /// [`RootDatabase::apply_concurrent_read`](inference_ide_db::RootDatabase::apply_concurrent_read)).
    pub fn apply_concurrent_read(
        &mut self,
        path: &Path,
        doc: &DocumentAnalysis,
        dispatch_epoch: u64,
        source: &AnalysisCancelSource,
    ) {
        self.db
            .get_mut()
            .apply_concurrent_read(path, &doc.analysis, dispatch_epoch, source);
    }

    /// Runs the deferred never-opened bookkeeping for a pool-recomputed `path`
    /// (#292); call only when no concurrent reads are in flight.
    pub fn apply_unopened_read_bookkeeping(&mut self, path: &Path) {
        self.db.get_mut().apply_unopened_read_bookkeeping(path);
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
/// This is the worker's shared-`&self` surface. The parallel cloned-handle read
/// model lives beside it (#292): [`AnalysisHost::plan_concurrent_read`] mints an
/// [`AnalysisSnapshot`] a pool thread serves without touching this `RefCell`, so
/// the two surfaces never contend. This one stays the worker's, driven from the
/// single worker thread.
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

/// The worker's decision for a snapshot read (#292): serve serially on the worker,
/// or hand an [`AnalysisSnapshot`] to a pool thread.
///
// The `Concurrent` payload carries a cloned database handle; the plan is created
// on the worker and immediately matched, then the snapshot is moved once into the
// read task, so boxing would only add an allocation on the common path.
#[allow(clippy::large_enum_variant)]
pub enum ReadPlan {
    /// Serve serially on the worker.
    Serial,
    /// Serve off this snapshot on a pool thread.
    Concurrent(AnalysisSnapshot),
}

/// A `Send` per-request read handle a pool thread serves off the worker (#292).
///
/// Wraps `ide-db`'s snapshot: serving runs the analysis query against a cloned
/// database handle and drops it before returning a [`DocumentAnalysis`].
pub struct AnalysisSnapshot(ReadSnapshot);

impl AnalysisSnapshot {
    /// The write epoch this snapshot was dispatched under, used to guard the
    /// worker-side fold-back (see [`AnalysisHost::apply_concurrent_read`]).
    #[must_use]
    pub fn dispatch_epoch(&self) -> u64 {
        self.0.dispatch_epoch()
    }

    /// Serves this snapshot on the current (pool) thread, consuming it.
    ///
    /// The cloned database handle drops before this returns, so the read never
    /// holds it across the response the caller sends.
    #[must_use = "the served document is the reason to serve"]
    pub fn serve(self) -> SnapshotServe {
        match self.0.serve() {
            ReadServe::Ready {
                analysis,
                recomputed,
            } => SnapshotServe::Ready(DocumentAnalysis {
                analysis,
                recomputed,
            }),
            ReadServe::NotServable => SnapshotServe::NotServable,
        }
    }
}

/// The outcome of serving an [`AnalysisSnapshot`].
pub enum SnapshotServe {
    /// The document's analysis, ready to answer feature queries.
    Ready(DocumentAnalysis),
    /// The entry was evicted between plan and serve; route the request back to the
    /// worker for serial service.
    NotServable,
}

/// A pool-served analysis for one document (#292).
///
/// Answers the same plain-old-data feature queries an [`Analysis`] does, from an
/// owned [`FileAnalysis`] handle rather than a borrow of the host — so a pool
/// thread can answer without touching the worker's `RefCell`. `Send + Sync`.
pub struct DocumentAnalysis {
    analysis: Arc<FileAnalysis>,
    recomputed: bool,
}

impl DocumentAnalysis {
    /// Whether serving re-executed the analysis (a stale recompute) rather than
    /// hitting the worker's stored memo — the worker uses this to decide whether
    /// the document's diagnostics need republishing.
    #[must_use]
    pub fn recomputed(&self) -> bool {
        self.recomputed
    }

    /// The diagnostics for this document: syntax, import, type, and analysis-rule
    /// findings that belong to it.
    #[must_use = "the diagnostics are the reason to call this"]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        diagnostics::diagnostics(&self.analysis)
    }

    /// The definition outline of this document.
    #[must_use = "the symbols are the reason to call this"]
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        document_symbols::document_symbols(&self.analysis)
    }

    /// The hover for byte `offset`, if anything is there.
    #[must_use = "the hover is the reason to call this"]
    pub fn hover(&self, offset: u32) -> Option<Hover> {
        hover::hover(&self.analysis, offset)
    }

    /// The definition(s) of the identifier at byte `offset`, if any.
    #[must_use = "the navigation targets are the reason to call this"]
    pub fn goto_definition(&self, offset: u32) -> Option<Vec<NavigationTarget>> {
        goto_definition::goto_definition(&self.analysis, offset)
    }

    /// The completions for byte `offset`.
    #[must_use = "the completions are the reason to call this"]
    pub fn completions(&self, offset: u32) -> Vec<CompletionItem> {
        completions::completions(&self.analysis, offset)
    }

    /// The non-det inlay hints, optionally clipped to `range`.
    #[must_use = "the inlay hints are the reason to call this"]
    pub fn inlay_hints(&self, range: Option<TextRange>) -> Vec<InlayHint> {
        inlay_hints::inlay_hints(&self.analysis, range)
    }

    /// The line index of this document, for byte-offset ↔ line/column conversion;
    /// `None` when the document is not analyzable.
    #[must_use = "the line index is the reason to call this"]
    pub fn line_index(&self) -> Option<Arc<LineIndex>> {
        self.analysis.line_index_arc(&[])
    }

    /// The line index of `target` as it appears in this document's analysis
    /// closure, or `None` when `target` is not part of that closure (mirrors
    /// [`Analysis::closure_line_index`]).
    #[must_use = "the line index is the reason to call this"]
    pub fn closure_line_index(&self, target: &Path) -> Option<Arc<LineIndex>> {
        let analysis = &self.analysis;
        analysis.arena().source_files().find_map(|source_file| {
            let closure_file = analysis.file(&source_file.module_path)?;
            (closure_file.path() == target).then(|| closure_file.line_index_arc())
        })
    }
}

const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_send_sync<T: Send + Sync>() {}
    // The snapshot crosses to a pool thread; the served document is shared back.
    assert_send::<AnalysisSnapshot>();
    assert_send_sync::<DocumentAnalysis>();
};

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
    fn a_snapshot_answers_every_query_like_the_analysis_surface() {
        // The pool-served DocumentAnalysis routes to the same feature functions the
        // worker's Analysis does, so a Concurrent plan's answers match the worker's
        // for the same document (#292).
        use crate::{AnalysisCancelSource, ReadPlan, SnapshotServe};

        let mut host = AnalysisHost::default();
        let lib_path = PathBuf::from("/inf-test/lib.inf");
        let lib = "pub fn helper() -> i32 { return 7; }";
        let main = "use lib;\nfn main() -> i32 { return lib::helper(); }";
        host.open_document(&lib_path, lib);
        host.open_document(&path(), main);
        let source = AnalysisCancelSource::detached();
        host.bind_cancellation(&source);

        // Memoize main so a plan is Concurrent (a hit).
        let worker = host.analysis();
        let want_diagnostics = worker.diagnostics(&path()).len();
        let want_symbols = worker.document_symbols(&path()).len();
        let hover_offset = main.find("helper").expect("helper present") as u32;
        let want_hover = worker.hover(&path(), hover_offset).is_some();
        let want_goto = worker.goto_definition(&path(), hover_offset).is_some();
        let want_completions = worker.completions(&path(), hover_offset).len();
        let want_inlays = worker.inlay_hints(&path(), None).len();
        let want_line_index = worker.line_index(&path()).is_some();
        let want_closure = worker.closure_line_index(&path(), &lib_path).is_some();

        let ReadPlan::Concurrent(snapshot) = host.plan_concurrent_read(&path(), &source) else {
            panic!("a memoized document plans Concurrent");
        };
        let SnapshotServe::Ready(doc) = snapshot.serve() else {
            panic!("a hit serves Ready");
        };

        assert!(!doc.recomputed(), "a hit is not a recompute");
        assert_eq!(doc.diagnostics().len(), want_diagnostics);
        assert_eq!(doc.document_symbols().len(), want_symbols);
        assert_eq!(doc.hover(hover_offset).is_some(), want_hover);
        assert_eq!(doc.goto_definition(hover_offset).is_some(), want_goto);
        assert_eq!(doc.completions(hover_offset).len(), want_completions);
        assert_eq!(doc.inlay_hints(None).len(), want_inlays);
        assert_eq!(doc.line_index().is_some(), want_line_index);
        assert_eq!(doc.closure_line_index(&lib_path).is_some(), want_closure);
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
