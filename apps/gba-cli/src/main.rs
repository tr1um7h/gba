//! GBA CLI - Command Line Interface for Geektime Bootcamp Agent.
//!
//! This is the main entry point for the GBA CLI application.
//! It parses command-line arguments and dispatches to the appropriate
//! command handler.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use tokio::signal;
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

mod cli;
mod commands;
mod config;
mod error;
mod state;
pub mod utils;
pub mod web;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Determine working directory early for log file location
    let workdir = cli
        .workdir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Setup logging with file output
    setup_logging(&workdir, cli.verbose);

    // Run the command with Ctrl+C handling
    tokio::select! {
        result = run_command(cli.command, &workdir, cli.verbose) => {
            result
        }
        _ = signal::ctrl_c() => {
            // Graceful shutdown on Ctrl+C
            println!("\nInterrupted. Shutting down...");
            Ok(())
        }
    }
}

/// Dispatch to command handler.
async fn run_command(command: Command, workdir: &Path, verbose: bool) -> Result<()> {
    match command {
        Command::Init => {
            commands::run_init(workdir, verbose).await?;
        }
        Command::List => {
            commands::run_list(workdir).await?;
        }
        Command::Status { slug } => {
            commands::run_status(workdir, &slug).await?;
        }
        Command::Plan { slug } => {
            commands::run_plan(workdir, &slug, verbose).await?;
        }
        Command::Run {
            slug,
            from_phase,
            dry_run,
            restart,
        } => {
            let options = commands::RunOptions {
                from_phase,
                dry_run,
                restart,
            };
            commands::run_run(workdir, &slug, options).await?;
        }
        Command::Clean { dry_run, force } => {
            commands::run_clean(workdir, dry_run, force).await?;
        }
        Command::Remove { slug, force } => {
            commands::run_remove(workdir, &slug, force).await?;
        }
        Command::Recover { slug } => {
            commands::run_recover(workdir, &slug).await?;
        }
    }

    Ok(())
}

/// Setup logging with console (WARN level) and file (INFO level) outputs.
///
/// Logs are written to `.gba/logs/gba.log` in the working directory.
/// The file appender uses daily rotation.
fn setup_logging(workdir: &std::path::Path, verbose: bool) {
    // Console layer: WARN by default, DEBUG if verbose
    let console_level = if verbose { Level::DEBUG } else { Level::WARN };
    let console_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .without_time()
        .with_filter(EnvFilter::from_default_env().add_directive(console_level.into()));

    // File layer: always INFO level for diagnostics
    let log_dir = workdir.join(".gba").join("logs");
    if std::fs::create_dir_all(&log_dir).is_ok() {
        let file_appender = tracing_appender::rolling::daily(&log_dir, "gba.log");
        let file_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_ansi(false)
            .with_writer(file_appender)
            .with_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()));

        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        // Fall back to console-only logging if we can't create log directory
        tracing_subscriber::registry().with(console_layer).init();
    }
}
