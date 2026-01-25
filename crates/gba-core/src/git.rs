//! Centralized Git and GitHub CLI operations.
//!
//! This module provides a clean API for git and GitHub CLI operations,
//! centralizing the scattered `std::process::Command` calls from across
//! the codebase.
//!
//! # Overview
//!
//! The module provides two main types:
//!
//! - [`GitRepo`] - Wrapper for git repository operations (branch, worktree, commit)
//! - [`GitHub`] - Wrapper for GitHub CLI operations (PR status)
//!
//! # Example
//!
//! ```no_run
//! use gba_core::git::{GitRepo, GitHub, PrStatus};
//!
//! let repo = GitRepo::new("/path/to/repo");
//!
//! // Get current branch
//! let branch = repo.current_branch().unwrap();
//!
//! // Stage and commit
//! repo.add(".").unwrap();
//! repo.commit("feat: add new feature").unwrap();
//! repo.push().unwrap();
//!
//! // Check PR status
//! let gh = GitHub::new("/path/to/repo");
//! if let Some(status) = gh.pr_status(&branch).unwrap() {
//!     match status {
//!         PrStatus::Open => println!("PR is open"),
//!         PrStatus::Merged => println!("PR was merged"),
//!         PrStatus::Closed => println!("PR was closed"),
//!     }
//! }
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{EngineError, Result};

/// PR status from GitHub CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrStatus {
    /// PR is open.
    Open,
    /// PR has been merged.
    Merged,
    /// PR has been closed without merging.
    Closed,
}

/// Git repository operations wrapper.
///
/// This struct provides a clean API for common git operations,
/// executing them via `std::process::Command` internally.
#[derive(Debug)]
pub struct GitRepo {
    workdir: PathBuf,
}

