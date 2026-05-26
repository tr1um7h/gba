//! Implementation of the `gba recover` command.
//!
//! This module rolls back state.yml to allow resuming a failed `run`
//! from the failure point, without performing any git operations.

use std::path::Path;
use std::process::Command;

use chrono::Utc;

use crate::error::CliError;
use crate::state::{CheckResultStatus, FeatureState, FeatureStatus, PhaseState, PhaseStatus};
use crate::utils;

/// The stage at which a feature failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureStage {
    /// A phase execution failed.
    Phase(usize),
    /// Code review failed (all phases completed).
    Review,
    /// Verification failed (review passed/skipped).
    Verification,
    /// PR creation or other post-phase step failed.
    Other,
}

/// Result of failure point detection and analysis.
#[derive(Debug)]
struct RecoveryAnalysis {
    /// The stage at which the feature failed.
    stage: FailureStage,
    /// Commit SHA of the last completed phase (if any).
    last_good_commit: Option<String>,
    /// Name of the last completed phase (if any).
    last_completed_phase_name: Option<String>,
}

/// Detect the failure point from feature state.
///
/// Uses the priority order from the design spec:
/// 1. Any phase has Failed or InProgress status → Phase failure
/// 2. result.review exists and status is not Passed/Skipped → Review failure
/// 3. result.verification exists and status is not Passed/Skipped → Verification failure
/// 4. None of the above → Other failure
fn detect_failure_point(state: &FeatureState) -> FailureStage {
    // Priority 1: Check for phase failure
    for (i, phase) in state.phases.iter().enumerate() {
        if matches!(phase.status, PhaseStatus::Failed | PhaseStatus::InProgress) {
            return FailureStage::Phase(i);
        }
    }

    // Priority 2: Check for review failure
    if let Some(ref review) = state.result.review
        && !matches!(
            review.status,
            CheckResultStatus::Passed | CheckResultStatus::Skipped
        )
    {
        return FailureStage::Review;
    }

    // Priority 3: Check for verification failure
    if let Some(ref verification) = state.result.verification
        && !matches!(
            verification.status,
            CheckResultStatus::Passed | CheckResultStatus::Skipped
        )
    {
        return FailureStage::Verification;
    }

    // Priority 4: Other failure (PR creation, etc.)
    FailureStage::Other
}

/// Analyze a failed feature state for recovery.
fn analyze_failure(state: &FeatureState) -> RecoveryAnalysis {
    let stage = detect_failure_point(state);

    // Find the last completed phase and its commit SHA
    let mut last_good_commit: Option<String> = None;
    let mut last_completed_phase_name: Option<String> = None;

    for phase in state.phases.iter() {
        if phase.status == PhaseStatus::Completed {
            last_good_commit = phase.commit_sha.clone();
            last_completed_phase_name = Some(phase.name.clone());
        }
    }

    RecoveryAnalysis {
        stage,
        last_good_commit,
        last_completed_phase_name,
    }
}

/// Apply state rollback based on the failure stage.
fn apply_rollback(state: &mut FeatureState, analysis: &RecoveryAnalysis) {
    // Common: set status to InProgress and clear error
    state.status = FeatureStatus::InProgress;
    state.error = None;
    state.feature.updated_at = Utc::now();

    match analysis.stage {
        FailureStage::Phase(failed_idx) => {
            // Reset the failed phase to Pending
            state.current_phase = failed_idx;
            reset_phase_to_pending(&mut state.phases[failed_idx]);
        }
        FailureStage::Review => {
            // Keep current_phase at last phase index
            state.result.review = None;
            state.result.verification = None;
        }
        FailureStage::Verification => {
            // Keep current_phase at last phase index
            state.result.review = None;
            state.result.verification = None;
        }
        FailureStage::Other => {
            // No field changes beyond status and error
        }
    }
}

/// Reset a phase state to Pending, clearing execution data.
fn reset_phase_to_pending(phase: &mut PhaseState) {
    phase.status = PhaseStatus::Pending;
    phase.started_at = None;
    phase.completed_at = None;
    phase.commit_sha = None;
    phase.stats = None;
}

