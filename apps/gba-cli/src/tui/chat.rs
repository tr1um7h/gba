//! Chat message display widget for the TUI.
//!
//! This module provides the [`ChatWidget`] for rendering conversation messages
//! with proper styling and wrapping.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use textwrap::wrap;

use super::app::{ChatMessage, MessageRole};

/// Widget for displaying chat messages.
pub struct ChatWidget<'a> {
    /// The messages to display.
    messages: &'a [ChatMessage],
    /// Current scroll position.
    scroll: u16,
    /// Visible height for rendering.
    visible_height: u16,
    /// Cached total lines (calculated during construction).
    total_lines: u16,
    /// Cached rendered lines.
    rendered_lines: Vec<(Line<'a>, Style)>,
}

impl<'a> ChatWidget<'a> {
    /// Create a new chat widget.
    ///
    /// # Arguments
    ///
    /// * `messages` - The conversation messages to display
    /// * `scroll` - Current scroll position
    /// * `visible_height` - Height of the visible area
    /// * `width` - Width of the rendering area for accurate line calculation
    pub fn new(messages: &'a [ChatMessage], scroll: u16, visible_height: u16, width: u16) -> Self {
        let mut widget = Self {
            messages,
            scroll,
            visible_height,
            total_lines: 0,
            rendered_lines: Vec::new(),
        };
        // Pre-calculate with actual width for accurate scroll calculations
        widget.prepare_lines(width);
        widget
    }

    /// Get the total number of lines in the chat.
    pub fn total_lines(&self) -> u16 {
        self.total_lines
    }

    /// Prepare lines for rendering.
    fn prepare_lines(&mut self, width: u16) {
        self.rendered_lines.clear();
        let wrap_width = width.saturating_sub(4) as usize; // Account for prefix

        for message in self.messages {
            let (prefix, style) = match message.role {
                MessageRole::User => (
                    "You: ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                MessageRole::Assistant => ("Assistant: ", Style::default().fg(Color::Cyan)),
                MessageRole::System => (
                    "",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
            };

            // Wrap the content
            let content = &message.content;
            let wrapped = wrap(content, wrap_width.max(20));

            let mut first_line = true;
            for line_content in wrapped {
                let line = if first_line {
                    Line::from(vec![
                        Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                        Span::styled(line_content.to_string(), style),
                    ])
                } else {
                    // Indent continuation lines
                    let indent = " ".repeat(prefix.len());
                    Line::from(vec![
                        Span::raw(indent),
                        Span::styled(line_content.to_string(), style),
                    ])
                };
                self.rendered_lines.push((line, style));
                first_line = false;
            }

            // Add empty line between messages
            self.rendered_lines
                .push((Line::default(), Style::default()));
        }

        self.total_lines = self.rendered_lines.len() as u16;
    }
}

impl Widget for ChatWidget<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        // Recalculate lines for actual width
        self.prepare_lines(area.width);

        let total = self.rendered_lines.len();
        let visible = self.visible_height as usize;

        // Clamp scroll to valid range: max_scroll ensures we can see all content
        let max_scroll = total.saturating_sub(visible);
        let scroll = (self.scroll as usize).min(max_scroll);

        // Get the lines to display based on scroll position
        let start = scroll;
        let end = (start + visible).min(total);

        for (i, (line, _style)) in self.rendered_lines[start..end].iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            // Render each span in the line
            let mut x = area.x;
            for span in line.spans.iter() {
                let span_width = span.content.len() as u16;
                let available = area.width.saturating_sub(x - area.x);
                let render_width = span_width.min(available);

                if render_width > 0 {
                    buf.set_string(x, y, &span.content[..render_width as usize], span.style);
                    x += render_width;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_message(role: MessageRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_chat_widget_creation() {
        let messages = vec![
            create_test_message(MessageRole::User, "Hello"),
            create_test_message(MessageRole::Assistant, "Hi there!"),
        ];

        let widget = ChatWidget::new(&messages, 0, 10, 80);
        assert!(widget.total_lines() > 0);
    }

    #[test]
    fn test_chat_widget_empty_messages() {
        let messages: Vec<ChatMessage> = vec![];
        let widget = ChatWidget::new(&messages, 0, 10, 80);
        assert_eq!(widget.total_lines(), 0);
    }

    #[test]
    fn test_chat_widget_scroll() {
        let messages = vec![
            create_test_message(MessageRole::User, "Line 1"),
            create_test_message(MessageRole::Assistant, "Line 2"),
            create_test_message(MessageRole::User, "Line 3"),
        ];

        let widget = ChatWidget::new(&messages, 2, 10, 80);
        assert_eq!(widget.scroll, 2);
    }
}
