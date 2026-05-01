//! Audit metrics observer trait — emits the canonical 5 metrics
//! mandated by WI-S03-007 §6.1.10.
//!
//! ## Canonical metric set
//!
//! | Name | Type | Labels | Purpose |
//! |---|---|---|---|
//! | `corelink.audit.events_emitted_total` | counter | `type` (event-type string), `tenant_tier` | per-type emit volume |
//! | `corelink.audit.outbox_lag_seconds` | gauge | `region` | observed lag from emit-time to drain-completion (recorded by drain worker; this trait surfaces the recording API) |
//! | `corelink.audit.redact_violations_total` | counter | `kind` (`pat` / `email` / `principal_id`) | every CI lint violation that slips into runtime is a SEV-2 alert |
//! | `corelink.audit.chain_hash_compute_duration_us` | histogram (microseconds) | — | per-event JCS+SHA-256 cost; SLO p99 < 100 µs |
//! | `corelink.audit.event_payload_size_bytes` | histogram | — | observed payload size; SLO p99 < 4 KiB |
//!
//! The trait is **observer-style** (sync, error-free) — telemetry pipe
//! plumbing is per-deploy in S-09 / S-13; this trait keeps the audit
//! crate clean and lets every property test assert on the observed
//! triples via [`InMemoryMetrics`].

use std::sync::{Arc, Mutex};

use crate::events::AuthEventType;
use crate::retention::RetentionHint;

/// Observer trait for the canonical 5 audit metrics.
///
/// Every method is infallible: telemetry observation must never break
/// the producer. Implementations buffer + emit asynchronously per
/// their own SLO.
pub trait MetricsObserver: Send + Sync {
    /// Record a `corelink.audit.events_emitted_total{type, tenant_tier}`
    /// counter increment.
    fn record_event_emitted(&self, event_type: AuthEventType, retention_hint: RetentionHint);

    /// Record a `corelink.audit.redact_violations_total{kind}` counter
    /// increment. SEV-2 alert on any non-zero value.
    fn record_redact_violation(&self, kind: RedactViolationKind);

    /// Record a `corelink.audit.chain_hash_compute_duration_us`
    /// histogram observation (microseconds).
    fn record_chain_hash_duration_us(&self, duration_us: u64);

    /// Record a `corelink.audit.event_payload_size_bytes` histogram
    /// observation.
    fn record_event_payload_size_bytes(&self, size_bytes: u64);

    /// Record a `corelink.audit.outbox_lag_seconds{region}` gauge
    /// observation. Called by the drain worker (S-09); exposed on the
    /// trait so the producer can also surface zero-lag (drain is
    /// keeping up) when emission is direct.
    fn record_outbox_lag_seconds(&self, region: &str, lag_seconds: f64);
}

/// Kind label tagged on `corelink.audit.redact_violations_total`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RedactViolationKind {
    /// Raw PAT plaintext slipped into an audit envelope.
    Pat,
    /// Raw email plaintext slipped into an audit envelope.
    Email,
    /// Raw principal id (Clerk subject) slipped into an audit envelope.
    PrincipalId,
}

impl RedactViolationKind {
    /// Canonical string label for metric tagging.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pat => "pat",
            Self::Email => "email",
            Self::PrincipalId => "principal_id",
        }
    }
}

/// No-op metrics observer. Default in unit tests that don't care
/// about telemetry.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopMetrics;

impl MetricsObserver for NoopMetrics {
    fn record_event_emitted(&self, _event_type: AuthEventType, _retention_hint: RetentionHint) {}
    fn record_redact_violation(&self, _kind: RedactViolationKind) {}
    fn record_chain_hash_duration_us(&self, _duration_us: u64) {}
    fn record_event_payload_size_bytes(&self, _size_bytes: u64) {}
    fn record_outbox_lag_seconds(&self, _region: &str, _lag_seconds: f64) {}
}

/// In-memory metrics observer. Used by property tests + integration
/// tests to assert "every code path emits the right metric triple".
///
/// Cloning shares the underlying state (`Arc<Mutex<…>>`).
#[derive(Clone, Debug, Default)]
pub struct InMemoryMetrics {
    state: Arc<Mutex<MetricsState>>,
}

#[derive(Debug, Default)]
struct MetricsState {
    events_emitted: Vec<(AuthEventType, RetentionHint)>,
    redact_violations: Vec<RedactViolationKind>,
    chain_hash_durations_us: Vec<u64>,
    payload_sizes: Vec<u64>,
    outbox_lags: Vec<(String, f64)>,
}

