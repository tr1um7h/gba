//! TUI application for `gba run` execution.
//!
//! This module provides a TUI that shows execution progress with streaming
//! output that replaces (not accumulates) between phases. Unlike the chat-based
//! planning TUI, this is optimized for showing execution progress.

use std::io;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};
use tokio::sync::mpsc;
use tracing::debug;

use gba_core::event::EventHandler;

use crate::error::CliError;
use crate::state::{FeatureState, PhaseStatus};

use super::progress::{PhaseDisplayStatus, PhaseInfo};

/// Spinner frames for loading animation.
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Messages sent to the TUI from the execution worker.
#[derive(Debug)]
pub enum RunMessage {
    /// Streaming text chunk.
    Text(String),
    /// Phase started.
    PhaseStarted { index: usize, name: String },
    /// Phase completed.
    PhaseCompleted {
        index: usize,
        commit_sha: Option<String>,
    },
    /// Phase failed.
    PhaseFailed { index: usize, error: String },
    /// Update stats.
    StatsUpdate { turns: u32, cost_usd: f64 },
    /// Activity message update.
    Activity(String),
    /// Execution complete.
    Complete,
    /// Error occurred.
    Error(String),
}

/// TUI application state for execution.
pub struct RunApp {
    /// Feature slug.
    feature_slug: String,
    /// Phases with their status.
    phases: Vec<PhaseInfo>,
    /// Current phase index.
    current_phase: usize,
    /// Current streaming content (replaces each phase).
    streaming_content: String,
    /// Activity message.
    activity: String,
    /// Total turns.
    total_turns: u32,
    /// Total cost.
    total_cost: f64,
    /// Whether the app is running.
    running: bool,
    /// Spinner frame index.
    spinner_frame: usize,
    /// Scroll position in streaming content.
    scroll: u16,
    /// Total lines in content (for scroll calculation).
    total_content_lines: u16,
    /// Visible content height.
    visible_content_height: u16,
    /// Error message if any.
    error: Option<String>,
    /// Whether execution is complete.
    complete: bool,
}

impl std::fmt::Debug for RunApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunApp")
            .field("feature_slug", &self.feature_slug)
            .field("current_phase", &self.current_phase)
            .field("phases_count", &self.phases.len())
            .field("running", &self.running)
            .finish()
    }
}

impl RunApp {
    /// Create a new run app from feature state.
    pub fn new(state: &FeatureState) -> Self {
        let phases: Vec<PhaseInfo> = state
            .phases
            .iter()
            .map(|p| PhaseInfo {
                name: p.name.clone(),
                status: match p.status {
                    PhaseStatus::Pending => PhaseDisplayStatus::Pending,
                    PhaseStatus::InProgress => PhaseDisplayStatus::InProgress,
                    PhaseStatus::Completed => PhaseDisplayStatus::Completed,
                    PhaseStatus::Failed => PhaseDisplayStatus::Failed,
                },
                commit_sha: p.commit_sha.clone(),
            })
            .collect();

        Self {
            feature_slug: state.feature.slug.clone(),
            phases,
            current_phase: state.current_phase,
            streaming_content: String::new(),
            activity: String::new(),
            total_turns: state.total_stats.turns,
            total_cost: state.total_stats.cost_usd,
            running: true,
            spinner_frame: 0,
            scroll: 0,
            total_content_lines: 0,
            visible_content_height: 0,
            error: None,
            complete: false,
        }
    }

