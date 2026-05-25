//! Implementation of the `gba remove` command.
//!
//! This module removes a feature and cleans up all associated resources
//! (worktree, branch, specs, state).

use std::io::{BufRead, Write};
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use gba_core::git::GitRepo;
use tracing::{info, warn};

use crate::error::CliError;
use crate::state::FeatureStatus;
use crate::utils;

/// Remove a feature and clean up all resources.
///
/// Removes the feature's worktree, branch, specs, and state.
/// Prompts for confirmation when the feature is in-progress or has
/// uncommitted changes, unless `force` is `true`.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Feature does not exist
/// - Git operations fail (worktree/branch removal)
pub async fn run_remove(workdir: &Path, slug: &str, force: bool) -> Result<()> {
    // Check initialization
    if !utils::is_initialized(workdir) {
        return Err(CliError::NotInitialized.into());
    }

    // Load feature state
    let state = utils::load_feature_state(workdir, slug)?;

    let worktree_path = utils::feature_worktree_path(workdir, slug);
    let branch = &state.git.branch;

    // Check if confirmation is needed
    let needs_confirm = !force && needs_confirmation(&state.status, &worktree_path);

    if needs_confirm {
        print_confirmation_prompt(&state.status, &worktree_path, slug);
        if !confirm_action("Are you sure you want to remove this feature? (y/N): ")? {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!("Removing feature '{slug}'...");

    // Remove the worktree
    remove_worktree(workdir, &worktree_path, slug)?;

    // Delete the branch
    delete_branch(workdir, branch)?;

    println!("Feature '{slug}' removed successfully.");
    info!("feature '{}' removed (branch: {})", slug, branch);

    Ok(())
}

/// Determine whether confirmation is required before removing.
///
/// Confirmation is needed for:
/// - **InProgress** — the feature might still be executing
/// - **Dirty worktree** — local code changes will be lost
///
/// Not needed for Planning, Planned, Failed, or Completed statuses.
fn needs_confirmation(status: &FeatureStatus, worktree_path: &Path) -> bool {
    if matches!(status, FeatureStatus::InProgress) {
        return true;
    }

    // Check for uncommitted changes in the worktree
    is_worktree_dirty(worktree_path)
}

/// Check if a worktree has uncommitted changes.
fn is_worktree_dirty(worktree_path: &Path) -> bool {
    if !worktree_path.exists() {
        return false;
    }

    let output = match Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };

    !output.stdout.is_empty()
}

/// Print a context-aware confirmation prompt.
fn print_confirmation_prompt(status: &FeatureStatus, worktree_path: &Path, slug: &str) {
    println!();

    if matches!(status, FeatureStatus::InProgress) {
        println!("WARNING: Feature '{}' is currently in progress.", slug);
        println!("         It may still be executing. Removing it could interrupt active work.");
    }

    if is_worktree_dirty(worktree_path) {
        println!(
            "WARNING: Feature '{}' has uncommitted changes that will be lost.",
            slug
        );
    }

    println!();
}

/// Prompt the user for a yes/no confirmation.
fn confirm_action(prompt: &str) -> Result<bool, CliError> {
    print!("{prompt}");
    std::io::stdout()
        .flush()
        .map_err(|e| CliError::Io(format!("failed to flush stdout: {e}")))?;

    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|e| CliError::Io(format!("failed to read line: {e}")))?;

    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Remove the git worktree, falling back to `fs::remove_dir_all`.
fn remove_worktree(workdir: &Path, worktree_path: &Path, slug: &str) -> Result<(), CliError> {
    let path_str = worktree_path.display().to_string();
    let repo = GitRepo::new(workdir);

    info!("removing worktree: {}", path_str);

    if let Err(e) = repo.remove_worktree(&path_str, true) {
        warn!("git worktree remove failed for '{}': {}", slug, e);
        // Fallback: force-remove the directory
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).map_err(|fs_err| {
                CliError::Io(format!(
                    "failed to remove worktree directory '{}': {}",
                    worktree_path.display(),
                    fs_err
                ))
            })?;
        }
    }

    Ok(())
}

/// Delete the feature branch.
fn delete_branch(workdir: &Path, branch: &str) -> Result<(), CliError> {
    info!("deleting branch: {}", branch);

    let repo = GitRepo::new(workdir);
    if let Err(e) = repo.delete_branch(branch, true) {
        let error_msg = e.to_string();
        if !error_msg.contains("not found") {
            warn!("failed to delete branch '{}': {}", branch, e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_confirmation_in_progress_status() {
        assert!(needs_confirmation(
            &FeatureStatus::InProgress,
            Path::new("/nonexistent")
        ));
    }

    #[test]
    fn test_needs_confirmation_planning_status() {
        // Planning status with nonexistent worktree — no dirty changes
        assert!(!needs_confirmation(
            &FeatureStatus::Planning,
            Path::new("/nonexistent")
        ));
    }

    #[test]
    fn test_needs_confirmation_planned_status() {
        assert!(!needs_confirmation(
            &FeatureStatus::Planned,
            Path::new("/nonexistent")
        ));
    }

    #[test]
    fn test_needs_confirmation_completed_status() {
        assert!(!needs_confirmation(
            &FeatureStatus::Completed,
            Path::new("/nonexistent")
        ));
    }

    #[test]
    fn test_needs_confirmation_failed_status() {
        assert!(!needs_confirmation(
            &FeatureStatus::Failed,
            Path::new("/nonexistent")
        ));
    }

    #[test]
    fn test_is_worktree_dirty_nonexistent_path() {
        assert!(!is_worktree_dirty(Path::new("/nonexistent/path")));
    }
}
