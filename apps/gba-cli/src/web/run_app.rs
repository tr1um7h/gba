//! Run mode web application.
//!
//! This module implements the run mode Web UI that replaces the TUI-based
//! `gba run` command. It bridges the execution pipeline's `RunMessage` channel
//! to a WebSocket connection, enabling browser-based progress monitoring.
//!
//! The run mode is simpler than plan mode because it's one-directional:
//! the pipeline sends `RunMessage` events, and we forward them to the browser.
//! The only client-initiated message is `Cancel`.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures::StreamExt;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::error::CliError;

use super::protocol::{RunClientMessage, RunServerMessage};
use super::server;

// ---------------------------------------------------------------------------
// Re-exported types (used by run.rs)
// ---------------------------------------------------------------------------

/// Event handler that sends streaming events to a channel.
///
/// This is used by `run.rs` to forward engine events into the run mode pipeline.
pub struct TuiEventHandler {
    tx: mpsc::Sender<RunMessage>,
}

impl TuiEventHandler {
    /// Create a new event handler.
    pub fn new(tx: mpsc::Sender<RunMessage>) -> Self {
        Self { tx }
    }
}

impl gba_core::event::EventHandler for TuiEventHandler {
    fn on_text(&mut self, text: &str) {
        let text = if text.ends_with('\n') {
            text.to_string()
        } else {
            format!("{text}\n")
        };
        let _ = self.tx.try_send(RunMessage::Text(text));
    }

    fn on_tool_use(&mut self, tool: &str, _input: &serde_json::Value) {
        debug!(tool = tool, "tool use during execution");
    }

    fn on_tool_result(&mut self, _result: &str) {
        // No-op
    }

    fn on_error(&mut self, error: &str) {
        let _ = self.tx.try_send(RunMessage::Error(error.to_string()));
    }

    fn on_complete(&mut self) {
        debug!("execution streaming complete");
    }
}

/// Type of check being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckType {
    /// Code review check.
    Review,
    /// Verification check.
    Verification,
}

impl CheckType {
    /// Get the display name for this check type.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Review => "Code Review",
            Self::Verification => "Verification",
        }
    }
}

/// Result of a single check iteration.
#[derive(Debug, Clone)]
pub enum CheckIterationResult {
    /// Check passed successfully.
    Passed,
    /// Check found issues that need to be addressed.
    NeedsChanges(String),
    /// Check itself failed to run.
    Error(String),
}

/// Final result of a check-fix loop.
#[derive(Debug, Clone)]
pub enum CheckFinalResult {
    /// Check passed successfully.
    Passed,
    /// Check still needs changes after max iterations.
    NeedsChanges(String),
    /// Check encountered an error.
    Error(String),
    /// Check was skipped.
    Skipped(String),
}

/// Messages sent to the run UI from the execution worker.
#[derive(Debug)]
pub enum RunMessage {
    /// Streaming text chunk.
    Text(String),
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
    Activity(String),
    /// Check started (review or verification).
    CheckStarted {
        check_type: CheckType,
        max_iterations: u32,
    },
    /// Check iteration started.
    CheckIterationStarted {
        check_type: CheckType,
        iteration: u32,
        max_iterations: u32,
    },
    /// Check iteration result.
    CheckIterationResult {
        check_type: CheckType,
        iteration: u32,
        result: CheckIterationResult,
    },
    /// Fix started for a check.
    FixStarted {
        check_type: CheckType,
        iteration: u32,
    },
    /// Fix completed for a check.
    FixCompleted {
        check_type: CheckType,
        iteration: u32,
        success: bool,
    },
    /// Check completed (review or verification).
    CheckCompleted {
        check_type: CheckType,
        result: CheckFinalResult,
    },
    /// PR creation started.
    PrCreationStarted,
    /// PR creation completed.
    PrCreationCompleted { pr_url: Option<String> },
    /// Execution complete.
    Complete,
    /// Error occurred.
    Error(String),
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared state accessible by axum handlers.
struct RunState {
    /// Receiver for run messages from the pipeline.
    run_rx: Arc<Mutex<mpsc::Receiver<RunMessage>>>,
    /// Shutdown signal for the server.
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

// ---------------------------------------------------------------------------
// WebRunApp
// ---------------------------------------------------------------------------

/// Web-based run application.
pub struct WebRunApp {
    /// Feature slug being executed.
    feature_slug: String,
    /// Web server host.
    host: String,
    /// Web server port.
    port: u16,
}

impl std::fmt::Debug for WebRunApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebRunApp")
            .field("feature_slug", &self.feature_slug)
            .finish()
    }
}

impl WebRunApp {
    /// Create a new web run application from feature state.
    pub fn new(state: &crate::state::FeatureState) -> Self {
        Self {
            feature_slug: state.feature.slug.clone(),
            host: "127.0.0.1".to_string(),
            port: 3456,
        }
    }

    /// Set the host for the web server.
    pub fn with_host(mut self, host: String) -> Self {
        self.host = host;
        self
    }

