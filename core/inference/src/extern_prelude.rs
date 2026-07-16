//! External module parsing.
//!
//! [`inference_ast::extern_prelude`] models parsed external modules and locates
//! their root source files, but it intentionally does not depend on the parser.
//! This module owns the parsing step: it reads a module's root file, parses it
//! with [`inference_parser`], and records the resulting AST in the prelude the
//! type-checker consumes.

use std::path::Path;

use inference_ast::errors::AstError;
use inference_ast::extern_prelude::{ExternPrelude, ParsedModule, find_module_root};

/// Parse an external module and add it to the prelude.
///
/// Locates the module's root source file using
/// [`find_module_root`](inference_ast::extern_prelude::find_module_root), parses
/// it, and adds the resulting AST to the prelude registry.
///
/// Module names are normalized: hyphens are replaced with underscores to match
/// Inference's convention for crate names.
///
/// # Errors
/// Returns an error if the module root is not found, the source cannot be read,
/// or the source contains syntax errors.
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

    let source = crate::read_source_file(&root_path).map_err(|e| AstError::FileReadError {
        path: root_path.clone(),
        source: e,
    })?;

    let arena = crate::parse(&source).map_err(|e| AstError::AstBuildError {
        path: root_path.clone(),
        reason: e.to_string(),
    })?;

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
