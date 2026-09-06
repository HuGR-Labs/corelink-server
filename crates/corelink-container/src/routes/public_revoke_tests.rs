#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use super::*;
use axum::body::to_bytes;
use axum::http::HeaderValue;
use std::collections::HashMap;
use tokio::sync::Mutex;

const KEY: &str = "test-erase-key-0000000000000000000000";
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// An UPSTREAM OCI digest an operator holds during an incident (64-hex, but a
/// DIFFERENT value from any CoreLink blake3 content_hash — this is the bug).
const UPSTREAM_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";

#[derive(Debug, Default)]
struct FakeStoreInner {
    blocklist: HashMap<String, String>, // content_hash -> audit_event_id
    map_deletes: Vec<String>,
    // `_public` adapter_cache_map: OCI digest wire string -> blake3 content_hash.
    resolve: HashMap<String, String>,
}

#[derive(Debug, Default)]
struct FakeStore {
    inner: Mutex<FakeStoreInner>,
}

#[async_trait]
impl PublicRevocationStore for FakeStore {
    async fn insert_blocklist(
        &self,
        content_hash: &str,
        _revoked_at_ms: i64,
        _reason: &str,
        _approver: &str,
        audit_event_id: &str,
    ) -> Result<BlocklistInsertOutcome, String> {
        let mut g = self.inner.lock().await;
        if let Some(existing) = g.blocklist.get(content_hash) {
            return Ok(BlocklistInsertOutcome::AlreadyRevoked {
                existing_audit_event_id: existing.clone(),
            });
        }
        g.blocklist
            .insert(content_hash.to_owned(), audit_event_id.to_owned());
        Ok(BlocklistInsertOutcome::Inserted)
    }

    async fn delete_cache_map(&self, content_hash: &str) -> Result<(), String> {
        self.inner
            .lock()
            .await
            .map_deletes
            .push(content_hash.to_owned());
        Ok(())
    }

    async fn resolve_public_digest(&self, oci_digest_wire: &str) -> Result<Option<String>, String> {
        Ok(self
            .inner
            .lock()
            .await
            .resolve
            .get(oci_digest_wire)
            .cloned())
    }
}

#[derive(Debug, Default)]
struct FakeAudit {
    events: Mutex<Vec<PublicRevocationAuditEvent>>,
}

#[async_trait]
impl PublicRevocationAuditSink for FakeAudit {
    async fn emit_revocation(&self, event: &PublicRevocationAuditEvent) -> Result<(), String> {
        self.events.lock().await.push(event.clone());
        Ok(())
    }
}

#[derive(Debug, Default)]
struct FakeEraser {
    calls: Mutex<Vec<(String, String)>>,
    fail: bool,
}

#[async_trait]
impl CasBlobEraser for FakeEraser {
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String> {
        if self.fail {
            return Err(format!("r2 down for {tenant}"));
        }
        self.calls
            .lock()
            .await
            .push((tenant.to_owned(), digest.to_owned()));
        Ok(())
    }
}

fn state_with(
    store: Arc<FakeStore>,
    audit: Arc<FakeAudit>,
    eraser: Arc<FakeEraser>,
) -> PublicRevokeRouteState {
    PublicRevokeRouteState {
        store,
        eraser,
        audit,
        erase_auth_key: Arc::from(KEY),
    }
}

fn auth_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(INTERNAL_AUTH_HEADER, HeaderValue::from_static(KEY));
    h
}

/// A revoke request in the raw-BLAKE3 `content_hash` space.
fn body(hash: &str) -> PublicRevokeRequest {
    PublicRevokeRequest {
        content_hash: Some(hash.to_owned()),
        upstream_digest: None,
        reason: "malware".to_owned(),
        approver: Some("sec-oncall".to_owned()),
    }
}

/// A revoke request in the upstream OCI `sha256:` digest space.
fn upstream_body(digest: &str) -> PublicRevokeRequest {
    PublicRevokeRequest {
        content_hash: None,
        upstream_digest: Some(digest.to_owned()),
        reason: "poisoned base layer".to_owned(),
        approver: Some("sec-oncall".to_owned()),
    }
}

async fn parse(resp: Response) -> PublicRevokeResponse {
    let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&b).unwrap()
}

