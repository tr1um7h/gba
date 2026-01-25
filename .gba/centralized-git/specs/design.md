# Design: Centralized Git Operations

## Overview

Centralize all git and GitHub CLI operations scattered across `gba-cli` into a dedicated `git` module in `gba-core`. This improves code reuse, testability, and maintainability.

## Current State

Git/gh operations are scattered across multiple files in `gba-cli`:

| File | Operations |
|------|------------|
| `run.rs` | `git rev-parse --short HEAD`, `git add`, `git commit`, `git push` |
| `clean.rs` | `git rev-parse --abbrev-ref HEAD`, `gh pr view`, `git worktree remove`, `git branch -D` |
| `plan.rs` | `git rev-parse --abbrev-ref HEAD`, `git worktree add` |
| `list.rs` | `git rev-parse --abbrev-ref HEAD` |
| `utils.rs` | `git remote show origin`, `git branch --list` |

## Design

### Module Structure

```
crates/gba-core/src/
├── lib.rs          # Add: pub mod git
├── git.rs          # New: centralized git/gh operations
├── error.rs        # Add: GitError, GitHubError variants
└── ...
```

### API Design

```rust
// crates/gba-core/src/git.rs

use std::path::{Path, PathBuf};
use crate::Result;

/// PR status from GitHub CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrStatus {
    Open,
    Merged,
    Closed,
}

/// Git repository operations wrapper.
#[derive(Debug)]
pub struct GitRepo {
    workdir: PathBuf,
}

impl GitRepo {
    /// Create a new GitRepo for the given working directory.
    pub fn new(workdir: impl Into<PathBuf>) -> Self;

    /// Get the working directory path.
    pub fn workdir(&self) -> &Path;

    // === Branch Operations ===

    /// Get the current branch name.
    pub fn current_branch(&self) -> Result<String>;

    /// Delete a local branch.
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()>;

    /// Detect the default branch (main/master) of the repository.
    pub fn detect_default_branch(&self) -> String;

    // === Worktree Operations ===

    /// Create a new worktree with a new branch.
    pub fn create_worktree(&self, path: &Path, branch: &str) -> Result<()>;

    /// Remove a worktree.
    pub fn remove_worktree(&self, path: &str, force: bool) -> Result<()>;

    // === Commit Operations ===

    /// Get the short SHA of HEAD.
    pub fn head_short_sha(&self) -> Result<Option<String>>;

    /// Stage a file.
    pub fn add(&self, path: &str) -> Result<()>;

    /// Create a commit with the given message.
    pub fn commit(&self, message: &str) -> Result<()>;

    /// Push to origin.
    pub fn push(&self) -> Result<()>;
}

/// GitHub CLI operations wrapper.
#[derive(Debug)]
pub struct GitHub {
    workdir: PathBuf,
}

impl GitHub {
    /// Create a new GitHub CLI wrapper for the given working directory.
    pub fn new(workdir: impl Into<PathBuf>) -> Self;

    /// Get PR status for a branch.
    /// Returns None if no PR exists for the branch.
    pub fn pr_status(&self, branch: &str) -> Result<Option<PrStatus>>;
}
```

### Error Handling

Add Git-related error variants to `EngineError`:

```rust
// crates/gba-core/src/error.rs

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    // ... existing variants ...

    /// Git operation error.
    #[error("git error: {0}")]
    GitError(String),

    /// GitHub CLI operation error.
    #[error("github cli error: {0}")]
    GitHubError(String),
}

impl EngineError {
    /// Create a new Git error.
    pub fn git_error(message: impl Into<String>) -> Self {
        Self::GitError(message.into())
    }

    /// Create a new GitHub CLI error.
    pub fn github_error(message: impl Into<String>) -> Self {
        Self::GitHubError(message.into())
    }
}
```

### CLI Migration

Update `gba-cli` to use the new centralized module:

1. **Remove** direct `std::process::Command` git/gh calls
2. **Import** `gba_core::git::{GitRepo, GitHub, PrStatus}`
3. **Replace** each scattered operation with method calls

Example migration in `plan.rs`:

```rust
// Before
let output = Command::new("git")
    .current_dir(worktree_path)
    .args(["rev-parse", "--abbrev-ref", "HEAD"])
    .output()
    .map_err(|e| CliError::Git(format!("failed to get branch: {}", e)))?;
let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

// After
let repo = GitRepo::new(worktree_path);
let branch = repo.current_branch()?;
```

### Error Mapping

The CLI will map `EngineError::GitError` and `EngineError::GitHubError` to `CliError::Git`:

```rust
// In gba-cli error handling
impl From<gba_core::EngineError> for CliError {
    fn from(err: gba_core::EngineError) -> Self {
        match err {
            gba_core::EngineError::GitError(msg) => CliError::Git(msg),
            gba_core::EngineError::GitHubError(msg) => CliError::Git(msg),
            other => CliError::Engine(other),
        }
    }
}
```

## Phases

- core-git-module: Create git.rs in gba-core with GitRepo, GitHub structs and error variants
- migrate-cli-commands: Update all gba-cli files to use centralized git module

## Notes

- The `PrStatus` enum will be moved from `clean.rs` to `gba-core::git`
- `detect_default_branch` returns `String` (not `Result`) for backward compatibility - it falls back to "main"
- All operations use `std::process::Command` internally but provide a clean API
