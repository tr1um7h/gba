# Design: Add PR Info Commit/Push to state.yml

## Overview

When a PR is created, `state.yml` is updated with `pr_url` and `pr_number`, but this change is not committed and pushed to the remote repository. This causes PR information to be lost when the worktree is cleaned up.

## Problem Analysis

Current flow in `apps/gba-cli/src/commands/run.rs`:
1. `create_pull_request()` extracts PR URL and number from LLM output (lines 1032-1038)
2. `state.result.pr_url` and `state.result.pr_number` are set
3. `state.save(&feature_dir)` writes to `.gba/<slug>/state.yml` (line 469)
4. **Missing**: No git commit/push to persist this change

## Solution

Add a helper function to commit and push state changes after PR creation. This ensures PR metadata is persisted in the repository.

## Implementation Details

### New Helper Function

Add `commit_and_push_state_update()` in `apps/gba-cli/src/commands/run.rs`:

```rust
/// Commit and push state.yml changes after PR creation.
///
/// This ensures PR information (url, number) is persisted in the repository.
fn commit_and_push_state_update(
    ctx: &TaskContext,
    feature_slug: &str,
    pr_number: Option<u32>,
) -> Result<(), CliError> {
    let state_file = format!(".gba/{}/state.yml", feature_slug);

    // Stage the state file
    let status = std::process::Command::new("git")
        .current_dir(&ctx.worktree_path)
        .args(["add", &state_file])
        .status()
        .map_err(|e| CliError::Git(format!("failed to stage state file: {}", e)))?;

    if !status.success() {
        return Err(CliError::Git("failed to stage state file".to_string()));
    }

    // Commit with appropriate message
    let commit_msg = match pr_number {
        Some(num) => format!("chore({}): record PR #{} in state", feature_slug, num),
        None => format!("chore({}): update state after PR creation", feature_slug),
    };

    let status = std::process::Command::new("git")
        .current_dir(&ctx.worktree_path)
        .args(["commit", "-m", &commit_msg])
        .status()
        .map_err(|e| CliError::Git(format!("failed to commit state: {}", e)))?;

    if !status.success() {
        // Commit may fail if no changes (already committed) - this is ok
        tracing::debug!("state commit returned non-zero (may be no changes)");
    }

    // Push to origin
    let status = std::process::Command::new("git")
        .current_dir(&ctx.worktree_path)
        .args(["push"])
        .status()
        .map_err(|e| CliError::Git(format!("failed to push state: {}", e)))?;

    if !status.success() {
        return Err(CliError::Git("failed to push state changes".to_string()));
    }

    Ok(())
}
```

### Integration Point

In `execute_full_pipeline_with_tui()`, after successful PR creation (around line 469):

```rust
// After: state.save(&feature_dir)?;
// Add:
if state.result.pr_url.is_some() {
    if let Err(e) = commit_and_push_state_update(&ctx, &state.feature.slug, state.result.pr_number) {
        warn!("Failed to commit/push state update: {}", e);
        // Non-fatal: PR was created successfully, state just wasn't persisted
    }
}
```

## File Changes

- `apps/gba-cli/src/commands/run.rs`: Add helper function and call it after PR creation

## Phases

- impl: Add commit_and_push_state_update function and integrate into PR creation flow
