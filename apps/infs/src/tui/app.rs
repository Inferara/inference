#![warn(clippy::pedantic)]

//! Main TUI application logic.
//!
//! This module contains the event loop, state management, and rendering
//! for the infs TUI interface.
//!
//! ## Input Modes
//!
//! - **Normal**: Default mode, shortcuts work directly (q to quit, : to enter command)
//! - **Command**: Input mode for entering commands with `:` prefix

use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::terminal::TerminalGuard;

/// Event polling timeout in milliseconds.
const POLL_TIMEOUT_MS: u64 = 100;

/// Input mode for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum InputMode {
    /// Normal mode: shortcuts work directly.
    #[default]
    Normal,
    /// Command mode: typing a command.
    Command,
}

/// Main application state.
struct App {
    /// Current input mode.
    input_mode: InputMode,
    /// Command input buffer.
    command_input: String,
    /// Status message to display.
    status_message: String,
    /// Whether the application should quit.
    should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input_mode: InputMode::Normal,
            command_input: String::new(),
            status_message: String::from("Press ':' to enter a command, 'q' to quit"),
            should_quit: false,
        }
    }
}

impl App {
    /// Handles a key event in normal mode.
    fn handle_normal_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), _) => {
                self.should_quit = true;
            }
            (KeyCode::Char(':'), _) => {
                self.input_mode = InputMode::Command;
                self.command_input.clear();
                self.status_message = String::from("Enter command (Esc to cancel)");
            }
            _ => {}
        }
    }

    /// Handles a key event in command mode.
    fn handle_command_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Esc, _) => {
                self.input_mode = InputMode::Normal;
                self.command_input.clear();
                self.status_message = String::from("Command cancelled");
            }
            (KeyCode::Enter, _) => {
                self.execute_command();
            }
            (KeyCode::Backspace, _) => {
                self.command_input.pop();
            }
            (KeyCode::Char(c), _) => {
                self.command_input.push(c);
            }
            _ => {}
        }
    }

    /// Executes the current command.
    fn execute_command(&mut self) {
        let command = self.command_input.trim().to_lowercase();
        self.command_input.clear();
        self.input_mode = InputMode::Normal;

        if command.is_empty() {
            self.status_message = String::from("No command entered");
            return;
        }

        match command.as_str() {
            "q" | "quit" | "exit" => {
                self.should_quit = true;
            }
            "build" | "new" | "install" | "doctor" | "help" | "version" => {
                self.status_message =
                    format!("Running 'infs {command}'... (not implemented in TUI)");
            }
            _ => {
                self.status_message = format!("Unknown command: {command}");
            }
        }
    }
}

