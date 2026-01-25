//! TUI (Terminal User Interface) module for GBA.
//!
//! This module provides a ratatui-based chat interface for the interactive
//! planning workflow. It handles user input, displays conversation history,
//! and manages the planning session state.
//!
//! # Architecture
//!
//! The TUI is organized into several components:
//!
//! - [`App`] - Main application state and logic
//! - [`ChatWidget`] - Widget for displaying chat messages
//! - [`InputHandler`] - Processes keyboard events
//! - [`ProgressWidget`] - Widget for displaying execution progress
//!
//! # Example
//!
//! ```no_run
//! use gba_cli::tui::App;
//! use gba_core::Engine;
//! use std::path::Path;
//!
//! # async fn example(engine: &Engine<'_>) -> anyhow::Result<()> {
//! let workdir = Path::new(".");
//! let mut app = App::new("my-feature".to_string(), "0001".to_string(), workdir);
//! app.run(engine).await?;
//! # Ok(())
//! # }
//! ```

mod app;
mod chat;
mod input;
mod progress;
mod run_app;

pub use app::App;
pub use chat::ChatWidget;
pub use input::{InputAction, InputHandler};
pub use progress::{PhaseDisplayStatus, PhaseInfo, ProgressState, ProgressWidget};
pub use run_app::{
    CheckFinalResult, CheckIterationResult, CheckState, CheckStatus, CheckType, ExecutionStage,
    RunApp, RunMessage, TuiEventHandler,
};
