//! Implementation of the `gba run` command.
//!
//! This module implements the execution pipeline for a planned feature,
//! including phase execution, auto-commit, code review, verification,
//! and PR creation with resume support.

use std::path::Path;

use chrono::Utc;
use gba_core::event::PrintEventHandler;
use gba_core::{Engine, EngineConfig, Task, TaskKind};
use gba_pm::PromptManager;
use serde_json::json;
use tracing::{debug, info, warn};

use crate::error::CliError;
use crate::state::{FeatureState, FeatureStatus, PhaseStatus};
use crate::utils::{find_feature_dir, is_initialized, trees_dir};

/// Run options for the execution pipeline.
#[derive(Debug)]
pub struct RunOptions {
    /// Resume from a specific phase (0-indexed).
    pub from_phase: Option<usize>,

    /// Dry run mode (no commits or pushes).
    pub dry_run: bool,

    /// Restart execution from the beginning.
    pub restart: bool,
}

/// Execute a planned feature.
///
/// This function orchestrates the full execution pipeline:
/// 1. Load and validate feature state
/// 2. Detect resume point or start fresh
/// 3. Execute remaining phases with auto-commit
/// 4. Run code review
/// 5. Run verification
/// 6. Create pull request
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - The feature is not found
/// - Any phase execution fails
/// - Git operations fail
pub async fn run_run(workdir: &Path, slug: &str, options: RunOptions) -> Result<(), CliError> {
    // Check initialization
    if !is_initialized(workdir) {
        return Err(CliError::NotInitialized);
    }

    // Find and load feature state
    let feature_dir = find_feature_dir(workdir, slug)?;
    let mut state = FeatureState::load(&feature_dir)?;

    info!(
        feature_id = %state.feature.id,
        feature_slug = %state.feature.slug,
        status = %state.status,
        "loading feature"
    );

    // Validate feature status
    match state.status {
        FeatureStatus::Completed => {
            println!("Feature '{}' is already completed.", slug);
            if let Some(ref url) = state.result.pr_url {
                println!("PR: {}", url);
            }
            return Ok(());
        }
        FeatureStatus::Failed => {
            if !options.restart {
                println!("Feature '{}' previously failed.", slug);
                println!("Use --restart to start fresh or fix the issue manually.");
                if let Some(ref error) = state.error {
                    println!("Error: {}", error);
                }
                return Ok(());
            }
            // Reset for restart
            reset_state_for_restart(&mut state);
        }
        FeatureStatus::Planned | FeatureStatus::InProgress => {
            // Continue execution
        }
    }

    // Determine starting phase
    let start_phase = if options.restart {
        println!("Restarting execution from the beginning...");
        0
    } else if let Some(phase) = options.from_phase {
        println!("Starting from phase {} (manual override)...", phase);
        phase
    } else {
        detect_resume_point(&state)
    };

    // Validate start phase
    if start_phase >= state.phases.len() {
        return Err(CliError::InvalidState(format!(
            "phase {} does not exist (feature has {} phases)",
            start_phase,
            state.phases.len()
        )));
    }

    // Check if resuming
    let is_resuming = start_phase > 0 || state.status == FeatureStatus::InProgress;
    if is_resuming && !options.restart {
        println!(
            "Resuming execution from phase {} ({})...",
            start_phase, state.phases[start_phase].name
        );
    }

    // Verify worktree exists
    let worktree_path =
        trees_dir(workdir).join(format!("{}_{}", state.feature.id, state.feature.slug));
    if !worktree_path.exists() {
        return Err(CliError::InvalidState(format!(
            "worktree not found: {}",
            worktree_path.display()
        )));
    }

    // Update state to in_progress
    state.status = FeatureStatus::InProgress;
    state.feature.updated_at = Utc::now();
    state.save(&feature_dir)?;

    // Create engine
    let engine = create_engine(workdir)?;

    // Execute phases
    let result = execute_phases(
        &engine,
        workdir,
        &feature_dir,
        &mut state,
        start_phase,
        is_resuming,
        options.dry_run,
    )
    .await;

    match result {
        Ok(()) => {
            // All phases completed, run review and verification
            println!();
            println!("All phases completed. Running code review...");

            // Run code review
            let review_result = run_review(&engine, &state).await;
            match review_result {
                Ok(review_output) => {
                    println!();
                    println!("=== Code Review Result ===");
                    println!("{}", review_output);

                    // Check if review passed
                    if review_output.contains("APPROVED") {
                        println!();
                        println!("Code review: APPROVED");
                    } else if review_output.contains("NEEDS_CHANGES") {
                        println!();
                        println!("Code review: NEEDS_CHANGES");
                        println!("Please address the review feedback and run again.");

                        state.status = FeatureStatus::Failed;
                        state.error = Some("Code review requires changes".to_string());
                        state.feature.updated_at = Utc::now();
                        state.save(&feature_dir)?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    warn!("Code review failed: {}", e);
                    println!("Warning: Code review failed: {}", e);
                    println!("Continuing with verification...");
                }
            }

            // Run verification
            println!();
            println!("Running verification...");

            let verify_result = run_verification(&engine, &state).await;
            match verify_result {
                Ok(verify_output) => {
                    println!();
                    println!("=== Verification Result ===");
                    println!("{}", verify_output);

                    if verify_output.contains("FAILED") {
                        println!();
                        println!("Verification: FAILED");
                        state.status = FeatureStatus::Failed;
                        state.error = Some("Verification failed".to_string());
                        state.feature.updated_at = Utc::now();
                        state.save(&feature_dir)?;
                        return Ok(());
                    }

                    println!();
                    println!("Verification: PASSED");
                }
                Err(e) => {
                    warn!("Verification failed: {}", e);
                    println!("Warning: Verification failed: {}", e);
                }
            }

            // Create PR if not dry run
            if options.dry_run {
                println!();
                println!("Dry run complete. Skipping PR creation.");
            } else {
                println!();
                println!("Creating pull request...");

                match create_pull_request(workdir, &mut state).await {
                    Ok(pr_url) => {
                        println!("PR created: {}", pr_url);
                        state.result.pr_url = Some(pr_url);
                        state.status = FeatureStatus::Completed;
                    }
                    Err(e) => {
                        warn!("Failed to create PR: {}", e);
                        println!("Warning: Failed to create PR: {}", e);
                        println!("You can create the PR manually.");
                    }
                }
            }

            // Update final state
            state.feature.updated_at = Utc::now();
            if state.status != FeatureStatus::Completed {
                state.status = FeatureStatus::Completed;
            }
            state.save(&feature_dir)?;

            // Print summary
            println!();
            println!("=== Execution Summary ===");
            println!("Feature: {} ({})", state.feature.slug, state.feature.id);
            println!("Phases completed: {}", state.phases.len());
            println!("Total cost: ${:.2}", state.total_stats.cost_usd);
            println!("Total turns: {}", state.total_stats.turns);
            if let Some(ref url) = state.result.pr_url {
                println!("PR: {}", url);
            }
        }
        Err(e) => {
            // Save failed state
            state.status = FeatureStatus::Failed;
            state.error = Some(e.to_string());
            state.feature.updated_at = Utc::now();
            state.save(&feature_dir)?;

            return Err(e);
        }
    }

    Ok(())
}

/// Reset state for a restart.
fn reset_state_for_restart(state: &mut FeatureState) {
    state.status = FeatureStatus::Planned;
    state.current_phase = 0;
    state.error = None;

    for phase in &mut state.phases {
        phase.status = PhaseStatus::Pending;
        phase.started_at = None;
        phase.completed_at = None;
        phase.commit_sha = None;
        phase.stats = None;
    }

    state.total_stats = crate::state::TaskStats::default();
    state.result = crate::state::FeatureResult::default();
}

/// Detect the resume point based on phase status.
fn detect_resume_point(state: &FeatureState) -> usize {
    // Find the first incomplete phase
    for (i, phase) in state.phases.iter().enumerate() {
        match phase.status {
            PhaseStatus::Pending | PhaseStatus::InProgress | PhaseStatus::Failed => {
                return i;
            }
            PhaseStatus::Completed => continue,
        }
    }

    // All phases complete, return the last phase index
    state.phases.len().saturating_sub(1)
}

/// Create the GBA engine.
///
/// The `PromptManager` only contains owned data (templates loaded from files),
/// so it can safely be `'static`. No unsafe code is needed.
fn create_engine(workdir: &Path) -> Result<Engine<'static>, CliError> {
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
        .workdir(workdir)
        .prompts(prompts)
        .build();

    let engine = Engine::new(config)?;
    Ok(engine)
}