/// Runs the main TUI event loop.
///
/// # Errors
///
/// Returns an error if:
/// - Terminal setup fails
/// - Drawing fails
/// - Event polling fails
pub fn run_app(guard: &mut TerminalGuard) -> Result<()> {
    let mut app = App::default();

    loop {
        guard
            .terminal
            .draw(|frame| render(&app, frame))
            .context("failed to draw frame")?;

        if event::poll(Duration::from_millis(POLL_TIMEOUT_MS)).context("event poll failed")?
            && let Event::Key(key) = event::read().context("failed to read event")?
            && key.kind == KeyEventKind::Press
        {
            match app.input_mode {
                InputMode::Normal => app.handle_normal_key(key.code, key.modifiers),
                InputMode::Command => app.handle_command_key(key.code, key.modifiers),
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Renders the TUI.
fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::vertical([
        Constraint::Length(8), // Logo and version
        Constraint::Min(6),    // Shortcuts
        Constraint::Length(3), // Input line
        Constraint::Length(1), // Status
    ])
    .split(area);

    render_header(frame, chunks[0]);
    render_shortcuts(frame, chunks[1]);
    render_input(app, frame, chunks[2]);
    render_status(app, frame, chunks[3]);
}

/// Renders the header with logo and version.
fn render_header(frame: &mut Frame, area: Rect) {
    let logo = r"
  _____       __
 |_   _|     / _| ___ _ __ ___ _ __   ___ ___
   | | _ __ | |_ / _ \ '__/ _ \ '_ \ / __/ _ \
   | || '_ \|  _|  __/ | |  __/ | | | (_|  __/
  _|_||_| |_||_|  \___|_|  \___|_| |_|\___\___|
";
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let cwd = std::env::current_dir()
        .map_or_else(|_| String::from("<unknown>"), |p| p.display().to_string());

    let header_text = vec![
        Line::from(logo.trim_start_matches('\n')),
        Line::from(""),
        Line::from(vec![
            Span::styled("Version: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&version),
            Span::raw("  "),
            Span::styled("Directory: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&cwd),
        ]),
    ];

    let header = Paragraph::new(header_text)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(header, area);
}

/// Renders the command shortcuts section.
fn render_shortcuts(frame: &mut Frame, area: Rect) {
    let shortcuts = vec![
        Line::from(vec![
            Span::styled(
                "  :build    ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Compile Inference source files"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :new      ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Create a new Inference project"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :install  ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Install toolchain components"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :doctor   ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Check installation health"),
        ]),
        Line::from(vec![
            Span::styled(
                "  :help     ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Show help information"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  q         ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Quit"),
        ]),
    ];

    let shortcuts_widget =
        Paragraph::new(shortcuts).block(Block::default().title(" Commands ").borders(Borders::ALL));

    frame.render_widget(shortcuts_widget, area);
}

/// Renders the command input line.
fn render_input(app: &App, frame: &mut Frame, area: Rect) {
    let (input_text, cursor_style) = match app.input_mode {
        InputMode::Normal => (
            String::from("Press ':' to enter command mode"),
            Style::default().fg(Color::DarkGray),
        ),
        InputMode::Command => (
            format!(":{}", app.command_input),
            Style::default().fg(Color::White),
        ),
    };

    let input = Paragraph::new(input_text)
        .style(cursor_style)
        .block(Block::default().title(" Input ").borders(Borders::ALL));

    frame.render_widget(input, area);

    if app.input_mode == InputMode::Command {
        #[allow(clippy::cast_possible_truncation)]
        let cursor_x = area.x + 1 + app.command_input.len() as u16 + 1;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Renders the status message line.
fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let status =
        Paragraph::new(app.status_message.as_str()).style(Style::default().fg(Color::DarkGray));

    frame.render_widget(status, area);
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
    }

    #[test]
    fn normal_mode_q_sets_should_quit() {
        let mut app = App::default();
        app.handle_normal_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn normal_mode_ctrl_c_sets_should_quit() {
        let mut app = App::default();
        app.handle_normal_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn normal_mode_colon_enters_command_mode() {
        let mut app = App::default();
        app.handle_normal_key(KeyCode::Char(':'), KeyModifiers::NONE);
        assert_eq!(app.input_mode, InputMode::Command);
    }

    #[test]
    fn command_mode_esc_returns_to_normal() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("test"),
            ..App::default()
        };

        app.handle_command_key(KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.command_input.is_empty());
    }

    #[test]
    fn command_mode_char_adds_to_input() {
        let mut app = App {
            input_mode: InputMode::Command,
            ..App::default()
        };

        app.handle_command_key(KeyCode::Char('h'), KeyModifiers::NONE);
        app.handle_command_key(KeyCode::Char('i'), KeyModifiers::NONE);

        assert_eq!(app.command_input, "hi");
    }

    #[test]
    fn command_mode_backspace_removes_char() {
        let mut app = App {
            input_mode: InputMode::Command,
            command_input: String::from("hi"),
            ..App::default()
        };

        app.handle_command_key(KeyCode::Backspace, KeyModifiers::NONE);

        assert_eq!(app.command_input, "h");
    }

    #[test]
    fn execute_quit_command_sets_should_quit() {
        let mut app = App {
            command_input: String::from("quit"),
            ..App::default()
        };

        app.execute_command();

        assert!(app.should_quit);
    }

    #[test]
    fn execute_known_command_shows_message() {
        let mut app = App {
            command_input: String::from("build"),
            ..App::default()
        };

        app.execute_command();

        assert!(!app.should_quit);
        assert!(app.status_message.contains("build"));
    }

    #[test]
    fn execute_unknown_command_shows_error() {
        let mut app = App {
            command_input: String::from("foobar"),
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
            ..App::default()
        };

        app.execute_command();

        assert!(!app.should_quit);
        assert!(app.status_message.contains("No command"));
    }
}
