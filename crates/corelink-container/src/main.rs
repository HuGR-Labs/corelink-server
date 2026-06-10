//! CoreLink server binary entry point.
//!
//! Hosts a single HTTP/1.1 stack on `PORT` (default 50051) — the port the
//! Cloudflare Durable Object talks to via `container.getTcpPort()` (an HTTP
//! fetcher). It serves:
//! - the composed data-plane router (CAS / AC / Admin / audit-export /
//!   audit-analytics / signup),
//! - `GET /_health` for the DO's container-readiness probe,
//! - and the R2-12 Stripe webhook route at `/v1/billing/stripe-webhook`
//!   (merged in only when `STRIPE_WEBHOOK_SECRET` is set).
//!
//! Historical note: this binary previously bound a tonic gRPC server on
//! 50051 that served only the Health service, while the composed axum router
//! was built and DISCARDED (`_composed_router`) — so the product data plane
//! was never reachable. The DO only ever speaks HTTP to this port, so the
//! gRPC server was dead weight blocking the port; it has been removed in
//! favour of serving the real HTTP data plane here.
#![forbid(unsafe_code)]
#![allow(missing_docs)]
#![allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    reason = "scaffolding lints — main.rs is the binary entry"
)]

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use corelink_billing::stripe::real::webhook_dispatch::{
    RecordingSliRecorder, StateMaterializer, SystemClock, WebhookDispatcher,
};
use corelink_billing_stripe_materializer::{
    BillingD1Writer, D1IdempotencyStore, D1SubscriptionStateHandler, InMemoryBillingAuditEmitter,
    InMemoryBillingD1, InMemoryTierSelector, RealStripeAuditEmitter,
};
use corelink_server::billing_d1_http::D1HttpBillingWriter;
use corelink_server::routes;
use corelink_server::routes::audit_analytics::ShadowSinkFactory;
use corelink_server::webhook::{router as webhook_router, WebhookState};
use corelink_tier_selection::tier::TierKind;
use tracing::{info, warn};

/// Storage backing kind captured once at boot by `main()`.
///
/// `"r2"` when `R2_S3_*` env vars are all non-empty (durable store).
/// `"inmemory"` otherwise (ephemeral fallback — operator action required).
///
/// The `OnceLock` is set exactly once during `main()`, before the listener
/// binds, so every subsequent call to `health_handler` sees a fully
/// initialised value.  On the (impossible in production) path where the
/// lock is read before it is set, we fall back to the literal `"unknown"`
/// so the health endpoint remains available.
static STORAGE_BACKING: OnceLock<&'static str> = OnceLock::new();

