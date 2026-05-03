//! `R2UsageSink` trait + `InMemoryR2UsageSink` append-only NDJSON
//! orchestrator.
//!
//! ## R2 NDJSON layout (per WI-S10-001 §1)
//!
//! Per the WI spec, the production R2 PutObject layout is:
//!
//! ```text
//! usage/{tenant_id}/{billing_period YYYY-MM}/{seq:08}.usage.ndjson
//! ```
//!
//! - `tenant_id` is the canonical UUIDv7 hyphenated lowercase (mirrors
//!   `Uuid::to_string()`).
//! - `billing_period` is the canonical UTC month bucket
//!   (`YYYY-MM` shape; validated at the [`crate::event::UsageEvent`]
//!   construction surface). Per-period partitioning lets the WI-S10-002
//!   counter aggregator scan a single month bucket without sweeping the
//!   whole tenant prefix.
//! - `seq` is the canonical 8-digit zero-padded sequence number
//!   per (tenant, billing_period). Lexicographic R2 key sort =
//!   chronological emit order within the period; the counter aggregator
//!   streams via a single R2 list scan ordered by key.
//!
//! ## INV-BILLING-APPEND-ONLY
//!
//! Per the WI-S10-001 §1 invariant 5, the sink MUST reject any attempt
//! to overwrite an existing `(tenant, billing_period, seq)` key. This
//! is the canonical analog of `INV-AUDIT-APPEND-ONLY` from S-09 audit
//! chain: production wiring at the CF R2 PutObject layer enforces it
//! via Object Lock Governance Mode 7y retention; the in-memory fake
//! here pins it at the trait surface so adversarial tests can falsify
//! INV-BILLING-APPEND-ONLY independently of the production R2 binding
//! (deferred to WI-S10-007 PRR ship gate per the
//! `trait-abstraction-defer` charter pattern).
//!
//! ## F-001 closure
//!
//! The sink holds the per-(tenant, billing_period) sequence ledger +
//! the captured NDJSON buffer under a per-instance `Arc<Mutex<>>` (NOT
//! a `static LazyLock`). Tests instantiate fresh sinks per case so the
//! orchestrator harness cannot accidentally leak chain state across
//! cases.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::error::R2UsageSinkError;
use crate::event::{IdemKey, UsageEvent};

/// Persisted R2 NDJSON record (canonical key + canonical line +
/// (tenant, billing_period, seq) coordinates). Cloning is cheap so
/// production wiring can fan out to multiple downstreams (R2 PUT +
/// counter aggregator) without re-serializing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedUsageLine {
    /// Canonical R2 object key
    /// (`usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson`).
    pub r2_key: String,
    /// Canonical NDJSON line (no trailing newline).
    pub ndjson: String,
    /// Tenant id of the event.
    pub tenant_id: Uuid,
    /// Canonical billing period (`YYYY-MM`) the event was bucketed
    /// into.
    pub billing_period: String,
    /// Per-(tenant, billing_period) monotonic sequence number.
    pub sequence_number: u64,
    /// 32-byte BLAKE3-256 idempotency key carried by the event (the
    /// derived value, post slot-rewrite).
    pub idem_key: IdemKey,
}

/// R2 sink trait. Production wiring composes:
///
/// - `R2PutObjectUsageSink` — fan-out to CF R2 PutObject with Object
///   Lock Governance Mode 7y retention (deferred to WI-S10-007 PRR
///   ship gate per `trait-abstraction-defer` charter pattern).
pub trait R2UsageSink: Send + Sync + core::fmt::Debug {
    /// Persist `line` durably. Production wiring fans out to CF R2
    /// PutObject + the counter aggregator queue. The trait surface
    /// guarantees INV-BILLING-APPEND-ONLY: the implementation MUST
    /// reject any attempt to overwrite an existing key with
    /// [`R2UsageSinkError::AppendOnlyViolation`].
    ///
    /// # Errors
    ///
    /// Returns [`R2UsageSinkError::Backend`] on any backend failure +
    /// [`R2UsageSinkError::AppendOnlyViolation`] on overwrite attempts.
    fn put(&self, line: PersistedUsageLine) -> Result<(), R2UsageSinkError>;
}

/// Canonical R2 object key per WI-S10-001 §1
/// (`usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson`).
#[must_use]
pub fn canonical_r2_key(
    tenant_id: Uuid,
    billing_period: &str,
    sequence_number: u64,
) -> String {
    format!(
        "usage/{tenant_id}/{billing_period}/{sequence_number:08}.usage.ndjson"
    )
}

