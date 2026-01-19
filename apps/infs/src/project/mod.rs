#![warn(clippy::pedantic)]

//! Project management module.
//!
//! This module provides functionality for creating and managing Inference
//! projects, including manifest handling, project scaffolding, and templates.
//!
//! ## Modules
//!
//! - [`manifest`] - Inference.toml parsing and validation
//! - [`templates`] - Project template file generation
//! - [`scaffold`] - Project creation and initialization

pub mod manifest;
pub mod scaffold;
pub mod templates;

pub use scaffold::{create_project, init_project};
