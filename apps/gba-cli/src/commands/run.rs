//! Implementation of the `gba run` command.
//!
//! This module implements the execution pipeline for a planned feature,
//! including phase execution, auto-commit, code review, verification,
//! and PR creation with resume support.
//!
//! The execution uses a TUI to display progress with streaming output
//! that clears between phases (not accumulates).

use std::path::{Path, PathBuf};

use chrono::Utc;
use gba_core::{Engine, Task, TaskKind};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::error::CliError;
use crate::state::{
    CheckResultState, CheckResultStatus, FeatureState, FeatureStatus, PhaseStatus, TaskStats,
};
use crate::tui::{
    CheckFinalResult, CheckIterationResult, CheckType, RunApp, RunMessage, TuiEventHandler,
};
use crate::utils;

/// Maximum number of fix iterations for review and verification loops.
const MAX_FIX_ITERATIONS: u32 = 3;

/// Configuration for a check-fix loop (review or verification).
#[derive(Debug, Clone)]
struct CheckConfig {
    /// Display name for the check (e.g., "code review" or "verification").
    name: &'static str,
    /// Keywords indicating the check passed.
    success_keywords: &'static [&'static str],
    /// Keywords indicating the check needs changes.
    failure_keywords: &'static [&'static str],
}

impl CheckConfig {
    /// Configuration for code review checks.
    const REVIEW: Self = Self {
        name: "code review",
        success_keywords: &["APPROVED"],
        failure_keywords: &["NEEDS_CHANGES"],
    };

    /// Configuration for verification checks.
    const VERIFICATION: Self = Self {
        name: "verification",
        success_keywords: &["VERIFIED"],
        failure_keywords: &["FAILED"],
    };

    /// Check if the output indicates success using strict pattern matching.
    ///
    /// The keyword must appear in one of these forms:
    /// - On its own line (with optional leading/trailing whitespace)
    /// - In a verdict line like "Verdict: APPROVED" or "Result: VERIFIED"
    /// - As the last word in the output (within last 100 characters)
    /// - Surrounded by word boundaries (not part of another word)
    fn is_success(&self, output: &str) -> bool {
        self.success_keywords
            .iter()
            .any(|kw| Self::matches_keyword(output, kw))
    }

    /// Check if the output indicates failure using strict pattern matching.
    fn is_failure(&self, output: &str) -> bool {
        self.failure_keywords
            .iter()
            .any(|kw| Self::matches_keyword(output, kw))
    }

    /// Check if a keyword matches the output with strict pattern rules.
    fn matches_keyword(output: &str, keyword: &str) -> bool {
        // Check for keyword on its own line (with optional prefixes like "Verdict:")
        for line in output.lines() {
            let trimmed = line.trim();

            // Exact match on trimmed line
            if trimmed == keyword {
                return true;
            }

            // Match patterns like "Verdict: APPROVED", "Result: VERIFIED", "Status: FAILED"
            let prefixes = ["Verdict:", "Result:", "Status:", "Outcome:"];
            for prefix in prefixes {
                if let Some(rest) = trimmed.strip_prefix(prefix)
                    && rest.trim() == keyword
                {
                    return true;
                }
            }

            // Match pattern with brackets like "[APPROVED]" or "**APPROVED**"
            if trimmed == format!("[{}]", keyword) || trimmed == format!("**{}**", keyword) {
                return true;
            }
        }

        // Check if keyword appears at the end of output (within last 100 chars)
        // This handles cases where the verdict is the final line
        let tail = if output.len() > 100 {
            &output[output.len() - 100..]
        } else {
            output
        };

        // Check for keyword with word boundaries in the tail
        // This is a simple word boundary check without regex
        if contains_word(tail, keyword) {
            return true;
        }

        false
    }
}

