//! `gba list` command implementation.
//!
//! Lists all features in the GBA project with their status.

use std::path::Path;

use anyhow::Result;
use tracing::debug;

use crate::error::CliError;
use crate::state::FeatureState;
use crate::utils::{format_cost, is_initialized, list_feature_dirs, parse_feature_id};

/// Run the `gba list` command.
///
/// Lists all features in the repository with their status, progress, and cost.
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

    let feature_dirs = list_feature_dirs(workdir)?;

    if feature_dirs.is_empty() {
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

    for dir in &feature_dirs {
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let id = parse_feature_id(dir_name).unwrap_or("????");

        match FeatureState::load(dir) {
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

                let status_str = format!("{}", state.status);
                let cost = format_cost(state.total_stats.cost_usd);

                println!(
                    "{:<6} {:<30} {:<12} {:<10} {:<10}",
                    id, state.feature.slug, status_str, progress, cost
                );
            }
            Err(e) => {
                debug!(error = %e, dir = %dir.display(), "Failed to load feature state");
                println!(
                    "{:<6} {:<30} {:<12} {:<10} {:<10}",
                    id, dir_name, "error", "-", "-"
                );
            }
        }
    }

    println!();
    println!("Total: {} feature(s)", feature_dirs.len());

    Ok(())
}
