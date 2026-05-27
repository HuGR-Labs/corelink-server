//! `GET /v1/audit/export` — customer-facing audit-log export route
//! (Wave-15.3; WI-S09-008 wiring).
//!
//! Streams NDJSON of every audit event in a tenant's `[from, to)`
//! window with a cryptographic inclusion proof per row, plus a
//! `chain_head_at_export` anchor footer the customer re-verifies
//! offline against the published daily-verify head.
//!
//! ## Auth model (production wiring)
//!
//! Production deployment slots a JWT-validating tower middleware in
//! front of this route. The middleware extracts the Clerk session
//! tenant id and injects it via the `X-Tenant-Id` request header
//! (mirrors the cas + admin wire-up patterns; see `cas.rs` doc on
//! the trait-object swap point for the wasm32 CF-Worker variant).
//! The route compares the header-supplied tenant id against the
//! `tenant_id` of every emitted row via `subtle::ConstantTimeEq`
//! (defence in depth — the comparator never short-circuits on the
//! first differing byte).
//!
//! Per the WI-S09-008 spec §4 the tenant_id is **NOT** a query
//! string. The route accepts an optional `tenant` query parameter
//! for the cross-tenant-attempt audit path: when a caller supplies
//! a `tenant` that disagrees with the authenticated principal we
//! emit `corelink.security.audit_export_cross_tenant_attempt.v1`
//! and reject with `403 Forbidden`. The query parameter is the
//! ONLY way to surface the attempted (vs. authenticated) tenant
//! mismatch.
//!
//! ## Inclusion proof
//!
//! Each NDJSON line carries `{event: <AuditEvent>, proof: <InclusionProof>}`
//! where `InclusionProof` is the canonical degenerate-Merkle chain
//! sibling list lifted from `corelink-audit-chain::exporter`. The
//! customer verifier (`corelink audit verify`) re-runs
//! [`corelink_audit_chain::verify_export_result`] over the streamed
//! manifest + rows; any mismatch surfaces as
//! `AuditChainError::ChainBreak` which the verifier renders as
//! `corelink.audit.export_verify_failed.v1` SEV-0 at server-side
//! emit time.
//!
//! ## Rate limit (per WI-S09-008 §7 Q2 default = yes)
//!
//! Bound to the existing `corelink-ratelimit::InMemoryTokenBucketRateLimiter`
//! framework (WI-S08-001). One concurrent export per tenant per
//! minute. The bucket key is `BucketKey::per_tenant_per_endpoint(
//! tenant_id, "audit.export")` so a tenant's export bucket is
//! isolated from its CAS/AC/admin buckets. Refill = 1 token per 60
//! seconds; burst = 1.
//!
//! ## Empty-range semantics
//!
//! A `[from, to)` window with zero matching events returns `200 OK`
//! with a zero-line NDJSON body + the manifest footer (empty
//! tenant audit history is a legitimate state per the WI-S09-008
//! spec — distinct from a `404` which would imply the tenant
//! itself is absent).
//!
//! ## Fail-CLOSED ordering
//!
//! The route emits the `corelink.audit.export_request.v1` audit row
//! BEFORE writing any bytes to the response stream. Audit-sink
//! failure aborts the export with `503 Service Unavailable` (mirrors
//! the cas + admin `AuditFailed → 503` discipline). Cross-tenant
//! reject emits `corelink.security.audit_export_cross_tenant_attempt.v1`
//! BEFORE the `403`. Verify-failed emits
//! `corelink.audit.export_verify_failed.v1` SEV-0 AFTER the bytes
//! are flushed (we still deliver the bytes — the caller decides
//! what to trust; the audit emit guarantees the server-side detect
//! anchor for the security team page-out).
//!
//! ### Wave-20 emit-discipline lift (closes A-P1-02/03/05/A-P2-01)
//!
//! Every pre-byte-stream audit-emit on a non-happy path is now
//! routed through [`emit_or_503`] — if the sink returns `Err` the
//! route aborts with `503 Service Unavailable` carrying the
//! `audit pipeline closed` body (mirrors the existing
//! `export_request.v1` arm at step 8). This closes the prior
//! regression where cross-tenant-reject / rate-limit-deny /
//! verify-failed-sev0 emits silently dropped on a paired
//! audit-sink-down + adversarial event, leaving the security team
//! with no anchor row.
//!
//! Mid-stream chain-break is the one case where a 503 is impossible
//! (response headers are already flushed). The async stream
//! generator surfaces an audit-emit failure via:
//!   1. a SEV-0 `tracing::error!` carrying the would-be audit row,
//!   2. force-closing the stream WITHOUT emitting the abort trailer
//!      frame (the consumer sees a truncated body — a louder
//!      signal than a missing audit row).
//! See [`stream::build_audit_export_async_stream`] for the
//! trade-off note.
//!
//! # Module layout (wave-33 stage 2.PRE-B.1.c decomposition)
//!
//! The monolithic `audit_export.rs` was decomposed into the
//! per-responsibility submodules below. Every pre-split public
//! symbol is re-exported here verbatim so external consumers (the
//! `apps/server/tests/audit_export.rs` integration tests, the
//! `routes.rs` builder, the customer-CLI compatibility tests) see
//! the SAME `crate::routes::audit_export::*` surface.
//!
//! - [`types`] — constants + [`ExportAuditRow`] data shape +
//!   [`EXIT_STATUS_VERIFY_FAILED_MID_STREAM`].
//! - [`audit_sink`] — [`ExportAuditSink`] trait,
//!   [`InMemoryExportAuditSink`] capture fake, and the
//!   [`emit_or_503`] shared fail-CLOSED helper.
//! - [`state`] — [`AuditExportRouteState`], [`build_state`],
//!   [`audit_export_rate_limit_config`], [`router`], and
//!   [`AuditExportQuery`].
//! - [`handler`] — `GET /v1/audit/export` axum handler.
//! - [`stream`] — wave-19 async page-by-page generator,
//!   [`R2ListPager`] + [`InMemoryR2ListPager`], and the
//!   mid-stream abort-trailer machinery.
//! - [`parse`] — timestamp + UUID-CT + NDJSON-serialize helpers.

