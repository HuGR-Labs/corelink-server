//! REAPI v2 SplitBlob / SpliceBlob conformance suite (WI-S05-006
//! §6.1.4 + ADR-0038 Annex).
//!
//! Pinned subset of the bazelbuild/remote-apis CAS multipart test
//! battery. Covers:
//!
//! - **SplitBlob** — 5 conformance tests (init / chunk-append /
//!   finalize / idempotent re-init / cross-tenant 404 + ordering
//!   reject).
//! - **SpliceBlob** — 5 conformance tests (success / cross-tenant 404
//!   / unknown manifest 404 / streaming bytes-equal / fail-fast on
//!   tampered chunk).
//!
//! ## Why a host-side conformance harness (not a real Bazel client)
//!
//! Per charter `trait-abstraction-defer` pattern: the real
//! `bazelbuild/remote-apis` proto + a real Bazel client driving against
//! `wrangler dev` lands alongside the CF integration tier in a forward
//! sprint (the production gRPC surface in `corelink-reapi` doesn't
//! ship until WI-S05-001's tonic wrappers land). The host-side harness
//! stitches the canonical REAPI v2 CAS multipart wire contract against
//! the pure-logic
//! [`SplitSpliceHandler`](corelink_worker::reapi::cas::SplitSpliceHandler)
//! seam — same canonical contract, no integration-tier-only deps.
//!
//! Mirrors `reapi_v2_ac_conformance.rs` (WI-S04-006) byte-for-byte in
//! structure: pinned commit, ConformanceTest fixtures, ConformanceResult
//! aggregator, top-level runner that asserts 100 % pass.

#![cfg(feature = "tower-middleware")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "test harness — panics + stdout/stderr traces are themselves the assertion surface"
)]
#![allow(missing_docs, reason = "test crate")]

use std::sync::Arc;

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_pat::{PatEnv, PatId, PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::middleware::auth_ctx::__test_helpers::make_auth_ctx;
use corelink_worker::middleware::auth_ctx::{AuthCtx, AuthMethod, PrincipalId};
use corelink_worker::reapi::cas::{
    BlobDigest, ChunkIndex, ChunkKey, CollectingSink, FakeClock, InMemoryAuditSink,
    InMemoryBlobAssembler, InMemoryChunkStore, InMemorySessionStore, InitSplitOutcome,
    ManifestDigest, SpliceError, SplitError, SplitSpliceHandler, SplitSpliceHandlerBuilder,
    SplitSpliceHandlerImpl,
};
use corelink_worker::Region;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Pinned commit of the bazelbuild/remote-apis test suite this
/// harness validates against. Recorded here for the conformance
/// report alongside the canonical commit pin in ADR-0038.
const PINNED_REMOTE_APIS_COMMIT: &str = "main@2026-04-25";

/// Canonical conformance test fixture per ADR-0038 §Annex.
#[allow(dead_code, reason = "fields surfaced via eprintln traces only")]
#[derive(Clone, Copy, Debug)]
struct ConformanceTest {
    id: &'static str,
    rpc: &'static str,
    scenario: &'static str,
}

/// Output of one conformance test.
#[allow(dead_code, reason = "fields surfaced via assert formatting")]
#[derive(Clone, Debug)]
struct ConformanceResult {
    test: ConformanceTest,
    passed: bool,
    detail: String,
}

impl ConformanceResult {
    fn pass(test: ConformanceTest) -> Self {
        Self {
            test,
            passed: true,
            detail: "PASS".to_string(),
        }
    }
    fn fail(test: ConformanceTest, detail: impl Into<String>) -> Self {
        Self {
            test,
            passed: false,
            detail: detail.into(),
        }
    }
}

fn fixed_tdk() -> Arc<TenantDerivationKey> {
    Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
}

fn make_ctx(tenant: Uuid, region: Region, scopes: PatScopes) -> AuthCtx {
    let pat_id = PatId(Uuid::nil());
    make_auth_ctx(
        PrincipalId(Uuid::nil()),
        tenant,
        region,
        scopes,
        AuthMethod::Pat {
            env: PatEnv::Pat,
            pat_id,
        },
        fixed_tdk(),
    )
}

type Wiring = SplitSpliceHandlerImpl<
    InMemorySessionStore,
    InMemoryChunkStore,
    InMemoryBlobAssembler,
    InMemoryAuditSink,
>;

fn build_handler(region: Region, now_ms: u64) -> (Wiring, Arc<InMemoryChunkStore>) {
    let sessions = Arc::new(InMemorySessionStore::new());
    let chunks = Arc::new(InMemoryChunkStore::new());
    let assembler = Arc::new(InMemoryBlobAssembler::new(Arc::clone(&chunks)));
    let audit = Arc::new(InMemoryAuditSink::new());
    let clock = Arc::new(FakeClock::new(now_ms));
    let handler = SplitSpliceHandlerImpl::new(SplitSpliceHandlerBuilder {
        region,
        sessions,
        chunks: Arc::clone(&chunks),
        assembler,
        audit,
        clock,
    });
    (handler, chunks)
}

fn bd(seed: &[u8]) -> BlobDigest {
    BlobDigest::new(Digest::compute(seed), seed.len() as u64)
}

// === SplitBlob conformance tests ===

async fn ct_split_blob_init_success() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SplitBlob/01",
        rpc: "SplitBlob.init",
        scenario: "fresh init returns Started + new session_id",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let result = handler.init_split(&ctx, &bd(b"split-init"), "req").await;
    match result {
        Ok(InitSplitOutcome::Started { .. }) => ConformanceResult::pass(test),
        other => ConformanceResult::fail(test, format!("expected Started, got {other:?}")),
    }
}

