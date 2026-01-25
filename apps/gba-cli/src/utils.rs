//! Utility functions for GBA CLI.
//!
//! This module provides common helper functions for CLI operations
//! including directory lookup, formatting, path manipulation, and engine creation.

use std::fs;
use std::path::{Path, PathBuf};

use gba_core::{Engine, EngineConfig};
use gba_pm::PromptManager;

use crate::error::CliError;
use crate::state::FeatureState;

/// GBA directory name.
pub const GBA_DIR: &str = ".gba";

/// Worktree directory name.
pub const TREES_DIR: &str = ".trees";

/// Get the GBA directory path for a workdir.
#[must_use]
pub fn gba_dir(workdir: &Path) -> PathBuf {
    workdir.join(GBA_DIR)
}

/// Get the worktree directory path for a workdir.
#[must_use]
pub fn trees_dir(workdir: &Path) -> PathBuf {
    workdir.join(TREES_DIR)
}

/// Get the worktree path for a feature.
///
/// Returns `.trees/{slug}/`
#[must_use]
pub fn feature_worktree_path(workdir: &Path, slug: &str) -> PathBuf {
    trees_dir(workdir).join(slug)
}

/// Get the feature's GBA directory path (where state and specs are stored).
///
/// Returns `.trees/{slug}/.gba/{slug}/`
#[must_use]
pub fn feature_gba_dir(workdir: &Path, slug: &str) -> PathBuf {
    feature_worktree_path(workdir, slug)
        .join(GBA_DIR)
        .join(slug)
}

/// Get the feature's state.yml file path.
///
/// Returns `.trees/{slug}/.gba/{slug}/state.yml`
#[must_use]
pub fn feature_state_file(workdir: &Path, slug: &str) -> PathBuf {
    feature_gba_dir(workdir, slug).join("state.yml")
}

/// Get the feature's specs directory path.
///
/// Returns `.trees/{slug}/.gba/{slug}/specs/`
#[must_use]
pub fn feature_specs_dir(workdir: &Path, slug: &str) -> PathBuf {
    feature_gba_dir(workdir, slug).join("specs")
}

/// Check if a feature exists (has state.yml).
#[must_use]
pub fn feature_exists(workdir: &Path, slug: &str) -> bool {
    feature_state_file(workdir, slug).exists()
}

/// Check if GBA is initialized in the given workdir.
///
/// GBA is considered initialized if `.gba/config.yml` exists.
/// This check is more robust than just checking for the directory,
/// since other processes (like logging) may create `.gba/` subdirectories.
#[must_use]
pub fn is_initialized(workdir: &Path) -> bool {
    gba_dir(workdir).join("config.yml").exists()
}

/// Find a feature directory by slug.
///
/// Searches the `.trees/{slug}/.gba/{slug}/` directory for state.yml.
/// Returns the path to the feature's state directory.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - No matching feature is found
pub fn find_feature_dir(workdir: &Path, slug: &str) -> Result<PathBuf, CliError> {
    if !is_initialized(workdir) {
        return Err(CliError::NotInitialized);
    }

    // Feature state is at .trees/{slug}/.gba/{slug}/state.yml
    let feature_dir = trees_dir(workdir).join(slug).join(GBA_DIR).join(slug);
    if feature_dir.join("state.yml").exists() {
        return Ok(feature_dir);
    }

    Err(CliError::FeatureNotFound(slug.to_string()))
}

/// List all feature directories in the trees directory.
///
/// Scans `.trees/` for worktrees and returns paths to feature state directories.
/// Each feature is at `.trees/{slug}/.gba/{slug}/`.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Cannot read the directory
pub fn list_feature_dirs(workdir: &Path) -> Result<Vec<PathBuf>, CliError> {
    if !is_initialized(workdir) {
        return Err(CliError::NotInitialized);
    }

    let trees = trees_dir(workdir);
    if !trees.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&trees)
        .map_err(|e| CliError::Io(format!("failed to read {}: {}", trees.display(), e)))?;

    let mut feature_dirs = Vec::new();
    for entry in entries.flatten() {
        let worktree_path = entry.path();
        if worktree_path.is_dir()
            && let Some(slug) = worktree_path.file_name().and_then(|n| n.to_str())
        {
            // Feature state is at .trees/{slug}/.gba/{slug}/state.yml
            let feature_dir = worktree_path.join(GBA_DIR).join(slug);
            if feature_dir.join("state.yml").exists() {
                feature_dirs.push(feature_dir);
            }
        }
    }

    // Sort by directory name
    feature_dirs.sort();

    Ok(feature_dirs)
}

