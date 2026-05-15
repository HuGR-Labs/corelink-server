//! Integration tests for [`corelink_cf_bindings::do_real::CfDurableObjectReal`].
//!
//! Native-only — `corelink-cf-bindings` carries `target_arch = "wasm32"`
//! gates on the production `worker::*`-wrapping modules, but `do_real`
//! is dual-target (it ships a native stub + FakeDoRouter so the
//! validation+audit+fetch round-trip can be exercised on host CI). The
//! tests below exercise:
//!
//! - Tenant-scoped naming enforcement (canonical `tenant:<id>:<purpose>`
//!   shape, including constant-time tenant-id mismatch rejection).
//! - Stub-fetch round-trip through `FakeDoRouter` (the in-memory
//!   stand-in for `worker::Stub`).
//! - Audit-emit-BEFORE-success ordering (fail-CLOSED) — an audit
//!   closure returning `Err` must prevent the fetch from reaching the
//!   router.
//! - Native-stub `WasmOnly:` refusal on every method when no fake
//!   router is wired.
//! - Method discrimination on `fetch_with_request` (the routed url
//!   carries the HTTP method prefix so tests can assert on it).
//! - Hex-id validation (length, charset).
//!
//! Charter constraints reasserted at the test level:
//!
//! - `#![forbid(unsafe_code)]` inherited from crate root.
//! - `unwrap` / `expect` / `panic` allowed only inside `#[cfg(test)]`
//!   (this file).
//! - Audit fail-CLOSED is asserted directly.

#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics on assertion failure are the canonical signal"
)]

use corelink_cf_bindings::do_real::{
    AuditFn, CfDurableObjectReal, DoError, DoOp, DoTenantPrefix, FakeDoRouter,
    FakeFetchResponse, AUDIT_DENY_PREFIX, TENANT_SCOPE_PREFIX, WASM_ONLY_PREFIX,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn tp(s: &str) -> DoTenantPrefix {
    DoTenantPrefix::new(s).expect("test prefix must be valid")
}

/// Helper: extract the message from a `DoError`. `DoError` is
/// `#[non_exhaustive]` so an open `match` requires a wildcard; this
/// helper centralises the unwrap so the test bodies stay readable.
fn err_msg(err: DoError) -> String {
    match err {
        DoError::Backend(msg) => msg,
        // `#[non_exhaustive]` requires this arm. If a future variant
        // is added the tests will fail loudly so we can update them.
        _ => panic!("unexpected DoError variant"),
    }
}

// ---------------------------------------------------------------------------
// Tenant-scoped naming enforcement (5 tests)
// ---------------------------------------------------------------------------

#[test]
fn scoped_name_prepends_canonical_prefix_for_bare_purpose() {
    let wrapper =
        CfDurableObjectReal::stub_for_native_tests(tp("tnt0123456789abcd"));
    let scoped = wrapper
        .scoped_name("dedup-counter")
        .expect("bare purpose must be accepted and prefixed");
    assert_eq!(
        scoped.as_str(),
        "tenant:tnt0123456789abcd:dedup-counter"
    );
}

#[test]
fn scoped_name_accepts_already_canonical_form() {
    let wrapper =
        CfDurableObjectReal::stub_for_native_tests(tp("tntABCD"));
    let scoped = wrapper
        .scoped_name("tenant:tntABCD:rollout-controller")
        .expect("canonical name must be accepted verbatim");
    assert_eq!(scoped.as_str(), "tenant:tntABCD:rollout-controller");
}

#[test]
fn scoped_name_rejects_cross_tenant_ceremony() {
    // The caller used the ceremony but supplied a DIFFERENT tenant id.
    // The wrapper MUST refuse rather than silently route into the
    // wrong tenant's DO instance.
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tntAAAA"));
    let err = wrapper
        .scoped_name("tenant:tntBBBB:dedup-counter")
        .expect_err("cross-tenant name must be refused");
    let msg = err_msg(err);
            assert!(msg.starts_with(TENANT_SCOPE_PREFIX));
            assert!(msg.contains("tenant-id segment does not match"));
}

#[test]
fn scoped_name_rejects_empty_purpose_segment() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper
        .scoped_name("tenant:tnt:")
        .expect_err("empty purpose tail must be refused");
    let msg = err_msg(err);
        assert!(msg.contains("empty purpose segment"));
}

