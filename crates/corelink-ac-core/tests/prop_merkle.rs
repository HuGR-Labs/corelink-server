//! Property tests at 10 000 iter (WI-S04-003 §10.s04.003.1).
//!
//! Six canonical properties:
//!
//! - `prop_merkle_determinism`: same input → same root, byte-equal,
//!   over many distinct inputs.
//! - `prop_merkle_root_independent_of_insertion_order`: shuffled
//!   `output_files` produces the identical root.
//! - `prop_merkle_tampering_detected`: any single byte flip in any
//!   leaf digest perturbs the root with probability 1.
//! - `prop_merkle_round_trip_codec_stable`: encode → decode → re-encode
//!   yields byte-identical output.
//! - `prop_outputs_missing_rejected`: a single tombstoned digest
//!   surfaces `OutputsCheckError::BlobMissing` 100 % of the time.
//! - `prop_bounds_enforcement`: `output_files.len() > MAX` always
//!   rejects with the canonical error variant.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::collections::HashSet;
use std::sync::Mutex;

use corelink_ac_core::{
    build_root, codec, BlobMetaReader, MerkleError, OutputsCheckError, OutputsValidator,
    StrictOutputsValidator, MAX_OUTPUT_FILES,
};
use corelink_ac_core::types::{
    AcEnvelope, ActionDigest, ActionResult, OutputFileDigest, MERKLE_ROOT_LEN, RESULT_HASH_LEN,
};
use corelink_hash::Digest;
use proptest::prelude::*;

#[derive(Debug)]
struct FixtureReader {
    alive: Mutex<HashSet<(String, [u8; 32])>>,
}

impl FixtureReader {
    fn new() -> Self {
        Self {
            alive: Mutex::new(HashSet::new()),
        }
    }

    fn add(&self, tenant: &str, d: &Digest) {
        let mut g = self.alive.lock().expect("fixture lock");
        g.insert((tenant.to_string(), *d.as_bytes()));
    }
}

impl BlobMetaReader for FixtureReader {
    type TenantId = String;

    fn batch_check_alive(
        &self,
        tenant_id: &String,
        digests: &[Digest],
    ) -> Result<Vec<bool>, OutputsCheckError> {
        let g = self.alive.lock().expect("fixture lock");
        Ok(digests
            .iter()
            .map(|d| g.contains(&(tenant_id.clone(), *d.as_bytes())))
            .collect())
    }
}

fn arb_digest() -> impl Strategy<Value = Digest> {
    proptest::collection::vec(any::<u8>(), 0..256).prop_map(|v| Digest::compute(&v))
}

fn arb_output_file() -> impl Strategy<Value = OutputFileDigest> {
    (arb_digest(), 1i64..=10_000_000).prop_map(|(d, sz)| OutputFileDigest::new(d, sz))
}

