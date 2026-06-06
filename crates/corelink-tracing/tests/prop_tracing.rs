// S-13 sprint-close P1: rust 1.88 stricter clippy on test format args.
#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]

//! Property tests pinning the load-bearing invariants of
//! `corelink-tracing` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env var override per S-07 P1-2 fix).
//!
//! Coverage map (mirrors WI-S09-003 §6.1.11):
//!
//! - `prop_traceparent_roundtrip` — `parse(format(ctx)) == ctx` for any
//!   valid context.
//! - `prop_traceparent_rejects_invalid` — invalid input → Err.
//! - `prop_zero_trace_id_rejected` — all-zero trace_id (W3C §3.2.2.2)
//!   rejected by both parser + constructor.
//! - `prop_sampler_rate_proportional` — over N spans at rate p, sampled
//!   count ≈ p × N within statistical tolerance.
//! - `prop_span_kind_matches_otlp` — span kind enum matches OTLP
//!   canonical 6 (UNSPECIFIED / INTERNAL / SERVER / CLIENT / PRODUCER /
//!   CONSUMER).
//! - `prop_tenant_isolation` — tenant A spans never reference tenant B
//!   trace IDs (INV-TENANT-ISOLATION canary).
//! - `prop_audit_emit_per_event_type` — every span lifecycle emits the
//!   canonical audit records.
//! - `prop_exemplar_link_to_metric` — span exemplar correctly links to
//!   `RedMetricKind` (CAP-OBS-009).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_analytics::{RedMetricKind, Tier};
use corelink_tracing::{
    canonical_audit_event_strings, canonical_span_kinds, format_traceparent, parse_traceparent,
    EndSpanInput, Exemplar, InMemoryOtlpExporter, InMemoryTracingAuditSink, RateBasedSampler,
    Sampler, SamplingDecision, SpanKind, SpanRecord, SpanStatus, StartSpanInput, TraceContext,
    TracingAuditEventType, TracingService, ALL_ZERO_SPAN_ID, ALL_ZERO_TRACE_ID, TRACEPARENT_HEADER,
    TRACESTATE_HEADER, TRACE_FLAGS_SAMPLED, W3C_TRACE_CONTEXT_VERSION,
};
use proptest::prelude::*;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

const ALL_TIERS: &[Tier] = &[
    Tier::Free,
    Tier::Solo,
    Tier::Team,
    Tier::Business,
    Tier::Enterprise,
];

const ALL_SPAN_KINDS: &[SpanKind] = &[
    SpanKind::Unspecified,
    SpanKind::Internal,
    SpanKind::Server,
    SpanKind::Client,
    SpanKind::Producer,
    SpanKind::Consumer,
];

const ALL_METRIC_KINDS: &[RedMetricKind] = &[
    RedMetricKind::CasPutRequestsTotal,
    RedMetricKind::CasPutDurationSeconds,
    RedMetricKind::CasGetBytesTotal,
    RedMetricKind::AcLookupRequestsTotal,
    RedMetricKind::GcRunsTotal,
    RedMetricKind::DedupRatio,
    RedMetricKind::RateLimitRejectsTotal,
    RedMetricKind::PrivacyDsrActiveTotal,
    RedMetricKind::BillingEventsEmittedTotal,
    RedMetricKind::CfCpuTimeUs,
    RedMetricKind::R2OpsTotal,
    RedMetricKind::D1RowScansTotal,
    RedMetricKind::KvReadQuotaUsed,
    RedMetricKind::KvWriteQuotaUsed,
    RedMetricKind::DoStorageSizeBytes,
];

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.tracing.span_started"));
    assert!(s.contains(&"corelink.tracing.span_ended"));
    assert!(s.contains(&"corelink.tracing.sampler_decision"));
    assert!(s.contains(&"corelink.tracing.exporter_failure"));
}

