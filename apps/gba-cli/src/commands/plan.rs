//! Plan command implementation.
//!
//! This module implements the `gba plan` command which opens an interactive
//! TUI session to plan a new feature through conversation with Claude.

use std::path::Path;

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::error::CliError;
use crate::state::FeatureState;
use crate::tui::App;
use crate::utils;

/// Run the plan command.
///
/// This function:
/// 1. Checks if GBA is initialized
/// 2. Checks if the feature already exists
/// 3. Generates a new feature ID
/// 4. Creates the Engine with prompts loaded
/// 5. Launches the TUI application
/// 6. On completion, creates git worktree and saves state
///
/// # Arguments
///
/// * `workdir` - Working directory path
/// * `slug` - Feature slug identifier
/// * `verbose` - Whether to show verbose output
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Feature already exists
/// - TUI cannot be launched
/// - Git operations fail
pub async fn run_plan(workdir: &Path, slug: &str, _verbose: bool) -> Result<()> {
    info!(slug = slug, workdir = %workdir.display(), "starting plan command");

    // Check if GBA is initialized
    if !utils::is_initialized(workdir) {
        return Err(CliError::NotInitialized.into());
    }

    // Check if feature already exists
    if utils::feature_exists(workdir, slug) {
        return Err(CliError::FeatureExists(slug.to_string()).into());
    }

    // Generate next feature ID
    let feature_id = utils::next_feature_id(workdir)?;
    info!(feature_id = %feature_id, "generated feature ID");

    // Create the engine
    let engine = utils::create_engine(workdir)?;

    // Launch TUI
    let mut app = App::new(slug.to_string(), feature_id.clone(), workdir);

    println!("Starting interactive planning session...");
    println!("Press Ctrl+C to exit at any time.");
    println!();

    // Run the TUI and get the result
    let result = app.run(&engine).await.context("TUI error")?;

    // Process result
    if let Some(state) = result {
        // Planning completed - verify artifacts created by Claude
        verify_feature_artifacts(workdir, &state)?;

        println!();
        println!("Planning completed!");
        println!("Feature ID: {}", state.feature.id);
        println!("Feature slug: {}", state.feature.slug);
        println!("Worktree: {}", state.git.worktree_path);
        println!("Branch: {}", state.git.branch);
        println!();
        println!(
            "Run `gba run {}` to execute the implementation.",
            state.feature.slug
        );
    } else {
        println!();
        println!("Planning session cancelled.");
    }

    Ok(())
}

/// Verify feature artifacts exist after planning completes.
///
/// The worktree, specs, and state.yml are created by Claude during planning.
/// This function just verifies they exist.
fn verify_feature_artifacts(workdir: &Path, state: &FeatureState) -> Result<()> {
    let slug = &state.feature.slug;

    // Verify worktree exists
    let worktree_path = utils::feature_worktree_path(workdir, slug);
    if !worktree_path.exists() {
        warn!(
            worktree = %worktree_path.display(),
            "worktree not found - may need manual creation"
        );
    } else {
        info!(worktree = %worktree_path.display(), "worktree verified");
    }

    // Verify state.yml exists
    let state_file = utils::feature_state_file(workdir, slug);
    if !state_file.exists() {
        warn!(
            state_file = %state_file.display(),
            "state.yml not found"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;

    use crate::utils;

    fn setup_gba_project() -> TempDir {
        let temp_dir = TempDir::new().unwrap();

        // Create .gba directory with config
        fs::create_dir_all(temp_dir.path().join(".gba")).unwrap();
        fs::write(temp_dir.path().join(".gba").join("config.yml"), "").unwrap();

        // Create tasks directory with plan task
        let plan_dir = temp_dir.path().join("tasks").join("plan");
        fs::create_dir_all(&plan_dir).unwrap();

        fs::write(
            plan_dir.join("config.yml"),
            "preset: true\ntools: []\ndisallowedTools: []\n",
        )
        .unwrap();

        fs::write(
            plan_dir.join("system.j2"),
            "You are GBA. Feature: {{ feature_slug }}",
        )
        .unwrap();

        fs::write(plan_dir.join("user.j2"), "Plan the feature.").unwrap();

        temp_dir
    }

    #[test]
    fn test_should_create_engine() {
        let temp_dir = setup_gba_project();
        let result = utils::create_engine(temp_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_should_fail_without_tasks_dir() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join(".gba")).unwrap();
        // No tasks directory

        let result = utils::create_engine(temp_dir.path());
        assert!(result.is_err());
    }
}
