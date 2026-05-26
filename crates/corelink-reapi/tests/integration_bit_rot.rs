//! WI-S02-006 — Bit rot integration test (FM-051) wired through the
//! integrated S-02 read-path stack and the WI-S02-003
//! `corelink-client-verify` SDK.
//!
//! ## Threat model
//!
//! `FM-051` is the canonical "R2 silent corruption" failure mode:
//! Cloudflare R2 returns bytes that do not BLAKE3-match the requested
//! digest. The defense is **client-side BLAKE3 verify default-on**
//! (CTRL-CAS-002 / `corelink-client-verify::VerifyConfig::new()`); a
//! bit-rot scenario MUST surface as `VerifyError::DigestMismatch` at
//! the SDK seam BEFORE the body reaches the application. The integrity
//! invariant is binary: 100 % of bit-rot scenarios are caught (WI §11
//! DoD).
//!
//! ## Why a hand-rolled corruption injector
//!
//! The host-side `InMemoryR2` fake exposes
//! [`InMemoryR2::inject_corrupt_for_test`] which mutates the value bytes
//! at a known canonical key while leaving the `blob_meta` row untouched
//! — the production-equivalent shape of the bit-rot bug.
//! `wrangler r2 object put --force corrupted-bytes` (the staging
//! analog called out in WI §6.1.2) is the integration-tier counterpart
//! that is exercised in the staging E2E pipeline; this host-side test
//! pins the SDK behavior at PR speed so a regression cannot land
//! without a CI failure.
//!
//! ## Scenarios (10 total, per WI §11)
//!
//! 1. Single-byte flip at offset 0.
//! 2. Single-byte flip mid-body.
//! 3. Single-byte flip at the last byte.
//! 4. 32-byte range zeroed in the middle.
//! 5. Body truncated by 1 byte.
//! 6. Body truncated to empty.
//! 7. Body extended by 1 trailing byte.
//! 8. Body replaced with a different known body of the same length.
//! 9. Body replaced with a different known body of a different length.
//! 10. Body bit-pattern inverted (`b ^= 0xFF` per byte).
//!
//! Every scenario MUST surface
//! [`corelink_client_verify::VerifyError::DigestMismatch`] when the
//! verifier checks the corrupted body against the **claimed** digest
//! (the digest of the ORIGINAL body — that's what the wire request
//! carries). `0` corrupted bodies must escape the verifier.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::type_complexity,
    reason = "test code: panics surface as test failures by design; type complexity is acceptable in test fixtures"
)]

use std::sync::Arc;

use bytes::Bytes;
use corelink_client_verify::{ClientVerifier, VerifyError};
use corelink_hash::{Digest, VerifiedBody};
use corelink_meta::{
    AuditEvent, AuditEventType, BlobMetaKey, CommitPutRequest, InMemoryMetaStore, MetaStore,
    RequestId,
};
use corelink_reapi::read::{CasReadOrchestrator, ReadOutcome};
use corelink_tenant_path::TenantDerivationKey;
use corelink_cas::r2_storage::{InMemoryR2, R2Reader, R2Writer};
use corelink_replication::region_resolver::{Region, TenantCtx};
use uuid::Uuid;
use zeroize::Zeroizing;

fn fixed_tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]))
}

fn ctx_for(seed: u64) -> TenantCtx {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&seed.to_le_bytes());
    TenantCtx::new(&fixed_tdk(), Uuid::from_bytes(bytes), Region::Wnam)
}