/// Execute the remaining phases.
async fn execute_phases(
    engine: &Engine<'_>,
    workdir: &Path,
    feature_dir: &Path,
    state: &mut FeatureState,
    start_phase: usize,
    is_resuming: bool,
    dry_run: bool,
) -> Result<(), CliError> {
    let total_phases = state.phases.len();

    for phase_idx in start_phase..total_phases {
        let phase_name = state.phases[phase_idx].name.clone();

        println!();
        println!(
            "[{}/{}] Executing phase: {}",
            phase_idx + 1,
            total_phases,
            phase_name
        );

        // Mark phase as in progress
        state.current_phase = phase_idx;
        state.phases[phase_idx].status = PhaseStatus::InProgress;
        state.phases[phase_idx].started_at = Some(Utc::now());
        state.feature.updated_at = Utc::now();
        state.save(feature_dir)?;

        // Build context for the execute task
        let phases_context: Vec<serde_json::Value> = state
            .phases
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "status": format!("{}", p.status),
                    "commit_sha": p.commit_sha,
                })
            })
            .collect();

        let context = json!({
            "repo_path": workdir.display().to_string(),
            "feature_id": state.feature.id,
            "feature_slug": state.feature.slug,
            "is_resuming": is_resuming && phase_idx == start_phase,
            "phases": phases_context,
        });

        // Create and run the execute task
        let task = Task::new(TaskKind::Execute, context);
        let mut handler = PrintEventHandler::new().with_auto_flush();

        let result = engine.run_stream(task, &mut handler).await?;

        if !result.success {
            state.phases[phase_idx].status = PhaseStatus::Failed;
            state.feature.updated_at = Utc::now();
            state.save(feature_dir)?;

            return Err(CliError::InvalidState(format!(
                "phase '{}' execution failed",
                phase_name
            )));
        }

        // Get commit SHA if auto-commit was done
        let commit_sha = if !dry_run {
            get_latest_commit_sha(workdir, state)?
        } else {
            None
        };

        // Update phase status
        state.phases[phase_idx].status = PhaseStatus::Completed;
        state.phases[phase_idx].completed_at = Some(Utc::now());
        state.phases[phase_idx].commit_sha = commit_sha;
        state.phases[phase_idx].stats = Some(crate::state::TaskStats {
            turns: result.stats.turns,
            input_tokens: result.stats.input_tokens,
            output_tokens: result.stats.output_tokens,
            cost_usd: result.stats.cost_usd,
        });

        // Update total stats
        state.total_stats.turns += result.stats.turns;
        state.total_stats.input_tokens += result.stats.input_tokens;
        state.total_stats.output_tokens += result.stats.output_tokens;
        state.total_stats.cost_usd += result.stats.cost_usd;

        state.feature.updated_at = Utc::now();
        state.save(feature_dir)?;

        println!("[✓] Phase '{}' completed", phase_name);
    }

    Ok(())
}

