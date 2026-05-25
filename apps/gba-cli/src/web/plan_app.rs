//! Plan mode web application.
//!
//! Provides a browser-based chat interface for interactive feature planning.
//! Spawns a local HTTP/WebSocket server, bridges WebSocket messages to the
//! Claude session via channels, and handles graceful shutdown.
//!
//! The worker_loop and event handler are adapted from the original TUI
//! implementation but communicate over WebSocket instead of terminal UI.

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures::StreamExt;
use serde_json::json;
use tokio::sync::{Mutex, mpsc, watch};
use tracing::{debug, info, warn};

use gba_core::{Engine, Session, TaskKind};

use crate::error::CliError;

use super::protocol::{PlanClientMessage, PlanServerMessage};
use super::server;

// ---------------------------------------------------------------------------
// Internal channel types
// ---------------------------------------------------------------------------

/// Messages sent from the WebSocket handler to the worker task.
#[derive(Debug)]
enum WorkerRequest {
    /// Send a chat message to Claude.
    Send(String),
    /// Shutdown the worker.
    Shutdown,
}

/// Session statistics.
#[derive(Debug, Clone, Default)]
struct SessionStats {
    turns: u32,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
}

/// Messages sent from the worker task to the WebSocket handler.
#[derive(Debug)]
enum WorkerEvent {
    /// Streaming text chunk from Claude.
    Text(String),
    /// Activity status update (e.g. tool usage).
    Status(String),
    /// Response complete with stats.
    Complete(SessionStats),
    /// Error occurred.
    Error(String),
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared state accessible by axum handlers.
struct PlanState {
    /// Sender for requests from WS to worker.
    request_tx: mpsc::Sender<WorkerRequest>,
    /// Receiver for events from worker to WS.
    worker_rx: Arc<Mutex<mpsc::Receiver<WorkerEvent>>>,
    /// Shutdown signal for the server.
    shutdown_tx: watch::Sender<bool>,
}

// ---------------------------------------------------------------------------
// WebPlanApp
// ---------------------------------------------------------------------------

/// Web-based plan application.
pub struct WebPlanApp {
    /// Feature slug being planned.
    feature_slug: String,
    /// Feature ID (e.g., "0001").
    feature_id: String,
    /// Base branch for the feature.
    base_branch: String,
    /// Working directory path.
    workdir: std::path::PathBuf,
    /// Web server host.
    host: String,
    /// Web server port.
    port: u16,
}

impl std::fmt::Debug for WebPlanApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebPlanApp")
            .field("feature_slug", &self.feature_slug)
            .field("feature_id", &self.feature_id)
            .field("base_branch", &self.base_branch)
            .finish()
    }
}

