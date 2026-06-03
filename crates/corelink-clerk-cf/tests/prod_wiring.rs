//! Integration tests for the CF Worker production wiring.
//!
//! These tests exercise the boot path defined in
//! `corelink_clerk_cf::prod_wiring` on the native target. The four real
//! binding wrappers (`CfR2BucketReal`, `CfD1DatabaseReal`,
//! `CfKvNamespaceReal`, `CfDurableObjectReal`) all expose
//! `stub_for_native_tests` constructors that return
//! `*Error::Backend("WasmOnly: …")` AFTER the tenant-prefix + audit
//! contract has been validated. By driving the same handler code via the
//! native stubs we pin the contract on host CI without the wasm32
//! toolchain.
//!
//! Each test asserts on two invariants:
//!
//! 1. Every request emits the canonical audit event for the binding op
//!    (captured by an `AuditSink::recorder` sink).
//! 2. Cross-tenant access fails CLOSED — a probe against a binding
//!    anchored to tenant A using tenant-B data is rejected by the
//!    wrapper before reaching the (native-stub) backend.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test code"
)]

use std::sync::{Arc, Mutex};

use corelink_clerk_cf::audit_sink::{AuditEvent, AuditSink};
use corelink_clerk_cf::health::handle_health_real;
use corelink_clerk_cf::prod_wiring::{build_real_bindings_for_tests, TenantContext};

const TENANT_A: &str = "0123456789abcdef";
const TENANT_B: &str = "fedcba9876543210";

fn build_wired(
    tenant: &str,
) -> (
    corelink_clerk_cf::prod_wiring::CfRealBindings,
    Arc<Mutex<Vec<AuditEvent>>>,
) {
    let (sink, buf) = AuditSink::recorder(tenant);
    let ctx = TenantContext::from_header_value(tenant).expect("valid tenant");
    let bindings = build_real_bindings_for_tests(&ctx, sink);
    (bindings, buf)
}

#[tokio::test]
async fn boot_path_emits_audit_for_all_four_surfaces() {
    // Drive the production handler through the native stubs. Each
    // binding wrapper validates the tenant prefix, emits its canonical
    // audit event, then returns `WasmOnly: …` from the stub backend.
    let (bindings, buf) = build_wired(TENANT_A);
    let now = "2026-05-15T12:00:00Z";
    let probe = handle_health_real(&bindings, now)
        .await
        .expect("native probe ok");

    // KV/R2/DO native stubs all return WasmOnly diagnostics after
    // running the validation + audit contract.
    let kv_get = probe.kv_get_err.as_ref().expect("kv_get WasmOnly");
    assert!(kv_get.contains("WasmOnly"), "kv_get diagnostic: {kv_get}");
    let kv_put = probe.kv_put_err.as_ref().expect("kv_put WasmOnly");
    assert!(kv_put.contains("WasmOnly"), "kv_put diagnostic: {kv_put}");
    let r2_head = probe.r2_head_err.as_ref().expect("r2_head WasmOnly");
    assert!(
        r2_head.contains("WasmOnly"),
        "r2_head diagnostic: {r2_head}"
    );
    let do_resolve = probe.do_resolve_err.as_ref().expect("do_resolve WasmOnly");
    assert!(
        do_resolve.contains("WasmOnly"),
        "do_resolve diagnostic: {do_resolve}",
    );

    // D1 contract: scoped_query SUCCEEDS on the validate-only check,
    // prepare emits WasmOnly post-audit, bind emits WasmOnly post-CT-eq.
    assert!(
        probe.d1_scope_err.is_none(),
        "d1 scoped_query ok: {:?}",
        probe.d1_scope_err
    );
    let d1_prepare = probe.d1_prepare_err.as_ref().expect("d1 prepare WasmOnly");
    assert!(
        d1_prepare.contains("WasmOnly"),
        "d1 prepare diagnostic: {d1_prepare}"
    );
    let d1_bind = probe.d1_bind_err.as_ref().expect("d1 bind WasmOnly");
    assert!(
        d1_bind.contains("WasmOnly"),
        "d1 bind diagnostic (after CT-eq passed): {d1_bind}"
    );

    // Inspect the recorded audit events. We expect at minimum one
    // event per surface (kv, r2, d1, do).
    let captured = buf.lock().expect("recorder lock").clone();
    let surfaces: std::collections::BTreeSet<&'static str> =
        captured.iter().map(|e| e.surface).collect();
    assert!(surfaces.contains("kv"), "kv audit emitted: {surfaces:?}");
    assert!(surfaces.contains("r2"), "r2 audit emitted: {surfaces:?}");
    assert!(surfaces.contains("d1"), "d1 audit emitted: {surfaces:?}");
    assert!(surfaces.contains("do"), "do audit emitted: {surfaces:?}");

    // Spot-check the per-op labels.
    let kv_ops: std::collections::BTreeSet<&'static str> = captured
        .iter()
        .filter(|e| e.surface == "kv")
        .map(|e| e.op)
        .collect();
    assert!(kv_ops.contains("get"), "kv get logged: {kv_ops:?}");
    assert!(kv_ops.contains("put"), "kv put logged: {kv_ops:?}");
    let d1_ops: std::collections::BTreeSet<&'static str> = captured
        .iter()
        .filter(|e| e.surface == "d1")
        .map(|e| e.op)
        .collect();
    assert!(d1_ops.contains("prepare"), "d1 prepare logged: {d1_ops:?}");
    assert!(d1_ops.contains("bind"), "d1 bind logged: {d1_ops:?}");

    // Tenant label propagated into every audit event.
    for event in &captured {
        assert_eq!(event.tenant, TENANT_A, "tenant label on event {event:?}");
    }
}

