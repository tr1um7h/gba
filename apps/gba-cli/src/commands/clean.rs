//! Implementation of the `gba clean` command.
//!
//! This module cleans up local worktrees and branches for PRs that have been
//! closed or merged.

use std::path::Path;
use std::process::{Command, Stdio};

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
    let worktrees_with_status: Vec<WorktreeInfo> = worktrees
        .into_iter()
        .map(|mut wt| {
            wt.pr_status = get_pr_status(workdir, &wt.branch).unwrap_or(None);
            // Print status
            match &wt.pr_status {
                Some(PrStatus::Merged) => println!("  ✓ {} - PR merged", wt.slug),
                Some(PrStatus::Closed) => println!("  ✗ {} - PR closed (not merged)", wt.slug),
                Some(PrStatus::Open) => println!("  ○ {} - PR still open", wt.slug),
                None => println!("  ? {} - No PR found", wt.slug),
            }
            wt
        })
        .collect();

    // Classify worktrees
    let (to_clean, to_keep) = classify_worktrees(worktrees_with_status, force);

    if to_clean.is_empty() {
        println!();
        println!("No worktrees to clean up.");
        let closed_count = count_closed_prs(&to_keep);
        if closed_count > 0 {
            println!(
                "Use --force to also clean {} closed (not merged) PR(s).",
                closed_count
            );
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
    // Suppress stderr to prevent progress indicators from messing up console output
    let output = Command::new("gh")
        .current_dir(workdir)
        .args(["pr", "view", branch, "--json", "state", "--jq", ".state"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
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

    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(parse_pr_state(&state))
}

/// Parse PR state from gh CLI output.
///
/// The gh CLI returns states like "OPEN", "MERGED", "CLOSED".
/// This function normalizes the input and returns the appropriate status.
fn parse_pr_state(state: &str) -> Option<PrStatus> {
    match state.trim().to_uppercase().as_str() {
        "OPEN" => Some(PrStatus::Open),
        "MERGED" => Some(PrStatus::Merged),
        "CLOSED" => Some(PrStatus::Closed),
        _ => None,
    }
}

/// Determine if a worktree should be cleaned based on its PR status.
///
/// Returns true if:
/// - PR is merged (always clean)
/// - PR is closed AND force flag is set
fn should_clean(status: Option<&PrStatus>, force: bool) -> bool {
    match status {
        Some(PrStatus::Merged) => true,
        Some(PrStatus::Closed) => force,
        Some(PrStatus::Open) => false,
        None => false,
    }
}

/// Classify worktrees into those to clean and those to keep.
///
/// Returns (to_clean, to_keep) tuple.
fn classify_worktrees(
    worktrees: Vec<WorktreeInfo>,
    force: bool,
) -> (Vec<WorktreeInfo>, Vec<WorktreeInfo>) {
    let mut to_clean = Vec::new();
    let mut to_keep = Vec::new();

    for wt in worktrees {
        if should_clean(wt.pr_status.as_ref(), force) {
            to_clean.push(wt);
        } else {
            to_keep.push(wt);
        }
    }

    (to_clean, to_keep)
}

/// Count worktrees with closed (not merged) PRs.
fn count_closed_prs(worktrees: &[WorktreeInfo]) -> usize {
    worktrees
        .iter()
        .filter(|w| w.pr_status == Some(PrStatus::Closed))
        .count()
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

    // Note: We keep .gba/<feature>/ directory for feature history
    info!("keeping .gba/{}/ for feature history", wt.slug);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_worktree(slug: &str, branch: &str, status: Option<PrStatus>) -> WorktreeInfo {
        WorktreeInfo {
            path: format!(".trees/{}", slug),
            branch: branch.to_string(),
            slug: slug.to_string(),
            pr_status: status,
        }
    }

    // Tests for parse_pr_state
    #[test]
    fn test_parse_pr_state_open() {
        assert_eq!(parse_pr_state("OPEN"), Some(PrStatus::Open));
        assert_eq!(parse_pr_state("open"), Some(PrStatus::Open));
        assert_eq!(parse_pr_state("  OPEN  "), Some(PrStatus::Open));
    }

    #[test]
    fn test_parse_pr_state_merged() {
        assert_eq!(parse_pr_state("MERGED"), Some(PrStatus::Merged));
        assert_eq!(parse_pr_state("merged"), Some(PrStatus::Merged));
    }

    #[test]
    fn test_parse_pr_state_closed() {
        assert_eq!(parse_pr_state("CLOSED"), Some(PrStatus::Closed));
        assert_eq!(parse_pr_state("closed"), Some(PrStatus::Closed));
    }

    #[test]
    fn test_parse_pr_state_unknown() {
        assert_eq!(parse_pr_state("UNKNOWN"), None);
        assert_eq!(parse_pr_state(""), None);
        assert_eq!(parse_pr_state("draft"), None);
    }

    // Tests for should_clean
    #[test]
    fn test_should_clean_merged_always() {
        assert!(should_clean(Some(&PrStatus::Merged), false));
        assert!(should_clean(Some(&PrStatus::Merged), true));
    }

    #[test]
    fn test_should_clean_closed_only_with_force() {
        assert!(!should_clean(Some(&PrStatus::Closed), false));
        assert!(should_clean(Some(&PrStatus::Closed), true));
    }

    #[test]
    fn test_should_clean_open_never() {
        assert!(!should_clean(Some(&PrStatus::Open), false));
        assert!(!should_clean(Some(&PrStatus::Open), true));
    }

    #[test]
    fn test_should_clean_none_never() {
        assert!(!should_clean(None, false));
        assert!(!should_clean(None, true));
    }

    // Tests for classify_worktrees
    #[test]
    fn test_classify_worktrees_empty() {
        let (to_clean, to_keep) = classify_worktrees(vec![], false);
        assert!(to_clean.is_empty());
        assert!(to_keep.is_empty());
    }

    #[test]
    fn test_classify_worktrees_all_merged() {
        let worktrees = vec![
            make_worktree("feat-a", "feature/001-feat-a", Some(PrStatus::Merged)),
            make_worktree("feat-b", "feature/002-feat-b", Some(PrStatus::Merged)),
        ];

        let (to_clean, to_keep) = classify_worktrees(worktrees, false);
        assert_eq!(to_clean.len(), 2);
        assert!(to_keep.is_empty());
    }

    #[test]
    fn test_classify_worktrees_mixed_without_force() {
        let worktrees = vec![
            make_worktree("merged", "feature/001-merged", Some(PrStatus::Merged)),
            make_worktree("closed", "feature/002-closed", Some(PrStatus::Closed)),
            make_worktree("open", "feature/003-open", Some(PrStatus::Open)),
            make_worktree("no-pr", "feature/004-no-pr", None),
        ];

        let (to_clean, to_keep) = classify_worktrees(worktrees, false);

        assert_eq!(to_clean.len(), 1);
        assert_eq!(to_clean[0].slug, "merged");

        assert_eq!(to_keep.len(), 3);
        let keep_slugs: Vec<&str> = to_keep.iter().map(|w| w.slug.as_str()).collect();
        assert!(keep_slugs.contains(&"closed"));
        assert!(keep_slugs.contains(&"open"));
        assert!(keep_slugs.contains(&"no-pr"));
    }

    #[test]
    fn test_classify_worktrees_mixed_with_force() {
        let worktrees = vec![
            make_worktree("merged", "feature/001-merged", Some(PrStatus::Merged)),
            make_worktree("closed", "feature/002-closed", Some(PrStatus::Closed)),
            make_worktree("open", "feature/003-open", Some(PrStatus::Open)),
            make_worktree("no-pr", "feature/004-no-pr", None),
        ];

        let (to_clean, to_keep) = classify_worktrees(worktrees, true);

        assert_eq!(to_clean.len(), 2);
        let clean_slugs: Vec<&str> = to_clean.iter().map(|w| w.slug.as_str()).collect();
        assert!(clean_slugs.contains(&"merged"));
        assert!(clean_slugs.contains(&"closed"));

        assert_eq!(to_keep.len(), 2);
        let keep_slugs: Vec<&str> = to_keep.iter().map(|w| w.slug.as_str()).collect();
        assert!(keep_slugs.contains(&"open"));
        assert!(keep_slugs.contains(&"no-pr"));
    }

    // Tests for count_closed_prs
    #[test]
    fn test_count_closed_prs_none() {
        let worktrees = vec![
            make_worktree("open", "feature/001-open", Some(PrStatus::Open)),
            make_worktree("no-pr", "feature/002-no-pr", None),
        ];
        assert_eq!(count_closed_prs(&worktrees), 0);
    }

    #[test]
    fn test_count_closed_prs_some() {
        let worktrees = vec![
            make_worktree("closed1", "feature/001-closed1", Some(PrStatus::Closed)),
            make_worktree("open", "feature/002-open", Some(PrStatus::Open)),
            make_worktree("closed2", "feature/003-closed2", Some(PrStatus::Closed)),
            make_worktree("merged", "feature/004-merged", Some(PrStatus::Merged)),
        ];
        assert_eq!(count_closed_prs(&worktrees), 2);
    }
}
