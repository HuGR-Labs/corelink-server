//! `corelink-server` — CoreLink server binary support library.
//!
//! Hosts modules that are exercised by integration tests (which need
//! to link against the crate as a library, not just the binary).
//!
//! # Modules
//!
//! - [`webhook`] — Stripe webhook HTTP shell (axum). Thin boundary
//!   over the canonical
//!   [`corelink_stripe_real::webhook_dispatch::WebhookDispatcher`]
//!   pipeline (wave-16 unification, audit doc
//!   `specs/_audits/sealed/2026-05-15-stripe-webhook-production.md`).
//! - [`routes`] — HTTP route surface. Currently exposes the CAS read
//!   end-to-end as the example wire-up for the R-prep handler-crate
//!   skeleton (see
//!   `specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md §6`).
//! - `byok` — feature-gated AWS-only BYOK provider factory (built
//!   when `--features byok-aws-real`). Preserved as a thin convenience
//!   wrapper; new code should use [`byok_orchestrator`].
//! - [`byok_orchestrator`] — singleton trait-object dispatch over the
//!   four production BYOK providers (AWS / GCP / Azure / Vault),
//!   feature-flag-selected at compile time. Default (no flag) returns
//!   an `InMemoryFake`. Multiple `byok-*-real` flags is a HARD
//!   compile error. See
//!   `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7`.
//! - [`wall_clock`] — cross-route wall-clock trait (`WallClock` +
//!   `SystemWallClock` + `InMemoryFakeWallClock`). Wave-21 closure of
//!   the `A-P2-05` (audit-export) + `B-P2-03` (audit-analytics)
//!   findings: both routes consume `Arc<dyn WallClock>` in their route
//!   state so the rate-limit `now_ms` becomes wall-clock-derived rather
//!   than window-derived. See
//!   `specs/_audits/sealed/2026-05-16-wave18-adversarial-review-streamA-audit-export.md`
//!   and `…-streamB-neon-shadow.md`.
//!
//! # INV pin map (W36-PROPTEST-FU-001 closure)
//!
//! This crate carries INV references in route-boundary doc comments
//! that name **which invariant is pinned at the HTTP boundary**, not
//! where the load-bearing property tests live. Per WI-PROPTEST-FU-W33-001
//! closure (`specs/_audits/sealed/2026-05-26-w36-proptest-fu-001-seal.md`),
//! the property tests for each pinned INV live in the owning crate
//! listed below; this crate is listed in
//! `scripts/proptest-density-allowlist.txt` as an
//! "inv-pin documentation" exemption:
//!
//! | INV ref pinned here              | Property-test owner crate(s)                         |
//! |----------------------------------|------------------------------------------------------|
//! | `INV-AUTH-MIGRATION-ADDITIVE`    | `corelink-d1-migrations` (`tests/prop_migration_additivity.rs`) |
//! | `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` | `corelink-audit` + `corelink-slack-real` (`tests/prop_slack_emit_atomic.rs`) |
//! | `INV-TENANT-ISOLATION`           | `corelink-tenant-path` + `corelink-auth::schema` (`corelink-auth/tests/schema_prop_schema.rs`) |
//!
//! The route-boundary references in `src/routes/*.rs` are **pin
//! annotations** documenting which invariant the route enforces; they
//! do NOT relocate the property logic. See the SEAL audit above for
//! the full closure narrative.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

#[cfg(feature = "byok-aws-real")]
pub mod byok;

/// 2-level content-dedup MOAT store shared by the cache adapters: bytes
/// are content-addressed in CAS (blake3, deduped) behind a D1
/// `(namespace, url_hash) → content_hash` map. Public deps share the
/// `_public` namespace (cross-tenant dedup — the network-effect moat);
/// private artifacts stay per-tenant. See module docs.
pub mod adapter_cache;
/// D1-backed KV for the cache adapters' MUTABLE metadata (npm package
/// documents in `adapter_npm_meta`, public/private namespaced). See module docs.
pub mod adapter_kv;
/// Durable D1-backed `ManifestKvStore` for the OCI registry adapter
/// (mutable manifests + tag lists in `adapter_oci_kv`; blobs go through the
/// content-addressed moat). See module docs.
pub mod adapter_oci_kv;
/// Shared container-side PAT verifier (Option B) for ALL cache adapters
/// (cargo / brew / npm / oci / pip). Trait-agnostic
/// [`adapter_pat::PatVerifier::verify`]; each adapter route wraps it in a
/// thin newtype impl of that adapter's `TenantResolver` port. The HMAC
/// fast-reject → D1 lookup → Argon2id → fail-CLOSED scope pipeline lives
/// here once (replaces the cargo-only `cargo_pat_resolver`). See module docs.
pub mod adapter_pat;
pub mod auth_tenant;
/// Durable native [`corelink_billing_stripe_materializer::BillingD1Writer`]
/// over the CF D1 REST API (`billing_d1_http::D1HttpBillingWriter`). Bridges
/// the SYNC billing-writer trait (shared with the wasm32 Worker) to the
/// async [`storage::d1_http::D1HttpClient`] via `block_in_place`, so the
/// native Stripe-webhook materializer writes DURABLY to D1 instead of the
/// in-memory mirror. See module docs.
pub mod billing_d1_http;
pub mod byok_orchestrator;
/// Production D1-backed customer-dashboard handler (dashboard revival
/// WP-3): [`customer_d1::D1CustomerHandler`] implements all 6
/// `corelink-handler-customer` traits over the CF D1 REST API
/// (sync↔async bridge per [`billing_d1_http`]), replacing the
/// InMemory 404-stub for real tenants. HONEST v1: real data where a
/// deployed table exists, explicit empty/zero/501 where it doesn't.
/// See module docs for the per-endpoint matrix.
pub mod customer_d1;
#[cfg(feature = "neon-real")]
pub mod neon_shadow_factory;
pub mod routes;
/// Cache-scope enforcement helper + extractor.
///
/// Parses the Worker-set, server-trusted `x-corelink-scope` header (the
/// PAT's D1 scope string) into a checkable form and gates the cache
/// surfaces (CAS / AC / Turbo) on read vs write capability. Fail-CLOSED:
/// missing/empty scope grants nothing. See module docs for the grammar.
pub mod scope;
/// Native-container storage adapters (R2 S3-compatible API + D1 HTTP).
///
/// WP-S1 Phase 1 — provides [`storage::r2_s3::R2CasHandler`] (real
/// CAS read/write against Cloudflare R2 via `aws-sdk-s3`) and
/// [`storage::d1_http::D1HttpClient`] (metadata reads via the CF D1
/// HTTP API). Runtime selection: when `R2_S3_ACCESS_KEY_ID` etc. are
/// present the real adapters are used; otherwise the InMemory fakes
/// remain active for tests + local dev.
pub mod storage;
pub mod wall_clock;
pub mod webhook;
