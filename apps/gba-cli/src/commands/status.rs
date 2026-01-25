//! `gba status` command implementation.
//!
//! Shows detailed status of a specific feature.

use std::path::Path;

use anyhow::Result;
use tracing::debug;

use crate::error::CliError;
use crate::state::PhaseStatus;
use crate::utils::{format_cost, is_initialized, load_feature_state};

/// Run the `gba status` command.
///
/// Shows detailed status for a feature including:
/// - Feature information
/// - Git branch information
/// - Phase progress with statistics
/// - Total cost and token usage
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Feature is not found
pub async fn run_status(workdir: &Path, slug: &str) -> Result<()> {
    if !is_initialized(workdir) {
        return Err(CliError::NotInitialized.into());
    }

    debug!(workdir = %workdir.display(), slug = %slug, "Showing feature status");

    let state = load_feature_state(workdir, slug)?;

    // Header
    println!("Feature: {} ({})", state.feature.slug, state.feature.id);
    println!("{}", "=".repeat(60));
    println!();

    // Status
    let status_icon = match state.status {
        crate::state::FeatureStatus::Planned => "[.]",
        crate::state::FeatureStatus::InProgress => "[>]",
        crate::state::FeatureStatus::Completed => "[+]",
        crate::state::FeatureStatus::Failed => "[!]",
    };
    println!("Status: {} {}", status_icon, state.status);
    println!();

    // Git info
    println!("Git:");
    println!("  Branch: {}", state.git.branch);
    println!("  Base: {}", state.git.base_branch);
    println!("  Worktree: {}", state.git.worktree_path);
    println!();

    // Phases
    println!("Phases:");
    for (i, phase) in state.phases.iter().enumerate() {
        let icon = match phase.status {
            PhaseStatus::Pending => "[ ]",
            PhaseStatus::InProgress => "[>]",
            PhaseStatus::Completed => "[+]",
            PhaseStatus::Failed => "[!]",
        };

        let current = if i == state.current_phase { " <--" } else { "" };

        print!("  {} {}{}", icon, phase.name, current);

        if let Some(ref stats) = phase.stats {
            print!(
                " (turns: {}, cost: {})",
                stats.turns,
                format_cost(stats.cost_usd)
            );
        }

        println!();

        if let Some(ref sha) = phase.commit_sha {
            println!("      commit: {}", &sha[..7.min(sha.len())]);
        }
    }
    println!();

    // Total stats
    println!("Statistics:");
    println!("  Total turns: {}", state.total_stats.turns);
    println!("  Input tokens: {}", state.total_stats.input_tokens);
    println!("  Output tokens: {}", state.total_stats.output_tokens);
    println!("  Total cost: {}", format_cost(state.total_stats.cost_usd));
    println!();

    // Result
    if let Some(ref pr_url) = state.result.pr_url {
        println!("Pull Request:");
        println!("  URL: {}", pr_url);
        if let Some(pr_num) = state.result.pr_number {
            println!("  Number: #{}", pr_num);
        }
        println!(
            "  Merged: {}",
            if state.result.merged { "Yes" } else { "No" }
        );
        println!();
    }

    // Error
    if let Some(ref error) = state.error {
        println!("Error:");
        println!("  {}", error);
        println!();
    }

    // Timestamps
    println!("Timeline:");
    println!(
        "  Created: {}",
        state.feature.created_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "  Updated: {}",
        state.feature.updated_at.format("%Y-%m-%d %H:%M:%S")
    );

    Ok(())
}