impl WebPlanApp {
    /// Create a new web plan application.
    pub fn new(
        feature_slug: String,
        feature_id: String,
        base_branch: String,
        workdir: &Path,
    ) -> Self {
        Self {
            feature_slug,
            feature_id,
            base_branch,
            workdir: workdir.to_path_buf(),
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

    /// Run the web plan application.
    ///
    /// Starts the web server, opens the browser, and handles the WebSocket
    /// connection to bridge chat messages between the browser and Claude.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start, the browser cannot be opened,
    /// or the Claude session fails.
    pub async fn run(&self, engine: &Engine<'_>) -> Result<(), CliError> {
        // Create Claude session
        let context = json!({
            "repo_path": self.workdir.display().to_string(),
            "feature_id": self.feature_id,
            "feature_slug": self.feature_slug,
            "base_branch": self.base_branch,
        });

        let mut session = engine
            .session_with_task(&TaskKind::Plan, &context, None)
            .map_err(CliError::Engine)?;

        session.connect().await.map_err(CliError::Engine)?;

        // Create channels
        let (request_tx, request_rx) = mpsc::channel::<WorkerRequest>(10);
        let (worker_tx, worker_rx) = mpsc::channel::<WorkerEvent>(100);

        // Spawn worker task
        let worker_handle = tokio::spawn(async move {
            plan_worker_loop(session, request_rx, worker_tx).await;
        });

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = server::shutdown_channel();

        // Build shared state
        let state = Arc::new(PlanState {
            request_tx: request_tx.clone(),
            worker_rx: Arc::new(Mutex::new(worker_rx)),
            shutdown_tx: shutdown_tx.clone(),
        });

        // Build router -- include all routes before with_state() so the router
        // type is inferred as Router<Arc<PlanState>>. Then with_state(state)
        // converts to Router<()> which axum::serve expects.
        let app = Router::new()
            .route("/plan", get(serve_plan_html))
            .route("/run", get(serve_run_html))
            .route("/ws/plan", get(ws_plan_upgrade))
            .with_state(state);

        // Bind to an available port
        let (listener, addr) = server::bind_tcp_listener(&self.host, self.port).await?;
        let url = format!("http://{}/plan", addr);
        println!("Opening browser: {}", url);

        // Open browser
        if let Err(e) = webbrowser::open(&url) {
            warn!(error = %e, "failed to open browser");
            println!("Please open {} manually in your browser.", url);
        }

        // Serve with graceful shutdown
        server::serve_with_graceful_shutdown(listener, app, shutdown_rx).await?;

        // Signal worker shutdown
        let _ = request_tx.send(WorkerRequest::Shutdown).await;
        let _ = worker_handle.await;

        info!("web plan app shut down");
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

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// WebSocket upgrade handler for plan mode.
async fn ws_plan_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<PlanState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_plan_ws(socket, state))
}

/// Serialize a message as JSON and send it over WebSocket.
///
/// Returns `Ok(())` on success, `Err(axum::Error)` on failure.
async fn send_ws_json(socket: &mut WebSocket, msg: &PlanServerMessage) -> Result<(), axum::Error> {
    let json =
        serde_json::to_string(msg).map_err(|e| axum::Error::new(std::io::Error::other(e)))?;
    socket.send(Message::Text(json.into())).await
}

/// Handle a plan mode WebSocket connection.
///
/// Bridges client messages to the worker and worker events back to the client.
/// When the WebSocket closes, sends a shutdown request to the worker.
async fn handle_plan_ws(mut socket: WebSocket, state: Arc<PlanState>) {
    debug!("plan WebSocket connected");

    // Send ready message
    if let Ok(msg) = serde_json::to_string(&PlanServerMessage::Ready)
        && socket.send(Message::Text(msg.into())).await.is_err()
    {
        return;
    }

    // Lock the worker event receiver
    let mut worker_rx = state.worker_rx.lock().await;

    loop {
        tokio::select! {
            // Worker event -> send to client
            event = worker_rx.recv() => {
                match event {
                    Some(WorkerEvent::Text(text)) => {
                        let msg = PlanServerMessage::Text { content: text };
                        if send_ws_json(&mut socket, &msg).await.is_err() {
                            break;
                        }
                    }
                    Some(WorkerEvent::Status(activity)) => {
                        let msg = PlanServerMessage::StatusUpdate { activity };
                        if send_ws_json(&mut socket, &msg).await.is_err() {
                            break;
                        }
                    }
                    Some(WorkerEvent::Complete(stats)) => {
                        let msg = PlanServerMessage::Complete {
                            turns: stats.turns,
                            input_tokens: stats.input_tokens,
                            output_tokens: stats.output_tokens,
                            cost_usd: stats.cost_usd,
                        };
                        if send_ws_json(&mut socket, &msg).await.is_err() {
                            break;
                        }
                    }
                    Some(WorkerEvent::Error(err)) => {
                        let msg = PlanServerMessage::Error { message: err };
                        if send_ws_json(&mut socket, &msg).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        // Worker channel closed
                        break;
                    }
                }
            }

            // Client message -> forward to worker
            ws_msg = socket.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<PlanClientMessage>(&text) {
                            Ok(PlanClientMessage::Chat { content }) => {
                                if state.request_tx.send(WorkerRequest::Send(content)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(PlanClientMessage::Done) => {
                                let _ = send_ws_json(&mut socket, &PlanServerMessage::Complete {
                                    turns: 0,
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    cost_usd: 0.0,
                                }).await;
                                break;
                            }
                            Ok(PlanClientMessage::Cancel) => {
                                let _ = send_ws_json(&mut socket, &PlanServerMessage::Cancelled).await;
                                break;
                            }
                            Err(e) => {
                                warn!(error = %e, "failed to parse client message");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("plan WebSocket closed by client");
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore ping/pong/binary
                    }
                    Some(Err(e)) => {
                        warn!(error = %e, "plan WebSocket error");
                        break;
                    }
                }
            }
        }
    }

    // Signal shutdown
    let _ = state.request_tx.send(WorkerRequest::Shutdown).await;
    let _ = state.shutdown_tx.send(false);

    debug!("plan WebSocket handler finished");
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

async fn plan_worker_loop(
    mut session: Session,
    mut request_rx: mpsc::Receiver<WorkerRequest>,
    worker_tx: mpsc::Sender<WorkerEvent>,
) {
    debug!("plan worker loop started");

    while let Some(request) = request_rx.recv().await {
        match request {
            WorkerRequest::Send(message) => {
                debug!(message_len = message.len(), "worker received message");

                let mut handler = PlanEventHandler::new(worker_tx.clone());

                match session.send_stream(&message, &mut handler).await {
                    Ok(_) => {
                        // Complete event is already sent by PlanEventHandler::on_complete_with_stats
                        debug!("plan worker: send_stream completed");
                    }
                    Err(e) => {
                        let _ = worker_tx.send(WorkerEvent::Error(e.to_string())).await;
                    }
                }
            }
            WorkerRequest::Shutdown => {
                debug!("worker received shutdown signal");
                if let Err(e) = session.disconnect().await {
                    warn!(error = %e, "failed to disconnect session in worker");
                }
                break;
            }
        }
    }

    debug!("plan worker loop finished");
}

// ---------------------------------------------------------------------------
// Event handler
// ---------------------------------------------------------------------------

struct PlanEventHandler {
    tx: mpsc::Sender<WorkerEvent>,
}

impl PlanEventHandler {
    fn new(tx: mpsc::Sender<WorkerEvent>) -> Self {
        Self { tx }
    }
}

impl gba_core::event::EventHandler for PlanEventHandler {
    fn on_text(&mut self, text: &str) {
        let text = if text.ends_with('\n') {
            text.to_string()
        } else {
            format!("{text}\n")
        };
        let _ = self.tx.try_send(WorkerEvent::Text(text));
    }

    fn on_tool_use(&mut self, tool: &str, input: &serde_json::Value) {
        let activity = format_tool_activity(tool, input);
        let _ = self.tx.try_send(WorkerEvent::Status(activity));
    }

    fn on_tool_result(&mut self, _result: &str) {
        // No-op
    }

    fn on_error(&mut self, error: &str) {
        let _ = self.tx.try_send(WorkerEvent::Error(error.to_string()));
    }

    fn on_complete_with_stats(
        &mut self,
        turns: u32,
        cost_usd: f64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        debug!(
            turns,
            cost_usd, input_tokens, output_tokens, "plan streaming complete"
        );
        let _ = self.tx.try_send(WorkerEvent::Complete(SessionStats {
            turns,
            input_tokens,
            output_tokens,
            cost_usd,
        }));
    }
}

/// Build a human-readable activity description from a tool-use event.
fn format_tool_activity(tool: &str, input: &serde_json::Value) -> String {
    match tool {
        "Read" | "read_file" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Reading {path}")
        }
        "Write" | "write_file" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Writing {path}")
        }
        "Edit" | "edit_file" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Editing {path}")
        }
        "Bash" | "bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let display = if cmd.len() > 60 {
                format!("{}...", &cmd[..57])
            } else {
                cmd.to_string()
            };
            format!("Running: {display}")
        }
        "Glob" | "glob" => {
            let pattern = input
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Searching: {pattern}")
        }
        "Grep" | "grep" => {
            let pattern = input
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Searching for: {pattern}")
        }
        other => format!("Using tool: {other}"),
    }
}
