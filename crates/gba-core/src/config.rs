//! Configuration types for the GBA core engine.
//!
//! This module defines configuration types for tasks and the engine,
//! including task-specific settings loaded from `config.yml` files.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use gba_pm::PromptManager;

/// Task configuration loaded from `tasks/<kind>/config.yml`.
///
/// This configuration determines how the Claude agent is configured
/// for a specific task type.
///
/// # Example YAML
///
/// ```yaml
/// preset: true
/// tools: []
/// disallowedTools:
///   - Write
///   - Edit
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TaskConfig {
    /// Whether to use the `claude_code` preset for the system prompt.
    ///
    /// When `true`, the system prompt from `system.j2` is appended to
    /// the preset. When `false`, the system prompt is used directly.
    #[serde(default)]
    pub preset: bool,

    /// List of allowed tools.
    ///
    /// An empty list means all tools are allowed.
    #[serde(default)]
    pub tools: Vec<String>,

    /// List of disallowed tools.
    ///
    /// An empty list means no tools are explicitly disallowed.
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
}

/// Engine configuration for creating an [`Engine`](crate::Engine) instance.
///
/// This configuration specifies the working directory, prompt manager,
/// and optional Claude agent options.
#[derive(TypedBuilder)]
#[builder(doc)]
pub struct EngineConfig<'a> {
    /// Working directory for the engine.
    ///
    /// This is typically the root of the repository where GBA is running.
    #[builder(setter(into))]
    pub workdir: PathBuf,

    /// Prompt manager instance containing loaded templates.
    pub prompts: PromptManager<'a>,

    /// Optional Claude agent options to merge with task-specific options.
    #[builder(default)]
    pub agent_options: Option<claude_agent_sdk_rs::ClaudeAgentOptions>,
}

impl std::fmt::Debug for EngineConfig<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig")
            .field("workdir", &self.workdir)
            .field("prompts", &self.prompts)
            .field("agent_options", &"<ClaudeAgentOptions>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_deserialize_task_config_with_defaults() {
        let yaml = "preset: true";
        let config: TaskConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.preset);
        assert!(config.tools.is_empty());
        assert!(config.disallowed_tools.is_empty());
    }

    #[test]
    fn test_should_deserialize_task_config_with_tools() {
        let yaml = r#"
preset: true
tools:
  - Read
  - Write
disallowedTools:
  - Bash
"#;
        let config: TaskConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.preset);
        assert_eq!(config.tools, vec!["Read", "Write"]);
        assert_eq!(config.disallowed_tools, vec!["Bash"]);
    }

    #[test]
    fn test_should_deserialize_review_task_config() {
        let yaml = r#"
preset: true
tools: []
disallowedTools:
  - Write
  - Edit
  - NotebookEdit
"#;
        let config: TaskConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.preset);
        assert!(config.tools.is_empty());
        assert_eq!(
            config.disallowed_tools,
            vec!["Write", "Edit", "NotebookEdit"]
        );
    }

    #[test]
    fn test_should_build_engine_config() {
        let prompts = PromptManager::new();
        let config = EngineConfig::builder()
            .workdir("/tmp/test")
            .prompts(prompts)
            .build();

        assert_eq!(config.workdir, PathBuf::from("/tmp/test"));
        assert!(config.agent_options.is_none());
    }
}
