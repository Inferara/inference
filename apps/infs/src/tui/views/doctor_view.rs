//! Doctor view rendering for the TUI.
//!
//! This module contains the rendering logic for the doctor check results screen,
//! showing the status of each health check.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::state::{DoctorCheckStatus, DoctorState};
use crate::tui::theme::Theme;

/// Renders the doctor view.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, state: &DoctorState) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // Summary
        Constraint::Min(6),    // Check list
        Constraint::Length(3), // Help text
    ])
    .split(area);

    render_summary(frame, chunks[0], theme, state);
    render_check_list(frame, chunks[1], theme, state);
    render_help(frame, chunks[2], theme);
}

/// Renders the summary bar showing counts.
fn render_summary(frame: &mut Frame, area: Rect, theme: &Theme, state: &DoctorState) {
    let ok_count = state.ok_count();
    let warning_count = state.warning_count();
    let error_count = state.error_count();

    let summary_line = Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{ok_count} passed"),
            Style::default().fg(theme.success),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("{warning_count} warnings"),
            Style::default().fg(theme.warning),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("{error_count} failed"),
            Style::default().fg(theme.error),
        ),
    ]);

    let summary = Paragraph::new(summary_line).block(
        Block::default()
            .title(" Summary ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(summary, area);
}

/// Renders the check results list.
fn render_check_list(frame: &mut Frame, area: Rect, theme: &Theme, state: &DoctorState) {
    let mut lines = Vec::new();

    if state.checks.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  Loading checks...",
            Style::default().fg(theme.muted),
        )]));
    } else {
        for (idx, check) in state.checks.iter().enumerate() {
            let is_selected = idx == state.selected;

            let status_indicator = match check.status {
                DoctorCheckStatus::Ok => Span::styled("[OK]  ", Style::default().fg(theme.success)),
                DoctorCheckStatus::Warning => {
                    Span::styled("[WARN]", Style::default().fg(theme.warning))
                }
                DoctorCheckStatus::Error => {
                    Span::styled("[FAIL]", Style::default().fg(theme.error))
                }
            };

            let name_style = if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let prefix = if is_selected { "> " } else { "  " };

            lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                status_indicator,
                Span::raw(" "),
                Span::styled(&check.name, name_style),
            ]));

            let message_style = Style::default().fg(theme.muted);

            lines.push(Line::from(vec![
                Span::raw("         "),
                Span::styled(&check.message, message_style),
            ]));

            lines.push(Line::from(""));
        }
    }

    let list_widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Check Results ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(list_widget, area);
}

/// Renders the help text at the bottom.
fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let help_text = Line::from(vec![
        Span::styled("[Esc] ", Style::default().fg(theme.highlight)),
        Span::styled("Back", Style::default().fg(theme.muted)),
        Span::raw("  "),
        Span::styled("[Up/Down] ", Style::default().fg(theme.highlight)),
        Span::styled("Navigate", Style::default().fg(theme.muted)),
        Span::raw("  "),
        Span::styled("[r] ", Style::default().fg(theme.highlight)),
        Span::styled("Refresh", Style::default().fg(theme.muted)),
    ]);

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
    use crate::tui::state::DoctorCheck;
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
        let state = DoctorState::default();

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }

    #[test]
    fn render_with_checks_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = DoctorState {
            checks: vec![
                DoctorCheck::ok("Platform", "linux x64"),
                DoctorCheck::warning("Toolchain", "No default set"),
                DoctorCheck::error("inf-llc", "Not found"),
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
    fn render_all_ok_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = DoctorState {
            checks: vec![
                DoctorCheck::ok("Platform", "linux x64"),
                DoctorCheck::ok("Toolchain directory", "~/.inference"),
                DoctorCheck::ok("Default toolchain", "0.1.0"),
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
    fn render_all_errors_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = DoctorState {
            checks: vec![
                DoctorCheck::error("inf-llc", "Not found"),
                DoctorCheck::error("rust-lld", "Not found"),
                DoctorCheck::error("infc", "Not found"),
            ],
            selected: 2,
            loaded: true,
        };

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Should render");
    }
}
