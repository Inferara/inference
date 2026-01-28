//! Arena-based Abstract Syntax Tree (AST) for the Inference compiler.
//!
//! This crate provides a memory-efficient AST representation with arena-based allocation,
//! ID-based node references, and O(1) parent-child traversal. All AST nodes are stored in
//! a central `Arena` with fast hash map lookups.
//!
//! # Quick Start
//!
//! ```no_run
//! use inference_ast::builder::Builder;
//! use tree_sitter::Parser;
//!
//! let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
//! let mut parser = Parser::new();
//! parser.set_language(&tree_sitter_inference::language()).unwrap();
//! let tree = parser.parse(source, None).unwrap();
//!
//! let mut builder = Builder::new();
//! builder.add_source_code(tree.root_node(), source.as_bytes());
//! let arena = builder.build_ast().unwrap();
//!
//! // Query the arena
//! let functions = arena.functions();
//! for func in functions {
//!     println!("Function: {}", func.name.name);
//! }
//! ```
//!
//! # Core Components
//!
//! - [`arena::Arena`] - Central storage for all AST nodes with O(1) lookups
//! - [`builder::Builder`] - Builds AST from tree-sitter concrete syntax tree
//! - [`nodes`] - AST node type definitions (`SourceFile`, `FunctionDefinition`, etc.)
//! - [`extern_prelude`] - External module discovery and parsing
//! - [`parser_context::ParserContext`] - Multi-file parsing context (WIP)
//! - [`errors`] - Structured error types for AST operations
//!
//! # Key Features
//!
//! - **ID-based references**: Nodes reference each other by `u32` ID, not pointers
//! - **Efficient traversal**: O(1) parent and children lookups via hash maps
//! - **Zero-copy locations**: Lightweight byte offset tracking with line/column info
//! - **Type-safe nodes**: Strongly-typed enums with exhaustive matching
//! - **Primitive type enums**: `SimpleTypeKind` for fast type checking without string comparison
//!
//! # Architecture
//!
//! The AST uses a three-tier storage system in the Arena:
//!
//! 1. **Node Storage** (`nodes: FxHashMap<u32, AstNode>`) - Maps IDs to nodes
//! 2. **Parent Map** (`parent_map: FxHashMap<u32, u32>`) - Child ID → Parent ID
//! 3. **Children Map** (`children_map: FxHashMap<u32, Vec<u32>>`) - Parent ID → Children IDs
//!
//! This provides O(1) lookups for nodes, parents, and children lists.
//!
//! See the [README](https://github.com/Inferara/inference/blob/main/core/ast/README.md)
//! and [architecture documentation](https://github.com/Inferara/inference/blob/main/core/ast/docs/architecture.md)
//! for detailed design rationale and usage examples.

#![warn(clippy::pedantic)]
pub mod arena;
pub mod builder;
pub(crate) mod enums_impl;
pub mod errors;
pub mod extern_prelude;
pub mod nodes;
pub(crate) mod nodes_impl;
pub mod parser_context;
