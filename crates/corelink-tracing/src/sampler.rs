//! `Sampler` trait + `RateBasedSampler` (canonical head-based
//! sampling at p%).
//!
//! ## Why head-based deterministic sampling
//!
//! Per OpenTelemetry SDK `Sampler::TraceIdRatioBased` + Google Dapper
//! 0.1% canonical precedent, head-based sampling derives the decision
//! from the trace_id alone. This guarantees:
//!
//! - **Cross-service consistency**: same trace_id → same decision at
//!   every hop in the call graph (Worker → DO → R2 → D1 → Neon). No
//!   "partial trace" pathology where some services sampled + others
//!   dropped the same trace.
//! - **Cheap evaluation**: ~10ns overhead per span start (per WI §1
//!   invariant 8 sampling tax budget).
//! - **Deterministic from trace_id**: avoids the per-instance PRNG
//!   state pathology where cloning the sampler diverges decisions.
//!
//! The canonical mapping is: take the LAST 8 bytes of the 16-byte
//! trace_id as a `u64`, divide by `u64::MAX` to get a fraction in
//! `[0, 1]`, sample if fraction < rate. The last 8 bytes match the
//! OpenTelemetry SDK `TraceIdRatioBased` reference implementation
//! (so cross-vendor tracing tools agree on which traces are
//! sampled).
//!
//! ## Tail-based sampling deferred
//!
//! Per WI §6.2 + the `trait-abstraction-defer` charter pattern, tail-
//! based sampling (100% errors + p99 SLO breach) is deferred to
//! WI-S09-007 PRR ship gate. The trait surface here reserves the
//! `SamplingDecision::TailSampled` variant for forward compatibility.

use std::sync::Arc;

use crate::audit::{
    TracingAuditEventType, TracingAuditRecord, TracingAuditSink,
};
use crate::context::TraceId;
use crate::error::TracingError;

/// Canonical sampling decision per OpenTelemetry SDK
/// `SamplingResult::Decision`.
///
/// `#[non_exhaustive]` so follow-on WIs (tail-based sampling at
/// WI-S09-007 PRR ship gate) can extend without breaking downstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SamplingDecision {
    /// Head-sampled at root: deterministic per trace_id ratio. Cheap.
    HeadSampled,
    /// Tail-sampled retroactively: 100% if span attribute
    /// `error=true` OR `latency_p99_breach=true`. Deferred to
    /// WI-S09-007 PRR ship gate.
    TailSampled,
    /// Drop: head-sampled-out span; production wiring drops at SDK
    /// boundary (no SDK overhead).
    Drop,
}

impl SamplingDecision {
    /// Canonical decision-slug for the
    /// `corelink_tracing_spans_exported_total{sampled_decision}`
    /// counter label per WI §6.1.10.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeadSampled => "head_sampled",
            Self::TailSampled => "tail_sampled",
            Self::Drop => "dropped",
        }
    }

    /// Whether the decision results in the span being exported.
    #[must_use]
    pub const fn is_sampled(self) -> bool {
        matches!(self, Self::HeadSampled | Self::TailSampled)
    }
}

impl core::fmt::Display for SamplingDecision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Sampler trait. Production wiring composes:
///
/// - `RateBasedSampler` — head-based deterministic per W3C trace_id.
/// - `TailSamplingProcessor` — defers decision to span end (WI-S09-007).
pub trait Sampler: Send + Sync + core::fmt::Debug {
    /// Determine the sampling decision for a candidate trace_id.
    /// Implementations MUST be deterministic per trace_id (so the
    /// decision is consistent across hops).
    fn should_sample(&self, trace_id: &TraceId) -> SamplingDecision;
}

/// Head-based rate sampler. Decision is deterministic per trace_id
/// (last 8 bytes interpreted as `u64`; sampled iff
/// `u64_value / u64::MAX < rate`).
///
/// Cloning shares the audit sink + the rate via [`Arc`] (the rate
/// itself is immutable; rate changes require constructing a new
/// sampler instance). This honors the F-001 closure pattern: the
/// sampler holds zero per-instance mutable state outside of the
/// audit sink (which itself is `Send + Sync`).
#[derive(Clone, Debug)]
pub struct RateBasedSampler<A>
where
    A: TracingAuditSink + 'static,
{
    rate: f64,
    audit: Arc<A>,
}

