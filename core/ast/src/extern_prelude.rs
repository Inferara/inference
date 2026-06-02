//! External module discovery.
//!
//! This module handles finding external module source files and modelling the
//! parsed ASTs that the type-checker integrates into its symbol table. Parsing
//! itself lives in the `inference` orchestration crate (`inference::extern_prelude`),
//! which owns the parser dependency; `inference-ast` only describes the data and
//! locates module roots.

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::arena::AstArena;

/// Represents a parsed external module
#[derive(Clone)]
pub struct ParsedModule {
    /// The name of the module (e.g., "std", "core")
    pub name: String,
    /// The parsed AST arena for this module
    pub arena: AstArena,
    /// The root file path
    pub root_path: PathBuf,
}

/// Registry of parsed external modules
/// Maps module name to its parsed AST
pub type ExternPrelude = FxHashMap<String, ParsedModule>;

/// Find the root source file for a module
///
/// Searches for the main entry point of a module in standard locations:
/// 1. `{module_dir}/src/lib.inf`
/// 2. `{module_dir}/src/main.inf`
///
/// Returns the first path that exists, or `None` if no root file is found.
#[must_use = "discarding the result loses the found path"]
pub fn find_module_root(module_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        module_dir.join("src").join("lib.inf"),
        module_dir.join("src").join("main.inf"),
    ];

    candidates.into_iter().find(|p| p.exists())
}

/// Create an empty prelude.
///
/// The prelude can be populated by calling `inference::extern_prelude::parse_external_module`
/// for each external dependency.
#[must_use]
pub fn create_empty_prelude() -> ExternPrelude {
    FxHashMap::default()
}
