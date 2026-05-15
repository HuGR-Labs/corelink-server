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

use corelink_billing_stripe_materializer::{
    BillingD1Writer, D1IdempotencyStore, D1SubscriptionStateHandler, InMemoryBillingAuditEmitter,
    InMemoryBillingD1, InMemoryTierSelector, RealStripeAuditEmitter,
};
use corelink_server::webhook::{router as webhook_router, WebhookState};
use corelink_stripe_real::webhook_dispatch::{
    RecordingSliRecorder, StateMaterializer, SystemClock, WebhookDispatcher,
};
use corelink_tier_selection::tier::TierKind;
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

        // Wave 17: the HTTP shell now binds the production
        // materializer + audit emitter + D1-backed idempotency store
        // from `corelink-billing-stripe-materializer`. On native
        // dev/CI runs the `InMemoryBillingD1` + `InMemoryBillingAuditEmitter`
        // are used (no D1 endpoint reachable from outside CF Worker
        // anyway); on wasm32 the production binder swaps these to the
        // `corelink-cf-bindings::CfD1DatabaseReal` D1 adapter + the
        // `corelink-audit-chain` `ArchiveProducer` sink behind the
        // same `BillingD1Writer` / `BillingAuditEmitter` traits.
        //
        // The single canonical seam (`WebhookDispatcher::new`) is
        // preserved end-to-end — axum, CF Worker, and the replay
        // cron all hit this exact constructor with target-specific
        // collaborators.
        let billing_d1: Arc<dyn BillingD1Writer> = Arc::new(InMemoryBillingD1::new());
        let billing_audit = Arc::new(InMemoryBillingAuditEmitter::new());
        // Canonical Stripe-plan-id → tier mapping. Production
        // operators flip these via the workspace tier config; the
        // defaults here mirror the canonical 5-tier taxonomy from
        // `corelink-tier-selection::tier::TierKind` so a fresh
        // deployment without overrides still classifies the four
        // production plans correctly.
        let tier_selector = Arc::new(InMemoryTierSelector::with_mapping(&[
            ("plan_starter", TierKind::Starter),
            ("plan_team", TierKind::Team),
            ("plan_pro", TierKind::Pro),
        ]));
        let materializer: Arc<dyn StateMaterializer> = Arc::new(D1SubscriptionStateHandler::new(
            billing_d1.clone(),
            billing_audit.clone(),
            tier_selector,
        ));
        let dispatcher_audit = Arc::new(RealStripeAuditEmitter::new(billing_audit.clone()));
        let idempotency = Arc::new(D1IdempotencyStore::new(billing_d1.clone()));
        let dispatcher = Arc::new(WebhookDispatcher::new(
            secret.into_bytes(),
            idempotency,
            materializer,
            dispatcher_audit,
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