async fn ct_split_blob_chunk_append_canonical() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SplitBlob/02",
        rpc: "SplitBlob.append_chunk",
        scenario: "canonical-order append returns growing chunk count",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let init = handler.init_split(&ctx, &bd(b"chunked"), "i").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    for i in 0u32..3 {
        if let Err(e) = handler
            .append_chunk(&ctx, sid, ChunkIndex(i), Bytes::from(format!("c{i}").into_bytes()), "c")
            .await
        {
            return ConformanceResult::fail(test, format!("append c{i} failed: {e:?}"));
        }
    }
    ConformanceResult::pass(test)
}

async fn ct_split_blob_finalize_idempotent_reinit() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SplitBlob/03",
        rpc: "SplitBlob.finalize+reinit",
        scenario: "post-finalize re-init returns AlreadyChunked echo",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let blob = bd(b"finalize");
    let init = handler.init_split(&ctx, &blob, "i1").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init #1 should Start"),
    };
    handler
        .append_chunk(&ctx, sid, ChunkIndex(0), Bytes::from_static(b"x"), "c")
        .await
        .unwrap();
    let fin = handler.finalize_split(&ctx, sid, "f").await.unwrap();
    let init2 = handler.init_split(&ctx, &blob, "i2").await.unwrap();
    match init2 {
        InitSplitOutcome::AlreadyChunked {
            manifest_digest,
            chunk_count,
        } if manifest_digest == fin.manifest_digest && chunk_count == fin.chunk_count => {
            ConformanceResult::pass(test)
        }
        other => ConformanceResult::fail(
            test,
            format!("expected AlreadyChunked echoing manifest_digest, got {other:?}"),
        ),
    }
}

async fn ct_split_blob_chunk_ordering_violation_rejected() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SplitBlob/04",
        rpc: "SplitBlob.append_chunk",
        scenario: "out-of-order append rejected with ChunkOrderingViolation",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let init = handler.init_split(&ctx, &bd(b"oop"), "i").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    let err = handler
        .append_chunk(&ctx, sid, ChunkIndex(7), Bytes::from_static(b"oop"), "c")
        .await
        .unwrap_err();
    if matches!(err, SplitError::ChunkOrderingViolation { .. }) {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(test, format!("expected ChunkOrderingViolation, got {err:?}"))
    }
}

