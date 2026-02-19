//! Main view rendering for the TUI.
//!
//! This module contains the rendering logic for the main menu screen,
//! including the logo, menu items, command input, and status line.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::menu::{MENU_ITEMS, Menu};
use crate::tui::theme::Theme;

/// Renders the main view.
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    menu: &Menu,
    command_input: &str,
    is_command_mode: bool,
    status_message: &str,
    cursor_pos: usize,
) {
    let chunks = Layout::vertical([
        Constraint::Length(8), // Logo and version
        Constraint::Min(6),    // Menu
        Constraint::Length(3), // Input line
        Constraint::Length(1), // Status
    ])
    .split(area);

    render_header(frame, chunks[0], theme);
    render_menu(frame, chunks[1], theme, menu);
    render_input(
        frame,
        chunks[2],
        theme,
        command_input,
        is_command_mode,
        cursor_pos,
    );
    render_status(frame, chunks[3], theme, status_message);
}

/// Renders the header with colorful "I" logo and version/directory info.
fn render_header(frame: &mut Frame, area: Rect, theme: &Theme) {
    // Split header into logo (left) and info (right)
    let header_chunks = Layout::horizontal([
        Constraint::Length(14), // Logo width
        Constraint::Min(20),    // Info area
    ])
    .split(area);

    render_logo(frame, header_chunks[0]);
    render_info(frame, header_chunks[1], theme);
}

/// Renders the Inference-style lowercase "i" logo.
fn render_logo(frame: &mut Frame, area: Rect) {
    use ratatui::style::Color;

    // Dot color: #810f0c (dark red) - matches Inference branding
    let dot_color = Color::Rgb(0x81, 0x0f, 0x0c);

    // Stem color: white/light for visibility (like the logo outline)
    let stem_color = Color::White;

    // Inference-style calligraphic "i"
    let styled_lines = vec![
        // Dot (red circle)
        Line::from(Span::styled("    ██    ", Style::default().fg(dot_color))),
        // Gap between dot and stem
        Line::from(""),
        // Top of stem with left serif
        Line::from(Span::styled("   ███    ", Style::default().fg(stem_color))),
        // Stem
        Line::from(Span::styled("    ██    ", Style::default().fg(stem_color))),
        Line::from(Span::styled("    ██    ", Style::default().fg(stem_color))),
        // Bottom curve sweeping left
        Line::from(Span::styled("  ██████  ", Style::default().fg(stem_color))),
    ];

    let logo = Paragraph::new(styled_lines).alignment(Alignment::Left);
    frame.render_widget(logo, area);
}

/// Renders the version and directory info.
fn render_info(frame: &mut Frame, area: Rect, theme: &Theme) {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let cwd = std::env::current_dir()
        .map_or_else(|_| String::from("<unknown>"), |p| p.display().to_string());

    // Truncate directory if too long
    let max_dir_len = area.width.saturating_sub(15) as usize;
    let display_cwd = if cwd.len() > max_dir_len && max_dir_len > 3 {
        format!("...{}", &cwd[cwd.len() - (max_dir_len - 3)..])
    } else {
        cwd
    };

    let info_lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Inference Toolchain",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Version:   ", Style::default().fg(theme.muted)),
            Span::styled(&version, Style::default().fg(theme.highlight)),
        ]),
        Line::from(vec![
            Span::styled("Directory: ", Style::default().fg(theme.muted)),
            Span::raw(&display_cwd),
        ]),
    ];

    let info = Paragraph::new(info_lines).alignment(Alignment::Left);
    frame.render_widget(info, area);
}

/// Renders the menu with navigation indicators.
fn render_menu(frame: &mut Frame, area: Rect, theme: &Theme, menu: &Menu) {
    let mut lines = Vec::with_capacity(MENU_ITEMS.len() + 2);

    for (idx, item) in MENU_ITEMS.iter().enumerate() {
        let is_selected = idx == menu.selected();

        let prefix = if is_selected { "> " } else { "  " };
        let key_style = if is_selected {
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.highlight)
                .add_modifier(Modifier::BOLD)
        };
        let label_style = if is_selected {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, label_style),
            Span::styled(format!("[{}] ", item.key), key_style),
            Span::styled(item.label, label_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            "Use arrows or keys to navigate, Enter to select, : for commands",
            Style::default().fg(theme.muted),
        ),
    ]));

    let menu_widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Menu ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(menu_widget, area);
}

/// Renders the command input line.
///
/// Uses the official ratatui `user_input` example pattern for cursor positioning.
fn render_input(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    command_input: &str,
    is_command_mode: bool,
    cursor_pos: usize,
) {
    let (input_text, cursor_style) = if is_command_mode {
        (format!(":{command_input}"), Style::default().fg(theme.text))
    } else {
        (
            String::from("Press ':' to enter command mode"),
            Style::default().fg(theme.muted),
        )
    };

    let input = Paragraph::new(input_text).style(cursor_style).block(
        Block::default()
            .title(" Input ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(input, area);

    if is_command_mode {
        // Official ratatui pattern: use outer area coordinates + 1 for borders
        // cursor_x: area.x + 1 (left border) + 1 (colon) + cursor position
        // cursor_y: area.y + 1 (top border)
        #[allow(clippy::cast_possible_truncation)]
        frame.set_cursor_position(Position::new(
            area.x + 1 + 1 + cursor_pos as u16,
            area.y + 1,
        ));
    }
}

/// Renders the status message line.
fn render_status(frame: &mut Frame, area: Rect, theme: &Theme, status_message: &str) {
    let status = Paragraph::new(status_message).style(Style::default().fg(theme.muted));
    frame.render_widget(status, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn create_test_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(80, 24);
        Terminal::new(backend).expect("Should create terminal")
    }

    #[test]
    fn render_main_view_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let menu = Menu::new();

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &menu, "", false, "", 0);
            })
            .expect("Should render");
    }

    #[test]
    fn render_command_mode_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let menu = Menu::new();

        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    &theme,
                    &menu,
                    "build --parse",
                    true,
                    "Ready",
                    5,
                );
            })
            .expect("Should render");
    }

    #[test]
    fn render_with_menu_selection_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let mut menu = Menu::new();
        menu.down();

        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    &theme,
                    &menu,
                    "",
                    false,
                    "Status message",
                    0,
                );
            })
            .expect("Should render");
    }

    #[test]
    fn render_with_long_command_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let menu = Menu::new();
        let long_command = "build very/long/path/to/file.inf --parse --analyze --codegen -o";

        terminal
            .draw(|frame| {
                render(
                    frame,
                    frame.area(),
                    &theme,
                    &menu,
                    long_command,
                    true,
                    "",
                    long_command.len(),
                );
            })
            .expect("Should render");
    }
}
