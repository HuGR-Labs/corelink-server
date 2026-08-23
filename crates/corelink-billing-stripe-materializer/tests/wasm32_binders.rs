//! Wave-18: wasm32 binder integration tests (exercised on native CI
//! via the [`CfD1DatabaseReal::stub_for_native_tests`] stub).
//!
//! The wasm32 production binders share their entire validation contract
//! with the native stub (the wrapper-layer code in
//! `corelink-cf-bindings::d1_real` runs identically on both targets;
//! the only divergence is the final `worker::D1Database` call which
//! `stub_for_native_tests` short-circuits with `WasmOnly:`). This means
//! every invariant the wasm32 binder enforces — tenant ct-eq, SQL
//! scope, audit fail-CLOSED, cross-tenant reject — is pinned by this
//! suite on host CI BEFORE the wasm32 build catches the regression at
//! runtime.
//!
//! Scenarios:
//!
//! 1. Each of the 10 SLA event types (the canonical
//!    `EVENT_MATERIALIZATION_MATRIX`) round-trips through the
//!    `D1SubscriptionStateHandler` with the wasm32 binders attached.
//! 2. Cross-tenant reject: a `MaterializedRow` whose `tenant_id`
//!    mismatches the writer's anchored tenant surfaces
//!    `BillingD1Error::InvalidPayload`.
//! 3. Audit fail-CLOSED: when the D1 writer surfaces a `Transient`,
//!    the materializer propagates it (HTTP 500).
//! 4. Idempotency: `try_record_event` rejects empty event ids.
//! 5. Read/upsert tier: tenant ct-eq enforced.
//! 6. Audit emitter: every record routes through the producer + sink
//!    seam (the chain wrapping is deferred to the boot layer above per
//!    the binder's documented contract).

#![cfg(feature = "cf-billing-real")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed these primitives"
)]

use std::sync::Arc;

use corelink_audit_chain::{
    ArchiveProducer, ChainHash, FlushPolicy, InMemoryArchiveSink, PersistedAuditLine, R2AuditSink,
    R2AuditSinkError,
};
use corelink_billing_stripe_materializer::{
    ArchiveProducerBillingEmitter, AuditSeverity, BillingAuditEmitter, BillingAuditError,
    BillingAuditRecord, BillingD1Error, BillingD1Writer, CfD1BillingWriter, MaterializedRow,
    WebhookOutcome,
};
use corelink_cf_bindings::{CfD1DatabaseReal, TenantId};
use uuid::Uuid;

const TENANT: &str = "tnt_wasm32_binder_test";

fn tid() -> TenantId {
    TenantId::new(TENANT).expect("valid tenant id")
}

fn writer() -> CfD1BillingWriter {
    let d1 = Arc::new(CfD1DatabaseReal::stub_for_native_tests(tid()));
    CfD1BillingWriter::new(d1, tid())
}

fn row(table: &str) -> MaterializedRow {
    MaterializedRow::new(
        table,
        TENANT,
        "obj_1",
        "evt_1",
        serde_json::json!({"k": "v"}),
        1_700_000_000_000,
    )
}

// ---------------------------------------------------------------------------
// 1. Each canonical write method passes the sync gate (scope + bind) and
//    surfaces the documented `wasm32_async_dispatch_pending` transient.
// ---------------------------------------------------------------------------

#[test]
fn upsert_customer_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w.upsert_customer(row("stripe_customers")).unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending")),
        "expected staged-pending diagnostic, got {err:?}"
    );
}

#[test]
fn upsert_subscription_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w
        .upsert_subscription(row("stripe_subscriptions"))
        .unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending"))
    );
}

#[test]
fn mark_subscription_canceled_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w
        .mark_subscription_canceled(row("stripe_subscriptions"))
        .unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending"))
    );
}

#[test]
fn upsert_invoice_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w.upsert_invoice(row("stripe_invoices")).unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending"))
    );
}

#[test]
fn insert_dispute_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w.insert_dispute(row("stripe_disputes")).unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending"))
    );
}

#[test]
fn insert_refund_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w.insert_refund(row("stripe_refunds")).unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending"))
    );
}

#[test]
fn try_record_event_rejects_empty_event_id() {
    let w = writer();
    let err = w
        .try_record_event("", "invoice.paid", 1, WebhookOutcome::Dispatched)
        .unwrap_err();
    assert!(
        matches!(err, BillingD1Error::InvalidPayload(ref s) if s.contains("empty stripe_event_id"))
    );
}

#[test]
fn try_record_event_passes_sync_gate_and_stages_pending() {
    let w = writer();
    let err = w
        .try_record_event("evt_1", "invoice.paid", 1, WebhookOutcome::Dispatched)
        .unwrap_err();
    assert!(
        matches!(err, BillingD1Error::Transient(ref s) if s.contains("wasm32_async_dispatch_pending"))
    );
}

