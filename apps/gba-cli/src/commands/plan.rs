//! Plan command implementation.
//!
//! This module implements the `gba plan` command which opens an interactive
//! TUI session to plan a new feature through conversation with Claude.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::{info, warn};

use gba_core::{Engine, EngineConfig};
use gba_pm::PromptManager;

use crate::error::CliError;
use crate::git;
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
    if utils::find_feature_dir(workdir, slug).is_ok() {
        return Err(CliError::FeatureExists(slug.to_string()).into());
    }

    // Generate next feature ID
    let feature_id = utils::next_feature_id(workdir)?;
    info!(feature_id = %feature_id, "generated feature ID");

    // Create the engine
    let engine = create_engine(workdir)?;

    // Launch TUI
    let mut app = App::new(slug.to_string(), feature_id.clone(), &engine, workdir)
        .await
        .context("failed to create TUI app")?;

    println!("Starting interactive planning session...");
    println!("Press Ctrl+C to exit at any time.");
    println!();

    // Run the TUI and get the result
    let result = app.run().await.context("TUI error")?;

    // Process result
    if let Some(state) = result {
        // Planning completed - create artifacts
        create_feature_artifacts(workdir, &state)?;

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

/// Create the GBA engine with prompts loaded.
fn create_engine(workdir: &Path) -> Result<Engine<'static>> {
    let tasks_dir = workdir.join("tasks");

    if !tasks_dir.exists() {
        return Err(CliError::Config(format!(
            "tasks directory not found: {}",
            tasks_dir.display()
        ))
        .into());
    }

    // Load prompts
    let mut prompts = PromptManager::new();
    prompts
        .load_dir(&tasks_dir)
        .context("failed to load task templates")?;

    // Create engine config
    let config = EngineConfig::builder()
        .workdir(workdir)
        .prompts(prompts)
        .build();

    let engine = Engine::new(config).context("failed to create engine")?;

    Ok(engine)
}

/// Create feature artifacts after planning completes.
fn create_feature_artifacts(workdir: &Path, state: &FeatureState) -> Result<()> {
    let feature_id = &state.feature.id;
    let slug = &state.feature.slug;

    // Create feature directory
    let feature_dir = utils::gba_dir(workdir).join(format!("{}_{}", feature_id, slug));
    fs::create_dir_all(&feature_dir).context("failed to create feature directory")?;

    // Create specs directory
    let specs_dir = feature_dir.join("specs");
    fs::create_dir_all(&specs_dir).context("failed to create specs directory")?;

    // Save state
    state.save(&feature_dir).context("failed to save state")?;

    info!(
        feature_dir = %feature_dir.display(),
        "feature artifacts created"
    );

    // Create git worktree
    let trees_dir = utils::trees_dir(workdir);
    if !trees_dir.exists() {
        fs::create_dir_all(&trees_dir).context("failed to create .trees directory")?;
    }

    let worktree_path = trees_dir.join(format!("{}_{}", feature_id, slug));

    // Find base branch
    let base_branch = git::find_base_branch(workdir).unwrap_or_else(|_| "main".to_string());

    // Create worktree
    if let Err(e) = git::create_worktree(workdir, &worktree_path, &state.git.branch, &base_branch) {
        warn!(error = %e, "failed to create worktree, continuing without it");
    } else {
        info!(worktree = %worktree_path.display(), "worktree created");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_gba_project() -> TempDir {
        let temp_dir = TempDir::new().unwrap();

        // Create .gba directory
        fs::create_dir_all(temp_dir.path().join(".gba")).unwrap();

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
        let result = create_engine(temp_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_should_fail_without_tasks_dir() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join(".gba")).unwrap();
        // No tasks directory

        let result = create_engine(temp_dir.path());
        assert!(result.is_err());
    }
}