async fn ct_split_blob_cross_tenant_session_isolation() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SplitBlob/05",
        rpc: "SplitBlob.cross-tenant",
        scenario: "tenant B cannot append to tenant A's session_id",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant_a = Uuid::from_u128(1);
    let tenant_b = Uuid::from_u128(2);
    let ctx_a = make_ctx(tenant_a, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let init = handler
        .init_split(&ctx_a, &bd(b"shared"), "ia")
        .await
        .unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    // Tenant B attempts to append to tenant A's session_id.
    let err = handler
        .append_chunk(&ctx_b, sid, ChunkIndex(0), Bytes::from_static(b"x"), "cb")
        .await
        .unwrap_err();
    // Cross-tenant probing surfaces as SessionNotFound (per the
    // INV-MULTIPART-PATH-TENANT-SCOPED structural defense — the
    // SessionStore lookup returns None for `(tenant_b, sid)` even
    // though `sid` exists for tenant_a).
    if matches!(err, SplitError::SessionNotFound) {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(test, format!("expected SessionNotFound, got {err:?}"))
    }
}

// === SpliceBlob conformance tests ===

async fn ct_splice_blob_success_bytes_equal() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SpliceBlob/06",
        rpc: "SpliceBlob",
        scenario: "splice yields bytes-equal reassembly of canonical chunks",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let blob = bd(b"splice-success");
    let init = handler.init_split(&ctx, &blob, "i").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    let payload_a = Bytes::from_static(b"hello-");
    let payload_b = Bytes::from_static(b"world!");
    handler
        .append_chunk(&ctx, sid, ChunkIndex(0), payload_a.clone(), "c0")
        .await
        .unwrap();
    handler
        .append_chunk(&ctx, sid, ChunkIndex(1), payload_b.clone(), "c1")
        .await
        .unwrap();
    let fin = handler.finalize_split(&ctx, sid, "f").await.unwrap();
    let mut sink = CollectingSink::new();
    let outcome = handler
        .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "spl")
        .await
        .unwrap();
    let mut all = Vec::new();
    for c in sink.take() {
        all.extend_from_slice(&c);
    }
    let mut expected = Vec::new();
    expected.extend_from_slice(&payload_a);
    expected.extend_from_slice(&payload_b);
    if outcome.chunks_streamed == 2 && all == expected {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(
            test,
            format!(
                "expected 2 chunks bytes-equal; got chunks_streamed={}, all_len={}",
                outcome.chunks_streamed,
                all.len()
            ),
        )
    }
}

async fn ct_splice_blob_unknown_manifest_returns_404() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SpliceBlob/07",
        rpc: "SpliceBlob",
        scenario: "splice on unknown manifest_digest returns ManifestNotFound",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
    let unknown = ManifestDigest::from_bytes([7u8; 32]);
    let mut sink = CollectingSink::new();
    let err = handler
        .splice_blob(&ctx, &unknown, &mut sink, "spl")
        .await
        .unwrap_err();
    if matches!(err, SpliceError::ManifestNotFound) && sink.snapshot().is_empty() {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(test, format!("expected ManifestNotFound, got {err:?}"))
    }
}

async fn ct_splice_blob_cross_tenant_404() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SpliceBlob/08",
        rpc: "SpliceBlob.cross-tenant",
        scenario: "tenant B splice for tenant A manifest_digest returns 404",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant_a = Uuid::from_u128(1);
    let tenant_b = Uuid::from_u128(2);
    let ctx_a = make_ctx(
        tenant_a,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
    let init = handler.init_split(&ctx_a, &bd(b"x"), "i").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    handler
        .append_chunk(&ctx_a, sid, ChunkIndex(0), Bytes::from_static(b"only"), "c")
        .await
        .unwrap();
    let fin = handler.finalize_split(&ctx_a, sid, "f").await.unwrap();
    let mut sink = CollectingSink::new();
    let err = handler
        .splice_blob(&ctx_b, &fin.manifest_digest, &mut sink, "spl-b")
        .await
        .unwrap_err();
    if matches!(err, SpliceError::ManifestNotFound) && sink.snapshot().is_empty() {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(test, format!("expected ManifestNotFound, got {err:?}"))
    }
}