/// Generate the next feature ID based on existing features.
///
/// Reads feature IDs from state.yml files in each worktree.
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
pub fn next_feature_id(workdir: &Path) -> Result<String, CliError> {
    let feature_dirs = list_feature_dirs(workdir).unwrap_or_default();

    let max_id = feature_dirs
        .iter()
        .filter_map(|dir| FeatureState::load(dir).ok())
        .filter_map(|state| state.feature.id.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    Ok(format!("{:04}", max_id + 1))
}

/// Format a duration in a human-readable way.
///
/// # Examples
///
/// ```
/// use gba_cli::utils::format_duration;
/// use std::time::Duration;
///
/// assert_eq!(format_duration(Duration::from_secs(65)), "1m 5s");
/// assert_eq!(format_duration(Duration::from_secs(3665)), "1h 1m 5s");
/// ```
#[must_use]
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format a cost in USD.
///
/// # Examples
///
/// ```
/// use gba_cli::utils::format_cost;
///
/// assert_eq!(format_cost(0.15), "$0.15");
/// assert_eq!(format_cost(1.5), "$1.50");
/// ```
#[must_use]
pub fn format_cost(cost_usd: f64) -> String {
    format!("${:.2}", cost_usd)
}

/// Load feature state by slug.
///
/// # Errors
///
/// Returns an error if the feature is not found or state cannot be loaded.
pub fn load_feature_state(workdir: &Path, slug: &str) -> Result<FeatureState, CliError> {
    let feature_dir = find_feature_dir(workdir, slug)?;
    FeatureState::load(&feature_dir)
}

/// Create the GBA engine with prompts loaded from the tasks directory.
///
/// # Arguments
///
/// * `workdir` - Main repository working directory (for loading tasks)
///
/// # Errors
///
/// Returns an error if:
/// - Tasks directory not found
/// - Failed to load prompts
/// - Failed to create engine
pub fn create_engine(workdir: &Path) -> Result<Engine<'static>, CliError> {
    create_engine_with_context(workdir, workdir)
}

/// Create the GBA engine with a custom context working directory.
///
/// This is useful when the engine should operate in a different directory
/// than where the tasks are loaded from (e.g., in a worktree).
///
/// # Arguments
///
/// * `workdir` - Main repository working directory (for loading tasks)
/// * `context_workdir` - Working directory for the engine context
///
/// # Errors
///
/// Returns an error if:
/// - Tasks directory not found
/// - Failed to load prompts
/// - Failed to create engine
pub fn create_engine_with_context(
    workdir: &Path,
    context_workdir: &Path,
) -> Result<Engine<'static>, CliError> {
    let tasks_dir = workdir.join("tasks");

    if !tasks_dir.exists() {
        return Err(CliError::Config(format!(
            "tasks directory not found: {}",
            tasks_dir.display()
        )));
    }

    let mut prompts = PromptManager::new();
    prompts.load_dir(&tasks_dir)?;

    let config = EngineConfig::builder()
        .workdir(context_workdir)
        .prompts(prompts)
        .build();

    let engine = Engine::new(config)?;
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 5s");
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h 0m 0s");
        assert_eq!(format_duration(Duration::from_secs(3665)), "1h 1m 5s");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(0.15), "$0.15");
        assert_eq!(format_cost(1.5), "$1.50");
        assert_eq!(format_cost(10.0), "$10.00");
        assert_eq!(format_cost(0.123), "$0.12");
    }

    #[test]
    fn test_gba_dir() {
        let workdir = PathBuf::from("/tmp/repo");
        assert_eq!(gba_dir(&workdir), PathBuf::from("/tmp/repo/.gba"));
    }

    #[test]
    fn test_trees_dir() {
        let workdir = PathBuf::from("/tmp/repo");
        assert_eq!(trees_dir(&workdir), PathBuf::from("/tmp/repo/.trees"));
    }

    #[test]
    fn test_feature_worktree_path() {
        let workdir = PathBuf::from("/tmp/repo");
        assert_eq!(
            feature_worktree_path(&workdir, "my-feature"),
            PathBuf::from("/tmp/repo/.trees/my-feature")
        );
    }

    #[test]
    fn test_feature_gba_dir() {
        let workdir = PathBuf::from("/tmp/repo");
        assert_eq!(
            feature_gba_dir(&workdir, "my-feature"),
            PathBuf::from("/tmp/repo/.trees/my-feature/.gba/my-feature")
        );
    }

    #[test]
    fn test_feature_state_file() {
        let workdir = PathBuf::from("/tmp/repo");
        assert_eq!(
            feature_state_file(&workdir, "my-feature"),
            PathBuf::from("/tmp/repo/.trees/my-feature/.gba/my-feature/state.yml")
        );
    }

    #[test]
    fn test_feature_specs_dir() {
        let workdir = PathBuf::from("/tmp/repo");
        assert_eq!(
            feature_specs_dir(&workdir, "my-feature"),
            PathBuf::from("/tmp/repo/.trees/my-feature/.gba/my-feature/specs")
        );
    }
}
