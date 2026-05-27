//! Wave-16 DSR completion statuspage publish job — pure aggregator.
//!
//! Closes the status-page wiring deferred at wave-15 per
//! `specs/_audits/sealed/2026-05-15-dsr-worker-production.md` §5 +
//! WI-S11-002 §6.
//!
//! # Surface
//!
//! - [`DsrCompletionStats`] — 24h-rolling aggregate over a slice of
//!   [`crate::verification_job::VerificationOutcome`]s. Pure-logic;
//!   wasm32-clean (no transport, no crypto, no clock).
//! - [`aggregate_24h_window`] — canonical aggregator.
//! - [`p95_of_observations`] — canonical p95 implementation (nearest-
//!   rank on sorted observations).
//!
//! # Wiring boundary
//!
//! The pure aggregator lives in this crate so the worker's
//! `wasm32-unknown-unknown` build target stays clean (no `reqwest` dep
//! creep). The Atlassian Statuspage real HTTP wiring lives in
//! `corelink-statuspage-real`; the
//! `corelink-statuspage-real::dsr_bridge` module bridges the two by
//! converting [`DsrCompletionStats`] into the canonical
//! `DsrCompletionReport` payload + publishing through the
//! `StatuspageBackend` trait.
//!
//! # SLI alignment
//!
//! The `p95_resolution_hours` field aggregates the same observations
//! the verification job emits to the `corelink_dsr_resolution_hours`
//! histogram (wave-15 binding). The aggregator is unit-tested against
//! [`crate::verification_job::dsr_resolution_hours`] so the SLO
//! catalog ↔ status-page numbers are regression-pinned.

use crate::event::ErasureDecision;
use crate::verification_job::{dsr_resolution_hours, VerificationOutcome};

/// 24h-rolling DSR completion stats aggregate.
///
/// Constructed by [`aggregate_24h_window`]; the bridge in
/// `corelink-statuspage-real::dsr_bridge` converts this into the
/// canonical `DsrCompletionReport` Statuspage payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DsrCompletionStats {
    /// Window start (Unix epoch seconds).
    pub window_start_unix_s: u64,
    /// Window end (Unix epoch seconds; must equal
    /// `window_start_unix_s + 86_400`).
    pub window_end_unix_s: u64,
    /// Count of outcomes that resolved to
    /// [`ErasureDecision::VerifiedComplete`].
    pub verified_complete_count: u64,
    /// Count of outcomes that resolved to
    /// [`ErasureDecision::VerifiedPartial`].
    pub verified_partial_count: u64,
    /// Count of outcomes that resolved to
    /// [`ErasureDecision::SlaBreached`].
    pub sla_breached_count: u64,
    /// p95 of the `dsr_resolution_hours` observations across both
    /// `VerifiedComplete` + `VerifiedPartial` outcomes. Empty window →
    /// `0` (Statuspage rejects negative + null values; 0 is the
    /// canonical "no data" sentinel matching the SLO bucket
    /// numerator lower bound).
    pub p95_resolution_hours: u64,
}

