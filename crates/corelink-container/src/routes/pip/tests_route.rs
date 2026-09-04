//! The property: **the served read path works end to end, and the wire
//! `<tenant>` segment is stripped and never trusted.**
//!
//! This is the one test that exercises the whole pip chain in a single
//! request — `nest_service` mount, the gate's tenant-segment rewrite, the
//! Option-B PAT resolve, the per-tenant index KV read, the moat get, and the
//! sha256↔blake3 digest fork — and it is deliberately built so a
//! tenant-trust regression cannot pass it.
//!
//! The load-bearing detail: the request's path tenant is the literal
//! `ignored`, which is NOT the PAT's tenant. The bytes still come back,
//! because the wheel lives under the shared `PUBLIC_NAMESPACE` and identity
//! comes from the verified PAT alone. Any change that started deriving the
//! tenant from the path, or that stopped stripping the segment before
//! `nest_service` routes, turns this into a 404 or a miss.
//!
//! It also pins the digest fork, which nothing else covers: the moat is
//! seeded with `(PUBLIC, sha256) → blake3(bytes)` at level 2 and the bytes
//! under `(PUBLIC, blake3)` at level 1, exactly the two-hop shape
//! `PipMoatStore` depends on. Collapsing the two hashes into one would break
//! here and nowhere else in this crate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use axum::http::StatusCode;
use corelink_adapter_host::pip::wheel::sha256_hex;
use corelink_pat::{mint, PatEnv, PatScopes, PrincipalId, TenantId as PatTenantId, SCOPE_CACHE_RW};
use tower::ServiceExt; // for `.oneshot`
use uuid::Uuid;

use crate::adapter_cache::canonical_hash_hex;
use crate::adapter_pat::PatRow;

use super::tests_support::{
    get, now_ms, router_with, test_key, FakeKv, FakeMap, OneTokenLookup, StubCas, SCOPE_RW,
};
use super::{CasReadHandler, CasWriteHandler, KvStore, PatVerifier, PUBLIC_NAMESPACE};

#[tokio::test]
async fn cache_hit_round_trip_with_tenant_stripped() {
    // Mint a real PAT; seed (a) the 2-level moat so the wheel BYTES are a
    // cache HIT (no upstream), and (b) the per-tenant index KV so the
    // wheel route can resolve the project entry. Proves end-to-end:
    // nest_service mount + tenant-strip + scope + Option-B resolve +
    // index KV read + moat get + sha256→blake3 digest fork.
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xBEEF);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0xF00D)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: SCOPE_RW.to_owned(),
            find_only: false,
            runner_job: false,
        },
        calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

    // The wheel bytes + their PyPI sha256 (the adapter's CAS Digest key).
    let wheel_bytes = b"PK\x03\x04fake-wheel-zip".to_vec();
    let sha256 = sha256_hex(&wheel_bytes); // 64 hex chars
    let filename = "requests-2.31.0-py3-none-any.whl";

    // Seed the moat: map (PUBLIC, sha256) → blake3(bytes); CAS holds bytes
    // under (PUBLIC, blake3). This is the sha256→blake3 fork in action.
    let blake3 = canonical_hash_hex(&wheel_bytes);
    let cas = Arc::new(StubCas::default());
    cas.0.lock().unwrap().insert(
        (PUBLIC_NAMESPACE.to_owned(), blake3.clone()),
        wheel_bytes.clone(),
    );
    let map = Arc::new(FakeMap::default());
    map.0
        .lock()
        .unwrap()
        .insert((PUBLIC_NAMESPACE.to_owned(), sha256.clone()), blake3);

    // Seed the per-tenant index KV with a PEP 691 payload that names the
    // wheel + its sha256 (the wheel route reads this to find the file).
    let index_json = serde_json::json!({
        "name": "requests",
        "files": [{
            "filename": filename,
            "url": format!("https://files.pythonhosted.org/packages/xx/{filename}#sha256={sha256}"),
            "hashes": { "sha256": sha256 },
            "requires-python": ">=3.7",
            "yanked": false,
        }],
    });
    let kv = Arc::new(FakeKv::default());
    let kv_key = format!("pip:idx:{}", "requests");
    kv.0.lock().unwrap().insert(
        (tenant_uuid.to_string(), kv_key),
        (serde_json::to_vec(&index_json).unwrap(), now_ms()),
    );

    let app = router_with(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        map,
        Arc::clone(&kv) as Arc<dyn KvStore>,
        verifier,
    );

    // path-tenant "ignored" ≠ the PAT tenant → proves the path tenant is
    // stripped + untrusted (the wheel bytes are the shared PUBLIC moat).
    let resp = app
        .oneshot(get(
            &format!("/pip/ignored/pkg/{sha256}/{filename}"),
            Some(&pt),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), wheel_bytes.as_slice());
}
