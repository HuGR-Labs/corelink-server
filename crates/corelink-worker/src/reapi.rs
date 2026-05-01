//! CoreLink REAPI v2 host-side handler module (S-04 / WI-S04-001).
//!
//! ## Module scope
//!
//! `corelink-worker::reapi` hosts the *pure-logic* handler implementations
//! for the REAPI v2 surface that S-04 brings online: the
//! `ActionCache::GetActionResult` + `ActionCache::UpdateActionResult`
//! handlers per `remote_cache_product_profile.md §6 (REAPI conformance)`
//! + `cas_profile.md §1.2`.
//!
//! ## What lives here, what is deferred
//!
//! - **Lives here (this WI, WI-S04-001)**: handler trait
//!   ([`ac::ActionCacheHandler`]) + canonical impl
//!   ([`ac::ActionCacheHandlerImpl`]); per-WI trait abstractions for
//!   `ac_meta` row store, AC envelope signer, Merkle verifier, output
//!   blob aliveness check, audit emitter. Each abstraction has an
//!   InMemory test fake exercising every code path; canonical 5-Layer
//!   Defense (TenantCtx-only, HMAC tenant prefix, scope check, region
//!   pinning, audit emit) is enforced at the type-system level.
//! - **Deferred per charter trait-abstraction-defer pattern**:
//!   - Tonic gRPC + axum REST surface gated behind a future
//!     `host-server` feature on `corelink-reapi` (handler trait is the
//!     integration seam). Lands alongside REAPI conformance suite in
//!     WI-S04-006 + the proto vendoring follow-up.
//!   - Real Cloudflare D1 binding shim for `AcMetaStore` (the `ac_meta`
//!     table itself is delivered by WI-S04-002; the production binding
//!     adapter is wired in WI-S04-006 alongside the REAPI handler
//!     mounting on `corelink-reapi`).
//!   - HKDF tenant-key signer impl ([`ac::sig::Signer`] real impl
//!     lands in WI-S04-004).
//!   - Real Merkle codec + verifier (`corelink-ac` crate; trait + fake
//!     ship here, real impl WI-S04-003).
//!
//! ## Why pure-logic core in `corelink-worker`
//!
//! The S-04 Lote 10.4-tris decision pinned the AC handler module to
//! `crates/corelink-worker/src/reapi/ac.rs` (per WI §13 Artifacts
//! Produced table). Locating it here keeps the integration with the
//! existing `corelink-worker::middleware::AuthCtx` (S-03 WI-S03-003) +
//! `corelink-worker::cache::negative` (S-02 WI-S02-005) +
//! `corelink-worker::storage::r2` (S-01 WI-S01-003) idiomatic — all
//! through `pub` re-exports. The host-server surface (`tonic`, `axum`)
//! lands inside `corelink-reapi` per the established sprint pattern.
//!
//! ## 5-Layer Defense for AC (auth_model.md §8.1 + ADR-0035)
//!
//! - **Layer 1 — auth verify**: `AuthLayer` (S-03 WI-S03-003).
//! - **Layer 2 — D1 enforcement**: handler builds `(tenant_id,
//!   action_digest)` keyed lookups via the `AcMetaStore` trait whose
//!   surface is structurally tenant-scoped (`AcKey::new(ctx.tenant_id(),
//!   digest)`) — cross-tenant queries are unreachable through the
//!   public surface.
//! - **Layer 3 — scope check**: `AuthCtx::has_scope(SCOPE_CACHE_R |
//!   SCOPE_CACHE_W)` enforced as the first step of every handler call.
//! - **Layer 4 — HMAC tenant prefix**: `AcMetaRow::tenant_prefix` is
//!   materialized at INSERT time + read back on GET (no TDK access on
//!   GET hot path; ADR-0035 H-3).
//! - **Layer 5 — audit emit**: every terminal flow path emits an audit
//!   event ([`ac::AcEventType`]) via the [`ac::audit::AuditSink`]
//!   trait (atomic with the meta row mutation by spec — production
//!   binding wires the row INSERT into the same D1 batch).
//!
//! ## SplitBlob/SpliceBlob (S-05 / WI-S05-001)
//!
//! [`cas`] hosts the SplitBlob/SpliceBlob handler trait + impl plus the
//! per-WI trait abstractions for the multipart session row store, the
//! chunk content-addressable store, the manifest builder/verifier, and
//! the audit sink. The 5-Layer Defense is enforced symmetrically; the
//! trait surface refuses cross-tenant probing structurally
//! (`SessionKey`, `ChunkKey`, `ManifestKey` all take `tenant_id` by
//! value as their first PK component).

pub mod ac;
pub mod cas;