/// Aggregate a slice of [`VerificationOutcome`]s into a single
/// 24h-rolling stats summary.
///
/// `window_start_unix_s` is the canonical window start (the publish
/// job picks `now - 86_400`); only outcomes whose signed report
/// `verified_at_ms / 1_000` falls in `[window_start_unix_s,
/// window_start_unix_s + 86_400)` are counted.
#[must_use]
pub fn aggregate_24h_window(
    outcomes: &[VerificationOutcome],
    window_start_unix_s: u64,
) -> DsrCompletionStats {
    let window_end_unix_s = window_start_unix_s.saturating_add(86_400);
    let mut verified_complete_count: u64 = 0;
    let mut verified_partial_count: u64 = 0;
    let mut sla_breached_count: u64 = 0;
    let mut observations: Vec<u64> = Vec::new();

    for o in outcomes {
        // Filter on report verified_at_ms (for VerifiedComplete /
        // VerifiedPartial) or — for SlaBreached — the verification ts
        // is implicit in the decision (no signed report attached).
        // The publish job feeds outcomes already filtered to the
        // window, but the aggregator double-checks via the report ts
        // when available.
        match &o.decision {
            ErasureDecision::VerifiedComplete { .. } => {
                if let Some(rep) = &o.report {
                    let ts_s = rep.verified_at_ms / 1_000;
                    if ts_s >= window_start_unix_s && ts_s < window_end_unix_s {
                        verified_complete_count =
                            verified_complete_count.saturating_add(1);
                        if let Some(h) =
                            dsr_resolution_hours(rep.plan.generated_at_ms, rep.verified_at_ms)
                        {
                            observations.push(h);
                        }
                    }
                }
            }
            ErasureDecision::VerifiedPartial { .. } => {
                if let Some(rep) = &o.report {
                    let ts_s = rep.verified_at_ms / 1_000;
                    if ts_s >= window_start_unix_s && ts_s < window_end_unix_s {
                        verified_partial_count = verified_partial_count.saturating_add(1);
                        if let Some(h) =
                            dsr_resolution_hours(rep.plan.generated_at_ms, rep.verified_at_ms)
                        {
                            observations.push(h);
                        }
                    }
                }
            }
            ErasureDecision::SlaBreached { .. } => {
                // SlaBreached has no signed report — count it
                // unconditionally (the publish job is responsible for
                // window-scoping the input slice in this arm).
                sla_breached_count = sla_breached_count.saturating_add(1);
            }
            ErasureDecision::Started { .. }
            | ErasureDecision::Rejected { .. }
            | ErasureDecision::VerificationFailed { .. } => {
                // Not in scope for the public completion stats.
            }
        }
    }

    let p95_resolution_hours = p95_of_observations(&observations);

    DsrCompletionStats {
        window_start_unix_s,
        window_end_unix_s,
        verified_complete_count,
        verified_partial_count,
        sla_breached_count,
        p95_resolution_hours,
    }
}

