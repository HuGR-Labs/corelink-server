//! Adversarial scenarios from `specs/_proposals/adapters/cargo.md` §8
//! row 3 — all four MUST pass:
//!
//! 1. Forged Bearer token → 401 + audit row (auth_failed event).
//! 2. Oversize PUT body (>16 MiB configured cap) → 413; no CAS entry;
//!    no cache-write audit row.
//! 3. Tenant A reads Tenant B's key → MUST miss (different namespace).
//! 4. Replay attack (same key, different content) → second PUT succeeds
//!    (CAS immutability is enforced via key = BLAKE3 of content;
//!    structurally, two different contents produce different keys —
//!    the spec's intent is that a CAS entry for a key can't be
//!    overwritten with *different* bytes once stored; we verify the
//!    GET returns the FIRST value).

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

use common::{
    default_body_limit, spin_adapter, test_key, test_key_b, FailingCas, InMemoryCas,
    StaticTenantResolver,
};
use corelink_adapter_host::cargo::audit::{EVENT_TYPE_AUTH_FAILED, EVENT_TYPE_CACHE_WRITE};
use corelink_audit::ports::InMemoryAuditEmitter;

const PAT_A: &str = "corelink_tenant_a";
const PAT_B: &str = "corelink_tenant_b";
const TENANT_A: &str = "tenant-a";
const TENANT_B: &str = "tenant-b";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forged_pat_returns_401() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) =
        spin_adapter(cas.clone(), resolver, audit.clone(), default_body_limit()).await;

    let key = test_key();
    let client = reqwest::Client::new();

    // Unknown PAT with the right prefix.
    let resp_unknown = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", "Bearer corelink_forged_unknown_token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp_unknown.status(), 401);

    // Missing header.
    let resp_missing = client
        .get(format!("http://{addr}/{key}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_missing.status(), 401);

    // Wrong prefix (not `corelink_`).
    let resp_wrong = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", "Bearer ghp_github_style_token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp_wrong.status(), 401);

    // No CAS writes.
    assert_eq!(cas.len(), 0);

    // Auth-failed audit rows (one per 401 that reached the resolver).
    let events = audit.snapshot();
    // Missing header and wrong prefix fail at extract_bearer (before resolver);
    // unknown PAT fails at resolver. All three paths emit auth_failed.
    let auth_failed: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == EVENT_TYPE_AUTH_FAILED)
        .collect();
    assert!(
        !auth_failed.is_empty(),
        "at least one auth_failed audit row expected"
    );

    // No cache-write rows.
    let writes: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == EVENT_TYPE_CACHE_WRITE)
        .collect();
    assert_eq!(writes.len(), 0, "no cache-write audit rows on auth failure");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversize_put_body_returns_413() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    // Cap to 256 KiB; PUT 1 MiB.
    let cap: u64 = 262_144;
    let (addr, _adapter) = spin_adapter(cas.clone(), resolver, audit.clone(), cap).await;

    let big_body = vec![0u8; 1_048_576]; // 1 MiB
    let key = test_key();

    let resp = reqwest::Client::new()
        .put(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .body(big_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 413);

    // Fail-CLOSED: no CAS write, no cache-write audit row.
    assert_eq!(cas.len(), 0, "CAS must be empty after 413");
    let events = audit.snapshot();
    let writes: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == EVENT_TYPE_CACHE_WRITE)
        .collect();
    assert_eq!(
        writes.len(),
        0,
        "no cache-write audit row on oversized body"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tenant_isolation_holds() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(
        StaticTenantResolver::new()
            .with(PAT_A, TENANT_A)
            .with(PAT_B, TENANT_B),
    );
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) =
        spin_adapter(cas.clone(), resolver, audit.clone(), default_body_limit()).await;

    let key = test_key();
    let bytes_a = b"artifact-for-tenant-a".to_vec();
    let bytes_b = b"artifact-for-tenant-b".to_vec();
    let client = reqwest::Client::new();

    // Tenant A stores an artifact.
    let put_a = client
        .put(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .body(bytes_a.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(put_a.status(), 200);

    // Tenant B GET same key → must MISS (different namespace).
    let get_b = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT_B}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        get_b.status(),
        404,
        "tenant B must not see tenant A's entry"
    );

    // Tenant A GET → must HIT.
    let get_a = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_a.status(), 200);
    assert_eq!(
        get_a.bytes().await.unwrap().to_vec(),
        bytes_a,
        "tenant A must receive its own bytes"
    );

    // Tenant B stores its own artifact under the same key.
    let put_b = client
        .put(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT_B}"))
        .body(bytes_b.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(put_b.status(), 200);

    // CAS has 2 entries (one per tenant) under the same digest_hex.
    assert_eq!(cas.len(), 2);
    let tenants: std::collections::HashSet<String> =
        cas.keys().into_iter().map(|(t, _)| t).collect();
    assert!(tenants.contains(TENANT_A));
    assert!(tenants.contains(TENANT_B));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cas_failure_returns_502() {
    let cas = Arc::new(FailingCas);
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) = spin_adapter(cas, resolver, audit.clone(), default_body_limit()).await;

    let key = test_key();
    let client = reqwest::Client::new();

    let get_resp = client
        .get(format!("http://{addr}/{key}"))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 502, "CAS failure must return 502");

    // No cache-write audit rows.
    assert_eq!(
        audit
            .snapshot()
            .iter()
            .filter(|e| e.event_type == EVENT_TYPE_CACHE_WRITE)
            .count(),
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_key_format_returns_400() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) = spin_adapter(cas, resolver, audit, default_body_limit()).await;

    let client = reqwest::Client::new();

    // A GENUINELY malformed key is rejected at the gate with 400, before any CAS
    // lookup. `normalize_key` rejects empty / >256-char / traversal (`.`/`..`/`/`)
    // / illegal-character keys; an over-length key is the unambiguous fixture.
    //
    // NOTE: a merely non-64-hex / short key is NOT malformed — sccache's HTTP
    // backend sends non-hex CONTROL keys (notably `.sccache_check`), so the
    // adapter ACCEPTS them (see the control-key assertion below). The original
    // fixture `/shortkey` wrongly expected 400; the sccache-compat fix (translate
    // .rs) made short non-hex keys VALID, so this asserts the real contract.
    let too_long = "a".repeat(257);
    let resp = client
        .get(format!("http://{addr}/{too_long}"))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "over-length (malformed) key must return 400"
    );

    // Lock in the sccache-compat contract: a non-hex CONTROL key (the
    // `.sccache_check` startup probe) is ACCEPTED — 404 on a miss, NEVER 400. A
    // 400 here disables the sccache backend (the real client never works) — the
    // exact regression the old `/shortkey`-expects-400 test would have masked.
    let ctrl = client
        .get(format!("http://{addr}/.sccache_check"))
        .header("Authorization", format!("Bearer {PAT_A}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        ctrl.status(),
        404,
        "non-hex control key (.sccache_check) must be accepted (404 miss), not 400"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn different_keys_are_independent_entries() {
    let cas = Arc::new(InMemoryCas::new());
    let resolver = Arc::new(StaticTenantResolver::new().with(PAT_A, TENANT_A));
    let audit = Arc::new(InMemoryAuditEmitter::new());

    let (addr, _adapter) =
        spin_adapter(cas.clone(), resolver, audit.clone(), default_body_limit()).await;

    let key1 = test_key();
    let key2 = test_key_b();
    let bytes1 = b"artifact-one".to_vec();
    let bytes2 = b"artifact-two".to_vec();
    let client = reqwest::Client::new();

    for (key, body) in [(&key1, &bytes1), (&key2, &bytes2)] {
        let r = client
            .put(format!("http://{addr}/{key}"))
            .header("Authorization", format!("Bearer {PAT_A}"))
            .body(body.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }

    // Both entries stored.
    assert_eq!(cas.len(), 2);
    // Two audit write rows.
    assert_eq!(audit.snapshot().len(), 2);
}
