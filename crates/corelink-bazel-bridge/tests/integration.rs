//! End-to-end REAPI v2 semantics: URI parse → adapter dispatch → handler.
//!
//! Tests mock REAPI HTTP requests at the URI-parse level and assert the full
//! round-trip (no real Bazel client required; no network I/O).
//!
//! Test count: ≥ 20 (see summary at the bottom of the file).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests may use these"
)]

use std::sync::Arc;

use corelink_bazel_bridge::{
    adapter::{BazelAdapter, WriteCtx},
    digest::{sha256_hex, Digest},
    error::BazelBridgeError,
    find_missing::{
        build_find_missing_response, parse_find_missing_request, FindMissingHandler,
        InMemoryFindMissing,
    },
    uri::{parse_reapi_path, RoapiOperation},
    FIND_MISSING_BLOB_CAP, REAPI_VERSION,
};
use corelink_handler_ac::{
    AcLookupHandler, AcUpdateHandler, InMemoryAcHandler, InMemoryAuditSink as AcAuditSink,
    InMemorySliObserver as AcSliObserver,
};
use corelink_handler_cas::handler::fake_hash;
use corelink_handler_cas::{
    CasReadHandler, CasWriteHandler, InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn make_adapter() -> BazelAdapter {
    // CAS and AC have separate AuditSink/SliObserver trait definitions.
    let cas_audit = Arc::new(InMemoryAuditSink::new());
    let cas_sli = Arc::new(InMemorySliObserver::new());
    let cas = Arc::new(InMemoryCasHandler::new(cas_audit, cas_sli));
    let ac_audit = Arc::new(AcAuditSink::new());
    let ac_sli = Arc::new(AcSliObserver::new());
    let ac = Arc::new(InMemoryAcHandler::new(ac_audit, ac_sli));
    BazelAdapter::new(
        cas.clone() as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        ac.clone() as Arc<dyn AcLookupHandler>,
        ac as Arc<dyn AcUpdateHandler>,
    )
}

fn make_cas_handler() -> Arc<InMemoryCasHandler> {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    Arc::new(InMemoryCasHandler::new(audit, sli))
}

const TENANT: &str = "corp";
/// 64 lowercase hex chars.
const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

// ─── REAPI_VERSION constant ───────────────────────────────────────────────────

#[test]
fn reapi_version_is_pinned() {
    assert_eq!(REAPI_VERSION, "2.3.0");
}

// ─── URI parse → adapter: CAS read ───────────────────────────────────────────

#[test]
fn end_to_end_cas_get_hit() {
    let adapter = make_adapter();
    let bytes = b"hello bazel".to_vec();
    // Bazel CAS keyspace content-addresses with SHA-256.
    let hash = sha256_hex(&bytes);
    let size = bytes.len() as u64;
    let digest = Digest::new(&hash, size).expect("digest");

    // Write via adapter first.
    adapter
        .cas_put(
            TENANT,
            &digest,
            bytes.clone(),
            WriteCtx {
                principal: "ci",
                caller_tenant: TENANT,
                at_unix_ms: 1,
                storage_quota_bytes: Some(0),
            },
        )
        .expect("put");

    // Now parse the REAPI GET URI.
    let path = format!("/{TENANT}/blobs/{hash}/{size}");
    let op = parse_reapi_path("GET", &path).expect("parse");
    let (instance, d) = match op {
        RoapiOperation::CasRead { instance, digest } => (instance, digest),
        other => panic!("expected CasRead, got {other:?}"),
    };

    let got = adapter
        .cas_get(&instance, &d, "ci", TENANT, 2)
        .expect("get");
    assert_eq!(got, bytes);
}

#[test]
fn end_to_end_cas_get_miss_returns_not_found() {
    let adapter = make_adapter();
    let path = format!("/{TENANT}/blobs/{HASH_A}/100");
    let op = parse_reapi_path("GET", &path).expect("parse");
    let (instance, digest) = match op {
        RoapiOperation::CasRead { instance, digest } => (instance, digest),
        other => panic!("{other:?}"),
    };
    let err = adapter
        .cas_get(&instance, &digest, "ci", TENANT, 0)
        .expect_err("miss");
    assert!(matches!(err, BazelBridgeError::NotFound { .. }));
}

// ─── URI parse → adapter: CAS write ──────────────────────────────────────────

#[test]
fn end_to_end_cas_put_via_upload_uri() {
    let adapter = make_adapter();
    let bytes = b"artifact content".to_vec();
    let hash = sha256_hex(&bytes);
    let size = bytes.len() as u64;
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    let path = format!("/{TENANT}/uploads/{uuid}/blobs/{hash}/{size}");
    let op = parse_reapi_path("POST", &path).expect("parse");
    let (instance, upload_id, digest) = match op {
        RoapiOperation::CasWrite {
            instance,
            upload_id,
            digest,
        } => (instance, upload_id, digest),
        other => panic!("{other:?}"),
    };
    assert_eq!(upload_id, uuid);
    adapter
        .cas_put(
            &instance,
            &digest,
            bytes.clone(),
            WriteCtx {
                principal: "ci",
                caller_tenant: TENANT,
                at_unix_ms: 0,
                storage_quota_bytes: Some(0),
            },
        )
        .expect("put");

    // Verify with a GET.
    let got = adapter
        .cas_get(&instance, &digest, "ci", TENANT, 1)
        .expect("get after put");
    assert_eq!(got, bytes);
}

#[test]
fn end_to_end_cas_put_size_mismatch_rejected() {
    let adapter = make_adapter();
    let bytes = b"short".to_vec();
    let hash = fake_hash(&bytes);
    let wrong_size = 100u64; // actual len = 5
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    let path = format!("/{TENANT}/uploads/{uuid}/blobs/{hash}/{wrong_size}");
    let op = parse_reapi_path("POST", &path).expect("parse");
    let (instance, _, digest) = match op {
        RoapiOperation::CasWrite {
            instance,
            upload_id,
            digest,
        } => (instance, upload_id, digest),
        other => panic!("{other:?}"),
    };
    let err = adapter
        .cas_put(
            &instance,
            &digest,
            bytes,
            WriteCtx {
                principal: "ci",
                caller_tenant: TENANT,
                at_unix_ms: 0,
                storage_quota_bytes: Some(0),
            },
        )
        .expect_err("size mismatch");
    assert!(matches!(err, BazelBridgeError::SizeMismatch { .. }));
}

// ─── URI parse → adapter: AC read/write ──────────────────────────────────────

#[test]
fn end_to_end_ac_put_and_get_via_uri() {
    let adapter = make_adapter();
    let payload = b"action_result_data".to_vec();
    let path = format!("/{TENANT}/blobs/ac/{HASH_A}/32");

    // Write.
    let put_op = parse_reapi_path("PUT", &path).expect("parse PUT");
    let (instance, digest) = match put_op {
        RoapiOperation::AcWrite { instance, digest } => (instance, digest),
        other => panic!("{other:?}"),
    };
    adapter
        .ac_put(
            &instance,
            &digest,
            payload.clone(),
            WriteCtx {
                principal: "ci",
                caller_tenant: TENANT,
                at_unix_ms: 0,
                storage_quota_bytes: Some(0),
            },
        )
        .expect("ac put");

    // Read.
    let get_op = parse_reapi_path("GET", &path).expect("parse GET");
    let (instance2, digest2) = match get_op {
        RoapiOperation::AcRead { instance, digest } => (instance, digest),
        other => panic!("{other:?}"),
    };
    let got = adapter
        .ac_get(&instance2, &digest2, "ci", TENANT, 1)
        .expect("ac get");
    assert_eq!(got, payload);
}

#[test]
fn end_to_end_ac_get_miss_returns_not_found() {
    let adapter = make_adapter();
    let path = format!("/{TENANT}/blobs/ac/{HASH_B}/10");
    let op = parse_reapi_path("GET", &path).expect("parse");
    let (instance, digest) = match op {
        RoapiOperation::AcRead { instance, digest } => (instance, digest),
        other => panic!("{other:?}"),
    };
    let err = adapter
        .ac_get(&instance, &digest, "ci", TENANT, 0)
        .expect_err("miss");
    assert!(matches!(err, BazelBridgeError::NotFound { .. }));
}

// ─── URI parse → findMissingBlobs ────────────────────────────────────────────

#[test]
fn end_to_end_find_missing_blobs_all_absent() {
    let cas = make_cas_handler();
    let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);

    let body = format!(
        r#"{{"blobDigests":[{{"hash":"{HASH_A}","sizeBytes":10}},{{"hash":"{HASH_B}","sizeBytes":20}}]}}"#
    );
    let digests = parse_find_missing_request(&body).expect("parse");
    let path = format!("/{TENANT}/findMissingBlobs");
    let op = parse_reapi_path("POST", &path).expect("parse op");
    let instance = match op {
        RoapiOperation::FindMissingBlobs { instance } => instance,
        other => panic!("{other:?}"),
    };
    let missing = fm
        .find_missing(&instance, "ci", &instance, 0, &digests)
        .expect("find_missing");
    let json = build_find_missing_response(missing).expect("build response");
    assert!(json.contains(HASH_A));
    assert!(json.contains(HASH_B));
}

