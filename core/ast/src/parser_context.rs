//! Multi-file AST parsing context.
//!
//! Manages parsing across multiple source files, handling module resolution
//! and building a unified AST with proper scope relationships.
//!
//! # Status
//!
//! **Work in Progress** - This module provides the skeleton for multi-file support
//! but is not yet functional. See CLAUDE.md: "Multi-file support not yet implemented."
//!
//! # Planned Implementation
//!
//! The parsing context will:
//! 1. Initialize with a root file path
//! 2. Process the queue of files, building AST for each
//! 3. Handle module declarations (`mod name;` and `mod name { ... }`)
//! 4. Resolve submodule file paths following Inference conventions
//!
//! Reference implementation patterns are preserved in function doc comments.

use std::path::PathBuf;
use std::rc::Rc;

use crate::arena::Arena;
use crate::builder::Builder;
use crate::nodes::{Definition, ModuleDefinition};
use tree_sitter::Parser;

/// Queue entry for pending file parsing.
#[allow(dead_code)]
struct ParseQueueEntry {
    /// The scope this file belongs to.
    scope_id: u32,
    /// Path to the source file.
    file_path: PathBuf,
}

/// Context for parsing multiple source files.
///
/// Maintains a queue of files to parse and tracks the relationships
/// between modules and their source files.
#[allow(dead_code)]
pub struct ParserContext {
    /// Current node ID counter.
    next_id: u32,
    /// Queue of files pending parsing.
    queue: Vec<ParseQueueEntry>,
    /// The arena being built.
    arena: Arena,
}

impl ParserContext {
    /// Creates a new parser context starting from a root file.
    ///
    /// The root file is added to the parse queue with scope ID 0 (root scope).
    #[must_use]
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            next_id: 0,
            queue: vec![ParseQueueEntry {
                scope_id: 0,
                file_path: root_path,
            }],
            arena: Arena::default(),
        }
    }

    /// Pushes a new file onto the parse queue for submodule resolution.
    ///
    /// # Planned Implementation
    ///
    /// Will add the file to the queue with its parent scope ID, enabling
    /// proper scope relationships when the file is parsed.
    pub fn push_file(&mut self, scope_id: u32, file_path: PathBuf) {
        self.queue.push(ParseQueueEntry { scope_id, file_path });
    }

    /// Parses all queued files and builds the unified AST.
    ///
    /// # Planned Implementation
    ///
    /// ```text
    /// while let Some(entry) = self.queue.pop() {
    ///     let ast_file = self.parse_file(&entry.file_path);
    ///     for child in ast_file.children {
    ///         match child {
    ///             Directive::Use(u) => { /* add to scope imports */ }
    ///             Definition::Module(m) => { self.process_module(m, entry.scope_id); }
    ///             _ => { /* process other definitions */ }
    ///         }
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn parse_all(&mut self) -> Arena {
        while let Some(entry) = self.queue.pop() {
            let Some(file_arena) = Self::parse_file(&entry.file_path) else {
                continue;
            };

            for source_file in file_arena.source_files() {
                for definition in &source_file.definitions {
                    if let Definition::Module(module_definition) = definition {
                        self.process_module(module_definition, entry.scope_id, &entry.file_path);
                    }
                }
            }

            let Arena {
                nodes,
                parent_map,
                children_map,
            } = file_arena;
            self.arena.nodes.extend(nodes);
            self.arena.parent_map.extend(parent_map);
            for (parent_id, children) in children_map {
                self.arena
                    .children_map
                    .entry(parent_id)
                    .or_default()
                    .extend(children);
            }
        }
        std::mem::take(&mut self.arena)
    }

    /// Resolves and processes a module definition.
    ///
    /// # Planned Implementation
    ///
    /// Handles both external and inline module declarations:
    ///
    /// ```text
    /// if module.body.is_none() {
    ///     // External module: `mod name;` - find the file
    ///     let mod_path = find_submodule_path(current_file_path, &module.name);
    ///     let mod_scope = create_child_scope(parent_scope_id, &module.name);
    ///     self.push_file(mod_scope.id, mod_path);
    /// } else {
    ///     // Inline module: `mod name { ... }`
    ///     let mod_scope = create_child_scope(parent_scope_id, &module.name);
    ///     for def in &module.body {
    ///         self.process_definition(def, mod_scope.id);
    ///     }
    /// }
    /// ```
    #[allow(dead_code, clippy::unused_self)]
    fn process_module(
        &mut self,
        module: &Rc<ModuleDefinition>,
        _parent_scope_id: u32,
        current_file_path: &PathBuf,
    ) {
        let module_scope_id = self.next_node_id();

        if module.body.is_none() {
            if let Some(mod_path) = find_submodule_path(current_file_path, &module.name()) {
                self.push_file(module_scope_id, mod_path);
            }
            return;
        }

        if let Some(body) = &module.body {
            for definition in body {
                if let Definition::Module(child_module) = definition {
                    self.process_module(child_module, module_scope_id, current_file_path);
                }
            }
        }
    }

    /// Generates a new unique node ID.
    #[allow(dead_code)]
    fn next_node_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn parse_file(file_path: &PathBuf) -> Option<Arena> {
        let source = std::fs::read_to_string(file_path).ok()?;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_inference::language()).ok()?;
        let tree = parser.parse(&source, None)?;

        let mut builder = Builder::new();
        builder.add_source_code(tree.root_node(), source.as_bytes());
        builder.build_ast().ok()
    }
}

/// Finds the path to a submodule file.
///
/// # Planned Implementation
///
/// Searches for submodule files in the following order:
/// 1. `{current_dir}/{module_name}.inf`
/// 2. `{current_dir}/{module_name}/mod.inf`
///
/// Returns the first path that exists, or `None` if no candidate is found.
#[must_use]
pub fn find_submodule_path(current_file: &PathBuf, module_name: &str) -> Option<PathBuf> {
    let current_dir = current_file.parent()?;
    let file_candidate = current_dir.join(format!("{module_name}.inf"));
    if file_candidate.exists() {
        return Some(file_candidate);
    }
    let mod_candidate = current_dir.join(module_name).join("mod.inf");
    if mod_candidate.exists() {
        return Some(mod_candidate);
    }
    None
}
