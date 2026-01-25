//! CLI error types for GBA.
//!
//! This module defines custom error types for CLI operations,
//! providing detailed context for different failure modes.

use thiserror::Error;

/// CLI operation errors.
#[derive(Debug, Error)]
pub enum CliError {
    /// GBA is not initialized in this repository.
    #[error("Not initialized. Please run `gba init` first.")]
    NotInitialized,

    /// GBA is already initialized in this repository.
    #[error("Already initialized. `.gba/` directory exists.")]
    AlreadyInitialized,

    /// Feature not found.
    #[error("Feature not found: {0}")]
    FeatureNotFound(String),

    /// Feature already exists.
    #[error("Feature already exists: {0}")]
    FeatureExists(String),

    /// Invalid state.
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// State management error.
    #[error("State error: {0}")]
    State(String),

    /// Git operation error.
    #[error("Git error: {0}")]
    Git(String),

    /// Prompt error.
    #[error("Prompt error: {0}")]
    Prompt(#[from] gba_pm::PromptError),

    /// Engine error (non-git).
    ///
    /// Note: `EngineError::GitError` and `EngineError::GitHubError` are mapped
    /// to `CliError::Git` via the manual `From` implementation below.
    #[error("Engine error: {0}")]
    Engine(gba_core::EngineError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(String),
}

impl From<gba_core::EngineError> for CliError {
    fn from(err: gba_core::EngineError) -> Self {
        match err {
            gba_core::EngineError::GitError(msg) => CliError::Git(msg),
            gba_core::EngineError::GitHubError(msg) => CliError::Git(msg),
            other => CliError::Engine(other),
        }
    }
}
