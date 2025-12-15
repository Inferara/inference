#![warn(clippy::pedantic)]
pub mod hir;
pub mod nodes;
mod nodes_impl;
pub mod type_infer;
// mod type_inference;
pub mod arena;
pub mod type_info;
