//! HTTP route surface for the CoreLink server.
//!
//! Wave-8 landed the CAS read end-to-end as the **example wire-up**
//! for the R-prep handler-crate skeleton (see
//! `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md §6`):
//! `apps/server::routes::cas` wires `corelink-handler-cas` + in-memory
//! fakes into an axum route, demonstrating where the
//! `#[cfg(target_arch = "wasm32")]` CF-Worker handler slots in.
//!
//! Wave-11 (this commit) extends the same pattern to the AC and
//! Admin handler crates:
//!
//! - [`ac`] — `GET/PUT /v1/ac/{tenant}/{action_digest}` against
//!   `corelink-handler-ac::{AcLookupHandler, AcUpdateHandler}`.
//!   Emits `Sli::AvailAcLookup` + `Sli::LatencyAcHitP99` on every
//!   route entry; enforces `INV-TENANT-ISOLATION` at the route
//!   boundary on top of handler-layer enforcement.
//! - [`admin`] — `GET /v1/admin/read/{resource}` +
//!   `POST /v1/admin/mutate` against
//!   `corelink-handler-admin::{AdminReadHandler, AdminMutateHandler}`.
//!   Emits `Sli::AvailControlPlane` on every route entry; the
//!   mutate route is gated by dual-approval (per
//!   `dual_approval.md §3`).
//!
//! # Router composition
//!
//! [`build`] returns a fully composed `axum::Router` merging all
//! three sub-routers (cas, ac, admin). Each sub-router owns its
//! own `State<...>` so the trait-object swap surface stays local
//! to each module.

use std::sync::Arc;

use axum::Router;
use corelink_analytics::Region;
use corelink_audit_chain::{InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink, NeonShadowSink};
use uuid::Uuid;

use crate::routes::audit_analytics::ShadowSinkFactory;

