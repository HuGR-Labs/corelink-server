//! Property test: replaying the same evidence batch any number of
//! times produces exactly N Drata pushes (== batch size of unique
//! hashes), regardless of replay multiplicity or batch chunking.
//!
//! This is the canonical idempotency invariant — if it ever fires,
//! Drata receives duplicates and SOC 2 audit-log evidence ceases to
//! be replay-safe (CTRL-AUDIT-DRATA-001).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_ops::drata::{
    record_sha256, DrataClient, EvidenceRecord, EvidenceStream, IdempotencyLedger,
    InMemoryDrataClient, InMemoryIdempotencyLedger, InMemorySyncAuditSink, SyncAuditSink,
    SyncRunner,
};
use proptest::prelude::*;

fn streams() -> impl Strategy<Value = EvidenceStream> {
    prop_oneof![
        Just(EvidenceStream::AuditLogs),
        Just(EvidenceStream::AccessReviews),
        Just(EvidenceStream::CredentialManagement),
        Just(EvidenceStream::ChangeManagement),
        Just(EvidenceStream::IncidentResponse),
        Just(EvidenceStream::VulnerabilityManagement),
    ]
}

fn records() -> impl Strategy<Value = Vec<EvidenceRecord>> {
    proptest::collection::vec(
        (streams(), "[a-z0-9]{1,8}", 0i64..1_000_000, 0u8..4u8).prop_map(
            |(stream, id, ts, k_count)| {
                let meta: Vec<(String, String)> = (0..k_count)
                    .map(|i| (format!("k{i}"), format!("v{i}")))
                    .collect();
                EvidenceRecord::new(stream, id, ts, meta)
            },
        ),
        0..16,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Replaying the same batch arbitrarily many times must yield
    /// exactly `unique(hashes)` Drata pushes — never more.
    #[test]
    fn replay_count_does_not_inflate_drata_pushes(
        batch in records(),
        replays in 1u8..6u8,
    ) {
        let drata = Arc::new(InMemoryDrataClient::new());
        let ledger = Arc::new(InMemoryIdempotencyLedger::new());
        let audit = Arc::new(InMemorySyncAuditSink::new());
        let runner = SyncRunner::new(
            Arc::clone(&drata) as Arc<dyn DrataClient>,
            Arc::clone(&ledger) as Arc<dyn IdempotencyLedger>,
            Arc::clone(&audit) as Arc<dyn SyncAuditSink>,
            Arc::new(corelink_ops::drata::runner::FixedClock(0)),
            "drata-***",
        );

        let unique: std::collections::HashSet<String> = batch
            .iter()
            .map(|r| record_sha256(r).unwrap())
            .collect();

        for _ in 0..replays {
            runner.run_batch(&batch).unwrap();
        }

        prop_assert_eq!(drata.push_count(), unique.len());
        prop_assert_eq!(ledger.len(), unique.len());
    }

    /// Splitting the batch across multiple `run_batch` calls must not
    /// change the total push count (associativity).
    #[test]
    fn batch_splitting_is_associative(batch in records()) {
        let drata_one = Arc::new(InMemoryDrataClient::new());
        let ledger_one = Arc::new(InMemoryIdempotencyLedger::new());
        let audit_one = Arc::new(InMemorySyncAuditSink::new());
        let r_one = SyncRunner::new(
            Arc::clone(&drata_one) as Arc<dyn DrataClient>,
            Arc::clone(&ledger_one) as Arc<dyn IdempotencyLedger>,
            Arc::clone(&audit_one) as Arc<dyn SyncAuditSink>,
            Arc::new(corelink_ops::drata::runner::FixedClock(0)),
            "drata-***",
        );
        r_one.run_batch(&batch).unwrap();

        let drata_two = Arc::new(InMemoryDrataClient::new());
        let ledger_two = Arc::new(InMemoryIdempotencyLedger::new());
        let audit_two = Arc::new(InMemorySyncAuditSink::new());
        let r_two = SyncRunner::new(
            Arc::clone(&drata_two) as Arc<dyn DrataClient>,
            Arc::clone(&ledger_two) as Arc<dyn IdempotencyLedger>,
            Arc::clone(&audit_two) as Arc<dyn SyncAuditSink>,
            Arc::new(corelink_ops::drata::runner::FixedClock(0)),
            "drata-***",
        );
        for r in &batch {
            r_two.run_batch(std::slice::from_ref(r)).unwrap();
        }

        prop_assert_eq!(drata_one.push_count(), drata_two.push_count());
        prop_assert_eq!(ledger_one.len(), ledger_two.len());
    }
}