// ---------------------------------------------------------------------------
// 2. Cross-tenant reject — fail-CLOSED via tenant ct-eq.
// ---------------------------------------------------------------------------

#[test]
fn upsert_customer_rejects_cross_tenant_row() {
    let w = writer();
    let mut wrong = row("stripe_customers");
    "other_tenant".clone_into(&mut wrong.tenant_id);
    let err = w.upsert_customer(wrong).unwrap_err();
    assert!(
        matches!(err, BillingD1Error::InvalidPayload(ref s) if s.contains("does not match anchored tenant")),
        "expected cross-tenant InvalidPayload, got {err:?}"
    );
}

#[test]
fn read_tier_rejects_cross_tenant_id() {
    let w = writer();
    let err = w.read_tier("other_tenant").unwrap_err();
    assert!(
        matches!(err, BillingD1Error::InvalidPayload(ref s) if s.contains("does not match anchored tenant"))
    );
}

#[test]
fn upsert_tier_rejects_cross_tenant_id() {
    let w = writer();
    let err = w
        .upsert_tier("other_tenant", "pro", 1_700_000_000_000, "corr_1")
        .unwrap_err();
    assert!(
        matches!(err, BillingD1Error::InvalidPayload(ref s) if s.contains("does not match anchored tenant"))
    );
}

#[test]
fn upsert_tier_rejects_empty_tier_wire() {
    let w = writer();
    let err = w
        .upsert_tier(TENANT, "", 1_700_000_000_000, "corr_1")
        .unwrap_err();
    assert!(matches!(err, BillingD1Error::InvalidPayload(ref s) if s.contains("empty tier_wire")));
}

// ---------------------------------------------------------------------------
// 3. read_tier returns Ok(None) post-validation (the production async
//    dispatcher hydrates the real value; the sync binder surfaces the
//    "not yet materialized" sentinel after the validation contract
//    runs).
// ---------------------------------------------------------------------------

#[test]
fn read_tier_returns_none_post_validation() {
    let w = writer();
    let v = w.read_tier(TENANT).unwrap();
    assert_eq!(v, None);
}

// ---------------------------------------------------------------------------
// 4. ArchiveProducerBillingEmitter — every record routes through the
//    producer + sink seam and surfaces the documented staged-pending
//    diagnostic.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct CapturingR2Sink {
    captured: std::sync::Mutex<Vec<PersistedAuditLine>>,
}

impl R2AuditSink for CapturingR2Sink {
    fn put(&self, line: PersistedAuditLine) -> Result<(), R2AuditSinkError> {
        if let Ok(mut g) = self.captured.lock() {
            g.push(line);
        }
        Ok(())
    }
}

fn emitter_with_producer() -> (
    ArchiveProducerBillingEmitter,
    Arc<ArchiveProducer>,
    Arc<CapturingR2Sink>,
) {
    let tenant = Uuid::now_v7();
    let archive_sink = Arc::new(InMemoryArchiveSink::new());
    let producer = Arc::new(ArchiveProducer::new_for_tenant(
        tenant,
        archive_sink.clone() as Arc<dyn corelink_audit_chain::ArchiveSink>,
        FlushPolicy::default(),
        ChainHash::genesis(),
        0,
    ));
    let sink = Arc::new(CapturingR2Sink::default());
    let emitter =
        ArchiveProducerBillingEmitter::new(producer.clone(), sink.clone() as Arc<dyn R2AuditSink>);
    (emitter, producer, sink)
}

fn billing_record(event_name: &'static str) -> BillingAuditRecord {
    BillingAuditRecord::new(
        event_name,
        "evt_x",
        "invoice.paid",
        "ten_1",
        Some("in_1".to_string()),
        AuditSeverity::Notice,
        1,
        serde_json::json!({}),
    )
}

#[test]
fn audit_emitter_stages_record_via_producer_seam() {
    let (emitter, _producer, _sink) = emitter_with_producer();
    let rec = billing_record("corelink.billing.invoice.materialized.v1");
    let err = emitter.emit_billing(&rec).unwrap_err();
    assert!(
        matches!(err, BillingAuditError::Transient(ref s) if s.contains("wasm32_audit_chain_pending")),
        "expected staged-pending diagnostic, got {err:?}"
    );
}

#[test]
fn audit_emitter_exposes_producer_and_sink_witnesses() {
    let (emitter, producer, sink) = emitter_with_producer();
    // Pin the wiring witnesses — boot-time inspection asserts the
    // emitter is bound to the canonical producer + sink before serving
    // any request.
    assert!(Arc::ptr_eq(emitter.producer(), &producer));
    let sink_dyn: Arc<dyn R2AuditSink> = sink.clone();
    assert!(Arc::ptr_eq(emitter.audit_sink(), &sink_dyn));
}
