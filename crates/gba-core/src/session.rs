//! Multi-turn interactive session management.
//!
//! This module provides the [`Session`] struct for managing multi-turn
//! conversations with Claude, maintaining conversation history and
//! accumulating statistics across turns.
//!
//! # Overview
//!
//! A session wraps a [`ClaudeClient`] to provide:
//!
//! - Multi-turn conversation support with history tracking
//! - Streaming responses with event handlers
//! - Cumulative statistics across all conversation turns
//! - Session isolation via unique session IDs
//!
//! # Example
//!
//! ```no_run
//! use gba_core::{Engine, EngineConfig, TaskStats};
//! use gba_core::event::PrintEventHandler;
//! use gba_pm::PromptManager;
//!
//! # async fn example() -> gba_core::Result<()> {
//! // Create engine
//! let mut prompts = PromptManager::new();
//! prompts.load_dir("./tasks")?;
//!
//! let config = EngineConfig::builder()
//!     .workdir(".")
//!     .prompts(prompts)
//!     .build();
//! let engine = Engine::new(config)?;
//!
//! // Create a session
//! let mut session = engine.session(None)?;
//!
//! // Send messages (non-streaming)
//! let response = session.send("What is Rust?").await?;
//! println!("Response: {}", response);
//!
//! // Send with streaming
//! let mut handler = PrintEventHandler::new().with_auto_flush();
//! let response = session.send_stream("Tell me more about ownership", &mut handler).await?;
//!
//! // Check accumulated stats
//! let stats = session.stats();
//! println!("Total turns: {}", stats.turns);
//! println!("Total cost: ${:.4}", stats.cost_usd);
//!
//! // Disconnect when done
//! session.disconnect().await?;
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;

use claude_agent_sdk_rs::{
    ClaudeAgentOptions, ClaudeClient, ContentBlock, Message, PermissionMode, ResultMessage,
    SystemPrompt, ToolResultContent,
};
use futures::StreamExt;
use tracing::{debug, info, instrument, trace};
use uuid::Uuid;

use crate::config::TaskConfig;
use crate::error::{EngineError, Result};
use crate::event::EventHandler;
use crate::task::TaskStats;

/// A message in a conversation.
///
/// Represents either a user message or an assistant response
/// in the conversation history.
#[derive(Debug, Clone)]
pub enum ConversationMessage {
    /// A message sent by the user.
    User(String),
    /// A response from the assistant.
    Assistant(String),
}

impl ConversationMessage {
    /// Get the content of the message.
    #[must_use]
    pub fn content(&self) -> &str {
        match self {
            Self::User(content) | Self::Assistant(content) => content,
        }
    }

    /// Check if this is a user message.
    #[must_use]
    pub fn is_user(&self) -> bool {
        matches!(self, Self::User(_))
    }

    /// Check if this is an assistant message.
    #[must_use]
    pub fn is_assistant(&self) -> bool {
        matches!(self, Self::Assistant(_))
    }
}

/// Multi-turn interactive session with Claude.
///
/// A session maintains a persistent connection to Claude, enabling
/// multi-turn conversations while tracking history and statistics.
///
/// Sessions are created via [`Engine::session()`](crate::Engine::session).
pub struct Session {
    /// The Claude client for bidirectional streaming.
    client: ClaudeClient,
    /// Unique session identifier.
    session_id: String,
    /// Conversation history.
    history: Vec<ConversationMessage>,
    /// Accumulated statistics across all turns.
    stats: TaskStats,
    /// Whether the session is connected.
    connected: bool,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("session_id", &self.session_id)
            .field("history_len", &self.history.len())
            .field("stats", &self.stats)
            .field("connected", &self.connected)
            .finish()
    }
}

impl Session {
    /// Create a new session with the given options.
    ///
    /// # Arguments
    ///
    /// * `options` - Claude agent options for the session
    /// * `session_id` - Optional session ID; if None, a UUID is generated
    ///
    /// # Errors
    ///
    /// Returns an error if the client cannot be created.
    pub(crate) fn new(options: ClaudeAgentOptions, session_id: Option<String>) -> Result<Self> {
        let session_id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        debug!(session_id = %session_id, "creating new session");

        let client = ClaudeClient::new(options);

        Ok(Self {
            client,
            session_id,
            history: Vec::new(),
            stats: TaskStats::default(),
            connected: false,
        })
    }

