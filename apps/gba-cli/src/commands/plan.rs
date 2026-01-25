//! Plan command implementation.
//!
//! This module implements the `gba plan` command which opens an interactive
//! TUI session to plan a new feature through conversation with Claude.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::info;

use crate::error::CliError;
use crate::state::{
    FeatureInfo, FeatureResult, FeatureState, FeatureStatus, GitState, PhaseState, PhaseStatus,
    TaskStats,
};
use crate::tui::App;
use crate::utils;

/// Run the plan command.
///
/// This function:
/// 1. Checks if GBA is initialized
/// 2. Checks if the feature already exists
/// 3. Generates a new feature ID
/// 4. Creates git worktree and spec directories
/// 5. Creates the Engine with prompts loaded
/// 6. Launches the TUI application
/// 7. On completion, generates state.yml if specs exist
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
/// - Git worktree creation fails
/// - TUI cannot be launched
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

    // Detect base branch
    let base_branch = utils::detect_default_branch(workdir);
    info!(base_branch = %base_branch, "detected base branch");

    // Create git worktree
    let branch_name = format!("feature/{}-{}", feature_id, slug);
    let worktree_path = utils::feature_worktree_path(workdir, slug);
    println!("Creating worktree for feature '{}'...", slug);
    create_worktree(workdir, &worktree_path, &branch_name)?;
    info!(worktree = %worktree_path.display(), branch = %branch_name, "worktree created");

    // Create spec directories
    let specs_dir = utils::feature_specs_dir(workdir, slug);
    std::fs::create_dir_all(&specs_dir)
        .map_err(|e| CliError::Io(format!("failed to create specs directory: {}", e)))?;
    info!(specs_dir = %specs_dir.display(), "specs directory created");

    // Create the engine (context is the worktree, not main repo)
    let engine = utils::create_engine_with_context(workdir, &worktree_path)?;

    // Launch TUI
    let mut app = App::new(
        slug.to_string(),
        feature_id.clone(),
        base_branch.clone(),
        &worktree_path,
    );

    println!("Worktree: {}", worktree_path.display());
    println!("Branch: {}", branch_name);
    println!();
    println!("Starting interactive planning session...");
    println!("Type /done when planning is complete.");
    println!();

    // Run the TUI
    app.run(&engine).await.context("TUI error")?;

    // Check if specs were created and approved
    let specs_dir = utils::feature_specs_dir(workdir, slug);
    let design_exists = specs_dir.join("design.md").exists();
    let verification_exists = specs_dir.join("verification.md").exists();

    if design_exists && verification_exists {
        // Generate state.yml
        let state = generate_state(workdir, slug, &feature_id, &base_branch)?;

        println!();
        println!("Planning completed!");
        println!("Feature ID: {}", state.feature.id);
        println!("Feature slug: {}", state.feature.slug);
        println!("Worktree: {}", state.git.worktree_path);
        println!("Branch: {}", state.git.branch);
        println!();
        println!("Run `gba run {}` to execute the implementation.", slug);
    } else {
        println!();
        println!("Planning session cancelled.");
    }

    Ok(())
}

/// Create a git worktree for the feature.
fn create_worktree(
    workdir: &Path,
    worktree_path: &Path,
    branch_name: &str,
) -> Result<(), CliError> {
    let output = Command::new("git")
        .current_dir(workdir)
        .args([
            "worktree",
            "add",
            worktree_path.to_string_lossy().as_ref(),
            "-b",
            branch_name,
        ])
        .output()
        .map_err(|e| CliError::Git(format!("failed to run git worktree add: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Git(format!(
            "failed to create worktree: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Generate state.yml after specs are approved.
fn generate_state(
    workdir: &Path,
    slug: &str,
    feature_id: &str,
    base_branch: &str,
) -> Result<FeatureState, CliError> {
    let now = Utc::now();

    // Read design.md to extract phases
    let specs_dir = utils::feature_specs_dir(workdir, slug);
    let design_content = std::fs::read_to_string(specs_dir.join("design.md")).unwrap_or_default();

    // Extract phases from design.md
    let phases = extract_phases_from_design(&design_content);

    let state = FeatureState {
        feature: FeatureInfo {
            id: feature_id.to_string(),
            slug: slug.to_string(),
            created_at: now,
            updated_at: now,
        },
        status: FeatureStatus::Planned,
        current_phase: 0,
        git: GitState {
            worktree_path: format!(".trees/{}", slug),
            branch: format!("feature/{}-{}", feature_id, slug),
            base_branch: base_branch.to_string(),
        },
        phases,
        total_stats: TaskStats::default(),
        result: FeatureResult::default(),
        error: None,
    };

    // Save state.yml
    let feature_dir = utils::feature_gba_dir(workdir, slug);
    state.save(&feature_dir)?;

    info!(
        state_file = %feature_dir.join("state.yml").display(),
        "state.yml generated"
    );

    Ok(state)
}

/// Extract phase information from design.md content.
///
/// Looks for a `## Phases` section with a list of phases in the format:
/// ```markdown
/// ## Phases
///
/// - phase-name: Description
/// - another-phase: Description
/// ```
fn extract_phases_from_design(content: &str) -> Vec<PhaseState> {
    let mut phases = Vec::new();
    let mut in_phases_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for "## Phases" header (case-insensitive)
        if trimmed.starts_with("##") && !trimmed.starts_with("###") {
            let header = trimmed.trim_start_matches('#').trim().to_lowercase();
            in_phases_section = header == "phases";
            continue;
        }

        // Stop if we hit another ## header while in phases section
        if in_phases_section && trimmed.starts_with("##") {
            break;
        }

        // Parse list items in phases section
        if in_phases_section && (trimmed.starts_with('-') || trimmed.starts_with('*')) {
            let item = trimmed.trim_start_matches(['-', '*']).trim();
            if item.is_empty() {
                continue;
            }

            // Extract phase name (before colon if present)
            let name = item
                .split(':')
                .next()
                .unwrap_or(item)
                .trim()
                .to_lowercase()
                .replace(' ', "-");

            if !name.is_empty() && name.len() < 50 {
                phases.push(PhaseState {
                    name,
                    status: PhaseStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    commit_sha: None,
                    stats: None,
                });
            }
        }
    }

    // If no phases found, create a default one
    if phases.is_empty() {
        phases.push(PhaseState {
            name: "implementation".to_string(),
            status: PhaseStatus::Pending,
            started_at: None,
            completed_at: None,
            commit_sha: None,
            stats: None,
        });
    }

    phases
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
