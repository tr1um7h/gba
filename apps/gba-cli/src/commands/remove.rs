//! Implementation of the `gba remove` command.
//!
//! This module removes a feature and cleans up all associated resources
//! (worktree, branch, specs, state).

use std::path::Path;

use anyhow::Result;
use tracing::info;

/// Remove a feature and clean up all resources.
///
/// Removes the feature's worktree, branch, specs, and state.
/// Prompts for confirmation when the feature is in-progress or has
/// uncommitted changes, unless `force` is `true`.
///
/// # Errors
///
/// Returns an error if the feature does not exist or GBA is not initialized.
pub async fn run_remove(_workdir: &Path, _slug: &str, _force: bool) -> Result<()> {
    // TODO: implement in phase 2
    info!("remove command called");
    Ok(())
}