    /// Connect the session.
    ///
    /// This must be called before sending messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails.
    pub async fn connect(&mut self) -> Result<()> {
        if self.connected {
            return Ok(());
        }

        debug!(session_id = %self.session_id, "connecting session");
        self.client.connect().await?;
        self.connected = true;
        info!(session_id = %self.session_id, "session connected");

        Ok(())
    }

    /// Send a message and get the complete response.
    ///
    /// This method sends a message to Claude and waits for the complete
    /// response. For streaming responses, use [`send_stream`](Self::send_stream).
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The session is not connected
    /// - Sending the message fails
    /// - Receiving the response fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use gba_core::session::Session;
    /// # async fn example(session: &mut Session) -> gba_core::Result<()> {
    /// let response = session.send("Hello Claude!").await?;
    /// println!("Claude says: {}", response);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, message), fields(session_id = %self.session_id))]
    pub async fn send(&mut self, message: &str) -> Result<String> {
        self.ensure_connected().await?;

        info!("sending message");
        self.history
            .push(ConversationMessage::User(message.to_string()));

        // Send the query
        self.client
            .query_with_session(message, &self.session_id)
            .await?;

        // Collect all messages first to avoid borrow conflicts
        let mut messages = Vec::new();
        {
            let mut stream = self.client.receive_response();
            while let Some(result) = stream.next().await {
                messages.push(result?);
            }
        }

        // Process collected messages
        let mut response_text = String::new();
        for msg in &messages {
            self.process_message_no_handler(msg, &mut response_text);
        }

        // Store assistant response
        self.history
            .push(ConversationMessage::Assistant(response_text.clone()));
        debug!(
            response_len = response_text.len(),
            "message sent and response received"
        );

        Ok(response_text)
    }

