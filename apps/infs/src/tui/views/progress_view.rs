//! Progress view rendering for the TUI.
//!
//! This module contains the rendering logic for the progress screen,
//! showing download progress and operation status.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};

use crate::tui::state::ProgressState;
use crate::tui::theme::Theme;

/// Renders the progress view.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, state: &ProgressState) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // Title and overall progress
        Constraint::Min(6),    // Progress items
        Constraint::Length(3), // Status/help
    ])
    .split(area);

    render_header(frame, chunks[0], theme, state);
    render_items(frame, chunks[1], theme, state);
    render_footer(frame, chunks[2], theme, state);
}

/// Renders the header with title and overall progress bar.
fn render_header(frame: &mut Frame, area: Rect, theme: &Theme, state: &ProgressState) {
    let percentage = state.overall_percentage();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent_u16 = (percentage * 100.0) as u16;

    let label = if state.completed {
        if state.error.is_some() {
            "Failed".to_string()
        } else {
            "Complete".to_string()
        }
    } else {
        format!("{percent_u16}%")
    };

    let gauge_style = if state.error.is_some() {
        Style::default().fg(theme.error)
    } else if state.completed {
        Style::default().fg(theme.success)
    } else {
        Style::default().fg(theme.highlight)
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(format!(" {} ", state.title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        )
        .gauge_style(gauge_style)
        .label(label)
        .ratio(percentage);

    frame.render_widget(gauge, area);
}

/// Renders the list of progress items.
fn render_items(frame: &mut Frame, area: Rect, theme: &Theme, state: &ProgressState) {
    let mut lines = Vec::new();

    if state.items.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  Waiting...",
            Style::default().fg(theme.muted),
        )]));
    } else {
        for item in &state.items {
            let status_indicator = if item.completed {
                Span::styled("[OK] ", Style::default().fg(theme.success))
            } else if item.total > 0 {
                let pct = item.percentage();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pct_u8 = (pct * 100.0) as u8;
                Span::styled(
                    format!("[{pct_u8:3}%] "),
                    Style::default().fg(theme.highlight),
                )
            } else {
                Span::styled("[...] ", Style::default().fg(theme.muted))
            };

            let desc_style = if item.completed {
                Style::default().fg(theme.muted)
            } else {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            };

            let progress_text = Span::styled(
                format!("  {}", item.format_progress()),
                Style::default().fg(theme.muted),
            );

            let speed_text = if item.completed {
                Span::raw("")
            } else {
                let speed = item.format_speed();
                if speed.is_empty() {
                    Span::raw("")
                } else {
                    Span::styled(format!("  {speed}"), Style::default().fg(theme.highlight))
                }
            };

            lines.push(Line::from(vec![
                Span::raw("  "),
                status_indicator,
                Span::styled(&item.description, desc_style),
                progress_text,
                speed_text,
            ]));
        }
    }

    let items_widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Progress ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(items_widget, area);
}

/// Renders the footer with status message and help text.
fn render_footer(frame: &mut Frame, area: Rect, theme: &Theme, state: &ProgressState) {
    let status_text = if let Some(ref error) = state.error {
        Line::from(vec![
            Span::styled("Error: ", Style::default().fg(theme.error)),
            Span::styled(error.as_str(), Style::default().fg(theme.error)),
        ])
    } else if state.completed {
        Line::from(vec![Span::styled(
            "Operation completed. Press Esc to continue.",
            Style::default().fg(theme.success),
        )])
    } else if state.status.is_empty() {
        Line::from(vec![Span::styled(
            "Please wait...",
            Style::default().fg(theme.muted),
        )])
    } else {
        Line::from(vec![Span::styled(
            state.status.as_str(),
            Style::default().fg(theme.muted),
        )])
    };

    let footer = Paragraph::new(status_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    );

    frame.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn create_test_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(80, 24);
        Terminal::new(backend).expect("Failed to create test terminal")
    }

    #[test]
    fn render_empty_progress_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let state = ProgressState::new("Test Operation");

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Failed to draw");
    }

    #[test]
    fn render_completed_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let mut state = ProgressState::new("Complete");
        state.complete();

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Failed to draw");
    }

    #[test]
    fn render_error_does_not_panic() {
        let mut terminal = create_test_terminal();
        let theme = Theme::dark();
        let mut state = ProgressState::new("Failed");
        state.set_error("Connection failed");

        terminal
            .draw(|frame| {
                render(frame, frame.area(), &theme, &state);
            })
            .expect("Failed to draw");
    }
}