    /// Set the port for the web server.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Run the web run application.
    ///
    /// Starts the web server, opens the browser, and bridges `RunMessage` events
    /// to the WebSocket.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or the browser cannot be opened.
    pub async fn run(self, rx: mpsc::Receiver<RunMessage>) -> Result<(), CliError> {
        let (shutdown_tx, shutdown_rx) = server::shutdown_channel();

        let state = Arc::new(RunState {
            run_rx: Arc::new(Mutex::new(rx)),
            shutdown_tx: shutdown_tx.clone(),
        });

        let app = Router::new()
            .route("/plan", get(serve_plan_html))
            .route("/run", get(serve_run_html))
            .route("/ws/run", get(ws_run_upgrade))
            .with_state(state);

        let (listener, addr) = server::bind_tcp_listener(&self.host, self.port).await?;
        let url = format!("http://{addr}/run");
        println!("Opening browser: {url}");

        if let Err(e) = webbrowser::open(&url) {
            tracing::warn!(error = %e, "failed to open browser");
            println!("Please open {url} manually in your browser.");
        }

        server::serve_with_graceful_shutdown(listener, app, shutdown_rx).await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

async fn serve_plan_html() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("assets/plan.html"))
}

async fn serve_run_html() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("assets/run.html"))
}

/// WebSocket upgrade handler for run mode.
async fn ws_run_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RunState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_run_ws(socket, state))
}

/// Handle a run mode WebSocket connection.
///
/// Bridges pipeline messages to the client and handles cancel requests.
async fn handle_run_ws(mut socket: WebSocket, state: Arc<RunState>) {
    debug!("run WebSocket connected");

    let mut run_rx = state.run_rx.lock().await;

    loop {
        tokio::select! {
            msg = run_rx.recv() => {
                match msg {
                    Some(run_msg) => {
                        let server_msg = convert_run_message(run_msg);
                        if send_ws_json(&mut socket, &server_msg).await.is_err() {
                            break;
                        }

                        if matches!(server_msg, RunServerMessage::Complete) {
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                            break;
                        }
                    }
                    None => {
                        break;
                    }
                }
            }

            ws_msg = socket.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<RunClientMessage>(&text) {
                            Ok(RunClientMessage::Cancel) => {
                                debug!("run cancel requested by client");
                                let _ = send_ws_json(&mut socket, &RunServerMessage::Error {
                                    message: "Cancelled by user".to_string(),
                                }).await;
                                break;
                            }
                            Err(e) => {
                                warn!(error = %e, "failed to parse run client message");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("run WebSocket closed by client");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        warn!(error = %e, "run WebSocket error");
                        break;
                    }
                }
            }
        }
    }

    let _ = state.shutdown_tx.send(false);
    debug!("run WebSocket handler finished");
}

// ---------------------------------------------------------------------------
// Message conversion
// ---------------------------------------------------------------------------

fn convert_run_message(msg: RunMessage) -> RunServerMessage {
    match msg {
        RunMessage::Text(text) => RunServerMessage::Text { content: text },
        RunMessage::PhaseStarted { index, name } => RunServerMessage::PhaseStarted { index, name },
        RunMessage::PhaseCompleted { index, commit_sha } => {
            RunServerMessage::PhaseCompleted { index, commit_sha }
        }
        RunMessage::PhaseFailed { index, error } => RunServerMessage::PhaseFailed { index, error },
        RunMessage::StatsUpdate { turns, cost_usd } => {
            RunServerMessage::StatsUpdate { turns, cost_usd }
        }
        RunMessage::Activity(message) => RunServerMessage::Activity { message },
        RunMessage::CheckStarted {
            check_type,
            max_iterations,
        } => RunServerMessage::CheckStarted {
            check_type: check_type.name().to_string(),
            max_iterations,
        },
        RunMessage::CheckIterationStarted {
            check_type,
            iteration,
            max_iterations,
        } => RunServerMessage::CheckIterationStarted {
            check_type: check_type.name().to_string(),
            iteration,
            max_iterations,
        },
        RunMessage::CheckIterationResult {
            check_type,
            iteration,
            result,
        } => {
            let result_str = match &result {
                CheckIterationResult::Passed => "passed".to_string(),
                CheckIterationResult::NeedsChanges(s) => {
                    format!("needsChanges: {}", s)
                }
                CheckIterationResult::Error(e) => format!("error: {}", e),
            };
            RunServerMessage::CheckIterationResult {
                check_type: check_type.name().to_string(),
                iteration,
                result: result_str,
            }
        }
        RunMessage::FixStarted {
            check_type,
            iteration,
        } => RunServerMessage::FixStarted {
            check_type: check_type.name().to_string(),
            iteration,
        },
        RunMessage::FixCompleted {
            check_type,
            iteration,
            success,
        } => RunServerMessage::FixCompleted {
            check_type: check_type.name().to_string(),
            iteration,
            success,
        },
        RunMessage::CheckCompleted { check_type, result } => {
            let result_str = match &result {
                CheckFinalResult::Passed => "passed".to_string(),
                CheckFinalResult::NeedsChanges(s) => {
                    format!("needsChanges: {}", s)
                }
                CheckFinalResult::Error(e) => format!("error: {}", e),
                CheckFinalResult::Skipped(s) => format!("skipped: {}", s),
            };
            RunServerMessage::CheckCompleted {
                check_type: check_type.name().to_string(),
                result: result_str,
            }
        }
        RunMessage::PrCreationStarted => RunServerMessage::PrCreationStarted,
        RunMessage::PrCreationCompleted { pr_url } => {
            RunServerMessage::PrCreationCompleted { pr_url }
        }
        RunMessage::Complete => RunServerMessage::Complete,
        RunMessage::Error(message) => RunServerMessage::Error { message },
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

async fn send_ws_json(socket: &mut WebSocket, msg: &RunServerMessage) -> Result<(), axum::Error> {
    let json =
        serde_json::to_string(msg).map_err(|e| axum::Error::new(std::io::Error::other(e)))?;
    futures::SinkExt::send(socket, Message::Text(json.into())).await
}
