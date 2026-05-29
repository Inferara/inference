//! Multi-file AST parsing context.
//!
//! Manages parsing across multiple source files, handling module resolution
//! and building a unified AST with proper scope relationships.
//!
//! # Status
//!
//! **Work in Progress** - This module provides the skeleton for multi-file support
//! but is not yet functional.

use std::path::{Path, PathBuf};

use crate::arena::AstArena;

/// Queue entry for pending file parsing.
#[allow(dead_code)]
struct ParseQueueEntry {
    scope_id: u32,
    file_path: PathBuf,
}

/// Context for parsing multiple source files.
#[allow(dead_code)]
pub struct ParserContext {
    next_id: u32,
    queue: Vec<ParseQueueEntry>,
    arena: AstArena,
}

impl ParserContext {
    #[must_use]
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            next_id: 0,
            queue: vec![ParseQueueEntry {
                scope_id: 0,
                file_path: root_path,
            }],
            arena: AstArena::default(),
        }
    }

    #[allow(clippy::unused_self)]
    pub fn push_file(&mut self, _scope_id: u32, _file_path: PathBuf) {}

    #[must_use]
    pub fn parse_all(&mut self) -> AstArena {
        std::mem::take(&mut self.arena)
    }

    #[allow(dead_code)]
    fn next_node_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Resolves a module name to its source file path relative to the current file.
///
/// Looks for `{dir}/{module_name}.inf` in the directory that contains `current_file`
/// and returns it only if that file exists. Returns `None` when `current_file` has no
/// parent directory or the candidate file is absent.
///
/// This is a standalone path-resolution helper with no AST, scope, or grammar coupling,
/// pending the file-module model tracked in #63.
#[must_use]
pub fn find_submodule_path(current_file: &Path, module_name: &str) -> Option<PathBuf> {
    let candidate = current_file.parent()?.join(format!("{module_name}.inf"));
    candidate.exists().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::find_submodule_path;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn resolves_sibling_inf_file_when_it_exists() {
        let dir = tempdir().unwrap();
        let current = dir.path().join("main.inf");
        fs::write(&current, "").unwrap();
        let module = dir.path().join("math.inf");
        fs::write(&module, "").unwrap();

        let resolved = find_submodule_path(&current, "math");

        assert_eq!(resolved, Some(module));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn returns_none_when_candidate_is_absent() {
        let dir = tempdir().unwrap();
        let current = dir.path().join("main.inf");
        fs::write(&current, "").unwrap();

        assert_eq!(find_submodule_path(&current, "missing"), None);
    }

    #[test]
    fn returns_none_when_current_file_has_no_parent() {
        assert_eq!(find_submodule_path(std::path::Path::new(""), "math"), None);
    }
}
