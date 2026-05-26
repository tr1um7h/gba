//! Implementation of the `gba recover` command.
//!
//! This module rolls back state.yml to allow resuming a failed `run`
//! from the failure point, without performing any git operations.

use std::path::Path;

use anyhow::Result;

use crate::error::CliError;

/// Recover a failed feature for resumption.
///
/// Rolls back state.yml to allow resuming a failed `run` from the
/// failure point. No git operations are performed.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Feature does not exist
/// - Feature status is not `Failed`
/// - Worktree does not exist
pub async fn run_recover(_workdir: &Path, _slug: &str) -> Result<(), CliError> {
    // TODO: implement in phase 2 (implement-recover-logic)
    Err(CliError::InvalidState(
        "recover command is not yet implemented".to_string(),
    ))
}
