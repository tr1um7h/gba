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
    ClaudeAgentOptions, ClaudeClient, ContentBlock, Message, PermissionMode, SystemPrompt,
    SystemPromptPreset, ToolResultContent, Tools, query,
};
use futures::StreamExt;
use tracing::{debug, info, instrument, trace};

use gba_pm::PromptManager;

use crate::config::{EngineConfig, TaskConfig};
use crate::error::{EngineError, Result};
use crate::event::EventHandler;
use crate::session::{Session, SessionBuilder};
use crate::task::{Task, TaskKind, TaskResult, TaskStats};

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

        // Process streaming response
        let mut output = String::new();
        let mut stats = TaskStats::default();
        let mut success = true;

        let mut stream = client.receive_response();
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => {
                    self.process_streaming_message(
                        &msg,
                        &mut output,
                        &mut stats,
                        &mut success,
                        handler,
                    )?;
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    handler.on_error(&error_msg);
                    drop(stream);
                    client.disconnect().await?;
                    return Err(e.into());
                }
            }
        }
        drop(stream);

        handler.on_complete();

        // Disconnect client
        client.disconnect().await?;

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
            if base.model.is_some() {
                options.model = base.model.clone();
            }
            if base.permission_mode.is_some() {
                options.permission_mode = base.permission_mode;
            }
            if base.max_turns.is_some() {
                options.max_turns = base.max_turns;
            }
            if base.cwd.is_some() {
                options.cwd = base.cwd.clone();
            }
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
        let mut output = String::new();
        let mut stats = TaskStats::default();
        let mut success = true;

        for message in messages {
            match message {
                Message::Assistant(msg) => {
                    for block in &msg.message.content {
                        if let ContentBlock::Text(text) = block {
                            if !output.is_empty() {
                                output.push('\n');
                            }
                            output.push_str(&text.text);
                        }
                    }
                }
                Message::Result(result) => {
                    stats.turns = result.num_turns;
                    stats.cost_usd = result.total_cost_usd.unwrap_or(0.0);

                    // Extract token usage if available
                    if let Some(usage) = result.usage {
                        if let Some(input) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                            stats.input_tokens = input;
                        }
                        if let Some(output_tokens) =
                            usage.get("output_tokens").and_then(|v| v.as_u64())
                        {
                            stats.output_tokens = output_tokens;
                        }
                    }

                    success = !result.is_error;
                }
                _ => {}
            }
        }

        Ok(TaskResult {
            success,
            output,
            artifacts: Vec::new(), // TODO: Extract artifacts from tool use
            stats,
        })
    }

    /// Process a single streaming message.
    fn process_streaming_message(
        &self,
        msg: &Message,
        output: &mut String,
        stats: &mut TaskStats,
        success: &mut bool,
        handler: &mut impl EventHandler,
    ) -> Result<()> {
        match msg {
            Message::Assistant(assistant_msg) => {
                for block in &assistant_msg.message.content {
                    match block {
                        ContentBlock::Text(text) => {
                            output.push_str(&text.text);
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
                stats.turns = result_msg.num_turns;
                stats.cost_usd = result_msg.total_cost_usd.unwrap_or(0.0);

                if let Some(ref usage) = result_msg.usage {
                    if let Some(input) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                        stats.input_tokens = input;
                    }
                    if let Some(output_tokens) = usage.get("output_tokens").and_then(|v| v.as_u64())
                    {
                        stats.output_tokens = output_tokens;
                    }
                }

                *success = !result_msg.is_error;

                if result_msg.is_error {
                    handler.on_error("Claude reported an error");
                }

                trace!(
                    turns = result_msg.num_turns,
                    cost = result_msg.total_cost_usd,
                    "result message processed"
                );
            }
            Message::System(_) | Message::StreamEvent(_) | Message::ControlCancelRequest(_) => {
                // Ignore these message types
            }
        }

        Ok(())
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
}