/// AC HTTP routes (R-prep wire-up; wave-11).
pub mod ac;
/// Admin HTTP routes (R-prep wire-up; wave-11).
pub mod admin;
/// Operator per-tenant deep-dive reads (usage/billing/consents/dsr/pats).
/// Operator posture: internal-auth gated, same as `admin` — reached via the
/// operator path, NOT the customer Worker (which strips internal-auth on
/// `/v1/*`). Wiring the admin-ui operator console to this surface is a separate
/// follow-up that applies to the whole operator plane, not just this module.
pub mod admin_tenant_detail;
/// Pilot-admin HTTP routes (Wave-29 stream-3): replaces the wave-27
/// placeholder scripts (`grant-pilot-tier.sh`, `list-pilot-tenants.sh`,
/// `pilot-24h-checkin.sh`) with proper endpoints + audit-emit
/// fail-CLOSED ordering + 5-Layer Defense scope gating.
pub mod admin_pilot;
/// Operator-gated BYOK **activation** control plane
/// (`POST /v1/admin/byok/{activate,deactivate}`): the WRITE authority that flips
/// a tenant's `tenant_byok_config.state` to `active` (engaging the r2_s3
/// at-rest encryption gate) + persists its CMK-wrapped Tcs, plus the crypto-shred
/// kill switch. Internal-auth gated exactly like `admin`. Closes the H5 gap
/// left by migration 0081 (the read model + engagement gate were inert with no
/// writer).
pub mod byok_admin;
/// Customer-facing audit-analytics routes (Wave-18 wiring of the
/// Neon analytics shadow sync): `GET /v1/audit/analytics/event-count`
/// + `GET /v1/audit/analytics/timeline` over the per-tenant
/// `audit_events_shadow` Neon table. The shadow is analytics-only;
/// the canonical chain-integrity store is the R2 NDJSON archive
/// (Wave 15) — see `specs/_audits/sealed/2026-05-15-neon-analytics-shadow.md`.
pub mod audit_analytics;
/// Internal S-09 audit-chain drain route: `POST /_internal/audit/drain`.
/// Seals the live `audit_outbox` trail into the BLAKE3 tamper-evident hash
/// chain (computes each row's RFC-8785 JCS canonical bytes + BLAKE3 link,
/// flips `emitted_at`, advances the per-partition `audit_chain_head`
/// checkpoint under a compare-and-set anti-fork guard). Internal-auth gated
/// (mirrors [`dsr`]); env-gated mount in [`crate::main`] (erase key + D1).
pub mod audit_drain;
/// Customer-facing audit-export route (Wave-15.3 wiring of
/// WI-S09-008): `GET /v1/audit/export?from=&to=` streams NDJSON
/// audit events + inclusion proofs.
pub mod audit_export;
/// Fabric PAT introspection route (M1): `POST /internal/v1/auth/introspect`.
/// Reached only from the corelink-runners fabric via the container's
/// internal listener. Gated by the `X-Corelink-Internal-Auth` header bound
/// to a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY` secret (tight blast radius —
/// distinct from the mint secret). Verifies an inbound PAT via the shared
/// [`crate::adapter_pat`] pipeline and resolves the tenant's plan; fail-CLOSED
/// (503) on any backend fault, uniform `{valid:false}` on a bad PAT.
pub mod auth_introspect;
/// REAPI v2 Bazel remote-cache routes (Phase 0 Stream B1):
/// `GET/PUT /bazel/v2/:instance/blobs/:hash/:size`,
/// `PUT /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size`,
/// `GET/PUT /bazel/v2/:instance/blobs/ac/:hash/:size`, and
/// `POST /bazel/v2/:instance/findMissingBlobs`.
///
/// Enables `--remote_cache=https://corelink-api.humangr.com/bazel/v2`
/// for any Bazel user; backed by the same R2 CAS/AC blobs as the
/// native `/v1/cas` and `/v1/ac` routes.
pub mod bazel_v2;
/// Runner billing usage-push INGEST route (ASK-2):
/// `POST /internal/v1/billing/usage`. Reached only from the corelink-runners
/// fabric via the container's internal listener. Gated by the
/// `X-Corelink-Internal-Auth` header bound to a DEDICATED
/// `BILLING_INGEST_AUTH_KEY` secret (tight blast radius — distinct from the
/// mint / introspect / erase secrets). Idempotently stages a JSON BATCH of
/// raw per-lease usage records into the canonical `usage_event_staging` D1
/// table the [`corelink_billing_aggregator`] drains + rolls up; it does NOT
/// aggregate or touch Stripe. Fail-CLOSED (503) on any backend fault, 400
/// on a malformed batch. Env-gated mount in [`crate::main`] (unmounted in
/// dev/CI).
pub mod billing_ingest;
/// Homebrew bottle cache surface (Phase B): `/brew/<tenant>/<bottle-path>`
/// nests the `corelink_adapter_host::brew` read-through proxy. Option-B PAT
/// re-verify via the shared [`crate::adapter_pat`] verifier; public bottle
/// bytes dedup cross-tenant through the 2-level [`crate::adapter_cache`] moat.
/// Env-gated mount in [`build_with_factory`].
pub mod brew;
/// sccache HTTP build-cache surface: `/cargo/<tenant>/<key>` (FINDING
/// Gap 1). Mounts `corelink_adapter_host::cargo` with a D1-backed PAT
/// resolver (Option B) + per-operation scope gate. Mounted only when
/// `PAT_SIGNING_KEY` + the D1/R2 `StorageEnv` are present (fail-CLOSED:
/// unmounted in dev/CI). See module docs.
pub mod cargo;
/// CAS HTTP routes (R-prep example wire-up; wave-8).
pub mod cas;
/// Per-hash CAS erase + 410-Gone tombstone (hugit-P2 seam B, WP-B):
/// `POST /_internal/cas/:tenant/:hash/erase` (internal-auth gated, write-side).
/// Deletes a blob from R2 (composing the DSR Wave 1 / PR #254 R2 CAS erase
/// primitives) and writes a `cas_tombstone` row (migration 0067) so a
/// subsequent GET returns HTTP 410 Gone. Pure decision logic lives in
/// `corelink-handler-cas-erase`. Route mounting is owner-gated on the R2 eraser
/// (the #254 seam); until then `build_state_from_env` returns `None`.
pub mod cas_erase;
/// Customer self-serve HTTP routes (Stream-2.6): `/v1/customer/*` endpoints
/// (overview, usage, billing, keys, team, audit) wired via
/// `corelink-handler-customer` trait objects. Worker matchRoute already
/// forwards these paths to the container; this module is the final link
/// that makes them return real responses instead of 404.
pub mod customer;
/// Customer Runners read surface (BE-10): tenant-scoped entitlement/allowlist/runs.
pub mod customer_runners;
/// Customer Workspaces surface (BE-11): tenant-scoped snapshot CRUD + pin.
pub mod workspaces;
/// Internal DSR erasure route (WI-S11-008): `POST /_internal/dsr/erase`.
/// Reachable only from the Cloudflare DO; gated by the same
/// `X-Corelink-Internal-Auth` shared secret. Drives the 12-backend erasure
/// orchestrator (Wave 0: in-memory no-op adapters; Wave 1 wires real transports).
pub mod dsr;
/// Internal PAT mint route (Stream-5): `POST /_internal/pat/mint`.
/// Only reachable from the Cloudflare Durable Object via
/// `container.getTcpPort(50051)`. Gated by the `X-Corelink-Internal-Auth`
/// shared-secret header. Mints a fresh PAT plaintext using
/// `corelink_pat::mint::mint(...)` and returns the hash + plaintext
/// for the signup-worker to write to D1 and Clerk session metadata.
pub mod internal_pat;
/// npm registry cache surface (Phase B): `/npm/<tenant>/<rest>` nests the
/// `corelink_adapter_host::npm` read-through `registry.npmjs.org` mirror.
/// Option-B PAT re-verify via the shared [`crate::adapter_pat`] verifier;
/// tarball bytes dedup through the 2-level [`crate::adapter_cache`] moat,
/// mutable package metadata in the D1-backed [`crate::adapter_kv`] KV.
/// Env-gated mount in [`build_with_factory`].
pub mod npm;
/// OCI Distribution v1.1 registry surface (Phase B): `/v2/*` + `/token`
/// mounts the `corelink_adapter_host::oci` adapter (docker / podman / buildah
/// / containerd / Helm OCI). Two-leg auth: `/token` exchanges a PAT (Option-B
/// re-verify via the shared [`crate::adapter_pat`] verifier) for a short-lived
/// HMAC registry bearer, downscoped to the PAT's capability; `/v2` ops verify
/// the bearer locally + enforce its repo scope. Blobs dedup through the
/// [`crate::adapter_cache`] moat; mutable manifests/tags in the durable
/// [`crate::adapter_oci_kv`] D1 KV. Env-gated mount in [`build_with_factory`].
pub mod oci;
/// PyPI (pip / uv / poetry / pdm) cache surface (Phase B):
/// `/pip/<tenant>/<pep-path>` nests the `corelink_adapter_host::pip`
/// read-through PyPI mirror (PEP 503/691 simple index + content-addressed
/// wheels/sdists). Option-B PAT re-verify via the shared [`crate::adapter_pat`]
/// verifier; wheel bytes dedup through the 2-level [`crate::adapter_cache`] moat,
/// the mutable simple index in a per-tenant D1 KV. Env-gated mount in
/// [`build_with_factory`].
pub mod pip;
/// PUBLIC erasure-attestation verifier routes (Artifact 1, WP-C1):
/// `GET /v1/public/attestation/{request_id}` + `GET /v1/public/keys/erasure/{region}.pub`.
/// UNAUTHENTICATED by design (an erasure proof is publicly verifiable) — D1-read
/// only, mounted OUTSIDE the ratelimit/residency/auth layers in [`crate::main`].
pub mod public_attestation;
/// Data-residency guard middleware (backlog #29 — Schrems II leak). A router
/// `layer` that runs BEFORE any handler: it reads the trusted
/// `x-corelink-primary-region` macro (set by the edge Worker), maps it to a colo
/// via the FROZEN [`crate::storage::region_map`], and rejects with 409
/// `residency_violation` if it does not match THIS container's own
/// `R2_CAS_REGION` — defence-in-depth against a mis-bound regional Worker
/// landing an EU tenant's bytes in a US container. Disjoint from cas.rs/ac.rs
/// (no handler-body edits); wired as one `.layer(...)` line in [`build_with_factory`].
pub mod residency;
/// Per-tenant request-rate token-bucket middleware (audit #14/#16). A router
/// `layer` wrapping the already-built `corelink-ratelimit` engine: it charges
/// one token per request against the DO-injected `x-corelink-tenant-id`
/// tenant's bucket and rejects over-limit traffic with 429 + `Retry-After`.
/// Wired with ONE `.layer(...)` line in [`build_with_factory`] so it covers
/// the composed data plane (CAS/AC, Bazel, Turbo, sccache, adapters) but NOT
/// the `/_health` probe or `/_internal/*` routes (those mount in `main.rs`
/// AFTER `build_with_factory` returns). Fail-OPEN on absent tenant + on the
/// limiter's own internal fault (logged); see module docs.
pub mod ratelimit_layer;
/// Pilot signup route (wave-29 stream-1; closes DEBT-027 engineering-side).
/// Surfaces `POST /v1/signup/pilot/{token}` over an HMAC-SHA256
/// signed token + per-IP rate-limit + fail-CLOSED audit emit. See
/// `specs/_audits/sealed/2026-05-16-signup-corelink-dev-backend.md`.
pub mod signup;
/// `POST /v1/onboarding/tier-select` — server-side Stripe Checkout
/// Session creation for self-serve tier upgrades. Internal-auth gated
/// (constant-time) + edge-verified `x-corelink-tenant-id` (fail-CLOSED);
/// INV-ONBOARD-DPA-FIRST + durable 60s lock + hosted Stripe Checkout.
/// WI-S19-004 production wiring.
pub mod tier_select;
/// Production [`tier_select::TierSelectAudit`] adapter (WP-C scaffold):
/// fail-CLOSED audit-chain emit (mirrors `internal_pat` tracing-audit).
pub mod tier_select_audit;
/// Production [`tier_select::CheckoutCreator`] adapter (WP-B scaffold):
/// hosted Stripe Checkout via `StripeRealClient` (`spawn_blocking`). Holds
/// the email-seam decision (trait stays email-free; Stripe collects it).
pub mod tier_select_checkout;
/// Production [`tier_select::TierSelectStore`] adapter (WP-A scaffold):
/// the durable D1-over-HTTP lock / DPA / active-subscription / persist
/// transaction. Stub bodies (`todo!("WP-A")`) until WP-A fills the SQL.
pub mod tier_select_store;
/// Turborepo remote-cache routes (Phase 0 / Stream B2):
/// `GET/PUT /v8/artifacts/:hash`, `POST /v8/artifacts/events`,
/// `POST /v8/artifacts/status`.
///
/// Wires [`corelink_turbo_bridge`] into the container router so Turborepo
/// users can set `TURBO_API=https://corelink-api.humangr.com` and use
/// CoreLink CAS as their remote build cache.
///
/// Phase 0 backing store: `InMemoryKvStore` (non-persistent).
/// TODO(v2): swap for `R2KvStore` — see module doc.
pub mod turbo_v8;
/// Caller-identity reflection route: `GET /v1/users/me` reads the
/// Worker-injected `x-corelink-tenant-id` / `-token-prefix` /
/// `-route-kind` headers and echoes them as JSON. Lets clients verify
/// PAT wiring without exercising any data-plane (CAS/AC) surface.
pub mod users;

