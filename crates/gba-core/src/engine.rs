//! Core execution engine for GBA.
//!
//! This module provides the [`Engine`] struct that orchestrates
//! AI-assisted workflows using the Claude Agent SDK.
//!
//! # Overview
//!
//! The engine supports two modes of operation:
//!
//! 1. **Single-shot tasks** via [`run()`](Engine::run) and [`run_stream()`](Engine::run_stream)
//!    for executing predefined task types with templates.
//!
//! 2. **Interactive sessions** via [`session()`](Engine::session) for multi-turn
//!    conversations with history tracking and streaming support.
//!
//! # Example
//!
//! ```no_run
//! use gba_core::{Engine, EngineConfig, Task, TaskKind};
//! use gba_core::event::PrintEventHandler;
//! use gba_pm::PromptManager;
//! use serde_json::json;
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
//! // Run a single-shot task
//! let task = Task::new(TaskKind::Init, json!({"repo_path": "."}));
//! let result = engine.run(task).await?;
//!
//! // Or run with streaming
//! let task = Task::new(TaskKind::Init, json!({"repo_path": "."}));
//! let mut handler = PrintEventHandler::new().with_auto_flush();
//! let result = engine.run_stream(task, &mut handler).await?;
//!
//! // Or create an interactive session
//! let mut session = engine.session(None)?;
//! let response = session.send("Hello!").await?;
//! session.disconnect().await?;
//! # Ok(())
//! # }
//! ```

use std::fs;
use std::path::PathBuf;

use claude_agent_sdk_rs::{
    ClaudeAgentOptions, ClaudeClient, Message, PermissionMode, SystemPrompt, SystemPromptPreset,
    Tools, query,
};
use futures::StreamExt;
use tracing::{debug, info, instrument};

use gba_pm::PromptManager;

use crate::config::{EngineConfig, TaskConfig, merge_base_options};
use crate::error::{EngineError, Result};
use crate::event::EventHandler;
use crate::message::MessageProcessor;
use crate::session::{Session, SessionBuilder};
use crate::task::{Task, TaskKind, TaskResult};

/// Core execution engine for GBA.
///
/// The engine orchestrates AI-assisted workflows by:
/// 1. Loading task configurations from the `tasks/` directory
/// 2. Rendering system and user prompts using templates
/// 3. Configuring and invoking the Claude agent
/// 4. Processing responses and extracting results
///
/// # Example
///
/// ```no_run
/// use gba_core::{Engine, EngineConfig, Task, TaskKind};
/// use gba_pm::PromptManager;
/// use serde_json::json;
///
/// # async fn example() -> gba_core::Result<()> {
/// // Create and configure the prompt manager
/// let mut prompts = PromptManager::new();
/// prompts.load_dir("./tasks")?;
///
/// // Create the engine
/// let config = EngineConfig::builder()
///     .workdir(".")
///     .prompts(prompts)
///     .build();
/// let engine = Engine::new(config)?;
///
/// // Run a task
/// let task = Task::new(TaskKind::Init, json!({"repo_path": "."}));
/// let result = engine.run(task).await?;
/// println!("Success: {}", result.success);
/// # Ok(())
/// # }
/// ```
pub struct Engine<'a> {
    /// Working directory for the engine.
    workdir: PathBuf,

    /// Prompt manager containing loaded templates.
    prompts: PromptManager<'a>,

    /// Base agent options to merge with task-specific options.
    base_options: Option<ClaudeAgentOptions>,
}

impl std::fmt::Debug for Engine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("workdir", &self.workdir)
            .field("prompts", &self.prompts)
            .field("base_options", &"<ClaudeAgentOptions>")
            .finish()
    }
}

impl<'a> Engine<'a> {
    /// Create a new engine with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Engine configuration including workdir and prompts
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn new(config: EngineConfig<'a>) -> Result<Self> {
        debug!(workdir = %config.workdir.display(), "creating engine");

        Ok(Self {
            workdir: config.workdir,
            prompts: config.prompts,
            base_options: config.agent_options,
        })
    }

