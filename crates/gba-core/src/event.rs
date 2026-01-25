//! Event handling for streaming responses.
//!
//! This module provides the [`EventHandler`] trait for handling streaming events
//! from Claude, and a default [`PrintEventHandler`] implementation that outputs
//! events to stdout.
//!
//! # Example
//!
//! ```
//! use gba_core::event::{EventHandler, PrintEventHandler};
//!
//! // Use the default print handler
//! let mut handler = PrintEventHandler::default();
//!
//! // Or implement your own
//! struct MyHandler;
//! impl EventHandler for MyHandler {
//!     fn on_text(&mut self, text: &str) {
//!         // Custom text handling
//!     }
//!     fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value) {
//!         // Custom tool use handling
//!     }
//!     fn on_tool_result(&mut self, result: &str) {
//!         // Custom tool result handling
//!     }
//!     fn on_error(&mut self, error: &str) {
//!         // Custom error handling
//!     }
//!     fn on_complete(&mut self) {
//!         // Custom completion handling
//!     }
//! }
//! ```

use std::io::{self, Write};

use tracing::{debug, error, trace};

/// Event handler trait for streaming responses.
///
/// Implement this trait to handle streaming events from Claude during
/// task execution or interactive sessions.
///
/// All methods have default no-op implementations, allowing you to
/// override only the events you care about.
pub trait EventHandler: Send {
    /// Called when text content is received from Claude.
    ///
    /// # Arguments
    ///
    /// * `text` - The text content received
    fn on_text(&mut self, text: &str) {
        let _ = text;
    }

    /// Called when Claude starts using a tool.
    ///
    /// # Arguments
    ///
    /// * `tool` - The name of the tool being used
    /// * `input` - The input parameters for the tool
    fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value) {
        let _ = (tool, input);
    }

    /// Called when a tool returns a result.
    ///
    /// # Arguments
    ///
    /// * `result` - The result from the tool execution
    fn on_tool_result(&mut self, result: &str) {
        let _ = result;
    }

    /// Called when an error occurs during streaming.
    ///
    /// # Arguments
    ///
    /// * `error` - Description of the error
    fn on_error(&mut self, error: &str) {
        let _ = error;
    }

    /// Called when the response is complete.
    fn on_complete(&mut self) {}
}

/// Simple event handler that prints to stdout.
///
/// This handler is useful for CLI applications and debugging.
/// It prints text directly to stdout and formats tool usage
/// and results in a readable manner.
///
/// # Example
///
/// ```
/// use gba_core::event::PrintEventHandler;
///
/// let mut handler = PrintEventHandler::default();
/// // Use with Engine::run_stream() or Session::send_stream()
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct PrintEventHandler {
    /// Whether to show tool usage details.
    show_tools: bool,
    /// Whether to flush stdout after each text output.
    auto_flush: bool,
}

impl PrintEventHandler {
    /// Create a new print event handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable showing tool usage details.
    #[must_use]
    pub fn with_tools(mut self) -> Self {
        self.show_tools = true;
        self
    }

    /// Enable auto-flushing stdout after each text output.
    ///
    /// This is useful for real-time streaming output where you want
    /// each piece of text to appear immediately.
    #[must_use]
    pub fn with_auto_flush(mut self) -> Self {
        self.auto_flush = true;
        self
    }
}

impl EventHandler for PrintEventHandler {
    fn on_text(&mut self, text: &str) {
        print!("{text}");
        if self.auto_flush {
            let _ = io::stdout().flush();
        }
        trace!(text_len = text.len(), "received text");
    }

    fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value) {
        if self.show_tools {
            println!("\n[Tool: {tool}]");
            if let Ok(formatted) = serde_json::to_string_pretty(input) {
                println!("Input: {formatted}");
            }
        }
        debug!(tool = tool, "tool use started");
    }

    fn on_tool_result(&mut self, result: &str) {
        if self.show_tools {
            let preview = if result.len() > 200 {
                format!("{}...", &result[..200])
            } else {
                result.to_string()
            };
            println!("[Result: {preview}]");
        }
        trace!(result_len = result.len(), "tool result received");
    }

    fn on_error(&mut self, error_msg: &str) {
        eprintln!("\nError: {error_msg}");
        error!(error = error_msg, "streaming error");
    }

    fn on_complete(&mut self) {
        println!();
        debug!("response complete");
    }
}

/// Event handler that collects text into a buffer.
///
/// This handler is useful when you need to capture the full response
/// while also allowing streaming events to be processed.
///
/// # Example
///
/// ```
/// use gba_core::event::CollectingEventHandler;
///
/// let mut handler = CollectingEventHandler::new();
/// // Use with streaming...
/// let collected_text = handler.text();
/// ```
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct CollectingEventHandler {
    text: String,
    tools_used: Vec<String>,
    has_error: bool,
}

impl CollectingEventHandler {
    /// Create a new collecting event handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the collected text content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Take ownership of the collected text.
    #[must_use]
    pub fn into_text(self) -> String {
        self.text
    }

    /// Get the list of tools that were used.
    #[must_use]
    pub fn tools_used(&self) -> &[String] {
        &self.tools_used
    }

    /// Check if any errors occurred during streaming.
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.has_error
    }

    /// Clear the collected content and reset state.
    pub fn clear(&mut self) {
        self.text.clear();
        self.tools_used.clear();
        self.has_error = false;
    }
}

impl EventHandler for CollectingEventHandler {
    fn on_text(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn on_tool_use(&mut self, tool: &str, _input: &serde_json::Value) {
        self.tools_used.push(tool.to_string());
    }

    fn on_tool_result(&mut self, _result: &str) {
        // No-op for collecting handler
    }

    fn on_error(&mut self, _error: &str) {
        self.has_error = true;
    }

    fn on_complete(&mut self) {
        // No-op for collecting handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_should_create_print_handler_with_options() {
        let handler = PrintEventHandler::new().with_tools().with_auto_flush();

        assert!(handler.show_tools);
        assert!(handler.auto_flush);
    }

    #[test]
    fn test_should_collect_text() {
        let mut handler = CollectingEventHandler::new();

        handler.on_text("Hello ");
        handler.on_text("World!");

        assert_eq!(handler.text(), "Hello World!");
    }

    #[test]
    fn test_should_track_tools_used() {
        let mut handler = CollectingEventHandler::new();

        handler.on_tool_use("Read", &json!({"path": "/test"}));
        handler.on_tool_use("Write", &json!({"path": "/out"}));

        assert_eq!(handler.tools_used(), &["Read", "Write"]);
    }

    #[test]
    fn test_should_track_errors() {
        let mut handler = CollectingEventHandler::new();

        assert!(!handler.has_error());

        handler.on_error("test error");

        assert!(handler.has_error());
    }

    #[test]
    fn test_should_clear_collected_state() {
        let mut handler = CollectingEventHandler::new();

        handler.on_text("test");
        handler.on_tool_use("Read", &json!({}));
        handler.on_error("error");

        handler.clear();

        assert!(handler.text().is_empty());
        assert!(handler.tools_used().is_empty());
        assert!(!handler.has_error());
    }

    #[test]
    fn test_should_take_ownership_of_text() {
        let mut handler = CollectingEventHandler::new();
        handler.on_text("owned text");

        let text = handler.into_text();

        assert_eq!(text, "owned text");
    }

    // Test that default trait implementation compiles
    struct NoOpHandler;

    impl EventHandler for NoOpHandler {}

    #[test]
    fn test_should_allow_no_op_handler() {
        let mut handler = NoOpHandler;

        // These should all be no-ops
        handler.on_text("test");
        handler.on_tool_use("tool", &json!({}));
        handler.on_tool_result("result");
        handler.on_error("error");
        handler.on_complete();
    }
}