/// Per-route handle to the per-tenant monthly $-ceiling gate (ADR-0068;
/// hugit-P2 WP-G1). Bundles the shared [`crate::tenant_quota::QuotaGuard`]
/// with the FLAT per-op cost resolved once at boot
/// ([`crate::tenant_quota::cost_per_op_micros`]), so each billable handler
/// can charge a fixed cost without re-reading env on the hot path.
///
/// A billable handler holds an `Option<QuotaGate>` in its route state
/// (`None` in dev/CI without D1) and calls [`QuotaGate::check`] at the TOP,
/// AFTER the rate-limit / scope gate and BEFORE the work:
///
/// ```ignore
/// if let Some(gate) = state.quota.as_ref() {
///     if let Some(resp) = gate.check(&tenant).await { return resp; }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct QuotaGate {
    guard: Arc<crate::tenant_quota::QuotaGuard>,
    cost_micros: i64,
}

impl QuotaGate {
    /// Build the production gate from process env, or `None` in dev/CI
    /// (no D1 storage env). The flat per-op cost is resolved once here.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let guard = crate::tenant_quota::quota_guard_from_env()?;
        Some(Self {
            guard,
            cost_micros: crate::tenant_quota::cost_per_op_micros(),
        })
    }

    /// Charge ONE flat-cost billable op against `tenant`'s monthly
    /// $-ceiling. `Some(resp)` ⇒ REJECT (402 over-ceiling / 503
    /// fail-CLOSED); `None` ⇒ proceed. Delegates to
    /// [`crate::tenant_quota::QuotaGuard::check`].
    pub async fn check(&self, tenant: &str) -> Option<axum::response::Response> {
        self.guard.check(tenant, self.cost_micros).await
    }

    /// Charge a BATCH of `n` flat-cost billable ops against `tenant`'s
    /// monthly $-ceiling in a SINGLE check-and-accrue (CAA-360 #14/#18).
    ///
    /// `findMissingBlobs` fans one request out to up to `FIND_MISSING_BLOB_CAP`
    /// (4096) backend existence probes, so charging one flat op cost for the
    /// whole batch let a tenant drive thousands of probes per accrued dollar
    /// (the per-request cost model assumes one). The prior fix charged per
    /// digest in a loop capped at 64 iterations, which still under-charged any
    /// batch over 64 digests. This delegates to
    /// [`crate::tenant_quota::QuotaGuard::check_batch`], which charges the FULL
    /// `n × cost` in one atomic statement — proportional for every batch size,
    /// no per-digest D1 round-trips, and no iteration cap.
    ///
    /// `Some(resp)` ⇒ REJECT (402 over-ceiling / 503 fail-CLOSED); `None` ⇒
    /// proceed. `n = 0` charges nothing and returns `None`.
    pub async fn check_batch(&self, tenant: &str, n: usize) -> Option<axum::response::Response> {
        self.guard.check_batch(tenant, n, self.cost_micros).await
    }

    /// Construct a gate from an explicit guard + per-op cost. Used by route
    /// integration tests to wire a hermetic in-memory quota store (the
    /// production path uses [`Self::from_env`]).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_for_test(
        guard: Arc<crate::tenant_quota::QuotaGuard>,
        cost_micros: i64,
    ) -> Self {
        Self { guard, cost_micros }
    }
}

/// Per-tenant in-memory shadow-sink factory. Production wiring
/// replaces this with a Neon-backed factory (see
/// `apps/server/src/main.rs` boot path); the in-memory factory
/// keeps the `/v1/audit/analytics/*` routes mounted in dev/CI so the
/// handler surface stays exercised end-to-end without a live Postgres
/// dependency.
///
/// Each `for_tenant` call returns a FRESH `InMemoryNeonShadowSink` so
/// the tenant's RLS-binding (the sink owns the `tenant_id`) is
/// preserved per request. The sink's in-memory state is per-instance —
/// the dev/CI shadow does NOT persist across requests (this is
/// intentional: the in-memory factory is a route-shape fixture, not a
/// stand-in for the production Neon backing store).
#[derive(Debug, Default)]
pub struct InMemoryShadowSinkFactory;