    /// Run a task and return the result.
    ///
    /// This method:
    /// 1. Loads the task configuration from `tasks/<kind>/config.yml`
    /// 2. Renders the system and user prompts from templates
    /// 3. Configures the Claude agent based on the task config
    /// 4. Executes the query and processes the response
    ///
    /// # Arguments
    ///
    /// * `task` - The task to execute
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The task configuration cannot be loaded
    /// - The prompt templates cannot be rendered
    /// - The Claude agent query fails
    #[instrument(skip(self, task), fields(task_kind = %task.kind))]
    pub async fn run(&self, task: Task) -> Result<TaskResult> {
        info!(task_kind = %task.kind, "running task");

        // Load task configuration
        let task_config = self.load_task_config(&task.kind)?;
        debug!(?task_config, "loaded task configuration");

        // Render prompts
        let system_prompt = self.render_system_prompt(&task, &task_config)?;
        let user_prompt = self.render_user_prompt(&task)?;
        debug!("rendered prompts");

        // Build agent options
        let options = self.build_agent_options(&task_config, system_prompt);

        // Execute query
        info!("executing Claude agent query");
        let messages = query(&user_prompt, Some(options)).await?;

        // Process results
        let result = self.process_messages(messages)?;
        info!(
            success = result.success,
            turns = result.stats.turns,
            "task completed"
        );

        Ok(result)
    }

