//! Codec round-trip tests (WI-S04-003 §10.s04.003.7).
//!
//! Pinned canonical envelopes traverse encode → decode → re-encode
//! and assert byte-equality + structural equality. The Merkle root
//! recorded in the envelope is recomputed from the carried result
//! and asserted equal so we catch any regression that desyncs the
//! `merkle_root` ↔ `result` binding.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use corelink_ac::types::{
    AcEnvelope, ActionDigest, ActionResult, OutputDirectoryDigest, OutputFileDigest,
    MERKLE_ROOT_LEN, RESULT_HASH_LEN,
};
use corelink_ac::{build_root, codec, compute_result_hash, verify_root};
use corelink_hash::Digest;

fn build_envelope(out_files: usize, out_dirs: usize, tenant: &str) -> AcEnvelope {
    let files = (0..out_files)
        .map(|i| {
            OutputFileDigest::new(
                Digest::compute(format!("{tenant}-file-{i}").as_bytes()),
                i64::try_from(i + 1).unwrap_or(1),
            )
        })
        .collect();
    let dirs = (0..out_dirs)
        .map(|i| {
            OutputDirectoryDigest::new(
                Digest::compute(format!("{tenant}-dir-{i}").as_bytes()),
                i64::try_from(i + 1).unwrap_or(1),
            )
        })
        .collect();
    let result = ActionResult::new(files, dirs, 0, b"raw".to_vec());
    let root = build_root(&result).unwrap();
    let result_hash = compute_result_hash(&root);
    AcEnvelope::new(
        tenant.to_string(),
        ActionDigest::new(Digest::compute(b"action"), 100),
        result,
        root,
        result_hash,
        1_700_000_000_000,
        vec![1, 2, 3, 4, 5],
        7,
    )
}

#[test]
fn round_trip_single_file_envelope() {
    let env = build_envelope(1, 0, "tenant-a");
    let b1 = codec::encode(&env).unwrap();
    let env_back = codec::decode(&b1).unwrap();
    assert_eq!(env, env_back);
    let b2 = codec::encode(&env_back).unwrap();
    assert_eq!(b1, b2);
}

#[test]
fn round_trip_mixed_files_and_dirs() {
    let env = build_envelope(7, 5, "tenant-x");
    let b1 = codec::encode(&env).unwrap();
    let env_back = codec::decode(&b1).unwrap();
    assert_eq!(env, env_back);
    // Verify the embedded merkle_root recomputes against the carried
    // result.
    verify_root(&env_back.result, &env_back.merkle_root).unwrap();
    let b2 = codec::encode(&env_back).unwrap();
    assert_eq!(b1, b2);
}

#[test]
fn round_trip_empty_outputs_envelope() {
    let env = build_envelope(0, 0, "tenant-empty");
    // Empty-output envelopes still round-trip; merkle_root = all-zeros
    // is the canonical sentinel.
    assert_eq!(env.merkle_root, [0u8; MERKLE_ROOT_LEN]);
    let b1 = codec::encode(&env).unwrap();
    let env_back = codec::decode(&b1).unwrap();
    assert_eq!(env, env_back);
    verify_root(&env_back.result, &env_back.merkle_root).unwrap();
}

#[test]
fn round_trip_three_distinct_tenants_disjoint_envelopes() {
    let a = build_envelope(3, 0, "alpha");
    let b = build_envelope(3, 0, "beta");
    let c = build_envelope(3, 0, "gamma");
    // Different tenant_ids + different file payload seeds yield
    // distinct envelopes (and distinct merkle_roots).
    assert_ne!(a.tenant_id, b.tenant_id);
    assert_ne!(b.tenant_id, c.tenant_id);
    assert_ne!(a.merkle_root, b.merkle_root);
    assert_ne!(b.merkle_root, c.merkle_root);
    // Each round-trips byte-stable independently.
    for env in [&a, &b, &c] {
        let b1 = codec::encode(env).unwrap();
        let env_back = codec::decode(&b1).unwrap();
        let b2 = codec::encode(&env_back).unwrap();
        assert_eq!(b1, b2);
    }
}

#[test]
fn result_hash_field_matches_canonical_derivation() {
    let env = build_envelope(4, 2, "tenant");
    let computed = compute_result_hash(&env.merkle_root);
    assert_eq!(env.result_hash, computed);
    // And after round-trip.
    let b = codec::encode(&env).unwrap();
    let back = codec::decode(&b).unwrap();
    assert_eq!(back.result_hash, compute_result_hash(&back.merkle_root));
}

#[test]
fn rebuilt_root_matches_envelope_merkle_root() {
    let env = build_envelope(8, 0, "tenant");
    let rebuilt = build_root(&env.result).unwrap();
    assert_eq!(rebuilt, env.merkle_root);
    let result_hash = compute_result_hash(&rebuilt);
    assert_eq!(result_hash, env.result_hash);
}

#[test]
fn codec_rejects_envelope_with_mutated_version_byte() {
    let env = build_envelope(1, 0, "tenant");
    let mut b = codec::encode(&env).unwrap();
    // Find the canonical "version":1 substring and patch to 2.
    let needle = b"\"version\":1";
    let pos = b
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("version key present");
    let one_offset = pos + needle.len() - 1; // point at the '1'
    b[one_offset] = b'2';
    let err = codec::decode(&b).unwrap_err();
    assert!(matches!(
        err,
        corelink_ac::MerkleError::VersionUnsupported(2)
    ));
}

#[test]
fn output_count_round_trip_matches() {
    let env = build_envelope(3, 4, "tenant");
    let env_back = codec::decode(&codec::encode(&env).unwrap()).unwrap();
    assert_eq!(env_back.result.output_count(), 7);
    assert_eq!(env.result.output_count(), env_back.result.output_count());
}

#[test]
fn merkle_root_and_result_hash_lengths_pinned() {
    let env = build_envelope(2, 0, "tenant");
    assert_eq!(env.merkle_root.len(), MERKLE_ROOT_LEN);
    assert_eq!(env.result_hash.len(), RESULT_HASH_LEN);
    assert_eq!(MERKLE_ROOT_LEN, 32);
    assert_eq!(RESULT_HASH_LEN, 32);
}