impl<A> RateBasedSampler<A>
where
    A: TracingAuditSink + 'static,
{
    /// Construct a new sampler at canonical rate. Returns
    /// [`TracingError::SamplerRateOutOfBounds`] if `rate < 0.0` or
    /// `rate > 1.0`.
    ///
    /// # Errors
    ///
    /// - Rate outside `[0.0, 1.0]`.
    pub fn new(rate: f64, audit: Arc<A>) -> Result<Self, TracingError> {
        if !(0.0..=1.0).contains(&rate) || rate.is_nan() {
            return Err(TracingError::SamplerRateOutOfBounds {
                observed_rate: rate,
            });
        }
        Ok(Self { rate, audit })
    }

    /// Construct a sampler at the canonical 1% production rate
    /// (WI §1 invariant 2; sprint contract §5.3 R-S09-8).
    pub fn at_canonical_production_rate(
        audit: Arc<A>,
    ) -> Result<Self, TracingError> {
        Self::new(0.01, audit)
    }

    /// Construct a sampler at the canonical 100% staging rate (WI
    /// §1; staging accepts full ingest because volume is low).
    pub fn at_canonical_staging_rate(
        audit: Arc<A>,
    ) -> Result<Self, TracingError> {
        Self::new(1.0, audit)
    }

    /// Snapshot the current rate.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// Evaluate the sampling decision + emit audit record.
    /// Production wiring composes `should_sample` for the hot-path
    /// (no audit emit; the orchestrator emits the
    /// `corelink.tracing.sampler_decision` audit batch-style). The
    /// `evaluate_with_audit` form is exposed here for the WI-S09-003
    /// audit fail-closed envelope coverage tests.
    ///
    /// # Errors
    ///
    /// - [`TracingError::Audit`] when the audit sink fails.
    pub fn evaluate_with_audit(
        &self,
        trace_id: &TraceId,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<SamplingDecision, TracingError> {
        let decision = self.should_sample(trace_id);
        // Audit BEFORE returning the decision. INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER:
        // we emit BEFORE the orchestrator commits the decision to its
        // ledger downstream.
        self.audit.emit(TracingAuditRecord {
            event_type: TracingAuditEventType::SamplerDecision,
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            trace_id_hex: trace_id_to_hex(trace_id),
            span_id_hex: String::new(),
        })?;
        Ok(decision)
    }
}

impl<A> Sampler for RateBasedSampler<A>
where
    A: TracingAuditSink + 'static,
{
    fn should_sample(&self, trace_id: &TraceId) -> SamplingDecision {
        if self.rate <= 0.0 {
            return SamplingDecision::Drop;
        }
        if self.rate >= 1.0 {
            return SamplingDecision::HeadSampled;
        }
        // Last 8 bytes of trace_id as u64, big-endian. Mirrors the
        // OpenTelemetry SDK `TraceIdRatioBased` reference implementation
        // (last 8 bytes; canonical so cross-vendor tools agree on which
        // traces are sampled).
        let last_8 = match trace_id.get(8..16) {
            Some(s) => s,
            None => return SamplingDecision::Drop,
        };
        let mut buf = [0u8; 8];
        for (i, b) in last_8.iter().enumerate() {
            if let Some(slot) = buf.get_mut(i) {
                *slot = *b;
            }
        }
        let value = u64::from_be_bytes(buf);
        // Compute fraction in [0, 1]. Avoid floating-point precision
        // loss on huge trace_ids by dividing in u128 space.
        let value_u128 = u128::from(value);
        let max_u128 = u128::from(u64::MAX);
        let threshold =
            ((self.rate * max_u128 as f64) as u128).min(max_u128);
        if value_u128 < threshold {
            SamplingDecision::HeadSampled
        } else {
            SamplingDecision::Drop
        }
    }
}

fn trace_id_to_hex(t: &TraceId) -> String {
    let mut out = String::with_capacity(32);
    for b in t {
        out.push(hex_digit(b >> 4));
        out.push(hex_digit(b & 0x0F));
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        // Unreachable by construction.
        _ => '0',
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit::{
        FailingTracingAuditSink, InMemoryTracingAuditSink,
    };

    fn fresh_sampler(
        rate: f64,
    ) -> (
        RateBasedSampler<InMemoryTracingAuditSink>,
        Arc<InMemoryTracingAuditSink>,
    ) {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let s = RateBasedSampler::new(rate, Arc::clone(&audit)).unwrap();
        (s, audit)
    }

    #[test]
    fn decision_canonical_strings_pinned() {
        assert_eq!(
            SamplingDecision::HeadSampled.as_str(),
            "head_sampled"
        );
        assert_eq!(
            SamplingDecision::TailSampled.as_str(),
            "tail_sampled"
        );
        assert_eq!(SamplingDecision::Drop.as_str(), "dropped");
    }

    #[test]
    fn decision_is_sampled_classifies() {
        assert!(SamplingDecision::HeadSampled.is_sampled());
        assert!(SamplingDecision::TailSampled.is_sampled());
        assert!(!SamplingDecision::Drop.is_sampled());
    }

    #[test]
    fn rate_zero_drops_all() {
        let (s, _) = fresh_sampler(0.0);
        for byte in 0_u8..=255 {
            let trace_id = [byte; 16];
            assert_eq!(s.should_sample(&trace_id), SamplingDecision::Drop);
        }
    }

    #[test]
    fn rate_one_samples_all() {
        let (s, _) = fresh_sampler(1.0);
        for byte in 0_u8..=255 {
            let trace_id = [byte; 16];
            assert_eq!(
                s.should_sample(&trace_id),
                SamplingDecision::HeadSampled
            );
        }
    }

    #[test]
    fn deterministic_per_trace_id() {
        let (s, _) = fresh_sampler(0.01);
        let trace_id = [
            0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6,
            0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e, 0x47, 0x36,
        ];
        let d1 = s.should_sample(&trace_id);
        let d2 = s.should_sample(&trace_id);
        let d3 = s.should_sample(&trace_id);
        assert_eq!(d1, d2);
        assert_eq!(d2, d3);
    }

    #[test]
    fn rate_out_of_bounds_rejected() {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let err =
            RateBasedSampler::new(1.5, Arc::clone(&audit)).unwrap_err();
        assert!(matches!(
            err,
            TracingError::SamplerRateOutOfBounds { .. }
        ));
        let err2 =
            RateBasedSampler::new(-0.1, Arc::clone(&audit)).unwrap_err();
        assert!(matches!(
            err2,
            TracingError::SamplerRateOutOfBounds { .. }
        ));
        let err3 =
            RateBasedSampler::new(f64::NAN, audit).unwrap_err();
        assert!(matches!(
            err3,
            TracingError::SamplerRateOutOfBounds { .. }
        ));
    }

    #[test]
    fn canonical_production_rate_is_one_percent() {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let s =
            RateBasedSampler::at_canonical_production_rate(audit)
                .unwrap();
        assert_eq!(s.rate(), 0.01);
    }

    #[test]
    fn canonical_staging_rate_is_full() {
        let audit = Arc::new(InMemoryTracingAuditSink::new());
        let s =
            RateBasedSampler::at_canonical_staging_rate(audit).unwrap();
        assert_eq!(s.rate(), 1.0);
    }

    #[test]
    fn evaluate_with_audit_emits_record() {
        let (s, audit) = fresh_sampler(1.0);
        let trace_id = [0xAA; 16];
        let d = s
            .evaluate_with_audit(&trace_id, "req-1", 1)
            .unwrap();
        assert_eq!(d, SamplingDecision::HeadSampled);
        let recs = audit
            .snapshot_of(TracingAuditEventType::SamplerDecision);
        assert_eq!(recs.len(), 1);
        assert_eq!(
            recs.first().unwrap().trace_id_hex,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn evaluate_with_audit_fails_closed_when_audit_fails() {
        let audit = Arc::new(FailingTracingAuditSink::new());
        let s = RateBasedSampler::new(1.0, audit).unwrap();
        let err = s
            .evaluate_with_audit(&[0xAA; 16], "req-1", 1)
            .unwrap_err();
        assert!(matches!(err, TracingError::Audit(_)));
    }

    #[test]
    fn rate_proportionality_smoke_at_50_percent() {
        // Smoke test for the proportionality invariant. Property test
        // covers more iterations.
        let (s, _) = fresh_sampler(0.5);
        let mut sampled = 0_u32;
        let n = 1024_u32;
        for i in 0..n {
            let mut trace_id = [0u8; 16];
            // Spread the trace_id bytes over the last 8 (the threshold
            // input) using a deterministic linear sweep + a high-byte
            // mixer so we sample the full u64 range.
            for j in 8..16 {
                if let Some(slot) = trace_id.get_mut(j) {
                    *slot = ((i.wrapping_mul(2654435761)
                        >> ((j - 8) * 4))
                        & 0xFF) as u8;
                }
            }
            if matches!(
                s.should_sample(&trace_id),
                SamplingDecision::HeadSampled
            ) {
                sampled = sampled.saturating_add(1);
            }
        }
        let observed = f64::from(sampled) / f64::from(n);
        // Tolerance ±10% (bounded; property test sweeps wider).
        assert!(
            (0.40..=0.60).contains(&observed),
            "expected ~0.5 sample rate; got {observed}"
        );
    }
}
