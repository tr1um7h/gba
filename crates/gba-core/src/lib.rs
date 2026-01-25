//! GBA Core Execute Engine
//!
//! This crate provides the core execution engine for the GBA agent system,
//! wrapping the Claude Agent SDK for programmatic access to Claude's capabilities.
//!
//! # Overview
//!
//! The GBA core engine orchestrates AI-assisted workflows by:
//!
//! 1. Loading task configurations from the `tasks/` directory
//! 2. Rendering system and user prompts using Jinja templates
//! 3. Configuring and invoking the Claude agent
//! 4. Processing responses and extracting results
//!
//! The engine supports two modes of operation:
//!
//! - **Single-shot tasks**: Execute predefined tasks with [`Engine::run()`] or
//!   [`Engine::run_stream()`] for streaming output.
//! - **Interactive sessions**: Multi-turn conversations with [`Session`] for
//!   conversational workflows with history tracking.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     ┌──────────────────┐     ┌────────────────────┐
//! │      Task       │────▶│      Engine      │────▶│  Claude Agent SDK  │
//! └─────────────────┘     └──────────────────┘     └────────────────────┘
//!         │                       │
//!         │               ┌───────┴───────┐
//!         │               ▼               ▼
//!         │       ┌──────────────┐ ┌─────────────┐
//!         │       │  run()       │ │  session()  │
//!         │       │  run_stream()│ │             │
//!         │       └──────────────┘ └──────┬──────┘
//!         │                               │
//!         │                               ▼
//!         │                       ┌─────────────┐
//!         │                       │   Session   │
//!         │                       │  send()     │
//!         │                       │  send_stream│
//!         │                       └─────────────┘
//!         │
//!         ▼
//! ┌──────────────────┐
//! │  PromptManager   │
//! │   (gba-pm)       │
//! └──────────────────┘
//! ```
//!
//! # Example: Single-shot Task
//!
//! ```no_run
//! use gba_core::{Engine, EngineConfig, Task, TaskKind};
//! use gba_pm::PromptManager;
//! use serde_json::json;
//!
//! # async fn example() -> gba_core::Result<()> {
//! // Create and configure the prompt manager
//! let mut prompts = PromptManager::new();
//! prompts.load_dir("./tasks")?;
//!
//! // Create the engine
//! let config = EngineConfig::builder()
//!     .workdir(".")
//!     .prompts(prompts)
//!     .build();
//! let engine = Engine::new(config)?;
//!
//! // Create and run a task
//! let task = Task::new(
//!     TaskKind::Init,
//!     json!({"repo_path": "."}),
//! );
//! let result = engine.run(task).await?;
//!
//! println!("Success: {}", result.success);
//! println!("Output: {}", result.output);
//! println!("Turns: {}", result.stats.turns);
//! println!("Cost: ${:.4}", result.stats.cost_usd);
//! # Ok(())
//! # }
//! ```
//!
//! # Example: Streaming Task
//!
//! ```no_run
//! use gba_core::{Engine, EngineConfig, Task, TaskKind};
//! use gba_core::event::PrintEventHandler;
//! use gba_pm::PromptManager;
//! use serde_json::json;
//!
//! # async fn example() -> gba_core::Result<()> {
//! let mut prompts = PromptManager::new();
//! prompts.load_dir("./tasks")?;
//!
//! let config = EngineConfig::builder()
//!     .workdir(".")
//!     .prompts(prompts)
//!     .build();
//! let engine = Engine::new(config)?;
//!
//! let task = Task::new(TaskKind::Init, json!({"repo_path": "."}));
//! let mut handler = PrintEventHandler::new().with_auto_flush();
//! let result = engine.run_stream(task, &mut handler).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Example: Interactive Session
//!
//! ```no_run
//! use gba_core::{Engine, EngineConfig};
//! use gba_core::event::PrintEventHandler;
//! use gba_pm::PromptManager;
//!
//! # async fn example() -> gba_core::Result<()> {
//! let prompts = PromptManager::new();
//! let config = EngineConfig::builder()
//!     .workdir(".")
//!     .prompts(prompts)
//!     .build();
//! let engine = Engine::new(config)?;
//!
//! // Create a session for multi-turn conversation
//! let mut session = engine.session(None)?;
//!
//! // Send messages
//! let response = session.send("What is Rust?").await?;
//! println!("Claude: {}", response);
//!
//! // Follow-up (Claude remembers context)
//! let mut handler = PrintEventHandler::new().with_auto_flush();
//! let response = session.send_stream("Tell me about ownership", &mut handler).await?;
//!
//! // View stats
//! let stats = session.stats();
//! println!("Total turns: {}", stats.turns);
//! println!("Total cost: ${:.4}", stats.cost_usd);
//!
//! session.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Task Configuration
//!
//! Each task type has a corresponding directory under `tasks/` containing:
//!
//! - `config.yml` - Task configuration (preset, tools, disallowed tools)
//! - `system.j2` - System prompt template
//! - `user.j2` - User prompt template
//!
//! Example task configuration:
//!
//! ```yaml
//! preset: true                    # Use claude_code preset
//! tools: []                       # Empty = all tools allowed
//! disallowedTools:                # Tools to disallow
//!   - Write
//!   - Edit
//! ```

mod config;
mod engine;
mod error;
pub mod event;
pub mod session;
mod task;

pub use config::{EngineConfig, TaskConfig};
pub use engine::Engine;
pub use error::{EngineError, Result};
pub use session::{ConversationMessage, Session};
pub use task::{Artifact, Task, TaskKind, TaskResult, TaskStats};