impl InMemoryShadowSinkFactory {
    /// Construct the canonical in-memory factory.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ShadowSinkFactory for InMemoryShadowSinkFactory {
    fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        // Default region for the in-memory dev/CI sink is IAD — the
        // production factory resolves the per-tenant pinned region
        // from the tenant-config store.
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        Ok(Arc::new(InMemoryNeonShadowSink::new(
            tenant_id,
            Region::Iad,
            audit,
        )))
    }
}

/// Build the composed handler router merging CAS, AC, Admin,
/// audit-export, and (Wave-20 closure) audit-analytics sub-routers.
/// Each sub-router carries its own state.
///
/// The audit-analytics router is mounted unconditionally — the
/// [`InMemoryShadowSinkFactory`] is the default backing for dev/CI;
/// production deployments swap the factory at the boot path
/// (`apps/server/src/main.rs`) when the `neon-real` feature + per-
/// region `NEON_DB_URL_<REGION>` env vars are present.
pub fn build() -> Router {
    build_with_factory(Arc::new(InMemoryShadowSinkFactory::new()))
}

/// Build the composed router with an explicit
/// [`ShadowSinkFactory`] — used by the production boot path to swap
/// in a Neon-backed factory while keeping every other route shape
/// identical.
pub fn build_with_factory(shadow_factory: Arc<dyn ShadowSinkFactory>) -> Router {
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1). ONE
    // gate (D1-backed) shared across every billable data-plane surface
    // (CAS/AC, Bazel REAPI, Turbo, sccache), exactly like the rate-limit
    // gate. `None` in dev/CI (no D1 storage env) — the ceiling is then not
    // enforced, mirroring the adapter routes' fail-CLOSED env-gate.
    let quota = QuotaGate::from_env();
    if quota.is_none() {
        tracing::warn!(
            "tenant-quota: D1 storage env absent; per-tenant $-ceiling gate NOT \
             enforced on billable routes (dev/CI mode)"
        );
    }

    // Per-tenant monthly REQUEST-count gate (rt-nuclear #8 — the request-count
    // half). Backs the OCI surface, which the Worker forwards RAW and so never
    // counts against `monthly_request_counts` (migration 0071). D1-backed;
    // `None` in dev/CI (no D1 storage env), mirroring the $-ceiling gate above.
    // Only the OCI router consumes it — the native + other-adapter surfaces are
    // already metered at the Worker edge by `checkRequestQuota`
    // (`incrementMonthlyRequestCount`, exactly once per request). DELIBERATELY
    // not cloned into the native CAS/AC/Bazel/Turbo states like `quota` /
    // `pat_gate` are: unlike the $-ceiling (a SUM read) and byte-cap (an
    // idempotent reserve→commit on real bytes), the request counter is a raw
    // per-request INCREMENT, so wiring it here too would double-count every
    // native op (Worker edge + container) and false-deny legitimate traffic at
    // half the contracted cap. OCI is the ONE surface the Worker forwards RAW
    // (never counted at the edge), so the container gate is its sole enforcer —
    // together the two planes meter the fleet exactly once each. The boot
    // watchdog (`main.rs`) asserts THIS gate must arm in prod for exactly that
    // reason, scoped to OCI (not a fleet-wide-native claim).
    let request_count = crate::request_count::RequestCountGate::from_env();
    if request_count.is_none() {
        tracing::warn!(
            "request-count: D1 storage env absent; OCI monthly request-count cap NOT \
             enforced (dev/CI mode)"
        );
    }

    // Display usage aggregator (usage-metering-roi — the customer ROI surface):
    // ONE in-process meter shared across the instrumented flagship cache surfaces
    // (native CAS, Bazel REAPI, Turbo). Unlike the gates above, `record` is a
    // cheap fire-and-forget lock+increment at each hit/miss/write decision — NEVER
    // a D1 round-trip on the hot path — and a background flusher drains the
    // accumulated deltas to `usage_daily` every ~30s. `from_env()` wires the
    // D1-backed sink when StorageEnv is present; in dev/CI it is INERT (`record`
    // is a no-op, `spawn_flusher` a no-op), mirroring the gates above. It is NEVER
    // read on a request path and never bills anything (display telemetry only).
    let usage_meter = crate::usage_meter::UsageMeter::from_env();
    usage_meter
        .clone()
        .spawn_flusher(std::time::Duration::from_secs(30));

    // Native data-plane PAT possession gate (red-team #4) + per-tenant storage
    // byte accounting (#1). Built from env (PAT_SIGNING_KEY + StorageEnv / D1);
    // `None` in dev/CI ⇒ the native plane skips the Argon2id backstop and the
    // storage accrual, mirroring the quota/tombstone fail-safe env-gates above.
    // ONE gate/accountant is cloned (cheap `Arc`) into every billable native
    // state (CAS/AC/Bazel/Turbo) so all four surfaces re-verify the bearer PAT
    // against D1 (never trusting the Worker-injected tenant header) and accrue
    // bytes against the same `tenant_storage_state` row.
    let native_pat_gate = crate::native_pat_gate::native_pat_gate_from_env();
    if native_pat_gate.is_none() {
        tracing::warn!(
            "native PAT gate disabled: PAT_SIGNING_KEY/StorageEnv absent — native \
             CAS/AC/Bazel/Turbo do NOT re-verify the bearer PAT (dev/CI mode)"
        );
    }
    let byte_accountant = crate::byte_accounting::byte_accountant_from_env();
    if byte_accountant.is_none() {
        tracing::warn!(
            "storage byte-accounting disabled: D1 StorageEnv absent — CAS/AC writes \
             NOT accrued against tenant_storage_state.bytes_used (dev/CI mode)"
        );
    }

    let (cas_read_raw, cas_write_raw, cas_delete_raw, cas_list) = cas::build_handlers();
    // Storage byte accounting (red-team #1 / cluster B+C): wrap the CAS write +
    // delete trait objects in the reserve→commit→release decorator at the SINGLE
    // chokepoint every CAS write surface flows through (native CAS, Bazel REAPI,
    // OCI, and the cargo/brew/npm/pip language adapters all drive these SAME
    // `Arc<dyn …>` objects). Wrapping here means all of them inherit identical,
    // atomic, fail-CLOSED byte accounting — no per-surface duplication, and the
    // previously-uncounted sibling planes (cluster B) are closed for free. When
    // the accountant is absent (dev/CI, no D1) the raw handlers pass through
    // unwrapped. read/list are not write surfaces and stay unwrapped.
    let (cas_write_acct, cas_delete_acct): (
        Arc<dyn corelink_handler_cas::CasWriteHandler>,
        Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    ) = match byte_accountant.as_ref() {
        Some(acc) => {
            let acct = Arc::new(crate::byte_accounting::AccountingCasHandler::new(
                cas_write_raw.clone(),
                cas_delete_raw.clone(),
                acc.clone(),
            ));
            (
                acct.clone() as Arc<dyn corelink_handler_cas::CasWriteHandler>,
                acct as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
            )
        }
        None => (cas_write_raw, cas_delete_raw),
    };
    // 410-Gone tombstone read gate (hugit-P2 seam B, WP-B). Wired from env
    // (D1-backed) when D1 creds are present so an erased hash answers 410 even
    // before the (#254-gated) erase WRITE route is mounted; `None` in dev/CI ⇒
    // classic 200/404 (fail-safe — no false 410s without a real store).
    // WP-2b integration: front the D1 tombstone store with an in-memory bloom so
    // the read hot path skips the per-request D1-over-HTTP round-trip for the
    // (overwhelmingly common) non-erased digest. Safe because the erase write
    // side deletes the R2 bytes BEFORE writing the tombstone row (cas_erase.rs
    // ordering): a bloom miss on a cross-instance-erased digest falls through to
    // an R2 read that 404s (bytes gone) — it never serves erased content, at
    // worst returns 404 instead of 410 within the bounded refresh window. See
    // `BloomTombstoneStore`.
    let cas_tombstones: Option<Arc<dyn cas_erase::TombstoneStore>> =
        cas_erase::D1TombstoneStore::from_env().map(|s| {
            Arc::new(cas_erase::BloomTombstoneStore::new(
                Arc::new(s) as Arc<dyn cas_erase::TombstoneStore>
            )) as Arc<dyn cas_erase::TombstoneStore>
        });
    // F-004 — CENTRALIZE the 410 erasure gate at the shared CAS seam. The native
    // route's inline tombstone gate (`cas::handle_read`) was the ONLY gate, so
    // GDPR-erased bytes were re-PUT-able (writes were ungated) and readable via
    // every OTHER surface (cargo/sccache/brew/npm/pip/Bazel/Turbo/OCI all drive
    // these SAME shared `Arc<dyn …>` handlers but never checked a tombstone).
    // Wrap the shared read/write/delete trait objects in
    // `TombstoneGatedCasHandler` HERE — the same chokepoint `AccountingCasHandler`
    // uses — so ALL surfaces inherit the erasure gate by construction: a
    // tombstoned read 404s (never serves erased bytes), a re-PUT of a tombstoned
    // hash is refused 410 (no resurrection), a gate fault fails CLOSED 503. The
    // bloom-fronted store (above) keeps this off the hot D1 path for the common
    // non-erased digest. When no tombstone store is wired (dev/CI) the handlers
    // pass through ungated. The native route keeps `cas_tombstones` too, so it
    // still answers a precise 410 (not 404) BEFORE reaching the handler.
    let (cas_read, cas_write, cas_delete): (
        Arc<dyn corelink_handler_cas::CasReadHandler>,
        Arc<dyn corelink_handler_cas::CasWriteHandler>,
        Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    ) = match cas_tombstones.as_ref() {
        Some(ts) => {
            let gated = Arc::new(cas_erase::TombstoneGatedCasHandler::new(
                cas_read_raw.clone(),
                cas_write_acct.clone(),
                cas_delete_acct.clone(),
                ts.clone(),
            ));
            (
                gated.clone() as Arc<dyn corelink_handler_cas::CasReadHandler>,
                gated.clone() as Arc<dyn corelink_handler_cas::CasWriteHandler>,
                gated as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
            )
        }
        None => (cas_read_raw, cas_write_acct, cas_delete_acct),
    };
    let cas_state = cas::CasRouteState {
        read: cas_read.clone(),
        write: cas_write.clone(),
        delete: cas_delete,
        list: cas_list,
        tombstones: cas_tombstones,
        quota: quota.clone(),
        pat_gate: native_pat_gate.clone(),
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        // usage-metering-roi: the ONE shared display meter (clone = cheap Arc).
        usage_meter: usage_meter.clone(),
    };
    let (ac_lookup, ac_update_raw, ac_delete_raw, ac_list) = ac::build_handlers();
    // Storage byte accounting (cluster B+C) for the AC plane: same decorator
    // chokepoint over the AC update + delete trait objects, shared with the
    // Bazel REAPI AC write surface. `None` (dev/CI) ⇒ raw handlers pass through.
    let (ac_update, ac_delete): (
        Arc<dyn corelink_handler_ac::AcUpdateHandler>,
        Arc<dyn corelink_handler_ac::AcDeleteHandler>,
    ) = match byte_accountant.as_ref() {
        Some(acc) => {
            let acct = Arc::new(crate::byte_accounting::AccountingAcHandler::new(
                ac_update_raw.clone(),
                ac_delete_raw.clone(),
                acc.clone(),
            ));
            (
                acct.clone() as Arc<dyn corelink_handler_ac::AcUpdateHandler>,
                acct as Arc<dyn corelink_handler_ac::AcDeleteHandler>,
            )
        }
        None => (ac_update_raw, ac_delete_raw),
    };
    let ac_state = ac::AcRouteState {
        lookup: ac_lookup.clone(),
        update: ac_update.clone(),
        delete: ac_delete,
        list: ac_list,
        quota: quota.clone(),
        pat_gate: native_pat_gate.clone(),
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    };
    // Cache adapters share the SAME CAS trait objects (one R2 connection) —
    // clone BEFORE they are moved into the Bazel bridge below. cargo writes
    // per-tenant via CargoCasBridge; brew/npm/pip dedup through the 2-level moat.
    let cargo_cas_read = cas_read.clone();
    let cargo_cas_write = cas_write.clone();
    let brew_cas_read = cas_read.clone();
    let brew_cas_write = cas_write.clone();
    let npm_cas_read = cas_read.clone();
    let npm_cas_write = cas_write.clone();
    let pip_cas_read = cas_read.clone();
    let pip_cas_write = cas_write.clone();
    let oci_cas_read = cas_read.clone();
    let oci_cas_write = cas_write.clone();
    // Phase 0 Stream B1: Bazel REAPI v2 routes share the same CAS/AC
    // trait objects so all four route surfaces read from / write to the
    // same backing store. No new R2 connections are opened.
    let mut bazel_state = bazel_v2::build_handlers_from(cas_read, cas_write, ac_lookup, ac_update);
    bazel_state.quota = quota.clone();
    bazel_state.pat_gate = native_pat_gate.clone();
    bazel_state.usage_meter = usage_meter.clone();
    // SECURITY (admin control-plane gate): the `/v1/admin/*` and
    // `/v1/admin/pilots/*` surfaces are OPERATOR-ONLY — they must NOT be
    // reachable by any authenticated tenant PAT. We gate them behind the
    // `CORELINK_INTERNAL_AUTH_KEY` shared secret, exactly like
    // `/_internal/pat/mint`. The key is threaded into each route state;
    // when it is unset at boot the handlers fail CLOSED (403) and never
    // run privileged logic. (We always-mount and fail-closed rather than
    // conditionally mount, because `build_with_factory` returns a
    // non-optional `Router` and dev/CI must still construct it.)
    let admin_internal_auth_key = admin::internal_auth_key_from_env();
    if admin_internal_auth_key.is_none() {
        tracing::warn!(
            "CORELINK_INTERNAL_AUTH_KEY unset; /v1/admin/* and /v1/admin/pilots/* \
             mounted but FAIL CLOSED (403) — operator gate not configured (dev/CI mode)"
        );
    }
    let (admin_read, admin_mutate) = admin::build_handlers();
    let admin_state = admin::AdminRouteState {
        read: admin_read,
        mutate: admin_mutate,
        internal_auth_key: admin_internal_auth_key.clone(),
    };
    let (default_pilot_store, pilot_audit) = admin_pilot::build_handlers();
    // Durable pilot-tenant store (hugit-P2 WP-FOUND): D1-backed when the
    // D1 env is present so pilots survive container restarts; the
    // non-durable InMemory store is the dev/CI fallback (fail-closed
    // env-gate, mirroring `customer::build_handlers_from_env`).
    let pilot_store: std::sync::Arc<dyn admin_pilot::PilotStore> =
        match admin_pilot::D1PilotStore::from_env() {
            Some(d1_store) => d1_store,
            None => {
                tracing::warn!(
                    "admin_pilot: D1 env absent; /v1/admin/pilots backed by \
                     NON-durable InMemoryPilotStore (dev/CI mode)"
                );
                default_pilot_store
            }
        };
    let pilot_admin_state = admin_pilot::PilotAdminRouteState {
        store: pilot_store,
        audit_sink: pilot_audit,
        wall_clock: crate::wall_clock::default_wall_clock(),
        internal_auth_key: admin_internal_auth_key,
    };
    // BYOK activation control plane (operator-gated, same secret as `/v1/admin/*`).
    // Writer is `None` in dev/CI (no D1 creds) → routes fail CLOSED (503).
    let byok_admin_state = byok_admin::ByokAdminRouteState::from_env();
    let mut audit_export_state = audit_export::build_state();
    // Native PAT possession backstop (rt-nuclear #17): the audit-export +
    // analytics surfaces were UN-gated, so a leaked PAT_SIGNING_KEY could forge a
    // bearer to exfiltrate any victim tenant's audit log / analytics. Wire the
    // SAME gate the native CAS/AC/Bazel/Turbo states carry.
    audit_export_state.pat_gate = native_pat_gate.clone();
    let mut audit_analytics_state = audit_analytics::build_state(shadow_factory);
    audit_analytics_state.pat_gate = native_pat_gate.clone();
    // Customer dashboard (WP-3): D1-backed handler when the D1 env is
    // present; InMemory fallback for dev/CI (fail-closed env-gate,
    // mirroring `adapter_pat::PatVerifier::from_env`).
    let mut customer_state = customer::build_handlers_from_env();
    // Native PAT possession backstop (cycle-2 nuclear red-team, cluster A — the
    // customer control plane was UN-gated, so a leaked PAT_SIGNING_KEY could
    // HMAC-forge a PAT that minted a genuine cas:rw PAT for any victim tenant).
    // Wire the SAME gate the native CAS/AC/Bazel/Turbo states carry — EXACTLY as
    // `bazel_state.pat_gate = native_pat_gate.clone();` above.
    customer_state.pat_gate = native_pat_gate.clone();
    // Self-serve account-deletion (C-ACCTDEL): wire the GDPR erasure requester
    // over the same D1 source + the in-process DSR erasure worker (the Clerk
    // `user.deleted` path's engine). Env-gated: `None` in dev/CI → the
    // `POST /v1/customer/account/delete` route fails CLOSED (503). Mirrors the
    // `pat_gate` wiring above (a cross-module collaborator composed at the root).
    customer_state.account_deletion = customer::account_deletion_from_env();
    // Customer Runners (BE-10) + Workspaces (BE-11): tenant-scoped, own-tenant
    // only (tenant derived from the session header, never a client param). Wire
    // the SAME native-PAT possession backstop the customer/CAS states carry — a
    // leaked PAT_SIGNING_KEY must not forge access to these surfaces either.
    let mut customer_runners_state = customer_runners::build_state_from_env();
    customer_runners_state.pat_gate = native_pat_gate.clone();
    let mut workspaces_state = workspaces::build_handlers_from_env();
    workspaces_state.pat_gate = native_pat_gate.clone();
    // `/v1/users/me` gets the identical backstop (it reflected a forged PAT's
    // claimed identity un-gated).
    let users_state = users::UsersRouteState {
        pat_gate: native_pat_gate.clone(),
    };
    let mut turbo_state = turbo_v8::build_handlers();
    turbo_state.quota = quota.clone();
    turbo_state.pat_gate = native_pat_gate.clone();
    turbo_state.bytes = byte_accountant.clone();
    turbo_state.usage_meter = usage_meter.clone();
    let mut router = Router::new()
        .merge(cas::router(cas_state))
        .merge(ac::router(ac_state))
        .merge(admin::router(admin_state))
        .merge(admin_tenant_detail::router(
            admin_tenant_detail::AdminTenantDetailState::from_env(),
        ))
        .merge(admin_pilot::router(pilot_admin_state))
        .merge(byok_admin::router(byok_admin_state))
        .merge(audit_export::router(audit_export_state))
        .merge(audit_analytics::router(audit_analytics_state))
        .merge(users::router(users_state))
        .merge(customer::router(customer_state))
        .merge(customer_runners::router(customer_runners_state))
        .merge(workspaces::router(workspaces_state))
        .merge(bazel_v2::router(bazel_state))
        .merge(turbo_v8::router(turbo_state));

    // Pilot-signup route — env-gated, fail-CLOSED (mirrors `internal_pat`).
    // Mounted only when `SIGNUP_TOKEN_KEY` is present + valid (hex, ≥ 32
    // bytes decoded); in dev/CI without the secret it is absent (404)
    // rather than running with the public hardcoded dev key, which would
    // make the pilot-token HMAC forgeable.
    if let Some(signup_state) = signup::build_state_from_env() {
        router = router.merge(signup::router(signup_state));
    } else {
        tracing::warn!(
            "SIGNUP_TOKEN_KEY unset/invalid; /v1/signup/pilot NOT mounted (fail-CLOSED)"
        );
    }

    // Cache-adapter surfaces (Option B — the container re-verifies the bearer
    // PAT against D1 via the SHARED `adapter_pat::PatVerifier`, never trusting
    // the Worker-injected tenant header). Mounted only when the verifier builds
    // from env (PAT_SIGNING_KEY + StorageEnv); in dev/CI it is absent and these
    // routes 404 (fail-CLOSED) rather than running with an unconfigured
    // validator. Mirrors `internal_pat`'s env-gate.
    //
    // ONE `PatVerifier` is shared across cargo/brew/npm/pip: cargo wraps it via
    // `cargo::resolver_from_verifier`; brew/npm/pip take it directly. cargo needs
    // only the verifier (sccache keys are client-content-addressed, stored
    // per-tenant via CargoCasBridge — no cross-tenant dedup). brew/npm/pip add
    // the 2-level content-dedup moat: they need the D1-over-HTTP client, which
    // doubles as the url→content-hash map (`adapter_cache::UrlMapStore`) AND
    // pip's index-KV backend. A per-adapter dependency that fails to build from
    // env skips ONLY that adapter (fail-CLOSED per-adapter).
    if let Some(verifier) = crate::adapter_pat::PatVerifier::from_env() {
        let verifier = Arc::new(verifier);

        // cargo/brew/npm/oci/pip ALL ride the 2-level moat map (the D1-over-HTTP
        // client). cargo JOINED the moat (private per-tenant namespace) to fix the
        // sccache-key-as-CAS-digest HashMismatch 502, so it now also needs the D1
        // map. A per-adapter env builder that returns None skips ONLY that adapter
        // (fail-CLOSED). The $-ceiling gate + the resolver's two-layer write check
        // (F27: scope header AND the PAT-derived `can_write`) are threaded into each.
        match crate::adapter_cache::d1_map_from_env() {
            Some(d1) => {
                // cargo (sccache): PRIVATE per-tenant moat namespace (no _public).
                // Thread the SAME D1-backed per-tier cap resolver OCI uses so a
                // FRESH tenant's first cargo write auto-seeds its
                // `tenant_storage_state` row with the REAL cap (else the
                // indeterminate-cap reservation fails CLOSED → 502 on a brand-new
                // sccache user's first PUT). Reuses the moat's D1 client.
                let cargo_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                let cargo_cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver> =
                    Arc::new(crate::oci_cap::D1TenantCapResolver::new(d1.clone()));
                router = router.merge(cargo::router(
                    cargo_cas_read,
                    cargo_cas_write,
                    cargo_map,
                    cargo::resolver_from_verifier(verifier.clone()),
                    quota.clone(),
                    Some(cargo_cap_resolver),
                ));

                // brew: shared map + verifier (public bottles, cross-tenant dedup).
                let brew_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                router = router.merge(brew::router(
                    brew_cas_read,
                    brew_cas_write,
                    brew_map,
                    verifier.clone(),
                    quota.clone(),
                ));

                // npm: shared map + verifier + the D1-backed metadata KV table
                // (`adapter_npm_meta`). If its env builder returns None, skip npm only.
                match crate::adapter_kv::npm_kv_from_env() {
                    Some(npm_meta_kv) => {
                        let npm_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                        router = router.merge(npm::router(
                            npm_cas_read,
                            npm_cas_write,
                            npm_map,
                            npm_meta_kv,
                            verifier.clone(),
                            quota.clone(),
                        ));
                    }
                    None => {
                        tracing::warn!(
                            "npm metadata KV unavailable from env; /npm/* NOT mounted \
                             (fail-CLOSED) — cargo/brew/pip unaffected"
                        );
                    }
                }

                // oci: shared moat map (blobs) + the durable D1 manifest KV
                // (`adapter_oci_kv`, migration 0061) + verifier + the OCI session
                // HMAC key (CORELINK_OCI_TOKEN_KEY). All four required; skip oci
                // (fail-CLOSED) if the KV or the key is absent.
                match (
                    crate::adapter_oci_kv::oci_kv_from_env(),
                    // CAA-360 #8: canonical name first, then the legacy
                    // HUGR_OCI_TOKEN_KEY prod was deployed with (name drift), so the
                    // OCI route mounts regardless of which secret name is set.
                    crate::storage::non_empty_env(oci::OCI_TOKEN_KEY_ENV)
                        .or_else(|| crate::storage::non_empty_env(oci::OCI_TOKEN_KEY_ENV_LEGACY)),
                ) {
                    (Some(oci_kv), Some(token_key)) => {
                        let oci_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                        let oci_manifest_kv: Arc<
                            dyn corelink_adapter_host::oci::ports::ManifestKvStore,
                        > = oci_kv;
                        // WP #10: resolve the tenant's RESOLVED per-tier storage
                        // cap at `/token` mint (the only seam where OCI knows the
                        // tenant). Reuses the SAME D1 client the moat map uses;
                        // the cap is embedded in the signed bearer and reserved
                        // against at finalize. The Worker forwards OCI RAW and
                        // never sets the native `STORAGE_QUOTA_HEADER`, so this is
                        // the sole carrier of the cap for the OCI plane.
                        let oci_cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver> =
                            Arc::new(crate::oci_cap::D1TenantCapResolver::new(d1.clone()));
                        // G4b: tenant-suspend gate over the SAME D1 client. A
                        // suspended/erased tenant is denied the token mint AND
                        // every `/v2` op (the Worker forwards OCI RAW, so the
                        // worker-side suspend gate never sees it). Single-flight +
                        // 30 s TTL, fail-CLOSED for a known-suspended tenant.
                        let oci_suspend_resolver: Arc<dyn crate::oci_suspend::SuspendResolver> =
                            Arc::new(crate::oci_suspend::CachedSuspendResolver::new(
                                Arc::new(crate::oci_suspend::D1SuspendResolver::new(d1.clone())),
                                Arc::new(crate::wall_clock::SystemWallClock::new()),
                                crate::oci_suspend::DEFAULT_SUSPEND_CACHE_TTL_MS,
                            ));
                        router = router.merge(oci::router(
                            oci_cas_read,
                            oci_cas_write,
                            oci_map,
                            oci_manifest_kv,
                            verifier.clone(),
                            corelink_core::SecretWrap::new(token_key),
                            quota.clone(),
                            request_count.clone(),
                            Some(oci_cap_resolver),
                            Some(oci_suspend_resolver),
                        ));
                    }
                    _ => {
                        tracing::warn!(
                            "CORELINK_OCI_TOKEN_KEY or OCI manifest KV unavailable; \
                             /v2/* + /token (OCI registry) NOT mounted (fail-CLOSED) \
                             — cargo/brew/npm/pip unaffected"
                        );
                    }
                }

                // pip: shared map + verifier + the SAME D1 client (reused for the
                // per-tenant simple-index KV table `adapter_pip_index`).
                let pip_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                router = router.merge(pip::router(
                    pip_cas_read,
                    pip_cas_write,
                    pip_map,
                    d1,
                    verifier,
                    quota.clone(),
                ));
            }
            None => {
                tracing::warn!(
                    "D1 moat map unavailable from env; /cargo/*, /brew/*, /npm/*, \
                     /v2/* (OCI), /pip/* NOT mounted (fail-CLOSED)"
                );
            }
        }
    } else {
        tracing::warn!(
            "PAT_SIGNING_KEY/StorageEnv unset; cache-adapter routes \
             (/cargo/*, /brew/*, /npm/*, /pip/*) NOT mounted (dev/CI mode)"
        );
    }

    // backlog #29 (Schrems II residency leak): data-residency guard. Runs BEFORE
    // every handler — rejects (409) any request whose trusted
    // x-corelink-primary-region macro does not map to THIS container's
    // R2_CAS_REGION colo. Defence-in-depth backstop for a mis-bound regional
    // Worker; zero storage I/O on the reject path. Disjoint from cas.rs/ac.rs.
    router = router.layer(axum::middleware::from_fn(residency::residency_guard));

    // audit #14/#16 (request-rate limiting): per-tenant token-bucket gate over
    // the metered data plane. Charges one token per request against the
    // DO-injected `x-corelink-tenant-id` tenant's bucket (100 req/s sustained,
    // burst 200 — see `ratelimit_layer` consts) and 429s + Retry-After over the
    // limit. Applied here (NOT in `main.rs`) so it wraps exactly the data-plane
    // routers — the `/_health` readiness probe and `/_internal/*` surfaces are
    // merged later in `main.rs` and stay OUTSIDE this layer by construction.
    // Fail-OPEN on absent tenant (non-billable traffic) + on the limiter's own
    // internal fault (logged), prioritising paid-plane availability.
    //
    // F-017: wire the D1-backed per-tenant tier resolver so the per-tier RPS
    // ladder ACTUALLY enforces (without it every tenant sits on the team default).
    // Build a dedicated D1 client from env — the moat `d1` above is scoped to the
    // cache-adapter block — and reuse `oci_cap::D1TenantCapResolver` (one source
    // of truth for the tenant→tier lookup). If StorageEnv/D1 init is unavailable,
    // fall back to the resolver-less state: a config gap must NEVER brick the data
    // plane (fail-SAFE → team default for all), and the resolver only TIGHTENS
    // free/solo + loosens paid, so its absence is non-fatal.
    let rate_limit_state = match crate::storage::StorageEnv::from_env()
        .and_then(|env| crate::storage::d1_http::D1HttpClient::new(&env).ok())
    {
        Some(client) => {
            let tier_resolver: Arc<dyn ratelimit_layer::TenantTierResolver> =
                Arc::new(crate::oci_cap::D1TenantCapResolver::new(Arc::new(client)));
            ratelimit_layer::RateLimitLayerState::with_tier_resolver(
                crate::wall_clock::default_wall_clock(),
                tier_resolver,
            )
        }
        None => {
            tracing::warn!(
                "F-017: D1 tier-resolver unavailable (StorageEnv/D1 init); per-tenant \
                 rate-limit stays on the team default for all tenants (no per-tier ladder)"
            );
            ratelimit_layer::RateLimitLayerState::new()
        }
    };
    router = router.layer(axum::middleware::from_fn_with_state(
        rate_limit_state,
        ratelimit_layer::rate_limit_layer,
    ));

    router
}