impl InMemoryMetrics {
    /// Construct an empty metrics sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the events_emitted counter (returns Vec of all observations).
    #[must_use]
    pub fn snapshot_events_emitted(&self) -> Vec<(AuthEventType, RetentionHint)> {
        match self.state.lock() {
            Ok(g) => g.events_emitted.clone(),
            Err(poisoned) => poisoned.into_inner().events_emitted.clone(),
        }
    }

    /// Snapshot the redact_violations counter (returns Vec of all observations).
    #[must_use]
    pub fn snapshot_redact_violations(&self) -> Vec<RedactViolationKind> {
        match self.state.lock() {
            Ok(g) => g.redact_violations.clone(),
            Err(poisoned) => poisoned.into_inner().redact_violations.clone(),
        }
    }

    /// Snapshot the chain hash duration observations.
    #[must_use]
    pub fn snapshot_chain_hash_durations_us(&self) -> Vec<u64> {
        match self.state.lock() {
            Ok(g) => g.chain_hash_durations_us.clone(),
            Err(poisoned) => poisoned.into_inner().chain_hash_durations_us.clone(),
        }
    }

    /// Snapshot the payload size observations.
    #[must_use]
    pub fn snapshot_payload_sizes(&self) -> Vec<u64> {
        match self.state.lock() {
            Ok(g) => g.payload_sizes.clone(),
            Err(poisoned) => poisoned.into_inner().payload_sizes.clone(),
        }
    }

    /// Snapshot the outbox lag gauge observations.
    #[must_use]
    pub fn snapshot_outbox_lags(&self) -> Vec<(String, f64)> {
        match self.state.lock() {
            Ok(g) => g.outbox_lags.clone(),
            Err(poisoned) => poisoned.into_inner().outbox_lags.clone(),
        }
    }
}

impl MetricsObserver for InMemoryMetrics {
    fn record_event_emitted(&self, event_type: AuthEventType, retention_hint: RetentionHint) {
        if let Ok(mut g) = self.state.lock() {
            g.events_emitted.push((event_type, retention_hint));
        }
    }

    fn record_redact_violation(&self, kind: RedactViolationKind) {
        if let Ok(mut g) = self.state.lock() {
            g.redact_violations.push(kind);
        }
    }

    fn record_chain_hash_duration_us(&self, duration_us: u64) {
        if let Ok(mut g) = self.state.lock() {
            g.chain_hash_durations_us.push(duration_us);
        }
    }

    fn record_event_payload_size_bytes(&self, size_bytes: u64) {
        if let Ok(mut g) = self.state.lock() {
            g.payload_sizes.push(size_bytes);
        }
    }

    fn record_outbox_lag_seconds(&self, region: &str, lag_seconds: f64) {
        if let Ok(mut g) = self.state.lock() {
            g.outbox_lags.push((region.to_string(), lag_seconds));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_does_not_panic() {
        let m = NoopMetrics;
        m.record_event_emitted(AuthEventType::TokenValidated, RetentionHint::Team90d);
        m.record_redact_violation(RedactViolationKind::Pat);
        m.record_chain_hash_duration_us(50);
        m.record_event_payload_size_bytes(512);
        m.record_outbox_lag_seconds("wnam", 0.0);
    }

    #[test]
    fn in_memory_records_all_metrics() {
        let m = InMemoryMetrics::new();
        m.record_event_emitted(AuthEventType::TokenValidated, RetentionHint::Solo30d);
        m.record_event_emitted(AuthEventType::TokenRevoked, RetentionHint::Team90d);
        m.record_redact_violation(RedactViolationKind::Pat);
        m.record_chain_hash_duration_us(50);
        m.record_chain_hash_duration_us(70);
        m.record_event_payload_size_bytes(512);
        m.record_outbox_lag_seconds("wnam", 1.5);
        assert_eq!(m.snapshot_events_emitted().len(), 2);
        assert_eq!(m.snapshot_redact_violations().len(), 1);
        assert_eq!(m.snapshot_chain_hash_durations_us(), vec![50, 70]);
        assert_eq!(m.snapshot_payload_sizes(), vec![512]);
        assert_eq!(m.snapshot_outbox_lags(), vec![("wnam".to_string(), 1.5)]);
    }
}
