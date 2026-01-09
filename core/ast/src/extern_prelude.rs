//! External module discovery and parsing.
//!
//! This module handles finding and parsing external module source files.
//! It returns parsed ASTs that can then be integrated into the symbol table
//! by the type-checker.

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::arena::Arena;
use crate::builder::Builder;
use crate::errors::AstError;

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

/// Create an empty prelude
///
/// The prelude can be populated by calling `parse_external_module` for each
/// external dependency.
#[must_use]
pub fn create_empty_prelude() -> ExternPrelude {
    FxHashMap::default()
}

/// Parse an external module and add it to the prelude.
///
/// Locates the module's root source file using `find_module_root`, parses it,
/// and adds the resulting AST to the prelude registry.
///
/// Module names are normalized: hyphens are replaced with underscores to match
/// Inference's convention for crate names.
///
/// # Arguments
/// * `module_dir` - Path to the module's root directory
/// * `name` - Name of the module
/// * `prelude` - The prelude registry to insert into
///
/// # Errors
/// Returns an error if:
/// - No module root file is found in standard locations
/// - The source file cannot be read
/// - The source code fails to parse
///
/// # Panics
/// Panics if the Inference grammar fails to load (should never happen with valid tree-sitter setup).
///
/// # Example
/// ```ignore
/// use inference_ast::extern_prelude::{create_empty_prelude, parse_external_module};
/// use std::path::Path;
///
/// let mut prelude = create_empty_prelude();
/// parse_external_module(Path::new("/path/to/mylib"), "mylib", &mut prelude)?;
/// ```
pub fn parse_external_module(
    module_dir: &Path,
    name: &str,
    prelude: &mut ExternPrelude,
) -> anyhow::Result<()> {
    let normalized_name = name.replace('-', "_");

    if prelude.contains_key(&normalized_name) {
        return Ok(());
    }

    let root_path = find_module_root(module_dir).ok_or_else(|| AstError::ModuleRootNotFound {
        path: module_dir.to_path_buf(),
        expected: format!(
            "src{}lib.inf or src{}main.inf",
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        ),
    })?;

    let source = std::fs::read_to_string(&root_path).map_err(|e| AstError::FileReadError {
        path: root_path.clone(),
        source: e,
    })?;

    let inference_language = tree_sitter_inference::language();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&inference_language)
        .expect("Error loading Inference grammar");

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| AstError::ParseError {
            path: root_path.clone(),
        })?;

    let mut builder = Builder::new();
    builder.add_source_code(tree.root_node(), source.as_bytes());
    let completed = builder.build_ast().map_err(|e| AstError::AstBuildError {
        path: root_path.clone(),
        reason: e.to_string(),
    })?;

    let arena = completed.arena();

    prelude.insert(
        normalized_name.clone(),
        ParsedModule {
            name: normalized_name,
            arena,
            root_path,
        },
    );

    Ok(())
}
