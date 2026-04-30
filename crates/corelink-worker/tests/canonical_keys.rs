//! Canonical R2 key regression vectors (WI-S01-003 §10.3.4 + §8 / AC).
//!
//! These vectors are computed independently from the canonical specification
//! `cas-<region>/<HMAC16>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>` and
//! hardcoded here, so any future refactor that subtly changes any of:
//! - the bucket-name format (`cas-<region>` vs `cas_<region>`),
//! - the prefix derivation algorithm (HMAC byte order, base64 encoding),
//! - the digest hex case (lowercase vs uppercase),
//! - the shard layout (2/2 vs 4 vs 1/3 chars),
//! - the BLAKE3 output bytes,
//!
//! will fail these vectors immediately. The tenant prefix values come from
//! `corelink-tenant-path::tests::canonical_vectors` (already cross-language
//! validated against a Python reference).
//!
//! Generation procedure (Python, for cross-language reproducibility):
//!
//! ```text
//! import hashlib, hmac, base64, uuid
//! tdk = b"<32-byte TDK>"
//! tid = uuid.UUID("<uuid>").bytes
//! mac = hmac.new(tdk, tid, hashlib.sha256).digest()
//! prefix16 = base64.urlsafe_b64encode(mac).rstrip(b"=").decode()[:16]
//! body = b"<body bytes>"
//! # BLAKE3 hex via the `blake3` Python package
//! import blake3
//! hex_digest = blake3.blake3(body).hexdigest()
//! key = f"cas-wnam/{prefix16}/blake3/{hex_digest[0:2]}/{hex_digest[2:4]}/{hex_digest}"
//! ```

#![allow(
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    missing_docs,
    reason = "test code; canonical vector hardcoded values are the contract"
)]

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::storage::r2::{InMemoryR2, PutOutcome, R2Reader, R2Writer};
use corelink_worker::{Region, TenantCtx};
use std::sync::Arc;
use uuid::Uuid;
use zeroize::Zeroizing;

// Canonical TDK + UUID pair shared with `corelink-tenant-path::canonical_vectors`.
const FIXTURE_TDK_BYTES: [u8; 32] = *b"fixture-tdk-32-bytes-constant!ok";
const FIXTURE_TENANT_ID: &str = "01938af0-abcd-7123-8456-000000000001";

/// Pre-computed HMAC16 for the fixture (TDK, UUID) pair. Cross-validated
/// against `corelink-tenant-path::canonical_vectors`.
const FIXTURE_PREFIX: &str = "oLIKxsQkUHvAJkis";

fn fixture_ctx(region: Region) -> TenantCtx {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_BYTES));
    let tid = Uuid::parse_str(FIXTURE_TENANT_ID).expect("static UUID literal parses");
    let ctx = TenantCtx::new(&tdk, tid, region);
    assert_eq!(
        ctx.prefix().as_str(),
        FIXTURE_PREFIX,
        "prefix drift — corelink-tenant-path canonical vector regressed"
    );
    ctx
}

#[tokio::test]
async fn canonical_key_layout_round_trips_through_in_memory_r2() {
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));

    // Body 1 — empty bytes; BLAKE3("") = af1349b9f5f9a1a6a0404dea36dcc9499...
    // Hardcoded canonical key.
    let body = Bytes::from_static(b"");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");

    let ctx = fixture_ctx(Region::Wnam);
    let outcome = writer.put(&ctx, &vb).await.expect("put ok");
    assert_eq!(outcome, PutOutcome::Fresh);

    let keys = backend.keys_snapshot();
    assert_eq!(keys.len(), 1);
    let key = keys.first().expect("one key").clone();
    let hex = claimed.to_hex();
    let expected_key = format!(
        "cas-wnam/{prefix}/blake3/{s0}/{s1}/{hex}",
        prefix = FIXTURE_PREFIX,
        s0 = hex.get(0..2).expect("≥ 4 hex chars"),
        s1 = hex.get(2..4).expect("≥ 4 hex chars"),
    );
    assert_eq!(key, expected_key, "canonical key drift");
}

