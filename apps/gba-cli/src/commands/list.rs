//! `gba list` command implementation.
//!
//! Lists all features in the GBA project with their status.
//! Combines information from both worktrees (.trees/) and feature directories (.gba/).

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::Result;
use gba_core::git::GitRepo;
use tracing::debug;

use crate::error::CliError;
use crate::state::FeatureState;
use crate::utils::{feature_gba_dir, format_cost, gba_dir, is_initialized, trees_dir};

/// Information about a feature for display.
#[derive(Debug)]
struct FeatureInfo {
    /// Feature ID (e.g., "0001").
    id: String,
    /// Feature slug.
    slug: String,
    /// Feature status.
    status: String,
    /// Progress (e.g., "1/3").
    progress: String,
    /// Total cost.
    cost: String,
}

/// Run the `gba list` command.
///
/// Lists all features in the repository with their status, progress, and cost.
/// Combines information from:
/// - Worktrees in `.trees/` (active features, including those in planning)
/// - Feature directories in `.gba/` (merged features with history)
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Cannot read feature directories
pub async fn run_list(workdir: &Path) -> Result<()> {
    if !is_initialized(workdir) {
        return Err(CliError::NotInitialized.into());
    }

    debug!(workdir = %workdir.display(), "Listing features");

    let features = collect_all_features(workdir)?;

    if features.is_empty() {
        println!("No features found.");
        println!();
        println!("Run `gba plan <feature-slug>` to plan a new feature.");
        return Ok(());
    }

    // Print header
    println!(
        "{:<6} {:<30} {:<12} {:<10} {:<10}",
        "ID", "SLUG", "STATUS", "PROGRESS", "COST"
    );
    println!("{}", "-".repeat(70));

    for feature in &features {
        println!(
            "{:<6} {:<30} {:<12} {:<10} {:<10}",
            feature.id, feature.slug, feature.status, feature.progress, feature.cost
        );
    }

    println!();
    println!("Total: {} feature(s)", features.len());

    Ok(())
}

/// Collect all features from worktrees and .gba directory.
fn collect_all_features(workdir: &Path) -> Result<Vec<FeatureInfo>, CliError> {
    let mut features = Vec::new();
    let mut seen_slugs = HashSet::new();

    // 1. Scan worktrees (.trees/)
    let trees = trees_dir(workdir);
    if trees.exists() {
        let entries = fs::read_dir(&trees)
            .map_err(|e| CliError::Io(format!("failed to read {}: {}", trees.display(), e)))?;

        for entry in entries.flatten() {
            let worktree_path = entry.path();
            if !worktree_path.is_dir() {
                continue;
            }

            let slug = match worktree_path.file_name().and_then(|n| n.to_str()) {
                Some(s) if !s.starts_with('.') => s.to_string(),
                _ => continue,
            };

            seen_slugs.insert(slug.clone());

            // Try to load state.yml
            let feature_dir = feature_gba_dir(workdir, &slug);
            let feature_info = if feature_dir.join("state.yml").exists() {
                // Has state - load it
                match FeatureState::load(&feature_dir) {
                    Ok(state) => {
                        let completed = state
                            .phases
                            .iter()
                            .filter(|p| p.status == crate::state::PhaseStatus::Completed)
                            .count();
                        let total = state.phases.len();
                        let progress = if total > 0 {
                            format!("{}/{}", completed, total)
                        } else {
                            "-".to_string()
                        };

                        FeatureInfo {
                            id: state.feature.id,
                            slug: state.feature.slug,
                            status: format!("{}", state.status),
                            progress,
                            cost: format_cost(state.total_stats.cost_usd),
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, slug = %slug, "Failed to load feature state");
                        FeatureInfo {
                            id: "????".to_string(),
                            slug,
                            status: "error".to_string(),
                            progress: "-".to_string(),
                            cost: "-".to_string(),
                        }
                    }
                }
            } else {
                // No state.yml - feature is in planning phase
                // Try to get feature ID from branch name
                let feature_id = get_feature_id_from_worktree(&worktree_path)
                    .unwrap_or_else(|| "????".to_string());

                FeatureInfo {
                    id: feature_id,
                    slug,
                    status: "planning".to_string(),
                    progress: "-".to_string(),
                    cost: "-".to_string(),
                }
            };

            features.push(feature_info);
        }
    }

    // 2. Scan main .gba/ for merged features (feature directories, not config/logs)
    let gba = gba_dir(workdir);
    if gba.exists() {
        let entries = fs::read_dir(&gba)
            .map_err(|e| CliError::Io(format!("failed to read {}: {}", gba.display(), e)))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(s) => s,
                None => continue,
            };

            // Skip known non-feature directories
            if name == "logs" || name == "config" || name.starts_with('.') {
                continue;
            }

            // Skip if already seen from worktree
            if seen_slugs.contains(name) {
                continue;
            }

            // Check if this is a feature directory (has state.yml)
            let state_file = path.join("state.yml");
            if state_file.exists() {
                match FeatureState::load(&path) {
                    Ok(state) => {
                        let completed = state
                            .phases
                            .iter()
                            .filter(|p| p.status == crate::state::PhaseStatus::Completed)
                            .count();
                        let total = state.phases.len();
                        let progress = if total > 0 {
                            format!("{}/{}", completed, total)
                        } else {
                            "-".to_string()
                        };

                        features.push(FeatureInfo {
                            id: state.feature.id,
                            slug: state.feature.slug,
                            status: format!("{}", state.status),
                            progress,
                            cost: format_cost(state.total_stats.cost_usd),
                        });
                    }
                    Err(e) => {
                        debug!(error = %e, name = %name, "Failed to load feature state from .gba");
                    }
                }
            }
        }
    }

    // Sort by ID
    features.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(features)
}

/// Get feature ID from worktree branch name.
fn get_feature_id_from_worktree(worktree_path: &Path) -> Option<String> {
    let repo = GitRepo::new(worktree_path);
    let branch = repo.current_branch().ok()?;

    // Parse feature/<id>-<slug> format
    branch
        .strip_prefix("feature/")
        .and_then(|s| s.split('-').next())
        .map(String::from)
}
