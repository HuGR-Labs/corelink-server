//! Unit tests for SplitBlob lifecycle (init / append / finalize / abort).
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2). 13 tests covering happy-path + every documented
//! error mode.

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
use crate::reapi::cas::types::ChunkIndex;
use crate::Region;

use super::errors::SplitError;
use super::handler_trait::SplitSpliceHandler;
use super::tests_common::{blob_digest, make_ctx, wire};
use super::types::{InitSplitOutcome, MAX_CHUNK_BYTES};

#[tokio::test]
async fn split_init_append_finalize_happy_path() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let bd = blob_digest(b"blob-1");
    let init = w.handler.init_split(&ctx, &bd, "req-init").await.unwrap();
    let session_id = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("expected Started, got {other:?}"),
    };
    // Append two chunks.
    let c0 = Bytes::from_static(b"chunk-zero-bytes");
    let c1 = Bytes::from_static(b"chunk-one-bytes");
    let _d0 = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), c0.clone(), "req-c0")
        .await
        .unwrap();
    let _d1 = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(1), c1.clone(), "req-c1")
        .await
        .unwrap();
    // Finalize.
    let fin = w
        .handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();
    assert_eq!(fin.chunk_count, 2);
    assert!(!fin.idempotent);
    // Idempotent finalize echoes.
    let fin2 = w
        .handler
        .finalize_split(&ctx, session_id, "req-fin-2")
        .await
        .unwrap();
    assert_eq!(fin2.manifest_digest, fin.manifest_digest);
    assert!(fin2.idempotent);
    // Audit captured.
    assert_eq!(w.audit.snapshot_of(BlobEventType::SplitStart).len(), 1);
    assert_eq!(
        w.audit.snapshot_of(BlobEventType::SplitChunkAppended).len(),
        2
    );
    assert_eq!(w.audit.snapshot_of(BlobEventType::SplitFinalized).len(), 2);
}

#[tokio::test]
async fn split_init_idempotent_after_finalize() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(
        tenant,
        Region::Wnam,
        PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R),
    );
    let bd = blob_digest(b"blob-2");
    let init = w.handler.init_split(&ctx, &bd, "req-init").await.unwrap();
    let session_id = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("expected Started, got {other:?}"),
    };
    let c0 = Bytes::from_static(b"only-chunk");
    w.handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), c0, "req-c0")
        .await
        .unwrap();
    let fin = w
        .handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();
    // Re-init for the same blob_digest yields AlreadyChunked.
    let init2 = w.handler.init_split(&ctx, &bd, "req-init-2").await.unwrap();
    match init2 {
        InitSplitOutcome::AlreadyChunked {
            manifest_digest,
            chunk_count,
        } => {
            assert_eq!(manifest_digest, fin.manifest_digest);
            assert_eq!(chunk_count, 1);
        }
        other => panic!("expected AlreadyChunked, got {other:?}"),
    }
}

#[tokio::test]
async fn split_init_live_session_echo() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-live");
    let init = w.handler.init_split(&ctx, &bd, "req-init").await.unwrap();
    let session_id = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("expected Started, got {other:?}"),
    };
    let init2 = w.handler.init_split(&ctx, &bd, "req-init-2").await.unwrap();
    match init2 {
        InitSplitOutcome::LiveSessionEcho { session_id: id } => {
            assert_eq!(id, session_id);
        }
        other => panic!("expected LiveSessionEcho, got {other:?}"),
    }
}

#[tokio::test]
async fn cross_tenant_session_lookup_returns_not_found() {
    let w = wire(Region::Wnam);
    let tenant_a = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let tenant_b = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
    let ctx_a = make_ctx(tenant_a, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-xt");
    let init = w
        .handler
        .init_split(&ctx_a, &bd, "req-init-a")
        .await
        .unwrap();
    let session_id = match init {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("got {other:?}"),
    };
    // Tenant B tries to append to A's session.
    let err = w
        .handler
        .append_chunk(
            &ctx_b,
            session_id,
            ChunkIndex(0),
            Bytes::from_static(b"x"),
            "req-c0-b",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SplitError::SessionNotFound));
    assert_eq!(err.cor_code(), "COR_MULTIPART_SESSION_NOT_FOUND");
}

