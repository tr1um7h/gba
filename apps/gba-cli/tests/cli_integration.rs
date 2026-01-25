//! Integration tests for the GBA CLI.
//!
//! These tests verify the CLI commands work correctly end-to-end.
//!
//! # Test Categories
//!
//! - **Basic tests** (`test_cli_*`): Can run without the gba binary (use cargo run)
//! - **Feature tests** (`test_*_command`, `test_*_without_init`): Require gba binary
//! - **Full integration** (`test_init_command`): Require Claude Code CLI
//!
//! # Running Tests
//!
//! ```bash
//! # Build first, then run all tests
//! cargo build -p gba-cli && cargo test -p gba-cli --test cli_integration
//!
//! # Run only tests that work without external dependencies
//! cargo test -p gba-cli --test cli_integration -- --skip requires_binary
//! ```

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

/// Get the path to the gba binary, checking if it exists.
fn gba_binary() -> Option<String> {
    // Use cargo's target directory
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let path = format!("{}/debug/gba", target_dir);

    if std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

/// Check if the gba binary exists, skip test if not.
/// Returns None if the binary doesn't exist (test should be skipped).
fn require_gba_binary() -> Option<String> {
    gba_binary()
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

/// Create minimal GBA initialization structure.
fn setup_gba_init(dir: &Path) {
    fs::create_dir_all(dir.join(".gba")).expect("mkdir .gba failed");
    fs::create_dir_all(dir.join(".trees")).expect("mkdir .trees failed");
}

/// Create a feature directory structure for testing.
fn setup_feature(dir: &Path, id: &str, slug: &str) {
    let feature_dir = dir.join(".gba").join(format!("{id}_{slug}"));
    fs::create_dir_all(&feature_dir).expect("mkdir feature failed");
    fs::create_dir_all(feature_dir.join("specs")).expect("mkdir specs failed");

    // Create minimal state.yml
    fs::write(
        feature_dir.join("state.yml"),
        format!(
            r#"feature:
  id: "{id}"
  slug: {slug}
  created_at: "2024-01-01T00:00:00Z"
  updated_at: "2024-01-01T00:00:00Z"
status: planned
current_phase: 0
git:
  worktree_path: .trees/{id}_{slug}
  branch: feature/{id}-{slug}
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
"#
        ),
    )
    .expect("write state failed");
}

// ============================================================================
// Basic CLI tests - test command parsing without external dependencies
// ============================================================================

#[test]
fn test_cli_help_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let output = Command::new(binary)
        .args(["--help"])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "gba --help should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check for expected subcommands
    assert!(
        stdout.contains("init"),
        "Help should list 'init' command: {stdout}"
    );
    assert!(
        stdout.contains("plan"),
        "Help should list 'plan' command: {stdout}"
    );
    assert!(
        stdout.contains("run"),
        "Help should list 'run' command: {stdout}"
    );
    assert!(
        stdout.contains("list"),
        "Help should list 'list' command: {stdout}"
    );
    assert!(
        stdout.contains("status"),
        "Help should list 'status' command: {stdout}"
    );
}

#[test]
fn test_cli_version_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let output = Command::new(binary)
        .args(["--version"])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "gba --version should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gba"),
        "Version output should contain 'gba': {stdout}"
    );
}

#[test]
fn test_cli_invalid_command_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let output = Command::new(binary)
        .args(["nonexistent-command"])
        .output()
        .expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "Invalid command should fail, stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Clap outputs error messages to stderr
    assert!(
        stderr.contains("error") || stderr.contains("unrecognized"),
        "Should show error for invalid command: {stderr}"
    );
}

// ============================================================================
// Feature tests - test commands that don't require Claude CLI
// ============================================================================

#[test]
fn test_list_without_init_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["list"])
        .output()
        .expect("Failed to execute command");

    // Should fail because not initialized
    assert!(
        !output.status.success(),
        "list should fail when not initialized, stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should indicate the project needs initialization
    assert!(
        stderr.to_lowercase().contains("not initialized")
            || stderr.to_lowercase().contains("init")
            || stderr.to_lowercase().contains(".gba"),
        "Error should mention initialization, got: {stderr}"
    );
}

#[test]
fn test_list_with_empty_init_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());
    setup_gba_init(temp_dir.path());

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["list"])
        .output()
        .expect("Failed to execute command");

    // Should succeed but show empty list
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either succeeds with empty output or shows "no features"
    if output.status.success() {
        // Empty or shows "no features" message
        assert!(
            stdout.is_empty()
                || stdout.to_lowercase().contains("no feature")
                || stdout.contains("Features"),
            "Should show empty or no features message, got stdout: {stdout}"
        );
    } else {
        // May fail if additional setup is needed, but error should be clear
        assert!(
            !stderr.is_empty(),
            "Should provide an error message, got empty stderr"
        );
    }
}

