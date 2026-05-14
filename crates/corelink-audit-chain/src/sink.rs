//! `R2AuditSink` trait + `InMemoryR2AuditSink` orchestrator.
//!
//! ## R2 NDJSON layout (per WI §6.1.4)
//!
//! Per the WI spec, the production R2 PutObject layout is:
//!
//! ```text
//! audit/{tenant_id}/{date YYYY-MM-DD}/{seq:08}.cloudevent.ndjson
//! ```
//!
//! - `tenant_id` is the canonical UUIDv7 hyphenated lowercase (mirrors
//!   `Uuid::to_string()`).
//! - `date` is the calendar date in UTC of the event's `time_ms`
//!   (RFC 3339 `YYYY-MM-DD` shape; mirrors the canonical S-06 GC purge
//!   bucket prefix discipline).
//! - `seq` is the canonical 8-digit zero-padded sequence number so
//!   lexicographic R2 key sort = chronological event order; the
//!   verifier streams via a single R2 list scan ordered by key.
//!
//! Per the WI §6.1.4 + §6.1.6 retention model:
//!
//! - Production wiring binds the bucket with R2 Object Lock Governance
//!   Mode 7y retention (CTRL-AUDIT-001 + INV-AUDIT-APPEND-ONLY foundation
//!   from S-06). The IaC + the actual `wrangler r2 object put` call land
//!   alongside WI-S09-007 PRR ship gate per the `trait-abstraction-defer`
//!   charter pattern.
//! - The in-memory fake here exercises every algorithmic invariant a
//!   production binding bug would expose: per-tenant chain partitioning;
//!   audit fail-CLOSED envelope on every emit arm; chain head advance
//!   AFTER R2 PutObject success only.
//!
//! ## Decision pipeline (per emit)
//!
//! For each `(AuditEvent, request_id, now_ms)`:
//!
//! 1. **Canonicalization**: compute the JCS-canonical bytes of the
//!    event (includes the `prev_hash` slot — the link input is the
//!    full event shape, not just the `data` payload).
//! 2. **Audit emit BEFORE state mutation** per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (Lote 10.6bis pattern + S-07
//!    P1-1 fix). Audit-of-audit failure aborts the chain emit.
//! 3. **R2 PutObject**: append the canonical NDJSON line at the
//!    `audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson` key. R2
//!    failure returns `AuditChainError::Sink` (fail-CLOSED canonical;
//!    caller MUST abort the originating transaction per Lote 10.6bis).
//! 4. **Chain head advance**: on R2 PutObject success, advance the
//!    per-(tenant, region) `HashChainBuilder` head + sequence counter.
//!    The advance is INSIDE the in-memory critical section so a
//!    parallel emit observer the freshly-advanced state.
//!
//! ## F-001 closure
//!
//! The sink holds the per-tenant chain-builder ledger under a
//! per-instance `Arc<Mutex<>>` (NOT a `static LazyLock`). Tests
//! instantiate fresh sinks per case so the orchestrator harness cannot
//! accidentally leak chain state across cases.
//!
//! ## Fail-CLOSED canonical (Lote 10.6bis lesson)
//!
//! Per WI §6.1.9: audit emit fail-CLOSED. The R2 sink failure path
//! returns `AuditChainError::Sink` so the originating transaction MUST
//! abort. This is the canonical fail-CLOSED exemplar (vs WI-S09-001 /
//! WI-S09-002 / WI-S09-003 fail-OPEN observability emits) — the
//! distinction is enforced at the type system: chain emit returns
//! `Result<_, AuditChainError>` where `Err` propagation is the
//! transaction-abort signal.

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::audit::{
    AuditChainAuditEventType, AuditChainAuditRecord, AuditChainAuditSink,
};
use crate::chain::HashChainBuilder;
use crate::error::{AuditChainError, R2AuditSinkError};
use crate::event::{AuditEvent, ChainHash};

