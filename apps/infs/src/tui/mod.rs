#![warn(clippy::pedantic)]

//! Terminal User Interface for the infs CLI.
//!
//! This module provides an interactive TUI for the Inference toolchain,
//! allowing users to navigate commands and manage projects visually.
//!
//! ## Usage
//!
//! The TUI is launched automatically when `infs` is run without arguments
//! in an interactive terminal environment.
//!
//! ## Headless Detection
//!
//! The TUI will not launch in headless environments:
//! - When `CI=true` or `CI=1` environment variable is set
//! - When `NO_COLOR` environment variable is set (any value)
//! - When stdout is not a terminal (piped or redirected)
//!
//! ## Modules
//!
//! - [`terminal`] - Terminal setup and cleanup with RAII guard
//! - [`app`] - Main application state and event loop

mod app;
mod terminal;

use std::io::IsTerminal;

use anyhow::{Context, Result};

use terminal::TerminalGuard;

/// Determines whether the TUI should be used based on environment.
///
/// Returns `false` in headless environments:
/// - `CI=true` or `CI=1` environment variable
/// - `NO_COLOR` environment variable (any value)
/// - Non-TTY stdout (piped or redirected)
#[must_use]
pub fn should_use_tui() -> bool {
    if let Ok(ci) = std::env::var("CI") {
        let ci_lower = ci.to_lowercase();
        if ci_lower == "true" || ci_lower == "1" {
            return false;
        }
    }

    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    std::io::stdout().is_terminal()
}

/// Runs the TUI application.
///
/// This function sets up the terminal, runs the main event loop,
/// and ensures proper cleanup on exit or error.
///
/// # Errors
///
/// Returns an error if:
/// - Terminal setup fails
/// - Event handling fails
/// - Drawing fails
pub fn run() -> Result<()> {
    let mut guard = TerminalGuard::new().context("failed to initialize terminal")?;

    app::run_app(&mut guard).context("TUI application error")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_tui_returns_bool() {
        // This test verifies the function can be called and returns a boolean.
        // The actual return value depends on the environment.
        let result = should_use_tui();
        // In CI, this should be false (CI=true is typically set)
        // We can't assert a specific value since test environments vary
        let _ = result;
    }

    #[test]
    fn should_use_tui_respects_ci_env() {
        // Save original value
        let original = std::env::var("CI").ok();

        // SAFETY: Tests run single-threaded with #[test], so setting env vars is safe
        // for the duration of this test.
        unsafe {
            std::env::set_var("CI", "true");
        }
        assert!(!should_use_tui());

        unsafe {
            std::env::set_var("CI", "1");
        }
        assert!(!should_use_tui());

        unsafe {
            std::env::set_var("CI", "TRUE");
        }
        assert!(!should_use_tui());

        // Restore original
        unsafe {
            match original {
                Some(val) => std::env::set_var("CI", val),
                None => std::env::remove_var("CI"),
            }
        }
    }

    #[test]
    fn should_use_tui_respects_no_color_env() {
        // Save original value
        let original = std::env::var("NO_COLOR").ok();

        // SAFETY: Tests run single-threaded with #[test], so setting env vars is safe
        // for the duration of this test.
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
        assert!(!should_use_tui());

        unsafe {
            std::env::set_var("NO_COLOR", "");
        }
        assert!(!should_use_tui());

        // Restore original
        unsafe {
            match original {
                Some(val) => std::env::set_var("NO_COLOR", val),
                None => std::env::remove_var("NO_COLOR"),
            }
        }
    }
}
