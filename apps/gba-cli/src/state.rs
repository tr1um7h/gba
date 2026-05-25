//! Feature state management for GBA.
//!
//! This module defines the structure for `.gba/<id>_<slug>/state.yml` which
//! tracks the execution state of each feature including phase progress,
//! git information, and statistics.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CliError;

/// Feature execution state.
///
/// Tracks the complete state of a feature including progress, git info,
/// phase history, and final results.
///
/// # Example YAML
///
/// ```yaml
/// feature:
///   id: "0001"
///   slug: add-user-auth
///   createdAt: "2024-01-15T10:30:00Z"
///   updatedAt: "2024-01-15T14:20:00Z"
/// status: inProgress
/// currentPhase: 2
/// git:
///   worktreePath: .trees/0001_add-user-auth
///   branch: feature/0001-add-user-auth
///   baseBranch: main
/// phases:
///   - name: setup
///     status: completed
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureState {
    /// Feature identification.
    pub feature: FeatureInfo,

    /// Overall feature status.
    pub status: FeatureStatus,

    /// Current phase index (0-based).
    pub current_phase: usize,

    /// Git configuration for this feature.
    pub git: GitState,

    /// Phase execution history.
    pub phases: Vec<PhaseState>,

    /// Accumulated statistics.
    #[serde(default)]
    pub total_stats: TaskStats,

    /// Final result (PR URL, etc.).
    pub result: FeatureResult,

    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FeatureState {
    /// Load feature state from a feature directory.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The state file cannot be read
    /// - The YAML is malformed
    pub fn load(feature_dir: &Path) -> Result<Self, CliError> {
        let state_path = feature_dir.join("state.yml");
        if !state_path.exists() {
            return Err(CliError::State(format!(
                "state file not found: {}",
                state_path.display()
            )));
        }

        let content = fs::read_to_string(&state_path).map_err(|e| {
            CliError::Io(format!(
                "failed to read state: {}: {}",
                state_path.display(),
                e
            ))
        })?;

        let state: Self = serde_yaml::from_str(&content).map_err(|e| {
            CliError::State(format!(
                "failed to parse state {}: {}",
                state_path.display(),
                e
            ))
        })?;

        Ok(state)
    }

    /// Save feature state to a feature directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, feature_dir: &Path) -> Result<(), CliError> {
        let state_path = feature_dir.join("state.yml");

        let content = serde_yaml::to_string(self)
            .map_err(|e| CliError::State(format!("failed to serialize state: {}", e)))?;

        fs::write(&state_path, content).map_err(|e| {
            CliError::Io(format!(
                "failed to write state {}: {}",
                state_path.display(),
                e
            ))
        })?;

        Ok(())
    }
}

/// Feature identification information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureInfo {
    /// Feature ID (e.g., "0001").
    pub id: String,

    /// Feature slug (e.g., "add-user-auth").
    pub slug: String,

    /// When the feature was created.
    pub created_at: DateTime<Utc>,

    /// When the feature was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Feature execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatureStatus {
    /// Feature is currently being planned (planning in progress).
    Planning,
    /// Feature is planned but not started.
    Planned,
    /// Feature is currently being executed.
    InProgress,
    /// Feature execution completed successfully.
    Completed,
    /// Feature execution failed.
    Failed,
}

impl std::fmt::Display for FeatureStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "planning"),
            Self::Planned => write!(f, "planned"),
            Self::InProgress => write!(f, "inProgress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Git state for a feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitState {
    /// Path to the git worktree.
    pub worktree_path: String,

    /// Feature branch name.
    pub branch: String,

    /// Base branch to merge into.
    pub base_branch: String,
}

/// Individual phase execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseState {
    /// Phase name (e.g., "setup", "implementation").
    pub name: String,

    /// Phase execution status.
    pub status: PhaseStatus,

    /// When the phase started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// When the phase completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Commit SHA after phase completion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,

    /// Phase execution statistics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<TaskStats>,
}

/// Phase execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PhaseStatus {
    /// Phase not yet started.
    Pending,
    /// Phase currently executing.
    InProgress,
    /// Phase completed successfully.
    Completed,
    /// Phase failed.
    Failed,
}

impl std::fmt::Display for PhaseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "inProgress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Task execution statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStats {
    /// Number of conversation turns.
    pub turns: u32,

    /// Total input tokens used.
    pub input_tokens: u64,

    /// Total output tokens generated.
    pub output_tokens: u64,

    /// Estimated cost in USD.
    pub cost_usd: f64,
}

/// Final feature result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureResult {
    /// URL of the created pull request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,

    /// Pull request number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u32>,

    /// Whether the PR was merged.
    #[serde(default)]
    pub merged: bool,

    /// Code review result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<CheckResultState>,

    /// Verification result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<CheckResultState>,
}