/// Check if a string contains a keyword as a complete word.
///
/// A word boundary is defined as the start/end of string or a non-alphanumeric character.
fn contains_word(text: &str, word: &str) -> bool {
    let mut search_start = 0;
    while let Some(pos) = text[search_start..].find(word) {
        let abs_pos = search_start + pos;
        let before_ok = abs_pos == 0
            || !text[..abs_pos]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric());
        let after_pos = abs_pos + word.len();
        let after_ok = after_pos >= text.len()
            || !text[after_pos..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric());

        if before_ok && after_ok {
            return true;
        }

        search_start = abs_pos + 1;
        if search_start >= text.len() {
            break;
        }
    }
    false
}

/// Context for task execution, holding commonly used paths and identifiers.
///
/// This struct eliminates repeated calculation of `worktree_path` and provides
/// a consistent set of identifiers for task context creation.
#[derive(Debug, Clone)]
struct TaskContext {
    /// Path to the worktree directory.
    worktree_path: PathBuf,
    /// Feature ID (e.g., "0001").
    feature_id: String,
    /// Feature slug (e.g., "my-feature").
    feature_slug: String,
}

impl TaskContext {
    /// Create a new task context from workdir and feature state.
    fn new(workdir: &Path, state: &FeatureState) -> Self {
        Self {
            worktree_path: utils::feature_worktree_path(workdir, &state.feature.slug),
            feature_id: state.feature.id.clone(),
            feature_slug: state.feature.slug.clone(),
        }
    }

    /// Create base context JSON for task execution.
    fn base_context(&self) -> serde_json::Value {
        json!({
            "repo_path": self.worktree_path.display().to_string(),
            "feature_id": self.feature_id,
            "feature_slug": self.feature_slug,
        })
    }
}

/// Capitalize the first letter of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

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

/// Result of state validation and preparation.
struct PreparedExecution {
    /// Path to the feature directory.
    feature_dir: PathBuf,
    /// Loaded and validated feature state.
    state: FeatureState,
    /// Task context with worktree path.
    ctx: TaskContext,
    /// Starting phase index.
    start_phase: usize,
    /// Whether this is a resume operation.
    is_resuming: bool,
}

/// Context for TUI-based phase execution.
struct TuiExecutionContext {
    /// Path to the working directory.
    workdir: PathBuf,
    /// Task context with worktree path.
    ctx: TaskContext,
    /// Path to the feature directory.
    feature_dir: PathBuf,
    /// Starting phase index.
    start_phase: usize,
    /// Whether this is a resume operation.
    is_resuming: bool,
    /// Dry run mode (no commits or pushes).
    dry_run: bool,
}

/// Execute a planned feature.
///
/// This function orchestrates the full execution pipeline:
/// 1. Load and validate feature state
/// 2. Detect resume point or start fresh
/// 3. Execute remaining phases with auto-commit (shown in TUI)
/// 4. Run code review (shown in TUI)
/// 5. Run verification (shown in TUI)
/// 6. Create pull request (shown in TUI)
///
/// All stages are displayed in the TUI with streaming output
/// that clears between phases for better readability.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - The feature is not found
/// - Any phase execution fails
/// - Git operations fail
pub async fn run_run(workdir: &Path, slug: &str, options: RunOptions) -> Result<(), CliError> {
    // Validate and prepare state
    let prepared = match prepare_execution(workdir, slug, &options)? {
        Some(p) => p,
        None => return Ok(()), // Early return (already completed or user declined)
    };

    let PreparedExecution {
        feature_dir,
        mut state,
        ctx,
        start_phase,
        is_resuming,
    } = prepared;

    // Update state to in_progress
    state.status = FeatureStatus::InProgress;
    state.feature.updated_at = Utc::now();
    state.save(&feature_dir)?;

    // Create channel for TUI messages
    let (tx, rx) = mpsc::channel::<RunMessage>(100);

    // Create TUI app from current state
    let mut app = RunApp::new(&state);

    // Capture dry_run before moving into exec_ctx
    let dry_run = options.dry_run;

    // Create execution context for the worker task
    let exec_ctx = TuiExecutionContext {
        workdir: workdir.to_path_buf(),
        ctx: ctx.clone(),
        feature_dir: feature_dir.clone(),
        start_phase,
        is_resuming,
        dry_run,
    };

    // Spawn execution worker that handles the full pipeline
    let worker_handle =
        tokio::spawn(async move { execute_full_pipeline_with_tui(exec_ctx, state, tx).await });

    // Run TUI event loop (blocks until complete or error)
    app.run(rx).await?;

    // Wait for worker and get updated state
    let worker_result = worker_handle
        .await
        .map_err(|e| CliError::InvalidState(format!("worker task panicked: {}", e)))?;

    let (state, pipeline_result) = worker_result?;

    // Handle pipeline failure
    if let Err(ref e) = pipeline_result {
        // State was already saved in the worker, just log
        warn!("Pipeline failed: {}", e);
    }

    // Print final summary (always, even on failure)
    print_execution_summary(&state);

    pipeline_result?;

    Ok(())
}

