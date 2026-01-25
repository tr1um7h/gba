//! `gba init` command implementation.
//!
//! Initializes GBA in the current repository by:
//! 1. Creating the `.gba/` directory structure
//! 2. Creating the `.trees/` directory
//! 3. Running the init task via the Engine

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;
use tracing::{debug, info};

use gba_core::event::PrintEventHandler;
use gba_core::{Engine, EngineConfig, Task, TaskKind};
use gba_pm::PromptManager;

use crate::error::CliError;
use crate::utils::{gba_dir, is_initialized, trees_dir};

/// Run the `gba init` command.
///
/// This command initializes GBA in the repository at the given workdir:
/// 1. Checks if GBA is already initialized
/// 2. Creates the Engine with prompts
/// 3. Runs the init task
/// 4. Prints a success message
///
/// # Errors
///
/// Returns an error if:
/// - GBA is already initialized
/// - The init task fails
pub async fn run_init(workdir: &Path, verbose: bool) -> Result<()> {
    // Check if already initialized
    if is_initialized(workdir) {
        return Err(CliError::AlreadyInitialized.into());
    }

    info!(workdir = %workdir.display(), "Initializing GBA");
    println!("Initializing GBA for this project...");

    // Find the GBA installation directory (where tasks/ is located)
    // In production, this would be derived from the executable path
    // For now, we use an environment variable or the current directory
    let gba_install_dir = std::env::var("GBA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            // Try to find tasks directory relative to workdir or current executable
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| workdir.to_path_buf());

            // Check common locations
            for candidate in [
                exe_dir.join("..").join(".."), // Development: target/debug -> project root
                workdir.to_path_buf(),
                std::env::current_dir().unwrap_or_default(),
            ] {
                if candidate.join("tasks").exists() {
                    return candidate;
                }
            }

            workdir.to_path_buf()
        });

    let tasks_dir = gba_install_dir.join("tasks");
    debug!(tasks_dir = %tasks_dir.display(), "Loading prompts from tasks directory");

    if !tasks_dir.exists() {
        return Err(anyhow::anyhow!(
            "Tasks directory not found at {}. Set GBA_HOME environment variable.",
            tasks_dir.display()
        ));
    }

    // Create and configure the prompt manager
    let mut prompts = PromptManager::new();
    prompts
        .load_dir(&tasks_dir)
        .context("Failed to load prompt templates")?;

    // Create the engine
    let config = EngineConfig::builder()
        .workdir(workdir)
        .prompts(prompts)
        .build();

    let engine = Engine::new(config).context("Failed to create engine")?;

    // Create and run the init task
    let task = Task::new(
        TaskKind::Init,
        json!({
            "repo_path": workdir.display().to_string()
        }),
    );

    // Run with streaming to show progress
    let mut handler = if verbose {
        PrintEventHandler::new().with_auto_flush()
    } else {
        PrintEventHandler::new()
    };

    let result = engine
        .run_stream(task, &mut handler)
        .await
        .context("Init task failed")?;

    if result.success {
        println!();
        println!("GBA initialized successfully!");
        println!();
        println!("Created:");
        println!("  - {}/ (GBA configuration)", gba_dir(workdir).display());
        println!("  - {}/ (Git worktrees)", trees_dir(workdir).display());
        println!();
        println!("Next steps:");
        println!("  1. Review the generated .gba.md documentation");
        println!("  2. Run `gba plan <feature-slug>` to plan a new feature");
    } else {
        println!();
        println!("Initialization completed with warnings.");
        if !result.output.is_empty() {
            println!("Output: {}", result.output);
        }
    }

    // Print stats if verbose
    if verbose {
        println!();
        println!("Stats:");
        println!("  Turns: {}", result.stats.turns);
        println!("  Cost: ${:.4}", result.stats.cost_usd);
    }

    Ok(())
}
