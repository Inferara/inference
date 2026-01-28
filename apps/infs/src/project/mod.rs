//! Project management module.
//!
//! This module provides functionality for creating and managing Inference
//! projects, including manifest handling and project scaffolding.
//!
//! ## Modules
//!
//! - [`manifest`] - Inference.toml parsing and validation
//! - [`scaffold`] - Project creation and initialization
//!
//! ## Key Types
//!
//! - [`InferenceToml`] - The manifest file structure
//! - [`ProjectConfig`] - Loaded and validated project configuration

pub mod manifest;
pub mod scaffold;

#[allow(unused_imports)]
pub use manifest::validate_project_name;
#[allow(unused_imports)]
pub use manifest::{Dependencies, Package};
#[allow(unused_imports)]
pub use scaffold::create_project_default;
pub use scaffold::{create_project, init_project};
