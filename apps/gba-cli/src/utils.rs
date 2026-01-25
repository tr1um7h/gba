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
/// Searches the `.gba/` directory for a feature matching the given slug.
/// The feature directory name format is `<id>_<slug>`.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - No matching feature is found
pub fn find_feature_dir(workdir: &Path, slug: &str) -> Result<PathBuf, CliError> {
    let gba = gba_dir(workdir);
    if !gba.exists() {
        return Err(CliError::NotInitialized);
    }

    let entries = fs::read_dir(&gba)
        .map_err(|e| CliError::Io(format!("failed to read {}: {}", gba.display(), e)))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.contains('_')
        {
            // Check if this directory matches the slug pattern: <id>_<slug>
            let parts: Vec<&str> = name.splitn(2, '_').collect();
            if parts.len() == 2 && parts[1] == slug {
                return Ok(path);
            }
        }
    }

    Err(CliError::FeatureNotFound(slug.to_string()))
}

/// Parse feature ID from a directory name.
///
/// Given a directory name like `0001_add-auth`, returns `Some("0001")`.
#[must_use]
pub fn parse_feature_id(dir_name: &str) -> Option<&str> {
    if dir_name.contains('_') {
        let parts: Vec<&str> = dir_name.splitn(2, '_').collect();
        if parts.len() == 2 {
            return Some(parts[0]);
        }
    }
    None
}

/// Parse feature slug from a directory name.
///
/// Given a directory name like `0001_add-auth`, returns `Some("add-auth")`.
#[must_use]
pub fn parse_feature_slug(dir_name: &str) -> Option<&str> {
    if dir_name.contains('_') {
        let parts: Vec<&str> = dir_name.splitn(2, '_').collect();
        if parts.len() == 2 {
            return Some(parts[1]);
        }
    }
    None
}

/// List all feature directories in the GBA directory.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Cannot read the directory
pub fn list_feature_dirs(workdir: &Path) -> Result<Vec<PathBuf>, CliError> {
    let gba = gba_dir(workdir);
    if !gba.exists() {
        return Err(CliError::NotInitialized);
    }

    let entries = fs::read_dir(&gba)
        .map_err(|e| CliError::Io(format!("failed to read {}: {}", gba.display(), e)))?;

    let mut feature_dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // Feature directories contain an underscore (e.g., 0001_slug)
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.contains('_')
            && path.join("state.yml").exists()
        {
            feature_dirs.push(path);
        }
    }

    // Sort by directory name (which includes ID prefix)
    feature_dirs.sort();

    Ok(feature_dirs)
}

/// Generate the next feature ID based on existing features.
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
pub fn next_feature_id(workdir: &Path) -> Result<String, CliError> {
    let feature_dirs = list_feature_dirs(workdir).unwrap_or_default();

    let max_id = feature_dirs
        .iter()
        .filter_map(|p| p.file_name())
        .filter_map(|n| n.to_str())
        .filter_map(parse_feature_id)
        .filter_map(|id| id.parse::<u32>().ok())
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
    fn test_parse_feature_id() {
        assert_eq!(parse_feature_id("0001_add-auth"), Some("0001"));
        assert_eq!(parse_feature_id("0123_my-feature"), Some("0123"));
        assert_eq!(parse_feature_id("invalid"), None);
        assert_eq!(parse_feature_id(""), None);
    }

    #[test]
    fn test_parse_feature_slug() {
        assert_eq!(parse_feature_slug("0001_add-auth"), Some("add-auth"));
        assert_eq!(parse_feature_slug("0123_my-feature"), Some("my-feature"));
        assert_eq!(
            parse_feature_slug("0001_slug_with_underscore"),
            Some("slug_with_underscore")
        );
        assert_eq!(parse_feature_slug("invalid"), None);
    }

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