#[test]
fn end_to_end_find_missing_blobs_partial_hit() {
    let cas = make_cas_handler();
    let bytes = b"cached_artifact".to_vec();
    let hash = fake_hash(&bytes);
    cas.seed(TENANT, &hash, bytes.clone()).expect("seed");

    let present_digest = Digest::new(&hash, bytes.len() as u64).expect("digest");
    let absent_digest = Digest::new(HASH_B, 1).expect("digest");

    let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
    let missing = fm
        .find_missing(
            TENANT,
            "ci",
            TENANT,
            0,
            &[present_digest, absent_digest.clone()],
        )
        .expect("find_missing");
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], absent_digest);
}

#[test]
fn end_to_end_find_missing_blobs_empty_request() {
    let cas = make_cas_handler();
    let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
    let body = r#"{"blobDigests":[]}"#;
    let digests = parse_find_missing_request(body).expect("parse");
    let missing = fm
        .find_missing(TENANT, "ci", TENANT, 0, &digests)
        .expect("find_missing empty");
    assert!(missing.is_empty());
}

#[test]
fn end_to_end_find_missing_blobs_deduplication_semantics() {
    // Bazel may submit the same digest multiple times; we report it once per
    // occurrence (the spec does not mandate dedup on the server side, but we
    // preserve the caller's intent: if both are missing, both are returned).
    let cas = make_cas_handler();
    let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
    let d = Digest::new(HASH_A, 5).expect("digest");
    let digests = vec![d.clone(), d];
    let missing = fm
        .find_missing(TENANT, "ci", TENANT, 0, &digests)
        .expect("find_missing dedup");
    // Both occurrences absent → both appear in the missing list.
    assert_eq!(missing.len(), 2);
}