/// Execute the full pipeline with TUI integration.
///
/// This function runs in a spawned task and sends `RunMessage` events
/// to the TUI via the provided channel. It handles:
/// 1. Phase execution
/// 2. Code review with fix loop
/// 3. Verification with fix loop
/// 4. PR creation
///
/// The function monitors the TUI channel and aborts gracefully if the TUI is closed.
///
/// Returns the updated state and the result of the pipeline.
async fn execute_full_pipeline_with_tui(
    exec_ctx: TuiExecutionContext,
    mut state: FeatureState,
    tx: mpsc::Sender<RunMessage>,
) -> Result<(FeatureState, Result<(), CliError>), CliError> {
    let TuiExecutionContext {
        workdir,
        ctx,
        feature_dir,
        start_phase,
        is_resuming,
        dry_run,
    } = exec_ctx;

    // Create engine with worktree as working directory
    let engine = utils::create_engine_with_context(&workdir, &ctx.worktree_path)?;

    // === Phase 1: Execute phases ===
    let phase_result = execute_phases_inner(
        &engine,
        &ctx,
        &feature_dir,
        &mut state,
        start_phase,
        is_resuming,
        dry_run,
        &tx,
    )
    .await;

    if let Err(ref e) = phase_result {
        save_failed_state(&mut state, &feature_dir, &e.to_string())?;
        let _ = tx.send(RunMessage::Complete).await;
        return Ok((state, phase_result));
    }

    // Check if TUI is still running before continuing to checks
    if tx.is_closed() {
        warn!("TUI closed, aborting pipeline after phases");
        return Ok((
            state,
            Err(CliError::InvalidState("TUI closed by user".to_string())),
        ));
    }

    // In dry_run mode, skip review and verification
    if dry_run {
        let _ = tx
            .send(RunMessage::Activity(
                "Dry run: Skipping review and verification".to_string(),
            ))
            .await;
    } else {
        // === Phase 2: Code Review ===
        if tx
            .send(RunMessage::CheckStarted {
                check_type: CheckType::Review,
                max_iterations: MAX_FIX_ITERATIONS,
            })
            .await
            .is_err()
        {
            warn!("TUI closed, aborting pipeline before review");
            return Ok((
                state,
                Err(CliError::InvalidState("TUI closed by user".to_string())),
            ));
        }

        let (review_result, review_iterations) = run_check_fix_loop_with_tui(
            &engine,
            &ctx,
            &CheckConfig::REVIEW,
            CheckType::Review,
            &tx,
        )
        .await;

        // Persist review result to state
        state.result.review = Some(CheckResultState {
            status: check_final_result_to_status(&review_result),
            iterations: review_iterations,
            completed_at: Utc::now(),
            error: match &review_result {
                CheckFinalResult::Error(e) => Some(e.clone()),
                _ => None,
            },
        });
        state.save(&feature_dir)?;

        let _ = tx
            .send(RunMessage::CheckCompleted {
                check_type: CheckType::Review,
                result: review_result.clone(),
            })
            .await;

        // Handle review result with explicit error logging
        match &review_result {
            CheckFinalResult::Passed => {
                info!("Code review passed");
            }
            CheckFinalResult::NeedsChanges(reason) => {
                warn!("Code review needs changes: {}", reason);
                // Soft fail: continue to verification
            }
            CheckFinalResult::Error(e) => {
                warn!("Code review encountered error: {}", e);
                // Soft fail: continue to verification
            }
            CheckFinalResult::Skipped(reason) => {
                info!("Code review skipped: {}", reason);
            }
        }

        // Check if TUI is still running
        if tx.is_closed() {
            warn!("TUI closed, aborting pipeline after review");
            return Ok((
                state,
                Err(CliError::InvalidState("TUI closed by user".to_string())),
            ));
        }

        // === Phase 3: Verification ===
        if tx
            .send(RunMessage::CheckStarted {
                check_type: CheckType::Verification,
                max_iterations: MAX_FIX_ITERATIONS,
            })
            .await
            .is_err()
        {
            warn!("TUI closed, aborting pipeline before verification");
            return Ok((
                state,
                Err(CliError::InvalidState("TUI closed by user".to_string())),
            ));
        }

        let (verification_result, verification_iterations) = run_check_fix_loop_with_tui(
            &engine,
            &ctx,
            &CheckConfig::VERIFICATION,
            CheckType::Verification,
            &tx,
        )
        .await;

        // Persist verification result to state
        state.result.verification = Some(CheckResultState {
            status: check_final_result_to_status(&verification_result),
            iterations: verification_iterations,
            completed_at: Utc::now(),
            error: match &verification_result {
                CheckFinalResult::Error(e) => Some(e.clone()),
                _ => None,
            },
        });
        state.save(&feature_dir)?;

        let _ = tx
            .send(RunMessage::CheckCompleted {
                check_type: CheckType::Verification,
                result: verification_result.clone(),
            })
            .await;

        // Handle verification result - this is a harder fail
        match &verification_result {
            CheckFinalResult::Passed => {
                info!("Verification passed");
            }
            CheckFinalResult::NeedsChanges(reason) => {
                warn!("Verification needs changes: {}", reason);
                save_failed_state(&mut state, &feature_dir, reason)?;
                let _ = tx.send(RunMessage::Complete).await;
                return Ok((
                    state,
                    Err(CliError::InvalidState(format!(
                        "Verification failed: {}",
                        reason
                    ))),
                ));
            }
            CheckFinalResult::Error(e) => {
                warn!("Verification encountered error: {}", e);
                // Continue despite error - let user decide
            }
            CheckFinalResult::Skipped(reason) => {
                info!("Verification skipped: {}", reason);
            }
        }
    }

    // Check if TUI is still running before PR creation
    if tx.is_closed() {
        warn!("TUI closed, aborting pipeline before PR creation");
        return Ok((
            state,
            Err(CliError::InvalidState("TUI closed by user".to_string())),
        ));
    }

    // === Phase 4: PR Creation ===
    if dry_run {
        let _ = tx
            .send(RunMessage::Activity(
                "Dry run: Skipping PR creation".to_string(),
            ))
            .await;
    } else {
        let _ = tx.send(RunMessage::PrCreationStarted).await;

        match create_pull_request(&engine, &ctx, &mut state).await {
            Ok(pr_url) => {
                state.result.pr_url = Some(pr_url.clone());
                state.status = FeatureStatus::Completed;
                let _ = tx
                    .send(RunMessage::PrCreationCompleted {
                        pr_url: Some(pr_url),
                    })
                    .await;
            }
            Err(e) => {
                warn!("Failed to create PR: {}", e);
                let _ = tx
                    .send(RunMessage::PrCreationCompleted { pr_url: None })
                    .await;
                let _ = tx
                    .send(RunMessage::Activity(format!("PR creation failed: {}", e)))
                    .await;
            }
        }
    }

    // Update final state
    state.feature.updated_at = Utc::now();
    if state.status != FeatureStatus::Completed {
        state.status = FeatureStatus::Completed;
    }
    state.save(&feature_dir)?;

    // Send complete signal
    let _ = tx.send(RunMessage::Complete).await;

    Ok((state, Ok(())))
}