#[test]
fn canonical_span_kinds_pinned() {
    let v = canonical_span_kinds();
    assert_eq!(v.len(), 6);
    let mut set = std::collections::HashSet::new();
    for k in v {
        assert!(set.insert(k.as_str()));
    }
    assert_eq!(set.len(), 6);
    assert!(set.contains("SPAN_KIND_UNSPECIFIED"));
    assert!(set.contains("SPAN_KIND_INTERNAL"));
    assert!(set.contains("SPAN_KIND_SERVER"));
    assert!(set.contains("SPAN_KIND_CLIENT"));
    assert!(set.contains("SPAN_KIND_PRODUCER"));
    assert!(set.contains("SPAN_KIND_CONSUMER"));
}

#[test]
fn w3c_canonical_constants_pinned() {
    assert_eq!(W3C_TRACE_CONTEXT_VERSION, 0x00);
    assert_eq!(TRACEPARENT_HEADER, "traceparent");
    assert_eq!(TRACESTATE_HEADER, "tracestate");
    assert_eq!(TRACE_FLAGS_SAMPLED, 0x01);
    assert_eq!(ALL_ZERO_TRACE_ID, [0u8; 16]);
    assert_eq!(ALL_ZERO_SPAN_ID, [0u8; 8]);
}

#[test]
fn schema_version_pinned() {
    assert_eq!(corelink_tracing::tracing_schema_version(), 1);
}

#[test]
fn w3c_canonical_example_round_trip() {
    // Canonical example from W3C Trace Context §3.2.4.
    let canonical = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let ctx = parse_traceparent(canonical).unwrap();
    assert_eq!(format_traceparent(&ctx), canonical);
    assert_eq!(ctx.version, W3C_TRACE_CONTEXT_VERSION);
    assert_eq!(ctx.flags, TRACE_FLAGS_SAMPLED);
    assert!(ctx.is_sampled());
}

// ---- Property test strategies ---------------------------------------

prop_compose! {
    fn arb_tier()(idx in 0_usize..ALL_TIERS.len()) -> Tier {
        ALL_TIERS[idx]
    }
}

prop_compose! {
    fn arb_span_kind()(
        idx in 0_usize..ALL_SPAN_KINDS.len()
    ) -> SpanKind {
        ALL_SPAN_KINDS[idx]
    }
}

prop_compose! {
    fn arb_metric_kind()(
        idx in 0_usize..ALL_METRIC_KINDS.len()
    ) -> RedMetricKind {
        ALL_METRIC_KINDS[idx]
    }
}

prop_compose! {
    fn arb_trace_id()(
        // u128 covers the full 16-byte trace_id space.
        // Skip 0 to avoid the all-zero rejection path which is
        // covered separately by `prop_zero_trace_id_rejected`.
        u in 1_u128..u128::MAX
    ) -> [u8; 16] {
        u.to_be_bytes()
    }
}

prop_compose! {
    fn arb_span_id()(
        // u64 covers the full 8-byte span_id space; skip 0.
        u in 1_u64..u64::MAX
    ) -> [u8; 8] {
        u.to_be_bytes()
    }
}