#![forbid(unsafe_code)]
// W35-P2: module-level `//!` docs use short paths to absorbed-sibling
// items; under the umbrella crate's scope they would require full
// prefixes. Suppressing the lint preserves the original text.
#![allow(rustdoc::broken_intra_doc_links)]

pub mod audit_sink;
pub mod handler;
pub mod parse;
pub mod state;
pub mod stream;
pub mod types;

#[cfg(test)]
mod tests_basic;
#[cfg(test)]
mod tests_proptest;
#[cfg(test)]
mod tests_routes;
#[cfg(test)]
mod tests_stream;

// ---------------------------------------------------------------------------
// Canonical re-exports — preserve the pre-split
// `crate::routes::audit_export::*` public surface verbatim.
// ---------------------------------------------------------------------------

pub use audit_sink::{emit_or_503, ExportAuditSink, InMemoryExportAuditSink};
pub use state::{
    audit_export_rate_limit_config, build_state, router, AuditExportQuery,
    AuditExportRouteState,
};
pub use stream::{
    build_audit_export_async_stream, mid_stream_abort_trailer_value, InMemoryR2ListPager,
    R2ListPager,
};
pub use types::{
    ExportAuditRow, AUDIT_EXPORT_ROUTE, DEFAULT_EXPORT_ROW_BUFFER_BYTES,
    ENV_EXPORT_ROW_BUFFER_BYTES, EVENT_TYPE_CROSS_TENANT_ATTEMPT, EVENT_TYPE_EXPORT_REQUEST,
    EVENT_TYPE_VERIFY_FAILED, EXIT_STATUS_VERIFY_FAILED_MID_STREAM, HEADER_CHAIN_HEAD_ANCHOR,
    HEADER_EXPORT_ABORTED, R2_LIST_PAGE_SIZE, TENANT_ID_HEADER,
};
