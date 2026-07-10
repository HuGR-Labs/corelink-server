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
use corelink_billing::stripe::real::dlq::InMemoryWebhookDlqStore;
use corelink_billing::stripe::real::webhook_dispatch::{
    RecordingSliRecorder, StateMaterializer, SystemClock, WebhookDispatcher,
};
use corelink_billing_stripe_materializer::{
    BillingD1Writer, D1IdempotencyStore, D1SubscriptionStateHandler, InMemoryBillingAuditEmitter,
    InMemoryBillingD1, InMemoryRunnersEntitlementResolver, InMemoryTierSelector,
    RealStripeAuditEmitter, RunnersEntitlement,
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

/// Decide whether a missing native PAT gate is a FATAL boot condition
/// (red-team finding #7, HIGH).
///
/// The native data planes (CAS / AC / Bazel / Turbo) mount with the
/// container-side Argon2id possession backstop ONLY when
/// `adapter_pat::PatVerifier::from_env()` (and thus
/// `native_pat_gate_from_env()`) builds. That builder returns `None` not only
/// in dev/CI (benign) but ALSO in prod when a `PAT_SIGNING_KEY` rotation
/// sibling (`PAT_SIGNING_KEY_PREV` / `_NEW`) is PRESENT-but-malformed — a
/// one-character typo at deploy. In that case the planes would silently mount
/// WITHOUT the backstop fleet-wide: a silent security downgrade.
///
/// This pure predicate isolates the policy so it is unit-testable (the caller
/// does the `std::process::exit(1)`): in prod (`prod == true`) a missing gate
/// (`gate_present == false`) is fatal; otherwise (dev/CI, or the gate is
/// present) it is not.
#[must_use]
const fn should_fatal_on_missing_gate(prod: bool, gate_present: bool) -> bool {
    prod && !gate_present
}

/// Decide whether a missing/empty `EMAIL_HASH_SALT` is a FATAL boot condition
/// (CAA-360 MEDIUM — email-hash salt fail-fast).
///
/// The CTRL-PRIV-001 `email_hash::hash_email` helper HMAC-SHA256s the normalized
/// email under `EMAIL_HASH_SALT` when it is set+non-empty, but silently falls
/// back to a rainbow-table-reversible plain `SHA-256(email)` when it is
/// unset/empty. The salt is now SET on every prod target, so a future deploy
/// that DROPPED it must NOT be allowed to silently regress to the unsalted
/// scheme. This pure predicate isolates the policy so it is unit-testable (the
/// caller does the `std::process::exit(1)`): in prod (`prod == true`) a missing
/// salt (`salt_present == false`) is fatal; outside prod (dev/CI) or when the
/// salt is present it is not — so non-prod behavior is unchanged.
#[must_use]
const fn email_hash_salt_missing_in_prod(prod: bool, salt_present: bool) -> bool {
    prod && !salt_present
}

/// The REVENUE path's must-arm control. Each of the four cache-tier
/// `STRIPE_PRICE_ID_*` env vars ([`TIER_PRICE_ENV_TABLE`]) must resolve to a real
/// Stripe price id in prod. When one is unset/empty, [`build_tier_selector_from`]
/// silently registers the literal `plan_{tier}` placeholder (F-001 back-compat)
/// — which a real Stripe `price_live_…` event can NEVER match, so the container
/// webhook materializer resolves `UnknownPlan` → 422, Stripe stops retrying, and
/// a PAYING customer is stranded on the free serving path with NO alarm. Solo
/// ($30/mo) is the primary self-serve SMB tier, so this is launch-critical.
/// Returns the env-var names still unset/empty in prod (empty ⇒ armed). Pure and
/// injectable so the policy is unit-tested without the process environment
/// (mirrors [`email_hash_salt_missing_in_prod`]). Keep the container's map
/// identical to the signup-worker's reverse map (`apps/signup-worker/src/webhooks/
/// stripe.ts`) — divergence re-opens the same `UnknownPlan` → 422 seam.
fn cache_tier_price_ids_missing_in_prod<F>(prod: bool, lookup: F) -> Vec<&'static str>
where
    F: Fn(&str) -> Option<String>,
{
    if !prod {
        return Vec::new();
    }
    TIER_PRICE_ENV_TABLE
        .iter()
        .filter(|(env_name, _fallback, _tier)| {
            // MSRV 1.80 — `Option::is_none_or` is 1.82; `map_or(true, …)` is the
            // MSRV-safe equivalent (unset OR whitespace-only ⇒ "missing").
            lookup(env_name).map_or(true, |v| v.trim().is_empty())
        })
        .map(|(env_name, _, _)| *env_name)
        .collect()
}

/// Canonical `(env-var-name, literal-fallback-key, tier)` table for the
/// container Stripe-webhook tier reconciliation (F-001).
///
/// Each tuple says: the live Stripe price id is read from the env var
/// `0`; if that env var is unset/empty we fall back to the literal
/// placeholder key `1` (back-compat with pre-price-id deployments and
/// the historical test fixtures). Both keys map to tier `2`.
///
/// Only the four Stripe-checkout tiers (`Solo/Starter/Pro/Max`) have a
/// [`TierKind`] variant; `team` is a legacy operator-assigned SKU and
/// `enterprise` is a contact-sales route (neither flows through this
/// container path), so they are intentionally absent.
const TIER_PRICE_ENV_TABLE: &[(&str, &str, TierKind)] = &[
    ("STRIPE_PRICE_ID_SOLO", "plan_solo", TierKind::Solo),
    ("STRIPE_PRICE_ID_STARTER", "plan_starter", TierKind::Starter),
    ("STRIPE_PRICE_ID_PRO", "plan_pro", TierKind::Pro),
    ("STRIPE_PRICE_ID_MAX", "plan_max", TierKind::Max),
];

/// Runners-tier price → `(max_concurrency, max_vcpu_h)` mapping (a SEPARATE
/// axis from the cache tiers above). Owner-ratified loss-proof ladder
/// (`corelink-runners docs/product/pricing.md §2`, 2026-06-16) on the real
/// ~$0.10/vCPU-h Cloudflare-Containers basis: Starter 20/100 · Pro 40/240 ·
/// Team 80/600 · Scale 160/1200 · Max 320/2400. Keyed on the live
/// `STRIPE_PRICE_ID_RUNNER_*` price id; unset env ⇒ that tier is dormant (no
/// literal fallback — a Runners price must be a real Stripe id, never guessed).
const RUNNER_PRICE_ENV_TABLE: &[(&str, u32, u32)] = &[
    ("STRIPE_PRICE_ID_RUNNER_STARTER", 20, 100),
    ("STRIPE_PRICE_ID_RUNNER_PRO", 40, 240),
    ("STRIPE_PRICE_ID_RUNNER_TEAM", 80, 600),
    ("STRIPE_PRICE_ID_RUNNER_SCALE", 160, 1200),
    ("STRIPE_PRICE_ID_RUNNER_MAX", 320, 2400),
];

/// Build the container tier mapping from the live `STRIPE_PRICE_ID_*`
/// env values, falling back to the literal `plan_{tier}` keys when an
/// env var is unset/empty (F-001 fix).
fn build_tier_selector() -> InMemoryTierSelector {
    build_tier_selector_from(|name| std::env::var(name).ok())
}

/// Build the Runners entitlement resolver from the live `STRIPE_PRICE_ID_RUNNER_*`
/// env, or `None` when NONE are set (the Runners seed path stays dormant →
/// every subscription is a cache-tier event, exactly as before the price IDs
/// exist). Mirrors [`build_tier_selector`] but with NO literal fallback (a
/// Runners price must be a real Stripe id) and returns `Option` so the caller
/// only wires the resolver when at least one Runners price is configured.
fn build_runners_resolver() -> Option<InMemoryRunnersEntitlementResolver> {
    build_runners_resolver_from(|name| std::env::var(name).ok())
}

fn build_runners_resolver_from<F>(lookup: F) -> Option<InMemoryRunnersEntitlementResolver>
where
    F: Fn(&str) -> Option<String>,
{
    let mut resolver = InMemoryRunnersEntitlementResolver::new();
    for (env_name, max_concurrency, max_vcpu_h) in RUNNER_PRICE_ENV_TABLE {
        if let Some(price_id) = lookup(env_name) {
            if !price_id.trim().is_empty() {
                resolver = resolver.with_price(
                    price_id.trim(),
                    RunnersEntitlement {
                        max_concurrency: *max_concurrency,
                        max_vcpu_h: *max_vcpu_h,
                    },
                );
            }
        }
    }
    if resolver.is_empty() {
        None
    } else {
        Some(resolver)
    }
}

/// Pure mapping builder: `lookup` resolves an env-var name to its value
/// (injected so the policy is unit-testable without touching the
/// process environment).
///
/// For each row: if `lookup(env_name)` yields a non-empty value, map
/// that real price id → tier; otherwise map the literal `plan_{tier}`
/// placeholder → tier so test fixtures and pre-price-id deployments
/// still classify. When the env var IS set, the real price id is the
/// authoritative key and the literal placeholder is NOT registered
/// (the real Stripe event never carries it).
fn build_tier_selector_from<F>(lookup: F) -> InMemoryTierSelector
where
    F: Fn(&str) -> Option<String>,
{
    let selector = InMemoryTierSelector::new();
    for (env_name, literal_fallback, tier) in TIER_PRICE_ENV_TABLE {
        match lookup(env_name) {
            Some(price_id) if !price_id.trim().is_empty() => {
                selector.register(price_id.trim(), *tier);
            }
            _ => {
                selector.register(literal_fallback, *tier);
            }
        }
    }
    selector
}

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

    // ── Native PAT-gate fail-CLOSED boot guard (red-team finding #7, HIGH) ──
    // The native planes (CAS/AC/Bazel/Turbo) only carry their container-side
    // Argon2id possession backstop when `adapter_pat::PatVerifier::from_env()`
    // builds. That builder fails-CLOSED to `None` on a PRESENT-but-malformed
    // `PAT_SIGNING_KEY` rotation sibling — and in PROD that `None` would
    // silently mount the planes WITHOUT the backstop fleet-wide. So in prod a
    // missing gate is FATAL: we refuse to boot rather than serve degraded.
    //
    // Prod is detected by the same two signals that PROVE prod: the D1 /
    // `StorageEnv` config is present (durable backing, not the InMemory
    // dev/CI fallback) AND `PAT_SIGNING_KEY` is set. Re-calling `from_env()`
    // here is fine — both are idempotent, side-effect-free env reads.
    {
        let storage_present = corelink_server::storage::StorageEnv::from_env().is_some();
        let signing_key_present = std::env::var("PAT_SIGNING_KEY")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let prod = storage_present && signing_key_present;
        let gate_present =
            corelink_server::adapter_pat::PatVerifier::from_env().is_some();
        if should_fatal_on_missing_gate(prod, gate_present) {
            tracing::error!(
                event = "native_pat_gate_missing_in_prod",
                severity = "FATAL",
                "PROD detected (D1 + PAT_SIGNING_KEY present) but the native PAT \
                 verifier did NOT build — refusing to boot the data plane WITHOUT \
                 its Argon2id possession backstop. The most likely cause is a \
                 PRESENT-but-malformed PAT_SIGNING_KEY rotation sibling \
                 (PAT_SIGNING_KEY_PREV / PAT_SIGNING_KEY_NEW): bad hex, < 32 bytes, \
                 or a stray newline. Fix the sibling secret (or unset it) and \
                 redeploy."
            );
            std::process::exit(1);
        }
    }

    // ── Positive prod-arming assertion (config-drift finding #3, MEDIUM) ──
    // The native-PAT FATAL above keys on the SAME `StorageEnv` signal that ALSO
    // gates the $-ceiling (`tenant_quota::quota_guard_from_env`), byte-cap
    // (`byte_accounting::byte_accountant_from_env`) and request-count
    // (`request_count::RequestCountGate::from_env`) controls. NOTE the scope
    // asymmetry: the $-ceiling, byte-cap and native-PAT backstop are wired
    // FLEET-WIDE into every native container state (CAS/AC/Bazel/Turbo); the
    // request-count gate is wired ONLY into the OCI router (it is the OCI op-cap
    // mirror — native is op-count-bounded at the Worker edge instead, see the
    // `missing.push("request-count gate …")` note below). All four are still
    // co-gated by `StorageEnv`, so the drift hazard is identical: a single
    // dropped
    // or renamed `R2_S3_*` / `CLOUDFLARE_ACCOUNT_ID` / `CF_API_TOKEN` /
    // `D1_DATABASE_ID` var flips `StorageEnv` to `None`, which SILENTLY disarms
    // every one of those guards AND simultaneously makes prod-detection FALSE —
    // so the watchdog that should scream goes quiet. The missing config that
    // disables the controls also disables the alarm (the circular dependency).
    //
    // Close the loop with an INDEPENDENT positive prod signal that does NOT
    // depend on `StorageEnv`: the multi-region R2 placement vars
    // (`R2_AC_REGION` / `R2_CAS_REGION` / `R2_AC_BUCKET`). Every prod env
    // (`[env.prod*.vars]` in wrangler.toml) sets them and the DO forwards them
    // into the container (`worker/src/durable_object.ts`); dev/CI sets NONE of
    // them, so this whole block is a no-op there (dev/CI behavior unchanged).
    // They are NOT part of `StorageEnv` and gate NONE of the controls, so they
    // cannot be co-dropped with the very thing they witness — and we read all
    // THREE so dropping any one still leaves the prod signal standing. If this
    // signal says "prod", then ALL of the launch controls MUST be armed; if any
    // is missing we refuse to boot a HALF-ARMED prod (loud FATAL naming it).
    {
        let prod_by_independent_signal = ["R2_AC_REGION", "R2_CAS_REGION", "R2_AC_BUCKET"]
            .iter()
            .any(|v| {
                std::env::var(v)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
            });
        if prod_by_independent_signal {
            let mut missing: Vec<&str> = Vec::new();
            if corelink_server::storage::StorageEnv::from_env().is_none() {
                missing.push(
                    "durable StorageEnv (R2_S3_ENDPOINT / R2_S3_ACCESS_KEY_ID / \
                     R2_S3_SECRET_ACCESS_KEY / CLOUDFLARE_ACCOUNT_ID / CF_API_TOKEN / \
                     D1_DATABASE_ID)",
                );
            }
            let signing_key_present = std::env::var("PAT_SIGNING_KEY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if !signing_key_present {
                missing.push("PAT_SIGNING_KEY");
            }
            if corelink_server::adapter_pat::PatVerifier::from_env().is_none() {
                missing.push("native PAT verifier (Argon2id possession backstop)");
            }
            if corelink_server::tenant_quota::quota_guard_from_env().is_none() {
                missing.push("$-ceiling quota guard (tenant_quota)");
            }
            if corelink_server::byte_accounting::byte_accountant_from_env().is_none() {
                missing.push("byte-cap accountant (tenant_storage_state)");
            }
            if corelink_server::request_count::RequestCountGate::from_env().is_none() {
                // SCOPE: this gate is the OCI-surface monthly op-cap (the
                // container-side mirror of the Worker's `checkRequestQuota`).
                // The Worker forwards `/v2/*` + `/token` RAW and returns BEFORE
                // its quota block, so OCI is the ONE surface the edge cannot
                // count — the container gate is its sole enforcer and so is a
                // genuine must-arm. The native CAS/AC/Bazel/Turbo + cargo/brew/
                // npm/pip surfaces are op-count-bounded at the WORKER edge
                // (`incrementMonthlyRequestCount`, exactly once per request);
                // they deliberately do NOT carry this gate at the container —
                // doing so would double-increment `monthly_request_counts` and
                // false-deny legitimate native traffic. So this assertion is
                // NOT a fleet-wide-native claim; it asserts exactly what is
                // wired (the OCI op-cap, routes.rs).
                missing.push("request-count gate (OCI monthly op cap)");
            }
            // ERASURE_SALT_KEY: the GDPR DSR account-delete / erasure path derives
            // a per-DSR salt via `HMAC-SHA256(ERASURE_SALT_KEY, dsr_id)` in
            // `routes/customer.rs::derive_salt_hex`, which fail-CLOSEDs to a 500 when
            // the key is absent/empty. Without this assertion a prod boot missing the
            // key looks "healthy" while EVERY erasure/account-delete call silently
            // 500s — the GDPR Art. 17 path is broken with no early alarm. Treat it as
            // a must-arm prod control (read identically to PAT_SIGNING_KEY).
            let erasure_salt_key_present = std::env::var("ERASURE_SALT_KEY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if !erasure_salt_key_present {
                missing.push("ERASURE_SALT_KEY (GDPR erasure/account-delete salt)");
            }
            // EMAIL_HASH_SALT: the CTRL-PRIV-001 email_hash pseudonym helper
            // (`email_hash::hash_email` — every team-invite WRITE / accept-time
            // MATCH / DSR Art.16 rectification routes through it) HMAC-SHA256s the
            // normalized email under this server-held salt when set+non-empty, but
            // silently falls back to plain `SHA-256(email)` — a RAINBOW-TABLE-
            // reversible pseudonym — when it is unset/empty. The salt is now SET on
            // all prod targets, so a future deploy that DROPPED it would silently
            // regress every new pseudonym to the unsalted scheme with NO alarm.
            // Treat it as a must-arm prod control (read identically to
            // ERASURE_SALT_KEY / PAT_SIGNING_KEY). NOTE the fail-fast lives HERE at
            // boot, NOT inside `hash_email()` per-call: a per-call error would break
            // non-prod tests and add hot-path cost — the boot gate just guarantees
            // the salt exists in prod while `hash_email`'s dual-path logic (salted
            // write + legacy-unsalted lookup candidate) stays intact.
            let email_hash_salt_present = std::env::var("EMAIL_HASH_SALT")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if email_hash_salt_missing_in_prod(prod_by_independent_signal, email_hash_salt_present) {
                missing.push("EMAIL_HASH_SALT (CTRL-PRIV-001 email_hash pseudonym salt)");
            }
            // STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}: the revenue path. The
            // container is a live Stripe activation writer; an unset cache-tier
            // price id silently maps a real `price_live_…` to the un-matchable
            // `plan_{tier}` placeholder → `UnknownPlan` → 422 on a paying customer
            // (see cache_tier_price_ids_missing_in_prod). Must-arm in prod, and
            // kept identical to the signup-worker's reverse map.
            for env_name in
                cache_tier_price_ids_missing_in_prod(prod_by_independent_signal, |n| {
                    std::env::var(n).ok()
                })
            {
                missing.push(env_name);
            }
            if !missing.is_empty() {
                tracing::error!(
                    event = "prod_controls_not_fully_armed",
                    severity = "FATAL",
                    missing_controls = ?missing,
                    "PROD detected via an INDEPENDENT signal (R2_AC_REGION / \
                     R2_CAS_REGION / R2_AC_BUCKET is set) but one or more launch \
                     controls did NOT arm — refusing to boot a HALF-ARMED prod. The \
                     most likely cause is a dropped or renamed config var (an \
                     R2_S3_* / CLOUDFLARE_ACCOUNT_ID / CF_API_TOKEN / D1_DATABASE_ID) \
                     that silently disabled the listed guard(s) WHILE leaving the \
                     independent prod signal set. Restore the missing config and \
                     redeploy."
                );
                std::process::exit(1);
            }
        }
    }

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

    // M1: `POST /internal/v1/auth/introspect` — corelink-runners fabric PAT
    // introspection. Gated by a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY` (NOT the
    // mint secret — tight blast radius). Mounted only when that secret (≥32
    // chars) + the PAT signing key + the D1 `StorageEnv` are ALL present
    // (fail-CLOSED: unmounted in dev/CI).
    if let Some(introspect_state) =
        corelink_server::routes::auth_introspect::build_state_from_env()
    {
        info!(
            "routes: /internal/v1/auth/introspect mounted (FABRIC_INTROSPECT_AUTH_KEY + PAT signing key + D1 present)"
        );
        app = app.merge(corelink_server::routes::auth_introspect::router(
            introspect_state,
        ));
    } else {
        warn!(
            "FABRIC_INTROSPECT_AUTH_KEY / PAT_SIGNING_KEY / D1 config incomplete; \
             /internal/v1/auth/introspect NOT mounted (dev/CI mode)"
        );
    }

    // ASK-2: `POST /internal/v1/billing/usage` — corelink-runners billing
    // usage-push INGEST. Gated by a DEDICATED `BILLING_INGEST_AUTH_KEY` (NOT the
    // mint / introspect / erase secrets — tight blast radius). Mounted only when
    // that secret (≥32 chars) + the D1 `StorageEnv` are present (fail-CLOSED:
    // unmounted in dev/CI). Idempotently stages raw per-lease usage records into
    // the canonical `usage_event_staging` table the aggregator drains; it does
    // NOT aggregate or touch Stripe.
    if let Some(billing_ingest_state) =
        corelink_server::routes::billing_ingest::build_state_from_env()
    {
        info!(
            "routes: /internal/v1/billing/usage mounted (BILLING_INGEST_AUTH_KEY + D1 present)"
        );
        app = app.merge(corelink_server::routes::billing_ingest::router(
            billing_ingest_state,
        ));
    } else {
        warn!(
            "BILLING_INGEST_AUTH_KEY / D1 config incomplete; \
             /internal/v1/billing/usage NOT mounted (dev/CI mode)"
        );
    }

    // WI-S11-008: `POST /_internal/dsr/erase` — gated by the
    // CORELINK_INTERNAL_AUTH_KEY. Drives the 12-backend erasure orchestrator
    // (Wave 1: real D1/R2/Stripe/KV/Loki transports wired in #254).
    if let Some(dsr_state) = corelink_server::routes::dsr::build_state_from_env() {
        info!(
            "routes: /_internal/dsr/{{erase,verify,access,portability,rectification}} mounted \
             (Art.17/15/20/16 data-subject rights; Wave 1 real adapters)"
        );
        app = app.merge(corelink_server::routes::dsr::router(dsr_state));
    } else {
        warn!("CORELINK_INTERNAL_AUTH_KEY unset; /_internal/dsr/* routes NOT mounted (dev/CI)");
    }

    // S-09 audit-chain drain: `POST /_internal/audit/drain` — seals the live
    // `audit_outbox` trail into the BLAKE3 tamper-evident hash chain (closes the
    // "audit trail is mutable / not tamper-evident" gap). Gated by the dedicated
    // ERASE key (falls back to CORELINK_INTERNAL_AUTH_KEY), same as the DSR
    // surface; mounts only when that key (≥32 chars) + the D1 `StorageEnv` are
    // present. Without D1 there is nothing to seal → unmounted (fail-CLOSED).
    if let Some(audit_drain_state) = corelink_server::routes::audit_drain::build_state_from_env() {
        info!("routes: /_internal/audit/drain route mounted (erase/internal auth key + D1 present)");
        app = app.merge(corelink_server::routes::audit_drain::router(
            audit_drain_state,
        ));
    } else {
        warn!(
            "CORELINK_ERASE_AUTH_KEY/CORELINK_INTERNAL_AUTH_KEY (<32) or D1 absent; \
             /_internal/audit/drain route NOT mounted (fail-CLOSED)"
        );
    }

    // Artifact 1 (WP-C1): PUBLIC erasure-attestation verifier routes —
    // `GET /v1/public/attestation/{request_id}` + `GET /v1/public/keys/erasure/{region}.pub`.
    // UNAUTHENTICATED by design (an erasure proof is publicly verifiable): merged
    // here, OUTSIDE the ratelimit/residency/auth layers (same as the `/_internal/*`
    // family above) but with NO internal-auth gate. D1-read only; mounts only when
    // the D1 `StorageEnv` is present (nothing to serve without the index).
    if let Some(public_att_state) =
        corelink_server::routes::public_attestation::build_state_from_env()
    {
        info!("routes: /v1/public/{{attestation,keys}} mounted (public verifier; D1 present)");
        app = app.merge(corelink_server::routes::public_attestation::router(
            public_att_state,
        ));
    } else {
        warn!(
            "D1 StorageEnv absent; /v1/public/attestation + /v1/public/keys/erasure \
             NOT mounted (dev/CI mode)"
        );
    }

    // hugit-P2 seam B, WP-B: `POST /_internal/cas/:tenant/:hash/erase` — per-hash
    // CAS erase + 410-Gone tombstone, gated by the same CORELINK_INTERNAL_AUTH_KEY.
    // The WRITE route mounts only when ALL prod transports build from env: the
    // internal-auth key, the R2 TDK (`R2_TDK_HEX`), and the D1 tombstone store.
    // Without the TDK the eraser cannot derive the writer's tenant prefix, so the
    // route stays UNMOUNTED (fail-CLOSED) — it can never tombstone a blob whose
    // bytes it could not address (mirrors the DSR R2 CAS adapter's fail-closed
    // posture). The read-side 410 gate is wired separately in `routes::cas`.
    // rt-nuclear #20/#21: CAS-erase MUST gate on the dedicated ERASE key
    // (CORELINK_ERASE_AUTH_KEY), not the ADMIN key — so an admin-key leak cannot
    // drive irreversible erases. `erase_auth_key_from_env` still falls back to the
    // shared CORELINK_INTERNAL_AUTH_KEY when the dedicated key is unset, so this is
    // non-breaking until the per-consumer secret (#158) is provisioned.
    let cas_erase_auth_key = corelink_server::routes::admin::erase_auth_key_from_env();
    if let Some(cas_erase_state) =
        corelink_server::routes::cas_erase::build_state_from_env(cas_erase_auth_key)
    {
        info!("routes: /_internal/cas/:tenant/:hash/erase route mounted (R2 TDK + D1 tombstone present)");
        app = app.merge(corelink_server::routes::cas_erase::router(cas_erase_state));
    } else {
        warn!(
            "CORELINK_INTERNAL_AUTH_KEY / R2_TDK_HEX / D1 incomplete; \
             /_internal/cas/:tenant/:hash/erase route NOT mounted (fail-CLOSED)"
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

    // Launch money-path unblock: `POST /v1/onboarding/dpa-accept` — writes the
    // durable `dpa_acceptances` row the tier-select gate (`is_dpa_accepted`)
    // reads (INV-ONBOARD-DPA-FIRST). Same onboarding proxy contract as
    // tier-select (worker sets x-corelink-internal-auth + x-corelink-tenant-id).
    // Mounted only when the internal-auth secret + D1 + DPA version + the RS256
    // receipt signing key (DPA_RECEIPT_SIGNING_KEY) are ALL present — fail-CLOSED
    // (unmounted, logged) rather than 500 when the key is unset/invalid.
    if let Some(dpa_accept_state) = corelink_server::routes::dpa_accept::build_state_from_env() {
        info!(
            "routes: /v1/onboarding/dpa-accept mounted (internal auth + D1 + DPA version + DPA_RECEIPT_SIGNING_KEY present)"
        );
        app = app.merge(corelink_server::routes::dpa_accept::router(
            dpa_accept_state,
        ));
    } else {
        warn!(
            "dpa-accept config incomplete (CORELINK_INTERNAL_AUTH_KEY / CORELINK_DPA_VERSION / \
             DPA_RECEIPT_SIGNING_KEY / D1); /v1/onboarding/dpa-accept NOT mounted (fail-CLOSED)"
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
        // Canonical Stripe-plan-id → tier mapping (F-001 fix).
        //
        // Real `customer.subscription.updated` events carry
        // `data.object.plan.id = price_…` (the live Stripe price id),
        // NOT the literal `plan_solo/…` placeholders. The Worker
        // forwards the real ids as `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}`
        // (`worker/src/durable_object.ts:554-558`), so the mapping MUST
        // be keyed off those env values — otherwise every real event
        // resolves to `UnknownPlan` → 422 and Stripe stops retrying.
        //
        // This mirrors the signup-worker resolver
        // (`apps/signup-worker/src/webhooks/stripe.ts:287-296`) which
        // keys the same map off the same env vars. The literal
        // `plan_{tier}` keys are retained as a backward-compatible
        // fallback (test fixtures / pre-price-id deployments) ONLY when
        // the corresponding env var is unset/empty.
        let tier_selector = Arc::new(build_tier_selector());
        let mut sub_handler =
            D1SubscriptionStateHandler::new(billing_d1.clone(), billing_audit.clone(), tier_selector);
        // Runners entitlement seed (env-gated): when STRIPE_PRICE_ID_RUNNER_* are
        // set, a Runners-tier subscription seeds `runners_entitlement` instead of
        // `tier_selections`. Dormant (no-op) until those prices exist.
        if let Some(runners_resolver) = build_runners_resolver() {
            tracing::info!(
                "Runners entitlement seed ENABLED ({} tier price(s) wired)",
                runners_resolver.len()
            );
            sub_handler = sub_handler.with_runners_resolver(Arc::new(runners_resolver));
        } else {
            tracing::info!(
                "Runners entitlement seed dormant (no STRIPE_PRICE_ID_RUNNER_* set)"
            );
        }
        let materializer: Arc<dyn StateMaterializer> = Arc::new(sub_handler);
        let dispatcher_audit = Arc::new(RealStripeAuditEmitter::new(billing_audit.clone()));
        let idempotency = Arc::new(D1IdempotencyStore::new(billing_d1.clone()));
        // F-008 closure: wire a DLQ sink so a TRANSIENT materialize
        // failure quarantines the (already HMAC-verified) event instead
        // of silently dropping it. The idempotency dedup row is committed
        // BEFORE materialize, so a Stripe retry hits `AlreadyProcessed`
        // and skips the handler — without the DLQ the state change is
        // lost while Stripe records success. The in-memory store captures
        // the event for the container's lifetime + exposes it via the DLQ
        // depth/age metrics; the durable D1-backed `WebhookDlqStore`
        // (`migrations/d1/0045_stripe_webhook_dlq.sql`) is the operator
        // follow-up so quarantines survive container restarts.
        let webhook_dlq = Arc::new(InMemoryWebhookDlqStore::new());
        let dispatcher = Arc::new(
            WebhookDispatcher::new(
                secret.into_bytes(),
                idempotency,
                materializer,
                dispatcher_audit,
                Arc::new(RecordingSliRecorder::new()),
                Arc::new(SystemClock),
            )
            .with_dlq(webhook_dlq),
        );
        let state = Arc::new(WebhookState::new(dispatcher));
        info!(
            route = corelink_server::webhook::STRIPE_WEBHOOK_ROUTE,
            "Stripe webhook route mounted on the data-plane listener"
        );
        app = app.merge(webhook_router(state));
    } else {
        warn!("STRIPE_WEBHOOK_SECRET unset; Stripe webhook route NOT mounted (dev/CI mode)");
    }

    // GDPR1 per-user erasure: `POST /_internal/dsr/anchor` — register the
    // `dsr_requested` legitimacy anchor for a per-user (not whole-account)
    // erasure, so the per-digest CAS erase can authorize it. Gated by the
    // dedicated CORELINK_DSR_ANCHOR_AUTH_KEY (shared-key fallback until
    // provisioned) — held by the erasure-request authority (githugr), a DIFFERENT
    // party than the eraser (hugit), or the legitimacy gate is moot. (Mounted last,
    // after the cited-in-OKF blocks above, to keep the anti-drift line-anchors stable.)
    let dsr_anchor_auth_key = corelink_server::routes::admin::dsr_anchor_auth_key_from_env();
    if let Some(dsr_anchor_state) =
        corelink_server::routes::dsr_anchor::build_state_from_env(dsr_anchor_auth_key)
    {
        info!("routes: /_internal/dsr/anchor route mounted (auth key + D1 present)");
        app = app.merge(corelink_server::routes::dsr_anchor::router(dsr_anchor_state));
    } else {
        warn!(
            "CORELINK_DSR_ANCHOR_AUTH_KEY / D1 incomplete; \
             /_internal/dsr/anchor route NOT mounted (fail-CLOSED)"
        );
    }

    // Single HTTP/1.1 listener on PORT (50051) — the DO's getTcpPort target.
    let listener = tokio::net::TcpListener::bind(serve_addr).await?;
    info!(%serve_addr, "CoreLink HTTP data-plane server starting");
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        build_runners_resolver_from, build_tier_selector_from, cache_tier_price_ids_missing_in_prod,
        email_hash_salt_missing_in_prod, should_fatal_on_missing_gate,
    };
    use corelink_billing_stripe_materializer::{
        RunnersEntitlement, RunnersEntitlementResolver, TierSelectError, TierSelector,
    };
    use corelink_tier_selection::tier::TierKind;
    use std::collections::HashMap;

    /// Finding #7 truth table: the boot guard fails fatal ONLY when prod is
    /// detected AND the native PAT gate did not build. Dev/CI (not prod) is
    /// never fatal regardless of the gate; a present gate in prod is fine.
    #[test]
    fn fatal_only_when_prod_and_gate_missing() {
        // prod + gate missing → FATAL (the silent-downgrade case finding #7
        // closes).
        assert!(should_fatal_on_missing_gate(true, false));
        // prod + gate present → OK (the backstop is wired).
        assert!(!should_fatal_on_missing_gate(true, true));
        // dev/CI + gate missing → OK (benign; this is the normal dev posture).
        assert!(!should_fatal_on_missing_gate(false, false));
        // dev/CI + gate present → OK.
        assert!(!should_fatal_on_missing_gate(false, true));
    }

    /// Revenue-path truth table: in prod the boot guard names EXACTLY the
    /// cache-tier `STRIPE_PRICE_ID_*` env vars that are unset/empty (the case
    /// where a real `price_live_…` would fall back to the un-matchable
    /// `plan_{tier}` placeholder → `UnknownPlan` → 422 on a paying customer).
    /// Non-prod is never gated (dev/CI + fixture deployments keep the literal
    /// fallbacks). A fully-configured prod map arms clean (empty result).
    #[test]
    fn cache_tier_price_ids_missing_names_exactly_the_unset_tiers_in_prod() {
        fn map_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect()
        }

        // prod + all four set → armed (empty).
        let full = map_of(&[
            ("STRIPE_PRICE_ID_SOLO", "price_live_solo"),
            ("STRIPE_PRICE_ID_STARTER", "price_live_starter"),
            ("STRIPE_PRICE_ID_PRO", "price_live_pro"),
            ("STRIPE_PRICE_ID_MAX", "price_live_max"),
        ]);
        assert!(cache_tier_price_ids_missing_in_prod(true, |n| full.get(n).cloned()).is_empty());

        // prod + Solo unset → names EXACTLY Solo (the primary-tier-unsellable case).
        let no_solo = map_of(&[
            ("STRIPE_PRICE_ID_STARTER", "price_live_starter"),
            ("STRIPE_PRICE_ID_PRO", "price_live_pro"),
            ("STRIPE_PRICE_ID_MAX", "price_live_max"),
        ]);
        assert_eq!(
            cache_tier_price_ids_missing_in_prod(true, |n| no_solo.get(n).cloned()),
            vec!["STRIPE_PRICE_ID_SOLO"]
        );

        // prod + a whitespace-only value counts as unset (trim → empty).
        let blank_pro = map_of(&[
            ("STRIPE_PRICE_ID_SOLO", "price_live_solo"),
            ("STRIPE_PRICE_ID_STARTER", "price_live_starter"),
            ("STRIPE_PRICE_ID_PRO", "   "),
            ("STRIPE_PRICE_ID_MAX", "price_live_max"),
        ]);
        assert_eq!(
            cache_tier_price_ids_missing_in_prod(true, |n| blank_pro.get(n).cloned()),
            vec!["STRIPE_PRICE_ID_PRO"]
        );

        // NON-prod is never gated, even with every price id unset.
        assert!(cache_tier_price_ids_missing_in_prod(false, |_| None).is_empty());
    }

    /// CAA-360 MEDIUM truth table: the boot guard refuses to boot ONLY when prod
    /// is detected AND `EMAIL_HASH_SALT` is unset/empty — the exact case where
    /// `email_hash::hash_email` would silently regress to the rainbow-table-
    /// reversible unsalted `SHA-256`. Prod + salt present is fine (salted path);
    /// non-prod is never fatal regardless of the salt (dev/CI + tests unchanged).
    #[test]
    fn email_hash_salt_fatal_only_when_prod_and_salt_missing() {
        // prod + salt unset/empty → FATAL (the silent-unsalted-regression case).
        assert!(email_hash_salt_missing_in_prod(true, false));
        // prod + salt present → OK (the salted HMAC path is guaranteed).
        assert!(!email_hash_salt_missing_in_prod(true, true));
        // non-prod + salt unset → OK (no regression: dev/CI + tests run unsalted).
        assert!(!email_hash_salt_missing_in_prod(false, false));
        // non-prod + salt present → OK.
        assert!(!email_hash_salt_missing_in_prod(false, true));
    }

    /// F-001 regression: when the live `STRIPE_PRICE_ID_*` env values are
    /// present, the tier selector MUST classify the real `price_…` ids
    /// (the keys a real `customer.subscription.updated` carries) — not
    /// the literal `plan_*` placeholders. Before the fix the container
    /// map only held `plan_*`, so every real event 422'd (`UnknownPlan`).
    #[test]
    fn tier_selector_maps_real_price_ids_when_env_set() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("STRIPE_PRICE_ID_SOLO", "price_live_solo_abc"),
            ("STRIPE_PRICE_ID_STARTER", "price_live_starter_def"),
            ("STRIPE_PRICE_ID_PRO", "price_live_pro_ghi"),
            ("STRIPE_PRICE_ID_MAX", "price_live_max_jkl"),
        ]);
        let sel = build_tier_selector_from(|name| env.get(name).map(|s| (*s).to_string()));

        // Real price ids resolve.
        assert_eq!(
            sel.compute_tier("price_live_solo_abc", 1).unwrap(),
            TierKind::Solo
        );
        assert_eq!(
            sel.compute_tier("price_live_starter_def", 3).unwrap(),
            TierKind::Starter
        );
        assert_eq!(
            sel.compute_tier("price_live_pro_ghi", 5).unwrap(),
            TierKind::Pro
        );
        assert_eq!(
            sel.compute_tier("price_live_max_jkl", 1).unwrap(),
            TierKind::Max
        );

        // The literal placeholder is NOT registered once the real id wins
        // (a real Stripe event never carries `plan_solo`).
        assert!(matches!(
            sel.compute_tier("plan_solo", 1),
            Err(TierSelectError::UnknownPlan(_))
        ));
    }

    /// F-001: an empty/whitespace env value falls back to the literal
    /// `plan_{tier}` key so test fixtures + pre-price-id deployments
    /// still classify (back-compat, no regression for the old wiring).
    #[test]
    fn tier_selector_falls_back_to_literal_when_env_unset_or_blank() {
        let env: HashMap<&str, &str> = HashMap::from([
            // SOLO unset entirely; STARTER blank; PRO whitespace-only.
            ("STRIPE_PRICE_ID_MAX", "price_live_max_only"),
            ("STRIPE_PRICE_ID_STARTER", ""),
            ("STRIPE_PRICE_ID_PRO", "   "),
        ]);
        let sel = build_tier_selector_from(|name| env.get(name).map(|s| (*s).to_string()));

        assert_eq!(sel.compute_tier("plan_solo", 1).unwrap(), TierKind::Solo);
        assert_eq!(
            sel.compute_tier("plan_starter", 1).unwrap(),
            TierKind::Starter
        );
        assert_eq!(sel.compute_tier("plan_pro", 1).unwrap(), TierKind::Pro);
        // MAX had a real id → real id wins.
        assert_eq!(
            sel.compute_tier("price_live_max_only", 1).unwrap(),
            TierKind::Max
        );
    }

    #[test]
    fn runners_resolver_dormant_when_no_price_ids_set() {
        // No STRIPE_PRICE_ID_RUNNER_* env → None (Runners seed stays dormant,
        // every subscription routes to the cache tier path).
        let env: HashMap<&str, &str> = HashMap::new();
        let r = build_runners_resolver_from(|name| env.get(name).map(|s| (*s).to_string()));
        assert!(r.is_none());
    }

    #[test]
    fn runners_resolver_maps_set_prices_to_the_ratified_ladder() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("STRIPE_PRICE_ID_RUNNER_STARTER", "price_live_run_starter"),
            ("STRIPE_PRICE_ID_RUNNER_TEAM", "price_live_run_team"),
            ("STRIPE_PRICE_ID_RUNNER_MAX", "price_live_run_max"),
            ("STRIPE_PRICE_ID_RUNNER_PRO", ""), // unset/blank → not wired
        ]);
        let r = build_runners_resolver_from(|name| env.get(name).map(|s| (*s).to_string()))
            .expect("at least one runner price set → Some");
        // Ratified ladder: Starter 20/100, Team 80/600, Max 320/2400.
        assert_eq!(
            r.resolve("price_live_run_starter"),
            Some(RunnersEntitlement {
                max_concurrency: 20,
                max_vcpu_h: 100
            })
        );
        assert_eq!(
            r.resolve("price_live_run_team"),
            Some(RunnersEntitlement {
                max_concurrency: 80,
                max_vcpu_h: 600
            })
        );
        assert_eq!(
            r.resolve("price_live_run_max"),
            Some(RunnersEntitlement {
                max_concurrency: 320,
                max_vcpu_h: 2400
            })
        );
        // Blank PRO was not wired; a cache price is not a runner price.
        assert_eq!(r.resolve("price_live_run_pro"), None);
        assert_eq!(r.resolve("plan_pro"), None);
    }
}
