//! Error types for the GBA core engine.
//!
//! This module defines error types for engine operations including
//! configuration, prompt rendering, agent execution, and I/O operations.

use std::path::PathBuf;

/// Error type for engine operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// Configuration error.
    #[error("configuration error: {0}")]
    ConfigError(String),

    /// Error from the prompt manager.
    #[error("prompt error: {0}")]
    PromptError(#[from] gba_pm::PromptError),

    /// Error from the Claude agent SDK.
    #[error("agent error: {0}")]
    AgentError(#[from] claude_agent_sdk_rs::ClaudeError),

    /// I/O error during file operations.
    #[error("I/O error at '{path}': {source}")]
    IoError {
        /// The path that caused the error.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// YAML parsing error.
    #[error("YAML parse error at '{path}': {source}")]
    YamlError {
        /// The path that caused the error.
        path: PathBuf,
        /// The underlying YAML error.
        #[source]
        source: serde_yaml::Error,
    },

    /// Task configuration not found.
    #[error("task configuration not found for task kind: {0}")]
    TaskConfigNotFound(String),

    /// Git operation error.
    #[error("git error: {0}")]
    GitError(String),

    /// GitHub CLI operation error.
    #[error("github cli error: {0}")]
    GitHubError(String),
}

impl EngineError {
    /// Create a new I/O error with path context.
    pub fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::IoError {
            path: path.into(),
            source,
        }
    }

    /// Create a new YAML error with path context.
    pub fn yaml_error(path: impl Into<PathBuf>, source: serde_yaml::Error) -> Self {
        Self::YamlError {
            path: path.into(),
            source,
        }
    }

    /// Create a new configuration error.
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError(message.into())
    }

    /// Create a new Git error.
    pub fn git_error(message: impl Into<String>) -> Self {
        Self::GitError(message.into())
    }

    /// Create a new GitHub CLI error.
    pub fn github_error(message: impl Into<String>) -> Self {
        Self::GitHubError(message.into())
    }
}

/// Result type alias for engine operations.
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_display_config_error() {
        let err = EngineError::config_error("invalid model");
        assert_eq!(err.to_string(), "configuration error: invalid model");
    }

    #[test]
    fn test_should_display_io_error_with_path() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = EngineError::io_error("/path/to/config.yml", io_err);
        assert!(err.to_string().contains("/path/to/config.yml"));
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn test_should_display_task_config_not_found() {
        let err = EngineError::TaskConfigNotFound("custom_task".to_string());
        assert!(err.to_string().contains("custom_task"));
    }

    #[test]
    fn test_should_display_git_error() {
        let err = EngineError::git_error("failed to get branch");
        assert_eq!(err.to_string(), "git error: failed to get branch");
    }

    #[test]
    fn test_should_display_github_error() {
        let err = EngineError::github_error("failed to get PR status");
        assert_eq!(err.to_string(), "github cli error: failed to get PR status");
    }
}
