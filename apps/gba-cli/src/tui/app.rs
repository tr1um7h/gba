//! TUI application state and logic.
//!
//! This module contains the main application struct that manages the TUI state,
//! handles user interaction, and orchestrates the planning session.
//!
//! The architecture separates UI rendering from the worker that communicates
//! with Claude. Communication happens via channels to keep the UI responsive.

use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyEvent};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use gba_core::{Engine, Session, TaskKind};

use crate::error::CliError;

use super::chat::ChatWidget;
use super::input::{InputAction, InputHandler};

/// Spinner frames for loading animation.
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Messages sent from UI to the worker.
#[derive(Debug)]
enum RequestMessage {
    /// Send a message to Claude.
    Send(String),
    /// Shutdown the worker.
    Shutdown,
}

/// Session statistics for display.
#[derive(Debug, Clone, Default)]
struct SessionStats {
    turns: u32,
    #[allow(dead_code)]
    input_tokens: u64,
    #[allow(dead_code)]
    output_tokens: u64,
    cost_usd: f64,
}

/// Messages sent from the worker to the UI.
#[derive(Debug)]
enum WorkerMessage {
    /// Streaming text chunk from Claude.
    Text(String),
    /// Response complete with updated stats.
    Complete(SessionStats),
    /// Error occurred.
    Error(String),
}

/// Planning phase state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanPhase {
    /// Active planning discussion with user.
    InProgress,
    /// Planning is complete (state.yml created or user typed /done).
    Done,
}

