//! Boot-time state plus the HTTP liveness and graceful-shutdown helpers.

use std::sync::OnceLock;

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use tokio::signal;
use tracing::info;

/// Storage backing kind captured once before the listener binds.
static STORAGE_BACKING: OnceLock<&'static str> = OnceLock::new();

/// Record the backing selected during startup for the liveness response.
pub(super) fn record_storage_backing(backing: &'static str) {
    // `main` is the only caller and runs before the listener binds.
    let _ = STORAGE_BACKING.set(backing);
}

/// Liveness probe for the DO and production smoke checks.
///
/// Its stable body is `{"status":"ok","storage":"r2"|"inmemory"}` and its
/// JSON content type lets operators detect the InMemory fallback without
/// tailing logs.
pub(super) async fn health_handler() -> impl IntoResponse {
    let backing = STORAGE_BACKING.get().copied().unwrap_or("unknown");
    let body = format!(r#"{{"status":"ok","storage":"{}"}}"#, backing);
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
}

/// Resolve after a rollout SIGTERM or local SIGINT, letting axum drain active
/// requests before the container exits.
pub(super) async fn shutdown_signal() {
    let term = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(error = %error, "tokio::signal::unix unavailable; falling back to ctrl_c");
                let _ = signal::ctrl_c().await;
            }
        }
    };
    let interrupt = signal::ctrl_c();

    tokio::select! {
        _ = term => info!(signal = "SIGTERM", "graceful shutdown signal received — stopping accept and draining in-flight requests"),
        _ = interrupt => info!(signal = "SIGINT", "graceful shutdown signal received — stopping accept and draining in-flight requests"),
    }
}
