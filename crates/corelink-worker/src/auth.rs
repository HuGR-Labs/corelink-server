//! Auth lifecycle adapters living above the Tower middleware
//! (`crate::middleware::auth`).
//!
//! S-03 ships [`revocation`] (WI-S03-004) — the canonical revocation
//! lifecycle adapter that orchestrates Neon SoT update, audit outbox
//! emission, KV session-cache invalidation, and cross-region
//! propagation across Cloudflare Workers regions in ≤ 60 s p99 single
//! SLA. The module is structured around four small trait abstractions
//! (one per Cloudflare primitive: Durable Object storage,
//! Postgres/Neon SoT writer, KV invalidator, Queue producer) so the
//! host-side property tests exercise the same orchestration surface
//! that runs in production.
//!
//! ## Why a feature gate
//!
//! Revocation is naturally async (each trait method awaits a
//! Cloudflare primitive); the test fakes use `tokio::sync::Mutex`
//! to model the single-writer-per-region invariant. To keep the
//! pure-logic storage adapters in `crate::storage` building cleanly
//! to `wasm32-unknown-unknown` without pulling tokio into the default
//! feature set, the module is gated behind the same
//! `tower-middleware` feature that already brings tokio + tracing +
//! corelink-pat into the build.
//!
//! ## Integration with the verify hot path
//!
//! Per **INV-AUTH-NEON-IS-SOT** the verify hot path consults
//! Neon `pat.revoked_at IS NULL` (cold path) or the session cache
//! (hot path) — never `RevocationStore::is_revoked()` directly. The
//! revocation adapter writes to all three surfaces atomically (Neon
//! UPDATE + audit_outbox INSERT in one Postgres transaction; DO
//! storage upsert immediately after; KV invalidate best-effort).
//! The DO `is_revoked` query exists for the admin / audit query
//! plane (S-13 forward); the verify middleware never calls it.
//!
//! ## ⚠️ Deployment status (2026-07-12) — DEFERRED, not the live path
//!
//! This entire module (behind `tower-middleware`) is DESIGN-INTENT and is
//! NOT wired into the production verify hot path — its only callers today
//! are the in-memory fakes used by the host-side property tests. The claim
//! above that "the verify hot path consults Neon" describes the DEFERRED
//! Neon-SoT design (ADR-0030 / `INV-AUTH-NEON-IS-SOT`), not the running
//! system. The LIVE PAT-revocation source-of-truth is **D1**: the container
//! verifier checks `revoked_at_ms IS NULL` against the D1 `pat` table
//! (migration `0063_pat_customer_keys`) on every verify
//! (`corelink-container::adapter_pat`), enforced under
//! `INV-PAT-REVOKE-PROPAGATION` and TLA-verified by `auth_pat_revoke.tla`.

pub mod revocation;
