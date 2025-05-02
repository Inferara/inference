#![warn(clippy::pedantic)]
pub(crate) mod arena;
pub mod builder;
pub mod symbols;
pub mod t_ast;
pub mod type_infer;
// pub(crate) mod type_inference;
pub mod types;
pub(crate) mod types_impl;