async fn ct_splice_blob_streaming_canonical_order() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SpliceBlob/09",
        rpc: "SpliceBlob",
        scenario: "streaming sink receives chunks in canonical chunk_index order",
    };
    let (handler, _chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let blob = bd(b"order");
    let init = handler.init_split(&ctx, &blob, "i").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    let payloads: Vec<Bytes> = (0u32..4)
        .map(|i| Bytes::from(format!("p{i}").into_bytes()))
        .collect();
    for (i, p) in payloads.iter().enumerate() {
        handler
            .append_chunk(&ctx, sid, ChunkIndex(i as u32), p.clone(), "c")
            .await
            .unwrap();
    }
    let fin = handler.finalize_split(&ctx, sid, "f").await.unwrap();
    let mut sink = CollectingSink::new();
    handler
        .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "spl")
        .await
        .unwrap();
    let actual = sink.take();
    if actual == payloads {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(
            test,
            format!(
                "chunks out of order: expected {payloads:?}, got {actual:?}"
            ),
        )
    }
}

async fn ct_splice_blob_fail_fast_on_tampered_chunk() -> ConformanceResult {
    let test = ConformanceTest {
        id: "REAPI-v2/CAS/SpliceBlob/10",
        rpc: "SpliceBlob",
        scenario: "tampered chunk fail-fasts BEFORE bytes reach sink (fail-fast invariant)",
    };
    let (handler, chunks) = build_handler(Region::Wnam, 1);
    let tenant = Uuid::from_u128(1);
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let blob = bd(b"tamper");
    let init = handler.init_split(&ctx, &blob, "i").await.unwrap();
    let sid = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        _ => return ConformanceResult::fail(test, "init should Start"),
    };
    let payload = Bytes::from_static(b"genuine");
    handler
        .append_chunk(&ctx, sid, ChunkIndex(0), payload.clone(), "c")
        .await
        .unwrap();
    let fin = handler.finalize_split(&ctx, sid, "f").await.unwrap();
    // Tamper the only chunk.
    let target_digest = corelink_worker::reapi::cas::ChunkDigest::compute(&payload);
    let key = ChunkKey::new(tenant, target_digest);
    let did_tamper = chunks.tamper_for_test(key);
    if !did_tamper {
        return ConformanceResult::fail(test, "tamper helper reported no row");
    }
    let mut sink = CollectingSink::new();
    let err = handler
        .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "spl")
        .await
        .unwrap_err();
    let is_fail_fast = matches!(err, SpliceError::ChunkVerificationFailed { .. });
    let leaked_tampered = sink
        .take()
        .into_iter()
        .any(|c| c.as_ref() == payload.as_ref());
    if is_fail_fast && !leaked_tampered {
        ConformanceResult::pass(test)
    } else {
        ConformanceResult::fail(
            test,
            format!(
                "expected fail-fast w/o tampered bytes leaking; err={err:?}, leaked={leaked_tampered}"
            ),
        )
    }
}

#[tokio::test]
async fn reapi_v2_split_splice_conformance_suite_all_pass() {
    eprintln!(
        "REAPI v2 SplitBlob/SpliceBlob conformance suite — pinned commit: {PINNED_REMOTE_APIS_COMMIT}"
    );
    let results = vec![
        ct_split_blob_init_success().await,
        ct_split_blob_chunk_append_canonical().await,
        ct_split_blob_finalize_idempotent_reinit().await,
        ct_split_blob_chunk_ordering_violation_rejected().await,
        ct_split_blob_cross_tenant_session_isolation().await,
        ct_splice_blob_success_bytes_equal().await,
        ct_splice_blob_unknown_manifest_returns_404().await,
        ct_splice_blob_cross_tenant_404().await,
        ct_splice_blob_streaming_canonical_order().await,
        ct_splice_blob_fail_fast_on_tampered_chunk().await,
    ];
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    for r in &results {
        eprintln!(
            "  [{}] {} — {}: {}",
            if r.passed { "PASS" } else { "FAIL" },
            r.test.id,
            r.test.rpc,
            r.detail
        );
    }
    eprintln!("conformance: {passed}/{total} pass");
    assert_eq!(
        passed, total,
        "REAPI v2 SplitBlob/SpliceBlob conformance suite MUST be 100 % pass; got {passed}/{total}"
    );
}