#[test]
fn scoped_name_rejects_missing_purpose_separator() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    // "tenant:tnt" has the prefix but no second colon → no purpose
    // segment at all. MUST be refused.
    let err = wrapper
        .scoped_name("tenant:tnt")
        .expect_err("missing purpose separator must be refused");
    let msg = err_msg(err);
        assert!(msg.contains("missing purpose segment"));
}

#[test]
fn scoped_name_rejects_empty_name() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper.scoped_name("").expect_err("empty name");
    let msg = err_msg(err);
        assert!(msg.contains("empty name"));
}

#[test]
fn scoped_name_rejects_nul_byte() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper
        .scoped_name("foo\0bar")
        .expect_err("NUL byte must be refused");
    let msg = err_msg(err);
        assert!(msg.contains("NUL"));
}

#[test]
fn scoped_name_rejects_whitespace_in_name() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper
        .scoped_name("foo bar")
        .expect_err("whitespace in name must be refused");
    let msg = err_msg(err);
        assert!(msg.contains("whitespace"));
}

// ---------------------------------------------------------------------------
// Stub-fetch round-trip through FakeDoRouter (4 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fetch_with_str_round_trips_through_fake_router() {
    let router = FakeDoRouter::on(
        "tenant:tnt:dedup-counter",
        "https://do/state",
        FakeFetchResponse::ok(b"OK".to_vec()),
    );
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tnt"), router);
    let resp = wrapper
        .fetch_with_str("dedup-counter", "https://do/state")
        .await
        .expect("happy-path fetch must succeed");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"OK");
}

#[tokio::test]
async fn fetch_with_request_routes_method_discriminated() {
    let router = FakeDoRouter::new(Arc::new(|name, routed_url| {
        // The wrapper encodes "{method} {url}" into the routed url so
        // fakes can assert on the method.
        if name == "tenant:tnt:counter" && routed_url == "POST https://do/inc" {
            Ok(FakeFetchResponse::ok(b"incremented".to_vec()))
        } else {
            Ok(FakeFetchResponse::empty(404))
        }
    }));
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tnt"), router);
    let resp = wrapper
        .fetch_with_request("counter", "POST", "https://do/inc", b"")
        .await
        .expect("happy-path method-routed fetch must succeed");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"incremented");
}

#[tokio::test]
async fn fetch_routes_through_router_using_canonical_scoped_name() {
    // Even when the caller passes the bare purpose, the router
    // receives the canonical `tenant:<id>:<purpose>` form.
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let router = FakeDoRouter::new(Arc::new(move |name, _url| {
        if let Ok(mut g) = captured_clone.lock() {
            *g = Some(name.to_owned());
        }
        Ok(FakeFetchResponse::empty(204))
    }));
    let wrapper =
        CfDurableObjectReal::with_fake_router(tp("tnt0123456789abcd"), router);
    let _ = wrapper
        .fetch_with_str("dedup-counter", "https://do/x")
        .await
        .expect("fetch must succeed");
    let captured_name = captured
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .expect("router must have received the name");
    assert_eq!(
        captured_name,
        "tenant:tnt0123456789abcd:dedup-counter"
    );
}

#[tokio::test]
async fn fetch_surfaces_router_error_with_do_fetch_prefix() {
    let router = FakeDoRouter::always_fail("upstream unreachable");
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tnt"), router);
    let err = wrapper
        .fetch_with_str("counter", "https://do/x")
        .await
        .expect_err("router failure must surface as DoError");
    let msg = err_msg(err);
            assert!(
                msg.contains("do_fetch:"),
                "binding-fault must carry do_fetch: prefix; got: {msg}"
            );
            assert!(msg.contains("upstream unreachable"));
}

