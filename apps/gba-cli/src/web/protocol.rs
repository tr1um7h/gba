//! WebSocket protocol message types.
//!
//! Defines the JSON messages exchanged between the browser and the server
//! for both plan mode (chat) and run mode (progress dashboard).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Plan mode messages
// ---------------------------------------------------------------------------

/// Messages sent from the browser to the server in plan mode.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlanClientMessage {
    /// User sends a chat message.
    Chat { content: String },
    /// User requests to end planning.
    Done,
    /// User requests cancellation.
    Cancel,
}

/// Messages sent from the server to the browser in plan mode.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlanServerMessage {
    /// Server is ready for input.
    Ready,
    /// Streaming text chunk from the assistant.
    Text { content: String },
    /// Session completed with stats.
    Complete {
        turns: u32,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
    },
    /// Session was cancelled.
    Cancelled,
    /// An error occurred.
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Run mode messages
// ---------------------------------------------------------------------------

/// Messages sent from the browser to the server in run mode.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunClientMessage {
    /// User requests cancellation.
    Cancel,
}

/// Messages sent from the server to the browser in run mode.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunServerMessage {
    /// Server is ready.
    Ready,
    /// Phase started.
    PhaseStarted { index: usize, name: String },
    /// Phase completed.
    PhaseCompleted {
        index: usize,
        commit_sha: Option<String>,
    },
    /// Phase failed.
    PhaseFailed { index: usize, error: String },
    /// Update stats.
    StatsUpdate { turns: u32, cost_usd: f64 },
    /// Activity message update.
    Activity { message: String },
    /// Check started (review or verification).
    CheckStarted {
        check_type: String,
        max_iterations: u32,
    },
    /// Check iteration started.
    CheckIterationStarted {
        check_type: String,
        iteration: u32,
        max_iterations: u32,
    },
    /// Check iteration result.
    CheckIterationResult {
        check_type: String,
        iteration: u32,
        result: String,
    },
    /// Fix started for a check.
    FixStarted { check_type: String, iteration: u32 },
    /// Fix completed for a check.
    FixCompleted {
        check_type: String,
        iteration: u32,
        success: bool,
    },
    /// Check completed.
    CheckCompleted { check_type: String, result: String },
    /// PR creation started.
    PrCreationStarted,
    /// PR creation completed.
    PrCreationCompleted { pr_url: Option<String> },
    /// Execution complete.
    Complete,
    /// An error occurred.
    Error { message: String },
    /// Streaming text chunk.
    Text { content: String },
}
