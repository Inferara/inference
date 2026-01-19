#![warn(clippy::pedantic)]

//! Toolchain management module for the infs CLI.
//!
//! This module provides functionality for managing Inference toolchain installations,
//! including downloading, verifying, installing, and switching between versions.
//!
//! ## Module Structure
//!
//! - [`platform`] - OS and architecture detection
//! - [`paths`] - Toolchain directory path management
//! - [`manifest`] - Release manifest fetching and parsing
//! - [`download`] - HTTP download with progress tracking
//! - [`verify`] - SHA256 checksum verification

pub mod download;
pub mod manifest;
pub mod paths;
pub mod platform;
pub mod verify;

pub use download::download_file;
pub use manifest::{fetch_artifact, fetch_manifest};
pub use paths::ToolchainPaths;
pub use platform::Platform;
pub use verify::verify_checksum;
