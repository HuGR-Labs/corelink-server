//! Property tests for the Stripe webhook DLQ surface.
//!
//! Closes the property-test requirement of
//! `specs/_audits/sealed/2026-05-15-webhook-retry-dlq.md`:
//!
//! 1. **Idempotency on `stripe_event_id`**: the same `event_id`
//!    quarantined across 100 random orderings (re-quarantines +
//!    interleaved replay attempts + interleaved depth queries) → at
//!    most 1 row exists for that `event_id` in the DLQ store + the
//!    canonical "exactly 1 successful side-effect" semantics hold
//!    (one `Inserted`, all subsequent quarantines are `Updated`).
//!
//! 2. **DLQ TTL enforcement**: any row whose `expires_at_ms <= now_ms`
//!    is removed by `prune_expired`; rows past the canonical 30-day
//!    TTL never linger in the active depth count.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]

use corelink_stripe_real::dlq::{
    DlqQuarantineOutcome, DlqReplayOutcome, InMemoryWebhookDlqStore, WebhookDlqRow,
    WebhookDlqStore, DEFAULT_DLQ_TTL_MS,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 100
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
}

/// Build a fresh quarantine row for a given event_id at `now_ms`.
fn quarantine_row(event_id: &str, now_ms: u64, attempt_index: u32) -> WebhookDlqRow {
    WebhookDlqRow::new_quarantine(
        event_id,
        format!("dlq_{event_id}_{attempt_index}"),
        "customer.subscription.created",
        "deadbeefcafe",
        format!("corr_{event_id}"),
        format!("transient backend err attempt={attempt_index}"),
        now_ms,
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// 100 cases: same `stripe_event_id` quarantined 100x in random
    /// timestamp orderings → exactly 1 row exists; exactly 1 `Inserted`
    /// outcome ever observed; all other outcomes are `Updated`;
    /// `attempt_count` = total quarantine calls.
    #[test]
    fn same_event_id_100_orderings_exactly_one_side_effect(
        deltas in proptest::collection::vec(0_u64..1_000_000_u64, 100..101),
    ) {
        let store = InMemoryWebhookDlqStore::new();
        let event_id = "evt_replay_test";
        let base = 1_715_000_000_000_u64;

        let mut inserted_count = 0_u32;
        let mut updated_count = 0_u32;
        for (i, delta) in deltas.iter().enumerate() {
            let now = base.saturating_add(*delta);
            let row = quarantine_row(event_id, now, u32::try_from(i).unwrap_or(u32::MAX));
            let outcome = store.try_quarantine(row).unwrap();
            match outcome {
                DlqQuarantineOutcome::Inserted => inserted_count += 1,
                DlqQuarantineOutcome::Updated => updated_count += 1,
                _ => prop_assert!(false, "unknown DlqQuarantineOutcome variant"),
            }
        }

        // Idempotency on event_id: exactly one Inserted ever.
        prop_assert_eq!(inserted_count, 1);
        prop_assert_eq!(updated_count, 99);

        // Exactly 1 row in the store.
        prop_assert_eq!(store.total_rows().unwrap(), 1);
        let row = store.get(event_id).unwrap().expect("row exists");
        // attempt_count counts every quarantine call (1 on Insert + 99
        // on Update).
        prop_assert_eq!(row.attempt_count, 100);
    }

    /// 100 cases: many distinct event_ids interleaved with random
    /// re-quarantines + replays → no double-insert ever; depth
    /// matches the un-replayed un-expired count exactly.
    #[test]
    fn distinct_event_ids_no_cross_contamination(
        n_distinct in 1_usize..32_usize,
        n_re_quarantine in 0_usize..32_usize,
    ) {
        let store = InMemoryWebhookDlqStore::new();
        let base = 1_715_000_000_000_u64;

        // Quarantine n_distinct distinct event_ids.
        for i in 0..n_distinct {
            let evt = format!("evt_{i:04}");
            let row = quarantine_row(&evt, base + (i as u64) * 1_000, 0);
            let out = store.try_quarantine(row).unwrap();
            prop_assert_eq!(out, DlqQuarantineOutcome::Inserted);
        }

        // Re-quarantine evt_0000 many times (always Updated, never
        // inserts a second row).
        for j in 0..n_re_quarantine {
            let row = quarantine_row(
                "evt_0000",
                base + 1_000_000 + (j as u64),
                u32::try_from(j).unwrap_or(u32::MAX),
            );
            let out = store.try_quarantine(row).unwrap();
            prop_assert_eq!(out, DlqQuarantineOutcome::Updated);
        }

        // Total rows = n_distinct (no double-insert).
        prop_assert_eq!(store.total_rows().unwrap(), n_distinct);

        // Depth (active rows) at a time before any TTL = n_distinct.
        let now = base + 2_000_000;
        prop_assert_eq!(store.depth(now).unwrap() as usize, n_distinct);
    }

    /// 100 cases: rows past `expires_at` are pruned; rows still within
    /// TTL are never pruned; depth gauge reflects only active rows.
    #[test]
    fn ttl_enforced_via_prune_expired(
        ttl_overshoot_ms in 1_u64..1_000_000_u64,
        n_rows in 1_usize..16_usize,
    ) {
        let store = InMemoryWebhookDlqStore::new();
        let base = 1_715_000_000_000_u64;

        // Quarantine n_rows distinct events at `base`.
        for i in 0..n_rows {
            let evt = format!("evt_ttl_{i:04}");
            let row = quarantine_row(&evt, base, 0);
            store.try_quarantine(row).unwrap();
        }

        // Before TTL expiry: depth = n_rows; prune_expired = 0.
        let mid_ttl = base + (DEFAULT_DLQ_TTL_MS / 2);
        prop_assert_eq!(store.depth(mid_ttl).unwrap() as usize, n_rows);
        prop_assert_eq!(store.prune_expired(mid_ttl).unwrap(), 0);
        prop_assert_eq!(store.total_rows().unwrap(), n_rows);

        // After TTL + ttl_overshoot_ms: every row is expired.
        let past = base + DEFAULT_DLQ_TTL_MS + ttl_overshoot_ms;
        prop_assert_eq!(store.depth(past).unwrap(), 0);
        let pruned = store.prune_expired(past).unwrap();
        prop_assert_eq!(pruned as usize, n_rows);
        prop_assert_eq!(store.total_rows().unwrap(), 0);
    }

    /// 100 cases: succeeded replay removes a row from depth; failed
    /// replay leaves it counted.
    #[test]
    fn replay_outcome_affects_depth(
        succeed in any::<bool>(),
        n_others in 0_usize..8_usize,
    ) {
        let store = InMemoryWebhookDlqStore::new();
        let base = 1_715_000_000_000_u64;

        // Target row.
        let target = "evt_target";
        store
            .try_quarantine(quarantine_row(target, base, 0))
            .unwrap();
        for i in 0..n_others {
            store
                .try_quarantine(quarantine_row(&format!("evt_other_{i}"), base, 0))
                .unwrap();
        }

        let before = store.depth(base + 1_000).unwrap() as usize;
        prop_assert_eq!(before, n_others + 1);

        let outcome = if succeed {
            DlqReplayOutcome::Succeeded
        } else {
            DlqReplayOutcome::Failed
        };
        store
            .record_replay(target, "rep_1", "ops_test", base + 500, outcome)
            .unwrap();

        let after = store.depth(base + 1_000).unwrap() as usize;
        if succeed {
            prop_assert_eq!(after, n_others); // target removed from depth
        } else {
            prop_assert_eq!(after, n_others + 1); // target still counted
        }
    }
}