    /// Run a task with streaming events.
    ///
    /// This method is similar to [`run()`](Self::run) but streams events
    /// to the provided handler during execution, enabling real-time
    /// feedback and progress tracking.
    ///
    /// # Arguments
    ///
    /// * `task` - The task to execute
    /// * `handler` - Event handler for streaming events
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The task configuration cannot be loaded
    /// - The prompt templates cannot be rendered
    /// - The Claude agent query fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::{Engine, EngineConfig, Task, TaskKind};
    /// use gba_core::event::PrintEventHandler;
    /// use gba_pm::PromptManager;
    /// use serde_json::json;
    ///
    /// # async fn example() -> gba_core::Result<()> {
    /// let mut prompts = PromptManager::new();
    /// prompts.load_dir("./tasks")?;
    ///
    /// let config = EngineConfig::builder()
    ///     .workdir(".")
    ///     .prompts(prompts)
    ///     .build();
    /// let engine = Engine::new(config)?;
    ///
    /// let task = Task::new(TaskKind::Init, json!({"repo_path": "."}));
    /// let mut handler = PrintEventHandler::new().with_auto_flush();
    /// let result = engine.run_stream(task, &mut handler).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, task, handler), fields(task_kind = %task.kind))]
    pub async fn run_stream(
        &self,
        task: Task,
        handler: &mut impl EventHandler,
    ) -> Result<TaskResult> {
        info!(task_kind = %task.kind, "running task with streaming");

        // Load task configuration
        let task_config = self.load_task_config(&task.kind)?;
        debug!(?task_config, "loaded task configuration");

        // Render prompts
        let system_prompt = self.render_system_prompt(&task, &task_config)?;
        let user_prompt = self.render_user_prompt(&task)?;
        debug!("rendered prompts");

        // Build agent options
        let options = self.build_agent_options(&task_config, system_prompt);

        // Create client and connect
        let mut client = ClaudeClient::new(options);
        client.connect().await?;

        // Send query
        info!("sending query to Claude");
        client.query(&user_prompt).await?;

        // Collect messages first, then process them
        let mut messages = Vec::new();
        {
            let mut stream = client.receive_response();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => messages.push(msg),
                    Err(e) => {
                        handler.on_error(&e.to_string());
                        // Stream is dropped here at end of scope
                        drop(stream);
                        client.disconnect().await?;
                        return Err(e.into());
                    }
                }
            }
        }

        // Process all messages using MessageProcessor
        let mut processor = MessageProcessor::new();
        for msg in &messages {
            processor.process_with_handler(msg, handler);
        }

        handler.on_complete();

        // Disconnect client
        client.disconnect().await?;

        // Get stats before taking output (which moves processor)
        let stats = processor.stats().clone();
        let success = processor.success();
        let output = processor.take_output();

        let result = TaskResult {
            success,
            output,
            artifacts: Vec::new(),
            stats,
        };

        info!(
            success = result.success,
            turns = result.stats.turns,
            "streaming task completed"
        );

        Ok(result)
    }

    /// Create an interactive session for multi-turn conversations.
    ///
    /// Sessions maintain a persistent connection to Claude and track
    /// conversation history and statistics across multiple turns.
    ///
    /// # Arguments
    ///
    /// * `session_id` - Optional session ID; if None, a UUID is generated
    ///
    /// # Errors
    ///
    /// Returns an error if the session cannot be created.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::{Engine, EngineConfig};
    /// use gba_pm::PromptManager;
    ///
    /// # async fn example() -> gba_core::Result<()> {
    /// let mut prompts = PromptManager::new();
    /// let config = EngineConfig::builder()
    ///     .workdir(".")
    ///     .prompts(prompts)
    ///     .build();
    /// let engine = Engine::new(config)?;
    ///
    /// let mut session = engine.session(None)?;
    /// let response = session.send("Hello Claude!").await?;
    /// println!("Claude: {}", response);
    ///
    /// // Follow-up in same session
    /// let response = session.send("Tell me more").await?;
    ///
    /// // Check accumulated stats
    /// let stats = session.stats();
    /// println!("Total turns: {}", stats.turns);
    ///
    /// session.disconnect().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn session(&self, session_id: Option<String>) -> Result<Session> {
        debug!(session_id = ?session_id, "creating session from engine");

        let mut builder = SessionBuilder::new(self.workdir.clone());

        if let Some(ref base) = self.base_options {
            builder = builder.with_base_options(base.clone());
        }

        if let Some(id) = session_id {
            builder = builder.with_session_id(id);
        }

        builder.build()
    }

    /// Create a session with a specific task configuration.
    ///
    /// This method creates a session that uses the configuration from
    /// a specific task type, including its system prompt and tool settings.
    ///
    /// # Arguments
    ///
    /// * `task_kind` - The task kind to use for configuration
    /// * `context` - Context for rendering the system prompt template
    /// * `session_id` - Optional session ID
    ///
    /// # Errors
    ///
    /// Returns an error if the task configuration cannot be loaded or
    /// the session cannot be created.
    pub fn session_with_task(
        &self,
        task_kind: &TaskKind,
        context: &serde_json::Value,
        session_id: Option<String>,
    ) -> Result<Session> {
        debug!(task_kind = %task_kind, session_id = ?session_id, "creating session with task config");

        let task_config = self.load_task_config(task_kind)?;

        // Create a temporary task to render the system prompt
        let temp_task = Task::new(task_kind.clone(), context.clone());
        let system_prompt = self.render_system_prompt(&temp_task, &task_config)?;

        let mut builder = SessionBuilder::new(self.workdir.clone()).with_task_config(task_config);

        if let Some(ref base) = self.base_options {
            builder = builder.with_base_options(base.clone());
        }

        if let Some(prompt) = system_prompt {
            builder = builder.with_system_prompt(prompt);
        }

        if let Some(id) = session_id {
            builder = builder.with_session_id(id);
        }

        builder.build()
    }

    /// Load task configuration from the tasks directory.
    fn load_task_config(&self, kind: &TaskKind) -> Result<TaskConfig> {
        let config_path = self
            .workdir
            .join("tasks")
            .join(kind.dir_name())
            .join("config.yml");

        if !config_path.exists() {
            return Err(EngineError::TaskConfigNotFound(kind.to_string()));
        }

        let content =
            fs::read_to_string(&config_path).map_err(|e| EngineError::io_error(&config_path, e))?;

        let config: TaskConfig =
            serde_yaml::from_str(&content).map_err(|e| EngineError::yaml_error(&config_path, e))?;

        Ok(config)
    }

    /// Render the system prompt for a task.
    fn render_system_prompt(
        &self,
        task: &Task,
        config: &TaskConfig,
    ) -> Result<Option<SystemPrompt>> {
        // If task has a custom system prompt override, use it directly
        if let Some(ref override_prompt) = task.system_prompt {
            return Ok(Some(SystemPrompt::Text(override_prompt.clone())));
        }

        // Try to render the system template
        let template_name = format!("{}/system", task.kind.dir_name());
        let rendered = match self.prompts.render(&template_name, &task.context) {
            Ok(content) => content,
            Err(gba_pm::PromptError::TemplateNotFound(_)) => {
                // No system template, use preset default or none
                if config.preset {
                    return Ok(Some(SystemPrompt::Preset(SystemPromptPreset::new(
                        "claude_code",
                    ))));
                }
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };

        // Build system prompt based on preset configuration
        if config.preset {
            Ok(Some(SystemPrompt::Preset(SystemPromptPreset::with_append(
                "claude_code",
                rendered,
            ))))
        } else {
            Ok(Some(SystemPrompt::Text(rendered)))
        }
    }

    /// Render the user prompt for a task.
    fn render_user_prompt(&self, task: &Task) -> Result<String> {
        let template_name = format!("{}/user", task.kind.dir_name());
        let rendered = self.prompts.render(&template_name, &task.context)?;
        Ok(rendered)
    }

    /// Build Claude agent options from task configuration.
    fn build_agent_options(
        &self,
        config: &TaskConfig,
        system_prompt: Option<SystemPrompt>,
    ) -> ClaudeAgentOptions {
        // Start with default options
        let mut options = ClaudeAgentOptions::default();

        // Apply base options if provided
        if let Some(ref base) = self.base_options {
            merge_base_options(&mut options, base);
        }

        // Set working directory if not already set
        if options.cwd.is_none() {
            options.cwd = Some(self.workdir.clone());
        }

        // Set system prompt
        if system_prompt.is_some() {
            options.system_prompt = system_prompt;
        }

        // Set tools configuration using `tools` field (maps to --tools CLI flag)
        if !config.tools.is_empty() {
            options.tools = Some(Tools::from(config.tools.clone()));
        }

        if !config.disallowed_tools.is_empty() {
            options.disallowed_tools = config.disallowed_tools.clone();
        }

        // Apply task-specific permission mode if configured
        if let Some(mode) = config.permission_mode {
            options.permission_mode = Some(mode.into());
        }

        // Default to bypass permissions if not set (for automated execution)
        if options.permission_mode.is_none() {
            options.permission_mode = Some(PermissionMode::BypassPermissions);
        }

        // Skip version check for faster execution
        options.skip_version_check = true;

        options
    }

    /// Process messages from Claude agent response.
    fn process_messages(&self, messages: Vec<Message>) -> Result<TaskResult> {
        let mut processor = MessageProcessor::new();

        for message in &messages {
            processor.process(message);
        }

        // Get stats before taking output (which moves processor)
        let stats = processor.stats().clone();
        let success = processor.success();
        let output = processor.take_output();

        Ok(TaskResult {
            success,
            output,
            artifacts: Vec::new(),
            stats,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_task_dir(temp_dir: &TempDir) -> PathBuf {
        let tasks_dir = temp_dir.path().join("tasks").join("init");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Create config.yml
        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: true
tools: []
disallowedTools: []
"#,
        )
        .unwrap();

        // Create system.j2
        fs::write(
            tasks_dir.join("system.j2"),
            "You are GBA. Working directory: {{ repo_path }}",
        )
        .unwrap();

        // Create user.j2
        fs::write(tasks_dir.join("user.j2"), "Initialize the repository.").unwrap();

        temp_dir.path().to_path_buf()
    }

    #[test]
    fn test_should_load_task_config() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine.load_task_config(&TaskKind::Init).unwrap();

        assert!(task_config.preset);
        assert!(task_config.tools.is_empty());
        assert!(task_config.disallowed_tools.is_empty());
    }

    #[test]
    fn test_should_render_user_prompt() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task = Task::new(TaskKind::Init, json!({"repo_path": "/test"}));

        let user_prompt = engine.render_user_prompt(&task).unwrap();
        assert_eq!(user_prompt, "Initialize the repository.");
    }

    #[test]
    fn test_should_render_system_prompt_with_preset() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task = Task::new(TaskKind::Init, json!({"repo_path": "/test"}));
        let task_config = engine.load_task_config(&task.kind).unwrap();

        let system_prompt = engine.render_system_prompt(&task, &task_config).unwrap();

        match system_prompt {
            Some(SystemPrompt::Preset(preset)) => {
                assert_eq!(preset.preset, "claude_code");
                assert!(preset.append.is_some());
                assert!(preset.append.unwrap().contains("/test"));
            }
            _ => panic!("Expected preset system prompt"),
        }
    }

    #[test]
    fn test_should_use_custom_system_prompt_override() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task = Task::new(TaskKind::Init, json!({})).with_system_prompt("Custom override");
        let task_config = engine.load_task_config(&task.kind).unwrap();

        let system_prompt = engine.render_system_prompt(&task, &task_config).unwrap();

        match system_prompt {
            Some(SystemPrompt::Text(text)) => {
                assert_eq!(text, "Custom override");
            }
            _ => panic!("Expected text system prompt"),
        }
    }

    #[test]
    fn test_should_return_error_for_missing_task_config() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("tasks")).unwrap();

        let prompts = PromptManager::new();
        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let result = engine.load_task_config(&TaskKind::Custom("nonexistent".to_string()));

        assert!(matches!(result, Err(EngineError::TaskConfigNotFound(_))));
    }

    #[test]
    fn test_should_build_agent_options_with_disallowed_tools() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("review");
        fs::create_dir_all(&tasks_dir).unwrap();

        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: true
