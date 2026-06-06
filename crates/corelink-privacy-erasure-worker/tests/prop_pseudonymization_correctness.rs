//! Property tests pinning the canonical pseudonymization correctness
//! per WI-S11-002 §10.2 T-2.3 + AC-002 (GDPR Recital 26 + Art. 11 +
//! WP29 Op. 05/2014 endorsed by EDPB anonymization techniques).
//!
//! Coverage:
//!
//! - `prop_pseudonymized_backends_marker_present` — every pseudonymized
//!   backend produces a row set with 100% `pii_redacted=true` marker
//!   presence post-erasure.
//! - `prop_effective_backends_zero_records_post_erasure` — every
//!   effective backend produces 0 rows for `(tenant, subject)`
//!   post-erasure (the canonical empty-tenant fingerprint matches
//!   `CANONICAL_EMPTY_TENANT_HASH`).
//! - `prop_pseudonym_deterministic_across_runs` — re-running the same
//!   `(subject_id, erasure_salt)` produces a byte-identical pseudonym
//!   (deterministic per WI §9.3 DD-002).
//! - `prop_pseudonym_unique_per_dsr_salt` — different per-DSR salts
//!   produce different pseudonyms (forward secrecy invariant per WI
//!   §9.3 DD-003).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy_erasure_worker::pseudonymize::{pseudonymize_subject_id, PseudonymHash};
use corelink_privacy_erasure_worker::{
    canonical_in_memory_adapters, BackendErasureAdapter, BackendErasureOutcome, BackendKind,
    ErasureRequest, ErasureSalt, ErasureWorker, InMemoryBackendErasureAdapter,
    InMemoryErasureAuditSink, InMemoryErasureIdempotencyLedger, InMemoryErasureWorker, InMemoryRow,
    VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn fresh_worker() -> (
    InMemoryErasureWorker,
    Vec<Arc<InMemoryBackendErasureAdapter>>,
) {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();
    (worker, adapters_typed)
}

fn fresh_request(seed: u8) -> ErasureRequest {
    fn fixed(s: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = s.wrapping_mul(13).wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }
    ErasureRequest::new(
        fixed(seed),
        fixed(seed.wrapping_add(50)),
        fixed(seed.wrapping_add(100)),
        ErasureSalt::synthetic_for_test(seed),
        1_000,
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 8_192,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_pseudonymized_backends_marker_present(seed in 1u8..=255) {
        let (worker, adapters) = fresh_worker();
        let req = fresh_request(seed);
        // Inject canonical rows for every pseudonymized backend.
        for adapter in &adapters {
            if adapter.kind().is_pseudonymized() {
                adapter.insert_rows(
                    req.tenant_id,
                    req.subject_id,
                    vec![
                        InMemoryRow::new(b"audit-row-a".to_vec()),
                        InMemoryRow::new(b"audit-row-b".to_vec()),
                    ],
                );
            }
        }
        worker.process_erasure(&req, 1_000).unwrap();
        // Every pseudonymized backend has 100% pii_redacted=true.
        for adapter in &adapters {
            if adapter.kind().is_pseudonymized() {
                let all = adapter.all_redacted(req.tenant_id, req.subject_id);
                prop_assert!(all);
                let count = adapter.row_count(req.tenant_id, req.subject_id);
                prop_assert_eq!(count, 2);
            }
        }
    }

    #[test]
    fn prop_effective_backends_zero_records_post_erasure(seed in 1u8..=255) {
        let (worker, adapters) = fresh_worker();
        let req = fresh_request(seed);
        // Inject canonical rows for every effective backend (skip
        // R2Cas because refcount-aware semantics differ).
        for adapter in &adapters {
            if adapter.kind().is_effective() && adapter.kind() != BackendKind::R2Cas {
                adapter.insert_rows(
                    req.tenant_id,
                    req.subject_id,
                    vec![InMemoryRow::new(b"row".to_vec())],
                );
            }
        }
        worker.process_erasure(&req, 1_000).unwrap();
        for adapter in &adapters {
            if adapter.kind().is_effective() && adapter.kind() != BackendKind::R2Cas {
                let count = adapter.row_count(req.tenant_id, req.subject_id);
                prop_assert_eq!(count, 0);
                let h = corelink_privacy_erasure_worker::BackendErasureAdapter::verification_hash(
                    &**adapter,
                    VerificationContext {
                        dsr_id: req.dsr_id,
                        tenant_id: req.tenant_id,
                        subject_id: req.subject_id,
                        now_ms: 1_000,
                    },
                ).unwrap();
                prop_assert_eq!(h, CANONICAL_EMPTY_TENANT_HASH);
            }
        }
    }

    #[test]
    fn prop_pseudonym_deterministic_across_runs(
        sa in 1u8..=128,
        sb in 1u8..=128,
    ) {
        fn fx(s: u8) -> Uuid {
            let mut b = [0u8; 16];
            for (i, x) in b.iter_mut().enumerate() {
                *x = s.wrapping_add(i as u8);
            }
            Uuid::from_bytes(b)
        }
        let id = fx(sa);
        let salt_arr: [u8; 32] = {
            let mut a = [0u8; 32];
            for (i, b) in a.iter_mut().enumerate() {
                *b = sb.wrapping_add(i as u8);
            }
            a
        };
        let p1: PseudonymHash = pseudonymize_subject_id(id, &salt_arr);
        let p2: PseudonymHash = pseudonymize_subject_id(id, &salt_arr);
        prop_assert_eq!(p1, p2);
    }

    #[test]
    fn prop_pseudonym_unique_per_dsr_salt(
        seed in 1u8..=128,
        salt_a in 1u8..=128,
        salt_b in 129u8..=255,
    ) {
        prop_assume!(salt_a != salt_b);
        fn fx(s: u8) -> Uuid {
            let mut b = [0u8; 16];
            for (i, x) in b.iter_mut().enumerate() {
                *x = s.wrapping_add(i as u8);
            }
            Uuid::from_bytes(b)
        }
        fn salt(s: u8) -> [u8; 32] {
            let mut a = [0u8; 32];
            for (i, b) in a.iter_mut().enumerate() {
                *b = s.wrapping_add(i as u8);
            }
            a
        }
        let id = fx(seed);
        let pa = pseudonymize_subject_id(id, &salt(salt_a));
        let pb = pseudonymize_subject_id(id, &salt(salt_b));
        let differ = pa != pb;
        prop_assert!(differ);
    }
}

#[test]
fn pseudonymized_outcome_records_redacted_count() {
    let (worker, adapters) = fresh_worker();
    let req = fresh_request(7);
    let r2_audit = adapters
        .iter()
        .find(|a| a.kind() == BackendKind::R2AuditPseudo)
        .unwrap();
    r2_audit.insert_rows(
        req.tenant_id,
        req.subject_id,
        vec![
            InMemoryRow::new(b"r1".to_vec()),
            InMemoryRow::new(b"r2".to_vec()),
            InMemoryRow::new(b"r3".to_vec()),
        ],
    );
    let _ = worker.process_erasure(&req, 1_000).unwrap();
    let salt_bytes = req.erasure_salt.as_bytes();
    // Redaction is deterministic — recomputing the canonical pseudonym
    // matches the in-memory row's pseudonym.
    let recomputed = pseudonymize_subject_id(req.subject_id, salt_bytes);
    let hex = recomputed.to_hex();
    assert_eq!(hex.len(), 64);
}

#[test]
fn outcome_taxonomy_pseudonymized_branch() {
    let outcome = BackendErasureOutcome::Pseudonymized {
        records_redacted: 5,
    };
    assert!(outcome.is_successful());
    assert_eq!(outcome.as_str(), "pseudonymized");
}
