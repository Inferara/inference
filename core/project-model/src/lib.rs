#![warn(clippy::pedantic)]
//! The shared project front end for the Inference toolchain: the one place that
//! turns an entry `.inf` file into the set of files a program is made of.
//!
//! A program is more than a single source file — a `use a::b;` directive pulls
//! `<src_root>/a/b.inf` into the compilation unit, transitively. This crate owns
//! that walk: given an entry file and a [`FileLoader`], it discovers the
//! import-reachable closure, reads and parses each file exactly once, and lowers
//! them all into one [`AstArena`]. It also owns the small pieces of project
//! structure the walk needs — reading a source file (stripping a UTF-8 BOM) and
//! deriving a project's source root from its `Inference.toml` manifest.
//!
//! # Why a leaf crate
//!
//! The compiler and the IDE must never disagree about which files a program
//! imports or how they are lowered — a diagnostic the IDE shows has to match what
//! the compiler would produce. The guarantee that they agree is *structural*:
//! there is exactly one closure-walk implementation, parameterized over where
//! bytes come from through the [`FileLoader`] seam. The compiler drives it with a
//! [`DiskLoader`] (straight to `std::fs`) via [`parse_project`]; the IDE drives it
//! with an overlay-then-disk loader via [`load_project_resilient`], so an open,
//! unsaved buffer shadows on-disk contents while both resolve imports the same
//! way.
//!
//! Keeping this front end a leaf — depending only on `inference-parser`,
//! `inference-ast`, `toml`, and `rustc-hash` — is what lets `ide-db` reach it
//! without transitively linking the WASM/Rocq backend (codegen, the translator,
//! the linker). The `inference` orchestration crate re-exports every item here, so
//! compiler-side consumers keep reaching them as `inference::…` unchanged.
//!
//! # Two entry points, one walk
//!
//! - [`parse_project`] — the compiler front end. Fails fast on the first problem,
//!   preserving the exact errors and ordering the compiler has always produced,
//!   and additionally reports [`ProjectWarning`]s for unreachable files.
//! - [`load_project_resilient`] / [`load_project_resilient_with_root`] — the IDE
//!   front end. Never fails fast: every file is parsed resiliently and every
//!   problem (a syntax error, an unresolved import, an unreadable file) is
//!   collected as data in a [`ResilientProjectParse`] so an editor can serve
//!   features on the healthy parts of a broken program.
//!
//! [`AstArena`]: inference_ast::arena::AstArena

pub mod errors;
pub mod manifest;
mod project;

pub use errors::InferenceError;
pub use manifest::manifest_source_root;
pub use project::{
    DiskLoader, FileLoader, FileParseErrors, ImportProblem, LoadedFile, ProjectParse,
    ProjectWarning, ResilientProjectParse, load_project_resilient,
    load_project_resilient_with_root, parse_project, read_source_file, strip_utf8_bom,
};