tools: []
disallowedTools:
  - Write
  - Edit
"#,
        )
        .unwrap();

        fs::write(tasks_dir.join("system.j2"), "Review mode.").unwrap();
        fs::write(tasks_dir.join("user.j2"), "Review the code.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine.load_task_config(&TaskKind::Review).unwrap();

        let options = engine.build_agent_options(&task_config, None);

        assert_eq!(options.disallowed_tools, vec!["Write", "Edit"]);
    }

    // =========================================================================
    // Non-preset system prompt rendering tests
    // =========================================================================

    #[test]
    fn test_should_render_system_prompt_without_preset() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("custom");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Create config with preset=false
        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: false
tools: []
disallowedTools: []
"#,
        )
        .unwrap();

        fs::write(
            tasks_dir.join("system.j2"),
            "Custom system prompt for {{ task_name }}.",
        )
        .unwrap();
        fs::write(tasks_dir.join("user.j2"), "Do the custom task.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task = Task::new(
            TaskKind::Custom("custom".to_string()),
            json!({"task_name": "testing"}),
        );
        let task_config = engine.load_task_config(&task.kind).unwrap();

        let system_prompt = engine.render_system_prompt(&task, &task_config).unwrap();

        match system_prompt {
            Some(SystemPrompt::Text(text)) => {
                assert_eq!(text, "Custom system prompt for testing.");
            }
            _ => panic!("Expected plain text system prompt for non-preset config"),
        }
    }

    // =========================================================================
    // Missing template fallback tests
    // =========================================================================

    #[test]
    fn test_should_fallback_to_preset_when_no_system_template() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("minimal");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Create config but NO system.j2 template
        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: true
