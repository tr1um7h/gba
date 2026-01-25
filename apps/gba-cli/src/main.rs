//! GBA CLI - Command Line Interface for Geektime Bootcamp Agent.
//!
//! This is the main entry point for the GBA CLI application.
//! It parses command-line arguments and dispatches to the appropriate
//! command handler.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

mod cli;
mod commands;
mod config;
mod error;
mod git;
mod state;
pub mod tui;
pub mod utils;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup tracing based on verbose flag
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .without_time()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Determine working directory
    let workdir = cli
        .workdir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Dispatch to command handler
    match cli.command {
        Command::Init => {
            commands::run_init(&workdir, cli.verbose).await?;
        }
        Command::List => {
            commands::run_list(&workdir).await?;
        }
        Command::Status { slug } => {
            commands::run_status(&workdir, &slug).await?;
        }
        Command::Plan { slug } => {
            commands::run_plan(&workdir, &slug, cli.verbose).await?;
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
            commands::run_run(&workdir, &slug, options).await?;
        }
    }

    Ok(())
}
