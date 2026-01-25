//! TUI application state and logic.
//!
//! This module contains the main application struct that manages the TUI state,
//! handles user interaction, and orchestrates the planning session.

use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyEvent};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use serde_json::json;
use tracing::{debug, info, warn};

use gba_core::{Engine, Session, TaskKind};

use crate::error::CliError;
use crate::state::{
    FeatureInfo, FeatureResult, FeatureState, FeatureStatus, GitState, PhaseState, PhaseStatus,
    TaskStats,
};

use super::chat::ChatWidget;
use super::input::{InputAction, InputHandler};

/// Planning phase state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanPhase {
    /// Initial discussion with user about requirements.
    Discussing,
    /// User is confirming the design before spec generation.
    ConfirmingSpec,
    /// Agent is generating specification files.
    GeneratingSpec,
    /// Planning is complete.
    Done,
}

impl std::fmt::Display for PlanPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discussing => write!(f, "Discussing"),
            Self::ConfirmingSpec => write!(f, "Confirming"),
            Self::GeneratingSpec => write!(f, "Generating"),
            Self::Done => write!(f, "Done"),
        }
    }
}

/// Role of a message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    /// Message from the user.
    User,
    /// Message from the assistant.
    Assistant,
    /// System notification message.
    System,
}

/// A message in the chat conversation.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// The role of the message sender.
    pub role: MessageRole,
    /// The content of the message.
    pub content: String,
    /// When the message was created.
    pub timestamp: DateTime<Utc>,
}

impl ChatMessage {
    /// Create a new chat message.
    fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            timestamp: Utc::now(),
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }
}

/// TUI application for interactive planning.
pub struct App {
    /// Chat messages (user and assistant).
    messages: Vec<ChatMessage>,
    /// Current input buffer.
    input: String,
    /// Input cursor position.
    cursor_position: usize,
    /// Scroll position in chat area.
    scroll: u16,
    /// Feature being planned.
    feature_slug: String,
    /// Feature ID (e.g., "0001").
    feature_id: String,
    /// Session for multi-turn conversation.
    session: Session,
    /// Whether the app is running.
    running: bool,
    /// Whether we're waiting for a response.
    waiting: bool,
    /// Current phase of planning.
    phase: PlanPhase,
    /// Working directory path (reserved for future use).
    #[allow(dead_code)]
    workdir: std::path::PathBuf,
    /// Total lines in chat (for scroll calculation).
    total_chat_lines: u16,
    /// Visible chat height (for scroll calculation).
    visible_chat_height: u16,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("feature_slug", &self.feature_slug)
            .field("feature_id", &self.feature_id)
            .field("messages_count", &self.messages.len())
            .field("input_len", &self.input.len())
            .field("phase", &self.phase)
            .field("running", &self.running)
            .field("waiting", &self.waiting)
            .finish()
    }
}

impl App {
    /// Create a new TUI application for planning a feature.
    ///
    /// # Arguments
    ///
    /// * `feature_slug` - The slug identifier for the feature
    /// * `feature_id` - The feature ID (e.g., "0001")
    /// * `engine` - The GBA engine for creating sessions
    /// * `workdir` - Working directory path
    ///
    /// # Errors
    ///
    /// Returns an error if the session cannot be created.
    pub async fn new(
        feature_slug: String,
        feature_id: String,
        engine: &Engine<'_>,
        workdir: &Path,
    ) -> Result<Self, CliError> {
        debug!(
            feature_slug = %feature_slug,
            feature_id = %feature_id,
            "creating TUI app"
        );

        // Create session with plan task configuration
        let context = json!({
            "repo_path": workdir.display().to_string(),
            "feature_id": feature_id,
            "feature_slug": feature_slug,
        });

        let session = engine
            .session_with_task(&TaskKind::Plan, &context, None)
            .map_err(CliError::Engine)?;

        let mut app = Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_position: 0,
            scroll: 0,
            feature_slug,
            feature_id,
            session,
            running: true,
            waiting: false,
            phase: PlanPhase::Discussing,
            workdir: workdir.to_path_buf(),
            total_chat_lines: 0,
            visible_chat_height: 0,
        };