prop_compose! {
    fn arb_trace_context()(
        trace in arb_trace_id(),
        span in arb_span_id(),
        flags in 0_u8..=0x01,
    ) -> TraceContext {
        TraceContext::new(trace, span, flags).unwrap_or_else(|_| {
            // Unreachable by construction (strategy excludes 0).
            TraceContext::new([0xAA; 16], [0xBB; 8], 0).unwrap()
        })
    }
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// `parse(format(ctx)) == ctx` for any valid context.
    /// W3C Trace Context Recommendation 2020 §3.2.4 round-trip.
    #[test]
    fn prop_traceparent_roundtrip(ctx in arb_trace_context()) {
        let s = format_traceparent(&ctx);
        prop_assert_eq!(s.len(), 55);
        let parsed = parse_traceparent(&s).unwrap();
        prop_assert_eq!(parsed, ctx);
    }

    /// Invalid `traceparent` inputs MUST be rejected.
    /// W3C §3.2.2 strict reject discipline.
    #[test]
    fn prop_traceparent_rejects_invalid(
        s in "[A-Z0-9 ]{0,80}",
    ) {
        // Strategy generates uppercase + digits + spaces; W3C requires
        // lowercase hex + dashes. Inputs of length 55 with the right
        // shape are statistically near-zero; we accept that some valid
        // inputs may slip through (e.g. all-numeric of length 55) and
        // explicitly skip those.
        if s.len() == 55 {
            // Skip the rare case where the random input happens to
            // have valid dashes + lowercase hex (statistically near 0
            // because uppercase + digits cannot match lowercase hex).
            let bytes = s.as_bytes();
            let dashes_at_canonical = bytes.get(2) == Some(&b'-')
                && bytes.get(35) == Some(&b'-')
                && bytes.get(52) == Some(&b'-');
            if !dashes_at_canonical {
                let res = parse_traceparent(&s);
                prop_assert!(res.is_err());
            }
        } else {
            let res = parse_traceparent(&s);
            prop_assert!(res.is_err());
        }
    }

    /// All-zero trace_id MUST be rejected by both parser + constructor.
    /// W3C §3.2.2.2 strict reject.
    #[test]
    fn prop_zero_trace_id_rejected(
        span in arb_span_id(),
        flags in 0_u8..=0x01,
    ) {
        // Constructor path.
        let res = TraceContext::new(ALL_ZERO_TRACE_ID, span, flags);
        prop_assert!(res.is_err());
        // Parser path.
        let span_hex = bytes_to_hex(&span);
        let s = format!(
            "00-00000000000000000000000000000000-{span_hex}-{:02x}",
            flags
        );
        let res = parse_traceparent(&s);
        prop_assert!(res.is_err());
    }

    /// `SpanKind` enum matches OTLP canonical 6.
    /// OTLP `trace/v1/trace.proto` SpanKind closed taxonomy.
    #[test]
    fn prop_span_kind_matches_otlp(kind in arb_span_kind()) {
        let s = kind.as_str();
        let valid = matches!(
            s,
            "SPAN_KIND_UNSPECIFIED"
                | "SPAN_KIND_INTERNAL"
                | "SPAN_KIND_SERVER"
                | "SPAN_KIND_CLIENT"
                | "SPAN_KIND_PRODUCER"
                | "SPAN_KIND_CONSUMER"
        );
        prop_assert!(valid);
        prop_assert!(s.starts_with("SPAN_KIND_"));
    }

    /// Tenant A spans never reference tenant B trace IDs.
    /// INV-TENANT-ISOLATION canary.
    #[test]
    fn prop_tenant_isolation(
        ctx_a in arb_trace_context(),
        ctx_b in arb_trace_context(),
        tier_a_idx in 0_usize..ALL_TIERS.len(),
        // Pick tier_b distinct from tier_a by offset; guaranteed
        // distinct without proptest reject pressure.
        tier_b_offset in 1_usize..ALL_TIERS.len(),
    ) {
        prop_assume!(ctx_a.trace_id != ctx_b.trace_id);
        let tier_a = ALL_TIERS[tier_a_idx];
        let tier_b = ALL_TIERS[(tier_a_idx + tier_b_offset) % ALL_TIERS.len()];
        let (svc, _audit, exporter) = fresh_service_full_sample();
        svc.start_span(StartSpanInput {
            trace_id: ctx_a.trace_id,
            span_id: ctx_a.span_id,
            parent_span_id: None,
            name: "tenant_a_op",
            kind: SpanKind::Internal,
            tenant_tier: tier_a,
            start_time_ns: 1,
            created_by_request_id: "req-a",
            now_ms: 1,
        })
        .unwrap();
        svc.start_span(StartSpanInput {
            trace_id: ctx_b.trace_id,
            span_id: ctx_b.span_id,
            parent_span_id: None,
            name: "tenant_b_op",
            kind: SpanKind::Internal,
            tenant_tier: tier_b,
            start_time_ns: 2,
            created_by_request_id: "req-b",
            now_ms: 2,
        })
        .unwrap();
        svc.end_span(EndSpanInput {
            span_id: ctx_a.span_id,
            tenant_tier: tier_a,
            status: SpanStatus::Ok,
            end_time_ns: 10,
            attributes: vec![(
                "tenant_marker".to_string(),
                "tenant_a".to_string(),
            )],
            exemplars: vec![],
            created_by_request_id: "req-a",
            now_ms: 10,
        })
        .unwrap();
        svc.end_span(EndSpanInput {
            span_id: ctx_b.span_id,
            tenant_tier: tier_b,
            status: SpanStatus::Ok,
            end_time_ns: 20,
            attributes: vec![(
                "tenant_marker".to_string(),
                "tenant_b".to_string(),
            )],
            exemplars: vec![],
            created_by_request_id: "req-b",
            now_ms: 20,
        })
        .unwrap();
        let snap = exporter.snapshot();
        prop_assert_eq!(snap.len(), 2);
        // Locate each span by name.
        let span_a = snap
            .iter()
            .find(|s| s.name == "tenant_a_op")
            .unwrap();
        let span_b = snap
            .iter()
            .find(|s| s.name == "tenant_b_op")
            .unwrap();
        prop_assert_eq!(span_a.trace_id, ctx_a.trace_id);
        prop_assert_eq!(span_b.trace_id, ctx_b.trace_id);
        prop_assert!(span_a.trace_id != span_b.trace_id);
        // Tenant A span should NOT carry tenant B markers.
        let a_attrs: Vec<_> = span_a
            .attributes
            .iter()
            .map(|(_k, v)| v.clone())
            .collect();
        let b_attrs: Vec<_> = span_b
            .attributes
            .iter()
            .map(|(_k, v)| v.clone())
            .collect();
        prop_assert!(!a_attrs.iter().any(|v| v == "tenant_b"));
        prop_assert!(!b_attrs.iter().any(|v| v == "tenant_a"));
    }

    /// Every span lifecycle emits the canonical audit records.
    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER discipline.
    #[test]
    fn prop_audit_emit_per_event_type(
        ctx in arb_trace_context(),
        kind in arb_span_kind(),
        tier in arb_tier(),
        status in prop::sample::select(vec![
            SpanStatus::Ok,
            SpanStatus::Error,
        ]),
    ) {
        let (svc, audit, _exp) = fresh_service_full_sample();
        svc.start_span(StartSpanInput {
            trace_id: ctx.trace_id,
            span_id: ctx.span_id,
            parent_span_id: None,
            name: "op",
            kind,
            tenant_tier: tier,
            start_time_ns: 1,
            created_by_request_id: "req",
            now_ms: 1,
        })
        .unwrap();
        svc.end_span(EndSpanInput {
            span_id: ctx.span_id,
            tenant_tier: tier,
            status,
            end_time_ns: 2,
            attributes: vec![],
            exemplars: vec![],
            created_by_request_id: "req",
            now_ms: 2,
        })
        .unwrap();
        let started =
            audit.snapshot_of(TracingAuditEventType::SpanStarted);
        let ended =
            audit.snapshot_of(TracingAuditEventType::SpanEnded);
        let sampler_audit = audit.snapshot_of(
            TracingAuditEventType::SamplerDecision,
        );
        prop_assert_eq!(started.len(), 1);
        prop_assert_eq!(ended.len(), 1);
        prop_assert_eq!(sampler_audit.len(), 1);
    }

    /// Span exemplar correctly links to `RedMetricKind`.
    /// CAP-OBS-009 metric ↔ trace correlation.
    #[test]
    fn prop_exemplar_link_to_metric(
        ctx in arb_trace_context(),
        metric in arb_metric_kind(),
        // Histogram observation value in seconds (canonical OpenMetrics
        // duration; clamped to a sensible range).
        value_ms in 0_u32..=10_000,
    ) {
        let (svc, _audit, exporter) = fresh_service_full_sample();
        svc.start_span(StartSpanInput {
            trace_id: ctx.trace_id,
            span_id: ctx.span_id,
            parent_span_id: None,
            name: "op",
            kind: SpanKind::Server,
            tenant_tier: Tier::Team,
            start_time_ns: 1,
            created_by_request_id: "req",
            now_ms: 1,
        })
        .unwrap();
        let value_seconds = f64::from(value_ms) / 1000.0;
        svc.end_span(EndSpanInput {
            span_id: ctx.span_id,
            tenant_tier: Tier::Team,
            status: SpanStatus::Ok,
            end_time_ns: 2,
            attributes: vec![],
            exemplars: vec![Exemplar {
                metric_kind: metric,
                value: value_seconds,
                time_ms: 100,
            }],
            created_by_request_id: "req",
            now_ms: 2,
        })
        .unwrap();
        let snap = exporter.snapshot();
        prop_assert_eq!(snap.len(), 1);
        let s = snap.first().unwrap();
        prop_assert_eq!(s.exemplars.len(), 1);
        let ex = s.exemplars.first().unwrap();
        prop_assert_eq!(ex.metric_kind, metric);
        prop_assert_eq!(ex.value, value_seconds);
        prop_assert_eq!(ex.time_ms, 100);
        // Sanity: serialize round-trip preserves the linkage.
        let json = serde_json::to_string(s).unwrap();
        let parsed: SpanRecord = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(parsed.exemplars.first().unwrap().metric_kind, metric);
    }
}

