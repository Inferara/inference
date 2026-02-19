//! Version select view rendering for the TUI.
//!
//! This module contains the rendering logic for the version selection screen,
//! showing available versions with their stability and platform availability.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::state::VersionSelectState;
use crate::tui::theme::Theme;

/// Renders the version select view.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, state: &VersionSelectState) {
    let chunks = Layout::vertical([
        Constraint::Min(6),    // Version list
        Constraint::Length(3), // Help text
    ])
    .split(area);

    render_version_list(frame, chunks[0], theme, state);
    render_help(frame, chunks[1], theme, state);
}

/// Renders the version list.
fn render_version_list(frame: &mut Frame, area: Rect, theme: &Theme, state: &VersionSelectState) {
    let mut lines = Vec::new();

    if state.loading {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  Loading versions...",
            Style::default().fg(theme.muted),
        )]));
    } else if let Some(error) = &state.error {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("  Error: {error}"),
            Style::default().fg(theme.error),
        )]));
    } else if state.versions.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  No versions available.",
            Style::default().fg(theme.muted),
        )]));
    } else {
        for (idx, version) in state.versions.iter().enumerate() {
            let is_selected = idx == state.selected;

            let prefix = if is_selected { "> " } else { "  " };

            let base_style = if !version.available_for_current {
                Style::default().fg(theme.muted)
            } else if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let stability_style = if version.stable {
                Style::default().fg(theme.success)
            } else {
                Style::default().fg(theme.warning)
            };

            let stability = if version.stable {
                "(stable)"
            } else {
                "(prerelease)"
            };

            let platform_list = if version.platforms.is_empty() {
                String::new()
            } else {
                format!("[{}]", version.platforms.join(", "))
            };

            let unavailable_marker = if version.available_for_current {
                Span::raw("")
            } else {
                Span::styled(" (not available)", Style::default().fg(theme.error))
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, base_style),
                Span::styled(&version.version, base_style),
                Span::raw(" "),
                Span::styled(stability, stability_style),
                Span::raw(" "),
                Span::styled(platform_list, Style::default().fg(theme.muted)),
                unavailable_marker,
            ]));
        }
    }

    let title = format!(" Select Version (current: {}) ", state.current_os);
    let list_widget = Paragraph::new(lines).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(list_widget, area);
}

/// Renders the help text at the bottom.
fn render_help(frame: &mut Frame, area: Rect, theme: &Theme, state: &VersionSelectState) {
    let help_text = if state.loading || state.error.is_some() || state.versions.is_empty() {
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(theme.highlight)),
            Span::styled("Cancel", Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(theme.highlight)),
            Span::styled("Cancel", Style::default().fg(theme.muted)),
            Span::raw("  "),
            Span::styled("[Up/Down] ", Style::default().fg(theme.highlight)),
            Span::styled("Navigate", Style::default().fg(theme.muted)),
            Span::raw("  "),
            Span::styled("[Enter] ", Style::default().fg(theme.highlight)),
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
    use crate::tui::state::VersionSelectInfo;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn create_test_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(80, 24);
        Terminal::new(backend).expect("Should create terminal")
    }

    #[test]
    fn render_loading_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = VersionSelectState {
            loading: true,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_error_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = VersionSelectState {
            error: Some("Network error".to_string()),
            loaded: true,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_empty_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = VersionSelectState {
            loaded: true,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_with_versions_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = VersionSelectState {
            versions: vec![
                VersionSelectInfo {
                    version: "0.2.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string(), "macos".to_string()],
                    available_for_current: true,
                },
                VersionSelectInfo {
                    version: "0.1.0".to_string(),
                    stable: true,
                    platforms: vec!["linux".to_string()],
                    available_for_current: true,
                },
                VersionSelectInfo {
                    version: "0.3.0-alpha".to_string(),
                    stable: false,
                    platforms: vec!["macos".to_string()],
                    available_for_current: false,
                },
            ],
            selected: 0,
            loaded: true,
            loading: false,
            error: None,
            current_os: "linux".to_string(),
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
        let state = VersionSelectState {
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
            selected: 1,
            loaded: true,
            loading: false,
            error: None,
            current_os: "linux".to_string(),
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }
}
