//! Web server utilities for the browser-based UI.
//!
//! Provides TCP binding with port fallback and shutdown channel management.

use std::net::SocketAddr;

use axum::Router;
use tokio::sync::watch;
use tracing::{debug, info};

use crate::error::CliError;

/// Maximum number of port retry attempts when the configured port is occupied.
const MAX_PORT_RETRIES: u32 = 10;

/// Bind a TCP listener, trying the configured port with fallback.
///
/// Tries the configured port first, then increments up to `MAX_PORT_RETRIES`
/// times, then falls back to OS-assigned port 0.
///
/// # Errors
///
/// Returns an error if binding fails after all retries.
pub async fn bind_tcp_listener(
    host: &str,
    port: u16,
) -> Result<(tokio::net::TcpListener, SocketAddr), CliError> {
    for attempt in 0..=MAX_PORT_RETRIES {
        let try_port = if attempt == MAX_PORT_RETRIES {
            0 // OS-assigned
        } else {
            port.saturating_add(attempt as u16)
        };

        let addr_str = format!("{host}:{try_port}");
        let listener = match tokio::net::TcpListener::bind(&addr_str).await {
            Ok(l) => l,
            Err(e) => {
                if attempt < MAX_PORT_RETRIES {
                    debug!(
                        port = try_port,
                        error = %e,
                        "port occupied, trying next"
                    );
                    continue;
                }
                return Err(CliError::Io(format!(
                    "failed to bind web server after {} attempts: {}",
                    MAX_PORT_RETRIES + 1,
                    e
                )));
            }
        };

        let local_addr = listener
            .local_addr()
            .map_err(|e| CliError::Io(format!("failed to get local address: {e}")))?;

        info!(host = host, port = local_addr.port(), "web server bound");

        return Ok((listener, local_addr));
    }

    unreachable!()
}

/// Create a watch channel for graceful shutdown.
///
/// Returns the sender (to trigger shutdown) and receiver (to wait for it).
pub fn shutdown_channel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
    watch::channel(true)
}

/// Run the axum server with graceful shutdown support.
///
/// # Errors
///
/// Returns an error if the server encounters an unrecoverable error.
pub async fn serve_with_graceful_shutdown(
    listener: tokio::net::TcpListener,
    app: Router,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<(), CliError> {
    axum::serve(listener, app)
        .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
        .await
        .map_err(|e| CliError::Io(format!("web server error: {e}")))?;

    info!("web server stopped");
    Ok(())
}

/// Wait for the shutdown signal from the watch channel.
async fn wait_for_shutdown(mut rx: watch::Receiver<bool>) {
    if rx.changed().await.is_ok() {
        info!("received shutdown signal");
    }
}
