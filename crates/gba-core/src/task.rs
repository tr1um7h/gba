//! Task types for the GBA core engine.
//!
//! This module defines the task structure and related types
//! for representing work units executed by the engine.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The kind of task to execute.
///
/// Each task kind maps to a directory under `tasks/` containing
/// the task configuration and prompt templates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskKind {
    /// Initialize a repository for GBA.
    Init,
    /// Plan a new feature through interactive conversation.
    Plan,
    /// Execute a planned feature phase.
    Execute,
    /// Review code changes (read-only).
    Review,
    /// Verify that implementations meet specifications.
    Verification,
    /// Custom task with a user-defined name.
    Custom(String),
}

impl TaskKind {
    /// Get the directory name for this task kind.
    ///
    /// This is used to locate the task configuration and templates
    /// under the `tasks/` directory.
    #[must_use]
    pub fn dir_name(&self) -> &str {
        match self {
            Self::Init => "init",
            Self::Plan => "plan",
            Self::Execute => "execute",
            Self::Review => "review",
            Self::Verification => "verification",
            Self::Custom(name) => name.as_str(),
        }
    }
}

impl std::fmt::Display for TaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.dir_name())
    }
}

/// A task to be executed by the engine.
///
/// A task combines a task kind with context data for template rendering
/// and an optional system prompt override.
#[derive(Debug, Clone)]
pub struct Task {
    /// The kind of task to execute.
    pub kind: TaskKind,

    /// Context data for template rendering.
    ///
    /// This is passed to the Jinja templates when rendering
    /// the system and user prompts.
    pub context: serde_json::Value,

    /// Optional system prompt override.
    ///
    /// If provided, this completely overrides the rendered system prompt
    /// from the template.
    pub system_prompt: Option<String>,
}

impl Task {
    /// Create a new task with the given kind and context.
    ///
    /// # Arguments
    ///
    /// * `kind` - The type of task to execute
    /// * `context` - Context data for template rendering
    ///
    /// # Example
    ///
    /// ```
    /// use gba_core::{Task, TaskKind};
    /// use serde_json::json;
    ///
    /// let task = Task::new(TaskKind::Init, json!({"repo_path": "/path/to/repo"}));
    /// ```
    #[must_use]
    pub fn new(kind: TaskKind, context: serde_json::Value) -> Self {
        Self {
            kind,
            context,
            system_prompt: None,
        }
    }

    /// Set an optional system prompt override.
    ///
    /// This method consumes and returns `self` for method chaining.
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

/// Result of a task execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TaskResult {
    /// Whether the task completed successfully.
    pub success: bool,

    /// The output text from the task execution.
    pub output: String,

    /// Artifacts produced by the task (e.g., files created or modified).
    pub artifacts: Vec<Artifact>,

    /// Execution statistics.
    pub stats: TaskStats,
}

/// An artifact produced by task execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Artifact {
    /// Path to the artifact file.
    pub path: PathBuf,

    /// Content of the artifact.
    pub content: String,
}

/// Statistics from task execution.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TaskStats {
    /// Number of conversation turns.
    pub turns: u32,

    /// Total input tokens used.
    pub input_tokens: u64,

    /// Total output tokens generated.
    pub output_tokens: u64,

    /// Estimated cost in USD.
    pub cost_usd: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_task_kind_dir_names() {
        assert_eq!(TaskKind::Init.dir_name(), "init");
        assert_eq!(TaskKind::Plan.dir_name(), "plan");
        assert_eq!(TaskKind::Execute.dir_name(), "execute");
        assert_eq!(TaskKind::Review.dir_name(), "review");
        assert_eq!(TaskKind::Verification.dir_name(), "verification");
        assert_eq!(
            TaskKind::Custom("my_task".to_string()).dir_name(),
            "my_task"
        );
    }

    #[test]
    fn test_task_kind_display() {
        assert_eq!(format!("{}", TaskKind::Init), "init");
        assert_eq!(
            format!("{}", TaskKind::Custom("custom".to_string())),
            "custom"
        );
    }

    #[test]
    fn test_should_create_task() {
        let task = Task::new(TaskKind::Init, json!({"repo_path": "/tmp/repo"}));

        assert_eq!(task.kind, TaskKind::Init);
        assert_eq!(task.context["repo_path"], "/tmp/repo");
        assert!(task.system_prompt.is_none());
    }

    #[test]
    fn test_should_create_task_with_system_prompt() {
        let task = Task::new(TaskKind::Plan, json!({})).with_system_prompt("Custom system prompt");

        assert_eq!(task.system_prompt, Some("Custom system prompt".to_string()));
    }

    #[test]
    fn test_task_kind_serialization() {
        assert_eq!(serde_json::to_string(&TaskKind::Init).unwrap(), "\"init\"");
        assert_eq!(
            serde_json::to_string(&TaskKind::Custom("test".to_string())).unwrap(),
            "{\"custom\":\"test\"}"
        );
    }

    #[test]
    fn test_task_kind_deserialization() {
        let kind: TaskKind = serde_json::from_str("\"init\"").unwrap();
        assert_eq!(kind, TaskKind::Init);

        let kind: TaskKind = serde_json::from_str("{\"custom\":\"my_task\"}").unwrap();
        assert_eq!(kind, TaskKind::Custom("my_task".to_string()));
    }
}
