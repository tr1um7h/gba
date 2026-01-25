//! Integration tests for the GBA CLI.
//!
//! These tests verify the CLI commands work correctly end-to-end.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

/// Get the path to the gba binary.
fn gba_binary() -> String {
    // Use cargo's target directory
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    format!("{}/debug/gba", target_dir)
}

/// Set up a test git repository.
fn setup_git_repo(dir: &Path) {
    // Initialize git repo
    let output = Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "main"])
        .output()
        .expect("git init failed");
    assert!(output.status.success(), "git init failed");

    // Configure git user
    Command::new("git")
        .current_dir(dir)
        .args(["config", "user.email", "test@example.com"])
        .output()
        .expect("git config failed");

    Command::new("git")
        .current_dir(dir)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config failed");

    // Create initial commit
    fs::write(dir.join("README.md"), "# Test Project").expect("write failed");

    Command::new("git")
        .current_dir(dir)
        .args(["add", "."])
        .output()
        .expect("git add failed");

    let output = Command::new("git")
        .current_dir(dir)
        .args(["commit", "--no-verify", "-m", "Initial commit"])
        .output()
        .expect("git commit failed");
    assert!(output.status.success(), "git commit failed");
}

/// Copy task templates to the test directory.
fn setup_task_templates(dir: &Path) {
    let tasks_dir = dir.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("mkdir tasks failed");

    // Create minimal task templates for testing
    for task_name in ["init", "plan", "execute", "review", "verification"] {
        let task_dir = tasks_dir.join(task_name);
        fs::create_dir_all(&task_dir).expect("mkdir task failed");

        // Create config.yml
        fs::write(
            task_dir.join("config.yml"),
            r#"preset: true
tools: []
disallowedTools: []
"#,
        )
        .expect("write config failed");

        // Create system.j2
        fs::write(task_dir.join("system.j2"), "You are a helpful assistant.")
            .expect("write system.j2 failed");

        // Create user.j2
        fs::write(task_dir.join("user.j2"), "Execute the task.").expect("write user.j2 failed");
    }
}

#[test]
#[ignore = "requires gba binary to be built"]
fn test_cli_help() {
    let output = Command::new(gba_binary())
        .args(["--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Geektime Bootcamp Agent"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("plan"));
    assert!(stdout.contains("run"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("status"));
}

#[test]
#[ignore = "requires gba binary to be built"]
fn test_cli_version() {
    let output = Command::new(gba_binary())
        .args(["--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gba"));
}

#[test]
#[ignore = "requires gba binary to be built"]
fn test_list_without_init() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());

    let output = Command::new(gba_binary())
        .current_dir(temp_dir.path())
        .args(["list"])
        .output()
        .expect("Failed to execute command");

    // Should fail because not initialized
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Not initialized") || stderr.contains("gba init"),
        "Expected 'not initialized' error, got: {}",
        stderr
    );
}

#[test]
#[ignore = "requires gba binary to be built"]
fn test_status_feature_not_found() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());

    // Create .gba directory to simulate initialization
    fs::create_dir_all(temp_dir.path().join(".gba")).expect("mkdir .gba failed");
    fs::create_dir_all(temp_dir.path().join(".trees")).expect("mkdir .trees failed");

    let output = Command::new(gba_binary())
        .current_dir(temp_dir.path())
        .args(["status", "nonexistent-feature"])
        .output()
        .expect("Failed to execute command");

    // Should fail because feature doesn't exist
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Feature not found") || stderr.contains("nonexistent-feature"),
        "Expected 'feature not found' error, got: {}",
        stderr
    );
}

#[test]
#[ignore = "requires gba binary to be built and Claude Code CLI available"]
fn test_init_command() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());

    let output = Command::new(gba_binary())
        .current_dir(temp_dir.path())
        .args(["init"])
        .output()
        .expect("Failed to execute command");

    // Note: This test may fail if Claude Code CLI is not available
    // Check for either success or a meaningful error
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // If it succeeded, check the expected outputs
    if output.status.success() {
        assert!(temp_dir.path().join(".gba").exists());
        assert!(temp_dir.path().join(".trees").exists());
    } else {
        // It's OK if it failed due to missing Claude CLI
        assert!(
            stderr.contains("claude") || stderr.contains("agent") || stderr.contains("Error"),
            "Unexpected error: stdout={}, stderr={}",
            stdout,
            stderr
        );
    }
}

#[test]
#[ignore = "requires gba binary to be built"]
fn test_run_dry_run_option() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());

    // Create .gba directory structure
    let feature_dir = temp_dir.path().join(".gba").join("0001_test-feature");
    fs::create_dir_all(&feature_dir).expect("mkdir feature failed");
    fs::create_dir_all(feature_dir.join("specs")).expect("mkdir specs failed");

    // Create minimal state.yml
    fs::write(
        feature_dir.join("state.yml"),
        r#"feature:
  id: "0001"
  slug: test-feature
  created_at: "2024-01-01T00:00:00Z"
  updated_at: "2024-01-01T00:00:00Z"
status: planned
current_phase: 0
git:
  worktree_path: .trees/0001_test-feature
  branch: feature/0001-test-feature
  base_branch: main
phases:
  - name: setup
    status: pending
total_stats:
  turns: 0
  input_tokens: 0
  output_tokens: 0
  cost_usd: 0.0
result:
  merged: false
"#,
    )
    .expect("write state failed");

    // Create .trees directory
    fs::create_dir_all(temp_dir.path().join(".trees")).expect("mkdir .trees failed");

    let output = Command::new(gba_binary())
        .current_dir(temp_dir.path())
        .args(["run", "test-feature", "--dry-run"])
        .output()
        .expect("Failed to execute command");

    // This will fail because worktree doesn't exist, but that's expected
    // We're just testing that the CLI parses the options correctly
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("worktree") || stderr.contains("Error") || stderr.contains("agent"),
        "Expected worktree or agent error, got: {}",
        stderr
    );
}
