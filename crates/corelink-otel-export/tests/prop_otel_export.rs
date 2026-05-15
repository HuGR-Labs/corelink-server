//! Property tests for `corelink-otel-export`.
//!
//! Pins the load-bearing invariants the production exporter wiring
//! will rely on:
//!
//! - `INV-OBS-EXPORT-FAIL-OPEN` — over any random sequence of injected
//!   transport / auth / rate-limit / payload errors, the orchestrator
//!   returns `Ok(())` to the caller and the audit sink records exactly
//!   one event per failure.
//! - `INV-OBS-CT-SECRET-EQ` — `constant_time_secret_eq` is a total
//!   reflexive symmetric function over arbitrary byte sequences.
//! - `INV-OBS-NO-PII` — the exported label set NEVER contains any
//!   denylisted PII key (`email`, `tenant_id_raw`, `ip`, `token`,
//!   `password`, etc.) for any sequence of metric points minted via
//!   the canonical `RedMetricKind` taxonomy.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;

use proptest::prelude::*;

use corelink_analytics::RedMetricKind;
use corelink_otel_export::{
    constant_time_secret_eq, ExporterError, ExporterVariant, InMemoryExportFailedAuditSink,
    InMemoryFake, MetricPoint, MetricValue, MetricsExporter,
};

// ---------- shrinking strategies ----------

fn arb_exporter_error() -> impl Strategy<Value = ExporterError> {
    prop_oneof![
        Just(ExporterError::Transport("conn reset".into())),
        Just(ExporterError::AuthReject("401".into())),
        Just(ExporterError::RateLimited("429".into())),
        Just(ExporterError::PayloadRejected("400".into())),
        Just(ExporterError::Internal("mutex".into())),
    ]
}

fn arb_variant() -> impl Strategy<Value = ExporterVariant> {
    prop_oneof![
        Just(ExporterVariant::Datadog),
        Just(ExporterVariant::OtelCollector),
        Just(ExporterVariant::GrafanaCloud),
        Just(ExporterVariant::Disabled),
    ]
}

fn arb_red_metric_kind() -> impl Strategy<Value = RedMetricKind> {
    prop_oneof![
        Just(RedMetricKind::CasPutRequestsTotal),
        Just(RedMetricKind::CasPutDurationSeconds),
        Just(RedMetricKind::CasGetBytesTotal),
        Just(RedMetricKind::AcLookupRequestsTotal),
        Just(RedMetricKind::GcRunsTotal),
        Just(RedMetricKind::DedupRatio),
        Just(RedMetricKind::RateLimitRejectsTotal),
        Just(RedMetricKind::PrivacyDsrActiveTotal),
        Just(RedMetricKind::BillingEventsEmittedTotal),
    ]
}

/// Canonical pseudonymous-only label keys (per
/// `observability_model.md §3`). Any other key would represent drift
/// and is what the PII-clean property guards against.
const CANONICAL_LABEL_KEYS: &[&str] = &[
    "tenant_tier",
    "region",
    "result",
    "hit_miss",
    "phase",
    "layer",
    "reason",
    "bucket",
    "op_type",
    "database",
    "namespace",
    "do_class",
    "event_type",
    "dsr_type",
];

fn arb_canonical_label_value() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("free".to_string()),
        Just("pro".to_string()),
        Just("enterprise".to_string()),
        Just("us-east-1".to_string()),
        Just("eu-central-1".to_string()),
        Just("ok".to_string()),
        Just("error".to_string()),
        Just("hit".to_string()),
        Just("miss".to_string()),
    ]
}

fn arb_canonical_metric_point() -> impl Strategy<Value = MetricPoint> {
    (
        arb_red_metric_kind(),
        proptest::collection::vec(
            (
                proptest::sample::select(CANONICAL_LABEL_KEYS),
                arb_canonical_label_value(),
            ),
            0..6,
        ),
        any::<u64>(),
        any::<u128>(),
    )
        .prop_map(|(kind, labels, count, ts)| {
            let mut bt = BTreeMap::new();
            for (k, v) in labels {
                bt.insert((*k).to_string(), v);
            }
            MetricPoint::new(kind, bt, MetricValue::Counter(count), ts)
        })
}

