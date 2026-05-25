//! Configuration types for GBA project settings.
//!
//! This module defines the structure for `.gba/config.yml` which stores
//! project-level GBA settings including agent configuration, git behavior,
//! and review options.
//!
//! These types are used by the plan and run commands (Phase 4 and 5).

// These types are public API that will be used in Phase 4 and 5.
#![allow(dead_code)]

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CliError;

/// Default permission mode for agent operations.
fn default_permission_mode() -> String {
    "auto".to_string()
}

/// Default value for auto_commit setting.
const fn default_auto_commit() -> bool {
    true
}

/// Default value for auto_pr setting.
const fn default_auto_pr() -> bool {
    true
}

/// Default web port.
const fn default_web_port() -> u16 {
    3456
}

/// Default web host.
fn default_web_host() -> String {
    "127.0.0.1".to_string()
}

/// Default value for auto_push setting.
const fn default_auto_push() -> bool {
    false
}

/// Default branch pattern for feature branches.
fn default_branch_pattern() -> String {
    "feature/{id}-{slug}".to_string()
}

/// Default value for review enabled setting.
const fn default_review_enabled() -> bool {
    true
}

/// Default review provider.
fn default_review_provider() -> String {
    "codex".to_string()
}

/// Web UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebConfig {
    /// Port for the web UI server.
    #[serde(default = "default_web_port")]
    pub port: u16,

    /// Host address for the web UI server.
    #[serde(default = "default_web_host")]
    pub host: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            port: default_web_port(),
            host: default_web_host(),
        }
    }
}

/// GBA project configuration.
///
/// This is the root configuration structure loaded from `.gba/config.yml`.
///
/// # Example YAML
///
/// ```yaml
/// agent:
///   permissionMode: auto
///   budgetLimit: 10.0
/// git:
///   autoCommit: true
///   branchPattern: "feature/{id}-{slug}"
/// review:
///   enabled: true
///   provider: codex
/// web:
///   port: 3456
///   host: "127.0.0.1"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GbaConfig {
    /// Agent configuration.
    #[serde(default)]
    pub agent: AgentConfig,

    /// Prompt configuration.
    #[serde(default)]
    pub prompts: PromptsConfig,

    /// Git configuration.
    #[serde(default)]
    pub git: GitConfig,

    /// Review configuration.
    #[serde(default)]
    pub review: ReviewConfig,

    /// Web UI configuration.
    #[serde(default)]
    pub web: WebConfig,
}

impl GbaConfig {
    /// Load configuration from the given GBA directory.
    ///
    /// Looks for `config.yml` in the provided `.gba/` directory.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The config file cannot be read
    /// - The YAML is malformed
    pub fn load(gba_dir: &Path) -> Result<Self, CliError> {
        let config_path = gba_dir.join("config.yml");
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path).map_err(|e| {
            CliError::Io(format!(
                "failed to read config: {}: {}",
                config_path.display(),
                e
            ))
        })?;

        let config: Self = serde_yaml::from_str(&content).map_err(|e| {
            CliError::Config(format!(
                "failed to parse config {}: {}",
                config_path.display(),
                e
            ))
        })?;

        Ok(config)
    }

    /// Save configuration to the given GBA directory.
    ///
    /// Writes `config.yml` to the provided `.gba/` directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, gba_dir: &Path) -> Result<(), CliError> {
        let config_path = gba_dir.join("config.yml");

        let content = serde_yaml::to_string(self)
            .map_err(|e| CliError::Config(format!("failed to serialize config: {}", e)))?;

        fs::write(&config_path, content).map_err(|e| {
            CliError::Io(format!(
                "failed to write config {}: {}",
                config_path.display(),
                e
            ))
        })?;

        Ok(())
    }
}

/// Agent configuration options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    /// Claude model to use (optional, SDK handles default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Permission mode: auto | manual | none.
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,

    /// Budget limit in USD (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_limit: Option<f64>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: None,
            permission_mode: default_permission_mode(),
            budget_limit: None,
        }
    }
}

/// Prompt template configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsConfig {
    /// Additional prompt template directories to include.
    #[serde(default)]
    pub include: Vec<String>,
}

/// Git configuration options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    /// Whether to auto-commit after each phase.
    #[serde(default = "default_auto_commit")]
    pub auto_commit: bool,

    /// Whether to auto-create PR after completing all phases.
    #[serde(default = "default_auto_pr")]
    pub auto_pr: bool,

    /// Whether to auto-push to remote after state updates.
    #[serde(default = "default_auto_push")]
    pub auto_push: bool,

    /// Branch naming pattern.
    ///
    /// Available variables: `{id}`, `{slug}`
    #[serde(default = "default_branch_pattern")]
    pub branch_pattern: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_commit: default_auto_commit(),
            auto_pr: default_auto_pr(),
            auto_push: default_auto_push(),
            branch_pattern: default_branch_pattern(),
        }
    }
}

/// Code review configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewConfig {
    /// Whether code review is enabled.
    #[serde(default = "default_review_enabled")]
    pub enabled: bool,

    /// Review provider: codex | claude.
    #[serde(default = "default_review_provider")]
    pub provider: String,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            enabled: default_review_enabled(),
            provider: default_review_provider(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_deserialize_minimal_config() {
        let yaml = "";
        let config: GbaConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.agent.permission_mode, "auto");
        assert!(config.git.auto_commit);
        assert!(config.review.enabled);
    }

    #[test]
    fn test_should_deserialize_full_config() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-20250514
  permissionMode: manual
  budgetLimit: 10.0
prompts:
  include:
    - ~/.config/gba/prompts
git:
  autoCommit: false
  autoPr: false
  autoPush: true
  branchPattern: "feat/{id}/{slug}"
review:
  enabled: false
  provider: claude
"#;
        let config: GbaConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(
            config.agent.model,
            Some("claude-sonnet-4-20250514".to_string())
        );
        assert_eq!(config.agent.permission_mode, "manual");
        assert_eq!(config.agent.budget_limit, Some(10.0));
        assert_eq!(config.prompts.include, vec!["~/.config/gba/prompts"]);
        assert!(!config.git.auto_commit);
        assert!(!config.git.auto_pr);
        assert!(config.git.auto_push);
        assert_eq!(config.git.branch_pattern, "feat/{id}/{slug}");
        assert!(!config.review.enabled);
        assert_eq!(config.review.provider, "claude");
    }

    #[test]
    fn test_should_serialize_config() {
        let config = GbaConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(yaml.contains("permissionMode: auto"));
        assert!(yaml.contains("autoCommit: true"));
        assert!(yaml.contains("autoPr: true"));
        assert!(yaml.contains("autoPush: false"));
    }

    #[test]
    fn test_should_use_defaults() {
        let config = GbaConfig::default();

        assert_eq!(config.agent.permission_mode, "auto");
        assert!(config.agent.model.is_none());
        assert!(config.agent.budget_limit.is_none());
        assert!(config.prompts.include.is_empty());
        assert!(config.git.auto_commit);
        assert!(config.git.auto_pr);
        assert!(!config.git.auto_push);
        assert_eq!(config.git.branch_pattern, "feature/{id}-{slug}");
        assert!(config.review.enabled);
        assert_eq!(config.review.provider, "codex");
    }
}
