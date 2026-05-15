//! KV cross-region propagation-lag SLI integration tests
//! (closes DEBT-011 R-PREP-REPL-P1-001).
//!
//! Covers:
//! 1. Metric name constant matches the verifier's `PROM_METRIC["kv"]` key
//!    (LOAD-BEARING string).
//! 2. Probe trait + InMemory fixture work as deterministic SLI source for
//!    the verifier `--mode=inmemory` path.
//! 3. 95% sample-fraction-within-typical SLO check matches `slo_catalog.md
//!    §4.25 "Target (typical)"`.
//! 4. Inter-region pairs (KV is global, not sibling-restricted) probe cleanly.
//! 5. Probe failure surfaces as `Err(String)` (caller emits SEV-3 per §4.25
//!    burn alert table).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests use direct assertions"
)]

use corelink_region::kv_propagation::{
    fraction_within_typical, FailingKvPropagationProbe, InMemoryKvPropagationProbe,
    KvPropagationProbe, KvPropagationSample, KV_PROBE_CADENCE_SECONDS,
    KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS, KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS,
    KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION, METRIC_KV_PROPAGATION_LAG_SECONDS,
};
use corelink_region::Region;

use proptest::prelude::*;

#[test]
fn metric_name_matches_verifier() {
    // LOAD-BEARING for `scripts/verify-replication-lag.py::PROM_METRIC["kv"]`.
    assert_eq!(
        METRIC_KV_PROPAGATION_LAG_SECONDS,
        "corelink_kv_propagation_lag_seconds"
    );
}

#[test]
fn cadence_matches_ticket_p1_001_acceptance_criterion() {
    assert_eq!(KV_PROBE_CADENCE_SECONDS, 30);
}

#[test]
fn slo_ceiling_constants_match_catalog_4_25() {
    assert_eq!(KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS, 60);
    assert_eq!(KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS, 300);
}

#[test]
fn inmemory_probe_records_typical_lag() {
    let p = InMemoryKvPropagationProbe::new();
    for w in Region::ALL {
        for r in Region::ALL {
            if *w == *r {
                continue;
            }
            // 5s typical lag — well under 60s.
            p.set_lag(*w, *r, 5.0);
        }
    }
    // Probe every inter-region pair.
    let mut samples = Vec::new();
    for w in Region::ALL {
        for r in Region::ALL {
            if *w == *r {
                continue;
            }
            samples.push(p.probe(*w, *r, 0).expect("probe"));
        }
    }
    assert_eq!(samples.len(), 12, "4 × 3 = 12 inter-region pairs");
    assert!(samples
        .iter()
        .all(|s| s.lag_seconds <= (KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS as f64)));
    assert!(
        (fraction_within_typical(&samples, KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS as f64)
            - 1.0)
            .abs()
            < f64::EPSILON
    );
}

#[test]
fn fraction_within_typical_at_slo_boundary() {
    // Construct a population where exactly 95% are under 60s — the SLO
    // boundary. This is the smallest population (20 samples; 19 under).
    let mut samples: Vec<KvPropagationSample> = (0..19)
        .map(|i| KvPropagationSample {
            write_region: Region::Wnam,
            read_region: Region::Enam,
            lag_seconds: 10.0 + (i as f64),
            probe_timestamp_ms: 0,
        })
        .collect();
    samples.push(KvPropagationSample {
        write_region: Region::Wnam,
        read_region: Region::Weur,
        lag_seconds: 120.0, // 1 sample over typical
        probe_timestamp_ms: 0,
    });
    let f = fraction_within_typical(&samples, KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS as f64);
    assert!((f - 0.95).abs() < f64::EPSILON);
    assert!(f >= KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION); // ≥ 95% — at boundary
}

#[test]
fn failing_probe_returns_err_for_sev3_path() {
    let p = FailingKvPropagationProbe::new("simulated KV unavailability");
    let r = p.probe(Region::Wnam, Region::Enam, 0);
    assert!(r.is_err());
    let err = r.unwrap_err();
    assert!(err.contains("simulated"));
}

fn arb_region() -> impl Strategy<Value = Region> {
    prop_oneof![
        Just(Region::Wnam),
        Just(Region::Enam),
        Just(Region::Weur),
        Just(Region::Sam),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128)
    ))]

    /// For every (write_region, read_region) pair the InMemory probe MUST
    /// return a sample with the configured lag and finite, non-negative
    /// `lag_seconds` (negative configured values are rejected by the probe).
    #[test]
    fn prop_inmemory_probe_round_trip(
        write in arb_region(),
        read in arb_region(),
        lag in 0.0f64..(KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS as f64 * 2.0),
    ) {
        let p = InMemoryKvPropagationProbe::new();
        p.set_lag(write, read, lag);
        let s = p.probe(write, read, 1_700_000_000_000).unwrap();
        prop_assert_eq!(s.write_region, write);
        prop_assert_eq!(s.read_region, read);
        prop_assert!(s.lag_seconds.is_finite());
        prop_assert!(s.lag_seconds >= 0.0);
        prop_assert!((s.lag_seconds - lag).abs() < 1e-9);
    }
}