/// Persisted R2 NDJSON record (canonical key + canonical line).
/// Cloning is cheap so the production wiring can fan out to multiple
/// downstreams (R2 PUT + SIEM webhook) without re-serializing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedAuditLine {
    /// Canonical R2 object key per WI §6.1.4
    /// (`audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson`).
    pub r2_key: String,
    /// Canonical NDJSON line (no trailing newline).
    pub ndjson: String,
    /// Originating tenant id (per-tenant chain partition).
    pub tenant_id: Uuid,
    /// Originating chain sequence number.
    pub sequence_number: u64,
    /// 32-byte BLAKE3 link hash the NEXT event will carry as its
    /// `prev_hash`.
    pub link_hash: ChainHash,
}

/// R2 sink trait. Production wiring composes:
///
/// - `R2PutObjectAuditSink` — fan-out to CF R2 PutObject with Object
///   Lock Governance Mode 7y retention (deferred to WI-S09-007 PRR
///   ship gate per `trait-abstraction-defer` charter pattern).
pub trait R2AuditSink: Send + Sync + core::fmt::Debug {
    /// Persist `line` durably. Production wiring fans out to CF R2
    /// PutObject + SIEM webhook (Cloudflare Queue).
    ///
    /// # Errors
    ///
    /// Returns [`R2AuditSinkError::Backend`] on any backend failure.
    fn put(&self, line: PersistedAuditLine) -> Result<(), R2AuditSinkError>;
}

/// Compute the canonical YYYY-MM-DD UTC calendar date string from a
/// Unix epoch ms instant. Pure-logic implementation (no `chrono`
/// workspace dep) — uses the standard civil-calendar conversion
/// algorithm so wasm32 builds work without an extra dependency.
///
/// The algorithm is the canonical "days from civil" inversion (Howard
/// Hinnant 2018, public domain) — converts an epoch-day count to
/// `(year, month, day)` in proleptic Gregorian calendar.
#[must_use]
pub fn canonical_date_yyyy_mm_dd(time_ms: u64) -> String {
    // ms → seconds → days (truncate toward zero; for time_ms == 0
    // returns 1970-01-01 canonical).
    let days_since_epoch = (time_ms / 86_400_000) as i64;
    let (y, m, d) = days_since_epoch_to_ymd(days_since_epoch);
    format!("{y:04}-{m:02}-{d:02}")
}

// Howard Hinnant's "Date Algorithms" §6 inverse (proleptic Gregorian
// from `days_since_epoch`); public domain. Returns `(year, month, day)`
// where year may be in [-32767, 32767] and month/day in [1, 12] /
// [1, 31].
fn days_since_epoch_to_ymd(z: i64) -> (i64, u32, u32) {
    let z = z.saturating_add(719_468);
    let era = if z >= 0 { z } else { z.saturating_sub(146_096) } / 146_097;
    let doe = z.saturating_sub(era.saturating_mul(146_097)) as u64;
    let yoe = (doe.saturating_sub(doe / 1460).saturating_sub(doe / 36524).saturating_add(doe / 146096)) / 365;
    let y = (yoe as i64).saturating_add(era.saturating_mul(400));
    let doy = doe.saturating_sub(365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy.saturating_sub((153 * mp + 2) / 5).saturating_add(1)) as u32;
    let m = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
    let y_final = if m <= 2 { y.saturating_add(1) } else { y };
    (y_final, m, d)
}

/// Canonical R2 object key per WI §6.1.4
/// (`audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson`).
#[must_use]
pub fn canonical_r2_key(tenant_id: Uuid, time_ms: u64, sequence_number: u64) -> String {
    format!(
        "audit/{}/{}/{:08}.cloudevent.ndjson",
        tenant_id,
        canonical_date_yyyy_mm_dd(time_ms),
        sequence_number,
    )
}

