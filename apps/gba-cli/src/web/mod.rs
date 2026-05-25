//! Web UI module for GBA.
//!
//! Replaces the terminal-based TUI with a browser-based interface using axum
//! (HTTP + WebSocket) and vanilla HTML/CSS/JS frontend.
//!
//! # Architecture
//!
//! ```text
//! plan.rs  ->  WebPlanApp.run(engine)  ->  worker_loop()  ->  Session.send_stream()
//!                ^                              |
//!                |--- mpsc::channel -----------|
//!                +---> axum server --> WS -----> browser
//!
//! run.rs   ->  WebRunApp.run(rx)     ->  (receives RunMessage)
//!                ^                              |
//!                |--- mpsc::channel -----------|
//!                +---> axum server --> WS -----> browser
//! ```

mod plan_app;
mod protocol;
mod run_app;
mod server;

pub use plan_app::WebPlanApp;
pub use run_app::{
    CheckFinalResult, CheckIterationResult, CheckType, RunMessage, TuiEventHandler, WebRunApp,
};
