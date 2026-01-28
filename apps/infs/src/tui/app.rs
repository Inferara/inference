//! Main TUI application logic.
//!
//! This module contains the event loop, state management, and rendering
//! for the infs TUI interface.
//!
//! ## Input Modes
//!
//! - **Normal**: Default mode, shortcuts work directly (q to quit, : to enter command)
//! - **Command**: Input mode for entering commands with `:` prefix
//!
//! ## Screens
//!
//! - **Main**: Main menu with navigation options
//! - **Toolchains**: List of installed toolchain versions
//! - **Doctor**: Health check results
//! - **Progress**: Download/operation progress display
//!
//! ## Features
//!
//! - Command history with Up/Down navigation
//! - Tab completion for commands
//! - Cursor movement with Left/Right arrows
//! - Toolchain operations (Enter to set as default)

use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Frame;

use super::install_task;
use super::menu::Menu;
use super::state::{
    DoctorState, InstallProgress, ProgressItem, ProgressState, Screen, ToolchainInfo,
    ToolchainsState, VersionSelectInfo, VersionSelectState,
};
use super::terminal::TerminalGuard;
use super::theme::Theme;
use super::views::{doctor_view, main_view, progress_view, toolchain_view, version_select_view};
use super::widgets::command_history::CommandHistory;
use crate::toolchain::ToolchainPaths;
use crate::toolchain::doctor::run_all_checks;

/// Event polling timeout in milliseconds.
const POLL_TIMEOUT_MS: u64 = 100;

/// Known commands for tab completion.
const KNOWN_COMMANDS: &[&str] = &[
    "build",
    "run",
    "verify",
    "new",
    "install",
    "doctor",
    "help",
    "version",
    "quit",
    "toolchains",
    "exit",
];

/// Input mode for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Normal mode: shortcuts work directly.
    #[default]
    Normal,
    /// Command mode: typing a command.
    Command,
}

/// Main application state.
pub struct App {
    /// Current screen.
    screen: Screen,
    /// Current input mode.
    input_mode: InputMode,
    /// Command input buffer.
    command_input: String,
    /// Cursor position in command input (byte offset).
    cursor_pos: usize,
    /// Status message to display.
    status_message: String,
    /// Whether the application should quit.
    should_quit: bool,
    /// Theme colors.
    theme: Theme,
    /// Menu state.
    menu: Menu,
    /// Toolchains view state.
    toolchains_state: ToolchainsState,
    /// Doctor view state.
    doctor_state: DoctorState,
    /// Progress view state.
    progress_state: ProgressState,
    /// Command history.
    command_history: CommandHistory,
    /// Command to execute after TUI exits (for commands requiring terminal access).
    pending_command: Option<String>,
    /// Override for executable path (used in tests).
    exe_path_override: Option<std::path::PathBuf>,
    /// Receiver for installation progress messages from background task.
    install_receiver: Option<Receiver<InstallProgress>>,
    /// Screen to return to after progress view is dismissed.
    previous_screen: Option<Screen>,
    /// Version select view state.
    version_select_state: VersionSelectState,
    /// Receiver for version loading results from background task.
    version_load_receiver: Option<Receiver<Result<Vec<VersionSelectInfo>, String>>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Main,
            input_mode: InputMode::Normal,
            command_input: String::new(),
            cursor_pos: 0,
            status_message: String::from("Press ':' to enter a command, 'q' to quit"),
            should_quit: false,
            theme: Theme::detect(),
            menu: Menu::new(),
            toolchains_state: ToolchainsState::new(),
            doctor_state: DoctorState::new(),
            progress_state: ProgressState::default(),
            command_history: CommandHistory::new(),
            pending_command: None,
            exe_path_override: None,
            install_receiver: None,
            previous_screen: None,
            version_select_state: VersionSelectState::new(),
            version_load_receiver: None,
        }
    }
}

impl App {
    /// Returns the cursor display position (characters, not bytes).
    #[must_use]
    pub fn cursor_display_pos(&self) -> usize {
        self.command_input[..self.cursor_pos].chars().count()
    }

    /// Sets an override for the executable path used by `run_quick_command`.
    ///
    /// This is used in tests to avoid infinite recursion when `std::env::current_exe()`
    /// returns the test binary instead of the actual `infs` binary.
    #[cfg(test)]
    pub fn set_exe_path_override(&mut self, path: std::path::PathBuf) {
        self.exe_path_override = Some(path);
    }