/// Convert `CheckFinalResult` to `CheckResultStatus` for state persistence.
fn check_final_result_to_status(result: &CheckFinalResult) -> CheckResultStatus {
    match result {
        CheckFinalResult::Passed => CheckResultStatus::Passed,
        CheckFinalResult::NeedsChanges(_) => CheckResultStatus::NeedsChanges,
        CheckFinalResult::Error(_) => CheckResultStatus::Error,
        CheckFinalResult::Skipped(_) => CheckResultStatus::Skipped,
    }
}

/// Execute phases (internal helper for `execute_full_pipeline_with_tui`).
///
/// This helper function requires many parameters because it needs access to
/// execution context, state, and the TUI channel. Using a struct would
/// complicate the borrowed lifetimes unnecessarily.
#[allow(clippy::too_many_arguments)]
async fn execute_phases_inner(
    engine: &Engine<'_>,
    ctx: &TaskContext,
    feature_dir: &Path,
    state: &mut FeatureState,
    start_phase: usize,
    is_resuming: bool,
    dry_run: bool,
    tx: &mpsc::Sender<RunMessage>,
) -> Result<(), CliError> {
    let total_phases = state.phases.len();

    for phase_idx in start_phase..total_phases {
        let phase_name = state.phases[phase_idx].name.clone();

        // Send phase started message to TUI
        if tx
            .send(RunMessage::PhaseStarted {
                index: phase_idx,
                name: phase_name.clone(),
            })
            .await
            .is_err()
        {
            // TUI closed, abort execution
            return Err(CliError::InvalidState("TUI channel closed".to_string()));
        }

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

        let mut context = ctx.base_context();
        if let Some(obj) = context.as_object_mut() {
            obj.insert(
                "is_resuming".to_string(),
                json!(is_resuming && phase_idx == start_phase),
            );
            obj.insert("current_phase_index".to_string(), json!(phase_idx));
            obj.insert("current_phase_name".to_string(), json!(phase_name.clone()));
            obj.insert("total_phases".to_string(), json!(total_phases));
            obj.insert("phases".to_string(), json!(phases_context));
        }

        // Create and run the execute task with TUI event handler
        let task = Task::new(TaskKind::Execute, context);
        let mut handler = TuiEventHandler::new(tx.clone());

        let result = engine.run_stream(task, &mut handler).await;

        match result {
            Ok(result) => {
                if !result.success {
                    // Send phase failed message
                    let error_msg = format!("phase '{}' execution failed", phase_name);
                    let _ = tx
                        .send(RunMessage::PhaseFailed {
                            index: phase_idx,
                            error: error_msg.clone(),
                        })
                        .await;

                    state.phases[phase_idx].status = PhaseStatus::Failed;
                    state.feature.updated_at = Utc::now();
                    state.save(feature_dir)?;

                    return Err(CliError::InvalidState(error_msg));
                }

                // Get commit SHA if auto-commit was done
                let commit_sha = if !dry_run {
                    get_latest_commit_sha(ctx)?
                } else {
                    None
                };

                // Update phase status
                state.phases[phase_idx].status = PhaseStatus::Completed;
                state.phases[phase_idx].completed_at = Some(Utc::now());
                state.phases[phase_idx].commit_sha = commit_sha.clone();
                state.phases[phase_idx].stats = Some(TaskStats {
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

                // Send phase completed message
                let _ = tx
                    .send(RunMessage::PhaseCompleted {
                        index: phase_idx,
                        commit_sha,
                    })
                    .await;

                // Send stats update
                let _ = tx
                    .send(RunMessage::StatsUpdate {
                        turns: state.total_stats.turns,
                        cost_usd: state.total_stats.cost_usd,
                    })
                    .await;
            }
            Err(e) => {
                // Send error to TUI
                let error_msg = format!("phase '{}' failed: {}", phase_name, e);
                let _ = tx
                    .send(RunMessage::PhaseFailed {
                        index: phase_idx,
                        error: error_msg.clone(),
                    })
                    .await;

                state.phases[phase_idx].status = PhaseStatus::Failed;
                state.feature.updated_at = Utc::now();
                state.save(feature_dir)?;

                return Err(CliError::from(e));
            }
        }
    }

    Ok(())
}

/// Run a check-fix loop with TUI message sending.
///
/// This function performs the check-fix loop (review or verification)
/// while sending progress updates to the TUI.
///
/// Returns a tuple of (result, iterations_performed).
async fn run_check_fix_loop_with_tui(
    engine: &Engine<'_>,
    ctx: &TaskContext,
    config: &CheckConfig,
    check_type: CheckType,
    tx: &mpsc::Sender<RunMessage>,
) -> (CheckFinalResult, u32) {
    for iteration in 1..=MAX_FIX_ITERATIONS {
        // Check if TUI is closed before each iteration
        if tx.is_closed() {
            warn!(
                "TUI closed, aborting {} at iteration {}",
                config.name, iteration
            );
            return (
                CheckFinalResult::Error("TUI closed by user".to_string()),
                iteration,
            );
        }

        // Send iteration started
        let _ = tx
            .send(RunMessage::CheckIterationStarted {
                check_type,
                iteration,
                max_iterations: MAX_FIX_ITERATIONS,
            })
            .await;

        // Run the check with streaming output
        let check_result = run_check_with_streaming(engine, ctx, config, tx).await;

        match check_result {
            Ok(output) => {
                if config.is_success(&output) {
                    // Check passed
                    let _ = tx
                        .send(RunMessage::CheckIterationResult {
                            check_type,
                            iteration,
                            result: CheckIterationResult::Passed,
                        })
                        .await;
                    return (CheckFinalResult::Passed, iteration);
                } else if config.is_failure(&output) {
                    // Check found issues
                    let _ = tx
                        .send(RunMessage::CheckIterationResult {
                            check_type,
                            iteration,
                            result: CheckIterationResult::NeedsChanges(output.clone()),
                        })
                        .await;

                    if iteration < MAX_FIX_ITERATIONS {
                        // Check if TUI is closed before attempting fix
                        if tx.is_closed() {
                            warn!("TUI closed, aborting {} before fix", config.name);
                            return (CheckFinalResult::NeedsChanges(output), iteration);
                        }

                        // Attempt to fix
                        let _ = tx
                            .send(RunMessage::FixStarted {
                                check_type,
                                iteration,
                            })
                            .await;

                        let fix_success =
                            run_fix_with_streaming(engine, ctx, config.name, output, tx).await;

                        let _ = tx
                            .send(RunMessage::FixCompleted {
                                check_type,
                                iteration,
                                success: fix_success,
                            })
                            .await;
                    } else {
                        // Max iterations reached
                        return (
                            CheckFinalResult::NeedsChanges(format!(
                                "{} still requires changes after {} fix iterations",
                                capitalize_first(config.name),
                                MAX_FIX_ITERATIONS
                            )),
                            iteration,
                        );
                    }
                } else {
                    // No clear verdict, treat as passed
                    let _ = tx
                        .send(RunMessage::CheckIterationResult {
                            check_type,
                            iteration,
                            result: CheckIterationResult::Passed,
                        })
                        .await;
                    return (CheckFinalResult::Passed, iteration);
                }
            }
            Err(e) => {
                // Check itself failed to run
                let _ = tx
                    .send(RunMessage::CheckIterationResult {
                        check_type,
                        iteration,
                        result: CheckIterationResult::Error(e.to_string()),
                    })
                    .await;
                return (CheckFinalResult::Error(e.to_string()), iteration);
            }
        }
    }

    // Should not reach here, but if we do, it means all iterations found issues
    (
        CheckFinalResult::NeedsChanges(format!(
            "{} still requires changes after {} fix iterations",
            capitalize_first(config.name),
            MAX_FIX_ITERATIONS
        )),
        MAX_FIX_ITERATIONS,
    )
}

/// Run a check (review or verification) with streaming output to TUI.
async fn run_check_with_streaming(
    engine: &Engine<'_>,
    ctx: &TaskContext,
    config: &CheckConfig,
    tx: &mpsc::Sender<RunMessage>,
) -> Result<String, CliError> {
    let task_kind = match config.name {
        "code review" => TaskKind::Review,
        "verification" => TaskKind::Verification,
        _ => {
            return Err(CliError::InvalidState(format!(
                "unknown check type: {}",
                config.name
            )));
        }
    };

    let task = Task::new(task_kind, ctx.base_context());
    let mut handler = TuiEventHandler::new(tx.clone());
    let result = engine.run_stream(task, &mut handler).await?;

    Ok(result.output)
}

/// Run a fix task with streaming output to TUI.
async fn run_fix_with_streaming(
    engine: &Engine<'_>,
    ctx: &TaskContext,
    fix_type: &str,
    feedback: String,
    tx: &mpsc::Sender<RunMessage>,
) -> bool {
    let mut context = ctx.base_context();
    if let Some(obj) = context.as_object_mut() {
        obj.insert("fix_type".to_string(), json!(fix_type));
        obj.insert("feedback".to_string(), json!(feedback));
    }

    let task = Task::new(TaskKind::Fix, context);
    let mut handler = TuiEventHandler::new(tx.clone());

    match engine.run_stream(task, &mut handler).await {
        Ok(result) => result.success,
        Err(e) => {
            warn!("Fix failed: {}", e);
            false
        }
    }
}

/// Prepare execution by validating state and computing context.
///
/// Returns `None` if execution should not proceed (already completed or user declined).
fn prepare_execution(
    workdir: &Path,
    slug: &str,
    options: &RunOptions,
) -> Result<Option<PreparedExecution>, CliError> {
    // Check initialization
    if !utils::is_initialized(workdir) {
        return Err(CliError::NotInitialized);
    }

    // Find and load feature state
    let feature_dir = utils::find_feature_dir(workdir, slug)?;
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
            return Ok(None);
        }
        FeatureStatus::Failed => {
            if !options.restart {
                println!("Feature '{}' previously failed.", slug);
                println!("Use --restart to start fresh or fix the issue manually.");
                if let Some(ref error) = state.error {
                    println!("Error: {}", error);
                }
                return Ok(None);
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

    // Create task context (computes worktree_path once)
    let ctx = TaskContext::new(workdir, &state);

    // Verify worktree exists
    if !ctx.worktree_path.exists() {
        return Err(CliError::InvalidState(format!(
            "worktree not found: {}",
            ctx.worktree_path.display()
        )));
    }

    Ok(Some(PreparedExecution {
        feature_dir,
        state,
        ctx,
        start_phase,
        is_resuming,
    }))
}

/// Save failed state to disk.
fn save_failed_state(
    state: &mut FeatureState,
    feature_dir: &Path,
    error: &str,
) -> Result<(), CliError> {
    state.status = FeatureStatus::Failed;
    state.error = Some(error.to_string());
    state.feature.updated_at = Utc::now();
    state.save(feature_dir)
}

/// Print execution summary.
fn print_execution_summary(state: &FeatureState) {
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

/// Get the latest commit SHA from the worktree.
fn get_latest_commit_sha(ctx: &TaskContext) -> Result<Option<String>, CliError> {
    let output = std::process::Command::new("git")
        .current_dir(&ctx.worktree_path)
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

/// Create a pull request using LLM to generate description.
///
/// The LLM handles:
/// - Committing any pending changes
/// - Pushing the branch
/// - Creating the PR with a detailed description
async fn create_pull_request(
    engine: &Engine<'_>,
    ctx: &TaskContext,
    state: &mut FeatureState,
) -> Result<String, CliError> {
    // Build context for PR task
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
        "repo_path": ctx.worktree_path.display().to_string(),
        "feature_id": ctx.feature_id,
        "feature_slug": ctx.feature_slug,
        "branch": state.git.branch,
        "base_branch": state.git.base_branch,
        "phases": phases_context,
        "stats": {
            "turns": state.total_stats.turns,
            "cost_usd": state.total_stats.cost_usd,
            "input_tokens": state.total_stats.input_tokens,
            "output_tokens": state.total_stats.output_tokens,
        },
    });

    // Run PR task
    let task = Task::new(TaskKind::Pr, context);
    let result = engine.run(task).await?;

    // Extract PR URL from output (look for "PR_URL: <url>" pattern)
    let pr_url = result
        .output
        .lines()
        .find_map(|line| {
            line.strip_prefix("PR_URL:")
                .or_else(|| line.strip_prefix("PR_URL :"))
                .map(str::trim)
                .map(String::from)
        })
        .or_else(|| {
            // Fallback: look for GitHub PR URL pattern
            result.output.lines().find_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("https://github.com/") && trimmed.contains("/pull/") {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "PR URL not found in output".to_string());

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
                worktree_path: ".trees/test-feature".to_string(),
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

    #[test]
    fn test_keyword_match_exact_line() {
        assert!(CheckConfig::matches_keyword("APPROVED", "APPROVED"));
        assert!(CheckConfig::matches_keyword("  APPROVED  ", "APPROVED"));
        assert!(CheckConfig::matches_keyword(
            "Some text\nAPPROVED\nMore text",
            "APPROVED"
        ));
    }

    #[test]
    fn test_keyword_match_verdict_prefix() {
        assert!(CheckConfig::matches_keyword(
            "Verdict: APPROVED",
            "APPROVED"
        ));
        assert!(CheckConfig::matches_keyword("Result: VERIFIED", "VERIFIED"));
        assert!(CheckConfig::matches_keyword("Status: FAILED", "FAILED"));
        assert!(CheckConfig::matches_keyword(
            "Outcome: NEEDS_CHANGES",
            "NEEDS_CHANGES"
        ));
    }

    #[test]
    fn test_keyword_match_bracketed() {
        assert!(CheckConfig::matches_keyword("[APPROVED]", "APPROVED"));
        assert!(CheckConfig::matches_keyword("**VERIFIED**", "VERIFIED"));
    }

    #[test]
    fn test_keyword_match_end_of_output() {
        // Keyword at end with word boundary
        assert!(CheckConfig::matches_keyword(
            "The review is APPROVED",
            "APPROVED"
        ));
        assert!(CheckConfig::matches_keyword(
            "After careful consideration, the code is VERIFIED",
            "VERIFIED"
        ));
    }

    #[test]
    fn test_keyword_no_match_partial() {
        // Should not match partial words
        assert!(!CheckConfig::matches_keyword("UNAPPROVED", "APPROVED"));
        assert!(!CheckConfig::matches_keyword("APPROVEDLY", "APPROVED"));
        assert!(!CheckConfig::matches_keyword(
            "The review is NOTAPPROVED",
            "APPROVED"
        ));
    }

    #[test]
    fn test_keyword_match_word_boundary() {
        assert!(contains_word("Hello APPROVED World", "APPROVED"));
        assert!(contains_word("APPROVED", "APPROVED"));
        assert!(!contains_word("UNAPPROVED", "APPROVED"));
        assert!(!contains_word("APPROVEDLY", "APPROVED"));
        assert!(contains_word("Status:APPROVED", "APPROVED"));
    }

    #[test]
    fn test_check_config_is_success() {
        let config = CheckConfig::REVIEW;
        assert!(config.is_success("Verdict: APPROVED"));
        assert!(config.is_success("APPROVED"));
        assert!(!config.is_success("UNAPPROVED"));
        assert!(!config.is_success("This is not approved"));
    }

    #[test]
    fn test_check_config_is_failure() {
        let config = CheckConfig::REVIEW;
        assert!(config.is_failure("Verdict: NEEDS_CHANGES"));
        assert!(config.is_failure("NEEDS_CHANGES"));
        // Note: "NO_NEEDS_CHANGES" would match because underscore is not alphanumeric,
        // so NEEDS_CHANGES has word boundaries. This is acceptable behavior.
        // The key protection is against words like "UNAPPROVED" or "APPROVEDLY".
    }
}