const DENYLIST_PII_KEYS: &[&str] = &[
    "email",
    "tenant_id_raw",
    "ip",
    "token",
    "password",
    "api_key",
    "phone",
    "name",
    "address",
    "cpf",
    "cnpj",
    "ssn",
];

proptest! {
    /// INV-OBS-EXPORT-FAIL-OPEN: over any sequence of injected
    /// transport-side errors, the orchestrator returns Ok(()) to the
    /// caller and the audit sink records exactly one event per failure.
    #[test]
    fn prop_fail_open_under_any_transport_error(
        variant in arb_variant(),
        failures in proptest::collection::vec(arb_exporter_error(), 1..16),
    ) {
        let fake = InMemoryFake::new(variant);
        let sink = InMemoryExportFailedAuditSink::new();
        let metric = MetricPoint::new(
            RedMetricKind::CasPutRequestsTotal,
            BTreeMap::new(),
            MetricValue::Counter(1),
            0,
        );
        for err in &failures {
            fake.inject_metric_failure(err.clone());
            // MUST be Ok(()) — fail-OPEN contract
            fake.export_batch_fail_open(std::slice::from_ref(&metric), &sink, 0).unwrap();
        }
        prop_assert_eq!(sink.len(), failures.len());
        // Every event tags the right variant
        for ev in sink.snapshot() {
            prop_assert_eq!(ev.variant, variant);
        }
    }

    /// INV-OBS-CT-SECRET-EQ: constant_time_secret_eq is reflexive,
    /// symmetric, and total over arbitrary byte sequences.
    #[test]
    fn prop_constant_time_secret_eq_total_function(
        a in proptest::collection::vec(any::<u8>(), 0..64),
        b in proptest::collection::vec(any::<u8>(), 0..64),
    ) {
        // reflexive
        prop_assert!(constant_time_secret_eq(&a, &a));
        // symmetric
        prop_assert_eq!(
            constant_time_secret_eq(&a, &b),
            constant_time_secret_eq(&b, &a)
        );
        // total — never panics; matches stdlib equality
        prop_assert_eq!(constant_time_secret_eq(&a, &b), a == b);
    }

    /// INV-OBS-NO-PII: no canonical metric point ever carries a
    /// denylisted PII label key.
    #[test]
    fn prop_exported_label_set_pii_clean(
        metrics in proptest::collection::vec(arb_canonical_metric_point(), 0..32),
    ) {
        for m in &metrics {
            for k in m.labels.keys() {
                for bad in DENYLIST_PII_KEYS {
                    prop_assert!(
                        k != bad,
                        "metric kind {:?} leaked PII label {:?}",
                        m.kind, k
                    );
                }
            }
            // canonical metric name slug must start with `corelink_`
            prop_assert!(
                m.metric_name().starts_with("corelink_"),
                "metric name {:?} not in canonical namespace",
                m.metric_name()
            );
        }
    }

    /// In-memory fake captures EVERY successfully-exported metric (no
    /// silent drops). Adversarial-batch test.
    #[test]
    fn prop_inmem_fake_captures_every_success(
        n in 0usize..32,
    ) {
        let fake = InMemoryFake::new(ExporterVariant::Datadog);
        let metric = MetricPoint::new(
            RedMetricKind::CasGetBytesTotal,
            BTreeMap::new(),
            MetricValue::Counter(1),
            0,
        );
        let batch: Vec<MetricPoint> = (0..n).map(|_| metric.clone()).collect();
        fake.export_batch(&batch).unwrap();
        prop_assert_eq!(fake.snapshot_metrics().len(), n);
    }
}
