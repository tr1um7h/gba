//! GBA Prompt Manager
//!
//! This crate provides a prompt management system using MiniJinja templates.
//! It allows loading, rendering, and managing prompts for the GBA agent system.
//!
//! # Features
//!
//! - Load templates from directories (supports `.j2`, `.jinja`, `.jinja2` extensions)
//! - Add templates programmatically from strings
//! - Render templates with arbitrary serializable context
//! - Render one-off string templates without registration
//!
//! # Example
//!
//! ```no_run
//! use gba_pm::PromptManager;
//! use serde_json::json;
//!
//! // Create a new prompt manager
//! let mut manager = PromptManager::new();
//!
//! // Load templates from a directory
//! manager.load_dir("./templates")?;
//!
//! // Or add templates programmatically
//! manager.add("greeting", "Hello, {{ name }}!")?;
//!
//! // Render a template with context
//! let result = manager.render("greeting", json!({"name": "World"}))?;
//! assert_eq!(result, "Hello, World!");
//!
//! // List all template names
//! for name in manager.names() {
//!     println!("Template: {}", name);
//! }
//! # Ok::<(), gba_pm::PromptError>(())
//! ```

mod error;
mod manager;

pub use error::{PromptError, Result};
pub use manager::PromptManager;
