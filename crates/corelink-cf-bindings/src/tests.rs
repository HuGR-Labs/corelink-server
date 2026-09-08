use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn tid(s: &str) -> TenantId {
    TenantId::new(s).expect("test tenant-id must be valid")
}

fn db(s: &str) -> CfD1DatabaseReal {
    CfD1DatabaseReal::stub_for_native_tests(tid(s))
}

/// Extract the backend message from a `D1Error`. Centralised here
/// because `D1Error` is `#[non_exhaustive]` (charter hard
/// requirement) — every test would otherwise need a `_ => panic!`
/// arm. Inside the crate `#[non_exhaustive]` is a no-op (the variant
/// is fully exhaustive at this site) so we use a direct match.
fn backend_msg(err: &D1Error) -> &str {
    let D1Error::Backend(msg) = err;
    msg.as_str()
}

// -----------------------------------------------------------------
// TenantId validation.
// -----------------------------------------------------------------

#[test]
fn tenant_id_rejects_empty() {
    let err = TenantId::new("").expect_err("empty tenant-id must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.starts_with("tenant_id:"), "got: {msg}");
}

#[test]
fn tenant_id_rejects_quote() {
    let err = TenantId::new("evil' OR 1=1--").expect_err("quote must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("forbidden character"), "got: {msg}");
}

#[test]
fn tenant_id_rejects_whitespace() {
    let err = TenantId::new("tnt 0").expect_err("whitespace must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("forbidden character"), "got: {msg}");
}

#[test]
fn tenant_id_rejects_over_cap() {
    let big = "a".repeat(129);
    let err = TenantId::new(big).expect_err("over-cap tenant-id must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("exceeds 128-byte cap"), "got: {msg}");
}

#[test]
fn tenant_id_ct_eq_equal() {
    let t = tid("tnt0123456789abcd");
    assert!(t.ct_eq_str("tnt0123456789abcd"));
}

#[test]
fn tenant_id_ct_eq_unequal_same_len() {
    let t = tid("tnt0123456789abcd");
    assert!(!t.ct_eq_str("tntFFFFFFFFFFFFFF"));
}

#[test]
fn tenant_id_ct_eq_length_mismatch() {
    let t = tid("tnt0123456789abcd");
    assert!(!t.ct_eq_str("tnt0123456789abc"));
    assert!(!t.ct_eq_str("tnt0123456789abcde"));
    assert!(!t.ct_eq_str(""));
}

// -----------------------------------------------------------------
// TenantScopedQuery validation.
// -----------------------------------------------------------------

#[test]
fn scoped_query_accepts_select_with_where_tenant_id() {
    let d = db("tnt");
    let q = d
        .scoped_query("SELECT id, body FROM blobs WHERE tenant_id = ? AND digest = ?")
        .expect("valid SELECT must be accepted");
    assert!(q.as_str().starts_with("SELECT"));
}

#[test]
fn scoped_query_rejects_select_without_tenant_scope() {
    let d = db("tnt");
    let err = d
        .scoped_query("SELECT * FROM blobs")
        .expect_err("SELECT without tenant scope must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_scope:"), "got: {msg}");
}

#[test]
fn scoped_query_accepts_update_with_where_tenant_id() {
    let d = db("tnt");
    d.scoped_query("UPDATE blobs SET refcount = refcount + 1 WHERE tenant_id = ? AND digest = ?")
        .expect("valid UPDATE must be accepted");
}

#[test]
fn scoped_query_accepts_delete_with_where_tenant_id() {
    let d = db("tnt");
    d.scoped_query("DELETE FROM blobs WHERE tenant_id = ? AND digest = ?")
        .expect("valid DELETE must be accepted");
}

#[test]
fn scoped_query_accepts_insert_listing_tenant_id() {
    let d = db("tnt");
    d.scoped_query(
        "INSERT INTO blobs (tenant_id, digest, size) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .expect("valid INSERT must be accepted");
}

#[test]
fn scoped_query_rejects_insert_missing_tenant_id() {
    let d = db("tnt");
    let err = d
        .scoped_query("INSERT INTO blobs (digest, size) VALUES (?, ?)")
        .expect_err("INSERT missing tenant_id must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_scope:"), "got: {msg}");
}

#[test]
fn scoped_query_rejects_empty() {
    let d = db("tnt");
    let err = d.scoped_query("").expect_err("empty SQL must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("tenant_scope: empty"), "got: {msg}");
}

#[test]
fn scoped_query_rejects_nul_byte() {
    let d = db("tnt");
    let err = d
        .scoped_query("SELECT * FROM x\0 WHERE tenant_id = ?")
        .expect_err("NUL byte must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("NUL"), "got: {msg}");
}

#[test]
fn scoped_query_rejects_unknown_verb() {
    let d = db("tnt");
    let err = d
        .scoped_query("DROP TABLE blobs")
        .expect_err("DROP must be rejected (not permitted)");
    let msg = backend_msg(&err);

    assert!(msg.contains("not permitted"), "got: {msg}");
}

#[test]
fn scoped_query_is_case_insensitive_on_keywords() {
    let d = db("tnt");
    d.scoped_query("select id from blobs where tenant_id = ?")
        .expect("lowercase keywords must be accepted");
    d.scoped_query("SeLeCt id FrOm blobs WhErE tenant_id = ?")
        .expect("mixed-case keywords must be accepted");
}

#[test]
fn scoped_query_tolerates_whitespace_around_eq() {
    let d = db("tnt");
    d.scoped_query("SELECT id FROM blobs WHERE tenant_id=?")
        .expect("no whitespace around `=` must be accepted");
    d.scoped_query("SELECT id FROM blobs WHERE tenant_id   =   ?")
        .expect("extra whitespace around `=` must be accepted");
}

// -----------------------------------------------------------------
// bind-time tenant-id constant-time verification.
// -----------------------------------------------------------------

#[tokio::test]
async fn bind_accepts_matching_first_param() {
    let d = db("tnt0123456789abcd");
    // Stub returns WasmOnly only AFTER first-param verification
    // + audit pass. A correct tenant-id should reach the WasmOnly
    // step.
    let err = d
        .bind(&["tnt0123456789abcd", "rest", "of", "args"])
        .expect_err("native stub must end at WasmOnly");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: bind");
}

#[tokio::test]
async fn bind_rejects_mismatched_first_param() {
    let d = db("tnt0123456789abcd");
    let err = d
        .bind(&["wrong-tenant", "rest"])
        .expect_err("mismatched tenant-id must be rejected");
    let msg = backend_msg(&err);
    assert!(
        msg.contains("tenant_bind:"),
        "tenant_bind: prefix must surface, got: {msg}"
    );
}

#[tokio::test]
async fn bind_rejects_empty_param_list() {
    let d = db("tnt");
    let err = d.bind(&[]).expect_err("empty param list must be rejected");
    let msg = backend_msg(&err);

    assert!(msg.contains("empty parameter list"), "got: {msg}");
}

// -----------------------------------------------------------------
// Native stub fail-CLOSED on each op.
// -----------------------------------------------------------------

#[test]
fn native_stub_returns_wasm_only_on_prepare() {
    let d = db("tnt");
    let q = d
        .scoped_query("SELECT id FROM blobs WHERE tenant_id = ?")
        .expect("valid query");
    let err = d.prepare(&q).expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: prepare");
}

#[tokio::test]
async fn native_stub_returns_wasm_only_on_first() {
    let d = db("tnt");
    let err = d.first().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: first");
}

#[tokio::test]
async fn native_stub_returns_wasm_only_on_all() {
    let d = db("tnt");
    let err = d.all().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: all");
}

#[tokio::test]
async fn native_stub_returns_wasm_only_on_run() {
    let d = db("tnt");
    let err = d.run().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: run");
}

// -----------------------------------------------------------------
// Audit hook fail-CLOSED + recording.
// -----------------------------------------------------------------

#[tokio::test]
async fn audit_hook_fail_closed_blocks_run() {
    let audit: AuditFn = Arc::new(|_op, _sql| {
        Err(D1Error::Backend(
            "audit: mutation denied by policy".to_owned(),
        ))
    });
    let d = db("tnt").with_audit(audit);
    let err = d.run().await.expect_err("audit deny must block");
    let msg = backend_msg(&err);
    assert!(
        msg.contains("audit: mutation denied by policy"),
        "fail-CLOSED must surface audit error, got: {msg}"
    );
}

#[tokio::test]
async fn audit_hook_fail_closed_blocks_prepare_validation_first() {
    // scoped_query failure surfaces BEFORE audit (tenant_scope:),
    // proving the validation order: scope first, audit second.
    let audit: AuditFn = Arc::new(|_op, _sql| Err(D1Error::Backend("audit: deny".to_owned())));
    let d = db("tnt").with_audit(audit);
    let err = d
        .scoped_query("SELECT * FROM blobs")
        .expect_err("scope failure must precede audit");
    let msg = backend_msg(&err);
    assert!(
        msg.contains("tenant_scope:"),
        "scope check must fire before audit, got: {msg}"
    );
}

#[test]
fn audit_hook_records_op_for_prepare() {
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&count);
    let captured_op: Arc<std::sync::Mutex<Option<D1Op>>> = Arc::new(std::sync::Mutex::new(None));
    let captured_clone = Arc::clone(&captured_op);
    let audit: AuditFn = Arc::new(move |op, _sql| {
        count_clone.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut g) = captured_clone.lock() {
            *g = Some(op);
        }
        Ok(())
    });
    let d = db("tnt").with_audit(audit);
    let q = d
        .scoped_query("SELECT id FROM blobs WHERE tenant_id = ?")
        .expect("valid query");
    let _ = d.prepare(&q);
    assert_eq!(count.load(Ordering::Acquire), 1);
    let op = captured_op
        .lock()
        .ok()
        .and_then(|g| *g)
        .expect("audit op must have been captured");
    assert_eq!(op, D1Op::Prepare);
}

// -----------------------------------------------------------------
// D1Op + D1Error stability.
// -----------------------------------------------------------------

#[test]
fn d1_op_display_is_stable() {
    assert_eq!(D1Op::Prepare.as_str(), "prepare");
    assert_eq!(D1Op::Bind.as_str(), "bind");
    assert_eq!(D1Op::First.as_str(), "first");
    assert_eq!(D1Op::All.as_str(), "all");
    assert_eq!(D1Op::Run.as_str(), "run");
}

#[test]
fn d1_error_display_prefixes_with_d1() {
    let e = D1Error::Backend("test message".to_owned());
    assert_eq!(format!("{e}"), "d1: test message");
}

// -----------------------------------------------------------------
// FakeD1 trait-bound test: simulate the wrapper layer end-to-end on
// native (no worker::* coupling). Verifies the tenant-prefix
// injection guard fires before any backend call would occur, and
// the bind-time first-param check is enforced.
// -----------------------------------------------------------------

/// Minimal fake exercising the wrapper layer + tenant-prefix
/// invariants without `worker::*`. Records every (op, payload) pair
/// that flows through the audit hook so tests can pin the ordering
/// invariants explicitly.
#[derive(Default)]
struct FakeD1 {
    log: std::sync::Mutex<Vec<(D1Op, String)>>,
}

impl FakeD1 {
    fn audit_fn(self: Arc<Self>) -> AuditFn {
        Arc::new(move |op, payload| {
            if let Ok(mut log) = self.log.lock() {
                log.push((op, payload.to_owned()));
            }
            Ok(())
        })
    }

    fn entries(&self) -> Vec<(D1Op, String)> {
        self.log.lock().ok().map(|g| g.clone()).unwrap_or_default()
    }
}

#[tokio::test]
async fn fake_d1_records_prepare_then_bind_audit_order() {
    let fake = Arc::new(FakeD1::default());
    let d = db("tnt0123456789abcd").with_audit(Arc::clone(&fake).audit_fn());
    let q = d
        .scoped_query("UPDATE blobs SET refcount = refcount + 1 WHERE tenant_id = ? AND digest = ?")
        .expect("valid scope");
    let _ = d.prepare(&q);
    let _ = d.bind(&["tnt0123456789abcd", "blake3:deadbeef"]);
    let entries = fake.entries();
    assert_eq!(entries.len(), 2, "exactly 2 audit emissions");
    assert_eq!(entries[0].0, D1Op::Prepare);
    assert!(entries[0].1.contains("UPDATE blobs"));
    assert_eq!(entries[1].0, D1Op::Bind);
    assert_eq!(entries[1].1, "(bind)");
}

#[tokio::test]
async fn fake_d1_run_emits_pre_audit_only_on_native_stub() {
    // On native, the post-mutation audit emission lives in the
    // wasm32 impl block (gated out here). The pre-mutation emission
    // (`run:pre`) fires before WasmOnly is returned.
    let fake = Arc::new(FakeD1::default());
    let d = db("tnt").with_audit(Arc::clone(&fake).audit_fn());
    let err = d.run().await.expect_err("native stub must refuse");
    let msg = backend_msg(&err);

    assert_eq!(msg, "WasmOnly: run");
    let entries = fake.entries();
    assert_eq!(entries.len(), 1, "only pre-mutation audit on native");
    assert_eq!(entries[0].0, D1Op::Run);
    assert_eq!(entries[0].1, "(run:pre)");
}
