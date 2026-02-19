//! Advanced input field widget with cursor support.
//!
//! This module provides an input field component that supports:
//! - Cursor positioning with Left/Right arrow keys
//! - Character insertion at cursor position
//! - Backspace and delete operations
//! - Command history navigation

/// Command history for navigating previous inputs.
#[derive(Debug, Clone, Default)]
pub struct CommandHistory {
    /// List of previous commands (oldest first).
    commands: Vec<String>,
    /// Current index in history (None means not navigating).
    index: Option<usize>,
    /// Maximum number of commands to store.
    max_size: usize,
    /// Temporary storage for current input when navigating.
    temp_input: String,
}

impl CommandHistory {
    /// Creates a new command history with default max size.
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            index: None,
            max_size: 100,
            temp_input: String::new(),
        }
    }

    /// Adds a command to the history.
    pub fn push(&mut self, command: String) {
        // Don't add empty commands or duplicates of the last command
        if command.is_empty() {
            return;
        }
        if self.commands.last() == Some(&command) {
            return;
        }

        self.commands.push(command);

        // Trim to max size
        if self.commands.len() > self.max_size {
            self.commands.remove(0);
        }

        // Reset navigation
        self.index = None;
        self.temp_input.clear();
    }

    /// Navigates to the previous command (Up arrow).
    /// Returns the command to display, or None if no change.
    pub fn previous(&mut self, current_input: &str) -> Option<&str> {
        if self.commands.is_empty() {
            return None;
        }

        match self.index {
            None => {
                // Start navigating, save current input
                self.temp_input = current_input.to_string();
                self.index = Some(self.commands.len() - 1);
                self.commands.last().map(String::as_str)
            }
            Some(idx) if idx > 0 => {
                self.index = Some(idx - 1);
                self.commands.get(idx - 1).map(String::as_str)
            }
            Some(_) => {
                // Already at the oldest command
                None
            }
        }
    }

    /// Navigates to the next command (Down arrow).
    /// Returns the command to display, or None if no change.
    pub fn next(&mut self) -> Option<&str> {
        match self.index {
            Some(idx) if idx + 1 < self.commands.len() => {
                self.index = Some(idx + 1);
                self.commands.get(idx + 1).map(String::as_str)
            }
            Some(_) => {
                // Return to current input
                self.index = None;
                Some(self.temp_input.as_str())
            }
            None => None,
        }
    }

    /// Resets navigation state.
    pub fn reset_navigation(&mut self) {
        self.index = None;
        self.temp_input.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_history_push_and_previous() {
        let mut history = CommandHistory::new();
        history.push("cmd1".to_string());
        history.push("cmd2".to_string());

        let prev = history.previous("current");
        assert_eq!(prev, Some("cmd2"));

        let prev = history.previous("current");
        assert_eq!(prev, Some("cmd1"));

        // At the oldest command
        let prev = history.previous("current");
        assert_eq!(prev, None);
    }

    #[test]
    fn command_history_next_returns_to_current() {
        let mut history = CommandHistory::new();
        history.push("cmd1".to_string());
        history.push("cmd2".to_string());

        history.previous("current input");
        history.previous("current input");

        let next = history.next();
        assert_eq!(next, Some("cmd2"));

        let next = history.next();
        assert_eq!(next, Some("current input"));
    }

    #[test]
    fn command_history_reset_navigation() {
        let mut history = CommandHistory::new();
        history.push("cmd".to_string());
        history.previous("current");
        history.reset_navigation();

        // After reset, previous should start from the end again
        let prev = history.previous("new current");
        assert_eq!(prev, Some("cmd"));
    }
}
