//! TUI view modules.
//!
//! This module contains the rendering logic for each screen in the TUI.
//!
//! ## Views
//!
//! - [`main_view`] - Main menu screen with shortcuts and navigation
//! - [`toolchain_view`] - Installed toolchains list
//! - [`doctor_view`] - Doctor check results
//! - [`progress_view`] - Download/operation progress display
//! - [`version_select_view`] - Version selection for installation

pub mod doctor_view;
pub mod main_view;
pub mod progress_view;
pub mod toolchain_view;
pub mod version_select_view;