// ---------------------------------------------------------------------------
// Audit-emit-before-success ordering + fail-CLOSED (4 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_hook_fires_exactly_once_before_router_dispatch() {
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&count);
    let audit: AuditFn = Arc::new(move |_op, _name| {
        count_clone.fetch_add(1, Ordering::AcqRel);
        Ok(())
    });
    let router = FakeDoRouter::constant(FakeFetchResponse::ok(b"hi".to_vec()));
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tnt"), router)
        .with_audit(audit);
    let _ = wrapper
        .fetch_with_str("counter", "https://do/x")
        .await
        .expect("fetch must succeed");
    assert_eq!(count.load(Ordering::Acquire), 1, "audit must fire exactly once");
}

#[tokio::test]
async fn audit_hook_fail_closed_blocks_fetch_dispatch() {
    // The router would have returned 200, but the audit closure
    // denies. The wrapper MUST NOT reach the router; we detect this
    // by an atomic counter on dispatch.
    let dispatched = Arc::new(AtomicUsize::new(0));
    let dispatched_clone = Arc::clone(&dispatched);
    let router = FakeDoRouter::new(Arc::new(move |_name, _url| {
        dispatched_clone.fetch_add(1, Ordering::AcqRel);
        Ok(FakeFetchResponse::ok(b"WRONG".to_vec()))
    }));
    let audit: AuditFn = Arc::new(|_op, _name| {
        Err(DoError::Backend(format!(
            "{AUDIT_DENY_PREFIX}write denied by policy"
        )))
    });
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tnt"), router)
        .with_audit(audit);
    let err = wrapper
        .fetch_with_str("counter", "https://do/x")
        .await
        .expect_err("audit deny must block fetch");
    let msg = err_msg(err);
            assert!(msg.contains("write denied by policy"), "got: {msg}");
            assert!(msg.contains(AUDIT_DENY_PREFIX), "got: {msg}");
    assert_eq!(
        dispatched.load(Ordering::Acquire),
        0,
        "router MUST NOT be reached when audit denies"
    );
}

#[tokio::test]
async fn audit_hook_receives_canonical_scoped_name_not_raw() {
    // The audit hook MUST see the scoped name (post-derivation), not
    // the raw caller input. This is the incident-response anchor.
    let captured: Arc<Mutex<Option<(DoOp, String)>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let audit: AuditFn = Arc::new(move |op, name| {
        if let Ok(mut g) = captured_clone.lock() {
            *g = Some((op, name.to_owned()));
        }
        Ok(())
    });
    let router = FakeDoRouter::constant(FakeFetchResponse::empty(204));
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tntABCD"), router)
        .with_audit(audit);
    let _ = wrapper
        .fetch_with_str("rollout-fsm", "https://do/state")
        .await
        .expect("fetch must succeed");
    let cap = captured.lock().ok().and_then(|g| g.clone()).expect("audit captured");
    assert_eq!(cap.0, DoOp::FetchStr);
    assert_eq!(cap.1, "tenant:tntABCD:rollout-fsm");
}

#[tokio::test]
async fn audit_fires_for_fetch_with_request_with_correct_op_label() {
    let captured: Arc<Mutex<Option<DoOp>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let audit: AuditFn = Arc::new(move |op, _name| {
        if let Ok(mut g) = captured_clone.lock() {
            *g = Some(op);
        }
        Ok(())
    });
    let router = FakeDoRouter::constant(FakeFetchResponse::empty(204));
    let wrapper = CfDurableObjectReal::with_fake_router(tp("tnt"), router)
        .with_audit(audit);
    let _ = wrapper
        .fetch_with_request("counter", "POST", "https://do/x", b"")
        .await
        .expect("fetch must succeed");
    let op = captured.lock().ok().and_then(|g| *g).expect("op captured");
    assert_eq!(op, DoOp::FetchRequest);
}

// ---------------------------------------------------------------------------
// Native-stub fail-CLOSED on each method (no fake router) (5 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn native_stub_returns_wasm_only_on_fetch_with_str() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper
        .fetch_with_str("counter", "https://do/x")
        .await
        .expect_err("native stub must refuse");
    let msg = err_msg(err);
            assert!(msg.contains(WASM_ONLY_PREFIX), "got: {msg}");
            assert!(msg.contains("fetch_with_str"), "got: {msg}");
}

