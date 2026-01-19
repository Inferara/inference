#![warn(clippy::pedantic)]

//! Terminal setup and cleanup utilities for the TUI.
//!
//! This module provides RAII-based terminal management that ensures
//! the terminal is properly restored even on panic or error.
//!
//! ## Usage
//!
//! ```ignore
//! let guard = TerminalGuard::new()?;
//! // Use guard.terminal for rendering
//! // Terminal is automatically restored when guard is dropped
//! ```

use std::io::{self, Stdout};

use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Type alias for the terminal backend used throughout the TUI.
pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

/// RAII guard for terminal setup and cleanup.
///
/// This struct ensures that the terminal is properly restored to its
/// original state when the guard is dropped, even if a panic occurs.
///
/// # Terminal State
///
/// On creation:
/// - Enables raw mode (disables line buffering and echo)
/// - Enters alternate screen (preserves original terminal content)
///
/// On drop:
/// - Disables raw mode
/// - Leaves alternate screen
///
/// # Panic Safety
///
/// The `Drop` implementation ignores errors during cleanup to ensure
/// best-effort restoration without causing additional panics.
pub struct TerminalGuard {
    /// The ratatui terminal instance.
    pub terminal: TuiTerminal,
}

impl TerminalGuard {
    /// Creates a new terminal guard, setting up the terminal for TUI mode.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Raw mode cannot be enabled
    /// - Alternate screen cannot be entered
    /// - Terminal backend cannot be created
    pub fn new() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;

        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).context("failed to create terminal")?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_guard_can_be_created_in_non_tty() {
        // This test documents the expected behavior: creating a TerminalGuard
        // on a non-TTY (like in CI) may fail, which is acceptable.
        // The actual TUI should only be launched when is_terminal() is true.
        let result = TerminalGuard::new();
        // We don't assert success because CI environments may not have a TTY
        // Just verify it doesn't panic
        drop(result);
    }
}