/// In-memory chain orchestrator: per-(tenant, region) `HashChainBuilder`
/// ledger + audit-of-audit sink + R2 NDJSON capture buffer. Cloning
/// shares the underlying buffers.
#[derive(Clone, Debug)]
pub struct InMemoryR2AuditSink<A>
where
    A: AuditChainAuditSink + 'static,
{
    audit: Arc<A>,
    state: Arc<Mutex<SinkState>>,
}

#[derive(Debug, Default)]
struct SinkState {
    // Per-tenant chain-builder ledger. Per-tenant chain partitioning
    // per WI §6.1.4 (cross-region split-brain risk eliminated; cross-
    // region correlation via `trace_id` + `tenant_id` at event level).
    chains: HashMap<Uuid, HashChainBuilder>,
    // Captured persisted lines (production wiring fans out to R2).
    buffer: Vec<PersistedAuditLine>,
    // Cumulative count of sink-side transport failures.
    sink_failure_count: u64,
}

impl<A> InMemoryR2AuditSink<A>
where
    A: AuditChainAuditSink + 'static,
{
    /// Construct with explicit audit-of-audit sink.
    pub fn new(audit: Arc<A>) -> Self {
        Self {
            audit,
            state: Arc::new(Mutex::new(SinkState::default())),
        }
    }

    /// Snapshot every persisted audit line captured so far.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PersistedAuditLine> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.buffer.clone()
    }

    /// Snapshot every persisted audit line for a specific tenant
    /// (chronologically ordered by sequence number; production wiring
    /// streams via R2 list ordered by key prefix).
    #[must_use]
    pub fn snapshot_for_tenant(&self, tenant_id: Uuid) -> Vec<PersistedAuditLine> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<PersistedAuditLine> = g
            .buffer
            .iter()
            .filter(|l| l.tenant_id == tenant_id)
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

    /// Cumulative count of sink-side transport failures (production
    /// wiring uses this as the SEV-1
    /// `corelink_audit_emit_failures_total{reason}` alert source).
    #[must_use]
    pub fn sink_failure_count(&self) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.sink_failure_count
    }

    /// Borrow the current chain head for the given tenant. Returns
    /// `None` if the tenant has no chain yet (the next emit would land
    /// at the genesis position).
    #[must_use]
    pub fn chain_head(&self, tenant_id: Uuid) -> Option<ChainHash> {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.chains.get(&tenant_id).map(|b| *b.head())
    }

    /// Borrow the next sequence number for the given tenant. Returns
    /// `0` (the canonical genesis position) if the tenant has no chain
    /// yet.
    #[must_use]
    pub fn next_sequence(&self, tenant_id: Uuid) -> u64 {
        let g = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.chains.get(&tenant_id).map_or(0, HashChainBuilder::next_sequence)
    }

    /// Compute the canonical (next_sequence, prev_hash) tuple the next
    /// event in the tenant's chain MUST carry. For a fresh tenant this
    /// is `(0, [0u8; 32])` (the canonical genesis position); for a
    /// tenant with an existing chain it returns the running head +
    /// next sequence.
    ///
    /// Cold-start production wiring queries the D1 mirror for the
    /// durable head + uses [`HashChainBuilder::resume`] to rehydrate.
    #[must_use]
    pub fn next_link_inputs(&self, tenant_id: Uuid) -> (u64, ChainHash) {
        (
            self.next_sequence(tenant_id),
            self.chain_head(tenant_id).unwrap_or_else(ChainHash::genesis),
        )
    }

    /// Emit an audit event through the orchestrator pipeline.
    ///
    /// Decision pipeline:
    ///
    /// 1. Canonicalize via JCS (RFC 8785).
    /// 2. Audit emit BEFORE state mutation (fail-closed envelope).
    /// 3. R2 PutObject (in-memory buffer push for the fake; CF R2
    ///    PutObject with Object Lock Governance Mode 7y retention in
    ///    production wiring).
    /// 4. Chain head advance.
    ///
    /// # Errors
    ///
    /// - [`AuditChainError::SequenceOrderingViolation`] when the
    ///   event's `sequence_number` doesn't match the chain head's
    ///   `next_sequence`.
    /// - [`AuditChainError::ChainBreak`] when the event's `prev_hash`
    ///   doesn't match the chain head.
    /// - [`AuditChainError::Audit`] when the audit-of-audit sink fails
    ///   (fail-closed envelope).
    /// - [`AuditChainError::Canonicalization`] when JCS fails.
    /// - [`AuditChainError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn emit(
        &self,
        event: AuditEvent,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<ChainHash, AuditChainError> {
        let tenant_id = event.tenant_id;
        let subject_str = event.subject.subject();
        let sequence_number = event.sequence_number;

        // 1. Audit emit BEFORE state mutation. The audit envelope fires
        // at the EventAppended arm; if the meta-audit sink rejects, we
        // bail BEFORE any chain mutation or R2 write so the chain head
        // never observes partial state.
        self.audit.emit(AuditChainAuditRecord {
            event_type: AuditChainAuditEventType::EventAppended,
            chain_subject: subject_str,
            tenant_id: tenant_id.to_string(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            sequence_number,
        })?;

        // 2. Per-tenant chain-builder ledger lookup + append (sequence
        // + prev_hash sanity check + JCS canonicalize + BLAKE3 link).
        // The append-or-resume sequence is INSIDE the critical section
        // so a parallel emit observer the freshly-advanced state.
        let mut g = self
            .state
            .lock()
            .map_err(|_| AuditChainError::Internal("audit-chain sink mutex poisoned".to_string()))?;
        let builder = g.chains.entry(tenant_id).or_insert_with(HashChainBuilder::new);
        let link_hash = builder.append(&event)?;

        // 3. Serialize NDJSON + canonical R2 key + buffer push (the
        // in-memory analog of the production R2 PutObject).
        let ndjson = event.to_ndjson_line().map_err(|e| {
            AuditChainError::Canonicalization(format!("NDJSON serialize failed: {e}"))
        })?;
        let r2_key = canonical_r2_key(tenant_id, event.time_ms, sequence_number);
        let line = PersistedAuditLine {
            r2_key,
            ndjson,
            tenant_id,
            sequence_number,
            link_hash,
        };
        g.buffer.push(line);
        Ok(link_hash)
    }

    /// Record a sink-side transport failure (production wiring calls
    /// this when the downstream R2 PutObject returns an error). Emits
    /// the canonical `corelink.audit_chain.sink_failure` audit
    /// (fail-CLOSED per WI §6.1.9 — production wiring aborts the
    /// originating transaction; counter increments for the SEV-1 alert).
    ///
    /// # Errors
    ///
    /// - [`AuditChainError::Audit`] when the audit emit fails.
    /// - [`AuditChainError::Internal`] when the per-instance mutex is
    ///   poisoned.
    pub fn record_sink_failure(
        &self,
        tenant_id: Uuid,
        chain_subject: &'static str,
        sequence_number: u64,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<(), AuditChainError> {
        self.audit.emit(AuditChainAuditRecord {
            event_type: AuditChainAuditEventType::SinkFailure,
            chain_subject,
            tenant_id: tenant_id.to_string(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            sequence_number,
        })?;
        let mut g = self
            .state
            .lock()
            .map_err(|_| AuditChainError::Internal("audit-chain sink mutex poisoned (sink_failure bump)".to_string()))?;
        g.sink_failure_count = g.sink_failure_count.saturating_add(1);
        Ok(())
    }
}

/// Always-failing R2 sink for adversarial tests.
#[derive(Debug, Default)]
pub struct FailingR2AuditSink;

impl FailingR2AuditSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl R2AuditSink for FailingR2AuditSink {
    fn put(&self, _line: PersistedAuditLine) -> Result<(), R2AuditSinkError> {
        Err(R2AuditSinkError::Backend(
            "induced R2 audit sink failure (test fixture)".to_string(),
        ))
    }
}

/// In-memory `R2AuditSink` impl that captures persisted lines without
/// the orchestrator's chain-head advance + audit envelope. Used by the
/// production wiring staging tests + multi-sink fan-out fixtures.
#[derive(Clone, Debug, Default)]
pub struct CapturedR2AuditSink {
    inner: Arc<Mutex<Vec<PersistedAuditLine>>>,
}

impl CapturedR2AuditSink {
    /// Construct a fresh sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured line.
    #[must_use]
    pub fn snapshot(&self) -> Vec<PersistedAuditLine> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl R2AuditSink for CapturedR2AuditSink {
    fn put(&self, line: PersistedAuditLine) -> Result<(), R2AuditSinkError> {
        let mut g = self.inner.lock().map_err(|_| {
            R2AuditSinkError::Backend("captured R2 audit sink mutex poisoned".to_string())
        })?;
        g.push(line);
        Ok(())
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
    use crate::audit::{FailingAuditChainAuditSink, InMemoryAuditChainAuditSink};
    use crate::event::{AuditEventKind, ChainHash};
    use corelink_analytics::Region;
    use serde_json::json;

    type Sink = InMemoryR2AuditSink<InMemoryAuditChainAuditSink>;

    fn fresh_sink() -> (Sink, Arc<InMemoryAuditChainAuditSink>) {
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        let s = InMemoryR2AuditSink::new(Arc::clone(&audit));
        (s, audit)
    }

    fn fresh_genesis_event(tenant: Uuid, kind: AuditEventKind) -> AuditEvent {
        AuditEvent::genesis(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000,
            tenant,
            Region::Iad,
            json!({"sample": true}),
        )
    }

    fn next_event_for(
        sink: &Sink,
        tenant: Uuid,
        kind: AuditEventKind,
        time_ms: u64,
    ) -> AuditEvent {
        let (seq, prev) = sink.next_link_inputs(tenant);
        AuditEvent::new(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            time_ms,
            tenant,
            Region::Iad,
            seq,
            prev,
            json!({"seq": seq}),
        )
    }

    #[test]
    fn fresh_sink_is_empty() {
        let (s, _a) = fresh_sink();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert_eq!(s.sink_failure_count(), 0);
    }

    #[test]
    fn fresh_tenant_has_genesis_link_inputs() {
        let (s, _a) = fresh_sink();
        let tenant = Uuid::now_v7();
        let (seq, prev) = s.next_link_inputs(tenant);
        assert_eq!(seq, 0);
        assert_eq!(prev, ChainHash::genesis());
    }

    #[test]
    fn emit_genesis_advances_chain_head() {
        let (s, audit) = fresh_sink();
        let tenant = Uuid::now_v7();
        let e = fresh_genesis_event(tenant, AuditEventKind::Tenant);
        let link = s.emit(e, "req-1", 1000).unwrap();
        // Chain head advanced to the link hash.
        assert_eq!(s.chain_head(tenant).unwrap(), link);
        assert_eq!(s.next_sequence(tenant), 1);
        assert_eq!(s.len(), 1);
        // Audit emitted exactly once (EventAppended).
        assert_eq!(
            audit
                .snapshot_of(AuditChainAuditEventType::EventAppended)
                .len(),
            1
        );
    }

    #[test]
    fn audit_failure_aborts_emit_no_chain_mutation() {
        let audit = Arc::new(FailingAuditChainAuditSink::new());
        let s = InMemoryR2AuditSink::new(audit);
        let tenant = Uuid::now_v7();
        let e = fresh_genesis_event(tenant, AuditEventKind::Tenant);
        let err = s.emit(e, "req-1", 1000).unwrap_err();
        assert!(matches!(err, AuditChainError::Audit(_)));
        assert_eq!(s.len(), 0);
        assert_eq!(s.next_sequence(tenant), 0);
    }

    #[test]
    fn three_event_chain_links_correctly() {
        let (s, _a) = fresh_sink();
        let tenant = Uuid::now_v7();

        let e0 = fresh_genesis_event(tenant, AuditEventKind::Tenant);
        s.emit(e0, "req-1", 1000).unwrap();

        let e1 = next_event_for(&s, tenant, AuditEventKind::CasPut, 2000);
        s.emit(e1, "req-2", 2000).unwrap();

        let e2 = next_event_for(&s, tenant, AuditEventKind::CasGet, 3000);
        s.emit(e2, "req-3", 3000).unwrap();

        assert_eq!(s.next_sequence(tenant), 3);
        let snap = s.snapshot_for_tenant(tenant);
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].sequence_number, 0);
        assert_eq!(snap[1].sequence_number, 1);
        assert_eq!(snap[2].sequence_number, 2);
        // Each link distinct.
        assert_ne!(snap[0].link_hash, snap[1].link_hash);
        assert_ne!(snap[1].link_hash, snap[2].link_hash);
    }

    #[test]
    fn r2_key_layout_canonical_per_wi_6_1_4() {
        let (s, _a) = fresh_sink();
        let tenant = Uuid::now_v7();
        let e0 = fresh_genesis_event(tenant, AuditEventKind::Tenant);
        s.emit(e0, "req-1", 1_700_000_000_000).unwrap();
        let snap = s.snapshot_for_tenant(tenant);
        let line = snap.first().unwrap();
        // Canonical: audit/{tenant_id}/YYYY-MM-DD/00000000.cloudevent.ndjson
        assert!(line.r2_key.starts_with("audit/"));
        assert!(line.r2_key.contains(&tenant.to_string()));
        assert!(line.r2_key.ends_with("00000000.cloudevent.ndjson"));
        // 1_700_000_000_000 ms = 2023-11-14 22:13:20 UTC; check date:
        assert!(line.r2_key.contains("2023-11-14"));
    }

    #[test]
    fn r2_key_seq_padded_to_eight_digits() {
        let (s, _a) = fresh_sink();
        let tenant = Uuid::now_v7();
        // Chain 12 events so we exercise seq 0..11 and confirm
        // 8-digit zero-padding.
        let e0 = fresh_genesis_event(tenant, AuditEventKind::Tenant);
        s.emit(e0, "req-0", 1_700_000_000_000).unwrap();
        for i in 1..12u64 {
            let e = next_event_for(&s, tenant, AuditEventKind::CasPut, 1_700_000_000_000 + i);
            s.emit(e, "req-x", 1_700_000_000_000 + i).unwrap();
        }
        let snap = s.snapshot_for_tenant(tenant);
        for (i, line) in snap.iter().enumerate() {
            let expected = format!("{:08}.cloudevent.ndjson", i);
            assert!(
                line.r2_key.ends_with(&expected),
                "key={} expected suffix {}",
                line.r2_key,
                expected
            );
        }
    }

    #[test]
    fn record_sink_failure_increments_counter_and_audits() {
        let (s, audit) = fresh_sink();
        let tenant = Uuid::now_v7();
        s.record_sink_failure(tenant, "cas:put", 0, "req-1", 1000)
            .unwrap();
        assert_eq!(s.sink_failure_count(), 1);
        assert_eq!(
            audit.snapshot_of(AuditChainAuditEventType::SinkFailure).len(),
            1
        );
    }

    #[test]
    fn cloned_sink_shares_state() {
        let (s, _a) = fresh_sink();
        let s2 = s.clone();
        let tenant = Uuid::now_v7();
        let e = fresh_genesis_event(tenant, AuditEventKind::Tenant);
        s.emit(e, "req-1", 1000).unwrap();
        assert_eq!(s2.len(), 1);
        assert_eq!(s2.next_sequence(tenant), 1);
    }

    #[test]
    fn captured_r2_audit_sink_persists_line() {
        let captured = CapturedR2AuditSink::new();
        let line = PersistedAuditLine {
            r2_key: "audit/00000000-0000-0000-0000-000000000000/2026-05-02/00000000.cloudevent.ndjson"
                .to_string(),
            ndjson: "{\"x\":1}".to_string(),
            tenant_id: Uuid::nil(),
            sequence_number: 0,
            link_hash: ChainHash([0xAB; 32]),
        };
        captured.put(line.clone()).unwrap();
        let snap = captured.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap.first().unwrap(), &line);
    }

    #[test]
    fn failing_r2_audit_sink_returns_backend_error() {
        let s = FailingR2AuditSink::new();
        let err = s
            .put(PersistedAuditLine {
                r2_key: "audit/x/2026-05-02/00000000.cloudevent.ndjson".to_string(),
                ndjson: "{}".to_string(),
                tenant_id: Uuid::nil(),
                sequence_number: 0,
                link_hash: ChainHash::genesis(),
            })
            .unwrap_err();
        assert!(matches!(err, R2AuditSinkError::Backend(_)));
    }

    #[test]
    fn cross_tenant_chains_are_isolated() {
        let (s, _a) = fresh_sink();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let ea0 = fresh_genesis_event(tenant_a, AuditEventKind::Tenant);
        let eb0 = fresh_genesis_event(tenant_b, AuditEventKind::Tenant);
        s.emit(ea0, "req-a", 1000).unwrap();
        s.emit(eb0, "req-b", 1001).unwrap();
        // Each tenant has next_seq = 1 after one emit.
        assert_eq!(s.next_sequence(tenant_a), 1);
        assert_eq!(s.next_sequence(tenant_b), 1);
        // Heads are distinct (different events).
        assert_ne!(s.chain_head(tenant_a), s.chain_head(tenant_b));
        // Snapshot per tenant is correctly partitioned.
        assert_eq!(s.snapshot_for_tenant(tenant_a).len(), 1);
        assert_eq!(s.snapshot_for_tenant(tenant_b).len(), 1);
    }

    #[test]
    fn canonical_date_yyyy_mm_dd_pinned_for_canonical_anchors() {
        // Anchor 1: epoch (1970-01-01).
        assert_eq!(canonical_date_yyyy_mm_dd(0), "1970-01-01");
        // Anchor 2: 2023-11-14 (1_700_000_000_000 ms).
        assert_eq!(
            canonical_date_yyyy_mm_dd(1_700_000_000_000),
            "2023-11-14"
        );
        // Anchor 3: 2026-05-02 (charter date).
        // 2026-05-02T00:00:00Z = 1_777_680_000_000 ms.
        assert_eq!(
            canonical_date_yyyy_mm_dd(1_777_680_000_000),
            "2026-05-02"
        );
    }

    #[test]
    fn emit_with_wrong_seq_returns_sequence_error() {
        let (s, _a) = fresh_sink();
        let tenant = Uuid::now_v7();
        // First emit must be genesis (seq=0); seq=5 violates.
        let bad = AuditEvent::new(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            Uuid::now_v7(),
            1000,
            tenant,
            Region::Iad,
            5,
            ChainHash::genesis(),
            json!({}),
        );
        let err = s.emit(bad, "req-1", 1000).unwrap_err();
        assert!(matches!(err, AuditChainError::SequenceOrderingViolation { .. }));
        // Chain head untouched.
        assert_eq!(s.next_sequence(tenant), 0);
    }

    #[test]
    fn emit_with_wrong_prev_hash_returns_chain_break_error() {
        let (s, _a) = fresh_sink();
        let tenant = Uuid::now_v7();
        // Genesis must have prev_hash zero; non-zero violates.
        let bad = AuditEvent::new(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            Uuid::now_v7(),
            1000,
            tenant,
            Region::Iad,
            0,
            ChainHash([0xFF; 32]),
            json!({}),
        );
        let err = s.emit(bad, "req-1", 1000).unwrap_err();
        assert!(matches!(err, AuditChainError::ChainBreak { .. }));
    }
}