    /// Send a message with streaming events.
    ///
    /// This method sends a message to Claude and streams the response
    /// through an event handler, allowing real-time processing of the
    /// response.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send
    /// * `handler` - Event handler for streaming events
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The session is not connected
    /// - Sending the message fails
    /// - Receiving the response fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use gba_core::session::Session;
    /// # use gba_core::event::PrintEventHandler;
    /// # async fn example(session: &mut Session) -> gba_core::Result<()> {
    /// let mut handler = PrintEventHandler::new().with_auto_flush();
    /// let response = session.send_stream("Explain async/await", &mut handler).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, message, handler), fields(session_id = %self.session_id))]
    pub async fn send_stream(
        &mut self,
        message: &str,
        handler: &mut impl EventHandler,
    ) -> Result<String> {
        self.ensure_connected().await?;

        info!("sending message with streaming");
        self.history
            .push(ConversationMessage::User(message.to_string()));

        // Send the query
        self.client
            .query_with_session(message, &self.session_id)
            .await?;

        // Collect messages first, then process them
        let mut messages = Vec::new();
        {
            let mut stream = self.client.receive_response();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => messages.push(msg),
                    Err(e) => {
                        let error_msg = e.to_string();
                        handler.on_error(&error_msg);
                        return Err(e.into());
                    }
                }
            }
        }

        // Process collected messages with handler
        let mut response_text = String::new();
        for msg in &messages {
            self.process_message_with_handler(msg, &mut response_text, handler);
        }

        handler.on_complete();

        // Store assistant response
        self.history
            .push(ConversationMessage::Assistant(response_text.clone()));
        debug!(
            response_len = response_text.len(),
            "streaming message sent and response received"
        );

        Ok(response_text)
    }

    /// Get the conversation history.
    ///
    /// Returns all messages exchanged in this session, in chronological order.
    #[must_use]
    pub fn history(&self) -> &[ConversationMessage] {
        &self.history
    }

    /// Clear the conversation history.
    ///
    /// This clears the local history but does not affect the Claude session's
    /// memory. To start a completely fresh conversation, create a new session.
    pub fn clear(&mut self) {
        self.history.clear();
        debug!(session_id = %self.session_id, "conversation history cleared");
    }

    /// Get the accumulated statistics for this session.
    ///
    /// Statistics are accumulated across all turns in the session.
    #[must_use]
    pub fn stats(&self) -> &TaskStats {
        &self.stats
    }

    /// Get the session ID.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Check if the session is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Interrupt the current operation.
    ///
    /// This sends an interrupt signal to stop any ongoing Claude operation.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not connected or interruption fails.
    pub async fn interrupt(&self) -> Result<()> {
        if !self.connected {
            return Err(EngineError::config_error("Session not connected"));
        }

        self.client.interrupt().await?;
        debug!(session_id = %self.session_id, "interrupt sent");

        Ok(())
    }

    /// Disconnect the session.
    ///
    /// This cleanly disconnects from Claude. The session cannot be used
    /// after disconnection.
    ///
    /// # Errors
    ///
    /// Returns an error if disconnection fails.
    pub async fn disconnect(&mut self) -> Result<()> {
        if !self.connected {
            return Ok(());
        }

        debug!(session_id = %self.session_id, "disconnecting session");
        self.client.disconnect().await?;
        self.connected = false;
        info!(session_id = %self.session_id, "session disconnected");

        Ok(())
    }

    /// Ensure the session is connected, connecting if necessary.
    async fn ensure_connected(&mut self) -> Result<()> {
        if !self.connected {
            self.connect().await?;
        }
        Ok(())
    }

    /// Process a message from the stream without a handler.
    fn process_message_no_handler(&mut self, msg: &Message, response_text: &mut String) {
        match msg {
            Message::Assistant(assistant_msg) => {
                for block in &assistant_msg.message.content {
                    if let ContentBlock::Text(text) = block {
                        response_text.push_str(&text.text);
                    }
                }
            }
            Message::Result(result_msg) => {
                self.update_stats_from_result(result_msg);
            }
            Message::User(_)
            | Message::System(_)
            | Message::StreamEvent(_)
            | Message::ControlCancelRequest(_) => {
                // Ignore these message types
            }
        }
    }

    /// Process a message from the stream with an event handler.
    fn process_message_with_handler(
        &mut self,
        msg: &Message,
        response_text: &mut String,
        handler: &mut impl EventHandler,
    ) {
        match msg {
            Message::Assistant(assistant_msg) => {
                for block in &assistant_msg.message.content {
                    match block {
                        ContentBlock::Text(text) => {
                            response_text.push_str(&text.text);
                            handler.on_text(&text.text);
                        }
                        ContentBlock::ToolUse(tool_use) => {
                            handler.on_tool_use(&tool_use.name, &tool_use.input);
                        }
                        _ => {}
                    }
                }
            }
            Message::User(user_msg) => {
                // Handle tool results from user messages
                if let Some(ref content) = user_msg.content {
                    for block in content {
                        if let ContentBlock::ToolResult(tool_result) = block {
                            let result_str = match &tool_result.content {
                                Some(ToolResultContent::Text(s)) => s.as_str(),
                                Some(ToolResultContent::Blocks(_)) => "[structured content]",
                                None => "",
                            };
                            handler.on_tool_result(result_str);
                        }
                    }
                }
            }
            Message::Result(result_msg) => {
                self.update_stats_from_result(result_msg);

                if result_msg.is_error {
                    handler.on_error("Claude reported an error");
                }
            }
            Message::System(_) | Message::StreamEvent(_) | Message::ControlCancelRequest(_) => {
                // Ignore these message types
            }
        }
    }

    /// Update session stats from a result message.
    fn update_stats_from_result(&mut self, result_msg: &ResultMessage) {
        self.stats.turns += result_msg.num_turns;
        self.stats.cost_usd += result_msg.total_cost_usd.unwrap_or(0.0);

        if let Some(usage) = &result_msg.usage {
            if let Some(input) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                self.stats.input_tokens += input;
            }
            if let Some(output) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                self.stats.output_tokens += output;
            }
        }

        trace!(
            turns = result_msg.num_turns,
            cost = result_msg.total_cost_usd,
            "result message processed"
        );
    }
}

/// Builder for creating sessions with custom options.
///
/// This is used internally by [`Engine::session()`](crate::Engine::session).
pub struct SessionBuilder {
    workdir: PathBuf,
    base_options: Option<ClaudeAgentOptions>,
    task_config: Option<TaskConfig>,
    system_prompt: Option<SystemPrompt>,
    session_id: Option<String>,
}