/// Result of a check (review or verification).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResultState {
    /// Final status of the check.
    pub status: CheckResultStatus,

    /// Number of iterations performed.
    pub iterations: u32,

    /// When the check completed.
    pub completed_at: DateTime<Utc>,

    /// Error message if the check failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Status of a check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckResultStatus {
    /// Check passed successfully.
    Passed,
    /// Check found issues that still need changes.
    NeedsChanges,
    /// Check encountered an error.
    Error,
    /// Check was skipped.
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state_yaml() -> &'static str {
        r#"
feature:
  id: "0001"
  slug: add-user-auth
  createdAt: "2024-01-15T10:30:00Z"
  updatedAt: "2024-01-15T14:20:00Z"
status: inProgress
currentPhase: 2
git:
  worktreePath: .trees/0001_add-user-auth
  branch: feature/0001-add-user-auth
  baseBranch: main
phases:
  - name: setup
    status: completed
    startedAt: "2024-01-15T10:35:00Z"
    completedAt: "2024-01-15T10:42:00Z"
    commitSha: abc1234
    stats:
      turns: 5
      inputTokens: 12500
      outputTokens: 8300
      costUsd: 0.15
  - name: implementation
    status: completed
    startedAt: "2024-01-15T10:45:00Z"
    completedAt: "2024-01-15T11:30:00Z"
    commitSha: def5678
    stats:
      turns: 12
      inputTokens: 45000
      outputTokens: 32000
      costUsd: 0.58
  - name: testing
    status: inProgress
    startedAt: "2024-01-15T11:35:00Z"
    stats:
      turns: 3
      inputTokens: 8000
      outputTokens: 5500
      costUsd: 0.10
totalStats:
  turns: 20
  inputTokens: 65500
  outputTokens: 45800
  costUsd: 0.83
result:
  merged: false
"#
    }

    #[test]
    fn test_should_deserialize_feature_state() {
        let state: FeatureState = serde_yaml::from_str(sample_state_yaml()).unwrap();

        assert_eq!(state.feature.id, "0001");
        assert_eq!(state.feature.slug, "add-user-auth");
        assert_eq!(state.status, FeatureStatus::InProgress);
        assert_eq!(state.current_phase, 2);
        assert_eq!(state.git.branch, "feature/0001-add-user-auth");
        assert_eq!(state.phases.len(), 3);
        assert_eq!(state.total_stats.turns, 20);
        assert!(!state.result.merged);
    }

    #[test]
    fn test_should_deserialize_phase_states() {
        let state: FeatureState = serde_yaml::from_str(sample_state_yaml()).unwrap();

        let setup = &state.phases[0];
        assert_eq!(setup.name, "setup");
        assert_eq!(setup.status, PhaseStatus::Completed);
        assert!(setup.commit_sha.is_some());
        assert!(setup.stats.is_some());

        let testing = &state.phases[2];
        assert_eq!(testing.name, "testing");
        assert_eq!(testing.status, PhaseStatus::InProgress);
        assert!(testing.completed_at.is_none());
        assert!(testing.commit_sha.is_none());
    }

    #[test]
    fn test_should_serialize_feature_state() {
        let state = FeatureState {
            feature: FeatureInfo {
                id: "0001".to_string(),
                slug: "test-feature".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            status: FeatureStatus::Planned,
            current_phase: 0,
            git: GitState {
                worktree_path: ".trees/0001_test-feature".to_string(),
                branch: "feature/0001-test-feature".to_string(),
                base_branch: "main".to_string(),
            },
            phases: vec![],
            total_stats: TaskStats::default(),
            result: FeatureResult::default(),
            error: None,
        };

        let yaml = serde_yaml::to_string(&state).unwrap();
        assert!(yaml.contains("slug: test-feature"));
        assert!(yaml.contains("status: planned"));
    }

    #[test]
    fn test_feature_status_display() {
        assert_eq!(format!("{}", FeatureStatus::Planning), "planning");
        assert_eq!(format!("{}", FeatureStatus::Planned), "planned");
        assert_eq!(format!("{}", FeatureStatus::InProgress), "inProgress");
        assert_eq!(format!("{}", FeatureStatus::Completed), "completed");
        assert_eq!(format!("{}", FeatureStatus::Failed), "failed");
    }

    #[test]
    fn test_phase_status_display() {
        assert_eq!(format!("{}", PhaseStatus::Pending), "pending");
        assert_eq!(format!("{}", PhaseStatus::InProgress), "inProgress");
        assert_eq!(format!("{}", PhaseStatus::Completed), "completed");
        assert_eq!(format!("{}", PhaseStatus::Failed), "failed");
    }
}