/// Drive a successful S-01 write so the bit-rot test has an alive R2
/// object + alive `blob_meta` row at a known canonical key.
async fn write_blob(
    backend: &Arc<InMemoryR2>,
    meta: &InMemoryMetaStore,
    ctx: &TenantCtx,
    body: &[u8],
    audit_id: u128,
) -> (Digest, String) {
    let digest = Digest::compute(body);
    let writer = R2Writer::new(Region::Wnam, Arc::clone(backend));
    let vb = VerifiedBody::new(Bytes::copy_from_slice(body), digest).unwrap();
    writer.put(ctx, &vb).await.unwrap();
    let key = BlobMetaKey::new(ctx.tenant_id(), digest);
    meta.commit_put(CommitPutRequest {
        key,
        size_bytes: body.len() as u64,
        now_ms: 1_700_000_000_000,
        audit: AuditEvent {
            id: Uuid::from_u128(audit_id),
            request_id: RequestId::new("req-bit-rot-write"),
            event_type: AuditEventType::CasPutCompleted,
            payload_json: r#"{"specversion":"1.0"}"#.to_owned(),
        },
    })
    .await
    .unwrap();

    // The canonical R2 key is private (`pub(crate)`); recover it via
    // the only-one-key-after-write invariant of the in-memory fake.
    let keys = backend.keys_snapshot();
    assert_eq!(keys.len(), 1, "post-write R2 must hold exactly one key");
    let key_str = keys
        .into_iter()
        .next()
        .expect("post-write key set is non-empty");
    (digest, key_str)
}

/// Run the read path end-to-end and return the bytes that the
/// orchestrator surfaces. Panics if the orchestrator errors or returns
/// `NotFound` — the test fixture always seeds an alive blob.
async fn read_bytes(
    backend: &Arc<InMemoryR2>,
    meta: &InMemoryMetaStore,
    ctx: &TenantCtx,
    digest: &Digest,
) -> Bytes {
    let reader = R2Reader::new(Region::Wnam, Arc::clone(backend));
    let orch = CasReadOrchestrator::new(&reader, meta);
    let out = orch.read_blob(ctx, digest).await.unwrap();
    match out {
        ReadOutcome::Hit { body, .. } => body,
        other => panic!("expected Hit (corrupt or not), got {other:?}"),
    }
}

/// Run the canonical 10-scenario battery. Asserts that every scenario
/// surfaces `VerifyError::DigestMismatch` from the WI-S02-003
/// `ClientVerifier::default_on()` instance.
#[tokio::test]
async fn bit_rot_10_scenarios_all_caught_by_client_verify() {
    let original_body = b"Hello, CoreLink CAS read-path bit-rot test!".to_vec();
    let scenarios: Vec<(&'static str, Box<dyn Fn(&[u8]) -> Vec<u8>>)> = vec![
        ("flip-byte-0", Box::new(|b: &[u8]| {
            let mut v = b.to_vec();
            v[0] ^= 0x01;
            v
        })),
        ("flip-byte-mid", Box::new(|b: &[u8]| {
            let mut v = b.to_vec();
            let m = v.len() / 2;
            v[m] ^= 0x40;
            v
        })),
        ("flip-byte-last", Box::new(|b: &[u8]| {
            let mut v = b.to_vec();
            let last = v.len() - 1;
            v[last] ^= 0x80;
            v
        })),
        ("zero-32-byte-range", Box::new(|b: &[u8]| {
            let mut v = b.to_vec();
            let start = 5.min(v.len());
            let end = (start + 32).min(v.len());
            for byte in &mut v[start..end] {
                *byte = 0;
            }
            v
        })),
        ("truncate-by-one", Box::new(|b: &[u8]| {
            let mut v = b.to_vec();
            v.pop();
            v
        })),
        ("truncate-to-empty", Box::new(|_b: &[u8]| Vec::new())),
        ("extend-by-one", Box::new(|b: &[u8]| {
            let mut v = b.to_vec();
            v.push(0xCC);
            v
        })),
        ("swap-same-length", Box::new(|b: &[u8]| {
            // Distinct body of the same length.
            (0..b.len()).map(|i| (b[i].wrapping_add(0xA5)) ^ 0x33).collect()
        })),
        ("swap-different-length", Box::new(|_b: &[u8]| {
            b"a totally unrelated payload".to_vec()
        })),
        ("invert-all-bits", Box::new(|b: &[u8]| {
            b.iter().map(|x| x ^ 0xFF).collect()
        })),
    ];

    for (label, mutator) in scenarios {
        // Fresh state per scenario so the keys snapshot is unambiguous.
        let backend = Arc::new(InMemoryR2::new());
        let meta = InMemoryMetaStore::new();
        let ctx = ctx_for(0xDEAD_BEEF_0000_0000);
        let (digest, canonical_key) =
            write_blob(&backend, &meta, &ctx, &original_body, 0x0193_84A0_FACE_7000_8000_0000_0000_FFFF).await;

        // Sanity: the body the verifier saw before injection must match.
        let pristine = read_bytes(&backend, &meta, &ctx, &digest).await;
        assert_eq!(pristine.as_ref(), original_body.as_slice(), "[{label}] pristine read drifted");
        let verifier = ClientVerifier::default_on();
        verifier
            .verify(pristine.as_ref(), &digest)
            .unwrap_or_else(|err| panic!("[{label}] pristine verify failed: {err:?}"));

        // Inject corruption.
        let corrupt = mutator(&original_body);
        let replaced =
            backend.inject_corrupt_for_test(&canonical_key, Bytes::copy_from_slice(&corrupt));
        assert!(replaced, "[{label}] corrupt injection did not overwrite");

        // Read post-corruption: the orchestrator returns whatever R2
        // hands back (it does NOT verify; that's the SDK's job).
        let post = read_bytes(&backend, &meta, &ctx, &digest).await;
        assert_eq!(post.as_ref(), corrupt.as_slice(), "[{label}] R2 did not propagate corruption");

        // SDK verify against the CLAIMED digest (= the digest of the
        // pristine body, which is what travels on the wire). MUST
        // surface `DigestMismatch`.
        let result = verifier.verify(post.as_ref(), &digest);
        match result {
            Err(VerifyError::DigestMismatch { expected, computed }) => {
                assert_eq!(
                    expected,
                    digest.to_hex(),
                    "[{label}] verifier reported the wrong expected digest"
                );
                assert_ne!(
                    computed,
                    digest.to_hex(),
                    "[{label}] computed digest must differ from expected"
                );
            }
            Err(other) => panic!("[{label}] expected DigestMismatch, got: {other:?}"),
            Ok(()) => panic!("[{label}] verifier ACCEPTED corrupted body — INV-CAS-INTEGRITY breach"),
        }
    }
}

