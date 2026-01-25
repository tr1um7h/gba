//! Shared message processing logic for Engine and Session.
//!
//! This module provides the [`MessageProcessor`] struct which consolidates
//! the common logic for processing Claude agent messages. It handles:
//!
//! - Extracting text content from assistant messages
//! - Processing tool use and tool result events
//! - Updating statistics from result messages
//! - Dispatching events to optional handlers
//!
//! # Example
//!
//! ```ignore
//! use gba_core::message::MessageProcessor;
//! use gba_core::TaskStats;
//!
//! let mut processor = MessageProcessor::new();
//!
//! // Process without handler
//! for msg in messages {
//!     processor.process(&msg);
//! }
//!
//! // Or with handler
//! for msg in messages {
//!     processor.process_with_handler(&msg, &mut handler);
//! }
//!
//! let output = processor.output();
//! let stats = processor.stats();
//! ```

use claude_agent_sdk_rs::{ContentBlock, Message, ResultMessage, ToolResultContent};
use tracing::trace;

use crate::event::EventHandler;
use crate::task::TaskStats;

/// Processor for Claude agent messages.
///
/// Consolidates shared message processing logic between [`Engine`](crate::Engine)
/// and [`Session`](crate::Session), handling text extraction, tool events,
/// and statistics tracking.
#[derive(Debug, Default)]
pub struct MessageProcessor {
    /// Accumulated output text from assistant messages.
    output: String,
    /// Execution statistics.
    stats: TaskStats,
    /// Whether the task completed successfully.
    success: bool,
    /// Whether to accumulate stats (true for sessions, false for single-shot).
    accumulate_stats: bool,
}

