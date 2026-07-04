#![warn(clippy::pedantic)]
//! Semantic database for the Inference IDE stack.
//!
//! `ide-db` sits above `vfs` (path ↔ id ↔ content overlay) and `base-db`
//! (line-index and position PODs) and below the feature layer. It answers one
//! question — *"what does this open file mean?"* — by analyzing each open
//! document as its own project entry and caching the result.
//!
//! # What it owns
//!
//! * [`RootDatabase`] — the open-document overlay plus a memoized
//!   [`FileAnalysis`] per entry file, with closure-aware invalidation so a
//!   keystroke in one buffer does not re-analyze unrelated ones.
//! * [`FileAnalysis`] — the merged arena (via its [`TypedContext`]), per-file
//!   parse errors, structured type diagnostics, unresolved-import problems,
//!   tagged analysis findings, and per-closure-file line indexes and paths.
//!
//! # What it does not do
//!
//! It leaks no protocol types: every result is compiler data or a plain struct.
//! The feature layer above translates these into LSP responses. Import
//! resolution is **not** reimplemented here — the closure walk lives in
//! `core/inference` behind a reader seam, and `ide-db` drives it with an
//! overlay-then-disk loader, so the compiler and the IDE resolve imports
//! identically.
//!
//! [`TypedContext`]: inference_type_checker::typed_context::TypedContext

mod analysis;
mod database;
mod hit_test;
mod loader;
mod symbols;

pub use analysis::{AnalysisFinding, ClosureFile, FileAnalysis};
pub use database::RootDatabase;
pub use hit_test::{NodeHit, hit_test};
pub use symbols::file_defs;

// Re-export the lower IDE layers' position primitives so the feature layer can
// depend on ide-db alone for everything it needs to describe a location.
pub use inference_base_db::{FilePosition, FileRange, LineCol, LineIndex, TextRange};
pub use inference_vfs::{FileId, Vfs};

// Re-export the compiler types that appear in `FileAnalysis`'s public results so
// a consumer names them through ide-db alone. `ide-db` is the single façade the
// feature layer above depends on.
pub use inference::{FileParseErrors, ImportProblem};
pub use inference_analysis::errors::{AnalysisDiagnostic, LabeledDiagnostic, Severity};
pub use inference_ast::ids::{DefId, NodeId, SourceFileId};
pub use inference_ast::nodes::Location;
pub use inference_parser::ParseError;
pub use inference_type_checker::TypeCheckDiagnostic;
pub use inference_type_checker::errors::TypeCheckError;