        // Add initial system message
        app.messages.push(ChatMessage::system(format!(
            "Planning feature: {} (ID: {})",
            app.feature_slug, app.feature_id
        )));

        Ok(app)
    }

    /// Run the TUI application.
    ///
    /// This method sets up the terminal, runs the event loop, and restores
    /// the terminal state when done.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal setup fails or an unrecoverable error occurs.
    pub async fn run(&mut self) -> Result<Option<FeatureState>, CliError> {
        // Setup terminal
        enable_raw_mode().map_err(|e| CliError::Io(format!("failed to enable raw mode: {}", e)))?;
        let mut stdout = io::stdout();
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|e| CliError::Io(format!("failed to enter alternate screen: {}", e)))?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)
            .map_err(|e| CliError::Io(format!("failed to create terminal: {}", e)))?;

        // Connect session
        self.session.connect().await.map_err(CliError::Engine)?;

        // Send initial message to start the conversation
        self.send_initial_message().await?;

        // Run event loop
        let result = self.event_loop(&mut terminal).await;

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

        // Disconnect session
        if let Err(e) = self.session.disconnect().await {
            warn!(error = %e, "failed to disconnect session");
        }

        result
    }

    /// Run the main event loop.
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<FeatureState>, CliError> {
        while self.running {
            // Draw UI
            terminal
                .draw(|frame| self.view(frame))
                .map_err(|e| CliError::Io(format!("failed to draw: {}", e)))?;

            // Handle input with timeout
            let has_event = event::poll(std::time::Duration::from_millis(100))
                .map_err(|e| CliError::Io(format!("failed to poll events: {}", e)))?;

            if has_event {
                let event = event::read()
                    .map_err(|e| CliError::Io(format!("failed to read event: {}", e)))?;

                if let Event::Key(key) = event
                    && !self.waiting
                {
                    match self.handle_input(key).await? {
                        InputAction::Continue => {}
                        InputAction::Exit => {
                            self.running = false;
                        }
                        InputAction::Send(message) => {
                            self.send_message(message).await?;
                        }
                        InputAction::ScrollUp => self.scroll_up(),
                        InputAction::ScrollDown => self.scroll_down(),
                    }
                }
            }
        }

        // Return feature state if planning completed
        if self.phase == PlanPhase::Done {
            Ok(Some(self.create_feature_state()))
        } else {
            Ok(None)
        }
    }

    /// Send the initial message to start the planning conversation.
    async fn send_initial_message(&mut self) -> Result<(), CliError> {
        self.waiting = true;

        // The initial user prompt comes from the template
        let initial_prompt = format!(
            "I want to plan a new feature: {}. Please help me understand and design it.",
            self.feature_slug
        );

        self.messages.push(ChatMessage::user(&initial_prompt));

        // Create event handler for streaming
        let mut response = String::new();
        let mut handler = TuiEventHandler::new(&mut response, &mut self.messages);

        match self
            .session
            .send_stream(&initial_prompt, &mut handler)
            .await
        {
            Ok(resp) => {
                if !resp.is_empty() {
                    self.messages.push(ChatMessage::assistant(resp));
                }
            }
            Err(e) => {
                self.messages.push(ChatMessage::system(format!(
                    "Error: Failed to get response: {}",
                    e
                )));
            }
        }

        self.waiting = false;
        self.scroll_to_bottom();
        Ok(())
    }

    /// Handle a keyboard event.
    async fn handle_input(&mut self, key: KeyEvent) -> Result<InputAction, CliError> {
        InputHandler::handle_key(self, key)
    }

    /// Send a message in the conversation.
    pub async fn send_message(&mut self, message: String) -> Result<(), CliError> {
        if message.trim().is_empty() {
            return Ok(());
        }

        self.waiting = true;
        self.messages.push(ChatMessage::user(&message));

        // Create event handler for streaming
        let mut response = String::new();
        let mut handler = TuiEventHandler::new(&mut response, &mut self.messages);

        match self.session.send_stream(&message, &mut handler).await {
            Ok(resp) => {
                if !resp.is_empty() {
                    self.messages.push(ChatMessage::assistant(resp));
                }

                // Check for phase transitions based on response content
                self.check_phase_transition();
            }
            Err(e) => {
                self.messages.push(ChatMessage::system(format!(
                    "Error: Failed to get response: {}",
                    e
                )));
            }
        }

        self.waiting = false;
        self.scroll_to_bottom();
        Ok(())
    }

    /// Check for phase transitions based on conversation content.
    fn check_phase_transition(&mut self) {
        let Some(last_msg) = self.messages.last() else {
            return;
        };

        if last_msg.role != MessageRole::Assistant {
            return;
        }

        let content = last_msg.content.to_lowercase();

        // Check for completion indicators
        if content.contains("plan finished")
            || content.contains("planning complete")
            || content.contains("gba run")
        {
            self.phase = PlanPhase::Done;
            info!(feature_slug = %self.feature_slug, "planning completed");
        } else if content.contains("generating spec") || content.contains("creating specification")
        {
            self.phase = PlanPhase::GeneratingSpec;
        } else if content.contains("approve") || content.contains("confirm") {
            self.phase = PlanPhase::ConfirmingSpec;
        }
    }

    /// Create the feature state for saving.
    fn create_feature_state(&self) -> FeatureState {
        let now = Utc::now();
        let stats = self.session.stats();

        FeatureState {
            feature: FeatureInfo {
                id: self.feature_id.clone(),
                slug: self.feature_slug.clone(),
                created_at: now,
                updated_at: now,
            },
            status: FeatureStatus::Planned,
            current_phase: 0,
            git: GitState {
                worktree_path: format!(".trees/{}_{}", self.feature_id, self.feature_slug),
                branch: format!("feature/{}-{}", self.feature_id, self.feature_slug),
                base_branch: "main".to_string(),
            },
            phases: vec![PhaseState {
                name: "setup".to_string(),
                status: PhaseStatus::Pending,
                started_at: None,
                completed_at: None,
                commit_sha: None,
                stats: None,
            }],
            total_stats: TaskStats {
                turns: stats.turns,
                input_tokens: stats.input_tokens,
                output_tokens: stats.output_tokens,
                cost_usd: stats.cost_usd,
            },
            result: FeatureResult::default(),
            error: None,
        }
    }

    /// Render the TUI view.
    pub fn view(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Main layout: title, chat area, input area, help
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(10),   // Chat
                Constraint::Length(3), // Input
                Constraint::Length(1), // Help
            ])
            .split(area);

        self.render_title(frame, chunks[0]);
        self.render_chat(frame, chunks[1]);
        self.render_input(frame, chunks[2]);
        self.render_help(frame, chunks[3]);
    }

    /// Render the title bar.
    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let phase_str = self.phase.to_string();
        let status = if self.waiting {
            "Working..."
        } else {
            &phase_str
        };
        let title = format!(" GBA Plan: {} [{}] ", self.feature_slug, status);

        let block = Block::default()
            .title(title)
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let stats = self.session.stats();
        let stats_text = format!("Turns: {} | Cost: ${:.4}", stats.turns, stats.cost_usd);

        let paragraph = Paragraph::new(stats_text)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));

        frame.render_widget(paragraph, area);
    }

    /// Render the chat area.
    fn render_chat(&mut self, frame: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2); // Account for borders
        self.visible_chat_height = inner_height;

        let chat_widget = ChatWidget::new(&self.messages, self.scroll, inner_height);
        self.total_chat_lines = chat_widget.total_lines();

        let block = Block::default()
            .title(" Chat ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        frame.render_widget(block, area);

        // Render chat content inside the block
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: inner_height,
        };

        frame.render_widget(chat_widget, inner);
    }

    /// Render the input area.
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let input_style = if self.waiting {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let input_text = if self.waiting {
            "Waiting for response...".to_string()
        } else {
            format!("> {}", self.input)
        };

        let block = Block::default()
            .title(" Input ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let paragraph = Paragraph::new(input_text).style(input_style).block(block);

        frame.render_widget(paragraph, area);

        // Show cursor position
        if !self.waiting {
            frame.set_cursor_position((area.x + 3 + self.cursor_position as u16, area.y + 1));
        }
    }

    /// Render the help bar.
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help_text = vec![
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" Send  "),
            Span::styled("[Ctrl+C]", Style::default().fg(Color::Yellow)),
            Span::raw(" Exit  "),
            Span::styled("[PgUp/PgDn]", Style::default().fg(Color::Yellow)),
            Span::raw(" Scroll"),
        ];

        let paragraph =
            Paragraph::new(Line::from(help_text)).style(Style::default().fg(Color::DarkGray));

        frame.render_widget(paragraph, area);
    }

    /// Scroll up in the chat area.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(3);
    }

    /// Scroll down in the chat area.
    pub fn scroll_down(&mut self) {
        let max_scroll = self
            .total_chat_lines
            .saturating_sub(self.visible_chat_height);
        self.scroll = (self.scroll + 3).min(max_scroll);
    }

    /// Scroll to the bottom of the chat.
    fn scroll_to_bottom(&mut self) {
        let max_scroll = self
            .total_chat_lines
            .saturating_sub(self.visible_chat_height);
        self.scroll = max_scroll;
    }

    /// Get the current input buffer.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Get the cursor position.
    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Set the input buffer.
    pub fn set_input(&mut self, input: String) {
        self.cursor_position = input.len();
        self.input = input;
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    /// Delete the character before the cursor.
    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
        }
    }

    /// Delete the character at the cursor.
    pub fn delete_char_forward(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
        }
    }

    /// Move cursor left.
    pub fn move_cursor_left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    /// Move cursor right.
    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    /// Move cursor to the beginning.
    pub fn move_cursor_home(&mut self) {
        self.cursor_position = 0;
    }

    /// Move cursor to the end.
    pub fn move_cursor_end(&mut self) {
        self.cursor_position = self.input.len();
    }

    /// Clear the input buffer.
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor_position = 0;
    }

    /// Take the input buffer (clears it and returns the value).
    pub fn take_input(&mut self) -> String {
        self.cursor_position = 0;
        std::mem::take(&mut self.input)
    }

    /// Check if the app is waiting for a response.
    pub fn is_waiting(&self) -> bool {
        self.waiting
    }
}