/// Check if the worktree has uncommitted changes.
///
/// Uses `git status --porcelain` to detect dirty state.
fn is_worktree_dirty(worktree_path: &Path) -> Result<bool, CliError> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .map_err(|e| CliError::Git(format!("failed to run git status: {}", e)))?;

    if !output.status.success() {
        return Err(CliError::Git(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

/// Print the recovery summary to stdout.
fn print_recovery_summary(state: &FeatureState, analysis: &RecoveryAnalysis, is_dirty: bool) {
    println!("Feature '{}' recovered.", state.feature.slug);
    println!();
    println!("Recovery summary:");

    let total_phases = state.phases.len();
    match analysis.stage {
        FailureStage::Phase(idx) => {
            let phase_name = &state.phases[idx].name;
            println!(
                "  Failed at: phase '{}' (index {}/{})",
                phase_name, idx, total_phases
            );

            // Count how many phases were rolled back (the failed one + any after it)
            let rolled_back_count = total_phases - idx;
            if rolled_back_count > 1 {
                println!(
                    "  Rolled back: phases {}-{} → pending",
                    idx,
                    total_phases - 1
                );
            } else {
                println!("  Rolled back: phase {} → pending", idx);
            }
        }
        FailureStage::Review => {
            println!(
                "  Failed at: code review (all {} phases completed)",
                total_phases
            );
            println!("  Rolled back: review and verification results");
        }
        FailureStage::Verification => {
            println!(
                "  Failed at: verification (all {} phases completed)",
                total_phases
            );
            println!("  Rolled back: review and verification results");
        }
        FailureStage::Other => {
            println!(
                "  Failed at: post-phase step (all {} phases completed)",
                total_phases
            );
        }
    }

    // Report last good commit
    match (
        &analysis.last_good_commit,
        &analysis.last_completed_phase_name,
    ) {
        (Some(sha), Some(name)) => {
            println!("  Last good commit: {} (phase '{}')", sha, name);
        }
        _ => {
            println!("  No prior commit available for reset suggestion.");
        }
    }

    // Git status section
    println!();
    println!("Git status:");
    if is_dirty {
        if let Some(ref sha) = analysis.last_good_commit {
            println!("  Working tree has uncommitted changes.");
            println!("  To undo failed phase changes: git reset --hard {}", sha);
        } else {
            println!("  Working tree has uncommitted changes.");
            println!("  Commit or stash changes before resuming.");
        }
    } else {
        println!("  Working tree is clean.");
    }

    println!();
    println!("Next step: gba run {}", state.feature.slug);
}

/// Recover a failed feature for resumption.
///
/// Rolls back state.yml to allow resuming a failed `run` from the
/// failure point. No git operations are performed.
///
/// # Errors
///
/// Returns an error if:
/// - GBA is not initialized
/// - Feature does not exist
/// - Feature status is not `Failed`
/// - Worktree does not exist
pub async fn run_recover(workdir: &Path, slug: &str) -> Result<(), CliError> {
    // Check initialization
    if !utils::is_initialized(workdir) {
        return Err(CliError::NotInitialized);
    }

    // Find and load feature state
    let feature_dir = utils::find_feature_dir(workdir, slug)?;
    let mut state = FeatureState::load(&feature_dir)?;

    // Validate status is Failed
    if state.status != FeatureStatus::Failed {
        return Err(CliError::InvalidState(format!(
            "feature '{}' has status '{}' — only features with Failed status can be recovered",
            slug, state.status
        )));
    }

    // Check worktree exists
    let worktree_path = utils::feature_worktree_path(workdir, &state.feature.slug);
    if !worktree_path.exists() {
        return Err(CliError::InvalidState(format!(
            "worktree not found: {}",
            worktree_path.display()
        )));
    }

    // Detect failure point and analyze
    let analysis = analyze_failure(&state);

    // Check for dirty worktree
    let is_dirty = is_worktree_dirty(&worktree_path)?;

    // Apply rollback
    apply_rollback(&mut state, &analysis);

    // Save updated state
    state.save(&feature_dir)?;

    // Print recovery summary
    print_recovery_summary(&state, &analysis, is_dirty);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CheckResultState, FeatureInfo, FeatureResult, GitState};

    fn create_failed_state_with_phase_failure() -> FeatureState {
        FeatureState {
            feature: FeatureInfo {
                id: "0001".to_string(),
                slug: "test-feature".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            status: FeatureStatus::Failed,
            current_phase: 2,
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
                    status: PhaseStatus::Completed,
                    started_at: Some(Utc::now()),
                    completed_at: Some(Utc::now()),
                    commit_sha: Some("def5678".to_string()),
                    stats: None,
                },
                PhaseState {
                    name: "testing".to_string(),
                    status: PhaseStatus::Failed,
                    started_at: Some(Utc::now()),
                    completed_at: None,
                    commit_sha: None,
                    stats: None,
                },
            ],
            total_stats: crate::state::TaskStats::default(),
            result: FeatureResult::default(),
            error: Some("testing phase failed".to_string()),
        }
    }

    fn create_failed_state_with_review_failure() -> FeatureState {
        let mut state = create_failed_state_with_phase_failure();
        state.current_phase = 2;
        state.phases[2].status = PhaseStatus::Completed;
        state.phases[2].commit_sha = Some("ghi9012".to_string());
        state.phases[2].completed_at = Some(Utc::now());
        state.result.review = Some(CheckResultState {
            status: CheckResultStatus::NeedsChanges,
            iterations: 2,
            completed_at: Utc::now(),
            error: Some("code review needs changes".to_string()),
        });
        state.error = Some("code review failed".to_string());
        state
    }

    fn create_failed_state_with_verification_failure() -> FeatureState {
        let mut state = create_failed_state_with_review_failure();
        state.result.review = Some(CheckResultState {
            status: CheckResultStatus::Passed,
            iterations: 1,
            completed_at: Utc::now(),
            error: None,
        });
        state.result.verification = Some(CheckResultState {
            status: CheckResultStatus::NeedsChanges,
            iterations: 3,
            completed_at: Utc::now(),
            error: Some("verification failed".to_string()),
        });
        state.error = Some("verification failed".to_string());
        state
    }

    fn create_failed_state_with_other_failure() -> FeatureState {
        let mut state = create_failed_state_with_review_failure();
        state.result.review = Some(CheckResultState {
            status: CheckResultStatus::Passed,
            iterations: 1,
            completed_at: Utc::now(),
            error: None,
        });
        state.result.verification = Some(CheckResultState {
            status: CheckResultStatus::Passed,
            iterations: 1,
            completed_at: Utc::now(),
            error: None,
        });
        state.error = Some("PR creation failed".to_string());
        state
    }

    fn create_failed_state_first_phase_failure() -> FeatureState {
        FeatureState {
            feature: FeatureInfo {
                id: "0001".to_string(),
                slug: "test-feature".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            status: FeatureStatus::Failed,
            current_phase: 0,
            git: GitState {
                worktree_path: ".trees/test-feature".to_string(),
                branch: "feature/0001-test-feature".to_string(),
                base_branch: "main".to_string(),
            },
            phases: vec![PhaseState {
                name: "setup".to_string(),
                status: PhaseStatus::Failed,
                started_at: Some(Utc::now()),
                completed_at: None,
                commit_sha: None,
                stats: None,
            }],
            total_stats: crate::state::TaskStats::default(),
            result: FeatureResult::default(),
            error: Some("setup failed".to_string()),
        }
    }

    #[test]
    fn test_should_detect_phase_failure() {
        let state = create_failed_state_with_phase_failure();
        let stage = detect_failure_point(&state);
        assert_eq!(stage, FailureStage::Phase(2));
    }

    #[test]
    fn test_should_detect_review_failure() {
        let state = create_failed_state_with_review_failure();
        let stage = detect_failure_point(&state);
        assert_eq!(stage, FailureStage::Review);
    }

    #[test]
    fn test_should_detect_verification_failure() {
        let state = create_failed_state_with_verification_failure();
        let stage = detect_failure_point(&state);
        assert_eq!(stage, FailureStage::Verification);
    }

    #[test]
    fn test_should_detect_other_failure() {
        let state = create_failed_state_with_other_failure();
        let stage = detect_failure_point(&state);
        assert_eq!(stage, FailureStage::Other);
    }

    #[test]
    fn test_should_detect_first_phase_failure() {
        let state = create_failed_state_first_phase_failure();
        let stage = detect_failure_point(&state);
        assert_eq!(stage, FailureStage::Phase(0));
    }

    #[test]
    fn test_should_analyze_phase_failure() {
        let state = create_failed_state_with_phase_failure();
        let analysis = analyze_failure(&state);

        assert_eq!(analysis.stage, FailureStage::Phase(2));
        assert_eq!(analysis.last_good_commit, Some("def5678".to_string()));
        assert_eq!(
            analysis.last_completed_phase_name,
            Some("implementation".to_string())
        );
    }

    #[test]
    fn test_should_analyze_first_phase_failure_no_prior_commit() {
        let state = create_failed_state_first_phase_failure();
        let analysis = analyze_failure(&state);

        assert_eq!(analysis.stage, FailureStage::Phase(0));
        assert_eq!(analysis.last_good_commit, None);
        assert_eq!(analysis.last_completed_phase_name, None);
    }

    #[test]
    fn test_should_rollback_phase_failure() {
        let state = create_failed_state_with_phase_failure();
        let analysis = analyze_failure(&state);

        let mut mutable_state = create_failed_state_with_phase_failure();
        apply_rollback(&mut mutable_state, &analysis);

        assert_eq!(mutable_state.status, FeatureStatus::InProgress);
        assert!(mutable_state.error.is_none());
        assert_eq!(mutable_state.current_phase, 2);

        // Failed phase should be reset to Pending
        assert_eq!(mutable_state.phases[2].status, PhaseStatus::Pending);
        assert!(mutable_state.phases[2].started_at.is_none());
        assert!(mutable_state.phases[2].completed_at.is_none());
        assert!(mutable_state.phases[2].commit_sha.is_none());
        assert!(mutable_state.phases[2].stats.is_none());

        // Prior phases should be untouched
        assert_eq!(mutable_state.phases[0].status, PhaseStatus::Completed);
        assert_eq!(mutable_state.phases[1].status, PhaseStatus::Completed);
        assert_eq!(
            mutable_state.phases[1].commit_sha,
            Some("def5678".to_string())
        );
    }

    #[test]
    fn test_should_rollback_review_failure() {
        let state = create_failed_state_with_review_failure();
        let analysis = analyze_failure(&state);

        let mut mutable_state = create_failed_state_with_review_failure();
        apply_rollback(&mut mutable_state, &analysis);

        assert_eq!(mutable_state.status, FeatureStatus::InProgress);
        assert!(mutable_state.error.is_none());

        // Review and verification should be cleared
        assert!(mutable_state.result.review.is_none());
        assert!(mutable_state.result.verification.is_none());

        // All phases should remain Completed
        for phase in &mutable_state.phases {
            assert_eq!(phase.status, PhaseStatus::Completed);
        }
    }

    #[test]
    fn test_should_rollback_verification_failure() {
        let state = create_failed_state_with_verification_failure();
        let analysis = analyze_failure(&state);

        let mut mutable_state = create_failed_state_with_verification_failure();
        apply_rollback(&mut mutable_state, &analysis);

        assert_eq!(mutable_state.status, FeatureStatus::InProgress);
        assert!(mutable_state.error.is_none());

        // Both review and verification should be cleared
        assert!(mutable_state.result.review.is_none());
        assert!(mutable_state.result.verification.is_none());
    }

    #[test]
    fn test_should_rollback_other_failure() {
        let state = create_failed_state_with_other_failure();
        let analysis = analyze_failure(&state);

        let mut mutable_state = create_failed_state_with_other_failure();
        apply_rollback(&mut mutable_state, &analysis);

        assert_eq!(mutable_state.status, FeatureStatus::InProgress);
        assert!(mutable_state.error.is_none());

        // PR URL should remain (it was created before the "other" failure)
        // State fields should be unchanged except status and error
    }

    #[test]
    fn test_should_rollback_first_phase_failure() {
        let state = create_failed_state_first_phase_failure();
        let analysis = analyze_failure(&state);

        let mut mutable_state = create_failed_state_first_phase_failure();
        apply_rollback(&mut mutable_state, &analysis);

        assert_eq!(mutable_state.status, FeatureStatus::InProgress);
        assert!(mutable_state.error.is_none());
        assert_eq!(mutable_state.current_phase, 0);
        assert_eq!(mutable_state.phases[0].status, PhaseStatus::Pending);
    }

    #[test]
    fn test_should_detect_in_progress_phase_as_failure() {
        let mut state = create_failed_state_with_phase_failure();
        state.phases[2].status = PhaseStatus::InProgress;

        let stage = detect_failure_point(&state);
        assert_eq!(stage, FailureStage::Phase(2));
    }

    #[test]
    fn test_should_preserve_total_stats_on_rollback() {
        let mut state = create_failed_state_with_phase_failure();
        state.total_stats.turns = 42;
        state.total_stats.cost_usd = 1.5;

        let analysis = analyze_failure(&state);
        apply_rollback(&mut state, &analysis);

        assert_eq!(state.total_stats.turns, 42);
        assert!((state.total_stats.cost_usd - 1.5).abs() < f64::EPSILON);
    }
}