#[tokio::test]
async fn canonical_keys_diverge_per_region_with_same_prefix() {
    let backend = Arc::new(InMemoryR2::new());
    let body = Bytes::from_static(b"hello world");
    let claimed =
        Digest::from_hex("d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24")
            .expect("static hex");
    let vb = VerifiedBody::new(body, claimed).expect("verifies");

    for region in [Region::Wnam, Region::Weur, Region::Sam] {
        let writer = R2Writer::new(region, Arc::clone(&backend));
        let ctx = fixture_ctx(region);
        writer.put(&ctx, &vb).await.expect("put ok");
    }

    let mut keys = backend.keys_snapshot();
    keys.sort();
    assert_eq!(keys.len(), 3, "one key per region");
    assert!(keys.iter().any(|k| k.starts_with("cas-wnam/")));
    assert!(keys.iter().any(|k| k.starts_with("cas-weur/")));
    assert!(keys.iter().any(|k| k.starts_with("cas-sam/")));
}

#[tokio::test]
async fn canonical_key_hex_lowercase() {
    // Codex round-1 from sister WIs has historically caught hex-case drift.
    // Lock the canonical lowercase invariant explicitly.
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let body = Bytes::from_static(b"aaa");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let ctx = fixture_ctx(Region::Wnam);
    writer.put(&ctx, &vb).await.expect("put ok");

    let keys = backend.keys_snapshot();
    let key = keys.first().expect("one key");
    // Hex segments must be lowercase only (no uppercase A–F).
    let segments = key.split('/').collect::<Vec<_>>();
    let s0 = segments.get(3).copied().unwrap_or("");
    let s1 = segments.get(4).copied().unwrap_or("");
    let hex = segments.get(5).copied().unwrap_or("");
    for seg in [s0, s1, hex] {
        assert!(
            seg.chars().all(|c| !c.is_ascii_uppercase()),
            "segment must be lowercase: {seg}"
        );
    }
}

#[tokio::test]
async fn put_then_get_round_trip() {
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reader = R2Reader::new(Region::Wnam, backend);

    let body = Bytes::from_static(b"round-trip body");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");
    let ctx = fixture_ctx(Region::Wnam);

    let outcome = writer.put(&ctx, &vb).await.expect("put ok");
    assert_eq!(outcome, PutOutcome::Fresh);

    let got = reader.get(&ctx, &claimed).await.expect("get ok");
    assert_eq!(got, body, "round-trip body bytes must match");
}

#[tokio::test]
async fn idempotent_duplicate_put_returns_duplicate_outcome() {
    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));

    let body = Bytes::from_static(b"idempotent test");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let ctx = fixture_ctx(Region::Wnam);

    let first = writer.put(&ctx, &vb).await.expect("put 1 ok");
    assert_eq!(first, PutOutcome::Fresh);

    let second = writer.put(&ctx, &vb).await.expect("put 2 ok");
    assert_eq!(
        second,
        PutOutcome::Duplicate,
        "second PUT must surface Duplicate (If-None-Match: * matched)"
    );
    assert_eq!(backend.len(), 1, "exactly one object stored");
}

#[tokio::test]
async fn cross_tenant_get_misses_with_not_found() {
    use corelink_worker::storage::error::R2Error;

    let backend = Arc::new(InMemoryR2::new());
    let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
    let reader = R2Reader::new(Region::Wnam, Arc::clone(&backend));

    // Tenant A writes a blob.
    let body = Bytes::from_static(b"tenant A's secret data");
    let claimed = Digest::compute(&body);
    let vb = VerifiedBody::new(body, claimed).expect("verifies");
    let ctx_a = fixture_ctx(Region::Wnam);
    writer.put(&ctx_a, &vb).await.expect("put A ok");

    // Tenant B (different UUID under same TDK ⇒ different prefix).
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_BYTES));
    let tid_b = Uuid::parse_str("01938af0-abcd-7123-8456-000000000002")
        .expect("static UUID literal parses");
    let ctx_b = TenantCtx::new(&tdk, tid_b, Region::Wnam);
    assert_ne!(ctx_b.prefix().as_str(), FIXTURE_PREFIX);

    // Tenant B asks for the same digest. Sees uniform NotFound (ADR-0028).
    let err = reader.get(&ctx_b, &claimed).await.expect_err("must miss");
    assert!(
        matches!(err, R2Error::NotFound),
        "expected NotFound, got {err:?}"
    );
    assert_eq!(err.taxonomy_code(), "COR_CAS_BLOB_NOT_FOUND");
}