    /// Handles a key event based on current screen and input mode.
    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match self.input_mode {
            InputMode::Normal => self.handle_normal_key(code),
            InputMode::Command => self.handle_command_key(code, modifiers),
        }
    }

    /// Handles a key event in normal mode.
    fn handle_normal_key(&mut self, code: KeyCode) {
        match self.screen {
            Screen::Main => self.handle_main_key(code),
            Screen::Toolchains => self.handle_toolchains_key(code),
            Screen::Doctor => self.handle_doctor_key(code),
            Screen::Progress => self.handle_progress_key(code),
            Screen::VersionSelect => self.handle_version_select_key(code),
        }
    }

    /// Handles key events on the main screen.
    fn handle_main_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char(':') => {
                self.input_mode = InputMode::Command;
                self.command_input.clear();
                self.cursor_pos = 0;
                self.command_history.reset_navigation();
                self.status_message =
                    String::from("Enter command (Esc to cancel, Tab to complete)");
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu.up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.menu.down();
            }
            KeyCode::Enter => {
                self.activate_menu_item();
            }
            KeyCode::Char(c) => {
                if let Some(item) = Menu::find_by_key(c) {
                    if item.quits {
                        self.should_quit = true;
                    } else if let Some(screen) = item.screen {
                        self.navigate_to(screen);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handles key events on the toolchains screen.
    fn handle_toolchains_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.status_message = String::from("Press ':' to enter a command, 'q' to quit");
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.toolchains_state.select_previous();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.toolchains_state.select_next();
            }
            KeyCode::Char('i') => {
                // Show version selection screen
                self.previous_screen = Some(Screen::Toolchains);
                self.version_select_state = VersionSelectState::new();
                self.navigate_to(Screen::VersionSelect);
            }
            KeyCode::Enter => {
                if self.toolchains_state.toolchains.is_empty() {
                    // No toolchains installed - show version selection
                    self.previous_screen = Some(Screen::Toolchains);
                    self.version_select_state = VersionSelectState::new();
                    self.navigate_to(Screen::VersionSelect);
                } else {
                    // Toolchains exist - set selected as default
                    self.set_selected_toolchain_as_default();
                }
            }
            _ => {}
        }
    }

    /// Sets the currently selected toolchain as the default.
    fn set_selected_toolchain_as_default(&mut self) {
        let Some(toolchain) = self
            .toolchains_state
            .toolchains
            .get(self.toolchains_state.selected)
        else {
            return;
        };

        if toolchain.is_default {
            self.status_message = format!("{} is already the default", toolchain.version);
            return;
        }

        let version = toolchain.version.clone();

        let result = ToolchainPaths::new().and_then(|paths| {
            paths.set_default_version(&version)?;
            paths.update_symlinks(&version)?;
            Ok(())
        });

        match result {
            Ok(()) => {
                self.status_message = format!("Set {version} as default toolchain");
                // Reload to reflect the change
                self.toolchains_state.loaded = false;
                self.load_toolchain_data();
            }
            Err(e) => {
                self.status_message = format!("Failed to set default: {e}");
            }
        }
    }

    /// Handles key events on the doctor screen.
    fn handle_doctor_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.status_message = String::from("Press ':' to enter a command, 'q' to quit");
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.doctor_state.select_previous();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.doctor_state.select_next();
            }
            KeyCode::Char('r') => {
                self.load_doctor_data();
            }
            _ => {}
        }
    }

    /// Handles key events on the progress screen.
    fn handle_progress_key(&mut self, code: KeyCode) {
        if code == KeyCode::Esc {
            if self.progress_state.completed {
                // Installation completed or failed - return to previous screen
                self.return_from_progress();
            } else {
                // Installation in progress - cancel it
                self.cancel_installation();
            }
        }
    }

    /// Handles key events on the version select screen.
    fn handle_version_select_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                // Return to previous screen
                let return_screen = self.previous_screen.unwrap_or(Screen::Main);
                self.previous_screen = None;
                self.version_load_receiver = None;
                self.navigate_to(return_screen);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.version_select_state.select_previous();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.version_select_state.select_next();
            }
            KeyCode::Enter => {
                if self.version_select_state.can_install_selected() {
                    if let Some(version_info) = self.version_select_state.selected_version() {
                        let version = version_info.version.clone();
                        self.start_installation(Some(version));
                    }
                } else {
                    self.status_message =
                        String::from("Selected version is not available for your platform");
                }
            }
            _ => {}
        }
    }

    /// Returns from progress screen to the previous screen.
    fn return_from_progress(&mut self) {
        // Reload toolchain data if we came from toolchains screen
        if self.previous_screen == Some(Screen::Toolchains) {
            self.toolchains_state.loaded = false;
        }

        let return_screen = self.previous_screen.unwrap_or(Screen::Main);
        self.previous_screen = None;
        self.install_receiver = None;
        self.navigate_to(return_screen);
    }

    /// Cancels an in-progress installation.
    fn cancel_installation(&mut self) {
        // Drop the receiver to signal cancellation (sender will fail)
        self.install_receiver = None;
        self.progress_state.set_error("Installation cancelled");
        self.status_message = String::from("Installation cancelled. Press Esc to return.");
    }

    /// Activates the currently selected menu item.
    fn activate_menu_item(&mut self) {
        let item = self.menu.selected_item();
        if item.quits {
            self.should_quit = true;
        } else if let Some(screen) = item.screen {
            self.navigate_to(screen);
        }
    }

    /// Navigates to a specific screen.
    fn navigate_to(&mut self, screen: Screen) {
        self.screen = screen;
        match screen {
            Screen::Main => {
                self.status_message = String::from("Press ':' to enter a command, 'q' to quit");
            }
            Screen::Toolchains => {
                if !self.toolchains_state.loaded {
                    self.load_toolchain_data();
                }
                self.status_message = String::from("Press Enter to set as default, Esc to go back");
            }
            Screen::Doctor => {
                if !self.doctor_state.loaded {
                    self.load_doctor_data();
                }
                self.status_message = String::from("Press 'r' to refresh, Esc to go back");
            }
            Screen::Progress => {
                self.status_message = String::from("Operation in progress...");
            }
            Screen::VersionSelect => {
                if !self.version_select_state.loaded && !self.version_select_state.loading {
                    self.load_version_data();
                }
                self.status_message = String::from("Press Enter to install, Esc to go back");
            }
        }
    }

    /// Handles a key event in command mode.
    fn handle_command_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.command_input.clear();
                self.cursor_pos = 0;
                self.command_history.reset_navigation();
                self.status_message = String::from("Command cancelled");
            }
            KeyCode::Enter => {
                self.execute_command();
            }
            KeyCode::Backspace => {
                self.backspace();
            }
            KeyCode::Delete => {
                self.delete_char();
            }
            KeyCode::Left if modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_cursor_word_left();
            }
            KeyCode::Left => {
                self.move_cursor_left();
            }
            KeyCode::Right if modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_cursor_word_right();
            }
            KeyCode::Right => {
                self.move_cursor_right();
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.command_input.len();
            }
            KeyCode::Up => {
                self.history_previous();
            }
            KeyCode::Down => {
                self.history_next();
            }
            KeyCode::Tab => {
                self.tab_complete();
            }
            KeyCode::Char(c) => {
                self.insert_char(c);
            }
            _ => {}
        }
    }

    /// Inserts a character at the cursor position.
    fn insert_char(&mut self, c: char) {
        self.command_input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    /// Removes the character before the cursor.
    fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            // Find the start of the previous character
            let prev_char_start = self.command_input[..self.cursor_pos]
                .char_indices()
                .last()
                .map_or(0, |(idx, _)| idx);
            self.command_input.remove(prev_char_start);
            self.cursor_pos = prev_char_start;
        }
    }

    /// Removes the character at the cursor position.
    fn delete_char(&mut self) {
        if self.cursor_pos < self.command_input.len() {
            self.command_input.remove(self.cursor_pos);
        }
    }

    /// Moves cursor left by one character.
    fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.command_input[..self.cursor_pos]
                .char_indices()
                .last()
                .map_or(0, |(idx, _)| idx);
        }
    }

    /// Moves cursor right by one character.
    fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.command_input.len()
            && let Some((_, c)) = self.command_input[self.cursor_pos..].char_indices().next()
        {
            self.cursor_pos += c.len_utf8();
        }
    }

    /// Moves cursor left to the start of the previous word.
    #[allow(clippy::skip_while_next)]
    fn move_cursor_word_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        // Skip any whitespace before current position, then skip non-whitespace
        // to find the start of the previous word
        let before_cursor = &self.command_input[..self.cursor_pos];
        let mut new_pos = before_cursor
            .char_indices()
            .rev()
            .skip_while(|(_, c)| c.is_whitespace())
            .skip_while(|(_, c)| !c.is_whitespace())
            .next()
            .map_or(0, |(idx, c)| idx + c.len_utf8());

        // If we didn't move, try going to the start
        if new_pos == self.cursor_pos {
            new_pos = 0;
        }

        self.cursor_pos = new_pos;
    }

    /// Moves cursor right to the start of the next word.
    #[allow(clippy::skip_while_next)]
    fn move_cursor_word_right(&mut self) {
        if self.cursor_pos >= self.command_input.len() {
            return;
        }

        let after_cursor = &self.command_input[self.cursor_pos..];

        // Skip current word (non-whitespace), then skip whitespace to get to next word
        let skip_chars = after_cursor
            .char_indices()
            .skip_while(|(_, c)| !c.is_whitespace())
            .skip_while(|(_, c)| c.is_whitespace())
            .next()
            .map_or(after_cursor.len(), |(idx, _)| idx);

        self.cursor_pos += skip_chars;
    }

    /// Navigates to the previous command in history.
    fn history_previous(&mut self) {
        if let Some(cmd) = self.command_history.previous(&self.command_input) {
            self.command_input = cmd.to_string();
            self.cursor_pos = self.command_input.len();
        }
    }

    /// Navigates to the next command in history.
    fn history_next(&mut self) {
        if let Some(cmd) = self.command_history.next() {
            self.command_input = cmd.to_string();
            self.cursor_pos = self.command_input.len();
        }
    }

    /// Performs tab completion on the current input.
    fn tab_complete(&mut self) {
        let input = self.command_input.trim().to_lowercase();
        if input.is_empty() {
            return;
        }

        let matches: Vec<&&str> = KNOWN_COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(&input))
            .collect();

        match matches.len() {
            0 => {
                // No matches
                self.status_message = String::from("No matching command");
            }
            1 => {
                // Single match, complete it
                self.command_input = (*matches[0]).to_string();
                self.cursor_pos = self.command_input.len();
                self.status_message =
                    String::from("Enter command (Esc to cancel, Tab to complete)");
            }
            _ => {
                // Multiple matches, show them
                let match_list: Vec<&str> = matches.iter().map(|s| **s).collect();
                self.status_message = format!("Matches: {}", match_list.join(", "));
            }
        }
    }

    /// Executes the current command.
    fn execute_command(&mut self) {
        let command = self.command_input.trim().to_lowercase();
        let original_input = self.command_input.clone();

        self.command_input.clear();
        self.cursor_pos = 0;
        self.input_mode = InputMode::Normal;

        if command.is_empty() {
            self.status_message = String::from("No command entered");
            return;
        }

        // Add to history (non-empty commands only)
        self.command_history.push(original_input);
        self.command_history.reset_navigation();

        match command.as_str() {
            "q" | "quit" | "exit" => {
                self.should_quit = true;
            }
            "toolchains" | "t" => {
                self.navigate_to(Screen::Toolchains);
            }
            "doctor" | "d" => {
                self.navigate_to(Screen::Doctor);
            }
            // Commands that need terminal access - exit TUI and run
            "build" | "new" | "install" | "run" | "verify" => {
                self.pending_command = Some(command);
                self.should_quit = true;
            }
            // Quick commands - spawn subprocess and show output
            "help" => {
                self.run_quick_command(&["--help"]);
            }
            "version" => {
                self.run_quick_command(&["version"]);
            }
            _ => {
                self.status_message = format!("Unknown command: {command}");
            }
        }
    }

    /// Runs a quick command via subprocess and displays output in status message.
    fn run_quick_command(&mut self, args: &[&str]) {
        let exe = self
            .exe_path_override
            .clone()
            .or_else(|| std::env::current_exe().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("infs"));
        match std::process::Command::new(&exe).args(args).output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = if stdout.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                // Truncate for display and take first line
                let first_line = combined.lines().next().unwrap_or("(no output)");
                let truncated = if first_line.len() > 60 {
                    format!("{}...", &first_line[..57])
                } else {
                    first_line.to_string()
                };
                self.status_message = truncated;
            }
            Err(e) => {
                self.status_message = format!("Failed to run command: {e}");
            }
        }
    }

    /// Loads toolchain data from the filesystem.
    fn load_toolchain_data(&mut self) {
        let paths = match ToolchainPaths::new() {
            Ok(p) => p,
            Err(e) => {
                self.status_message = format!("Cannot load toolchains: {e}");
                self.toolchains_state.toolchains.clear();
                self.toolchains_state.loaded = true;
                return;
            }
        };

        let default_version = match paths.get_default_version() {
            Ok(v) => v,
            Err(e) => {
                self.status_message = format!("Cannot read default version: {e}");
                None
            }
        };

        let versions = match paths.list_installed_versions() {
            Ok(v) => v,
            Err(e) => {
                self.status_message = format!("Cannot list toolchains: {e}");
                Vec::new()
            }
        };

        self.toolchains_state.toolchains = versions
            .into_iter()
            .map(|version| {
                let is_default = default_version.as_ref() == Some(&version);
                let metadata = paths.read_metadata(&version);
                ToolchainInfo {
                    version,
                    is_default,
                    metadata,
                }
            })
            .collect();

        self.toolchains_state.selected = 0;
        self.toolchains_state.loaded = true;
    }

    /// Loads doctor check data.
    fn load_doctor_data(&mut self) {
        self.doctor_state.checks = run_all_checks();
        self.doctor_state.selected = 0;
        self.doctor_state.loaded = true;
    }

    /// Loads version data from the release manifest in a background thread.
    ///
    /// Creates a channel for the result, spawns a thread with a tokio runtime
    /// to fetch the manifest, and sets up the version select state for loading.
    fn load_version_data(&mut self) {
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel();
        self.version_load_receiver = Some(rx);
        self.version_select_state.loading = true;
        self.version_select_state.error = None;

        // Detect platform and set current_os
        let platform = crate::toolchain::Platform::detect()
            .map_or_else(|_| "unknown".to_string(), |p| p.os().to_string());
        self.version_select_state.current_os.clone_from(&platform);

        // Spawn version loading task on a separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            let result = rt.block_on(async {
                use crate::toolchain::Platform;
                use crate::toolchain::manifest::{fetch_manifest, sorted_versions};

                let platform =
                    Platform::detect().map_err(|e| format!("Platform detection failed: {e}"))?;
                let manifest = fetch_manifest()
                    .await
                    .map_err(|e| format!("Failed to fetch manifest: {e}"))?;

                let versions: Vec<VersionSelectInfo> = sorted_versions(&manifest)
                    .into_iter()
                    .map(|v| VersionSelectInfo {
                        version: v.version.clone(),
                        stable: v.stable,
                        platforms: v
                            .available_platforms()
                            .into_iter()
                            .map(String::from)
                            .collect(),
                        available_for_current: v.has_platform(platform),
                    })
                    .collect();

                Ok(versions)
            });

            let _ = tx.send(result);
        });
    }

    /// Polls the version loading channel and updates the version select state.
    ///
    /// This method should be called in each iteration of the TUI event loop
    /// when on the version select screen.
    fn poll_version_loading(&mut self) {
        let Some(receiver) = self.version_load_receiver.as_ref() else {
            return;
        };

        if let Ok(result) = receiver.try_recv() {
            match result {
                Ok(versions) => {
                    self.version_select_state.versions = versions;
                    self.version_select_state.selected = 0;
                    self.version_select_state.loaded = true;
                    self.version_select_state.loading = false;
                }
                Err(error) => {
                    self.version_select_state.error = Some(error);
                    self.version_select_state.loaded = true;
                    self.version_select_state.loading = false;
                }
            }
            self.version_load_receiver = None;
        }
    }

    /// Starts a background installation task.
    ///
    /// Creates a channel for progress messages, sets up the progress state,
    /// spawns a thread with a tokio runtime to run the installation, and
    /// navigates to the progress screen.
    ///
    /// # Arguments
    ///
    /// * `version` - Optional version to install. If `None`, installs the latest version.
    fn start_installation(&mut self, version: Option<String>) {
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel();
        self.install_receiver = Some(rx);

        // Set up progress state
        self.progress_state = ProgressState::new("Installing Toolchain");
        self.progress_state.set_status("Starting installation...");

        // Add a progress item that will be updated with current phase
        let progress_item = ProgressItem::new("Initializing...");
        self.progress_state.add_item(progress_item);

        // Remember current screen to return to
        self.previous_screen = Some(self.screen);

        // Spawn installation task on a separate thread with its own tokio runtime
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(install_task::run_installation(version, tx));
        });

        // Navigate to progress screen
        self.screen = Screen::Progress;
        self.status_message = String::from("Installing... Press Esc to cancel.");
    }

    /// Polls the installation progress channel and updates the progress state.
    ///
    /// This method should be called in each iteration of the TUI event loop.
    /// It performs a non-blocking receive on the channel and processes any
    /// available progress messages.
    fn poll_install_progress(&mut self) {
        let Some(receiver) = self.install_receiver.as_ref() else {
            return;
        };

        // Collect all available messages first to avoid borrow issues
        let mut messages = Vec::new();
        while let Ok(msg) = receiver.try_recv() {
            messages.push(msg);
        }

        // Process collected messages
        let mut clear_receiver = false;
        for msg in messages {
            match msg {
                InstallProgress::PhaseStarted { phase } => {
                    self.progress_state.set_status(format!("{phase}..."));
                    // Update progress item description to show current phase
                    if let Some(item) = self.progress_state.items.first_mut() {
                        item.description = phase;
                    }
                }
                InstallProgress::DownloadStarted { total } => {
                    if let Some(item) = self.progress_state.items.first_mut() {
                        item.total = total;
                        item.start();
                    }
                }
                InstallProgress::DownloadProgress { downloaded, speed } => {
                    if let Some(item) = self.progress_state.items.first_mut() {
                        item.update_with_speed(downloaded, speed);
                    }
                }
                InstallProgress::PhaseCompleted { phase } => {
                    self.progress_state.set_status(format!("{phase} - done"));
                }
                InstallProgress::Completed { version } => {
                    self.progress_state.complete();
                    self.progress_state
                        .set_status(format!("Toolchain v{version} installed successfully"));
                    if let Some(item) = self.progress_state.items.first_mut() {
                        item.description = format!("Installed v{version}");
                        item.complete();
                    }
                    self.status_message =
                        String::from("Installation complete! Press Esc to return.");
                    clear_receiver = true;
                }
                InstallProgress::Failed { error } => {
                    self.progress_state.set_error(&error);
                    self.status_message = String::from("Installation failed. Press Esc to return.");
                    clear_receiver = true;
                }
            }
        }

        if clear_receiver {
            self.install_receiver = None;
        }
    }
}

