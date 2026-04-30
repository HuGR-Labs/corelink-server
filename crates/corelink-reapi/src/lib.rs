//! CoreLink REAPI v2 handlers (WI-S01-005).
//!
//! This crate is the API surface that ties together every prior S-01 WI:
//!
//! ```text
//!   gRPC inbound (Bazel/Buck2 client)
//!         │
//!         ▼
//!   PatValidator → TenantCtx          ← S-01-005 stub; Clerk-real in S-03
//!         │
//!         ▼
//!   VerifiedBody::new                 ← corelink-hash (WI-S01-002),
//!                                       INV-CAS-INTEGRITY enforced at type
//!         │
//!         ▼
//!   ScopedR2Writer.put_verified       ← corelink-worker (WI-S01-003),
//!                                       INV-CAS-IMMUTABILITY via
//!                                       `If-None-Match: *`
//!         │
//!         ▼
//!   MetaStore.commit_put              ← corelink-meta (WI-S01-004),
//!                                       atomic D1 batch with audit_outbox
//!                                       (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER)
//!         │
//!         ▼
//!   gRPC outbound (per-blob status)
//! ```
//!
//! ## Feature gating
//!
//! The `host-server` feature (default-on) enables the tonic-based gRPC server
//! glue. The pure-logic modules (`pat`, `audit`, `capabilities`, `error_map`,
//! `orchestrator`) compile to `wasm32-unknown-unknown` without `host-server`
//! so the same code paths can run inside a Cloudflare Worker via `tonic-web`
//! or `worker::Router` integration in a follow-up WI.
//!
//! ## Public surface
//!
//! - [`PatValidator`] — auth-stub interface contract (S-02↔S-03 bridge per
//!   `auth_stub_contract.md`).
//! - [`StubPatValidator`] — fixture-backed test fake.
//! - [`TenantContext`] — per-request bundle: tenant_id, region, scopes,
//!   request_id (immutable; constructed once per request).
//! - [`AuditEnvelopeBuilder`] — CloudEvents 1.0 envelope serializer; pairs
//!   atomically with every `MetaStore.commit_put` via the outbox pattern.
//! - [`CasWriteOrchestrator`] — pure-logic per-blob orchestration: VerifiedBody
//!   verify → R2 PUT → D1 batch. Used by both the gRPC handlers below and
//!   any future REST/WASM transport.
//! - [`CasWriteService`] (host-server) — gRPC `ContentAddressableStorage`.
//! - [`CapabilitiesService`] (host-server) — gRPC `Capabilities`.
//! - [`ByteStreamService`] (host-server) — gRPC `ByteStream::Write` +
//!   `ByteStream::Read` (the latter landed in WI-S02-001; the older name
//!   `ByteStreamWriteService` was renamed for symmetry — read + write
//!   land on the same service).
//!
//! ## Anti-patterns (do NOT)
//!
//! - **Do not** bypass [`CasWriteOrchestrator`] in any new transport. The
//!   per-blob orchestration order (VerifiedBody → R2 → D1) is the load-bearing
//!   correctness guarantee; reordering means orphan classes (R2-only,
//!   D1-only).
//! - **Do not** synthesize a [`TenantContext`] outside the auth path. The
//!   only constructor sources are [`PatValidator::authenticate`] and
//!   the public [`TenantContext::new`] (intentionally callable only by
//!   `PatValidator` impls + `#[cfg(test)]` modules in this crate).
//! - **Do not** emit audit events outside
//!   [`CasWriteOrchestrator::commit_put`]. Audit emission is paired
//!   atomically with `MetaStore.commit_put` to preserve
//!   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.

#![forbid(unsafe_code)]

pub mod audit;
pub mod capabilities;
pub mod error_map;
pub mod orchestrator;
pub mod pat;
pub mod read;

#[cfg(feature = "host-server")]
pub mod proto;

#[cfg(feature = "host-server")]
pub mod handler;

#[cfg(feature = "host-server")]
pub mod http_read;

pub use audit::{
    AuditEnvelope, AuditEnvelopeBuilder, REAPI_CROSS_TENANT_ATTEMPT, REAPI_PUT_COMPLETED,
    REAPI_R2_ORPHAN_DETECTED, REAPI_READ_COMPLETED, REAPI_READ_MISS, REAPI_TOMBSTONED_READ_ATTEMPT,
};
pub use capabilities::{
    cache_capabilities, server_capabilities, MAX_BATCH_TOTAL_SIZE_BYTES, MAX_CAS_BLOB_SIZE_BYTES,
};
pub use error_map::{
    miss_mapping, HashErrorMapping, MetaErrorMapping, R2ErrorMapping, ReadErrorMapping,
    COR_AUTH_PAT_INVALID, COR_AUTH_SCOPE_INSUFFICIENT, COR_CAS_BAD_DIGEST,
    COR_CAS_BAD_RESOURCE_NAME, COR_CAS_BATCH_TOO_LARGE, COR_CAS_BLOB_NOT_FOUND,
    COR_CAS_BLOB_TOO_LARGE, COR_CAS_DIGEST_FUNCTION_UNSUPPORTED, COR_INTERNAL,
    COR_SERVICE_DEGRADED, COR_TRANSIENT,
};
pub use orchestrator::{
    audit_request_id_for_blob, CasPutOutcome, CasWriteOrchestrator, NoopOrphanReconciler,
    OrchestratorError, OrphanReconciler, R2DeleteReconciler,
};
pub use pat::{AuthScope, AuthStubError, PatValidator, StubPatValidator, TenantContext};
pub use read::{CasReadOrchestrator, MissReason, ReadOrchestratorError, ReadOutcome};

#[cfg(feature = "host-server")]
pub use handler::{ByteStreamService, CapabilitiesService, CasWriteService};

#[cfg(feature = "host-server")]
pub use http_read::{cas_get_router, HttpReadState};