#[tokio::test]
async fn cross_tenant_kv_isolation_silent_renamespace() {
    // KV cross-tenant defense is namespace ISOLATION (not a hard
    // reject): a key already shaped like `<tnt-B>:foo` is treated as a
    // tenant-A-local TAIL and re-prefixed to `<tnt-A>:<tnt-B>:foo`.
    // The wrapper never reaches TENANT_B's namespace, regardless of
    // what the caller asks for. We verify this by inspecting the
    // emitted audit subject: it MUST start with TENANT_A's prefix.
    let (bindings, buf) = build_wired(TENANT_A);
    let foreign_key = format!("{TENANT_B}:health:last_seen");
    // The call itself succeeds the validate phase (re-namespacing) and
    // then returns the native WasmOnly diagnostic at the stub. What we
    // really care about is the audit subject.
    let _ = bindings.kv.get_bytes(&foreign_key).await;
    let captured = buf.lock().expect("lock");
    let kv_events: Vec<&AuditEvent> = captured.iter().filter(|e| e.surface == "kv").collect();
    assert!(
        !kv_events.is_empty(),
        "kv audit event emitted for cross-tenant probe"
    );
    let subject = &kv_events[0].subject;
    let tenant_a_prefix = format!("{TENANT_A}:");
    assert!(
        subject.starts_with(&tenant_a_prefix),
        "kv probe re-namespaced under TENANT_A: subject={subject}"
    );
    // The audit subject MUST NOT be the bare TENANT_B-prefixed key —
    // that would imply the foreign namespace was reachable.
    assert_ne!(
        subject, &foreign_key,
        "kv subject must NOT bypass tenant prefix"
    );
}

#[tokio::test]
async fn cross_tenant_r2_isolation_silent_renamespace() {
    // Same namespace-isolation property for R2 (separator `/`).
    let (bindings, buf) = build_wired(TENANT_A);
    let foreign_key = format!("{TENANT_B}/health/probe");
    // `delete` is audit-fenced on both wasm32 and native (unlike `head`
    // whose native stub only validates prefix without emitting audit).
    let _ = bindings.r2.delete(&foreign_key).await;
    let captured = buf.lock().expect("lock");
    let r2_events: Vec<&AuditEvent> = captured.iter().filter(|e| e.surface == "r2").collect();
    assert!(
        !r2_events.is_empty(),
        "r2 audit event emitted for cross-tenant probe"
    );
    let subject = &r2_events[0].subject;
    let tenant_a_prefix = format!("{TENANT_A}/");
    assert!(
        subject.starts_with(&tenant_a_prefix),
        "r2 probe re-namespaced under TENANT_A: subject={subject}"
    );
    assert_ne!(
        subject, &foreign_key,
        "r2 subject must NOT bypass tenant prefix"
    );
}

#[tokio::test]
async fn cross_tenant_d1_bind_rejected_close() {
    // Anchor a D1 binding to TENANT_A then attempt to bind a statement
    // with TENANT_B as the first positional parameter. The wrapper's
    // CT-eq check refuses the bind. This MUST surface a `tenant_bind:`
    // error — distinct from the native stub's `WasmOnly:` diagnostic,
    // proving the CT-eq gate fires BEFORE the stub.
    let (bindings, _buf) = build_wired(TENANT_A);
    let err = bindings
        .d1
        .bind(&[TENANT_B, "2026-05-15", "spoof"])
        .expect_err("cross-tenant d1 bind must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("tenant_bind") || msg.contains("first positional"),
        "expected tenant_bind rejection, got: {msg}"
    );
    // The error must NOT be the WasmOnly diagnostic — that would mean
    // the CT-eq gate did not fire.
    assert!(
        !msg.contains("WasmOnly"),
        "CT-eq gate must fire before native stub: {msg}"
    );
}

#[tokio::test]
async fn cross_tenant_do_resolve_rejected_close() {
    // DO scoped name format is `tenant:<id>:<purpose>`. A name already
    // prefixed with TENANT_B is rejected by the scoped-name validator.
    let (bindings, _buf) = build_wired(TENANT_A);
    let foreign_name = format!("tenant:{TENANT_B}:health");
    let err = bindings
        .do_
        .stub_by_name(&foreign_name)
        .expect_err("cross-tenant do resolve must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("tenant") || msg.contains("Tenant"),
        "expected tenant-scoped rejection, got: {msg}"
    );
}

#[tokio::test]
async fn audit_sink_emits_canonical_ndjson_shape() {
    // Independent check that the recorder serialises a canonical
    // NDJSON line per event — the production console_log path uses the
    // same `to_ndjson()` serialiser, so this test pins the over-the-
    // wire shape on host CI.
    let (sink, buf) = AuditSink::recorder(TENANT_A);
    let r2 = sink.r2();
    r2(
        corelink_cf_bindings::R2Op::Head,
        "0123456789abcdef/health/probe",
    )
    .expect("audit ok");
    let captured = buf.lock().expect("lock");
    assert_eq!(captured.len(), 1);
    let line = captured[0].to_ndjson();
    assert!(line.starts_with("{\"surface\":\"r2\""));
    assert!(line.contains("\"op\":\"head\""));
    assert!(line.contains("\"tenant\":\"0123456789abcdef\""));
    assert!(line.contains("0123456789abcdef/health/probe"));
}