/// Compute the canonical p95 (nearest-rank, 0-indexed
/// `ceil(0.95 * n) - 1`) over a slice of observations. Empty slice →
/// `0` (the Statuspage "no data" sentinel).
#[must_use]
pub fn p95_of_observations(observations: &[u64]) -> u64 {
    if observations.is_empty() {
        return 0;
    }
    let mut sorted: Vec<u64> = observations.to_vec();
    sorted.sort_unstable();
    // nearest-rank: idx = ceil(0.95 * n) - 1, clamped to [0, n-1].
    let n = sorted.len();
    // ceil(0.95 * n) = (95 * n + 99) / 100 (integer-arithmetic).
    let ceil_part = (95usize.saturating_mul(n).saturating_add(99)) / 100;
    let idx = ceil_part.saturating_sub(1).min(n.saturating_sub(1));
    sorted.get(idx).copied().unwrap_or(0)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::event::{
        canonical_cloudevent_types, BackendCompletion, BackendErasureOutcome, BackendKind,
        ErasurePlan, ErasureRequest, ErasureSalt,
    };
    use crate::report::ErasureReport;
    use crate::verification_job::VerificationOutcome;
    use uuid::Uuid;

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    fn make_outcome_verified_complete(generated_ms: u64, verified_ms: u64) -> VerificationOutcome {
        let request = ErasureRequest::new(
            fixed_uuid(1),
            fixed_uuid(2),
            fixed_uuid(3),
            ErasureSalt::synthetic_for_test(7),
            generated_ms,
        );
        let plan = ErasurePlan::canonical(&request, generated_ms);
        let completions = vec![BackendCompletion {
            dsr_id: request.dsr_id,
            tenant_id: request.tenant_id,
            backend: BackendKind::D1,
            outcome: BackendErasureOutcome::Erased { records_deleted: 1 },
            idempotency_key: format!("{}:{}", request.dsr_id, BackendKind::D1.as_str()),
            started_at_ms: generated_ms,
            completed_at_ms: verified_ms,
            retry_count: 0,
            verification_hash: [0u8; 32],
        }];
        let report = ErasureReport {
            dsr_id: request.dsr_id,
            tenant_id: request.tenant_id,
            plan,
            completions: completions.clone(),
            verified_at_ms: verified_ms,
            verified_complete: true,
            cloudevent_types: canonical_cloudevent_types()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        };
        VerificationOutcome {
            decision: ErasureDecision::VerifiedComplete { completions },
            report: Some(report),
            signature: None,
            object_key: None,
        }
    }

    fn make_outcome_sla_breached() -> VerificationOutcome {
        VerificationOutcome {
            decision: ErasureDecision::SlaBreached {
                unverified_count: 1,
                elapsed_ms: 25 * 3_600 * 1_000,
            },
            report: None,
            signature: None,
            object_key: None,
        }
    }

    #[test]
    fn p95_empty_slice_is_zero() {
        assert_eq!(p95_of_observations(&[]), 0);
    }

    #[test]
    fn p95_single_observation_is_that_observation() {
        assert_eq!(p95_of_observations(&[42]), 42);
    }

    #[test]
    fn p95_20_observations_returns_19th_smallest() {
        let obs: Vec<u64> = (1..=20).collect();
        // ceil(0.95 * 20) = 19 → idx 18 → value 19.
        assert_eq!(p95_of_observations(&obs), 19);
    }

    #[test]
    fn p95_unsorted_input_is_sorted_internally() {
        let obs = vec![100, 1, 50, 200, 5];
        // n=5, ceil(0.95*5)=5 → idx 4 → max value 200.
        assert_eq!(p95_of_observations(&obs), 200);
    }

    #[test]
    fn aggregate_empty_window_zeroes_counts_and_p95() {
        let stats = aggregate_24h_window(&[], 1_700_000_000);
        assert_eq!(stats.window_start_unix_s, 1_700_000_000);
        assert_eq!(stats.window_end_unix_s, 1_700_086_400);
        assert_eq!(stats.verified_complete_count, 0);
        assert_eq!(stats.verified_partial_count, 0);
        assert_eq!(stats.sla_breached_count, 0);
        assert_eq!(stats.p95_resolution_hours, 0);
    }

    #[test]
    fn aggregate_counts_verified_complete_and_observations() {
        // Two outcomes: one resolved in 5h, one in 15h. p95 → 15h.
        let window_start_unix_s = 1_700_000_000;
        let window_start_ms = window_start_unix_s * 1_000;
        let one_h_ms: u64 = 3_600 * 1_000;
        let a = make_outcome_verified_complete(window_start_ms + 1_000, window_start_ms + 1_000 + 5 * one_h_ms);
        let b = make_outcome_verified_complete(window_start_ms + 1_000, window_start_ms + 1_000 + 15 * one_h_ms);
        let stats = aggregate_24h_window(&[a, b], window_start_unix_s);
        assert_eq!(stats.verified_complete_count, 2);
        assert_eq!(stats.verified_partial_count, 0);
        assert_eq!(stats.sla_breached_count, 0);
        // p95 on 2 observations = max → 15h.
        assert_eq!(stats.p95_resolution_hours, 15);
    }

    #[test]
    fn aggregate_filters_out_of_window_outcomes() {
        let window_start_unix_s = 1_700_000_000;
        let window_start_ms = window_start_unix_s * 1_000;
        let one_h_ms: u64 = 3_600 * 1_000;
        // Outcome verified BEFORE window start.
        let before = make_outcome_verified_complete(window_start_ms - 30 * one_h_ms, window_start_ms - one_h_ms);
        // Outcome verified AFTER window end.
        let after = make_outcome_verified_complete(window_start_ms + 50 * one_h_ms, window_start_ms + 25 * 3_600 * 1_000);
        let stats = aggregate_24h_window(&[before, after], window_start_unix_s);
        assert_eq!(stats.verified_complete_count, 0);
    }

    #[test]
    fn aggregate_counts_sla_breached_unconditionally() {
        let stats = aggregate_24h_window(
            &[make_outcome_sla_breached(), make_outcome_sla_breached()],
            1_700_000_000,
        );
        assert_eq!(stats.sla_breached_count, 2);
        assert_eq!(stats.verified_complete_count, 0);
    }
}