#[tokio::test]
async fn native_stub_returns_wasm_only_on_fetch_with_request() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper
        .fetch_with_request("counter", "POST", "https://do/x", b"")
        .await
        .expect_err("native stub must refuse");
    let msg = err_msg(err);
            assert!(msg.contains(WASM_ONLY_PREFIX));
            assert!(msg.contains("fetch_with_request"));
}

#[test]
fn native_stub_returns_wasm_only_on_stub_by_name() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper.stub_by_name("counter").expect_err("native stub must refuse");
    let msg = err_msg(err);
            assert!(msg.contains(WASM_ONLY_PREFIX));
            assert!(msg.contains("resolve"));
}

#[test]
fn native_stub_returns_wasm_only_on_stub_by_hex_id_after_validation() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let hex = "0".repeat(64);
    let err = wrapper.stub_by_hex_id(&hex).expect_err("native stub must refuse");
    let msg = err_msg(err);
            assert!(msg.contains(WASM_ONLY_PREFIX));
            assert!(msg.contains("resolve"));
}

#[test]
fn stub_by_hex_id_rejects_wrong_length_before_audit() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    let err = wrapper.stub_by_hex_id("abcd").expect_err("short hex must be refused");
    let msg = err_msg(err);
            assert!(msg.contains(TENANT_SCOPE_PREFIX));
            assert!(msg.contains("64 chars"));
}

#[test]
fn stub_by_hex_id_rejects_non_hex_charset() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"));
    // 64 chars but `z` is not hex.
    let bad = format!("z{}", "0".repeat(63));
    let err = wrapper.stub_by_hex_id(&bad).expect_err("non-hex must be refused");
    let msg = err_msg(err);
            assert!(msg.contains(TENANT_SCOPE_PREFIX));
            assert!(msg.contains("ascii-hex"));
}

// ---------------------------------------------------------------------------
// Validation runs BEFORE WasmOnly + audit gating (2 tests — pin the
// order so refactors don't silently leak validation faults into the
// "WasmOnly" channel).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validation_fires_before_wasm_only_on_fetch() {
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tntAAAA"));
    let err = wrapper
        .fetch_with_str("tenant:tntBBBB:counter", "https://do/x")
        .await
        .expect_err("cross-tenant name must be refused pre-stub");
    let msg = err_msg(err);
            // Must be the tenant_scope error, NOT WasmOnly.
            assert!(
                msg.contains(TENANT_SCOPE_PREFIX),
                "validation must fire before WasmOnly: got: {msg}"
            );
            assert!(!msg.contains(WASM_ONLY_PREFIX));
}

#[tokio::test]
async fn audit_fires_before_wasm_only_on_fetch_with_no_router() {
    // Audit closure denies. Without a fake router, the native stub
    // WOULD return WasmOnly, but the audit deny MUST surface first
    // (fail-CLOSED ordering).
    let audit: AuditFn = Arc::new(|_op, _name| {
        Err(DoError::Backend(format!(
            "{AUDIT_DENY_PREFIX}denied by policy"
        )))
    });
    let wrapper = CfDurableObjectReal::stub_for_native_tests(tp("tnt"))
        .with_audit(audit);
    let err = wrapper
        .fetch_with_str("counter", "https://do/x")
        .await
        .expect_err("audit deny must surface");
    let msg = err_msg(err);
            assert!(msg.contains(AUDIT_DENY_PREFIX), "got: {msg}");
            assert!(!msg.contains(WASM_ONLY_PREFIX), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Smoke test on the surfaced public types (1 test — pin the re-exports
// at the crate root so a rename surfaces as a test failure).
// ---------------------------------------------------------------------------

#[test]
fn crate_reexports_do_real_public_types() {
    // Compile-time assertions: if any of these renames, this test
    // fails to compile.
    let _: corelink_cf_bindings::DoError =
        corelink_cf_bindings::DoError::Backend("x".to_owned());
    let _: corelink_cf_bindings::DoOp = corelink_cf_bindings::DoOp::FetchStr;
    let _: corelink_cf_bindings::DoTenantPrefix =
        corelink_cf_bindings::DoTenantPrefix::new("tnt").expect("valid");
    let _ = corelink_cf_bindings::CfDurableObjectReal::stub_for_native_tests(
        corelink_cf_bindings::DoTenantPrefix::new("tnt").expect("valid"),
    );
}
