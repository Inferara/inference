//! Toolchain view rendering for the TUI.
//!
//! This module contains the rendering logic for the installed toolchains screen,
//! showing a list of toolchain versions with their installation details.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::state::ToolchainsState;
use crate::tui::theme::Theme;

/// Renders the toolchains view.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, state: &ToolchainsState) {
    let chunks = Layout::vertical([
        Constraint::Min(6),    // Toolchain list
        Constraint::Length(3), // Help text
    ])
    .split(area);

    render_toolchain_list(frame, chunks[0], theme, state);
    render_help(frame, chunks[1], theme, state.toolchains.is_empty());
}

/// Renders the toolchain list.
fn render_toolchain_list(frame: &mut Frame, area: Rect, theme: &Theme, state: &ToolchainsState) {
    let mut lines = Vec::new();

    if state.toolchains.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  No toolchains installed.",
            Style::default().fg(theme.muted),
        )]));
        lines.push(Line::from(""));
        // Show selectable "Install" option
        lines.push(Line::from(vec![
            Span::styled(
                "> ",
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[i] ",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Install latest toolchain",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        for (idx, toolchain) in state.toolchains.iter().enumerate() {
            let is_selected = idx == state.selected;

            let prefix = if is_selected { "> " } else { "  " };

            let version_style = if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let default_indicator = if toolchain.is_default {
                Span::styled(" (default)", Style::default().fg(theme.success))
            } else {
                Span::raw("")
            };

            let installed_ago = toolchain.metadata.as_ref().map_or_else(String::new, |m| {
                format!(" - installed {}", m.installed_ago())
            });

            lines.push(Line::from(vec![
                Span::styled(prefix, version_style),
                Span::styled(&toolchain.version, version_style),
                default_indicator,
                Span::styled(installed_ago, Style::default().fg(theme.muted)),
            ]));
        }
    }

    let list_widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Installed Toolchains ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(list_widget, area);
}

/// Renders the help text at the bottom.
fn render_help(frame: &mut Frame, area: Rect, theme: &Theme, is_empty: bool) {
    let help_text = if is_empty {
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(theme.highlight)),
            Span::styled("Back", Style::default().fg(theme.muted)),
            Span::raw("  "),
            Span::styled("[i/Enter] ", Style::default().fg(theme.highlight)),
            Span::styled("Install", Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(theme.highlight)),
            Span::styled("Back", Style::default().fg(theme.muted)),
            Span::raw("  "),
            Span::styled("[Up/Down] ", Style::default().fg(theme.highlight)),
            Span::styled("Navigate", Style::default().fg(theme.muted)),
            Span::raw("  "),
            Span::styled("[Enter] ", Style::default().fg(theme.highlight)),
            Span::styled("Set default", Style::default().fg(theme.muted)),
            Span::raw("  "),
            Span::styled("[i] ", Style::default().fg(theme.highlight)),
            Span::styled("Install", Style::default().fg(theme.muted)),
        ])
    };

    let help = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(help, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::ToolchainInfo;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn create_test_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(80, 24);
        Terminal::new(backend).expect("Should create terminal")
    }

    #[test]
    fn render_empty_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = ToolchainsState::default();

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_with_toolchains_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = ToolchainsState {
            toolchains: vec![
                ToolchainInfo {
                    version: "0.2.0".to_string(),
                    is_default: true,
                    metadata: None,
                },
                ToolchainInfo {
                    version: "0.1.0".to_string(),
                    is_default: false,
                    metadata: None,
                },
            ],
            selected: 0,
            loaded: true,
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_with_selection_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = ToolchainsState {
            toolchains: vec![
                ToolchainInfo {
                    version: "0.2.0".to_string(),
                    is_default: true,
                    metadata: None,
                },
                ToolchainInfo {
                    version: "0.1.0".to_string(),
                    is_default: false,
                    metadata: None,
                },
            ],
            selected: 1,
            loaded: true,
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_single_default_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = ToolchainsState {
            toolchains: vec![ToolchainInfo {
                version: "0.1.0".to_string(),
                is_default: true,
                metadata: None,
            }],
            selected: 0,
            loaded: true,
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }
}
