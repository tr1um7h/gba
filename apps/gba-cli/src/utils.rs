//! Utility functions for GBA CLI.
//!
//! This module provides common helper functions for CLI operations
//! including directory lookup, formatting, and path manipulation.

use std::fs;
use std::path::{Path, PathBuf};

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
}
