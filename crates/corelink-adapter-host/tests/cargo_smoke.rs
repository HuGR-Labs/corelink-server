//! Smoke end-to-end: spin the adapter, simulate sccache PUT + GET + HEAD
//! wire. Per `specs/_proposals/adapters/cargo.md` §8 row 1 — the second
//! GET request MUST serve from CAS (zero re-computation).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args,
    clippy::missing_docs_in_private_items
)]

#[path = "cargo_common.rs"]
mod common;

use std::sync::Arc;

use common::{default_body_limit, spin_adapter, InMemoryCas, StaticTenantResolver, test_key};
use corelink_adapter_host::cargo::audit::EVENT_TYPE_CACHE_WRITE;
use corelink_audit::ports::InMemoryAuditEmitter;

const ARTIFACT_BYTES: &[u8] = b"\x7fELF\x02\x01\x01\x00fake-rust-artifact";
const PAT: &str = "hugr-pat_tenant_smoke";
const TENANT: &str = "tenant-smoke";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_then_get_is_a_cache_hit() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT, TENANT));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) = spin_adapter(
        cas.clone(),
        resolver,
        audit.clone(),
        default_body_limit(),
    )
    .await;

    let key = test_key();
    let client = reqwest::Client::new();

    // First GET: CAS miss → 404.
    let miss = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .send()
        .await
        .unwrap();
    assert_eq!(miss.status(), 404, "initial GET must be a miss");

    // PUT: write the artifact.
    let put_resp = client
        .put(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .body(ARTIFACT_BYTES.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(put_resp.status(), 200, "PUT must succeed");

    // Second GET: CAS hit → 200 + bytes.
    let hit = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .send()
        .await
        .unwrap();
    assert_eq!(hit.status(), 200, "second GET must hit cache");
    assert_eq!(
        hit.bytes().await.unwrap().as_ref(),
        ARTIFACT_BYTES,
        "CAS must return exact artifact bytes"
    );

    // Audit: exactly one write row.
    let events = audit.snapshot();
    assert_eq!(events.len(), 1, "exactly one audit row for the PUT");
    assert_eq!(events[0].event_type, EVENT_TYPE_CACHE_WRITE);
    assert_eq!(events[0].tenant_id, TENANT);
    assert_eq!(events[0].payload["adapter"], "cargo");
    assert_eq!(
        events[0].payload["body_size_bytes"],
        ARTIFACT_BYTES.len()
    );

    // CAS: exactly one entry, namespaced to TENANT.
    assert_eq!(cas.len(), 1);
    let k = cas.keys().pop().unwrap();
    assert_eq!(k.0, TENANT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn head_returns_200_after_put() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT, TENANT));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) = spin_adapter(
        cas.clone(),
        resolver,
        audit.clone(),
        default_body_limit(),
    )
    .await;

    let key = test_key();
    let client = reqwest::Client::new();

    // HEAD before PUT → 404.
    let head_miss = client
        .head(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .send()
        .await
        .unwrap();
    assert_eq!(head_miss.status(), 404, "HEAD before PUT must be 404");

    // PUT.
    client
        .put(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .body(ARTIFACT_BYTES.to_vec())
        .send()
        .await
        .unwrap();

    // HEAD after PUT → 200, no body.
    let head_hit = client
        .head(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT}"))
        .send()
        .await
        .unwrap();
    assert_eq!(head_hit.status(), 200, "HEAD after PUT must be 200");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_is_idempotent() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT, TENANT));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) = spin_adapter(
        cas.clone(),
        resolver,
        audit.clone(),
        default_body_limit(),
    )
    .await;

    let key = test_key();
    let client = reqwest::Client::new();

    // Two PUTs for the same key: both must succeed (CAS append-only,
    // idempotent overwrite).
    for _ in 0..2u8 {
        let r = client
            .put(format!("http://{addr}/{key}"))
            .header("Authorization", format!("Bearer {PAT}"))
            .body(ARTIFACT_BYTES.to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }

    // Two PUT audit rows.
    assert_eq!(audit.snapshot().len(), 2);
    // But CAS still has one entry (idempotent).
    assert_eq!(cas.len(), 1);
}