    /// Run the TUI event loop.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal setup fails.
    pub async fn run(&mut self, mut rx: mpsc::Receiver<RunMessage>) -> Result<(), CliError> {
        // Setup terminal
        enable_raw_mode().map_err(|e| CliError::Io(format!("failed to enable raw mode: {}", e)))?;
        let mut stdout = io::stdout();
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|e| CliError::Io(format!("failed to enter alternate screen: {}", e)))?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)
            .map_err(|e| CliError::Io(format!("failed to create terminal: {}", e)))?;

        // Initial draw
        terminal
            .draw(|frame| self.view(frame))
            .map_err(|e| CliError::Io(format!("failed to draw: {}", e)))?;

        // Event loop
        while self.running {
            // Draw UI
            terminal
                .draw(|frame| self.view(frame))
                .map_err(|e| CliError::Io(format!("failed to draw: {}", e)))?;

            tokio::select! {
                // Check for messages from worker
                msg = rx.recv() => {
                    if let Some(msg) = msg {
                        self.handle_message(msg);
                    } else {
                        // Channel closed, exit
                        self.running = false;
                    }
                }

                // Poll for keyboard input
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(16)) => {
                    if event::poll(std::time::Duration::from_millis(0))
                        .map_err(|e| CliError::Io(format!("failed to poll events: {}", e)))?
                    {
                        let evt = event::read()
                            .map_err(|e| CliError::Io(format!("failed to read event: {}", e)))?;

                        if let Event::Key(key) = evt {
                            self.handle_key(key);
                        }
                    }
                }
            }
        }

        // Cleanup terminal
        disable_raw_mode()
            .map_err(|e| CliError::Io(format!("failed to disable raw mode: {}", e)))?;
        terminal
            .backend_mut()
            .execute(LeaveAlternateScreen)
            .map_err(|e| CliError::Io(format!("failed to leave alternate screen: {}", e)))?;
        terminal
            .show_cursor()
            .map_err(|e| CliError::Io(format!("failed to show cursor: {}", e)))?;

        Ok(())
    }

    /// Handle a message from the worker.
    fn handle_message(&mut self, msg: RunMessage) {
        match msg {
            RunMessage::Text(text) => {
                self.streaming_content.push_str(&text);
                self.auto_scroll();
            }
            RunMessage::PhaseStarted { index, name } => {
                self.current_phase = index;
                // Clear streaming content for new phase
                self.streaming_content.clear();
                self.scroll = 0;
                self.activity = format!("Executing: {}", name);
                if index < self.phases.len() {
                    self.phases[index].status = PhaseDisplayStatus::InProgress;
                }
            }
            RunMessage::PhaseCompleted { index, commit_sha } => {
                if index < self.phases.len() {
                    self.phases[index].status = PhaseDisplayStatus::Completed;
                    self.phases[index].commit_sha = commit_sha;
                }
                self.activity = format!("Phase {} completed", index + 1);
            }
            RunMessage::PhaseFailed { index, error } => {
                if index < self.phases.len() {
                    self.phases[index].status = PhaseDisplayStatus::Failed;
                }
                self.error = Some(error);
                self.activity = format!("Phase {} failed", index + 1);
            }
            RunMessage::StatsUpdate { turns, cost_usd } => {
                self.total_turns = turns;
                self.total_cost = cost_usd;
            }
            RunMessage::Activity(msg) => {
                self.activity = msg;
            }
            RunMessage::Complete => {
                self.complete = true;
                self.activity = "Execution complete!".to_string();
                // Keep running so user can see final state
                // They can press q or Ctrl+C to exit
            }
            RunMessage::Error(error) => {
                self.error = Some(error);
            }
        }
    }

    /// Handle a keyboard event.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // Ctrl+C always exits
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            self.running = false;
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.complete || self.error.is_some() {
                    self.running = false;
                }
            }
            KeyCode::Up | KeyCode::PageUp => self.scroll_up(),
            KeyCode::Down | KeyCode::PageDown => self.scroll_down(),
            _ => {}
        }
    }

    /// Render the view.
    fn view(&mut self, frame: &mut Frame) {
        // Advance spinner
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();

        let area = frame.area();

        // Layout: title, progress sidebar + content area, stats footer
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(10),   // Main area
                Constraint::Length(3), // Stats/help
            ])
            .split(area);

        self.render_title(frame, main_chunks[0]);

        // Split main area: phases sidebar + streaming content
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(35), // Phases sidebar
                Constraint::Min(40),    // Streaming content
            ])
            .split(main_chunks[1]);

        self.render_phases(frame, content_chunks[0]);
        self.render_content(frame, content_chunks[1]);
        self.render_footer(frame, main_chunks[2]);
    }

    /// Render the title bar with progress.
    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let progress = self.progress_percent();
        let spinner = SPINNER_FRAMES[self.spinner_frame];

        let status = if self.complete {
            "✓ Complete".to_string()
        } else if self.error.is_some() {
            "✗ Failed".to_string()
        } else {
            format!("{} Running", spinner)
        };

        let title = format!(" GBA Run: {} [{}] ", self.feature_slug, status);

        let block = Block::default()
            .title(title)
            .title_style(
                Style::default()
                    .fg(if self.error.is_some() {
                        Color::Red
                    } else if self.complete {
                        Color::Green
                    } else {
                        Color::Cyan
                    })
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        // Progress bar in title area
        let gauge = Gauge::default()
            .block(block)
            .gauge_style(Style::default().fg(Color::Cyan))
            .ratio(f64::from(progress) / 100.0)
            .label(format!("{}%", progress));

        frame.render_widget(gauge, area);
    }

    /// Render the phases sidebar.
    fn render_phases(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .phases
            .iter()
            .enumerate()
            .map(|(i, phase)| {
                let is_current =
                    i == self.current_phase && phase.status == PhaseDisplayStatus::InProgress;

                let mut spans = vec![
                    Span::styled(
                        format!("{} ", phase.status.icon()),
                        Style::default().fg(phase.status.color()),
                    ),
                    Span::styled(format!("{}. ", i + 1), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        &phase.name,
                        if is_current {
                            Style::default()
                                .add_modifier(Modifier::BOLD)
                                .fg(Color::Yellow)
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

        let list = List::new(items).block(
            Block::default()
                .title(" Phases ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        );

        frame.render_widget(list, area);
    }

    /// Render the streaming content area.
    fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2);
        let inner_width = area.width.saturating_sub(2);
        self.visible_content_height = inner_height;

        // Display content
        let content = if let Some(ref error) = self.error {
            format!("Error: {}\n\n{}", error, self.streaming_content)
        } else if self.streaming_content.is_empty() {
            if self.complete {
                "All phases completed successfully!".to_string()
            } else {
                "Waiting for output...".to_string()
            }
        } else {
            self.streaming_content.clone()
        };

        // Calculate total lines for scrolling
        let wrapped_lines: Vec<&str> = content.lines().collect();
        let mut total_lines = 0u16;
        for line in &wrapped_lines {
            let line_width = line.len() as u16;
            let lines_needed = if inner_width > 0 {
                (line_width / inner_width).max(1)
            } else {
                1
            };
            total_lines = total_lines.saturating_add(lines_needed);
        }
        self.total_content_lines = total_lines;

        let title = format!(" {} ", self.activity);
        let border_color = if self.error.is_some() {
            Color::Red
        } else if self.complete {
            Color::Green
        } else {
            Color::Yellow
        };

        let paragraph = Paragraph::new(content)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, area);
    }

    /// Render the footer with stats and help.
    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let stats = format!(
            "Turns: {} | Cost: ${:.4}",
            self.total_turns, self.total_cost
        );

        let help = if self.complete || self.error.is_some() {
            " | [q] Exit | [↑/↓] Scroll"
        } else {
            " | [Ctrl+C] Cancel | [↑/↓] Scroll"
        };

        let content = vec![
            Span::styled(stats, Style::default().fg(Color::Cyan)),
            Span::styled(help, Style::default().fg(Color::DarkGray)),
        ];

        let paragraph = Paragraph::new(Line::from(content)).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }

    /// Calculate progress percentage.
    fn progress_percent(&self) -> u16 {
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

    /// Scroll up.
    fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(3);
    }

    /// Scroll down.
    fn scroll_down(&mut self) {
        let max_scroll = self
            .total_content_lines
            .saturating_sub(self.visible_content_height);
        self.scroll = (self.scroll + 3).min(max_scroll);
    }

    /// Auto-scroll to bottom when new content arrives.
    fn auto_scroll(&mut self) {
        let max_scroll = self
            .total_content_lines
            .saturating_sub(self.visible_content_height);
        self.scroll = max_scroll;
    }
}

/// Event handler that sends messages to the TUI channel.
pub struct TuiEventHandler {
    tx: mpsc::Sender<RunMessage>,
}

impl TuiEventHandler {
    /// Create a new TUI event handler.
    pub fn new(tx: mpsc::Sender<RunMessage>) -> Self {
        Self { tx }
    }
}

impl EventHandler for TuiEventHandler {
    fn on_text(&mut self, text: &str) {
        let _ = self.tx.try_send(RunMessage::Text(text.to_string()));
    }

    fn on_tool_use(&mut self, tool: &str, _input: &serde_json::Value) {
        debug!(tool = tool, "tool use during execution");
    }

    fn on_tool_result(&mut self, _result: &str) {
        // No-op
    }

    fn on_error(&mut self, error: &str) {
        let _ = self.tx.try_send(RunMessage::Error(error.to_string()));
    }

    fn on_complete(&mut self) {
        debug!("execution streaming complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        FeatureInfo, FeatureResult, FeatureStatus, GitState, PhaseState, TaskStats,
    };
    use chrono::Utc;

    fn create_test_state() -> FeatureState {
        FeatureState {
            feature: FeatureInfo {
                id: "0001".to_string(),
                slug: "test-feature".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            status: FeatureStatus::InProgress,
            current_phase: 0,
            git: GitState {
                worktree_path: ".trees/test-feature".to_string(),
                branch: "feature/0001-test-feature".to_string(),
                base_branch: "main".to_string(),
            },
            phases: vec![
                PhaseState {
                    name: "setup".to_string(),
                    status: PhaseStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    commit_sha: None,
                    stats: None,
                },
                PhaseState {
                    name: "implementation".to_string(),
                    status: PhaseStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    commit_sha: None,
                    stats: None,
                },
            ],
            total_stats: TaskStats::default(),
            result: FeatureResult::default(),
            error: None,
        }
    }

    #[test]
    fn test_run_app_creation() {
        let state = create_test_state();
        let app = RunApp::new(&state);

        assert_eq!(app.feature_slug, "test-feature");
        assert_eq!(app.phases.len(), 2);
        assert_eq!(app.current_phase, 0);
        assert!(app.running);
    }

    #[test]
    fn test_progress_percent_empty() {
        let mut state = create_test_state();
        state.phases.clear();
        let app = RunApp::new(&state);

        assert_eq!(app.progress_percent(), 0);
    }

    #[test]
    fn test_progress_percent_partial() {
        let mut state = create_test_state();
        state.phases[0].status = PhaseStatus::Completed;
        let app = RunApp::new(&state);

        assert_eq!(app.progress_percent(), 50); // 1/2 = 50%
    }

    #[test]
    fn test_handle_phase_started() {
        let state = create_test_state();
        let mut app = RunApp::new(&state);

        app.handle_message(RunMessage::PhaseStarted {
            index: 0,
            name: "setup".to_string(),
        });

        assert_eq!(app.current_phase, 0);
        assert!(app.streaming_content.is_empty());
        assert_eq!(app.phases[0].status, PhaseDisplayStatus::InProgress);
    }

    #[test]
    fn test_handle_text_clears_on_new_phase() {
        let state = create_test_state();
        let mut app = RunApp::new(&state);

        // Add some text
        app.handle_message(RunMessage::Text("some output".to_string()));
        assert!(!app.streaming_content.is_empty());

        // Start new phase - should clear
        app.handle_message(RunMessage::PhaseStarted {
            index: 1,
            name: "implementation".to_string(),
        });
        assert!(app.streaming_content.is_empty());
    }
}
