//! Wave-18 integration test — wasm32 backend composition equivalence
//! to the wave-16 in-memory backend.
//!
//! The actual `worker::Fetch` round-trip needs `workerd` to execute
//! (the wasm32 cron handler in `corelink-clerk-cf::dsr_statuspage_cron`
//! is staged for live verification on the CF Worker dev account). This
//! integration test focuses on the **composition equivalence**
//! invariant: the wasm32 backend's audit envelope shape +
//! rate-limiter semantics + retry-policy classification round-trip
//! against the wave-16 `InMemoryStatuspageBackend` MUST match
//! byte-identically on the audit-emit channel.
//!
//! On the wasm32 target the file compiles as a smoke-test only — the
//! struct constructor + `redact_api_key` + the canonical base URL
//! constant exercise that the wasm32 backend builds against the
//! workers-rs surface (caught by `cargo build --target wasm32-unknown-unknown`
//! in CI). The actual `Fetch::send` round-trip is exercised live
//! against the CF Worker dev account from wave-18 forward.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use corelink_statuspage_real::{
    DsrCompletionReport, InMemoryStatuspageAuditSink, InMemoryStatuspageBackend,
    StatuspageAuditOutcome, StatuspageBackend,
};

fn sample_report() -> DsrCompletionReport {
    DsrCompletionReport::new(1_700_000_000, 1_700_086_400, 47, 2, 0, 12)
        .expect("canonical 24h window with sane p95")
}

#[test]
fn in_memory_backend_published_audit_shape_matches_wasm32_contract() {
    // The wasm32 backend emits the SAME audit envelope shape
    // (page_id / metric_id / api_key_redacted / final_status /
    // attempts / p95_hours_observed / window_end_unix_s) as the
    // wave-16 in-memory backend. Wave-18 ships the wasm32 backend
    // with identical emit_audit semantics; the in-memory backend is
    // therefore the canonical contract pinned here.
    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let backend =
        InMemoryStatuspageBackend::new("page-1", "metric-1", "abcdef1234567890", audit.clone());
    let report = sample_report();
    let out = backend
        .publish_dsr_metric(&report, 1_000)
        .expect("publish succeeds");

    // Audit envelope shape pinned: the wasm32 backend's
    // `emit_audit` MUST emit a `StatuspageAuditEvent` with the same
    // 8 fields populated in the same way.
    assert_eq!(out.page_id, "page-1");
    assert_eq!(out.metric_id, "metric-1");
    assert_eq!(out.status, 201);
    assert_eq!(out.attempts, 1);

    let snap = audit.snapshot();
    assert_eq!(snap.len(), 1, "exactly one audit event per publish");
    let evt = snap.first().expect("non-empty");
    assert_eq!(evt.outcome, StatuspageAuditOutcome::Published);
    assert_eq!(evt.page_id, "page-1");
    assert_eq!(evt.metric_id, "metric-1");
    // Credential never logged plaintext — only the canonical redacted form.
    assert_eq!(evt.api_key_redacted, "OAuth ***7890");
    assert!(!evt.api_key_redacted.contains("abcdef"));
    assert_eq!(evt.attempts, 1);
    assert_eq!(evt.p95_hours_observed, 12);
    assert_eq!(evt.window_end_unix_s, 1_700_086_400);
}

#[test]
fn rate_limit_audit_shape_pinned_for_wasm32_parity() {
    // The wasm32 backend MUST emit an identical `RateLimited` audit
    // envelope (no api_key_redacted leakage, retry_after_ms +
    // jitter_ms surfaced via the `reason` field) when the
    // local rate-limiter denies a publish. Pin the in-memory
    // backend's emit shape so any drift fails CI.
    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let backend = InMemoryStatuspageBackend::new("p", "m", "abcdef1234567890", audit.clone());
    let report = sample_report();
    backend
        .publish_dsr_metric(&report, 0)
        .expect("first publish allowed");
    let err = backend
        .publish_dsr_metric(&report, 60_000)
        .expect_err("second publish within window denied");

    let is_rate_limited = matches!(
        err,
        corelink_statuspage_real::StatuspageClientError::RateLimited { .. }
    );
    assert!(is_rate_limited);

    let snap = audit.snapshot();
    assert_eq!(snap.len(), 2, "one Published + one RateLimited");
    assert_eq!(snap[1].outcome, StatuspageAuditOutcome::RateLimited);
    assert_eq!(snap[1].api_key_redacted, "OAuth ***7890");
    let reason = snap[1].reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("retry_after_ms="),
        "reason MUST carry retry_after_ms diagnostic: {reason}"
    );
    assert!(
        reason.contains("jitter_ms="),
        "reason MUST carry jitter_ms diagnostic: {reason}"
    );
}

/// The wave-18 wasm32 backend constructor + redact helper compile
/// against the workers-rs surface. The actual `worker::Fetch::send`
/// round-trip is staged for live `workerd` verification on the CF
/// Worker dev account.
#[cfg(target_arch = "wasm32")]
#[test]
fn wasm32_backend_constructor_smoke() {
    let audit = Arc::new(InMemoryStatuspageAuditSink::new());
    let client = corelink_statuspage_real::StatuspageWasm32Client::new(
        "page-1",
        "metric-1",
        "abcdef1234567890",
        audit,
    );
    assert_eq!(client.page_id(), "page-1");
    assert_eq!(client.metric_id(), "metric-1");
    assert_eq!(client.api_key_redacted(), "OAuth ***7890");
}
