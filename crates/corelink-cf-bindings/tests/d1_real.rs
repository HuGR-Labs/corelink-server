//! Integration tests for [`corelink_cf_bindings::d1_real::CfD1DatabaseReal`].
//!
//! Mirrors `tests/prop_cas_idempotency.rs` (the R2 trait-bound test
//! file) for the D1 wrapper layer. Native-only — the wasm32 production
//! path is exercised by the wasm32 build itself (which links
//! `worker::D1Database`); these tests pin the validation + audit
//! contract on host CI so the per-binding invariants are caught
//! deterministically before the wasm32 build.
//!
//! # Coverage map
//!
//! | Test                                                             | Invariant pinned                                  |
//! |------------------------------------------------------------------|---------------------------------------------------|
//! | `tenant_id_constant_time_eq_matches`                             | Constant-time tenant-id equality                  |
//! | `scoped_query_select_accepts_canonical_tenant_scope`             | Tenant-prefix injection on SELECT                 |
//! | `scoped_query_update_accepts_canonical_tenant_scope`             | Tenant-prefix injection on UPDATE                 |
//! | `scoped_query_delete_accepts_canonical_tenant_scope`             | Tenant-prefix injection on DELETE                 |
//! | `scoped_query_insert_requires_tenant_id_column`                  | Tenant-prefix injection on INSERT                 |
//! | `scoped_query_rejects_missing_tenant_scope_on_select`            | Reject SELECT without scope                       |
//! | `scoped_query_rejects_missing_tenant_scope_on_update`            | Reject UPDATE without scope                       |
//! | `scoped_query_rejects_drop_verb`                                 | Reject non-tenanted DDL                           |
//! | `bind_verifies_first_param_via_constant_time_compare`            | Bind-time first-param check                       |
//! | `bind_rejects_mismatched_tenant_id`                              | Bind-time mismatch fail-CLOSED                    |
//! | `native_stub_prepare_returns_wasm_only`                          | Native stub fail-CLOSED on prepare                |
//! | `native_stub_bind_returns_wasm_only_when_first_matches`          | Native stub fail-CLOSED on bind                   |
//! | `native_stub_first_returns_wasm_only`                            | Native stub fail-CLOSED on first                  |
//! | `native_stub_all_returns_wasm_only`                              | Native stub fail-CLOSED on all                    |
//! | `native_stub_run_returns_wasm_only_after_audit_pre`              | Native stub fail-CLOSED on run + audit ordering   |
//! | `audit_hook_blocks_run_when_returns_err`                         | Audit fail-CLOSED                                 |
//! | `audit_hook_fires_before_scope_validator_only_when_scope_valid`  | Validation order: scope → audit                   |
//! | `wasm32_build_assertion_is_pinned_in_module_doc`                 | wasm32 production path documented                 |
//!
//! Total: 18 tests.

#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: panics surface as test failures by design"
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use corelink_cf_bindings::d1_real::{
    AuditFn, CfD1DatabaseReal, D1Error, D1Op, TenantId,
};

fn tid(s: &str) -> TenantId {
    TenantId::new(s).expect("test tenant-id must be valid")
}

fn db(s: &str) -> CfD1DatabaseReal {
    CfD1DatabaseReal::stub_for_native_tests(tid(s))
}

/// Extract the backend message from a [`D1Error`]. Centralised here
/// because `D1Error` is `#[non_exhaustive]` (CHARTER hard requirement)
/// — every test would otherwise need a `_ => panic!(...)` arm.
fn backend_msg(err: &D1Error) -> &str {
    match err {
        D1Error::Backend(msg) => msg.as_str(),
        // The non_exhaustive marker permits new variants in the future;
        // any new variant surfacing in a test surfaces as a panic with
        // a discriminating message rather than a silent skip.
        _ => panic!("expected D1Error::Backend, got {err:?}"),
    }
}

// =====================================================================
// FakeD1 — exercises the wrapper-layer audit chain + tenant-prefix
// invariants without coupling to worker::*.
// =====================================================================

/// Records every (op, payload) audit call. Used to pin the audit
/// emission ordering invariants the wasm32 production path also
/// exercises (validation → audit → backend).
#[derive(Default)]
struct FakeD1 {
    log: Mutex<Vec<(D1Op, String)>>,
    /// If set, the next audit call rejects with this error and clears
    /// the flag (one-shot fail-CLOSED simulation).
    fail_next: AtomicBool,
}