// ---- Sampler proportionality test (custom sweep, not proptest) -------

/// Sampler rate proportionality. Over N spans at rate p, sampled count
/// ≈ p × N within statistical tolerance. Pinned by deterministic
/// `ChaCha20Rng` so the test is reproducible (per charter PRNG-pinned
/// discipline).
#[test]
fn prop_sampler_rate_proportional() {
    // Deterministic seed per charter.
    let mut rng = ChaCha20Rng::seed_from_u64(0xCAFE_F00D_DEAD_BEEF);
    // Test at a sweep of rates: 0.0, 0.01, 0.1, 0.5, 0.9, 1.0.
    let rates = [0.0_f64, 0.01, 0.1, 0.5, 0.9, 1.0];
    let n = 10_000_u32;
    for rate in rates {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let sampler = RateBasedSampler::new(rate, audit).unwrap();
        let mut sampled = 0_u32;
        for _ in 0..n {
            let mut trace_id = [0u8; 16];
            rng.fill_bytes(&mut trace_id);
            // Skip the all-zero canary (per W3C reject; would make the
            // sampler input invalid in production).
            if trace_id == [0u8; 16] {
                trace_id[0] = 1;
            }
            if matches!(
                sampler.should_sample(&trace_id),
                SamplingDecision::HeadSampled
            ) {
                sampled = sampled.saturating_add(1);
            }
        }
        let observed = f64::from(sampled) / f64::from(n);
        // Tolerance: max(absolute=0.02, relative=0.10).
        let absolute_tol = 0.02_f64;
        let relative_tol = if rate > 0.0 { rate * 0.10 } else { 0.0 };
        let tol = absolute_tol.max(relative_tol);
        let lo = (rate - tol).max(0.0);
        let hi = (rate + tol).min(1.0);
        assert!(
            (lo..=hi).contains(&observed),
            "rate={rate} expected in [{lo}, {hi}]; got {observed}"
        );
    }
}

// ---- helpers ---------------------------------------------------------

type Svc = TracingService<
    RateBasedSampler<InMemoryTracingAuditSink>,
    InMemoryTracingAuditSink,
    InMemoryOtlpExporter,
>;

fn fresh_service_full_sample() -> (
    Svc,
    Arc<InMemoryTracingAuditSink>,
    Arc<InMemoryOtlpExporter>,
) {
    let audit = Arc::new(InMemoryTracingAuditSink::new());
    let sampler = Arc::new(RateBasedSampler::new(1.0, Arc::clone(&audit)).unwrap());
    let exporter = Arc::new(InMemoryOtlpExporter::new());
    let svc = TracingService::new(sampler, Arc::clone(&audit), Arc::clone(&exporter));
    (svc, audit, exporter)
}

fn bytes_to_hex(bs: &[u8]) -> String {
    let mut out = String::with_capacity(bs.len() * 2);
    for b in bs {
        out.push(hex_digit(*b >> 4));
        out.push(hex_digit(*b & 0x0F));
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}