/// Get the latest commit SHA from the worktree.
fn get_latest_commit_sha(workdir: &Path, state: &FeatureState) -> Result<Option<String>, CliError> {
    let worktree_path =
        trees_dir(workdir).join(format!("{}_{}", state.feature.id, state.feature.slug));

    let output = std::process::Command::new("git")
        .current_dir(&worktree_path)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| CliError::Git(format!("failed to get commit SHA: {}", e)))?;

    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Some(sha))
    } else {
        Ok(None)
    }
}

/// Run code review.
async fn run_review(engine: &Engine<'_>, state: &FeatureState) -> Result<String, CliError> {
    let context = json!({
        "feature_id": state.feature.id,
        "feature_slug": state.feature.slug,
    });

    let task = Task::new(TaskKind::Review, context);
    let result = engine.run(task).await?;

    Ok(result.output)
}

/// Run verification.
async fn run_verification(engine: &Engine<'_>, state: &FeatureState) -> Result<String, CliError> {
    let context = json!({
        "feature_id": state.feature.id,
        "feature_slug": state.feature.slug,
    });

    let task = Task::new(TaskKind::Verification, context);
    let result = engine.run(task).await?;

    Ok(result.output)
}

/// Create a pull request using gh CLI.
async fn create_pull_request(workdir: &Path, state: &mut FeatureState) -> Result<String, CliError> {
    let worktree_path =
        trees_dir(workdir).join(format!("{}_{}", state.feature.id, state.feature.slug));

    let branch = &state.git.branch;
    let base_branch = &state.git.base_branch;

    // Push the branch
    debug!("pushing branch {}", branch);
    let push_output = std::process::Command::new("git")
        .current_dir(&worktree_path)
        .args(["push", "-u", "origin", branch])
        .output()
        .map_err(|e| CliError::Git(format!("failed to push: {}", e)))?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        return Err(CliError::Git(format!("git push failed: {}", stderr)));
    }

    // Create PR using gh CLI
    let title = format!("feat({}): implementation", state.feature.slug);
    let body = format!(
        r#"## Summary

Implementation of feature `{}`.

## Phases Completed

{}

## Statistics

- Total turns: {}
- Total cost: ${:.2}
- Input tokens: {}
- Output tokens: {}

---

Generated by GBA (Geektime Bootcamp Agent)
"#,
        state.feature.slug,
        state
            .phases
            .iter()
            .map(|p| format!("- [x] {}", p.name))
            .collect::<Vec<_>>()
            .join("\n"),
        state.total_stats.turns,
        state.total_stats.cost_usd,
        state.total_stats.input_tokens,
        state.total_stats.output_tokens,
    );

    debug!("creating PR with gh cli");
    let pr_output = std::process::Command::new("gh")
        .current_dir(&worktree_path)
        .args([
            "pr",
            "create",
            "--base",
            base_branch,
            "--head",
            branch,
            "--title",
            &title,
            "--body",
            &body,
        ])
        .output()
        .map_err(|e| CliError::Git(format!("failed to create PR: {}", e)))?;

    if !pr_output.status.success() {
        let stderr = String::from_utf8_lossy(&pr_output.stderr);
        return Err(CliError::Git(format!("gh pr create failed: {}", stderr)));
    }

    let pr_url = String::from_utf8_lossy(&pr_output.stdout)
        .trim()
        .to_string();

    // Extract PR number from URL
    if let Some(number) = pr_url
        .split('/')
        .next_back()
        .and_then(|s| s.parse::<u32>().ok())
    {
        state.result.pr_number = Some(number);
    }

    Ok(pr_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FeatureInfo, FeatureResult, GitState, PhaseState};

    fn create_test_state() -> FeatureState {
        FeatureState {
            feature: FeatureInfo {
                id: "0001".to_string(),
                slug: "test-feature".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            status: FeatureStatus::InProgress,
            current_phase: 1,
            git: GitState {
                worktree_path: ".trees/0001_test-feature".to_string(),
                branch: "feature/0001-test-feature".to_string(),
                base_branch: "main".to_string(),
            },
            phases: vec![
                PhaseState {
                    name: "setup".to_string(),
                    status: PhaseStatus::Completed,
                    started_at: Some(Utc::now()),
                    completed_at: Some(Utc::now()),
                    commit_sha: Some("abc1234".to_string()),
                    stats: None,
                },
                PhaseState {
                    name: "implementation".to_string(),
                    status: PhaseStatus::InProgress,
                    started_at: Some(Utc::now()),
                    completed_at: None,
                    commit_sha: None,
                    stats: None,
                },
                PhaseState {
                    name: "testing".to_string(),
                    status: PhaseStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    commit_sha: None,
                    stats: None,
                },
            ],
            total_stats: crate::state::TaskStats::default(),
            result: FeatureResult::default(),
            error: None,
        }
    }

    #[test]
    fn test_detect_resume_point_finds_in_progress() {
        let state = create_test_state();
        let resume_point = detect_resume_point(&state);
        assert_eq!(resume_point, 1); // Should find the in_progress phase
    }

    #[test]
    fn test_detect_resume_point_finds_pending() {
        let mut state = create_test_state();
        state.phases[1].status = PhaseStatus::Completed;

        let resume_point = detect_resume_point(&state);
        assert_eq!(resume_point, 2); // Should find the pending phase
    }

    #[test]
    fn test_detect_resume_point_all_completed() {
        let mut state = create_test_state();
        for phase in &mut state.phases {
            phase.status = PhaseStatus::Completed;
        }

        let resume_point = detect_resume_point(&state);
        assert_eq!(resume_point, 2); // Should return last phase index
    }

    #[test]
    fn test_reset_state_for_restart() {
        let mut state = create_test_state();
        state.status = FeatureStatus::Failed;
        state.error = Some("test error".to_string());
        state.total_stats.turns = 10;

        reset_state_for_restart(&mut state);

        assert_eq!(state.status, FeatureStatus::Planned);
        assert_eq!(state.current_phase, 0);
        assert!(state.error.is_none());
        assert_eq!(state.total_stats.turns, 0);

        for phase in &state.phases {
            assert_eq!(phase.status, PhaseStatus::Pending);
            assert!(phase.started_at.is_none());
            assert!(phase.completed_at.is_none());
            assert!(phase.commit_sha.is_none());
        }
    }
}