/// Negative control: with the verifier explicitly disabled (opt-out
/// path; CTRL-CAS-002 documents the warning surface), the bit-rot
/// scenario MUST NOT raise `DigestMismatch`. This pins the opt-out
/// contract: a customer who explicitly disables verify accepts the
/// risk; the cache server cannot do anything about it. The bit-rot
/// detection rate THEREFORE depends on default-on; the WI §11 DoD
/// "100 % bit-rot caught" assumes the default-on path.
#[tokio::test]
async fn bit_rot_opt_out_does_not_raise_digest_mismatch() {
    let original_body = b"opt-out-control".to_vec();
    let backend = Arc::new(InMemoryR2::new());
    let meta = InMemoryMetaStore::new();
    let ctx = ctx_for(0xCAFE_F00D_DEAD_BEEF);
    let (digest, key) = write_blob(
        &backend,
        &meta,
        &ctx,
        &original_body,
        0x0193_84A0_FACE_7000_8000_0000_0000_FFFE,
    )
    .await;

    // Inject a flip.
    let mut corrupt = original_body.clone();
    corrupt[0] ^= 0x01;
    let replaced = backend.inject_corrupt_for_test(&key, Bytes::copy_from_slice(&corrupt));
    assert!(replaced);

    let post = read_bytes(&backend, &meta, &ctx, &digest).await;
    assert_eq!(post.as_ref(), corrupt.as_slice());

    // Disabled verifier MUST surface VerifyDisabled (NOT
    // DigestMismatch) — the SDK's documented opt-out behavior.
    let disabled = ClientVerifier::new(corelink_client_verify::VerifyConfig::disabled());
    let result = disabled.verify(post.as_ref(), &digest);
    match result {
        Err(VerifyError::VerifyDisabled) => {}
        Err(other) => panic!("expected VerifyDisabled, got {other:?}"),
        Ok(()) => panic!("disabled verifier returned Ok; surface contract drift"),
    }
}