#[tokio::test]
async fn happy_path_revokes_deletes_map_audits_and_erases_once() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());
    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        auth_headers(),
        Json(body(HASH)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let out = parse(resp).await;
    assert!(out.revoked);
    assert!(!out.already_revoked);
    assert!(out.r2_delete_failures.is_empty());
    // F-1: the response carries the resolved content_hash so the Worker seam
    // can mark the brew/pip edge from the RESPONSE (raw space: verbatim).
    assert_eq!(out.content_hash, HASH);

    assert_eq!(store.inner.lock().await.blocklist.len(), 1);
    assert_eq!(store.inner.lock().await.map_deletes, vec![HASH.to_owned()]);
    assert_eq!(audit.events.lock().await.len(), 1);
    // Erase hits the `_public` prefix ONCE (not per-principal).
    let calls = eraser.calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], (PUBLIC_NAMESPACE.to_owned(), HASH.to_owned()));
}

#[tokio::test]
async fn re_revoke_is_idempotent_reuses_audit_id() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());
    let st = state_with(store.clone(), audit.clone(), eraser.clone());

    let _ = handle_revoke(State(st.clone()), auth_headers(), Json(body(HASH))).await;
    let resp = handle_revoke(State(st), auth_headers(), Json(body(HASH))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let out = parse(resp).await;
    assert!(out.revoked);
    assert!(out.already_revoked);

    assert_eq!(store.inner.lock().await.blocklist.len(), 1);
    let events = audit.events.lock().await;
    assert_eq!(
        events.len(),
        2,
        "both re-revokes emit (idempotent by id at D1)"
    );
    assert_eq!(
        events[0].audit_event_id, events[1].audit_event_id,
        "the re-revoke reuses the ORIGINAL audit id"
    );
}

#[tokio::test]
async fn bad_hash_is_400_and_touches_nothing() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());
    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        auth_headers(),
        Json(body("not-hex")),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(store.inner.lock().await.blocklist.is_empty());
    assert!(audit.events.lock().await.is_empty());
    assert!(eraser.calls.lock().await.is_empty());
}

#[tokio::test]
async fn wrong_auth_is_401_and_touches_nothing() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());
    let mut h = HeaderMap::new();
    h.insert(INTERNAL_AUTH_HEADER, HeaderValue::from_static("wrong"));
    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        h,
        Json(body(HASH)),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(store.inner.lock().await.blocklist.is_empty());
    assert!(audit.events.lock().await.is_empty());
    assert!(eraser.calls.lock().await.is_empty());
}

#[tokio::test]
async fn r2_erase_failure_is_reported_not_fatal() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser {
        calls: Mutex::new(Vec::new()),
        fail: true,
    });
    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser)),
        auth_headers(),
        Json(body(HASH)),
    )
    .await;
    // Blocklist row is durable → revoked=true even though R2 failed.
    assert_eq!(resp.status(), StatusCode::OK);
    let out = parse(resp).await;
    assert!(out.revoked);
    assert_eq!(out.r2_delete_failures.len(), 1);
    assert_eq!(store.inner.lock().await.blocklist.len(), 1);
    assert_eq!(audit.events.lock().await.len(), 1);
}

#[test]
fn is_64_hex_guards() {
    assert!(is_64_hex(HASH));
    assert!(!is_64_hex("abc"));
    assert!(!is_64_hex(&"g".repeat(64)));
    assert!(!is_64_hex(&"a".repeat(63)));
}

// ── WP-F: revoke-by-sha256 upstream digest ──────────────────────────────