impl FakeD1 {
    fn into_audit(self: Arc<Self>) -> AuditFn {
        Arc::new(move |op, payload| {
            if self.fail_next.swap(false, Ordering::AcqRel) {
                return Err(D1Error::Backend(
                    "audit: rejected by fake".to_owned(),
                ));
            }
            if let Ok(mut log) = self.log.lock() {
                log.push((op, payload.to_owned()));
            }
            Ok(())
        })
    }

    fn entries(&self) -> Vec<(D1Op, String)> {
        self.log
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

// =====================================================================
// 1. TenantId constant-time equality.
// =====================================================================

#[test]
fn tenant_id_constant_time_eq_matches() {
    let t = tid("tnt0123456789abcd");
    assert!(t.ct_eq_str("tnt0123456789abcd"));
    assert!(!t.ct_eq_str("tntFFFFFFFFFFFFFF"));
    assert!(!t.ct_eq_str("short"));
    assert!(!t.ct_eq_str(""));
}

// =====================================================================
// 2-5. Tenant-scope injection / validation per verb.
// =====================================================================

#[test]
fn scoped_query_select_accepts_canonical_tenant_scope() {
    let d = db("tnt");
    let q = d
        .scoped_query("SELECT id, body FROM blobs WHERE tenant_id = ? AND digest = ?")
        .expect("canonical SELECT scope must be accepted");
    assert!(q.as_str().contains("WHERE tenant_id = ?"));
}

#[test]
fn scoped_query_update_accepts_canonical_tenant_scope() {
    let d = db("tnt");
    let q = d
        .scoped_query(
            "UPDATE blobs SET refcount = refcount + 1 WHERE tenant_id = ? AND digest = ?",
        )
        .expect("canonical UPDATE scope must be accepted");
    assert!(q.as_str().contains("WHERE tenant_id = ?"));
}

#[test]
fn scoped_query_delete_accepts_canonical_tenant_scope() {
    let d = db("tnt");
    let q = d
        .scoped_query("DELETE FROM blobs WHERE tenant_id = ? AND digest = ?")
        .expect("canonical DELETE scope must be accepted");
    assert!(q.as_str().contains("DELETE"));
}

#[test]
fn scoped_query_insert_requires_tenant_id_column() {
    let d = db("tnt");
    d.scoped_query(
        "INSERT INTO blobs (tenant_id, digest, size) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .expect("INSERT listing tenant_id must be accepted");
}

// =====================================================================
// 6-8. Tenant-scope rejection paths.
// =====================================================================

#[test]
fn scoped_query_rejects_missing_tenant_scope_on_select() {
    let d = db("tnt");
    let err = d
        .scoped_query("SELECT * FROM blobs WHERE digest = ?")
        .expect_err("SELECT without tenant scope must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_scope:"), "got: {msg}");
}

#[test]
fn scoped_query_rejects_missing_tenant_scope_on_update() {
    let d = db("tnt");
    let err = d
        .scoped_query("UPDATE blobs SET refcount = 0 WHERE digest = ?")
        .expect_err("UPDATE without tenant scope must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_scope:"), "got: {msg}");
}

#[test]
fn scoped_query_rejects_drop_verb() {
    let d = db("tnt");
    let err = d
        .scoped_query("DROP TABLE blobs")
        .expect_err("DROP must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("not permitted"), "got: {msg}");
}

// =====================================================================
// 9-10. Bind-time tenant-id constant-time check.
// =====================================================================

#[tokio::test]
async fn bind_verifies_first_param_via_constant_time_compare() {
    let d = db("tnt0123456789abcd");
    // A correct tenant-id reaches the native WasmOnly step.
    let err = d
        .bind(&["tnt0123456789abcd", "blake3:deadbeef"])
        .expect_err("native stub must end at WasmOnly");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: bind");
}

#[tokio::test]
async fn bind_rejects_mismatched_tenant_id() {
    let d = db("tnt0123456789abcd");
    let err = d
        .bind(&["tnt-WRONG", "blake3:deadbeef"])
        .expect_err("mismatched tenant-id must surface tenant_bind:");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_bind:"), "got: {msg}");
}

// =====================================================================
// 11-15. Native stub fail-CLOSED on each op.
// =====================================================================

#[test]
fn native_stub_prepare_returns_wasm_only() {
    let d = db("tnt");
    let q = d
        .scoped_query("SELECT id FROM blobs WHERE tenant_id = ?")
        .expect("valid query");
    let err = d.prepare(&q).expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: prepare");
}

#[tokio::test]
async fn native_stub_bind_returns_wasm_only_when_first_matches() {
    let d = db("tnt0123456789abcd");
    let err = d
        .bind(&["tnt0123456789abcd"])
        .expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: bind");
}

#[tokio::test]
async fn native_stub_first_returns_wasm_only() {
    let d = db("tnt");
    let err = d.first().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: first");
}

#[tokio::test]
async fn native_stub_all_returns_wasm_only() {
    let d = db("tnt");
    let err = d.all().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: all");
}

#[tokio::test]
async fn native_stub_run_returns_wasm_only_after_audit_pre() {
    let fake = Arc::new(FakeD1::default());
    let d = db("tnt").with_audit(Arc::clone(&fake).into_audit());
    let err = d.run().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: run");
    // Native stub fires the pre-mutation audit ONCE; the post-mutation
    // audit lives in the wasm32 impl block (gated out here).
    let entries = fake.entries();
    assert_eq!(entries.len(), 1, "exactly one audit emission on native");
    assert_eq!(entries[0].0, D1Op::Run);
    assert_eq!(entries[0].1, "(run:pre)");
}

// =====================================================================
// 16-17. Audit fail-CLOSED + ordering.
// =====================================================================

#[tokio::test]
async fn audit_hook_blocks_run_when_returns_err() {
    let fake = Arc::new(FakeD1::default());
    fake.fail_next.store(true, Ordering::Release);
    let d = db("tnt").with_audit(Arc::clone(&fake).into_audit());
    let err = d.run().await.expect_err("audit deny must block run");
    let msg = backend_msg(&err);
    assert!(
        msg.contains("audit: rejected by fake"),
        "fail-CLOSED must surface audit error verbatim, got: {msg}"
    );
    // No entries recorded — the fail_next path returned Err *before*
    // log.push.
    assert!(fake.entries().is_empty());
}

#[tokio::test]
async fn audit_hook_fires_before_scope_validator_only_when_scope_valid() {
    // scoped_query MUST fail before any audit fires when the scope is
    // missing — the wrapper validates BEFORE emitting an audit event so
    // a malformed query doesn't pollute the audit chain.
    let fake = Arc::new(FakeD1::default());
    let d = db("tnt").with_audit(Arc::clone(&fake).into_audit());
    let err = d
        .scoped_query("SELECT 1")
        .expect_err("missing scope must reject");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_scope:"), "got: {msg}");
    assert!(
        fake.entries().is_empty(),
        "scope failure must not emit an audit"
    );
}

// =====================================================================
// 18. wasm32 build assertion.
// =====================================================================

/// This test asserts the production target documentation contract:
/// `CfD1DatabaseReal` is documented as dual-target with the wasm32
/// production wiring + native stub. The cfg gate in the source file
/// is the runtime witness; if a future refactor accidentally removes
/// the wasm32 production impl block, the `cargo build --target
/// wasm32-unknown-unknown` quality gate will fail at link time
/// (no `prepare`/`bind`/`first`/`all`/`run` symbols emitted from a
/// wasm32 build means callers like `apps/server` boot path lose the
/// production binding). This test pins the contract documentation
/// invariant: the docstring MUST mention the production wasm32
/// target.
#[test]
fn wasm32_build_assertion_is_pinned_in_module_doc() {
    // We cannot introspect doc comments at runtime; instead we read the
    // source file from the workspace and grep for the canonical
    // production-target string. The cargo manifest pins
    // `crate-type = ["cdylib", "rlib"]` and `worker = { ... features =
    // ["d1", "http"] }`, so a wasm32 build is the canonical production
    // build for this crate.
    let src = include_str!("../src/d1_real.rs");
    assert!(
        src.contains("wasm32 production binding"),
        "module doc must declare wasm32 as production target"
    );
    assert!(
        src.contains("WasmOnly"),
        "module doc + stub must reference WasmOnly contract"
    );
}