impl std::fmt::Debug for SessionBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionBuilder")
            .field("workdir", &self.workdir)
            .field("task_config", &self.task_config)
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl SessionBuilder {
    /// Create a new session builder.
    pub(crate) fn new(workdir: PathBuf) -> Self {
        Self {
            workdir,
            base_options: None,
            task_config: None,
            system_prompt: None,
            session_id: None,
        }
    }

    /// Set base agent options.
    pub(crate) fn with_base_options(mut self, options: ClaudeAgentOptions) -> Self {
        self.base_options = Some(options);
        self
    }

    /// Set task configuration.
    pub(crate) fn with_task_config(mut self, config: TaskConfig) -> Self {
        self.task_config = Some(config);
        self
    }

    /// Set system prompt.
    pub(crate) fn with_system_prompt(mut self, prompt: SystemPrompt) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Set session ID.
    pub(crate) fn with_session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    /// Build the session.
    ///
    /// # Errors
    ///
    /// Returns an error if the session cannot be created.
    pub(crate) fn build(self) -> Result<Session> {
        let mut options = ClaudeAgentOptions::default();

        // Apply base options
        if let Some(base) = self.base_options {
            if base.model.is_some() {
                options.model = base.model;
            }
            if base.permission_mode.is_some() {
                options.permission_mode = base.permission_mode;
            }
            if base.max_turns.is_some() {
                options.max_turns = base.max_turns;
            }
            if base.cwd.is_some() {
                options.cwd = base.cwd;
            }
        }

        // Set working directory
        if options.cwd.is_none() {
            options.cwd = Some(self.workdir);
        }

        // Apply task config
        if let Some(config) = self.task_config {
            if !config.tools.is_empty() {
                options.allowed_tools = config.tools;
            }
            if !config.disallowed_tools.is_empty() {
                options.disallowed_tools = config.disallowed_tools;
            }
        }

        // Set system prompt
        if let Some(prompt) = self.system_prompt {
            options.system_prompt = Some(prompt);
        }

        // Default to bypass permissions - no approval needed for any operation
        if options.permission_mode.is_none() {
            options.permission_mode = Some(PermissionMode::BypassPermissions);
        }

        // Skip version check for faster startup
        options.skip_version_check = true;

        Session::new(options, self.session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_create_conversation_message() {
        let user_msg = ConversationMessage::User("Hello".to_string());
        let assistant_msg = ConversationMessage::Assistant("Hi there".to_string());

        assert!(user_msg.is_user());
        assert!(!user_msg.is_assistant());
        assert_eq!(user_msg.content(), "Hello");

        assert!(assistant_msg.is_assistant());
        assert!(!assistant_msg.is_user());
        assert_eq!(assistant_msg.content(), "Hi there");
    }

    #[test]
    fn test_should_build_session_with_defaults() {
        let builder = SessionBuilder::new(PathBuf::from("/tmp/test"));
        let session = builder.build().unwrap();

        assert!(!session.session_id().is_empty());
        assert!(session.history().is_empty());
        assert_eq!(session.stats().turns, 0);
    }

    #[test]
    fn test_should_build_session_with_custom_id() {
        let builder = SessionBuilder::new(PathBuf::from("/tmp/test"))
            .with_session_id("custom-session".to_string());
        let session = builder.build().unwrap();

        assert_eq!(session.session_id(), "custom-session");
    }

    #[test]
    fn test_should_clear_history() {
        let builder = SessionBuilder::new(PathBuf::from("/tmp/test"));
        let mut session = builder.build().unwrap();

        // Manually add history for testing
        session
            .history
            .push(ConversationMessage::User("test".to_string()));
        session
            .history
            .push(ConversationMessage::Assistant("response".to_string()));

        assert_eq!(session.history().len(), 2);

        session.clear();

        assert!(session.history().is_empty());
    }

    #[test]
    fn test_task_stats_accumulation() {
        let mut stats = TaskStats::default();

        stats.turns += 5;
        stats.input_tokens += 1000;
        stats.output_tokens += 500;
        stats.cost_usd += 0.05;

        stats.turns += 3;
        stats.input_tokens += 800;
        stats.output_tokens += 400;
        stats.cost_usd += 0.03;

        assert_eq!(stats.turns, 8);
        assert_eq!(stats.input_tokens, 1800);
        assert_eq!(stats.output_tokens, 900);
        assert!((stats.cost_usd - 0.08).abs() < f64::EPSILON);
    }
}