/// DoD: revoke by an operator-held `sha256:` digest that maps to a LIVE
/// `_public` blob → the blob's BLAKE3 content_hash is blocklisted + erased
/// (serving is killed). The upstream digest and the blake3 differ; the map
/// resolves the indirection.
#[tokio::test]
async fn revoke_by_upstream_sha256_that_maps_kills_serving() {
    let store = Arc::new(FakeStore::default());
    // Seed the `_public` adapter_cache_map: upstream OCI digest → blake3.
    store
        .inner
        .lock()
        .await
        .resolve
        .insert(UPSTREAM_DIGEST.to_owned(), HASH.to_owned());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());

    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        auth_headers(),
        Json(upstream_body(UPSTREAM_DIGEST)),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    let out = parse(resp).await;
    assert!(out.revoked);
    assert!(!out.already_revoked);
    // F-1: the response returns the RESOLVED blake3 content_hash (NOT the
    // sha256 upstream digest) — this is what lets the Worker seam mark the
    // brew/pip edge `pubblock:<hash>` for the upstream_digest incident path.
    assert_eq!(out.content_hash, HASH);
    assert_ne!(out.content_hash, UPSTREAM_DIGEST);
    // The BLAKE3 content_hash (NOT the sha256 digest) is what got blocklisted.
    let g = store.inner.lock().await;
    assert!(g.blocklist.contains_key(HASH));
    assert!(!g.blocklist.contains_key(UPSTREAM_DIGEST));
    assert_eq!(g.map_deletes, vec![HASH.to_owned()]);
    drop(g);
    let calls = eraser.calls.lock().await;
    assert_eq!(calls[0], (PUBLIC_NAMESPACE.to_owned(), HASH.to_owned()));
}

/// DoD: a `sha256:` digest that resolves to NOTHING → EXPLICIT error (assert
/// NOT 200) and touches nothing. This is the silent-audited-no-op the WP
/// exists to kill: the operator's incident action MUST surface, not succeed.
#[tokio::test]
async fn revoke_by_upstream_sha256_resolving_to_nothing_is_loud_error() {
    let store = Arc::new(FakeStore::default()); // empty resolve map
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());

    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        auth_headers(),
        Json(upstream_body(UPSTREAM_DIGEST)),
    )
    .await;

    assert_ne!(resp.status(), StatusCode::OK, "must NOT be a false success");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(store.inner.lock().await.blocklist.is_empty());
    assert!(store.inner.lock().await.map_deletes.is_empty());
    assert!(audit.events.lock().await.is_empty());
    assert!(eraser.calls.lock().await.is_empty());
}

/// DoD: a pre-emptive block by a raw BLAKE3 content_hash with no currently
/// active row → still 200 idempotent (a legitimate pre-emptive block).
#[tokio::test]
async fn preemptive_raw_blake3_block_with_no_active_row_is_200() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());

    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        auth_headers(),
        Json(body(HASH)),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    let out = parse(resp).await;
    assert!(out.revoked);
    assert!(!out.already_revoked);
    assert!(store.inner.lock().await.blocklist.contains_key(HASH));
}

/// DoD: a bare 64-hex supplied in the upstream space (no `sha256:` prefix,
/// but INTENDED as an upstream digest) → refused as ambiguous, touches
/// nothing. It could be a blake3 or a stripped sha256 — the operator must
/// name the space.
#[tokio::test]
async fn bare_64hex_as_upstream_digest_is_rejected_ambiguous() {
    let store = Arc::new(FakeStore::default());
    let audit = Arc::new(FakeAudit::default());
    let eraser = Arc::new(FakeEraser::default());

    let resp = handle_revoke(
        State(state_with(store.clone(), audit.clone(), eraser.clone())),
        auth_headers(),
        Json(upstream_body(HASH)), // HASH is a bare 64-hex, no `sha256:`
    )
    .await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(store.inner.lock().await.blocklist.is_empty());
    assert!(audit.events.lock().await.is_empty());
    assert!(eraser.calls.lock().await.is_empty());
}

/// Guard: neither field, or both fields, is a 400 (exactly-one contract).
#[tokio::test]
async fn neither_or_both_fields_is_400() {
    let mk = || {
        (
            Arc::new(FakeStore::default()),
            Arc::new(FakeAudit::default()),
            Arc::new(FakeEraser::default()),
        )
    };

    let (s, a, e) = mk();
    let neither = PublicRevokeRequest {
        content_hash: None,
        upstream_digest: None,
        reason: String::new(),
        approver: None,
    };
    let resp = handle_revoke(State(state_with(s, a, e)), auth_headers(), Json(neither)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let (s, a, e) = mk();
    let both = PublicRevokeRequest {
        content_hash: Some(HASH.to_owned()),
        upstream_digest: Some(UPSTREAM_DIGEST.to_owned()),
        reason: String::new(),
        approver: None,
    };
    let resp = handle_revoke(State(state_with(s, a, e)), auth_headers(), Json(both)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
