//! CoreLink server binary entry point.
//!
//! Hosts:
//! - gRPC stack on `PORT` (default 50051): Health + (TODO) CAS / AC /
//!   ByteStream / Capabilities.
//! - HTTP stack on `HTTP_PORT` (default 50052): R2-12 Stripe webhook
//!   route at `/v1/billing/stripe-webhook` (only mounted when
//!   `STRIPE_WEBHOOK_SECRET` is set; absent → HTTP listener is not
//!   started so dev/CI runs without billing wiring stay green).
#![forbid(unsafe_code)]
// The tonic-generated proto module emits structs without docstrings;
// allow at the crate root since the lint is `deny` workspace-wide.
#![allow(missing_docs)]
#![allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    reason = "scaffolding lints — main.rs is the binary entry"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use corelink_server::webhook::{router as webhook_router, WebhookState};
use corelink_stripe_real::webhook_dispatch::{
    InMemoryIdempotencyStore, RecordingAuditEmitter, RecordingSliRecorder,
    RecordingStateMaterializer, SystemClock, WebhookDispatcher,
};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn};

#[allow(missing_docs, reason = "tonic-generated code does not emit docstrings")]
pub mod health {
    tonic::include_proto!("corelink.health.v1");
}

use health::health_server::{Health, HealthServer};
use health::{health_check_response::ServingStatus, HealthCheckRequest, HealthCheckResponse};

struct HealthService;

#[tonic::async_trait]
impl Health for HealthService {
    async fn check(
        &self,
        _req: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: ServingStatus::Serving as i32,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,corelink_server=debug".into()),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50051u16);

    let grpc_addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;

    // R2-12: HTTP server with Stripe webhook route. Only started when
    // STRIPE_WEBHOOK_SECRET is present; otherwise we log and skip so
    // local dev / CI don't fail without billing config.
    if let Ok(secret) = std::env::var("STRIPE_WEBHOOK_SECRET") {
        let http_port: u16 = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50052u16);
        let http_addr: SocketAddr = format!("0.0.0.0:{}", http_port).parse()?;

        // Wave 16: the HTTP shell now binds the canonical
        // `WebhookDispatcher` from `corelink-stripe-real`. The
        // in-memory store / recording materializer / recording audit
        // emitter shipped here are still placeholders for native dev
        // / CI runs — production wiring (S-13+) replaces them with the
        // D1-backed idempotency store, the
        // `corelink-tier-selection`/`corelink-billing-*` ledger
        // adapters, and the `corelink-audit-chain` sink. The
        // dispatcher shape (`WebhookDispatcher::new`) is the single
        // canonical seam used everywhere (axum, CF Worker, replay
        // crons).
        let dispatcher = Arc::new(WebhookDispatcher::new(
            secret.into_bytes(),
            Arc::new(InMemoryIdempotencyStore::new()),
            Arc::new(RecordingStateMaterializer::new()),
            Arc::new(RecordingAuditEmitter::new()),
            Arc::new(RecordingSliRecorder::new()),
            Arc::new(SystemClock),
        ));
        let state = Arc::new(WebhookState::new(dispatcher));
        info!(%http_addr, route = corelink_server::webhook::STRIPE_WEBHOOK_ROUTE,
            "CoreLink HTTP listener starting (Stripe webhook)");
        let app = webhook_router(state);
        let listener = tokio::net::TcpListener::bind(http_addr).await?;
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!(error = %e, "HTTP listener exited");
            }
        });
    } else {
        warn!("STRIPE_WEBHOOK_SECRET unset; Stripe webhook route NOT mounted (dev/CI mode)");
    }

    info!(%grpc_addr, "CoreLink gRPC server starting");

    Server::builder()
        .add_service(HealthServer::new(HealthService))
        .serve(grpc_addr)
        .await?;

    Ok(())
}
