//! Adversarial scenarios from `specs/_proposals/adapters/brew.md` §8
//! row 3 — all four MUST pass:
//!
//! 1. Oversize bottle (> `bottle_size_limit_bytes`) → 413 + audit row not emitted.
//! 2. Forged / unknown PAT → 401 + audit row not emitted.
//! 3. Tenant isolation: A's bottle is NOT served to B's request.
//! 4. Upstream 5xx → 502 + audit row not emitted; no half-store in CAS.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::missing_docs_in_private_items
)]

#[path = "brew_common.rs"]
mod common;

use std::sync::Arc;

use common::{default_bottle_limit, spin_adapter, InMemoryCas, StaticTenantResolver};
use corelink_adapter_host::brew::audit::{EVENT_TYPE_CACHE_FILL, EVENT_TYPE_INTEGRITY_MISMATCH};
use corelink_audit::ports::InMemoryAuditEmitter;
use sha2::{Digest as _, Sha256};
use url::Url;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAT_A: &str = "corelink_tenant_a";
const PAT_B: &str = "corelink_tenant_b";
const TENANT_A: &str = "tenant-a";
const TENANT_B: &str = "tenant-b";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversize_bottle_returns_413() {
    let upstream = MockServer::start().await;
    // 1 MiB payload; we cap to 256 KiB.
    let big = vec![0u8; 1_048_576];
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(big))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        262_144, // 256 KiB cap
    )
    .await;

    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/v2/homebrew/core/big/blobs/sha256:{}",
            "a".repeat(64)
        ))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 413);

    // Fail-CLOSED: NO CAS entry, NO audit row.
    assert_eq!(cas.len(), 0, "CAS must not contain a partial bottle");
    assert_eq!(
        audit.snapshot().len(),
        0,
        "audit row must NOT be emitted on oversize"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forged_pat_returns_401() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"x".to_vec()))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    // Unknown PAT with the right prefix.
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/v2/some/bottle"))
        .header("Authorization", "Bearer corelink_forged_unknown_token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Missing header.
    let resp_missing = reqwest::Client::new()
        .get(format!("http://{addr}/v2/some/bottle"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_missing.status(), 401);

    // Wrong prefix (not `corelink_`).
    let resp_wrong = reqwest::Client::new()
        .get(format!("http://{addr}/v2/some/bottle"))
        .header("Authorization", "Bearer ghp_github_style_token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp_wrong.status(), 401);

    assert_eq!(cas.len(), 0);
    assert_eq!(audit.snapshot().len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tenant_isolation_holds() {
    let upstream = MockServer::start().await;
    let bytes_a = b"bottle-for-tenant-a".to_vec();
    let bytes_b = b"bottle-for-tenant-b".to_vec();

    // Same URL, but we hand each tenant a different upstream body.
    // To simulate that without per-request state we run the test in
    // two phases — phase 1 stores tenant_a's bytes, phase 2 swaps the
    // upstream mock and runs tenant_b's request. The crucial check is
    // that tenant_b's request does NOT see tenant_a's CAS entry.
    //
    // The path is content-addressed (`…/blobs/sha256:<digest-of-bytes_a>`):
    // digest-addressed bottles are the ONLY ones cached after F-005 (mutable
    // tag-addressed paths are served but never cached into `_public`), so this
    // is the path that actually exercises per-tenant CAS isolation. The honest
    // digest matches bytes_a; bytes_b deliberately differs (a swapped upstream)
    // and would fail bytes_a's digest — so phase 2 must NOT be a verified store
    // of bytes_b under that digest. To keep BOTH stores legitimate we give each
    // tenant its OWN honest digest path.
    let digest_a = hex::encode(Sha256::digest(&bytes_a));
    let digest_b = hex::encode(Sha256::digest(&bytes_b));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes_a.clone()))
        .up_to_n_times(1)
        .mount(&upstream)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes_b.clone()))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(
        StaticTenantResolver::new()
            .with(PAT_A, TENANT_A)
            .with(PAT_B, TENANT_B),
    );
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let client = reqwest::Client::new();

    let r_a = client
        .get(format!(
            "http://{addr}/v2/homebrew/core/shared/blobs/sha256:{digest_a}"
        ))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(r_a.status(), 200);
    assert_eq!(r_a.bytes().await.unwrap().to_vec(), bytes_a);

    // Tenant B different content-addressed URL — must NOT see tenant A's CAS
    // entry; must refetch upstream and receive bytes_b.
    let r_b = client
        .get(format!(
            "http://{addr}/v2/homebrew/core/shared/blobs/sha256:{digest_b}"
        ))
        .header("Authorization", format!("Bearer {PAT_B}"))
        .send()
        .await
        .unwrap();
    assert_eq!(r_b.status(), 200);
    let body_b = r_b.bytes().await.unwrap().to_vec();
    assert_eq!(
        body_b, bytes_b,
        "tenant B MUST refetch upstream and receive its own bytes"
    );
    assert_ne!(
        body_b, bytes_a,
        "tenant B MUST NOT receive tenant A's CAS entry"
    );

    // CAS now has 2 entries (one per tenant), namespaced by tenant_id.
    assert_eq!(cas.len(), 2);
    let tenants: std::collections::HashSet<String> =
        cas.keys().into_iter().map(|(t, _)| t).collect();
    assert!(tenants.contains(TENANT_A));
    assert!(tenants.contains(TENANT_B));

    // Two cache-fill audit rows, one per tenant.
    let events = audit.snapshot();
    assert_eq!(events.len(), 2);
    let tenants_in_audit: std::collections::HashSet<String> =
        events.iter().map(|e| e.tenant_id.clone()).collect();
    assert!(tenants_in_audit.contains(TENANT_A));
    assert!(tenants_in_audit.contains(TENANT_B));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upstream_5xx_returns_502_with_no_half_store() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/v2/homebrew/core/curl/manifests/8.5.0"
        ))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);

    // No CAS write, no audit row (audit is emitted only on the
    // success path, AFTER a clean upstream fetch).
    assert_eq!(cas.len(), 0, "no half-stored bottle on upstream 5xx");
    assert_eq!(audit.snapshot().len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mitm_digest_mismatch_is_refused_and_never_stored() {
    // Content-addressed ghcr.io blob path whose declared digest does NOT
    // match the served bytes (a MITMed / corrupt upstream). The adapter
    // must refuse: 502, NOTHING stored, an integrity_mismatch audit row.
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered-bytes".to_vec()))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let declared = "a".repeat(64); // ≠ sha256(b"tampered-bytes")
    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/v2/homebrew/core/curl/blobs/sha256:{declared}"
        ))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502, "digest mismatch must refuse, not serve");

    assert_eq!(cas.len(), 0, "tampered bytes must NEVER reach the store");
    let events = audit.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EVENT_TYPE_INTEGRITY_MISMATCH);
    assert_eq!(events[0].tenant_id, TENANT_A);
    assert_eq!(events[0].payload["expected_sha256"], declared);
    assert_eq!(events[0].payload["outcome"], "refused");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forbidden_repo_path_is_refused_before_any_fetch_or_store() {
    // F-005: a path OUTSIDE the allowed Homebrew repo namespace
    // (`homebrew/core` / `homebrew/cask`) must be rejected with 403 BEFORE the
    // adapter touches the network or the cache — brew is NOT an unrestricted
    // authenticated ghcr.io proxy. The upstream mock counts ZERO hits.
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"attacker-controlled".to_vec()))
        .expect(0) // MUST NOT be reached
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let client = reqwest::Client::new();
    // An attacker-chosen repo + a digest-form ref: even digest-addressed, an
    // off-allowlist repo must be refused (the proxy-abuse vector).
    for path in [
        "/v2/attacker/evil/blobs/sha256:".to_owned() + &"a".repeat(64),
        "/v2/homebrew/evil/manifests/latest".to_owned(),
        "/v2/library/ubuntu/manifests/latest".to_owned(),
        "/etc/passwd".to_owned(),
    ] {
        let resp = client
            .get(format!("http://{addr}{path}"))
            .header("Authorization", format!("Bearer {PAT_A}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 403, "off-allowlist path {path} must be 403");
    }

    assert_eq!(
        cas.len(),
        0,
        "off-allowlist paths must NEVER reach the store"
    );
    assert_eq!(
        audit.snapshot().len(),
        0,
        "off-allowlist paths must NOT emit an audit row"
    );
    let received = upstream.received_requests().await.unwrap();
    assert!(
        received.is_empty(),
        "off-allowlist paths must NEVER reach upstream; saw {received:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tag_addressed_manifest_is_served_but_not_cached_into_public() {
    // F-005: a tag-addressed (mutable) manifest under an ALLOWED repo is served
    // to the requesting client but MUST NOT be pinned into the shared `_public`
    // namespace — only digest-verified bytes may be cached, else one tenant's
    // fetch poisons every later tenant. Proven by a SECOND request hitting
    // upstream again (no cache entry to short-circuit it) + an empty CAS.
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"mutable-manifest-bytes".to_vec()))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/v2/homebrew/core/jq/manifests/1.7");

    let r1 = client
        .get(&url)
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), 200, "tag-addressed manifest is still served");
    assert_eq!(
        r1.bytes().await.unwrap().as_ref(),
        b"mutable-manifest-bytes"
    );

    let r2 = client
        .get(&url)
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), 200);

    assert_eq!(
        cas.len(),
        0,
        "tag-addressed (mutable) bytes must NEVER be cached into _public (F-005)"
    );
    // Both requests reached upstream (no cache short-circuit), proving no
    // poisonable entry was created.
    let received = upstream.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        2,
        "each tag-addressed request must re-fetch upstream (not served from a cache entry)"
    );
    // No cache-fill audit row for an uncached fetch.
    assert!(
        audit
            .snapshot()
            .iter()
            .all(|e| e.event_type != EVENT_TYPE_CACHE_FILL),
        "no cache-fill audit row may be emitted for an uncached tag-addressed fetch"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn matching_digest_is_verified_and_stored() {
    // The same content-addressed path with HONEST bytes: 200, stored, and
    // the cache-fill audit row carries integrity = verified-sha256.
    let bottle: &[u8] = b"\x1f\x8b\x08\x00honest-bottle";
    let digest = hex::encode(Sha256::digest(bottle));

    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.to_vec()))
        .mount(&upstream)
        .await;

    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let upstream_url = Url::parse(&upstream.uri()).unwrap();
    let (addr, _adapter) = spin_adapter(
        upstream_url,
        cas.clone(),
        resolver,
        audit.clone(),
        default_bottle_limit(),
    )
    .await;

    let resp = reqwest::Client::new()
        .get(format!(
            "http://{addr}/v2/homebrew/core/curl/blobs/sha256:{digest}"
        ))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), bottle);

    assert_eq!(cas.len(), 1, "verified bytes are stored");
    let events = audit.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EVENT_TYPE_CACHE_FILL);
    assert_eq!(events[0].payload["integrity"], "verified-sha256");
}