// ─── Cross-tenant denial end-to-end ──────────────────────────────────────────

#[test]
fn end_to_end_cas_get_cross_tenant_denied() {
    let adapter = make_adapter();
    let digest = Digest::new(HASH_A, 0).expect("digest");
    let err = adapter
        .cas_get("victim", &digest, "attacker", "attacker_corp", 0)
        .expect_err("cross-tenant");
    assert!(matches!(err, BazelBridgeError::CrossTenantDenied { .. }));
    assert_eq!(err.http_status(), 403);
}

#[test]
fn end_to_end_ac_put_cross_tenant_denied() {
    let adapter = make_adapter();
    let digest = Digest::new(HASH_B, 0).expect("digest");
    let err = adapter
        .ac_put(
            "victim",
            &digest,
            vec![],
            WriteCtx {
                principal: "attacker",
                caller_tenant: "attacker_corp",
                at_unix_ms: 0,
                storage_quota_bytes: Some(0),
            },
        )
        .expect_err("cross-tenant");
    assert!(matches!(err, BazelBridgeError::CrossTenantDenied { .. }));
}

// ─── Digest edge cases ────────────────────────────────────────────────────────

#[test]
fn digest_empty_blob_sha_is_valid() {
    // SHA-256 of empty bytes.
    let sha256_empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let d = Digest::new(sha256_empty, 0).expect("empty blob digest");
    assert_eq!(d.size_bytes, 0);
}

