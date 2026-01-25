//! Input handling for the TUI.
//!
//! This module processes keyboard events and translates them into
//! application actions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;
use crate::error::CliError;

/// Actions resulting from input handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Continue running (no action taken).
    Continue,
    /// Send a message.
    Send(String),
    /// Exit the application.
    Exit,
    /// Scroll up in the chat area.
    ScrollUp,
    /// Scroll down in the chat area.
    ScrollDown,
}

/// Handles keyboard input for the TUI.
pub struct InputHandler;

impl InputHandler {
    /// Process a keyboard event and return the resulting action.
    ///
    /// # Arguments
    ///
    /// * `app` - Mutable reference to the application state
    /// * `key` - The key event to process
    ///
    /// # Returns
    ///
    /// The action to take based on the key event.
    pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<InputAction, CliError> {
        // Handle control key combinations first
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Self::handle_ctrl_key(app, key.code);
        }

        // Handle regular keys
        match key.code {
            KeyCode::Enter => {
                let input = app.take_input();
                if input.is_empty() {
                    Ok(InputAction::Continue)
                } else {
                    Ok(InputAction::Send(input))
                }
            }
            KeyCode::Char(c) => {
                app.insert_char(c);
                Ok(InputAction::Continue)
            }
            KeyCode::Backspace => {
                app.delete_char();
                Ok(InputAction::Continue)
            }
            KeyCode::Delete => {
                app.delete_char_forward();
                Ok(InputAction::Continue)
            }
            KeyCode::Left => {
                app.move_cursor_left();
                Ok(InputAction::Continue)
            }
            KeyCode::Right => {
                app.move_cursor_right();
                Ok(InputAction::Continue)
            }
            KeyCode::Home => {
                app.move_cursor_home();
                Ok(InputAction::Continue)
            }
            KeyCode::End => {
                app.move_cursor_end();
                Ok(InputAction::Continue)
            }
            KeyCode::PageUp => Ok(InputAction::ScrollUp),
            KeyCode::PageDown => Ok(InputAction::ScrollDown),
            KeyCode::Up => Ok(InputAction::ScrollUp),
            KeyCode::Down => Ok(InputAction::ScrollDown),
            KeyCode::Esc => Ok(InputAction::Exit),
            _ => Ok(InputAction::Continue),
        }
    }

    /// Handle control key combinations.
    fn handle_ctrl_key(app: &mut App, code: KeyCode) -> Result<InputAction, CliError> {
        match code {
            KeyCode::Char('c') | KeyCode::Char('C') => Ok(InputAction::Exit),
            KeyCode::Char('u') | KeyCode::Char('U') => {
                // Clear input (like Ctrl+U in bash)
                app.clear_input();
                Ok(InputAction::Continue)
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Move to beginning of line
                app.move_cursor_home();
                Ok(InputAction::Continue)
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Move to end of line
                app.move_cursor_end();
                Ok(InputAction::Continue)
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                // Delete word backward
                Self::delete_word_backward(app);
                Ok(InputAction::Continue)
            }
            _ => Ok(InputAction::Continue),
        }
    }

    /// Delete the word before the cursor.
    fn delete_word_backward(app: &mut App) {
        let input = app.input().to_string();
        let cursor = app.cursor_position();

        if cursor == 0 {
            return;
        }

        // Find the start of the word
        let before_cursor = &input[..cursor];
        let trimmed = before_cursor.trim_end();

        // Find the last word boundary
        let word_start = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        // Remove characters from word_start to cursor
        let new_input = format!("{}{}", &input[..word_start], &input[cursor..]);
        app.set_input(new_input);

        // Adjust cursor position
        let new_cursor = word_start;
        for _ in 0..(app.cursor_position() - new_cursor) {
            app.move_cursor_left();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: We can't easily test InputHandler without a real App instance,
    // but we can test the InputAction enum.

    #[test]
    fn test_input_action_eq() {
        assert_eq!(InputAction::Continue, InputAction::Continue);
        assert_eq!(InputAction::Exit, InputAction::Exit);
        assert_eq!(InputAction::ScrollUp, InputAction::ScrollUp);
        assert_eq!(InputAction::ScrollDown, InputAction::ScrollDown);
        assert_eq!(
            InputAction::Send("hello".to_string()),
            InputAction::Send("hello".to_string())
        );
        assert_ne!(
            InputAction::Send("hello".to_string()),
            InputAction::Send("world".to_string())
        );
    }

    #[test]
    fn test_input_action_debug() {
        let action = InputAction::Send("test".to_string());
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("Send"));
        assert!(debug_str.contains("test"));
    }
}
