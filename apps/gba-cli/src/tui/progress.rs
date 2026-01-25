//! Progress display component for the TUI.
//!
//! This module provides a progress bar and phase tracker widget
//! for displaying execution progress during `gba run`.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};

/// Phase execution status for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseDisplayStatus {
    /// Phase not started.
    Pending,
    /// Phase currently executing.
    InProgress,
    /// Phase completed successfully.
    Completed,
    /// Phase failed.
    Failed,
}

impl PhaseDisplayStatus {
    /// Get the icon for this status.
    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◐",
            Self::Completed => "●",
            Self::Failed => "✗",
        }
    }

    /// Get the color for this status.
    #[must_use]
    pub fn color(&self) -> Color {
        match self {
            Self::Pending => Color::DarkGray,
            Self::InProgress => Color::Yellow,
            Self::Completed => Color::Green,
            Self::Failed => Color::Red,
        }
    }
}

/// A phase in the execution pipeline.
#[derive(Debug, Clone)]
pub struct PhaseInfo {
    /// Phase name.
    pub name: String,
    /// Current status.
    pub status: PhaseDisplayStatus,
    /// Optional commit SHA if completed.
    pub commit_sha: Option<String>,
}

/// Progress display state.
#[derive(Debug, Clone)]
pub struct ProgressState {
    /// Feature slug being executed.
    pub feature_slug: String,
    /// List of phases.
    pub phases: Vec<PhaseInfo>,
    /// Current phase index.
    pub current_phase: usize,
    /// Total execution turns so far.
    pub total_turns: u32,
    /// Total cost so far.
    pub total_cost: f64,
    /// Current activity message.
    pub activity: String,
}

impl ProgressState {
    /// Create a new progress state.
    #[must_use]
    pub fn new(feature_slug: String, phases: Vec<PhaseInfo>) -> Self {
        Self {
            feature_slug,
            phases,
            current_phase: 0,
            total_turns: 0,
            total_cost: 0.0,
            activity: String::new(),
        }
    }

    /// Calculate the progress percentage.
    #[must_use]
    pub fn progress_percent(&self) -> u16 {
        if self.phases.is_empty() {
            return 0;
        }

        let completed = self
            .phases
            .iter()
            .filter(|p| p.status == PhaseDisplayStatus::Completed)
            .count();

        ((completed * 100) / self.phases.len()) as u16
    }
}

/// Progress display widget.
pub struct ProgressWidget<'a> {
    state: &'a ProgressState,
}

impl<'a> ProgressWidget<'a> {
    /// Create a new progress widget.
    #[must_use]
    pub fn new(state: &'a ProgressState) -> Self {
        Self { state }
    }

    /// Render the progress widget.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Length(3), // Progress bar
                Constraint::Min(5),    // Phase list
                Constraint::Length(3), // Stats
            ])
            .split(area);

        self.render_title(frame, chunks[0]);
        self.render_progress_bar(frame, chunks[1]);
        self.render_phases(frame, chunks[2]);
        self.render_stats(frame, chunks[3]);
    }

    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let title = Paragraph::new(format!("Executing: {}", self.state.feature_slug))
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::BOTTOM));

        frame.render_widget(title, area);
    }

    fn render_progress_bar(&self, frame: &mut Frame, area: Rect) {
        let progress = self.state.progress_percent();
        let label = format!("{}%", progress);

        let gauge = Gauge::default()
            .block(Block::default().title("Progress").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Cyan))
            .ratio(f64::from(progress) / 100.0)
            .label(label);

        frame.render_widget(gauge, area);
    }

    fn render_phases(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .state
            .phases
            .iter()
            .enumerate()
            .map(|(i, phase)| {
                let is_current =
                    i == self.state.current_phase && phase.status == PhaseDisplayStatus::InProgress;

                let mut spans = vec![
                    Span::styled(
                        format!("{} ", phase.status.icon()),
                        Style::default().fg(phase.status.color()),
                    ),
                    Span::styled(
                        &phase.name,
                        if is_current {
                            Style::default().add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                ];

                if let Some(ref sha) = phase.commit_sha {
                    spans.push(Span::styled(
                        format!(" ({})", sha),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items).block(Block::default().title("Phases").borders(Borders::ALL));

        frame.render_widget(list, area);
    }

    fn render_stats(&self, frame: &mut Frame, area: Rect) {
        let stats = format!(
            "Turns: {} | Cost: ${:.2} | {}",
            self.state.total_turns, self.state.total_cost, self.state.activity
        );

        let paragraph = Paragraph::new(stats)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::TOP));

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_status_icon() {
        assert_eq!(PhaseDisplayStatus::Pending.icon(), "○");
        assert_eq!(PhaseDisplayStatus::InProgress.icon(), "◐");
        assert_eq!(PhaseDisplayStatus::Completed.icon(), "●");
        assert_eq!(PhaseDisplayStatus::Failed.icon(), "✗");
    }

    #[test]
    fn test_progress_percent_empty() {
        let state = ProgressState::new("test".to_string(), vec![]);
        assert_eq!(state.progress_percent(), 0);
    }

    #[test]
    fn test_progress_percent_partial() {
        let phases = vec![
            PhaseInfo {
                name: "setup".to_string(),
                status: PhaseDisplayStatus::Completed,
                commit_sha: Some("abc123".to_string()),
            },
            PhaseInfo {
                name: "impl".to_string(),
                status: PhaseDisplayStatus::InProgress,
                commit_sha: None,
            },
            PhaseInfo {
                name: "test".to_string(),
                status: PhaseDisplayStatus::Pending,
                commit_sha: None,
            },
        ];

        let state = ProgressState::new("test".to_string(), phases);
        assert_eq!(state.progress_percent(), 33); // 1/3 = 33%
    }

    #[test]
    fn test_progress_percent_all_complete() {
        let phases = vec![
            PhaseInfo {
                name: "setup".to_string(),
                status: PhaseDisplayStatus::Completed,
                commit_sha: Some("abc123".to_string()),
            },
            PhaseInfo {
                name: "impl".to_string(),
                status: PhaseDisplayStatus::Completed,
                commit_sha: Some("def456".to_string()),
            },
        ];

        let state = ProgressState::new("test".to_string(), phases);
        assert_eq!(state.progress_percent(), 100);
    }
}