impl GitRepo {
    /// Create a new `GitRepo` for the given working directory.
    ///
    /// # Arguments
    ///
    /// * `workdir` - Path to the git repository working directory
    ///
    /// # Example
    ///
    /// ```
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new("/path/to/repo");
    /// ```
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            workdir: workdir.into(),
        }
    }

    /// Get the working directory path.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    // === Branch Operations ===

    /// Get the current branch name.
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails or if HEAD is detached.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// let branch = repo.current_branch()?;
    /// println!("Current branch: {}", branch);
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn current_branch(&self) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!(
                "failed to get current branch: {stderr}"
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Delete a local branch.
    ///
    /// # Arguments
    ///
    /// * `name` - The branch name to delete
    /// * `force` - If true, use `-D` instead of `-d` for force deletion
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// repo.delete_branch("feature/old-branch", false)?;
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        let flag = if force { "-D" } else { "-d" };
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["branch", flag, name])
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!(
                "failed to delete branch '{name}': {stderr}"
            )));
        }

        Ok(())
    }

    /// Detect the default branch (main/master) of the repository.
    ///
    /// Queries the remote origin to determine the default branch.
    /// Falls back to "main" if detection fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// let default = repo.detect_default_branch();
    /// println!("Default branch: {}", default);
    /// ```
    pub fn detect_default_branch(&self) -> String {
        // Try to detect from remote
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["remote", "show", "origin"])
            .output();

        if let Ok(output) = output
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("HEAD branch:")
                    && let Some(branch) = line.split(':').nth(1)
                {
                    return branch.trim().to_string();
                }
            }
        }

        // Fallback: check if main or master exists
        let branches = Command::new("git")
            .current_dir(&self.workdir)
            .args(["branch", "--list", "main", "master"])
            .output();

        if let Ok(output) = branches {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("main") {
                return "main".to_string();
            }
            if stdout.contains("master") {
                return "master".to_string();
            }
        }

        // Default fallback
        "main".to_string()
    }

    // === Worktree Operations ===

    /// Create a new worktree with a new branch.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the worktree should be created
    /// * `branch` - Name of the new branch to create
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// repo.create_worktree(Path::new(".trees/feature"), "feature/new-feature")?;
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn create_worktree(&self, path: &Path, branch: &str) -> Result<()> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["worktree", "add", "-b", branch])
            .arg(path)
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!(
                "failed to create worktree at '{}': {stderr}",
                path.display()
            )));
        }

        Ok(())
    }

    /// Remove a worktree.
    ///
    /// # Arguments
    ///
    /// * `path` - Path of the worktree to remove
    /// * `force` - If true, force removal even with uncommitted changes
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// repo.remove_worktree(".trees/feature", false)?;
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn remove_worktree(&self, path: &str, force: bool) -> Result<()> {
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(path);

        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(&args)
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!(
                "failed to remove worktree '{path}': {stderr}"
            )));
        }

        Ok(())
    }

    // === Commit Operations ===

    /// Get the short SHA of HEAD.
    ///
    /// Returns `None` if the repository has no commits yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails for reasons other than
    /// an empty repository.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// if let Some(sha) = repo.head_short_sha()? {
    ///     println!("Current commit: {}", sha);
    /// }
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn head_short_sha(&self) -> Result<Option<String>> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if this is just an empty repo (no commits)
            if stderr.contains("unknown revision")
                || stderr.contains("bad revision")
                || stderr.contains("Needed a single revision")
            {
                return Ok(None);
            }
            return Err(EngineError::git_error(format!(
                "failed to get HEAD SHA: {stderr}"
            )));
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sha.is_empty() {
            Ok(None)
        } else {
            Ok(Some(sha))
        }
    }

    /// Stage a file or path pattern.
    ///
    /// # Arguments
    ///
    /// * `path` - The file or path pattern to stage (e.g., ".", "src/", "*.rs")
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// repo.add("src/main.rs")?;
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn add(&self, path: &str) -> Result<()> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["add", path])
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!(
                "failed to stage '{path}': {stderr}"
            )));
        }

        Ok(())
    }

    /// Create a commit with the given message.
    ///
    /// # Arguments
    ///
    /// * `message` - The commit message
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// repo.add(".")?;
    /// repo.commit("feat: add new feature")?;
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn commit(&self, message: &str) -> Result<()> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["commit", "-m", message])
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!(
                "failed to commit: {stderr}"
            )));
        }

        Ok(())
    }

    /// Push to origin.
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::GitRepo;
    ///
    /// let repo = GitRepo::new(".");
    /// repo.push()?;
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn push(&self) -> Result<()> {
        let output = Command::new("git")
            .current_dir(&self.workdir)
            .args(["push"])
            .output()
            .map_err(|e| EngineError::git_error(format!("failed to execute git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::git_error(format!("failed to push: {stderr}")));
        }

        Ok(())
    }
}

/// GitHub CLI operations wrapper.
///
/// This struct provides a clean API for GitHub CLI operations.
#[derive(Debug)]
pub struct GitHub {
    workdir: PathBuf,
}

impl GitHub {
    /// Create a new GitHub CLI wrapper for the given working directory.
    ///
    /// # Arguments
    ///
    /// * `workdir` - Path to the git repository working directory
    ///
    /// # Example
    ///
    /// ```
    /// use gba_core::git::GitHub;
    ///
    /// let gh = GitHub::new("/path/to/repo");
    /// ```
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            workdir: workdir.into(),
        }
    }

    /// Get PR status for a branch.
    ///
    /// Returns `None` if no PR exists for the branch.
    ///
    /// # Arguments
    ///
    /// * `branch` - The branch name to check
    ///
    /// # Errors
    ///
    /// Returns an error if the gh command fails for reasons other than
    /// no PR existing.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_core::git::{GitHub, PrStatus};
    ///
    /// let gh = GitHub::new(".");
    /// if let Some(status) = gh.pr_status("feature/my-branch")? {
    ///     println!("PR status: {:?}", status);
    /// } else {
    ///     println!("No PR for this branch");
    /// }
    /// # Ok::<(), gba_core::EngineError>(())
    /// ```
    pub fn pr_status(&self, branch: &str) -> Result<Option<PrStatus>> {
        let output = Command::new("gh")
            .current_dir(&self.workdir)
            .args(["pr", "view", branch, "--json", "state", "-q", ".state"])
            .output()
            .map_err(|e| EngineError::github_error(format!("failed to execute gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if this is just "no PR exists"
            if stderr.contains("no pull requests found")
                || stderr.contains("Could not resolve")
                || stderr.contains("no open pull requests")
            {
                return Ok(None);
            }
            return Err(EngineError::github_error(format!(
                "failed to get PR status for '{branch}': {stderr}"
            )));
        }

        let state = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_uppercase();

        match state.as_str() {
            "OPEN" => Ok(Some(PrStatus::Open)),
            "MERGED" => Ok(Some(PrStatus::Merged)),
            "CLOSED" => Ok(Some(PrStatus::Closed)),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    /// Helper to run a git command in an isolated temp repo.
    /// Uses `GIT_CEILING_DIRECTORIES` to prevent discovery of parent repos.
    fn git_cmd(temp_dir: &TempDir) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(temp_dir.path());
        // Prevent git from walking up to find parent repos
        if let Some(parent) = temp_dir.path().parent() {
            cmd.env("GIT_CEILING_DIRECTORIES", parent);
        }
        cmd
    }

    /// Helper to create an initialized git repo in a temp directory.
    /// The repo is completely isolated from any parent repositories.
    fn create_test_repo() -> (TempDir, GitRepo) {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        // Initialize git repo with ceiling directories to prevent parent discovery
        git_cmd(&temp_dir)
            .args(["init"])
            .output()
            .expect("failed to init git repo");

        // Configure user for commits
        git_cmd(&temp_dir)
            .args(["config", "user.email", "test@example.com"])
            .output()
            .expect("failed to set user email");

        git_cmd(&temp_dir)
            .args(["config", "user.name", "Test User"])
            .output()
            .expect("failed to set user name");

        // Disable hooks to prevent interference from parent repo's pre-commit
        git_cmd(&temp_dir)
            .args(["config", "core.hooksPath", "/dev/null"])
            .output()
            .expect("failed to disable hooks");

        let repo = GitRepo::new(temp_dir.path());
        (temp_dir, repo)
    }

    /// Helper to create a git repo with an initial commit.
    fn create_test_repo_with_commit() -> (TempDir, GitRepo) {
        let (temp_dir, repo) = create_test_repo();

        // Create a file and make initial commit
        let file_path = temp_dir.path().join("README.md");
        fs::write(&file_path, "# Test Repository").expect("failed to write file");

        repo.add(".").expect("failed to stage file");
        repo.commit("Initial commit").expect("failed to commit");

        (temp_dir, repo)
    }

    // === GitRepo Construction Tests ===

    #[test]
    fn test_should_create_git_repo_with_path() {
        let repo = GitRepo::new("/path/to/repo");
        assert_eq!(repo.workdir(), Path::new("/path/to/repo"));
    }

    #[test]
    fn test_should_create_git_repo_from_pathbuf() {
        let path = PathBuf::from("/another/path");
        let repo = GitRepo::new(path);
        assert_eq!(repo.workdir(), Path::new("/another/path"));
    }

    // === Branch Operations Tests ===

    #[test]
    fn test_should_get_current_branch_on_main() {
        let (_temp_dir, repo) = create_test_repo_with_commit();

        // Default branch after init could be 'main' or 'master' depending on git config
        let branch = repo.current_branch().expect("failed to get current branch");
        assert!(
            branch == "main" || branch == "master",
            "Expected 'main' or 'master', got '{branch}'"
        );
    }

    #[test]
    fn test_should_fail_get_branch_in_non_git_directory() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let repo = GitRepo::new(temp_dir.path());

        let result = repo.current_branch();
        assert!(result.is_err());
    }

    #[test]
    fn test_should_delete_branch() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create a new branch using isolated command
        git_cmd(&temp_dir)
            .args(["branch", "feature/test-branch"])
            .output()
            .expect("failed to create branch");

        // Delete the branch
        let result = repo.delete_branch("feature/test-branch", false);
        assert!(result.is_ok());

        // Verify branch is gone
        let output = git_cmd(&temp_dir)
            .args(["branch", "--list", "feature/test-branch"])
            .output()
            .expect("failed to list branches");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains("feature/test-branch"));
    }

    #[test]
    fn test_should_force_delete_unmerged_branch() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create and checkout a new branch
        git_cmd(&temp_dir)
            .args(["checkout", "-b", "feature/unmerged"])
            .output()
            .expect("failed to create branch");

        // Make a commit on the new branch
        let file_path = temp_dir.path().join("new_file.txt");
        fs::write(&file_path, "content").expect("failed to write file");
        repo.add("new_file.txt").expect("failed to stage file");
        repo.commit("Unmerged commit").expect("failed to commit");

        // Go back to main/master
        let main_branch = repo.detect_default_branch();
        git_cmd(&temp_dir)
            .args(["checkout", &main_branch])
            .output()
            .expect("failed to checkout main");

        // Try normal delete - should fail
        let result = repo.delete_branch("feature/unmerged", false);
        assert!(result.is_err());

        // Force delete - should succeed
        let result = repo.delete_branch("feature/unmerged", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_should_fail_delete_nonexistent_branch() {
        let (_temp_dir, repo) = create_test_repo_with_commit();

        let result = repo.delete_branch("nonexistent-branch", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_should_detect_default_branch_as_main_or_master() {
        let (_temp_dir, repo) = create_test_repo_with_commit();

        let default_branch = repo.detect_default_branch();
        assert!(
            default_branch == "main" || default_branch == "master",
            "Expected 'main' or 'master', got '{default_branch}'"
        );
    }

    // === Worktree Operations Tests ===

    #[test]
    fn test_should_create_and_remove_worktree() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create worktree
        let worktree_path = temp_dir.path().join("worktree-test");
        let result = repo.create_worktree(&worktree_path, "feature/worktree-test");
        assert!(result.is_ok(), "Failed to create worktree: {result:?}");

        // Verify worktree exists
        assert!(worktree_path.exists());
        assert!(worktree_path.join(".git").exists());

        // Remove worktree
        let result = repo.remove_worktree(worktree_path.to_str().unwrap(), false);
        assert!(result.is_ok(), "Failed to remove worktree: {result:?}");
    }

    #[test]
    fn test_should_fail_create_worktree_with_existing_branch() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create a branch first
        git_cmd(&temp_dir)
            .args(["branch", "existing-branch"])
            .output()
            .expect("failed to create branch");

        // Try to create worktree with existing branch name
        let worktree_path = temp_dir.path().join("worktree-fail");
        let result = repo.create_worktree(&worktree_path, "existing-branch");
        assert!(result.is_err());
    }

    #[test]
    fn test_should_force_remove_worktree_with_changes() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create worktree
        let worktree_path = temp_dir.path().join("worktree-dirty");
        repo.create_worktree(&worktree_path, "feature/dirty-worktree")
            .expect("failed to create worktree");

        // Make uncommitted changes in worktree
        let file_path = worktree_path.join("dirty-file.txt");
        fs::write(&file_path, "uncommitted content").expect("failed to write file");

        // Stage the file in the worktree (need to set ceiling for worktree too)
        let mut cmd = Command::new("git");
        cmd.current_dir(&worktree_path);
        if let Some(parent) = worktree_path.parent() {
            cmd.env("GIT_CEILING_DIRECTORIES", parent);
        }
        cmd.args(["add", "dirty-file.txt"])
            .output()
            .expect("failed to stage file");

        // Normal remove should fail
        let result = repo.remove_worktree(worktree_path.to_str().unwrap(), false);
        assert!(result.is_err());

        // Force remove should succeed
        let result = repo.remove_worktree(worktree_path.to_str().unwrap(), true);
        assert!(result.is_ok());
    }

    // === Commit Operations Tests ===

    #[test]
    fn test_should_return_none_for_head_sha_in_empty_repo() {
        let (_temp_dir, repo) = create_test_repo();

        let result = repo.head_short_sha().expect("unexpected error");
        assert!(result.is_none());
    }

    #[test]
    fn test_should_return_sha_after_commit() {
        let (_temp_dir, repo) = create_test_repo_with_commit();

        let sha = repo
            .head_short_sha()
            .expect("failed to get sha")
            .expect("expected some sha");
        assert!(!sha.is_empty());
        // Short SHA is typically 7 characters
        assert!(sha.len() >= 7);
    }

    #[test]
    fn test_should_stage_and_commit_file() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create a new file
        let file_path = temp_dir.path().join("new_feature.rs");
        fs::write(&file_path, "fn main() {}").expect("failed to write file");

        // Stage and commit
        repo.add("new_feature.rs").expect("failed to stage");
        repo.commit("Add new feature").expect("failed to commit");

        // Verify commit was made by checking log
        let output = git_cmd(&temp_dir)
            .args(["log", "--oneline", "-1"])
            .output()
            .expect("failed to run git log");
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("Add new feature"));
    }

    #[test]
    fn test_should_stage_multiple_files_with_pattern() {
        let (temp_dir, repo) = create_test_repo_with_commit();

        // Create multiple files
        fs::write(temp_dir.path().join("file1.txt"), "content1").expect("failed to write file1");
        fs::write(temp_dir.path().join("file2.txt"), "content2").expect("failed to write file2");

        // Stage all with "."
        repo.add(".").expect("failed to stage");
        repo.commit("Add multiple files").expect("failed to commit");

        // Verify both files are in the repo
        let output = git_cmd(&temp_dir)
            .args(["ls-files"])
            .output()
            .expect("failed to list files");
        let files = String::from_utf8_lossy(&output.stdout);
        assert!(files.contains("file1.txt"));
        assert!(files.contains("file2.txt"));
    }

    #[test]
    fn test_should_fail_commit_with_nothing_staged() {
        let (_temp_dir, repo) = create_test_repo_with_commit();

        // Try to commit with nothing staged
        let result = repo.commit("Empty commit");
        assert!(result.is_err());
    }

    #[test]
    fn test_should_fail_add_nonexistent_file() {
        let (_temp_dir, repo) = create_test_repo_with_commit();

        // Try to stage a file that doesn't exist
        let result = repo.add("nonexistent-file.txt");
        assert!(result.is_err());
    }

    // === Error Handling Tests ===

    #[test]
    fn test_should_fail_operations_on_non_git_directory() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let repo = GitRepo::new(temp_dir.path());

        // All operations should fail gracefully
        assert!(repo.current_branch().is_err());
        assert!(repo.head_short_sha().is_err());
        assert!(repo.add(".").is_err());
        assert!(repo.commit("test").is_err());
    }
}