#[test]
fn digest_max_size_bytes_boundary() {
    let d = Digest::new(HASH_A, corelink_bazel_bridge::MAX_BLOB_SIZE_BYTES).expect("max size");
    assert_eq!(d.size_bytes, corelink_bazel_bridge::MAX_BLOB_SIZE_BYTES);
}

#[test]
fn digest_over_max_size_bytes_rejected() {
    let err =
        Digest::new(HASH_A, corelink_bazel_bridge::MAX_BLOB_SIZE_BYTES + 1).expect_err("over cap");
    assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
}

// ─── findMissingBlobs cap ─────────────────────────────────────────────────────

#[test]
fn find_missing_cap_constant_is_4096() {
    assert_eq!(FIND_MISSING_BLOB_CAP, 4096);
}

#[test]
fn find_missing_exactly_at_cap_is_accepted() {
    let cas = make_cas_handler();
    let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
    let d = Digest::new(HASH_A, 1).expect("digest");
    let digests: Vec<Digest> = (0..FIND_MISSING_BLOB_CAP).map(|_| d.clone()).collect();
    // 4096 entries: at cap, should succeed (all will be missing).
    let result = fm.find_missing(TENANT, "ci", TENANT, 0, &digests);
    assert!(result.is_ok(), "exactly at cap should succeed");
}

#[test]
fn find_missing_over_cap_is_rejected() {
    let cas = make_cas_handler();
    let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
    let d = Digest::new(HASH_A, 1).expect("digest");
    let digests: Vec<Digest> = (0..=FIND_MISSING_BLOB_CAP).map(|_| d.clone()).collect();
    let err = fm
        .find_missing(TENANT, "ci", TENANT, 0, &digests)
        .expect_err("over cap");
    assert!(matches!(
        err,
        BazelBridgeError::BatchTooLarge { cap: 4096, .. }
    ));
    assert_eq!(err.http_status(), 413);
}

// ─── Error HTTP status codes ──────────────────────────────────────────────────

#[test]
fn error_http_status_codes_match_spec() {
    assert_eq!(
        BazelBridgeError::NotFound {
            tenant: "t".into(),
            hash: "h".into()
        }
        .http_status(),
        404
    );
    assert_eq!(
        BazelBridgeError::InvalidDigest { reason: "r".into() }.http_status(),
        400
    );
    assert_eq!(
        BazelBridgeError::SizeMismatch {
            digest_size: 1,
            actual: 2
        }
        .http_status(),
        400
    );
    assert_eq!(
        BazelBridgeError::CrossTenantDenied {
            caller: "a".into(),
            requested: "b".into()
        }
        .http_status(),
        403
    );
    assert_eq!(BazelBridgeError::AuditFailed("x".into()).http_status(), 503);
    assert_eq!(
        BazelBridgeError::BatchTooLarge {
            requested: 5000,
            cap: 4096
        }
        .http_status(),
        413
    );
    assert_eq!(BazelBridgeError::Internal("x".into()).http_status(), 500);
}
