//! Arena-based Abstract Syntax Tree (AST) for the Inference compiler.
//!
//! This crate provides a memory-efficient AST representation with typed arena
//! allocation and index-based node references. All AST nodes are stored in
//! a central `AstArena` with O(1) typed index lookups.
//!
//! # Key Features
//!
//! - **Typed indices**: `ExprId`, `StmtId`, `DefId` etc. prevent mixing categories
//! - **Vec-based storage**: Cache-friendly sequential allocation
//! - **Send + Sync**: No `RefCell` or `Arc` — safe for parallel analysis
//! - **Zero-copy locations**: Lightweight byte offset tracking with line/column info

#![warn(clippy::pedantic)]
pub mod arena;
pub mod builder;
pub mod errors;
pub mod extern_prelude;
pub mod ids;
#[allow(clippy::pedantic)]
pub mod la_arena;
pub mod nodes;
mod nodes_impl;
pub mod parser_context;