impl std::fmt::Display for PlanPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InProgress => write!(f, "Planning"),
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
    /// Current streaming response buffer.
    streaming_response: String,
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
    /// Base branch for the feature.
    base_branch: String,
    /// Channel to send requests to worker.
    request_tx: Option<mpsc::Sender<RequestMessage>>,
    /// Current session stats (updated by worker).
    stats: SessionStats,
    /// Whether the app is running.
    running: bool,
    /// Whether we're waiting for a response.
    waiting: bool,
    /// Current phase of planning.
    phase: PlanPhase,
    /// Working directory path.
    workdir: std::path::PathBuf,
    /// Total lines in chat (for scroll calculation).
    total_chat_lines: u16,
    /// Visible chat height (for scroll calculation).
    visible_chat_height: u16,
    /// Current spinner frame index for loading animation.
    spinner_frame: usize,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("feature_slug", &self.feature_slug)
            .field("feature_id", &self.feature_id)
            .field("base_branch", &self.base_branch)
            .field("messages_count", &self.messages.len())
            .field("input_len", &self.input.len())
            .field("phase", &self.phase)
            .field("running", &self.running)
            .field("waiting", &self.waiting)
            .field("stats", &self.stats)
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
    /// * `base_branch` - The base branch for the feature
    /// * `workdir` - Working directory path
    pub fn new(
        feature_slug: String,
        feature_id: String,
        base_branch: String,
        workdir: &Path,
    ) -> Self {
        debug!(
            feature_slug = %feature_slug,
            feature_id = %feature_id,
            base_branch = %base_branch,
            "creating TUI app"
        );

        Self {
            messages: Vec::new(),
            streaming_response: String::new(),
            input: String::new(),
            cursor_position: 0,
            scroll: 0,
            feature_slug,
            feature_id,
            base_branch,
            request_tx: None,
            stats: SessionStats::default(),
            running: true,
            waiting: false,
            phase: PlanPhase::InProgress,
            workdir: workdir.to_path_buf(),
            total_chat_lines: 0,
            visible_chat_height: 0,
            spinner_frame: 0,
        }
    }

    /// Run the TUI application.
    ///
    /// This method sets up the terminal, spawns the worker task, runs the event loop,
    /// and restores the terminal state when done.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal setup fails or an unrecoverable error occurs.
    pub async fn run(&mut self, engine: &Engine<'_>) -> Result<(), CliError> {
        // Setup terminal
        enable_raw_mode().map_err(|e| CliError::Io(format!("failed to enable raw mode: {}", e)))?;
        let mut stdout = io::stdout();
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|e| CliError::Io(format!("failed to enter alternate screen: {}", e)))?;
        stdout
            .execute(EnableBracketedPaste)
            .map_err(|e| CliError::Io(format!("failed to enable bracketed paste: {}", e)))?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)
            .map_err(|e| CliError::Io(format!("failed to create terminal: {}", e)))?;

        // Draw initial UI immediately so user sees something
        self.messages
            .push(ChatMessage::system("Connecting to Claude...".to_string()));
        terminal
            .draw(|frame| self.view(frame))
            .map_err(|e| CliError::Io(format!("failed to draw: {}", e)))?;

        // Create session with plan task configuration
        let context = json!({
            "repo_path": self.workdir.display().to_string(),
            "feature_id": self.feature_id,
            "feature_slug": self.feature_slug,
            "base_branch": self.base_branch,
        });

        let mut session = engine
            .session_with_task(&TaskKind::Plan, &context, None)
            .map_err(CliError::Engine)?;

        // Connect session
        session.connect().await.map_err(CliError::Engine)?;

        // Create channels for UI <-> Worker communication
        let (request_tx, request_rx) = mpsc::channel::<RequestMessage>(10);
        let (worker_tx, worker_rx) = mpsc::channel::<WorkerMessage>(100);

        // Store request sender for use in event loop
        self.request_tx = Some(request_tx.clone());

        // Spawn worker task
        let worker_handle = tokio::spawn(async move {
            worker_loop(session, request_rx, worker_tx).await;
        });

        // Update status - prompt user for feature description
        self.messages.pop(); // Remove "Connecting..." message
        self.messages.push(ChatMessage::assistant(format!(
            "I'm ready to help you plan the **{}** feature.\n\n\
             Can you tell me more about what you want this feature to do? \
             Include any requirements, constraints, or specific details you have in mind.",
            self.feature_slug
        )));

        // Run event loop (user will type their description first)
        let result = self.event_loop(&mut terminal, worker_rx).await;

        // Signal worker to shutdown
        let _ = request_tx.send(RequestMessage::Shutdown).await;

        // Wait for worker to finish
        if let Err(e) = worker_handle.await {
            warn!(error = %e, "worker task panicked");
        }

        // Cleanup terminal
        disable_raw_mode()
            .map_err(|e| CliError::Io(format!("failed to disable raw mode: {}", e)))?;
        terminal
            .backend_mut()
            .execute(DisableBracketedPaste)
            .map_err(|e| CliError::Io(format!("failed to disable bracketed paste: {}", e)))?;
        terminal
            .backend_mut()
            .execute(LeaveAlternateScreen)
            .map_err(|e| CliError::Io(format!("failed to leave alternate screen: {}", e)))?;
        terminal
            .show_cursor()
            .map_err(|e| CliError::Io(format!("failed to show cursor: {}", e)))?;

        result
    }

    /// Run the main event loop with proper UI/worker separation.
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        mut worker_rx: mpsc::Receiver<WorkerMessage>,
    ) -> Result<(), CliError> {
        while self.running {
            // Draw UI
            terminal
                .draw(|frame| self.view(frame))
                .map_err(|e| CliError::Io(format!("failed to draw: {}", e)))?;

            // Use select to handle both keyboard input and worker messages
            tokio::select! {
                // Check for worker messages (non-blocking)
                msg = worker_rx.recv() => {
                    if let Some(msg) = msg {
                        match msg {
                            WorkerMessage::Text(text) => {
                                self.streaming_response.push_str(&text);
                            }
                            WorkerMessage::Complete(stats) => {
                                // Update stats from worker
                                self.stats = stats;
                                // Move streaming response to messages
                                if !self.streaming_response.is_empty() {
                                    let response = std::mem::take(&mut self.streaming_response);
                                    self.messages.push(ChatMessage::assistant(response));
                                }
                                self.waiting = false;
                                self.scroll_to_bottom();
                            }
                            WorkerMessage::Error(err) => {
                                self.streaming_response.clear();
                                self.messages.push(ChatMessage::system(format!("Error: {}", err)));
                                self.waiting = false;
                                self.scroll_to_bottom();
                            }
                        }
                    }
                }

                // Poll for keyboard input with short timeout
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(16)) => {
                    // Check for keyboard events
                    if event::poll(std::time::Duration::from_millis(0))
                        .map_err(|e| CliError::Io(format!("failed to poll events: {}", e)))?
                    {
                        let evt = event::read()
                            .map_err(|e| CliError::Io(format!("failed to read event: {}", e)))?;

                        match evt {
                            Event::Key(key) => {
                                // Allow scrolling even while waiting
                                if self.waiting {
                                    self.handle_scroll_input(key);
                                } else {
                                    match self.handle_input(key)? {
                                        InputAction::Continue => {}
                                        InputAction::Exit => {
                                            self.running = false;
                                        }
                                        InputAction::Send(message) => {
                                            self.send_message(&message);
                                        }
                                        InputAction::ScrollUp => self.scroll_up(),
                                        InputAction::ScrollDown => self.scroll_down(),
                                    }
                                }
                            }
                            Event::Paste(text) => {
                                // Handle paste event - insert entire text at once
                                if !self.waiting {
                                    self.insert_str(&text);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle scroll-only input while waiting for response.
    fn handle_scroll_input(&mut self, key: KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Allow Ctrl+C to exit even while waiting
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            self.running = false;
            return;
        }

        match key.code {
            KeyCode::PageUp | KeyCode::Up => self.scroll_up(),
            KeyCode::PageDown | KeyCode::Down => self.scroll_down(),
            KeyCode::Esc => self.running = false,
            _ => {}
        }
    }

    /// Handle a keyboard event.
    fn handle_input(&mut self, key: KeyEvent) -> Result<InputAction, CliError> {
        InputHandler::handle_key(self, key)
    }

    /// Send a message to the worker (non-blocking).
    ///
    /// Handles special commands:
    /// - `/done` or `/exit` - Exit planning mode
    fn send_message(&mut self, message: &str) {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return;
        }

        // Handle user commands
        let lower = trimmed.to_lowercase();
        if lower == "/done" || lower == "/exit" {
            self.messages
                .push(ChatMessage::system("Exiting planning mode...".to_string()));
            self.phase = PlanPhase::Done;
            self.running = false;
            return;
        }

        // Update UI state immediately
        self.waiting = true;
        self.messages.push(ChatMessage::user(message));
        self.streaming_response.clear();
        self.scroll_to_bottom();

        // Send to worker via channel (non-blocking)
        if let Some(tx) = &self.request_tx {
            // Use try_send to avoid blocking
            if let Err(e) = tx.try_send(RequestMessage::Send(message.to_string())) {
                warn!(error = %e, "failed to send message to worker");
                self.messages
                    .push(ChatMessage::system("Failed to send message".to_string()));
                self.waiting = false;
            }
        }
    }

    /// Render the TUI view.
    pub fn view(&mut self, frame: &mut Frame) {
        // Advance spinner when waiting
        if self.waiting {
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
        }

        let area = frame.area();

        // Main layout: title, chat area, input area, help
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(10),   // Chat
                Constraint::Length(5), // Input (taller for wrapping)
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
        let title = if self.waiting {
            let spinner = SPINNER_FRAMES[self.spinner_frame];
            format!(" GBA Plan: {} [{} Working...] ", self.feature_slug, spinner)
        } else {
            format!(" GBA Plan: {} [{}] ", self.feature_slug, phase_str)
        };

        let block = Block::default()
            .title(title)
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let stats_text = format!(
            "Turns: {} | Cost: ${:.4}",
            self.stats.turns, self.stats.cost_usd
        );

        let paragraph = Paragraph::new(stats_text)
            .block(block)
            .style(Style::default().fg(Color::DarkGray));

        frame.render_widget(paragraph, area);
    }

    /// Render the chat area.
    fn render_chat(&mut self, frame: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2); // Account for borders
        let inner_width = area.width.saturating_sub(2); // Account for borders
        self.visible_chat_height = inner_height;

        // Create a temporary list including streaming response if any
        let mut display_messages = self.messages.clone();
        if !self.streaming_response.is_empty() {
            display_messages.push(ChatMessage::assistant(&self.streaming_response));
        }

        // Pass actual width for accurate line calculation
        let chat_widget =
            ChatWidget::new(&display_messages, self.scroll, inner_height, inner_width);
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

        let paragraph = Paragraph::new(input_text)
            .style(input_style)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);

        // Show cursor position (accounting for text wrapping)
        if !self.waiting {
            // Inner width (excluding borders)
            let inner_width = area.width.saturating_sub(2) as usize;
            // Account for "> " prefix (2 chars)
            let cursor_with_prefix = self.cursor_position + 2;
            let cursor_row = cursor_with_prefix / inner_width;
            let cursor_col = cursor_with_prefix % inner_width;

            frame.set_cursor_position((
                area.x + 1 + cursor_col as u16,
                area.y + 1 + cursor_row as u16,
            ));
        }
    }

    /// Render the help bar.
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help_text = vec![
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" Send  "),
            Span::styled("/done", Style::default().fg(Color::Yellow)),
            Span::raw(" Finish  "),
            Span::styled("[Ctrl+C]", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel  "),
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

    /// Insert a string at the cursor position (used for paste).
    pub fn insert_str(&mut self, s: &str) {
        self.input.insert_str(self.cursor_position, s);
        self.cursor_position += s.len();
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

/// Worker loop that handles communication with Claude in a separate task.
///
/// This function runs in a spawned tokio task and processes messages from
/// the UI, sending responses back via the worker channel.
async fn worker_loop(
    mut session: Session,
    mut request_rx: mpsc::Receiver<RequestMessage>,
    worker_tx: mpsc::Sender<WorkerMessage>,
) {
    debug!("worker loop started");

    while let Some(request) = request_rx.recv().await {
        match request {
            RequestMessage::Send(message) => {
                debug!(message_len = message.len(), "worker received message");

                // Create event handler for streaming
                let mut handler = ChannelEventHandler::new(worker_tx.clone());

                // Send message and stream response
                // Note: All text is sent via handler.on_text() during streaming,
                // so we don't need to send the return value again.
                match session.send_stream(&message, &mut handler).await {
                    Ok(_) => {
                        // Get updated stats
                        let stats = session.stats();
                        let session_stats = SessionStats {
                            turns: stats.turns,
                            input_tokens: stats.input_tokens,
                            output_tokens: stats.output_tokens,
                            cost_usd: stats.cost_usd,
                        };

                        let _ = worker_tx.send(WorkerMessage::Complete(session_stats)).await;
                    }
                    Err(e) => {
                        let _ = worker_tx.send(WorkerMessage::Error(e.to_string())).await;
                    }
                }
            }
            RequestMessage::Shutdown => {
                debug!("worker received shutdown signal");
                // Disconnect session before exiting
                if let Err(e) = session.disconnect().await {
                    warn!(error = %e, "failed to disconnect session in worker");
                }
                break;
            }
        }
    }

    debug!("worker loop finished");
}

/// Event handler that sends streaming events via a channel.
struct ChannelEventHandler {
    tx: mpsc::Sender<WorkerMessage>,
}

impl ChannelEventHandler {
    fn new(tx: mpsc::Sender<WorkerMessage>) -> Self {
        Self { tx }
    }
}

impl gba_core::event::EventHandler for ChannelEventHandler {
    fn on_text(&mut self, text: &str) {
        // Ensure text ends with newline for readability
        let text = if text.ends_with('\n') {
            text.to_string()
        } else {
            format!("{text}\n")
        };
        // Use try_send to avoid blocking
        let _ = self.tx.try_send(WorkerMessage::Text(text));
    }

    fn on_tool_use(&mut self, tool: &str, _input: &serde_json::Value) {
        debug!(tool = tool, "tool use in TUI session");
    }

    fn on_tool_result(&mut self, _result: &str) {
        // No-op for TUI
    }

    fn on_error(&mut self, error: &str) {
        let _ = self.tx.try_send(WorkerMessage::Error(error.to_string()));
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
        assert_eq!(format!("{}", PlanPhase::InProgress), "Planning");
        assert_eq!(format!("{}", PlanPhase::Done), "Done");
    }
}