#[tokio::test]
async fn append_after_finalize_rejected() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-final");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    w.handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(0),
            Bytes::from_static(b"a"),
            "req",
        )
        .await
        .unwrap();
    w.handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();
    let err = w
        .handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(1),
            Bytes::from_static(b"b"),
            "req-late",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SplitError::SessionAlreadyFinalized));
}

#[tokio::test]
async fn abort_then_append_rejected() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-abort");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    w.handler
        .abort_split(&ctx, session_id, "req-abort")
        .await
        .unwrap();
    // Idempotent abort.
    w.handler
        .abort_split(&ctx, session_id, "req-abort-2")
        .await
        .unwrap();
    // Append rejected.
    let err = w
        .handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(0),
            Bytes::from_static(b"x"),
            "req-late",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SplitError::SessionAborted));
    assert_eq!(err.cor_code(), "COR_MULTIPART_SESSION_ABORTED");
}

#[tokio::test]
async fn finalize_then_abort_rejected() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-fa");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
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
    w.handler
        .finalize_split(&ctx, session_id, "req-fin")
        .await
        .unwrap();
    let err = w
        .handler
        .abort_split(&ctx, session_id, "req-abort")
        .await
        .unwrap_err();
    assert!(matches!(err, SplitError::SessionAlreadyFinalized));
}

#[tokio::test]
async fn append_chunk_ordering_violation_rejected() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-ord");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    // Out-of-order: try to append index 1 before 0.
    let err = w
        .handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(1),
            Bytes::from_static(b"x"),
            "req",
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        SplitError::ChunkOrderingViolation { index: 1, .. }
    ));
}

#[tokio::test]
async fn append_chunk_idempotent_same_bytes_ok() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-idem");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    let bytes = Bytes::from_static(b"the-same-bytes");
    let d0 = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), bytes.clone(), "req-1")
        .await
        .unwrap();
    let d0b = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), bytes.clone(), "req-2")
        .await
        .unwrap();
    assert_eq!(d0, d0b);
}

#[tokio::test]
async fn append_chunk_idempotent_different_bytes_rejected() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-bytes-mis");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    w.handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(0),
            Bytes::from_static(b"original"),
            "req-1",
        )
        .await
        .unwrap();
    let err = w
        .handler
        .append_chunk(
            &ctx,
            session_id,
            ChunkIndex(0),
            Bytes::from_static(b"DIFFERENT"),
            "req-2",
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        SplitError::ChunkOrderingViolation {
            reason: "bytes_mismatch",
            ..
        }
    ));
}

#[tokio::test]
async fn append_chunk_too_large_rejected() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-big");
    let session_id = match w.handler.init_split(&ctx, &bd, "req").await.unwrap() {
        InitSplitOutcome::Started { session_id } => session_id,
        other => panic!("{other:?}"),
    };
    let huge = Bytes::from(vec![0u8; MAX_CHUNK_BYTES + 1]);
    let err = w
        .handler
        .append_chunk(&ctx, session_id, ChunkIndex(0), huge, "req")
        .await
        .unwrap_err();
    assert!(matches!(err, SplitError::ChunkTooLarge { .. }));
    assert_eq!(err.cor_code(), "COR_MULTIPART_CHUNK_TOO_LARGE");
}

#[tokio::test]
async fn missing_scope_rejected_403_split() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Wnam, PatScopes::empty());
    let bd = blob_digest(b"blob-noscope");
    let err = w.handler.init_split(&ctx, &bd, "req").await.unwrap_err();
    assert!(matches!(
        err,
        SplitError::ScopeInsufficient {
            required: SCOPE_CACHE_W
        }
    ));
    assert_eq!(err.cor_code(), "COR_AUTH_SCOPE_INSUFFICIENT");
}

#[tokio::test]
async fn region_mismatch_returns_internal_split() {
    let w = wire(Region::Wnam);
    let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
    let ctx = make_ctx(tenant, Region::Weur, PatScopes::single(SCOPE_CACHE_W));
    let bd = blob_digest(b"blob-region");
    let err = w.handler.init_split(&ctx, &bd, "req").await.unwrap_err();
    assert!(matches!(err, SplitError::RegionMismatch { .. }));
}