/// Liveness probe for two callers:
/// (1) the DO's `waitForContainerHealth` — only checks status === 200;
/// (2) `scripts/smoke-prod-corelink.sh` check [2] — asserts 200 *and*
///     `content-type: application/json`.
///
/// Body: `{"status":"ok","storage":"r2"|"inmemory"}` — the `storage` field
/// lets operators detect the InMemory silent fallback without tailing logs.
/// The `status` and `content-type` fields are preserved for backward compat.
async fn health_handler() -> impl IntoResponse {
    let backing = STORAGE_BACKING.get().copied().unwrap_or("unknown");
    // Build the JSON inline — no serde dependency in main.rs.
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,corelink_server=debug".into()),
        )
        .init();

    // Determine storage backing once at boot so `/_health` can surface it.
    // This mirrors the decision gate in `routes/cas.rs` and `routes/ac.rs`
    // without touching those files (they are owned by agent A3).
    let storage_backing: &'static str =
        if corelink_server::storage::StorageEnv::from_env().is_some() {
            "r2"
        } else {
            warn!(
                storage = "inmemory",
                "CAS/AC falling back to InMemory store — set R2_S3_* env vars for durable storage"
            );
            "inmemory"
        };
    // Unwrap is safe: this is the only setter and it runs before the listener.
    let _ = STORAGE_BACKING.set(storage_backing);
    info!(storage = storage_backing, "storage backing selected");

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50051u16);

    let serve_addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;

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
                // `specs/_audits/sealed/2026-05-16-shadow-sink-full-adoption.md`.
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
        "routes: building composed data-plane router (CAS + AC + Admin + audit-export + audit-analytics + signup) + /_health"
    );
    // The composed data-plane router is the product surface. We bind it to the
    // HTTP listener on PORT (50051) — the exact port the DO forwards HTTP to and
    // probes for /_health. `/_health` is added here so the DO's container
    // readiness probe (GET /_health, expects 200) succeeds.
    // H5 DoS guard: cap the request body for EVERY route at 10 MiB so an
    // authenticated PAT cannot OOM the shared container with a multi-GB body on
    // any JSON/CAS route. `DefaultBodyLimit` is an outer layer; axum honours the
    // innermost limit, so the Turbo `/v8/artifacts/:hash` PUT (which legitimately
    // carries larger build artifacts) sets its own larger per-route limit inside
    // `turbo_v8::router()` and is NOT constrained by this global default.
    const GLOBAL_BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024; // 10 MiB
    let mut app = routes::build_with_factory(shadow_factory)
        .route("/_health", get(health_handler))
        .layer(axum::extract::DefaultBodyLimit::max(
            GLOBAL_BODY_LIMIT_BYTES,
        ));

    // Stream-5: `POST /_internal/pat/mint` — gated by shared secret.
    // Mounted when CORELINK_INTERNAL_AUTH_KEY + PAT_SIGNING_KEY are both set.
    if let Some(internal_pat_state) = corelink_server::routes::internal_pat::build_state_from_env()
    {
        info!("routes: /_internal/pat/mint route mounted (internal auth key + PAT signing key present)");
        app = app.merge(corelink_server::routes::internal_pat::router(
            internal_pat_state,
        ));
    } else {
        warn!(
            "CORELINK_INTERNAL_AUTH_KEY or PAT_SIGNING_KEY unset; \
             /_internal/pat/mint route NOT mounted (dev/CI mode)"
        );
    }

    // L3: `POST /v1/onboarding/tier-select` — self-serve Stripe Checkout.
    // Mounted only when the internal-auth secret + D1 + Stripe + DPA version
    // are ALL configured (fail-safe; same internal-auth gate as the PAT route).
    if let Some(tier_select_state) = corelink_server::routes::tier_select::build_state_from_env() {
        info!(
            "routes: /v1/onboarding/tier-select mounted (internal auth + D1 + Stripe + DPA version present)"
        );
        app = app.merge(corelink_server::routes::tier_select::router(
            tier_select_state,
        ));
    } else {
        warn!(
            "tier-select config incomplete (CORELINK_INTERNAL_AUTH_KEY / CORELINK_DPA_VERSION / \
             D1 / Stripe); /v1/onboarding/tier-select NOT mounted (dev/CI mode)"
        );
    }

    // R2-12: the Stripe webhook route is MERGED onto the same listener when
    // STRIPE_WEBHOOK_SECRET is present; absent → skip (dev/CI without billing
    // config stays green). Either way the data plane above is always served.
    if let Ok(secret) = std::env::var("STRIPE_WEBHOOK_SECRET") {
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
        // Item 7b (money-path launch-blocker): wire the DURABLE D1-HTTP
        // billing writer when the D1 config is present, so Stripe-webhook
        // state (customers / subscriptions / invoices / disputes / refunds
        // / tier / the idempotency dedup row) is materialized to D1 over
        // the CF REST API — NOT lost to the in-memory mirror on the next
        // container restart. The sync `BillingD1Writer` trait is bridged
        // to the async `D1HttpClient` inside `D1HttpBillingWriter`
        // (`block_in_place`; the trait stays sync for the shared wasm32
        // Worker path). Dev/CI without `R2_S3_*`/CF D1 config keep the
        // `InMemoryBillingD1` mirror so the suite stays green offline.
        let billing_d1: Arc<dyn BillingD1Writer> =
            match corelink_server::storage::StorageEnv::from_env() {
                Some(storage_env) => {
                    match corelink_server::storage::d1_http::D1HttpClient::new(&storage_env) {
                        Ok(client) => {
                            info!(
                                "billing: DURABLE D1-HTTP writer wired (CF D1 REST API); \
                             Stripe-webhook state persists across restarts"
                            );
                            Arc::new(D1HttpBillingWriter::new(Arc::new(client)))
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "billing: D1HttpClient init failed; \
                                 FALLING BACK to InMemoryBillingD1 (webhook state is NOT durable)"
                            );
                            Arc::new(InMemoryBillingD1::new())
                        }
                    }
                }
                None => {
                    warn!(
                        "billing: D1 config absent (R2_S3_*/CF D1); \
                         using InMemoryBillingD1 (dev/CI — webhook state is NOT durable)"
                    );
                    Arc::new(InMemoryBillingD1::new())
                }
            };
        let billing_audit = Arc::new(InMemoryBillingAuditEmitter::new());
        // Canonical Stripe-plan-id → tier mapping. Production
        // operators flip these via the workspace tier config; the
        // defaults here mirror the canonical 6-tier taxonomy from
        // `corelink-tier-selection::tier::TierKind` so a fresh
        // deployment without overrides still classifies the four
        // paid production plans correctly.
        let tier_selector = Arc::new(InMemoryTierSelector::with_mapping(&[
            ("plan_solo", TierKind::Solo),
            ("plan_starter", TierKind::Starter),
            ("plan_pro", TierKind::Pro),
            ("plan_max", TierKind::Max),
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
        info!(
            route = corelink_server::webhook::STRIPE_WEBHOOK_ROUTE,
            "Stripe webhook route mounted on the data-plane listener"
        );
        app = app.merge(webhook_router(state));
    } else {
        warn!("STRIPE_WEBHOOK_SECRET unset; Stripe webhook route NOT mounted (dev/CI mode)");
    }

    // Single HTTP/1.1 listener on PORT (50051) — the DO's getTcpPort target.
    let listener = tokio::net::TcpListener::bind(serve_addr).await?;
    info!(%serve_addr, "CoreLink HTTP data-plane server starting");
    axum::serve(listener, app).await?;

    Ok(())
}
