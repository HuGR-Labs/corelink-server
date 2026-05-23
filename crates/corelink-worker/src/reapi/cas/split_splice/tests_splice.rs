//! Unit tests for SpliceBlob + audit + clock-pinning behaviour.
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2). 4 tests covering happy splice, 404, tamper detection,
//! audit emit on chunk-too-large, and clock-advance neutrality.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use bytes::Bytes;
use corelink_pat::{PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
use uuid::Uuid;

use crate::reapi::cas::audit::BlobEventType;
use crate::reapi::cas::chunk_store::ChunkKey;
use crate::reapi::cas::types::{ChunkIndex, ManifestDigest};
use crate::Region;

use super::errors::SpliceError;
use super::handler_trait::SplitSpliceHandler;
use super::tests_common::{blob_digest, make_ctx, wire};
use super::types::{InitSplitOutcome, MAX_CHUNK_BYTES};

#[tokio::test]
async fn splice_blob_happy_path() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let bd = blob_digest(b"blob-splice");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    let c0 = Bytes::from_static(b"AAAA");
    let c1 = Bytes::from_static(b"BBBB");
    let c2 = Bytes::from_static(b"CCCC");
    w.handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), c0.clone(), "req")
        .await
        .unwrap();
    w.handler
        .append_chunk(&ctx, session_id, ChunkIndex(1), c1.clone(), "req")
        .await
        .unwrap();
    w.handler
        .append_chunk(&ctx, session_id, ChunkIndex(2), c2.clone(), "req")
        .await
        .unwrap();
    let fin = w
        .handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();

    // Splice.
    let mut sink = crate::reapi::cas::assembler::CollectingSink::new();
    let outcome = w
        .handler
        .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "req-spl")
        .await
        .unwrap();
    assert_eq!(outcome.chunks_streamed, 3);
    assert_eq!(outcome.bytes_streamed, 12);
    // Sink received the full blob.
    let collected: Vec<Bytes> = sink.take();
    let mut all = Vec::new();
    for c in collected {
        all.extend_from_slice(&c);
    }
    assert_eq!(&all, b"AAAABBBBCCCC");
}

#[tokio::test]
async fn splice_blob_unknown_manifest_404() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
    let unknown = ManifestDigest::from_bytes([0u8; 32]);
    let mut sink = crate::reapi::cas::assembler::CollectingSink::new();
    let err = w
        .handler
        .splice_blob(&ctx, &unknown, &mut sink, "req")
        .await
        .unwrap_err();
    assert!(matches!(err, SpliceError::ManifestNotFound));
    assert_eq!(err.cor_code(), "COR_MULTIPART_MANIFEST_NOT_FOUND");
}

#[tokio::test]
async fn splice_blob_chunk_verify_failure_aborts_stream() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let bd = blob_digest(b"blob-tamper");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    let c0 = Bytes::from_static(b"AAAA");
    let c1 = Bytes::from_static(b"BBBB");
    w.handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), c0, "req")
        .await
        .unwrap();
    let d1 = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(1), c1, "req")
        .await
        .unwrap();
    let fin = w
        .handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();
    // Tamper chunk 1 in the chunk store.
    let key = ChunkKey::new(tenant, d1);
    assert!(w.chunks.tamper_for_test(key));
    let mut sink = crate::reapi::cas::assembler::CollectingSink::new();
    let err = w
        .handler
        .splice_blob(&ctx, &fin.manifest_digest, &mut sink, "req-spl")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        SpliceError::ChunkVerificationFailed { chunk_index: 1 }
    ));
    // Sink received chunk 0 only — chunk 1 never fanned out.
    let collected: Vec<Bytes> = sink.take();
    let mut all = Vec::new();
    for c in collected {
        all.extend_from_slice(&c);
    }
    assert_eq!(&all, b"AAAA", "no unverified bytes may reach the sink");
}

#[tokio::test]
async fn audit_emit_on_chunk_too_large_path() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-big-audit");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    let huge = Bytes::from(vec![0u8; MAX_CHUNK_BYTES + 1]);
    let _ = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), huge, "req")
        .await;
    // Audit captured the failure.
    let appended = w.audit.snapshot_of(BlobEventType::SplitChunkAppended);
    assert!(
        appended.iter().any(|r| r.reason == "chunk_too_large"),
        "must emit audit on chunk_too_large path"
    );
}

#[tokio::test]
async fn clock_advance_does_not_affect_session_lifecycle() {
    // Session lifecycle is event-driven, not wall-clock-driven —
    // make sure advancing the clock does not flip a Live session
    // into something stale (sweeper handling lives in WI-S05-006).
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-clock");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    w.clock.advance_ms(7 * 24 * 60 * 60 * 1000);
    // Append still works.
    w.handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(0),
            Bytes::from_static(b"x"),
            "req",
        )
        .await
        .unwrap();
    // Finalize still works.
    w.handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();
}