tools: []
disallowedTools: []
"#,
        )
        .unwrap();

        // Only create user.j2, no system.j2
        fs::write(tasks_dir.join("user.j2"), "Execute minimal task.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task = Task::new(TaskKind::Custom("minimal".to_string()), json!({}));
        let task_config = engine.load_task_config(&task.kind).unwrap();

        let system_prompt = engine.render_system_prompt(&task, &task_config).unwrap();

        // Should fall back to preset when template is missing
        match system_prompt {
            Some(SystemPrompt::Preset(preset)) => {
                assert_eq!(preset.preset, "claude_code");
                assert!(preset.append.is_none());
            }
            _ => panic!("Expected preset fallback when no system template exists"),
        }
    }

    #[test]
    fn test_should_return_none_when_no_system_template_and_no_preset() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("bare");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Create config with preset=false and NO system.j2 template
        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: false
tools: []
disallowedTools: []
"#,
        )
        .unwrap();

        fs::write(tasks_dir.join("user.j2"), "Execute bare task.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task = Task::new(TaskKind::Custom("bare".to_string()), json!({}));
        let task_config = engine.load_task_config(&task.kind).unwrap();

        let system_prompt = engine.render_system_prompt(&task, &task_config).unwrap();

        // Should return None when no template and no preset
        assert!(system_prompt.is_none());
    }

    // =========================================================================
    // Agent options building with all configurations
    // =========================================================================

    #[test]
    fn test_should_build_agent_options_with_tools() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("restricted");
        fs::create_dir_all(&tasks_dir).unwrap();

        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: true
tools:
  - Read
  - Grep
  - Glob