fn arb_result(min_files: usize, max_files: usize) -> impl Strategy<Value = ActionResult> {
    proptest::collection::vec(arb_output_file(), min_files..=max_files).prop_map(|files| {
        ActionResult::new(files, Vec::new(), 0, b"raw".to_vec())
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        ..ProptestConfig::default()
    })]

    /// Determinism: building the root from the same `ActionResult`
    /// twice yields identical bytes. The test inputs cover any
    /// 0..=64 file count drawn from the canonical strategy.
    #[test]
    fn prop_merkle_determinism(r in arb_result(0, 64)) {
        let r1 = build_root(&r).unwrap();
        let r2 = build_root(&r).unwrap();
        prop_assert_eq!(r1, r2);
    }

    /// Order independence: shuffled `output_files` produces the
    /// identical Merkle root (lex-sort canonicalization).
    #[test]
    fn prop_merkle_root_independent_of_insertion_order(
        mut r in arb_result(2, 64),
        seed in any::<u64>(),
    ) {
        let baseline = build_root(&r).unwrap();
        // Deterministic shuffle via the seed: rotate by `seed % len`.
        if !r.output_files.is_empty() {
            let n = r.output_files.len();
            let by = (seed as usize) % n;
            r.output_files.rotate_left(by);
        }
        let shuffled = build_root(&r).unwrap();
        prop_assert_eq!(baseline, shuffled);
    }

    /// Tampering: replacing any leaf digest with a different
    /// content-hash perturbs the root.
    #[test]
    fn prop_merkle_tampering_detected(
        r in arb_result(1, 32),
        idx_seed in any::<u64>(),
        replacement_payload in proptest::collection::vec(any::<u8>(), 1..256),
    ) {
        let baseline = build_root(&r).unwrap();
        let n = r.output_files.len();
        let i = (idx_seed as usize) % n;
        let mut tampered = r.clone();
        let new_d = Digest::compute(&replacement_payload);
        // Skip cases where the replacement happens to produce the
        // same digest (proptest will retry via prop_assume).
        prop_assume!(*tampered.output_files[i].digest.as_bytes() != *new_d.as_bytes());
        tampered.output_files[i] =
            OutputFileDigest::new(new_d, tampered.output_files[i].size_bytes);
        let tampered_root = build_root(&tampered).unwrap();
        prop_assert_ne!(baseline, tampered_root);
    }

    /// Codec round-trip is byte-stable across two re-encodings.
    #[test]
    fn prop_merkle_round_trip_codec_stable(r in arb_result(0, 16)) {
        let env = AcEnvelope::new(
            "00000000-0000-7000-8000-000000000000".to_string(),
            ActionDigest::new(Digest::compute(b"action"), 100),
            r,
            [0xAA; MERKLE_ROOT_LEN],
            [0xBB; RESULT_HASH_LEN],
            1_700_000_000_000,
            vec![1, 2, 3],
            42,
        );
        let b1 = codec::encode(&env).unwrap();
        let env_back = codec::decode(&b1).unwrap();
        let b2 = codec::encode(&env_back).unwrap();
        prop_assert_eq!(b1, b2);
        prop_assert_eq!(env, env_back);
    }

    /// One tombstoned digest in a fixture causes the strict
    /// validator to reject 100 % of the time.
    #[test]
    fn prop_outputs_missing_rejected(
        r in arb_result(2, 16),
        miss_idx_seed in any::<u64>(),
    ) {
        let n = r.output_files.len();
        let miss_idx = (miss_idx_seed as usize) % n;
        let reader = FixtureReader::new();
        // Add every digest EXCEPT the one at miss_idx.
        for (i, f) in r.output_files.iter().enumerate() {
            if i != miss_idx {
                reader.add("t", &f.digest);
            }
        }
        // If the tampered-out digest happens to ALSO be one of the
        // remaining (digest collision in a small input), prop_assume
        // away.
        let tombstoned = r.output_files[miss_idx].digest;
        let collisions = r
            .output_files
            .iter()
            .enumerate()
            .filter(|(i, f)| *i != miss_idx && f.digest.as_bytes() == tombstoned.as_bytes())
            .count();
        prop_assume!(collisions == 0);
        let v = StrictOutputsValidator::new(reader);
        let err = v.validate(&"t".to_string(), &r).unwrap_err();
        let is_missing = matches!(err, OutputsCheckError::BlobMissing { .. });
        prop_assert!(is_missing);
    }

    /// Bounds: any `output_files.len() > MAX_OUTPUT_FILES` rejects
    /// before any hashing occurs.
    #[test]
    fn prop_bounds_enforcement(
        excess in 1usize..=8,
    ) {
        // Build a synthetic excess slice. We don't hash; the bound
        // check fires first.
        let len = MAX_OUTPUT_FILES + excess;
        let mut files: Vec<OutputFileDigest> = Vec::with_capacity(len);
        for i in 0..len {
            files.push(OutputFileDigest::new(
                Digest::compute(format!("f{i}").as_bytes()),
                1,
            ));
        }
        let r = ActionResult::new(files, Vec::new(), 0, Vec::new());
        let err = build_root(&r).unwrap_err();
        let is_too_many = matches!(err, MerkleError::TooManyOutputFiles { .. });
        prop_assert!(is_too_many);
    }
}