/// In-memory append-only orchestrator. Per-(tenant, billing_period)
/// sequence ledger + key-set guard for INV-BILLING-APPEND-ONLY.
/// Cloning shares the underlying buffers (so tests + verifiers can hold
/// independent handles).
#[derive(Clone, Default, Debug)]
pub struct InMemoryR2UsageSink {
    state: Arc<Mutex<SinkState>>,
}

#[derive(Debug, Default)]
struct SinkState {
    // Per-(tenant, billing_period) next sequence number. Cold-start
    // production wiring rehydrates from the durable D1 mirror; the
    // fake starts every chain at 0.
    next_seq: HashMap<(Uuid, String), u64>,
    // Persisted lines (lex-ordered by key when snapshotted).
    buffer: Vec<PersistedUsageLine>,
    // Canonical key set for INV-BILLING-APPEND-ONLY enforcement.
    keys: HashSet<String>,
}

impl InMemoryR2UsageSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every persisted usage line captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PersistedUsageLine> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.buffer.clone()
    }

    /// Snapshot every persisted usage line for a specific tenant +
    /// billing period (chronologically ordered by sequence number;
    /// production wiring streams via R2 list ordered by key prefix).
    #[must_use]
    pub fn snapshot_for_period(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Vec<PersistedUsageLine> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<PersistedUsageLine> = g
            .buffer
            .iter()
            .filter(|l| l.tenant_id == tenant_id && l.billing_period == billing_period)
            .cloned()
            .collect();
        out.sort_by_key(|l| l.sequence_number);
        out
    }

    /// Number of records captured.
    #[must_use]
    pub fn len(&self) -> usize {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.buffer.len()
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the next sequence number for the (tenant, billing_period)
    /// chain. Returns `0` if the chain has no events yet.
    #[must_use]
    pub fn next_sequence(&self, tenant_id: Uuid, billing_period: &str) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.next_seq
            .get(&(tenant_id, billing_period.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Append `event` to the (tenant, billing_period) chain at the
    /// next sequence number. The orchestrator at
    /// [`crate::emitter::InMemoryUsageEventEmitter`] is the canonical
    /// caller; this method exists separately so chaos tests can
    /// adversarially exercise INV-BILLING-APPEND-ONLY in isolation.
    ///
    /// # Errors
    ///
    /// - [`R2UsageSinkError::Backend`] if NDJSON serialization fails.
    /// - [`R2UsageSinkError::AppendOnlyViolation`] if the canonical
    ///   key already exists in the sink (defensive guard for
    ///   INV-BILLING-APPEND-ONLY; production wiring at CF R2
    ///   PutObject reproduces this via Object Lock Governance Mode).
    pub fn append(&self, event: &UsageEvent) -> Result<PersistedUsageLine, R2UsageSinkError> {
        let mut g = self.state.lock().map_err(|_| {
            R2UsageSinkError::Backend("usage sink mutex poisoned".to_string())
        })?;
        let tenant_id = event.data.tenant_id;
        let billing_period = event.data.billing_period.clone();
        let key_pair = (tenant_id, billing_period.clone());
        let seq = g.next_seq.get(&key_pair).copied().unwrap_or(0);
        let r2_key = canonical_r2_key(tenant_id, &billing_period, seq);

        if g.keys.contains(&r2_key) {
            return Err(R2UsageSinkError::AppendOnlyViolation { key: r2_key });
        }

        let ndjson = serde_json::to_string(event).map_err(|e| {
            R2UsageSinkError::Backend(format!("usage NDJSON serialize failed: {e}"))
        })?;

        let line = PersistedUsageLine {
            r2_key: r2_key.clone(),
            ndjson,
            tenant_id,
            billing_period: billing_period.clone(),
            sequence_number: seq,
            idem_key: event.idem_key,
        };

        g.buffer.push(line.clone());
        g.keys.insert(r2_key);
        g.next_seq.insert(key_pair, seq.saturating_add(1));
        Ok(line)
    }

    /// Adversarial helper: attempt to write a line at an EXPLICIT key
    /// (bypassing the per-(tenant, billing_period) sequence ledger);
    /// used by chaos tests to exercise the INV-BILLING-APPEND-ONLY
    /// boundary directly (e.g., re-emit at an existing seq).
    ///
    /// # Errors
    ///
    /// - [`R2UsageSinkError::AppendOnlyViolation`] when the explicit
    ///   key already exists.
    /// - [`R2UsageSinkError::Backend`] when the per-instance mutex is
    ///   poisoned.
    pub fn put_at_explicit_key(
        &self,
        line: PersistedUsageLine,
    ) -> Result<(), R2UsageSinkError> {
        let mut g = self.state.lock().map_err(|_| {
            R2UsageSinkError::Backend("usage sink mutex poisoned".to_string())
        })?;
        if g.keys.contains(&line.r2_key) {
            return Err(R2UsageSinkError::AppendOnlyViolation {
                key: line.r2_key,
            });
        }
        g.keys.insert(line.r2_key.clone());
        g.buffer.push(line);
        Ok(())
    }
}

impl R2UsageSink for InMemoryR2UsageSink {
    fn put(&self, line: PersistedUsageLine) -> Result<(), R2UsageSinkError> {
        self.put_at_explicit_key(line)
    }
}

/// Always-failing R2 sink for adversarial tests.
#[derive(Debug, Default)]
pub struct FailingR2UsageSink;

impl FailingR2UsageSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl R2UsageSink for FailingR2UsageSink {
    fn put(&self, _line: PersistedUsageLine) -> Result<(), R2UsageSinkError> {
        Err(R2UsageSinkError::Backend(
            "induced R2 usage sink failure (test fixture)".to_string(),
        ))
    }
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
    use crate::event::{UsageEventKind, USAGE_EVENT_TYPE};
    use crate::idempotency::{compute_canonical_bytes_for_idem, derive_idem_key_from_canonical};
    use corelink_analytics::Region;

    fn fresh_event(tenant: Uuid, qty: u64, period: &str) -> UsageEvent {
        let mut e = UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            qty,
            period,
        )
        .unwrap();
        let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
        e.idem_key = derive_idem_key_from_canonical(&canonical);
        e
    }

    #[test]
    fn fresh_sink_is_empty() {
        let s = InMemoryR2UsageSink::new();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn fresh_chain_has_zero_next_sequence() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        assert_eq!(s.next_sequence(tenant, "2026-05"), 0);
    }

    #[test]
    fn append_advances_sequence_and_persists_line() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 4096, "2026-05");
        let line = s.append(&e).unwrap();
        assert_eq!(line.sequence_number, 0);
        assert_eq!(line.tenant_id, tenant);
        assert_eq!(line.billing_period, "2026-05");
        assert!(line.r2_key.starts_with("usage/"));
        assert!(line.r2_key.contains("2026-05"));
        assert!(line.r2_key.ends_with("00000000.usage.ndjson"));
        assert_eq!(s.next_sequence(tenant, "2026-05"), 1);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn append_serializes_ndjson_with_canonical_attributes() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 4096, "2026-05");
        let line = s.append(&e).unwrap();
        // No trailing newline (NDJSON delimiter is added by the writer).
        assert!(!line.ndjson.ends_with('\n'));
        // Contains canonical CloudEvents pinned attributes.
        assert!(line.ndjson.contains("\"specversion\":\"1.0\""));
        assert!(line.ndjson.contains(&format!("\"type\":\"{USAGE_EVENT_TYPE}\"")));
        assert!(line.ndjson.contains("\"datacontenttype\":\"application/json\""));
        assert!(line.ndjson.contains("\"region\":\"iad\""));
    }

    #[test]
    fn append_pads_seq_to_eight_digits() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        // Append 12 events to exercise seq 0..11 padding.
        for i in 0..12u64 {
            let mut e = fresh_event(tenant, 1, "2026-05");
            // Re-derive idem_key per event with distinct qty so each
            // canonical-bytes input differs (avoids collision in the
            // test fixture).
            e.data.qty = i;
            let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
            e.idem_key = derive_idem_key_from_canonical(&canonical);
            s.append(&e).unwrap();
        }
        let snap = s.snapshot_for_period(tenant, "2026-05");
        for (i, line) in snap.iter().enumerate() {
            let expected = format!("{i:08}.usage.ndjson");
            assert!(
                line.r2_key.ends_with(&expected),
                "key={} expected suffix {}",
                line.r2_key,
                expected
            );
            assert_eq!(line.sequence_number, i as u64);
        }
    }

    #[test]
    fn put_at_explicit_key_rejects_overwrite() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 1, "2026-05");
        s.append(&e).unwrap();
        // Attempt overwrite at seq=0 explicit key.
        let line = PersistedUsageLine {
            r2_key: canonical_r2_key(tenant, "2026-05", 0),
            ndjson: "{\"k\":\"v\"}".to_string(),
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            sequence_number: 0,
            idem_key: IdemKey::genesis(),
        };
        let err = s.put_at_explicit_key(line).unwrap_err();
        assert!(matches!(err, R2UsageSinkError::AppendOnlyViolation { .. }));
    }

    #[test]
    fn put_trait_method_rejects_overwrite() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        let line = PersistedUsageLine {
            r2_key: canonical_r2_key(tenant, "2026-05", 0),
            ndjson: "{\"k\":\"v\"}".to_string(),
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            sequence_number: 0,
            idem_key: IdemKey::genesis(),
        };
        s.put(line.clone()).unwrap();
        let err = s.put(line).unwrap_err();
        assert!(matches!(err, R2UsageSinkError::AppendOnlyViolation { .. }));
    }

    #[test]
    fn cross_tenant_chains_partitioned() {
        let s = InMemoryR2UsageSink::new();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let mut e_a = fresh_event(tenant_a, 1, "2026-05");
        let mut e_b = fresh_event(tenant_b, 1, "2026-05");
        // Re-derive (tenant_id is part of canonical bytes; keys diverge).
        let c_a = compute_canonical_bytes_for_idem(&e_a).unwrap();
        let c_b = compute_canonical_bytes_for_idem(&e_b).unwrap();
        e_a.idem_key = derive_idem_key_from_canonical(&c_a);
        e_b.idem_key = derive_idem_key_from_canonical(&c_b);
        s.append(&e_a).unwrap();
        s.append(&e_b).unwrap();
        assert_eq!(s.next_sequence(tenant_a, "2026-05"), 1);
        assert_eq!(s.next_sequence(tenant_b, "2026-05"), 1);
        assert_eq!(s.snapshot_for_period(tenant_a, "2026-05").len(), 1);
        assert_eq!(s.snapshot_for_period(tenant_b, "2026-05").len(), 1);
    }

    #[test]
    fn cross_period_chains_partitioned() {
        let s = InMemoryR2UsageSink::new();
        let tenant = Uuid::now_v7();
        let mut e_may = fresh_event(tenant, 1, "2026-05");
        let mut e_jun = fresh_event(tenant, 1, "2026-06");
        let c_may = compute_canonical_bytes_for_idem(&e_may).unwrap();
        let c_jun = compute_canonical_bytes_for_idem(&e_jun).unwrap();
        e_may.idem_key = derive_idem_key_from_canonical(&c_may);
        e_jun.idem_key = derive_idem_key_from_canonical(&c_jun);
        s.append(&e_may).unwrap();
        s.append(&e_jun).unwrap();
        assert_eq!(s.next_sequence(tenant, "2026-05"), 1);
        assert_eq!(s.next_sequence(tenant, "2026-06"), 1);
    }

    #[test]
    fn canonical_r2_key_pinned() {
        let tenant = Uuid::nil();
        let key = canonical_r2_key(tenant, "2026-05", 42);
        assert_eq!(
            key,
            "usage/00000000-0000-0000-0000-000000000000/2026-05/00000042.usage.ndjson"
        );
    }

    #[test]
    fn cloned_sink_shares_state() {
        let s1 = InMemoryR2UsageSink::new();
        let s2 = s1.clone();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 1, "2026-05");
        s1.append(&e).unwrap();
        assert_eq!(s2.len(), 1);
        assert_eq!(s2.next_sequence(tenant, "2026-05"), 1);
    }

    #[test]
    fn failing_sink_returns_backend_error() {
        let s = FailingR2UsageSink::new();
        let err = s
            .put(PersistedUsageLine {
                r2_key: "usage/x/2026-05/00000000.usage.ndjson".to_string(),
                ndjson: "{}".to_string(),
                tenant_id: Uuid::nil(),
                billing_period: "2026-05".to_string(),
                sequence_number: 0,
                idem_key: IdemKey::genesis(),
            })
            .unwrap_err();
        assert!(matches!(err, R2UsageSinkError::Backend(_)));
    }
}