disallowedTools: []
"#,
        )
        .unwrap();

        fs::write(tasks_dir.join("system.j2"), "Read-only mode.").unwrap();
        fs::write(tasks_dir.join("user.j2"), "Search for files.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine
            .load_task_config(&TaskKind::Custom("restricted".to_string()))
            .unwrap();

        let options = engine.build_agent_options(&task_config, None);

        // Tools should be set
        assert!(options.tools.is_some());
    }

    #[test]
    fn test_should_build_agent_options_with_permission_mode() {
        use crate::config::TaskPermissionMode;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("manual");
        fs::create_dir_all(&tasks_dir).unwrap();

        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: true
tools: []
disallowedTools: []
permissionMode: acceptEdits
"#,
        )
        .unwrap();

        fs::write(tasks_dir.join("system.j2"), "Manual approval mode.").unwrap();
        fs::write(tasks_dir.join("user.j2"), "Make changes.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine
            .load_task_config(&TaskKind::Custom("manual".to_string()))
            .unwrap();

        // Verify task config has the permission mode
        assert_eq!(
            task_config.permission_mode,
            Some(TaskPermissionMode::AcceptEdits)
        );

        let options = engine.build_agent_options(&task_config, None);

        // Permission mode should be set
        assert_eq!(options.permission_mode, Some(PermissionMode::AcceptEdits));
    }

    #[test]
    fn test_should_build_agent_options_with_base_options() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("tasks").join("init");
        fs::create_dir_all(&tasks_dir).unwrap();

        fs::write(
            tasks_dir.join("config.yml"),
            r#"
preset: true
tools: []
disallowedTools: []
"#,
        )
        .unwrap();

        fs::write(tasks_dir.join("system.j2"), "Init task.").unwrap();
        fs::write(tasks_dir.join("user.j2"), "Initialize.").unwrap();

        let mut prompts = PromptManager::new();
        prompts.load_dir(temp_dir.path().join("tasks")).unwrap();

        // Create engine with base options
        let base_options = ClaudeAgentOptions {
            model: Some("claude-3-opus".to_string()),
            max_turns: Some(50),
            ..Default::default()
        };

        let config = EngineConfig::builder()
            .workdir(temp_dir.path())
            .prompts(prompts)
            .agent_options(Some(base_options))
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine.load_task_config(&TaskKind::Init).unwrap();

        let options = engine.build_agent_options(&task_config, None);

        // Base options should be merged
        assert_eq!(options.model, Some("claude-3-opus".to_string()));
        assert_eq!(options.max_turns, Some(50));
    }

    #[test]
    fn test_should_set_workdir_in_agent_options() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine.load_task_config(&TaskKind::Init).unwrap();

        let options = engine.build_agent_options(&task_config, None);

        assert_eq!(options.cwd, Some(workdir));
    }

    #[test]
    fn test_should_skip_version_check() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let task_config = engine.load_task_config(&TaskKind::Init).unwrap();

        let options = engine.build_agent_options(&task_config, None);

        // Version check should be skipped for faster execution
        assert!(options.skip_version_check);
    }

    // =========================================================================
    // Session creation tests
    // =========================================================================

    #[test]
    fn test_should_create_session_from_engine() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let session = engine.session(None).unwrap();

        assert!(!session.session_id().is_empty());
        assert!(!session.is_connected());
    }

    #[test]
    fn test_should_create_session_with_custom_id() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let session = engine
            .session(Some("my-custom-session".to_string()))
            .unwrap();

        assert_eq!(session.session_id(), "my-custom-session");
    }

    #[test]
    fn test_should_create_session_with_task_config() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let session = engine
            .session_with_task(&TaskKind::Init, &json!({"repo_path": "/test"}), None)
            .unwrap();

        assert!(!session.session_id().is_empty());
    }

    // =========================================================================
    // Engine Debug implementation test
    // =========================================================================

    #[test]
    fn test_engine_debug_format() {
        let temp_dir = TempDir::new().unwrap();
        let workdir = create_test_task_dir(&temp_dir);

        let mut prompts = PromptManager::new();
        prompts.load_dir(workdir.join("tasks")).unwrap();

        let config = EngineConfig::builder()
            .workdir(&workdir)
            .prompts(prompts)
            .build();

        let engine = Engine::new(config).unwrap();
        let debug_output = format!("{:?}", engine);

        assert!(debug_output.contains("Engine"));
        assert!(debug_output.contains("workdir"));
    }
}
