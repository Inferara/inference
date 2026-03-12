//! Multi-file AST parsing context.
//!
//! Manages parsing across multiple source files, handling module resolution
//! and building a unified AST with proper scope relationships.
//!
//! # Status
//!
//! **Work in Progress** - This module provides the skeleton for multi-file support
//! but is not yet functional.

use std::path::PathBuf;

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

#[must_use]
pub fn find_submodule_path(_current_file: &PathBuf, _module_name: &str) -> Option<PathBuf> {
    None
}
