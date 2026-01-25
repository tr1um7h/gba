//! Implementation of the `gba clean` command.
//!
//! This module cleans up local worktrees and branches for PRs that have been
//! closed or merged.

use std::path::Path;
use std::process::Command;

use tracing::{debug, info, warn};

use crate::error::CliError;
use crate::utils;

/// Information about a worktree and its associated PR.
#[derive(Debug)]
struct WorktreeInfo {
    /// Path to the worktree.
    path: String,
    /// Branch name.
    branch: String,
    /// Feature slug (extracted from path).
    slug: String,
    /// PR status if found.
    pr_status: Option<PrStatus>,
}

/// PR status from GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PrStatus {
    Open,
    Merged,
    Closed,
}

/// Clean up worktrees for closed/merged PRs.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Git operations fail
pub async fn run_clean(workdir: &Path, dry_run: bool, force: bool) -> Result<(), CliError> {
    // Check initialization
    if !utils::is_initialized(workdir) {
        return Err(CliError::NotInitialized);
    }

    println!("Scanning worktrees for cleanup...");

    // Get list of worktrees
    let worktrees = list_worktrees(workdir)?;

    if worktrees.is_empty() {
        println!("No worktrees found in .trees/");
        return Ok(());
    }

    println!("Found {} worktree(s)", worktrees.len());

    // Check PR status for each worktree
    let mut to_clean: Vec<WorktreeInfo> = Vec::new();
    let mut to_keep: Vec<WorktreeInfo> = Vec::new();

    for mut wt in worktrees {
        // Get PR status
        wt.pr_status = get_pr_status(workdir, &wt.branch)?;

        match &wt.pr_status {
            Some(PrStatus::Merged) => {
                println!("  ✓ {} - PR merged", wt.slug);
                to_clean.push(wt);
            }
            Some(PrStatus::Closed) => {
                println!("  ✗ {} - PR closed (not merged)", wt.slug);
                if force {
                    to_clean.push(wt);
                } else {
                    to_keep.push(wt);
                }
            }
            Some(PrStatus::Open) => {
                println!("  ○ {} - PR still open", wt.slug);
                to_keep.push(wt);
            }
            None => {
                println!("  ? {} - No PR found", wt.slug);
                to_keep.push(wt);
            }
        }
    }

    if to_clean.is_empty() {
        println!();
        println!("No worktrees to clean up.");
        if !to_keep.is_empty() {
            let closed_count = to_keep
                .iter()
                .filter(|w| w.pr_status == Some(PrStatus::Closed))
                .count();
            if closed_count > 0 {
                println!(
                    "Use --force to also clean {} closed (not merged) PR(s).",
                    closed_count
                );
            }
        }
        return Ok(());
    }

    println!();
    if dry_run {
        println!("Dry run - would clean up {} worktree(s):", to_clean.len());
        for wt in &to_clean {
            println!("  - {} ({})", wt.slug, wt.path);
        }
    } else {
        println!("Cleaning up {} worktree(s)...", to_clean.len());

        for wt in &to_clean {
            clean_worktree(workdir, wt)?;
        }

        println!();
        println!("Cleanup complete!");
    }

    Ok(())
}

/// List all worktrees in the .trees directory.
fn list_worktrees(workdir: &Path) -> Result<Vec<WorktreeInfo>, CliError> {
    let trees_dir = utils::trees_dir(workdir);

    if !trees_dir.exists() {
        return Ok(Vec::new());
    }

    let mut worktrees = Vec::new();

    let entries = std::fs::read_dir(&trees_dir)
        .map_err(|e| CliError::Io(format!("failed to read .trees directory: {}", e)))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| CliError::Io(format!("failed to read directory entry: {}", e)))?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let slug = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if slug.is_empty() || slug.starts_with('.') {
            continue;
        }

        // Get the branch for this worktree
        let branch = get_worktree_branch(&path)?;

        if let Some(branch) = branch {
            worktrees.push(WorktreeInfo {
                path: path.display().to_string(),
                branch,
                slug,
                pr_status: None,
            });
        }
    }

    Ok(worktrees)
}

/// Get the branch name for a worktree.
fn get_worktree_branch(worktree_path: &Path) -> Result<Option<String>, CliError> {
    let output = Command::new("git")
        .current_dir(worktree_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| CliError::Git(format!("failed to get branch: {}", e)))?;

    if !output.status.success() {
        debug!(
            "failed to get branch for {}: {}",
            worktree_path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(None);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(Some(branch))
}

/// Get PR status for a branch using gh CLI.
fn get_pr_status(workdir: &Path, branch: &str) -> Result<Option<PrStatus>, CliError> {
    // Use gh pr view to get PR status
    let output = Command::new("gh")
        .current_dir(workdir)
        .args(["pr", "view", branch, "--json", "state", "--jq", ".state"])
        .output()
        .map_err(|e| CliError::Git(format!("failed to run gh pr view: {}", e)))?;

    if !output.status.success() {
        // No PR found for this branch
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no pull requests found")
            || stderr.contains("Could not resolve")
            || stderr.contains("no pull request found")
        {
            debug!("no PR found for branch {}", branch);
            return Ok(None);
        }
        warn!("gh pr view failed for {}: {}", branch, stderr);
        return Ok(None);
    }

    let state = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_uppercase();

    let status = match state.as_str() {
        "OPEN" => PrStatus::Open,
        "MERGED" => PrStatus::Merged,
        "CLOSED" => PrStatus::Closed,
        _ => {
            debug!("unknown PR state for {}: {}", branch, state);
            return Ok(None);
        }
    };

    Ok(Some(status))
}

/// Clean up a worktree and its branch.
fn clean_worktree(workdir: &Path, wt: &WorktreeInfo) -> Result<(), CliError> {
    info!("cleaning up worktree: {} ({})", wt.slug, wt.branch);
    println!("  Removing worktree: {}", wt.slug);

    // Remove the worktree
    let output = Command::new("git")
        .current_dir(workdir)
        .args(["worktree", "remove", "--force", &wt.path])
        .output()
        .map_err(|e| CliError::Git(format!("failed to remove worktree: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("failed to remove worktree {}: {}", wt.slug, stderr);
        // Try to force remove the directory
        if let Err(e) = std::fs::remove_dir_all(&wt.path) {
            warn!("failed to remove directory {}: {}", wt.path, e);
        }
    }

    // Delete the local branch
    println!("  Deleting branch: {}", wt.branch);
    let output = Command::new("git")
        .current_dir(workdir)
        .args(["branch", "-D", &wt.branch])
        .output()
        .map_err(|e| CliError::Git(format!("failed to delete branch: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Branch might already be deleted, that's okay
        if !stderr.contains("not found") {
            warn!("failed to delete branch {}: {}", wt.branch, stderr);
        }
    }

    // Also clean up the .gba feature directory
    let gba_feature_dir = workdir.join(".gba").join(&wt.slug);
    if gba_feature_dir.exists() {
        println!("  Removing .gba/{}/", wt.slug);
        if let Err(e) = std::fs::remove_dir_all(&gba_feature_dir) {
            warn!("failed to remove .gba/{}: {}", wt.slug, e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_status_equality() {
        assert_eq!(PrStatus::Open, PrStatus::Open);
        assert_eq!(PrStatus::Merged, PrStatus::Merged);
        assert_eq!(PrStatus::Closed, PrStatus::Closed);
        assert_ne!(PrStatus::Open, PrStatus::Closed);
    }
}
