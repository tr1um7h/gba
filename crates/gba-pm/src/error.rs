//! Error types for the prompt manager.
//!
//! This module defines error types for template operations including
//! loading, rendering, and I/O operations.

use std::path::PathBuf;

/// Error type for prompt manager operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PromptError {
    /// Template not found in the manager.
    #[error("template not found: {0}")]
    TemplateNotFound(String),

    /// Error rendering a template.
    #[error("render error: {0}")]
    RenderError(#[from] minijinja::Error),

    /// I/O error during template loading.
    #[error("I/O error at '{path}': {source}")]
    IoError {
        /// The path that caused the error.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl PromptError {
    /// Create a new I/O error with path context.
    pub fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::IoError {
            path: path.into(),
            source,
        }
    }
}

/// Result type alias for prompt manager operations.
pub type Result<T> = std::result::Result<T, PromptError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_display_template_not_found_error() {
        let err = PromptError::TemplateNotFound("test_template".to_string());
        assert_eq!(err.to_string(), "template not found: test_template");
    }

    #[test]
    fn test_should_display_io_error_with_path() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = PromptError::io_error("/path/to/template.j2", io_err);
        assert!(err.to_string().contains("/path/to/template.j2"));
        assert!(err.to_string().contains("I/O error"));
    }
}
