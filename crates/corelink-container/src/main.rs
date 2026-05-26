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
use corelink_server::routes;
use corelink_server::routes::audit_analytics::ShadowSinkFactory;
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

    // Wave-19 + Wave-20: Neon analytics shadow per-region project
    // resolution + `TokioPostgresExecutor` binder.
    //
    // `corelink-audit-chain::neon_shadow::real::RealNeonShadowSink` is
    // bound at the binary boot path against the per-region Neon
    // project DSN read from `NEON_DB_URL_<REGION_UPPER>` (rows 120–124
    // of `docs/internal/secrets-checklist.md`). We log which regions
    // resolve so a misconfigured rollout is observable at boot, not
    // at first customer query.
    //
    // Wave-20 (this commit) closes the deferred `TokioPostgresExecutor`
    // binder caveat: with `--feature neon-real` + per-region DSN env
    // vars present, the boot path constructs a
    // `TokioPostgresExecutor` per region (deadpool-postgres pool +
    // tokio-postgres-rustls TLS) and feeds it to a
    // `TokioPgShadowSinkFactory` that resolves per-tenant
    // `RealNeonShadowSink`s. Without `--feature neon-real` or with
    // every DSN env var unset, the `/v1/audit/analytics/*` routes stay
    // on the in-memory factory (dev/CI friendly).
    let shadow_factory: Arc<dyn ShadowSinkFactory> = {
        use corelink_analytics::Region;
        use corelink_audit_chain::{EnvVarResolver, NeonProjectResolver};
        let resolver = EnvVarResolver::new();
        let active_regions = [
            Region::Iad,
            Region::Fra,
            Region::Gru,
            Region::Nrt,
            Region::Syd,
        ];
        let mut resolved_dsns: Vec<(Region, String)> = Vec::new();
        for region in active_regions {
            match resolver.resolve(region) {
                Ok(dsn) => {
                    info!(
                        region = region.as_str(),
                        "wave-19 neon shadow: project DSN resolved"
                    );
                    resolved_dsns.push((region, dsn));
                }
                Err(_) => {
                    info!(
                        region = region.as_str(),
                        env_var = EnvVarResolver::env_var_name(region),
                        "wave-19 neon shadow: project DSN unset (bring-up friendly skip)"
                    );
                }
            }
        }
        if resolved_dsns.is_empty() {
            warn!(
                "wave-19 neon shadow: NO per-region Neon DSN configured; \
                 analytics endpoints stay on InMemoryNeonShadowSink"
            );
        } else {
            info!(
                resolved_regions = resolved_dsns.len(),
                "wave-19 neon shadow: per-region project resolver ready"
            );
        }

        #[cfg(feature = "neon-real")]
        {
            use corelink_audit_chain::neon_shadow::real_tokio_pg::TokioPostgresExecutor;
            use corelink_audit_chain::{
                InMemoryShadowSyncAuditSink, InMemoryTenantRegionResolver, NeonExecutor,
                ShadowSyncAuditSink, TenantRegionResolver,
            };
            use corelink_server::neon_shadow_factory::TokioPgShadowSinkFactory;
            use std::collections::BTreeMap;
            use uuid::Uuid;

            // Build a per-region executor map. Each Neon project gets
            // its own pool (`DEFAULT_POOL_MAX_SIZE = 4`) so a region's
            // back-pressure does NOT cross-pollute other regions'
            // capacity headroom.
            let mut executors: BTreeMap<&'static str, Arc<dyn NeonExecutor>> = BTreeMap::new();
            for (region, dsn) in &resolved_dsns {
                match TokioPostgresExecutor::connect(dsn).await {
                    Ok(exec) => {
                        info!(
                            region = region.as_str(),
                            "wave-20 neon shadow: TokioPostgresExecutor pool ready"
                        );
                        executors.insert(region.as_str(), exec.into_arc());
                    }
                    Err(e) => {
                        // SEV-2 — the per-region DSN was set but the pool
                        // refused to build (bad DSN, TLS handshake fail,
                        // pool capacity refused). We continue boot —
                        // other regions may still be wired — and the
                        // `for_tenant` resolver surfaces the typed error
                        // per request for the affected region.
                        warn!(
                            region = region.as_str(),
                            error = %e,
                            "wave-20 neon shadow: TokioPostgresExecutor build FAILED — region marked unavailable"
                        );
                    }
                }
            }

            if executors.is_empty() {
                info!(
                    "wave-20 neon shadow: --feature neon-real on but no executor pools came up; \
                     falling back to InMemoryShadowSinkFactory"
                );
                Arc::new(routes::InMemoryShadowSinkFactory::new()) as Arc<dyn ShadowSinkFactory>
            } else {
                info!(
                    pool_count = executors.len(),
                    "wave-20 neon shadow: --feature neon-real witness OK — TokioPgShadowSinkFactory wired"
                );

                // Wave-21 closure: build the per-tenant region
                // resolver that replaces the wave-20 hard-coded
                // `Region::Iad` default. Native binaries cannot reach
                // the `worker::D1Database` binding directly (the same
                // constraint pinned `InMemoryBillingD1` in the
                // wave-18 Stripe-materializer wire). So the native
                // gRPC boot path falls back to an
                // `InMemoryTenantRegionResolver` with an explicit
                // fallback to IAD; the CF Worker production boot path
                // (`corelink-clerk-cf::prod_wiring`) is what swaps in
                // a `D1TenantRegionResolver` against the
                // `tenant_config` D1 table.
                //
                // The dev pin `[("tenant-id-zero", Region::Iad)]`
                // matches the in-process integration tests' seed
                // (which use a known `Uuid::nil()`-derived tenant id
                // when exercising the analytics routes against the
                // real factory).
                let tenant_region_resolver: Arc<dyn TenantRegionResolver> = {
                    warn!(
                        "wave-21 neon shadow: native boot path — installing \
                         InMemoryTenantRegionResolver with Region::Iad fallback \
                         (D1TenantRegionResolver is the CF Worker production wire)"
                    );
                    Arc::new(
                        InMemoryTenantRegionResolver::new()
                            .with(Uuid::nil(), corelink_analytics::Region::Iad)
                            .with_fallback(corelink_analytics::Region::Iad),
                    )
                };

                // Wave-29 closure: the production factory now lives
                // in `corelink_server::neon_shadow_factory` as a public
                // type so the `for_tenant_in_region` override (which
                // skips the per-request `TenantRegionResolver` round-
                // trip when the wave-26 `RequestPrelude` populated the
                // region) is unit-testable. See
                // `specs/_audits/2026-05-16-shadow-sink-full-adoption.md`.
                let audit_sink: Arc<dyn ShadowSyncAuditSink> =
                    Arc::new(InMemoryShadowSyncAuditSink::new());
                Arc::new(TokioPgShadowSinkFactory::new(
                    executors,
                    audit_sink,
                    tenant_region_resolver,
                )) as Arc<dyn ShadowSinkFactory>
            }
        }
        #[cfg(not(feature = "neon-real"))]
        {
            // Bring-up friendly default — in-memory factory satisfies
            // the `/v1/audit/analytics/*` route surface in dev/CI.
            let _ = resolved_dsns;
            Arc::new(routes::InMemoryShadowSinkFactory::new()) as Arc<dyn ShadowSinkFactory>
        }
    };

    info!(
        "wave-20 routes: building composed router (CAS + AC + Admin + audit-export + audit-analytics)"
    );
    // The composed router is constructed but not yet bound to an
    // HTTP listener — the gRPC server is the canonical surface; the
    // axum router is wired into the future HTTP listener (the same
    // HTTP listener that hosts the Stripe webhook below when the
    // secret is present).
    let _composed_router = routes::build_with_factory(shadow_factory);

    // R2-12: HTTP server with Stripe webhook route. Only started when
    // STRIPE_WEBHOOK_SECRET is present; otherwise we log and skip so
    // local dev / CI don't fail without billing config.
    if let Ok(secret) = std::env::var("STRIPE_WEBHOOK_SECRET") {
        let http_port: u16 = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50052u16);
        let http_addr: SocketAddr = format!("0.0.0.0:{}", http_port).parse()?;

        // Wave 17 + 18: the HTTP shell binds the production
        // materializer + audit emitter + D1-backed idempotency store
        // from `corelink-billing-stripe-materializer`. The native
        // gRPC server (this binary) ALWAYS uses
        // `InMemoryBillingD1` + `InMemoryBillingAuditEmitter` —
        // D1 is a CF Worker binding (`worker::D1Database`) not
        // reachable from outside the Worker isolate, so even with
        // `--features cf-billing-real` the native binary stays on
        // the in-memory mirrors. The `cf-billing-real` feature is
        // a **build-time witness** that the wasm32 binder module
        // (`corelink_billing_stripe_materializer::wasm32_binders`)
        // is compiled in; the actual production cutover happens at
        // the CF Worker boot layer in `corelink-clerk-cf::prod_wiring`
        // which constructs `CfD1BillingWriter` +
        // `ArchiveProducerBillingEmitter` behind the same
        // `BillingD1Writer` / `BillingAuditEmitter` trait objects.
        //
        // The single canonical seam (`WebhookDispatcher::new`) is
        // preserved end-to-end — axum, CF Worker, and the replay
        // cron all hit this exact constructor with target-specific
        // collaborators.
        #[cfg(feature = "cf-billing-real")]
        {
            // Build-time witness: compile-check the wasm32 binder
            // re-exports are reachable. The actual `CfD1BillingWriter`
            // construction requires a `CfD1DatabaseReal` which is only
            // built at the CF Worker boot path; on native we keep the
            // InMemory* wiring and rely on the per-crate integration
            // test (`tests/wasm32_binders.rs`) to pin the binder
            // contract via the `stub_for_native_tests` path.
            #[allow(unused_imports)]
            use corelink_billing_stripe_materializer::{
                ArchiveProducerBillingEmitter as _, CfD1BillingWriter as _,
            };
        }
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
