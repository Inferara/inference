//! External module discovery and parsing.
//!
//! This module handles finding and parsing external module source files.
//! It returns parsed ASTs that can then be integrated into the symbol table
//! by the type-checker.

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::arena::Arena;

/// Represents a parsed external module
#[derive(Clone)]
pub struct ParsedModule {
    /// The name of the module (e.g., "std", "core")
    pub name: String,
    /// The parsed AST arena for this module
    pub arena: Arena,
    /// The root file path
    pub root_path: PathBuf,
}

/// Registry of parsed external modules
/// Maps module name to its parsed AST
pub type ExternPrelude = FxHashMap<String, ParsedModule>;

/// Parse an external module and add it to the prelude
///
/// TODO: Implement in Phase 5 when external prelude is ready
///
/// # Arguments
/// * `module_dir` - Path to the module's root directory
/// * `name` - Name of the module
/// * `prelude` - The prelude registry to insert into
#[allow(dead_code)]
pub fn parse_external_module(
    _module_dir: &Path,
    _name: &str,
    _prelude: &mut ExternPrelude,
) {
    // TODO: Implement me
    // Reference pattern:
    // let name = name.replace('-', "_");
    // if prelude.contains_key(&name) { return; }
    // if let Some(root_path) = find_module_root(module_dir) {
    //     let source = std::fs::read_to_string(&root_path).ok()?;
    //     let arena = crate::builder::build_ast(source);
    //     prelude.insert(name.clone(), ParsedModule { name, arena, root_path });
    // }
}

/// Find the root source file for a module
///
/// Searches for the main entry point of a module in standard locations.
///
/// TODO: Implement in Phase 5
#[must_use]
pub fn find_module_root(_module_dir: &Path) -> Option<PathBuf> {
    // TODO: Implement me - search for:
    // 1. {module_dir}/src/lib.inf
    // 2. {module_dir}/src/main.inf
    // 3. {module_dir}/lib.inf
    None
}

/// Create an empty prelude
///
/// The prelude can be populated by calling `parse_external_module` for each
/// external dependency.
#[must_use]
pub fn create_empty_prelude() -> ExternPrelude {
    FxHashMap::default()
}
