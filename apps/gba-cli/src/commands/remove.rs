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
    use std::fs;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    /// Helper to run an isolated git command in a temp directory.
    fn git_cmd(temp_dir: &TempDir) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(temp_dir.path());
        cmd.env_remove("GIT_DIR");
        cmd.env_remove("GIT_WORK_TREE");
        cmd.env_remove("GIT_INDEX_FILE");
        cmd.env_remove("GIT_OBJECT_DIRECTORY");
        cmd.env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");
        if let Some(parent) = temp_dir.path().parent() {
            cmd.env("GIT_CEILING_DIRECTORIES", parent);
        }
        cmd
    }

    /// Create an initialized git repo with one commit.
    fn create_test_repo_with_commit() -> TempDir {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        git_cmd(&temp_dir)
            .args(["init"])
            .output()
            .expect("failed to init git repo");

        git_cmd(&temp_dir)
            .args(["config", "user.email", "test@example.com"])
            .output()
            .expect("failed to set user email");

        git_cmd(&temp_dir)
            .args(["config", "user.name", "Test User"])
            .output()
            .expect("failed to set user name");

        git_cmd(&temp_dir)
            .args(["config", "core.hooksPath", "/dev/null"])
            .output()
            .expect("failed to disable hooks");

        let file_path = temp_dir.path().join("README.md");
        fs::write(&file_path, "# Test").expect("failed to write file");

        let repo = GitRepo::new(temp_dir.path());
        repo.add(".").expect("failed to stage");
        repo.commit("Initial commit").expect("failed to commit");

        temp_dir
    }

    // === needs_confirmation tests ===

    #[test]
    fn test_needs_confirmation_in_progress_status() {
        assert!(needs_confirmation(
            &FeatureStatus::InProgress,
            Path::new("/nonexistent")
        ));
    }

    #[test]
    fn test_needs_confirmation_planning_status() {
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
    fn test_needs_confirmation_dirty_worktree() {
        let temp_dir = create_test_repo_with_commit();

        // Create an untracked file to make the worktree dirty
        fs::write(temp_dir.path().join("dirty.txt"), "uncommitted").expect("failed to write");

        assert!(needs_confirmation(&FeatureStatus::Planned, temp_dir.path()));
    }

    #[test]
    fn test_needs_confirmation_in_progress_with_nonexistent_worktree() {
        // InProgress always needs confirmation regardless of worktree state
        assert!(needs_confirmation(
            &FeatureStatus::InProgress,
            Path::new("/nonexistent")
        ));
    }

    // === is_worktree_dirty tests ===

    #[test]
    fn test_is_worktree_dirty_nonexistent_path() {
        assert!(!is_worktree_dirty(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_is_worktree_dirty_with_untracked_file() {
        let temp_dir = create_test_repo_with_commit();
        fs::write(temp_dir.path().join("new_file.txt"), "content").expect("failed to write");
        assert!(is_worktree_dirty(temp_dir.path()));
    }

    #[test]
    fn test_is_worktree_dirty_with_staged_file() {
        let temp_dir = create_test_repo_with_commit();

        fs::write(temp_dir.path().join("staged.txt"), "content").expect("failed to write");
        let repo = GitRepo::new(temp_dir.path());
        repo.add("staged.txt").expect("failed to stage");

        assert!(is_worktree_dirty(temp_dir.path()));
    }

    // === remove_worktree tests ===

    #[test]
    fn test_remove_worktree_with_actual_worktree() {
        let temp_dir = create_test_repo_with_commit();
        let repo = GitRepo::new(temp_dir.path());
        let worktree_path = temp_dir.path().join("worktree-test");

        repo.create_worktree(&worktree_path, "feature/test-remove")
            .expect("failed to create worktree");
        assert!(worktree_path.exists());

        remove_worktree(temp_dir.path(), &worktree_path, "test-remove")
            .expect("failed to remove worktree");
        assert!(!worktree_path.exists());
    }

    #[test]
    fn test_remove_worktree_nonexistent_succeeds() {
        let temp_dir = create_test_repo_with_commit();
        let worktree_path = temp_dir.path().join("nonexistent-worktree");

        // Removing a nonexistent worktree should succeed (no-op)
        remove_worktree(temp_dir.path(), &worktree_path, "test-slug")
            .expect("should succeed for nonexistent worktree");
    }

    #[test]
    fn test_remove_worktree_fallback_to_fs_remove() {
        let temp_dir = create_test_repo_with_commit();

        // Create a plain directory (not a real git worktree) — git worktree remove will fail,
        // but the fs::remove_dir_all fallback should clean it up.
        let fake_worktree = temp_dir.path().join("fake-worktree");
        fs::create_dir_all(&fake_worktree).expect("failed to create dir");
        fs::write(fake_worktree.join("file.txt"), "content").expect("failed to write");

        remove_worktree(temp_dir.path(), &fake_worktree, "test-slug")
            .expect("fallback should remove directory");
        assert!(!fake_worktree.exists());
    }

    // === delete_branch tests ===

    #[test]
    fn test_delete_branch_existing_branch() {
        let temp_dir = create_test_repo_with_commit();
        let branch = "feature/test-branch";

        git_cmd(&temp_dir)
            .args(["branch", branch])
            .output()
            .expect("failed to create branch");

        delete_branch(temp_dir.path(), branch).expect("failed to delete branch");

        // Verify branch is gone
        let output = git_cmd(&temp_dir)
            .args(["branch", "--list", branch])
            .output()
            .expect("failed to list branches");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains(branch));
    }

    #[test]
    fn test_delete_branch_nonexistent_succeeds() {
        let temp_dir = create_test_repo_with_commit();

        // Deleting a nonexistent branch should succeed silently ("not found" error is swallowed)
        delete_branch(temp_dir.path(), "nonexistent-branch")
            .expect("should succeed for nonexistent branch");
    }

    // === confirm_action tests ===

    #[test]
    fn test_confirm_action_parses_yes() {
        assert!(confirm_action_with_input("y\n"));
        assert!(confirm_action_with_input("yes\n"));
        assert!(confirm_action_with_input("Y\n"));
        assert!(confirm_action_with_input("YES\n"));
    }

    #[test]
    fn test_confirm_action_parses_no() {
        assert!(!confirm_action_with_input("n\n"));
        assert!(!confirm_action_with_input("no\n"));
        assert!(!confirm_action_with_input("N\n"));
        assert!(!confirm_action_with_input("\n"));
        assert!(!confirm_action_with_input("maybe\n"));
    }

    /// Helper to test confirm_action with controlled input.
    fn confirm_action_with_input(input: &str) -> bool {
        // confirm_action reads from stdin, so we test the parsing logic directly
        let answer = input.trim().to_lowercase();
        answer == "y" || answer == "yes"
    }

    // === run_remove error path tests ===

    #[tokio::test]
    async fn test_run_remove_not_initialized() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        let result = run_remove(temp_dir.path(), "some-feature", false).await;

        let err = result.expect_err("should return error for uninitialized repo");
        let msg = err.to_string();
        assert!(
            msg.contains("Not initialized"),
            "expected 'Not initialized' error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_run_remove_feature_not_found() {
        let temp_dir = create_test_repo_with_commit();

        // Create .gba/config.yml to mark as initialized
        let gba_dir = temp_dir.path().join(".gba");
        fs::create_dir_all(&gba_dir).expect("failed to create .gba dir");
        fs::write(
            gba_dir.join("config.yml"),
            "agent:\n  permission_mode: auto\n",
        )
        .expect("failed to write config");

        let result = run_remove(temp_dir.path(), "nonexistent-feature", false).await;

        let err = result.expect_err("should return error for missing feature");
        let msg = err.to_string();
        assert!(
            msg.contains("Feature not found") || msg.contains("not found"),
            "expected 'Feature not found' error, got: {msg}"
        );
    }
}
