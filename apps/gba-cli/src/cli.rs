//! CLI definition for GBA (Geektime Bootcamp Agent).
//!
//! This module defines the command-line interface using clap derive macros.
//! The CLI supports subcommands for initializing repositories, planning features,
//! executing plans, and viewing feature status.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// GBA (Geektime Bootcamp Agent) - AI-assisted feature development.
///
/// GBA helps you plan and implement features through structured workflows
/// with AI assistance. It manages feature specifications, git worktrees,
/// and execution state.
#[derive(Debug, Parser)]
#[command(name = "gba", about = "Geektime Bootcamp Agent")]
#[command(version, author)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,

    /// Working directory (defaults to current directory).
    ///
    /// All GBA operations will be performed relative to this directory.
    #[arg(short, long, global = true)]
    pub workdir: Option<PathBuf>,

    /// Enable verbose output.
    ///
    /// When enabled, shows debug-level logs for troubleshooting.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

/// Available GBA commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize GBA in the current repository.
    ///
    /// Creates the `.gba/` directory structure, generates project documentation
    /// in `.gba.md`, and sets up the `.trees/` directory for git worktrees.
    Init,

    /// Plan a new feature interactively.
    ///
    /// Opens an interactive browser-based session to discuss and plan a new feature.
    /// Generates design specs and creates a git worktree for implementation.
    Plan {
        /// Feature slug (e.g., "add-user-auth").
        ///
        /// This identifier is used for directory names and git branches.
        slug: String,
    },

    /// Execute a planned feature.
    ///
    /// Runs the implementation phases defined in the feature spec.
    /// Supports resuming from interruptions and restarting from scratch.
    Run {
        /// Feature slug to execute.
        slug: String,

        /// Resume from a specific phase (0-indexed).
        ///
        /// Overrides automatic resume detection.
        #[arg(long)]
        from_phase: Option<usize>,

        /// Dry run mode (no commits or pushes).
        ///
        /// Useful for testing the execution flow without making changes.
        #[arg(long)]
        dry_run: bool,

        /// Restart execution from the beginning.
        ///
        /// Ignores any existing progress and starts fresh.
        #[arg(long)]
        restart: bool,
    },

    /// List all features.
    ///
    /// Shows a table of all features with their status and progress.
    List,

    /// Show detailed feature status.
    ///
    /// Displays phase progress, execution statistics, and current state.
    Status {
        /// Feature slug to show status for.
        slug: String,
    },

    /// Clean up worktrees for closed/merged PRs.
    ///
    /// Scans existing git worktrees, checks their PR status, and removes
    /// worktrees and branches for PRs that have been merged or closed.
    Clean {
        /// Dry run mode (show what would be cleaned without actually cleaning).
        #[arg(long)]
        dry_run: bool,

        /// Also clean worktrees for closed (not merged) PRs.
        ///
        /// By default, only merged PRs are cleaned. Use this flag to also
        /// clean PRs that were closed without merging.
        #[arg(long, short)]
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_parse_init_command() {
        let cli = Cli::try_parse_from(["gba", "init"]).expect("should parse init");
        assert!(matches!(cli.command, Command::Init));
        assert!(cli.workdir.is_none());
        assert!(!cli.verbose);
    }

    #[test]
    fn test_should_parse_list_command() {
        let cli = Cli::try_parse_from(["gba", "list"]).expect("should parse list");
        assert!(matches!(cli.command, Command::List));
    }

    #[test]
    fn test_should_parse_status_command() {
        let cli =
            Cli::try_parse_from(["gba", "status", "my-feature"]).expect("should parse status");
        match cli.command {
            Command::Status { slug } => assert_eq!(slug, "my-feature"),
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn test_should_parse_plan_command() {
        let cli =
            Cli::try_parse_from(["gba", "plan", "add-auth"]).expect("should parse plan command");
        match cli.command {
            Command::Plan { slug } => assert_eq!(slug, "add-auth"),
            _ => panic!("expected Plan command"),
        }
    }

    #[test]
    fn test_should_parse_run_command_with_options() {
        let cli =
            Cli::try_parse_from(["gba", "run", "my-feature", "--from-phase", "2", "--dry-run"])
                .expect("should parse run");
        match cli.command {
            Command::Run {
                slug,
                from_phase,
                dry_run,
                restart,
            } => {
                assert_eq!(slug, "my-feature");
                assert_eq!(from_phase, Some(2));
                assert!(dry_run);
                assert!(!restart);
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn test_should_parse_global_options() {
        let cli = Cli::try_parse_from(["gba", "-v", "-w", "/tmp/repo", "init"])
            .expect("should parse with global options");
        assert!(cli.verbose);
        assert_eq!(cli.workdir, Some(PathBuf::from("/tmp/repo")));
    }

    #[test]
    fn test_should_parse_clean_command() {
        let cli = Cli::try_parse_from(["gba", "clean"]).expect("should parse clean");
        match cli.command {
            Command::Clean { dry_run, force } => {
                assert!(!dry_run);
                assert!(!force);
            }
            _ => panic!("expected Clean command"),
        }
    }

    #[test]
    fn test_should_parse_clean_command_with_options() {
        let cli = Cli::try_parse_from(["gba", "clean", "--dry-run", "--force"])
            .expect("should parse clean with options");
        match cli.command {
            Command::Clean { dry_run, force } => {
                assert!(dry_run);
                assert!(force);
            }
            _ => panic!("expected Clean command"),
        }
    }
}