#[test]
fn test_status_feature_not_found_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());
    setup_gba_init(temp_dir.path());

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["status", "nonexistent-feature"])
        .output()
        .expect("Failed to execute command");

    // Should fail because feature doesn't exist
    assert!(
        !output.status.success(),
        "status for nonexistent feature should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Error should mention the feature name or "not found"
    assert!(
        stderr.to_lowercase().contains("not found")
            || stderr.contains("nonexistent-feature")
            || stderr.to_lowercase().contains("no feature"),
        "Error should indicate feature not found, got: {stderr}"
    );
}

#[test]
fn test_status_existing_feature_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());
    setup_gba_init(temp_dir.path());
    setup_feature(temp_dir.path(), "0001", "test-feature");

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["status", "test-feature"])
        .output()
        .expect("Failed to execute command");

    // Should succeed and show status
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        // Should display feature information
        assert!(
            stdout.contains("test-feature")
                || stdout.to_lowercase().contains("status")
                || stdout.to_lowercase().contains("planned"),
            "Status output should contain feature info, got: {stdout}"
        );
    } else {
        // If it fails, it should be for a specific reason (like missing worktree)
        // not a generic error
        assert!(
            stderr.to_lowercase().contains("worktree")
                || stderr.to_lowercase().contains("branch")
                || stderr.contains("test-feature"),
            "Error should be specific, got: {stderr}"
        );
    }
}

#[test]
fn test_run_missing_feature_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());
    setup_gba_init(temp_dir.path());

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["run", "missing-feature"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "run missing feature should fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("not found")
            || stderr.contains("missing-feature")
            || stderr.to_lowercase().contains("no feature"),
        "Should indicate feature not found, got: {stderr}"
    );
}

#[test]
fn test_run_with_dry_run_option_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());
    setup_gba_init(temp_dir.path());
    setup_feature(temp_dir.path(), "0001", "test-feature");

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["run", "test-feature", "--dry-run"])
        .output()
        .expect("Failed to execute command");

    // Will fail because worktree doesn't exist, but should parse --dry-run correctly
    // We're testing that the CLI accepts the flag
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT show an error about unknown flag
    assert!(
        !stderr.contains("unexpected argument")
            && !stderr.contains("unknown")
            && !stderr.to_lowercase().contains("invalid option"),
        "Should accept --dry-run flag, got: {stderr}"
    );

    // Error should be about missing worktree or similar, not about the flag
    if !output.status.success() {
        assert!(
            stderr.to_lowercase().contains("worktree")
                || stderr.to_lowercase().contains("branch")
                || stderr.to_lowercase().contains("error")
                || stderr.to_lowercase().contains("agent"),
            "Error should be about execution, not flag parsing, got: {stderr}"
        );
    }
}

// ============================================================================
// Full integration tests - require Claude Code CLI
// ============================================================================

#[test]
#[ignore = "requires Claude Code CLI to be installed and available"]
fn test_init_command_full_integration() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["init"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        // Verify initialization created expected directories
        assert!(
            temp_dir.path().join(".gba").exists(),
            ".gba directory should be created"
        );
        assert!(
            temp_dir.path().join(".trees").exists(),
            ".trees directory should be created"
        );
    } else {
        // If it failed, it should be because Claude CLI is not available
        let combined_output = format!("{stdout}{stderr}").to_lowercase();
        assert!(
            combined_output.contains("claude")
                || combined_output.contains("not found")
                || combined_output.contains("cli")
                || combined_output.contains("agent"),
            "Init failure should indicate missing Claude CLI, got stdout: {stdout}, stderr: {stderr}"
        );
    }
}

// ============================================================================
// Plan command tests
// ============================================================================

#[test]
fn test_plan_missing_slug_requires_binary() {
    let Some(binary) = require_gba_binary() else {
        eprintln!("Skipping test: gba binary not found - run `cargo build -p gba-cli` first");
        return;
    };
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    setup_git_repo(temp_dir.path());
    setup_task_templates(temp_dir.path());
    setup_gba_init(temp_dir.path());

    let output = Command::new(&binary)
        .current_dir(temp_dir.path())
        .args(["plan"])
        .output()
        .expect("Failed to execute command");

    // Should fail because slug is required
    assert!(
        !output.status.success(),
        "plan without slug should fail, stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Clap should complain about missing argument
    assert!(
        stderr.contains("required") || stderr.contains("SLUG") || stderr.contains("argument"),
        "Should indicate missing required argument, got: {stderr}"
    );
}