/// Runs the main TUI event loop.
///
/// Returns `Ok(Some(command))` if the TUI exits with a pending command to execute,
/// or `Ok(None)` if the TUI exits normally without a pending command.
///
/// # Errors
///
/// Returns an error if:
/// - Terminal setup fails
/// - Drawing fails
/// - Event polling fails
pub fn run_app(guard: &mut TerminalGuard) -> Result<Option<String>> {
    let mut app = App::default();

    loop {
        // Poll for async operations (non-blocking)
        app.poll_install_progress();
        app.poll_version_loading();

        guard
            .terminal
            .draw(|frame| render(&app, frame))
            .context("failed to draw frame")?;

        if event::poll(Duration::from_millis(POLL_TIMEOUT_MS)).context("event poll failed")?
            && let Event::Key(key) = event::read().context("failed to read event")?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key.code, key.modifiers);
        }

        if app.should_quit {
            break;
        }
    }

    Ok(app.pending_command)
}

/// Renders the TUI based on current screen.
fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    match app.screen {
        Screen::Main => {
            main_view::render(
                frame,
                area,
                &app.theme,
                &app.menu,
                &app.command_input,
                app.input_mode == InputMode::Command,
                &app.status_message,
                app.cursor_display_pos(),
            );
        }
        Screen::Toolchains => {
            toolchain_view::render(frame, area, &app.theme, &app.toolchains_state);
        }
        Screen::Doctor => {
            doctor_view::render(frame, area, &app.theme, &app.doctor_state);
        }
        Screen::Progress => {
            progress_view::render(frame, area, &app.theme, &app.progress_state);
        }
        Screen::VersionSelect => {
            version_select_view::render(frame, area, &app.theme, &app.version_select_state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_default_is_normal_mode() {
        let app = App::default();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(!app.should_quit);
        assert!(app.command_input.is_empty());
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn app_default_screen_is_main() {
        let app = App::default();
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn normal_mode_q_sets_should_quit() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn normal_mode_ctrl_c_sets_should_quit() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn normal_mode_colon_enters_command_mode() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char(':'), KeyModifiers::NONE);
        assert_eq!(app.input_mode, InputMode::Command);
    }

    #[test]
    fn command_mode_esc_returns_to_normal() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("test"),
            cursor_pos: 4,
            ..App::default()
        };

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.command_input.is_empty());
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn command_mode_char_adds_to_input() {
        let mut app = App {
            input_mode: InputMode::Command,
            ..App::default()
        };

        app.handle_key(KeyCode::Char('h'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);

        assert_eq!(app.command_input, "hi");
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn command_mode_backspace_removes_char() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("hi"),
            cursor_pos: 2,
            ..App::default()
        };

        app.handle_key(KeyCode::Backspace, KeyModifiers::NONE);

        assert_eq!(app.command_input, "h");
        assert_eq!(app.cursor_pos, 1);
    }

    #[test]
    fn cursor_movement_left_right() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("abc"),
            cursor_pos: 3,
            ..App::default()
        };

        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(app.cursor_pos, 2);

        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(app.cursor_pos, 1);

        app.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn cursor_movement_home_end() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("abc"),
            cursor_pos: 1,
            ..App::default()
        };

        app.handle_key(KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(app.cursor_pos, 0);

        app.handle_key(KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.cursor_pos, 3);
    }

    #[test]
    fn cursor_movement_word_left() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("one two three"),
            cursor_pos: 13, // end of "three"
            ..App::default()
        };

        // Move to start of "three"
        app.handle_key(KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 8);

        // Move to start of "two"
        app.handle_key(KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 4);

        // Move to start of "one"
        app.handle_key(KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 0);

        // At start, should stay at 0
        app.handle_key(KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn cursor_movement_word_right() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("one two three"),
            cursor_pos: 0,
            ..App::default()
        };

        // Move to start of "two"
        app.handle_key(KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 4);

        // Move to start of "three"
        app.handle_key(KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 8);

        // Move to end
        app.handle_key(KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 13);

        // At end, should stay at end
        app.handle_key(KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(app.cursor_pos, 13);
    }

    #[test]
    fn insert_at_cursor_position() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("ac"),
            cursor_pos: 1,
            ..App::default()
        };

        app.handle_key(KeyCode::Char('b'), KeyModifiers::NONE);
        assert_eq!(app.command_input, "abc");
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn delete_at_cursor_position() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("abc"),
            cursor_pos: 1,
            ..App::default()
        };

        app.handle_key(KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(app.command_input, "ac");
        assert_eq!(app.cursor_pos, 1);
    }

    #[test]
    fn execute_quit_command_sets_should_quit_without_pending() {
        let mut app = App {
            command_input: String::from("quit"),
            cursor_pos: 4,
            ..App::default()
        };

        app.execute_command();

        assert!(app.should_quit);
        assert!(app.pending_command.is_none());
    }

    #[test]
    fn execute_terminal_command_sets_pending_and_quits() {
        let mut app = App {
            command_input: String::from("build"),
            cursor_pos: 5,
            ..App::default()
        };

        app.execute_command();

        assert!(app.should_quit);
        assert_eq!(app.pending_command, Some(String::from("build")));
    }

    #[test]
    fn execute_run_command_sets_pending_and_quits() {
        let mut app = App {
            command_input: String::from("run"),
            cursor_pos: 3,
            ..App::default()
        };

        app.execute_command();

        assert!(app.should_quit);
        assert_eq!(app.pending_command, Some(String::from("run")));
    }

    #[test]
    fn execute_verify_command_sets_pending_and_quits() {
        let mut app = App {
            command_input: String::from("verify"),
            cursor_pos: 6,
            ..App::default()
        };

        app.execute_command();

        assert!(app.should_quit);
        assert_eq!(app.pending_command, Some(String::from("verify")));
    }

    #[test]
    fn execute_help_command_stays_in_tui() {
        let mut app = App {
            command_input: String::from("help"),
            cursor_pos: 4,
            ..App::default()
        };
        // Use a simple command that exits successfully to avoid infinite recursion
        // when std::env::current_exe() returns the test binary
        app.set_exe_path_override(std::path::PathBuf::from("/bin/true"));

        app.execute_command();

        assert!(!app.should_quit);
        assert!(app.pending_command.is_none());
        // Status message should be set (either output or "(no output)" from /bin/true)
        assert!(!app.status_message.is_empty());
    }

    #[test]
    fn execute_version_command_stays_in_tui() {
        let mut app = App {
            command_input: String::from("version"),
            cursor_pos: 7,
            ..App::default()
        };
        // Use a simple command that exits successfully to avoid infinite recursion
        // when std::env::current_exe() returns the test binary
        app.set_exe_path_override(std::path::PathBuf::from("/bin/true"));

        app.execute_command();

        assert!(!app.should_quit);
        assert!(app.pending_command.is_none());
        // Status message should be set (either output or "(no output)" from /bin/true)
        assert!(!app.status_message.is_empty());
    }

    #[test]
    fn execute_unknown_command_shows_error() {
        let mut app = App {
            command_input: String::from("foobar"),
            cursor_pos: 6,
            ..App::default()
        };

        app.execute_command();

        assert!(!app.should_quit);
        assert!(app.status_message.contains("Unknown command"));
    }

    #[test]
    fn execute_empty_command_shows_message() {
        let mut app = App {
            command_input: String::from("   "),
            cursor_pos: 3,
            ..App::default()
        };

        app.execute_command();

        assert!(!app.should_quit);
        assert!(app.status_message.contains("No command"));
    }

    #[test]
    fn navigate_to_toolchains_changes_screen() {
        let mut app = App::default();
        app.navigate_to(Screen::Toolchains);
        assert_eq!(app.screen, Screen::Toolchains);
    }

    #[test]
    fn navigate_to_doctor_changes_screen() {
        let mut app = App::default();
        app.navigate_to(Screen::Doctor);
        assert_eq!(app.screen, Screen::Doctor);
    }

    #[test]
    fn shortcut_t_navigates_to_toolchains() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Toolchains);
    }

    #[test]
    fn shortcut_d_navigates_to_doctor() {
        let mut app = App::default();
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Doctor);
    }

    #[test]
    fn esc_from_toolchains_returns_to_main() {
        let mut app = App {
            screen: Screen::Toolchains,
            ..App::default()
        };
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn esc_from_doctor_returns_to_main() {
        let mut app = App {
            screen: Screen::Doctor,
            ..App::default()
        };
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn menu_navigation_with_arrows() {
        let mut app = App::default();
        assert_eq!(app.menu.selected(), 0);

        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.menu.selected(), 1);

        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.menu.selected(), 0);
    }

    #[test]
    fn menu_navigation_with_j_k() {
        let mut app = App::default();
        assert_eq!(app.menu.selected(), 0);

        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.menu.selected(), 1);

        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.menu.selected(), 0);
    }

    #[test]
    fn enter_activates_menu_item() {
        let mut app = App::default();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Toolchains);
    }

    #[test]
    fn command_doctor_navigates_to_doctor() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("doctor"),
            cursor_pos: 6,
            ..App::default()
        };
        app.execute_command();
        assert_eq!(app.screen, Screen::Doctor);
    }

    #[test]
    fn command_toolchains_navigates_to_toolchains() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("toolchains"),
            cursor_pos: 10,
            ..App::default()
        };
        app.execute_command();
        assert_eq!(app.screen, Screen::Toolchains);
    }

    #[test]
    fn tab_completion_single_match() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("bui"),
            cursor_pos: 3,
            ..App::default()
        };

        app.tab_complete();
        assert_eq!(app.command_input, "build");
        assert_eq!(app.cursor_pos, 5);
    }

    #[test]
    fn tab_completion_multiple_matches() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("ver"),
            cursor_pos: 3,
            ..App::default()
        };

        app.tab_complete();
        // Should show matches in status message
        assert!(app.status_message.contains("verify"));
        assert!(app.status_message.contains("version"));
    }

    #[test]
    fn tab_completion_no_match() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("xyz"),
            cursor_pos: 3,
            ..App::default()
        };

        app.tab_complete();
        assert!(app.status_message.contains("No matching"));
    }

    #[test]
    fn command_history_up_down() {
        let mut app = App {
            input_mode: InputMode::Command,
            ..App::default()
        };

        // Execute some commands
        app.command_input = String::from("build");
        app.cursor_pos = 5;
        app.execute_command();

        app.input_mode = InputMode::Command;
        app.command_input = String::from("doctor");
        app.cursor_pos = 6;
        app.execute_command();

        // Now enter command mode and navigate history
        app.input_mode = InputMode::Command;
        app.command_input.clear();
        app.cursor_pos = 0;

        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.command_input, "doctor");

        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.command_input, "build");

        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.command_input, "doctor");
    }

    #[test]
    fn esc_from_progress_when_completed() {
        let mut app = App {
            screen: Screen::Progress,
            progress_state: ProgressState::new("Test"),
            ..App::default()
        };
        app.progress_state.complete();

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn esc_from_progress_when_not_completed_cancels() {
        let mut app = App {
            screen: Screen::Progress,
            progress_state: ProgressState::new("Test"),
            ..App::default()
        };
        // Not completed, Esc should cancel the operation

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        // Should stay on progress screen but mark as completed with error
        assert_eq!(app.screen, Screen::Progress);
        assert!(app.progress_state.completed);
        assert!(app.progress_state.error.is_some());
    }

    #[test]
    fn cursor_display_pos_matches_chars() {
        let mut app = App {
            command_input: String::from("hello"),
            cursor_pos: 5,
            ..App::default()
        };
        assert_eq!(app.cursor_display_pos(), 5);

        // With unicode
        app.command_input = String::from("h\u{00e9}llo"); // e with accent
        app.cursor_pos = 6; // After the accented e (which is 2 bytes)
        assert_eq!(app.cursor_display_pos(), 5); // 5 characters
    }

    #[test]
    fn toolchains_enter_on_empty_shows_version_select() {
        let mut app = App {
            screen: Screen::Toolchains,
            toolchains_state: ToolchainsState::new(), // empty
            ..App::default()
        };

        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        // Should switch to version select screen
        assert_eq!(app.screen, Screen::VersionSelect);
        assert!(!app.should_quit);
        assert!(app.pending_command.is_none());
        assert_eq!(app.previous_screen, Some(Screen::Toolchains));
    }

    #[test]
    fn toolchains_i_on_empty_shows_version_select() {
        let mut app = App {
            screen: Screen::Toolchains,
            toolchains_state: ToolchainsState::new(), // empty
            ..App::default()
        };

        app.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);

        // Should switch to version select screen
        assert_eq!(app.screen, Screen::VersionSelect);
        assert!(!app.should_quit);
        assert!(app.pending_command.is_none());
        assert_eq!(app.previous_screen, Some(Screen::Toolchains));
    }

    #[test]
    fn toolchains_i_with_toolchains_shows_version_select() {
        let mut app = App {
            screen: Screen::Toolchains,
            toolchains_state: ToolchainsState {
                toolchains: vec![ToolchainInfo {
                    version: "0.1.0".to_string(),
                    is_default: true,
                    metadata: None,
                }],
                selected: 0,
                loaded: true,
            },
            ..App::default()
        };

        app.handle_key(KeyCode::Char('i'), KeyModifiers::NONE);

        // Should show version select screen
        assert!(!app.should_quit);
        assert!(app.pending_command.is_none());
        assert_eq!(app.screen, Screen::VersionSelect);
        assert_eq!(app.previous_screen, Some(Screen::Toolchains));
    }

    #[test]
    fn start_installation_switches_to_progress_screen() {
        let mut app = App {
            screen: Screen::Main,
            ..App::default()
        };

        app.start_installation(None);

        assert_eq!(app.screen, Screen::Progress);
        assert_eq!(app.previous_screen, Some(Screen::Main));
        assert!(app.install_receiver.is_some());
        assert!(!app.progress_state.items.is_empty());
    }

    #[test]
    fn poll_install_progress_updates_state_on_download_progress() {
        use std::sync::mpsc;

        let mut app = App::default();
        let (tx, rx) = mpsc::channel();
        app.install_receiver = Some(rx);
        app.progress_state = ProgressState::new("Test");
        app.progress_state.add_item(ProgressItem::new("Download"));

        // Send a download progress message
        tx.send(InstallProgress::DownloadProgress {
            downloaded: 512,
            speed: 1024,
        })
        .expect("Should send");

        app.poll_install_progress();

        let item = app.progress_state.items.first().expect("Should have item");
        assert_eq!(item.current, 512);
        assert_eq!(item.speed_bytes_per_sec, Some(1024));
    }

    #[test]
    fn poll_install_progress_handles_completion() {
        use std::sync::mpsc;

        let mut app = App::default();
        let (tx, rx) = mpsc::channel();
        app.install_receiver = Some(rx);
        app.progress_state = ProgressState::new("Test");
        app.progress_state.add_item(ProgressItem::new("Download"));

        // Send completion message
        tx.send(InstallProgress::Completed {
            version: String::from("0.1.0"),
        })
        .expect("Should send");

        app.poll_install_progress();

        assert!(app.progress_state.completed);
        assert!(app.install_receiver.is_none());
        assert!(app.progress_state.status.contains("0.1.0"));
    }

    #[test]
    fn poll_install_progress_handles_failure() {
        use std::sync::mpsc;

        let mut app = App::default();
        let (tx, rx) = mpsc::channel();
        app.install_receiver = Some(rx);
        app.progress_state = ProgressState::new("Test");

        // Send failure message
        tx.send(InstallProgress::Failed {
            error: String::from("Network error"),
        })
        .expect("Should send");

        app.poll_install_progress();

        assert!(app.progress_state.completed);
        assert!(app.progress_state.error.is_some());
        assert!(app.install_receiver.is_none());
    }

    #[test]
    fn return_from_progress_navigates_to_previous_screen() {
        let mut app = App {
            screen: Screen::Progress,
            previous_screen: Some(Screen::Toolchains),
            progress_state: ProgressState::new("Test"),
            ..App::default()
        };
        app.progress_state.complete();

        app.return_from_progress();

        assert_eq!(app.screen, Screen::Toolchains);
        assert!(app.previous_screen.is_none());
        assert!(app.install_receiver.is_none());
    }

    #[test]
    fn cancel_installation_sets_error() {
        use std::sync::mpsc;

        let mut app = App::default();
        let (_tx, rx) = mpsc::channel::<InstallProgress>();
        app.install_receiver = Some(rx);
        app.progress_state = ProgressState::new("Test");

        app.cancel_installation();

        assert!(app.progress_state.completed);
        assert!(app.progress_state.error.is_some());
        assert!(app.install_receiver.is_none());
    }

    #[test]
    fn version_select_esc_returns_to_previous_screen() {
        let mut app = App {
            screen: Screen::VersionSelect,
            previous_screen: Some(Screen::Toolchains),
            ..App::default()
        };

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Toolchains);
        assert!(app.previous_screen.is_none());
    }

    #[test]
    fn version_select_navigation() {
        let mut app = App {
            screen: Screen::VersionSelect,
            version_select_state: VersionSelectState {
                versions: vec![
                    VersionSelectInfo {
                        version: "0.2.0".to_string(),
                        stable: true,
                        platforms: vec!["linux".to_string()],
                        available_for_current: true,
                    },
                    VersionSelectInfo {
                        version: "0.1.0".to_string(),
                        stable: true,
                        platforms: vec!["linux".to_string()],
                        available_for_current: true,
                    },
                ],
                selected: 0,
                loaded: true,
                loading: false,
                error: None,
                current_os: "linux".to_string(),
            },
            ..App::default()
        };

        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.version_select_state.selected, 1);

        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.version_select_state.selected, 0);

        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.version_select_state.selected, 1);

        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.version_select_state.selected, 0);
    }

    #[test]
    fn version_select_enter_on_available_starts_install() {
        let mut app = App {
            screen: Screen::VersionSelect,
            previous_screen: Some(Screen::Toolchains),
            version_select_state: VersionSelectState {
                versions: vec![VersionSelectInfo {
                    version: "0.2.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string()],
                    available_for_current: true,
                }],
                selected: 0,
                loaded: true,
                loading: false,
                error: None,
                current_os: "linux".to_string(),
            },
            ..App::default()
        };

        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.screen, Screen::Progress);
        assert!(app.install_receiver.is_some());
    }

    #[test]
    fn version_select_enter_on_unavailable_shows_error() {
        let mut app = App {
            screen: Screen::VersionSelect,
            previous_screen: Some(Screen::Toolchains),
            version_select_state: VersionSelectState {
                versions: vec![VersionSelectInfo {
                    version: "0.2.0".to_string(),
                    stable: true,
                    platforms: vec!["macos".to_string()],
                    available_for_current: false,
                }],
                selected: 0,
                loaded: true,
                loading: false,
                error: None,
                current_os: "linux".to_string(),
            },
            ..App::default()
        };

        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        // Should stay on version select screen
        assert_eq!(app.screen, Screen::VersionSelect);
        assert!(app.status_message.contains("not available"));
    }

    #[test]
    fn poll_version_loading_updates_state_on_success() {
        use std::sync::mpsc;

        let mut app = App::default();
        let (tx, rx) = mpsc::channel();
        app.version_load_receiver = Some(rx);
        app.version_select_state.loading = true;

        let versions = vec![VersionSelectInfo {
            version: "0.1.0".to_string(),
            stable: true,
            platforms: vec!["linux".to_string()],
            available_for_current: true,
        }];

        tx.send(Ok(versions.clone())).expect("Should send");

        app.poll_version_loading();

        assert!(app.version_select_state.loaded);
        assert!(!app.version_select_state.loading);
        assert_eq!(app.version_select_state.versions.len(), 1);
        assert!(app.version_load_receiver.is_none());
    }

    #[test]
    fn poll_version_loading_updates_state_on_error() {
        use std::sync::mpsc;

        let mut app = App::default();
        let (tx, rx) = mpsc::channel();
        app.version_load_receiver = Some(rx);
        app.version_select_state.loading = true;

        tx.send(Err("Network error".to_string()))
            .expect("Should send");

        app.poll_version_loading();

        assert!(app.version_select_state.loaded);
        assert!(!app.version_select_state.loading);
        assert!(app.version_select_state.error.is_some());
        assert!(app.version_load_receiver.is_none());
    }
}