/// Event handler for TUI streaming.
struct TuiEventHandler<'a> {
    response: &'a mut String,
    messages: &'a mut Vec<ChatMessage>,
}

impl<'a> TuiEventHandler<'a> {
    fn new(response: &'a mut String, messages: &'a mut Vec<ChatMessage>) -> Self {
        Self { response, messages }
    }
}

impl gba_core::event::EventHandler for TuiEventHandler<'_> {
    fn on_text(&mut self, text: &str) {
        self.response.push_str(text);
    }

    fn on_tool_use(&mut self, tool: &str, _input: &serde_json::Value) {
        debug!(tool = tool, "tool use in TUI session");
    }

    fn on_tool_result(&mut self, _result: &str) {
        // No-op for TUI
    }

    fn on_error(&mut self, error: &str) {
        self.messages
            .push(ChatMessage::system(format!("Error: {}", error)));
    }

    fn on_complete(&mut self) {
        debug!("TUI streaming complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let user_msg = ChatMessage::user("Hello");
        assert_eq!(user_msg.role, MessageRole::User);
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = ChatMessage::assistant("Hi there");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);
        assert_eq!(assistant_msg.content, "Hi there");

        let system_msg = ChatMessage::system("System notification");
        assert_eq!(system_msg.role, MessageRole::System);
        assert_eq!(system_msg.content, "System notification");
    }

    #[test]
    fn test_plan_phase_display() {
        assert_eq!(format!("{}", PlanPhase::Discussing), "Discussing");
        assert_eq!(format!("{}", PlanPhase::ConfirmingSpec), "Confirming");
        assert_eq!(format!("{}", PlanPhase::GeneratingSpec), "Generating");
        assert_eq!(format!("{}", PlanPhase::Done), "Done");
    }
}