/// Internal DSR legitimacy-anchor register route (GDPR1, per-user erasure):
/// `POST /_internal/dsr/anchor`. Lets the erasure-REQUEST authority register a
/// `dsr_requested` anchor for `(dsr_id, tenant)` so the per-digest CAS erase can
/// authorize a per-user (not whole-account) erasure. Dedicated key, held by a
/// DIFFERENT authority than the eraser (anti-forge). See `routes/dsr_anchor.rs`.
/// (Declared here, after the OKF-cited items above, to keep line-anchors stable.)
pub mod dsr_anchor;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn build_returns_composed_router() {
        // Smoke test: the composed router is constructable on the
        // native target. Per-route behaviour is covered by the
        // module-level integration tests in cas.rs / ac.rs / admin.rs.
        let _router = build();
    }

    #[test]
    fn in_memory_shadow_sink_factory_resolves_per_tenant() {
        // Wave-20: the dev/CI default factory yields a fresh
        // InMemoryNeonShadowSink per tenant id, with the tenant pin
        // load-bearing for the route's RLS-like construction-time gate.
        let f = InMemoryShadowSinkFactory::new();
        let tenant = uuid::Uuid::now_v7();
        let sink = f.for_tenant(tenant).expect("resolves");
        assert_eq!(sink.tenant_id(), tenant);
        assert_eq!(sink.region(), Region::Iad);
    }

    #[test]
    fn build_with_factory_accepts_in_memory_factory() {
        // Wave-20 closure: the production boot path uses
        // build_with_factory() to swap in a TokioPgShadowSinkFactory
        // under --feature neon-real; tests + dev use the in-memory
        // factory. Both branches must construct without panic.
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(InMemoryShadowSinkFactory::new());
        let _router = build_with_factory(factory);
    }

    #[tokio::test]
    async fn build_with_factory_produces_a_routing_router() {
        use axum::body::Body;
        use axum::http::{Method, Request, StatusCode};
        use tower::ServiceExt;

        // A real router has the routes mounted; the degenerate
        // `build_with_factory -> Default::default()` mutant returns an EMPTY
        // `Router` that 404s every path. GET on the POST-only `/v1/admin/mutate`
        // route → axum 405 (path registered, method mismatch) on the real
        // router, vs 404 on the empty one — kills that mutant.
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(InMemoryShadowSinkFactory::new());
        let router = build_with_factory(factory);
        let resp = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/admin/mutate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "build_with_factory must mount real routes (an empty router would 404)"
        );
    }
}