impl MessageProcessor {
    /// Create a new message processor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            success: true,
            ..Default::default()
        }
    }

    /// Configure whether to accumulate statistics across multiple result messages.
    ///
    /// - `true`: Add new token counts to existing values (for multi-turn sessions)
    /// - `false`: Replace token counts with latest values (for single-shot queries)
    #[must_use]
    pub fn with_accumulate_stats(mut self, accumulate: bool) -> Self {
        self.accumulate_stats = accumulate;
        self
    }

    /// Process a single message from the Claude agent without a handler.
    ///
    /// This method handles all message types:
    /// - `Assistant`: Extracts text content
    /// - `Result`: Updates statistics
    /// - Other types are ignored
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to process
    pub fn process(&mut self, msg: &Message) {
        match msg {
            Message::Assistant(assistant_msg) => {
                for block in &assistant_msg.message.content {
                    if let ContentBlock::Text(text) = block {
                        self.output.push_str(&text.text);
                    }
                }
            }
            Message::Result(result_msg) => {
                self.update_from_result(result_msg);
            }
            Message::User(_)
            | Message::System(_)
            | Message::StreamEvent(_)
            | Message::ControlCancelRequest(_) => {
                // Ignore these message types for non-handler processing
            }
        }
    }

    /// Process a single message from the Claude agent with a handler.
    ///
    /// This method handles all message types:
    /// - `Assistant`: Extracts text and calls `on_text`/`on_tool_use` on handler
    /// - `User`: Processes tool results and calls `on_tool_result` on handler
    /// - `Result`: Updates statistics and calls `on_error` if applicable
    /// - Other types are ignored
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to process
    /// * `handler` - Event handler for streaming events
    pub fn process_with_handler(&mut self, msg: &Message, handler: &mut dyn EventHandler) {
        match msg {
            Message::Assistant(assistant_msg) => {
                for block in &assistant_msg.message.content {
                    match block {
                        ContentBlock::Text(text) => {
                            self.output.push_str(&text.text);
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
                self.update_from_result(result_msg);
                if result_msg.is_error {
                    handler.on_error("Claude reported an error");
                }
            }
            Message::System(_) | Message::StreamEvent(_) | Message::ControlCancelRequest(_) => {
                // Ignore these message types
            }
        }
    }

    /// Get the accumulated output text.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Take ownership of the accumulated output text.
    #[must_use]
    pub fn take_output(self) -> String {
        self.output
    }

    /// Get the execution statistics.
    #[must_use]
    pub fn stats(&self) -> &TaskStats {
        &self.stats
    }

    /// Get a mutable reference to the statistics.
    #[must_use]
    pub fn stats_mut(&mut self) -> &mut TaskStats {
        &mut self.stats
    }

    /// Check if the task completed successfully.
    #[must_use]
    pub fn success(&self) -> bool {
        self.success
    }

    /// Update stats from a result message.
    fn update_from_result(&mut self, result_msg: &ResultMessage) {
        // Update turns and cost
        if self.accumulate_stats {
            self.stats.turns += result_msg.num_turns;
            self.stats.cost_usd += result_msg.total_cost_usd.unwrap_or(0.0);
        } else {
            self.stats.turns = result_msg.num_turns;
            self.stats.cost_usd = result_msg.total_cost_usd.unwrap_or(0.0);
        }

        // Update token usage
        if let Some(ref usage) = result_msg.usage {
            self.stats.update_from_usage(usage, self.accumulate_stats);
        }

        // Track success/error status
        if result_msg.is_error {
            self.success = false;
        }

        trace!(
            turns = result_msg.num_turns,
            cost = result_msg.total_cost_usd,
            "result message processed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claude_agent_sdk_rs::{
        AssistantMessage, AssistantMessageInner, ContentBlock, Message, ResultMessage, TextBlock,
        ToolResultBlock, ToolUseBlock, UserMessage,
    };
    use serde_json::json;

    /// A test event handler that collects all events.
    #[derive(Debug, Default)]
    struct CollectingHandler {
        texts: Vec<String>,
        tool_uses: Vec<(String, serde_json::Value)>,
        tool_results: Vec<String>,
        errors: Vec<String>,
        complete_called: bool,
    }

    impl EventHandler for CollectingHandler {
        fn on_text(&mut self, text: &str) {
            self.texts.push(text.to_string());
        }

        fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value) {
            self.tool_uses.push((tool.to_string(), input.clone()));
        }

        fn on_tool_result(&mut self, result: &str) {
            self.tool_results.push(result.to_string());
        }

        fn on_error(&mut self, error: &str) {
            self.errors.push(error.to_string());
        }

        fn on_complete(&mut self) {
            self.complete_called = true;
        }
    }

    fn create_assistant_message_msg(content: Vec<ContentBlock>) -> Message {
        Message::Assistant(AssistantMessage {
            message: AssistantMessageInner {
                content,
                model: None,
                id: None,
                stop_reason: None,
                usage: None,
                error: None,
            },
            parent_tool_use_id: None,
            session_id: None,
            uuid: None,
        })
    }

    fn create_result_message_msg(num_turns: u32, cost: f64, is_error: bool) -> Message {
        Message::Result(ResultMessage {
            subtype: "query_complete".to_string(),
            duration_ms: 1000,
            duration_api_ms: 800,
            is_error,
            num_turns,
            session_id: "test-session".to_string(),
            total_cost_usd: Some(cost),
            usage: Some(json!({
                "input_tokens": 100,
                "output_tokens": 50
            })),
            result: None,
            structured_output: None,
        })
    }

    #[test]
    fn test_should_process_text_content_without_handler() {
        let mut processor = MessageProcessor::new();
        let msg = create_assistant_message_msg(vec![ContentBlock::Text(TextBlock {
            text: "Hello, world!".to_string(),
        })]);

        processor.process(&msg);

        assert_eq!(processor.output(), "Hello, world!");
    }

    #[test]
    fn test_should_process_text_content_with_handler() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = create_assistant_message_msg(vec![ContentBlock::Text(TextBlock {
            text: "Hello, world!".to_string(),
        })]);

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(processor.output(), "Hello, world!");
        assert_eq!(handler.texts, vec!["Hello, world!"]);
    }

    #[test]
    fn test_should_process_multiple_text_blocks() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = create_assistant_message_msg(vec![
            ContentBlock::Text(TextBlock {
                text: "First ".to_string(),
            }),
            ContentBlock::Text(TextBlock {
                text: "Second".to_string(),
            }),
        ]);

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(processor.output(), "First Second");
        assert_eq!(handler.texts, vec!["First ", "Second"]);
    }

    #[test]
    fn test_should_process_tool_use() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = create_assistant_message_msg(vec![ContentBlock::ToolUse(ToolUseBlock {
            id: "tool_123".to_string(),
            name: "read_file".to_string(),
            input: json!({"path": "/tmp/test.txt"}),
        })]);

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(handler.tool_uses.len(), 1);
        assert_eq!(handler.tool_uses[0].0, "read_file");
        assert_eq!(handler.tool_uses[0].1["path"], "/tmp/test.txt");
    }

    #[test]
    fn test_should_process_tool_result() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = Message::User(UserMessage {
            text: None,
            content: Some(vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "tool_123".to_string(),
                is_error: Some(false),
                content: Some(ToolResultContent::Text("File contents here".to_string())),
            })]),
            uuid: None,
            parent_tool_use_id: None,
            extra: json!({}),
        });

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(handler.tool_results, vec!["File contents here"]);
    }

    #[test]
    fn test_should_process_result_message_single_shot() {
        let mut processor = MessageProcessor::new();

        let msg = create_result_message_msg(5, 0.05, false);

        processor.process(&msg);

        assert_eq!(processor.stats().turns, 5);
        assert!((processor.stats().cost_usd - 0.05).abs() < f64::EPSILON);
        assert_eq!(processor.stats().input_tokens, 100);
        assert_eq!(processor.stats().output_tokens, 50);
        assert!(processor.success());
    }

    #[test]
    fn test_should_accumulate_stats_for_sessions() {
        let mut processor = MessageProcessor::new().with_accumulate_stats(true);

        let msg1 = create_result_message_msg(3, 0.02, false);
        let msg2 = create_result_message_msg(2, 0.03, false);

        processor.process(&msg1);
        processor.process(&msg2);

        assert_eq!(processor.stats().turns, 5);
        assert!((processor.stats().cost_usd - 0.05).abs() < f64::EPSILON);
        assert_eq!(processor.stats().input_tokens, 200);
        assert_eq!(processor.stats().output_tokens, 100);
    }

    #[test]
    fn test_should_handle_error_result() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = create_result_message_msg(1, 0.01, true);

        processor.process_with_handler(&msg, &mut handler);

        assert!(!processor.success());
        assert_eq!(handler.errors.len(), 1);
        assert!(handler.errors[0].contains("error"));
    }

    #[test]
    fn test_should_ignore_system_messages() {
        let mut processor = MessageProcessor::new();

        let msg = Message::System(claude_agent_sdk_rs::SystemMessage {
            subtype: "session_start".to_string(),
            cwd: None,
            session_id: None,
            tools: None,
            mcp_servers: None,
            model: None,
            permission_mode: None,
            uuid: None,
            data: json!({}),
        });

        processor.process(&msg);

        assert!(processor.output().is_empty());
        assert_eq!(processor.stats().turns, 0);
    }

    #[test]
    fn test_should_handle_structured_tool_result() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = Message::User(UserMessage {
            text: None,
            content: Some(vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "tool_123".to_string(),
                is_error: Some(false),
                content: Some(ToolResultContent::Blocks(vec![])),
            })]),
            uuid: None,
            parent_tool_use_id: None,
            extra: json!({}),
        });

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(handler.tool_results, vec!["[structured content]"]);
    }

    #[test]
    fn test_should_handle_empty_tool_result() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = Message::User(UserMessage {
            text: None,
            content: Some(vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "tool_123".to_string(),
                is_error: Some(false),
                content: None,
            })]),
            uuid: None,
            parent_tool_use_id: None,
            extra: json!({}),
        });

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(handler.tool_results, vec![""]);
    }

    #[test]
    fn test_should_take_output() {
        let mut processor = MessageProcessor::new();
        let msg = create_assistant_message_msg(vec![ContentBlock::Text(TextBlock {
            text: "Hello".to_string(),
        })]);

        processor.process(&msg);

        let output = processor.take_output();
        assert_eq!(output, "Hello");
    }

    #[test]
    fn test_should_process_mixed_content() {
        let mut processor = MessageProcessor::new();
        let mut handler = CollectingHandler::default();

        let msg = create_assistant_message_msg(vec![
            ContentBlock::Text(TextBlock {
                text: "Calling tool...".to_string(),
            }),
            ContentBlock::ToolUse(ToolUseBlock {
                id: "tool_1".to_string(),
                name: "bash".to_string(),
                input: json!({"command": "ls -la"}),
            }),
        ]);

        processor.process_with_handler(&msg, &mut handler);

        assert_eq!(processor.output(), "Calling tool...");
        assert_eq!(handler.texts, vec!["Calling tool..."]);
        assert_eq!(handler.tool_uses.len(), 1);
        assert_eq!(handler.tool_uses[0].0, "bash");
    }
}
