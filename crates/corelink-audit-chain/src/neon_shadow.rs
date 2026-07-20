//! Neon Postgres analytics shadow sync for the audit chain (Wave 18 GA).
//!
//! ## What this module ships
//!
//! The R2 NDJSON archive (`archive_producer.rs`, Wave 15) is the
//! **canonical** chain-integrity store. The daily-verify cron walks R2
//! and treats any chain break there as SEV-0.
//!
//! Customer-facing analytics queries (multi-event aggregation,
//! time-range filtering, tenant-cross-referencing) are painful on R2
//! NDJSON. To unblock them, every successfully-flushed R2 archive chunk
//! is also mirrored to a Neon Postgres **analytics shadow** table
//! (`audit_events_shadow`; see `migrations/neon/0001_audit_events_shadow.sql`).
//!
//! The shadow is **analytics-only** — never authoritative for chain
//! integrity. A Neon write failure surfaces SEV-2 (analytics lag), NOT
//! SEV-0 (chain break). The split mirrors the storage tier discipline:
//! R2 = retention + integrity; Neon = analytics + customer SQL.
//!
//! ## Source-of-truth invariant
//!
//! Per `INV-OBS-AUDIT-CHAIN-INTEGRITY` (HIGH) the canonical chain head
//! lives in R2. On any analytics anomaly the operator pivots to R2 +
//! the canonical verifier (`corelink audit verify`). The shadow is a
//! convenience read tier — divergence is observable + bounded by the
//! lag SLO (≤ 5 min nominal; ≥ 60 min → SEV-2 page).
//!
//! ## Lag SLO
//!
//! - **Nominal:** ≤ 5 min (acceptable for analytics; mirrors the
//!   archive producer's 5-min flush cadence).
//! - **Soft ceiling:** ≥ 60 min → SEV-2 alert. The shadow is allowed to
//!   lag the canonical R2 archive — the alert exists so operators
//!   diagnose Neon platform issues before customers complain about
//!   "stale analytics".
//!
//! ## Audit emit shape (CloudEvents 1.0)
//!
//! On every successful shadow sync we emit
//! `corelink.audit.neon_shadow_synced.v1` carrying the chunk's
//! `(tenant_id, first_seq, last_seq, observed_lag_ms, region)`.
//!
//! On any shadow-sync failure we emit
//! `corelink.audit.neon_shadow_sync_failed.v1` SEV-2. The R2 archive
//! producer is NOT aborted by a shadow-sync failure — R2 commit succeeded;
//! Neon shadow is best-effort.
//!
//! ## Residency (INV-DATA-RESIDENCY CRITICAL)
//!
//! The shadow row carries the chunk's pinned `Region`. Production wiring
//! routes the write to the per-region Neon project; the trait surface
//! here is region-pinned at construction so a misrouted call is caught
//! at the type system rather than at the platform layer.
//!
//! ## Tenant isolation (INV-AUTH-SCHEMA-RLS-DEFAULT-ON CRITICAL)
//!
//! Tenant scope is enforced at TWO layers:
//!
//! 1. **App layer** — `NeonShadowSink::sync_chunk` takes a `tenant_id`
//!    argument; every shadow row carries that tenant_id; the
//!    `InMemoryNeonShadowSink` rejects cross-tenant query attempts
//!    fail-CLOSED so the SQL-layer RLS bug equivalent is caught at
//!    test time.
//! 2. **SQL layer** — the Neon table has `ENABLE ROW LEVEL SECURITY` plus
//!    a `tenant_isolation_audit_events_shadow` policy keyed on
//!    `current_setting('app.current_tenant')`; the production
//!    `RealNeonShadowSink` sets `app.current_tenant` inside every txn.
//!
//! ## No-tokio, wasm-clean
//!
//! Per the autonomous execution charter: no `tokio` in `src/`. The
//! sink trait is **synchronous**; the production CF Worker adapter
//! (deferred to a follow-on WI) wraps the call in `worker::send::SendFuture`
//! the same way `CfR2BucketAdapter` does (`corelink-cf-bindings::cf_r2`).
//!
//! ## INV pin-list (compile-time invariant constants)
//!
//! Pinned in unit tests so toggling a constant without an ADR is caught
//! at CI:
//!
//! - `SHADOW_LAG_NOMINAL_MAX_MS` = 5 min (300_000 ms).
//! - `SHADOW_LAG_SEV2_THRESHOLD_MS` = 60 min (3_600_000 ms).
//! - `EVENT_TYPE_SHADOW_SYNCED` = `"corelink.audit.neon_shadow_synced.v1"`.
//! - `EVENT_TYPE_SHADOW_SYNC_FAILED` = `"corelink.audit.neon_shadow_sync_failed.v1"`.

// Wave-20 (B-P3-02 closure): the wave-15 module-level
// `#![allow(clippy::uninlined_format_args)]` is dropped; every `format!`
// in this module uses the inlined-args style (`format!("{x}")`, NOT
// `format!("{}", x)`).

/// Production `RealNeonShadowSink` driver — closes the wave-18
/// caveat #3 (in-memory fake shipped wave-18; this submodule ships
/// the real Postgres driver with per-region project resolver +
/// `app.current_tenant` RLS GUC SET + idempotent ON CONFLICT INSERT +
/// aggregate-query SQL). See `real.rs` doc.
pub mod real;

/// Wave-20 closure: `tokio-postgres` + `deadpool-postgres` adapter
/// that satisfies the [`real::NeonExecutor`] trait. Native-only +
/// gated by the `neon-real` Cargo feature so wasm32 + default builds
/// stay free of the Postgres driver transitive deps. The wasm32 stub
/// is always compiled in so the `Arc<dyn NeonExecutor>` wiring type-
/// checks on both targets — every stub method surfaces
/// [`real::NeonError::WasmOnly`].
///
/// Pattern reference: `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md`
/// §7 (wave-20 closure-note appended).
pub mod real_tokio_pg;

/// Wave-21 closure: per-tenant pinned-region resolver replacing the
/// hard-coded `Region::Iad` default in `apps/server/src/main.rs`
/// (`TokioPgShadowSinkFactory::for_tenant`). Ships the trait surface
/// + an in-memory map impl (dev / staging / tests) + a D1-backed
/// production impl that delegates to a [`tenant_region::TenantConfigStore`]
/// wrapping the new `tenant_config` D1 table (migration
/// `0052_tenant_config_region.sql`).
///
/// Pattern reference: `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md`
/// §7 (wave-21 closure-note appended).
pub mod tenant_region;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_analytics::Region;

use crate::archive_producer::ArchiveReceipt;
use crate::event::ChainHash;
use crate::sink::PersistedAuditLine;

/// Nominal shadow lag ceiling: 5 min (300_000 ms). Matches the
/// `archive_producer::DEFAULT_FLUSH_AFTER_MS` cadence so the shadow can
/// be at most one flush behind in steady state.
pub const SHADOW_LAG_NOMINAL_MAX_MS: u64 = 5 * 60 * 1_000;

/// SEV-2 lag threshold: 60 min (3_600_000 ms). Breach surfaces a
/// PagerDuty SEV-2 (analytics lag) — never SEV-0 (R2 is canonical).
pub const SHADOW_LAG_SEV2_THRESHOLD_MS: u64 = 60 * 60 * 1_000;

/// Canonical CloudEvents `type` for the successful shadow-sync emit.
pub const EVENT_TYPE_SHADOW_SYNCED: &str = "corelink.audit.neon_shadow_synced.v1";

/// Canonical CloudEvents `type` for the failed shadow-sync emit.
pub const EVENT_TYPE_SHADOW_SYNC_FAILED: &str = "corelink.audit.neon_shadow_sync_failed.v1";

/// One row of the `audit_events_shadow` Neon table — analytics-only
/// projection of one chain link. The R2 NDJSON archive remains the
/// authoritative chain store; this row is a convenience read tier.
///
/// `Region` carries no serde derive (the enum is `#[non_exhaustive]`
/// in `corelink-analytics::labels` and serializing a stable canonical
/// 3-letter colocode is the on-the-wire shape). For wire emission use
/// `region.as_str()` directly.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShadowEventRow {
    /// Tenant id (per-tenant chain partition; populated from
    /// `PersistedAuditLine::tenant_id`).
    pub tenant_id: Uuid,
    /// Chain sequence number (`PersistedAuditLine::sequence_number`).
    pub seq: u64,
    /// Event wall-clock instant (Unix epoch ms; extracted from the
    /// NDJSON `time_ms` field).
    pub event_time_ms: u64,
    /// Canonical CloudEvents `type` (extracted from the NDJSON
    /// `event_type` field; matches `AuditEventKind::event_type`).
    pub event_type: String,
    /// 32-byte BLAKE3 `prev_hash` of the previous chain link.
    pub prev_hash: ChainHash,
    /// 32-byte BLAKE3 `link_hash` of THIS event (the chain head AFTER
    /// this row; equals `PersistedAuditLine::link_hash`).
    pub link_hash: ChainHash,
    /// Region the chunk was emitted in (informational + residency pin).
    pub region: Region,
    /// Raw NDJSON line — Neon stores it as `JSONB` for SQL queries on
    /// arbitrary data fields without parsing at write time.
    pub payload_json: String,
}

impl ShadowEventRow {
    /// Construct a row with every field explicit. Provided so external
    /// test harnesses can build a row without struct-expression access
    /// (the type is `#[non_exhaustive]` for additive growth).
    ///
    /// Wave-20 (B-P3-01 closure): the 8-arg signature is a deliberate
    /// compromise — the row is `#[non_exhaustive]`, so external callers
    /// CANNOT build it via struct expression even from within the same
    /// crate. A builder pattern (`ShadowEventRow::builder()...build()`)
    /// would lift the lint waiver but couples the row construction to
    /// runtime validation; the current explicit-arg shape keeps the
    /// type a pure data carrier. Tracked as cosmetic follow-on.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        seq: u64,
        event_time_ms: u64,
        event_type: String,
        prev_hash: ChainHash,
        link_hash: ChainHash,
        region: Region,
        payload_json: String,
    ) -> Self {
        Self {
            tenant_id,
            seq,
            event_time_ms,
            event_type,
            prev_hash,
            link_hash,
            region,
            payload_json,
        }
    }

    /// Build a row from a freshly-flushed [`PersistedAuditLine`] +
    /// pinned region.
    ///
    /// Wave-21 (B-P1-05 closure): returns a `Result<_, ParseError>`
    /// instead of the wave-18 "best-effort" silent default
    /// (`event_time_ms = 0` / `event_type = ""`) which silently inflated
    /// the analytics `[0..granularity)` bucket. The producer-side wiring
    /// (`archive_producer.rs`) is the only happy-path caller, and it
    /// ALWAYS writes well-formed NDJSON (every line round-trips through
    /// `AuditEvent::serialize` → `serde_json::to_string`), so the
    /// `Err` path is structurally unreachable on the production happy
    /// path. The Result is surfaced anyway so:
    ///
    /// 1. A wire-shape mutation (a future field rename / type change)
    ///    that breaks the canonical fields surfaces a compile-time
    ///    type error at every call site rather than silently admitting
    ///    bogus rows.
    /// 2. Adversarial fixtures (replay of a corrupted R2 line) get a
    ///    typed error instead of a malformed-but-accepted row.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] when the NDJSON line is not valid JSON or
    /// is missing one of the canonical required fields (`time_ms`,
    /// `type`, `prev_hash` as 32-byte hex).
    pub fn from_persisted_line(
        line: &PersistedAuditLine,
        region: Region,
    ) -> Result<Self, ParseError> {
        let v: serde_json::Value = serde_json::from_str(&line.ndjson)
            .map_err(|e| ParseError::InvalidJson(e.to_string()))?;
        let event_time_ms = v
            .get("time_ms")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ParseError::MissingField("time_ms"))?;
        // CloudEvents canonical attribute name is `type`
        // (`AuditEvent` serializes via `#[serde(rename = "type")]`).
        let event_type = v
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or(ParseError::MissingField("type"))?
            .to_string();
        let prev_hash_hex = v
            .get("prev_hash")
            .and_then(serde_json::Value::as_str)
            .ok_or(ParseError::MissingField("prev_hash"))?;
        let mut buf = [0u8; 32];
        hex::decode_to_slice(prev_hash_hex, &mut buf)
            .map_err(|e| ParseError::InvalidPrevHash(e.to_string()))?;
        let prev_hash = ChainHash(buf);
        Ok(Self {
            tenant_id: line.tenant_id,
            seq: line.sequence_number,
            event_time_ms,
            event_type,
            prev_hash,
            link_hash: line.link_hash,
            region,
            payload_json: line.ndjson.clone(),
        })
    }
}

/// Parse-error surface for [`ShadowEventRow::from_persisted_line`].
///
/// Wave-21 closure of B-P1-05 (per
/// `specs/_audits/sealed/2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md`
/// §6): the wave-18 implementation silently admitted malformed NDJSON
/// rows with `event_time_ms = 0` and `event_type = ""`, which inflated
/// the analytics `[0..granularity)` bucket. This typed error is the
/// structural fail-CLOSED downgrade so a future wire-shape mutation
/// surfaces at the type system rather than as a silent data-quality
/// regression.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// The NDJSON line did not parse as a JSON value.
    #[error("invalid NDJSON: {0}")]
    InvalidJson(String),

    /// A canonical required field was absent from the NDJSON line.
    #[error("missing canonical NDJSON field: {0}")]
    MissingField(&'static str),

    /// The `prev_hash` field was present but not a valid 32-byte hex
    /// string (e.g. wrong length / non-hex chars).
    #[error("prev_hash hex decode failed: {0}")]
    InvalidPrevHash(String),
}

/// Outcome of a successful [`NeonShadowSink::sync_chunk`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShadowSyncReceipt {
    /// Tenant id the chunk belongs to.
    pub tenant_id: Uuid,
    /// First sequence number persisted to the shadow.
    pub first_seq: u64,
    /// Last sequence number persisted to the shadow.
    pub last_seq: u64,
    /// Number of rows persisted (= `last_seq - first_seq + 1` for a
    /// contiguous chunk).
    pub rows_persisted: u64,
    /// Observed lag in ms between the R2 chunk's first event wall-clock
    /// and the `now_ms` passed to `sync_chunk`. Used to drive the SEV-2
    /// lag alert.
    pub observed_lag_ms: u64,
    /// Region the chunk was sync'd into (residency pin; informational).
    pub region: Region,
}

impl ShadowSyncReceipt {
    /// `true` iff this sync's observed lag is within the nominal SLO
    /// (≤ 5 min). Informational — analytics queries are still served
    /// when `false` (the shadow is best-effort).
    #[must_use]
    pub fn within_nominal_lag(&self) -> bool {
        self.observed_lag_ms <= SHADOW_LAG_NOMINAL_MAX_MS
    }

    /// `true` iff this sync's observed lag crossed the SEV-2 threshold
    /// (≥ 60 min). Production wiring pages an on-call when this returns
    /// `true` for 5 consecutive samples.
    #[must_use]
    pub fn breaches_sev2_threshold(&self) -> bool {
        self.observed_lag_ms >= SHADOW_LAG_SEV2_THRESHOLD_MS
    }
}

/// Wave-20 (B-P2-02 closure) — pseudonymize a tenant UUID to its first 8
/// hex characters for safe surfacing via the `Display` impl on
/// [`NeonShadowError`]. The full UUID is retained in the structured error
/// fields for downstream Splunk / Drata correlation.
#[must_use]
pub(crate) fn redact_tenant_uuid(uuid: &Uuid) -> String {
    let s = uuid.simple().to_string();
    s.chars().take(8).collect()
}

/// Canonical error surface for the Neon shadow-sync pipeline.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NeonShadowError {
    /// The sync was bound to `tenant_a` but received a row carrying
    /// `tenant_b`. Fail-CLOSED (tenant isolation). The corresponding
    /// SQL-layer enforcement is RLS keyed on `app.current_tenant`.
    ///
    /// **Display redaction (wave-20 B-P2-02 closure):** the `Display`
    /// impl is generated via `thiserror`'s `#[error(...)]` over the
    /// `sink_tenant_redacted` / `observed_tenant_redacted` fields, which
    /// carry the FIRST 8 HEX CHARACTERS of the UUID only. The full UUIDs
    /// are preserved as separate `sink_tenant` / `observed_tenant`
    /// structured fields for downstream Splunk / Drata correlation, but
    /// a log-scrape of a shared logging pipeline never surfaces the raw
    /// UUID. Mirrors the `audit_export.rs` discipline (the route never
    /// logs raw tenant_id). The 8-hex prefix is collision-safe at the
    /// per-tenant-incident granularity (1/16^8 ~= 1/4B).
    #[error(
        "neon shadow tenant isolation violation: sink tenant={sink_tenant_redacted}, observed tenant={observed_tenant_redacted}"
    )]
    TenantIsolationViolation {
        /// Sink-bound tenant id (full UUID, for structured-field correlation).
        sink_tenant: String,
        /// Sink-bound tenant id (8-hex prefix; surfaced via `Display`).
        sink_tenant_redacted: String,
        /// Offending row's tenant id (full UUID, for structured-field correlation).
        observed_tenant: String,
        /// Offending row's tenant id (8-hex prefix; surfaced via `Display`).
        observed_tenant_redacted: String,
    },

    /// The sync was bound to `region_a` but received a row carrying
    /// `region_b`. Fail-CLOSED (INV-DATA-RESIDENCY CRITICAL — a Neon
    /// project per region; cross-region writes violate Schrems II +
    /// LGPD Art. 33).
    #[error(
        "neon shadow residency violation: sink region={sink_region}, observed region={observed_region}"
    )]
    ResidencyViolation {
        /// Sink-bound region.
        sink_region: &'static str,
        /// Offending row's region.
        observed_region: &'static str,
    },

    /// The Neon backend (or test fake) rejected the INSERT.
    /// SEV-2 (analytics lag), not SEV-0 (R2 remains authoritative).
    #[error("neon shadow backend error: {0}")]
    Backend(String),

    /// Internal invariant violation (mutex poisoned, empty chunk, etc.).
    #[error("neon shadow internal invariant violated: {0}")]
    Internal(String),

    /// The downstream audit-emit pipeline rejected a shadow-sync audit
    /// row.
    ///
    /// Wave-21 (B-P1-02 closure): replaces the wave-18 `let _ = ...`
    /// discard pattern on `audit_sink.emit(...)`. Surfaces SEV-1 to the
    /// caller; the route layer translates this to a 503 response so the
    /// analytics-of-audit pager fires loudly when the audit sink is
    /// flapping (the previous behaviour silently swallowed the
    /// audit-emit failure, masking the SEV-2 anchor that the security
    /// team subscribes to). The R2 archive remains the chain-integrity
    /// source of truth — this error never implies a chain break.
    ///
    /// The wrapped string is the static reason returned by
    /// [`ShadowSyncAuditSink::emit`] (e.g. `"shadow audit sink mutex
    /// poisoned"`).
    #[error("neon shadow audit-emit failed: {0}")]
    AuditEmitFailed(&'static str),
}

/// Audit emit record for the shadow-sync pipeline. Production wiring
/// composes this with the CloudEvents-1.0 envelope used by the rest of
/// the audit-chain emits. Captured by the in-memory fake at test time.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShadowSyncAuditRow {
    /// Canonical CloudEvents `type` (`EVENT_TYPE_SHADOW_SYNCED` or
    /// `EVENT_TYPE_SHADOW_SYNC_FAILED`).
    pub event_type: &'static str,
    /// Tenant id (per-tenant chain partition).
    pub tenant_id: Uuid,
    /// First sequence number in the chunk being sync'd (`0` on the
    /// pre-sync rejected paths).
    pub first_seq: u64,
    /// Last sequence number in the chunk being sync'd.
    pub last_seq: u64,
    /// Region the chunk targets.
    pub region: Region,
    /// Observed lag in ms (best-effort on failure paths; the failure
    /// arm captures the `now_ms - first_event_time_ms` at attempt time).
    pub observed_lag_ms: u64,
    /// Operator-visible failure reason (empty on success).
    pub failure_reason: String,
    /// SEV classification: `"sev-2"` on failure / lag-breach;
    /// `"info"` on nominal sync.
    pub sev: &'static str,
}

/// Audit-emit boundary for shadow-sync events.
pub trait ShadowSyncAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one [`ShadowSyncAuditRow`]. Production wiring binds this
    /// to the canonical CloudEvents emitter (the same path used by
    /// `archive_producer` for its `sink_failure` arm).
    ///
    /// # Errors
    ///
    /// Returns a static error string when the audit pipeline is closed.
    /// The shadow sink does NOT abort R2 on audit-emit failure — the
    /// R2 archive already committed; shadow sync is best-effort and a
    /// missed audit-of-audit row is captured in the next sync window's
    /// reconciliation (RB-NEON-SHADOW-LAG runbook step 3).
    fn emit(&self, row: ShadowSyncAuditRow) -> Result<(), &'static str>;
}

/// In-memory capture sink for shadow-sync audit emits. Cloning shares
/// the captured buffer.
#[derive(Clone, Debug, Default)]
pub struct InMemoryShadowSyncAuditSink {
    inner: Arc<Mutex<Vec<ShadowSyncAuditRow>>>,
}

impl InMemoryShadowSyncAuditSink {
    /// Construct a fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured row in emit order.
    ///
    /// # Errors
    ///
    /// Returns a static error string when the mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<ShadowSyncAuditRow>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "shadow audit sink mutex poisoned")?;
        Ok(g.clone())
    }

    /// Number of captured rows.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// `true` iff no rows have been captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ShadowSyncAuditSink for InMemoryShadowSyncAuditSink {
    fn emit(&self, row: ShadowSyncAuditRow) -> Result<(), &'static str> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "shadow audit sink mutex poisoned")?;
        g.push(row);
        Ok(())
    }
}

/// Neon analytics shadow sink trait.
///
/// Production wiring binds [`real::RealNeonShadowSink`] (deferred follow-on
/// WI — needs the `tokio-postgres` / Neon serverless driver wiring).
/// Tests bind [`InMemoryNeonShadowSink`].
///
/// The sink is bound to a `(tenant_id, region)` pair at construction
/// time so cross-tenant / cross-region writes are caught at the type
/// system rather than at the SQL layer (defense in depth — RLS is the
/// authoritative tenant gate, but a wiring bug that drops the GUC SET
/// would otherwise silently leak across tenants).
pub trait NeonShadowSink: Send + Sync + core::fmt::Debug {
    /// Tenant id this sink is bound to.
    fn tenant_id(&self) -> Uuid;

    /// Region this sink writes into.
    fn region(&self) -> Region;

    /// Persist every row of a freshly-flushed R2 archive chunk to the
    /// `audit_events_shadow` Neon table.
    ///
    /// **Pre-validation pass (wave-20 B-P2-01 closure):** implementations
    /// MUST validate tenant_id and region per row; the canonical
    /// `InMemoryNeonShadowSink` implementation walks the rows ONCE with
    /// both checks inlined (single-pass; short-circuits on the FIRST
    /// mismatch). Implementations are NOT required to surface every
    /// violation in the chunk — fail-CLOSED on the first observed
    /// mismatch is the canonical contract. The chunk-size cap is bounded
    /// by `archive_producer::DEFAULT_FLUSH_AFTER_LINES` (canonical 25
    /// rows in steady state), so the O(n) pass is bounded at the producer.
    ///
    /// # Errors
    ///
    /// - [`NeonShadowError::TenantIsolationViolation`] if any row's
    ///   `tenant_id` mismatches the sink's bound tenant.
    /// - [`NeonShadowError::ResidencyViolation`] if `region` mismatches
    ///   the sink's bound region.
    /// - [`NeonShadowError::Backend`] on Neon driver error (SEV-2 lag).
    /// - [`NeonShadowError::Internal`] on mutex poisoning / empty chunk.
    fn sync_chunk(
        &self,
        receipt: &ArchiveReceipt,
        rows: &[ShadowEventRow],
        now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError>;

    /// Aggregate the per-event-type count over a `[from_ms, to_ms)`
    /// window for the bound tenant.
    ///
    /// # Errors
    ///
    /// Returns [`NeonShadowError::Backend`] on driver failure.
    fn aggregate_event_count(
        &self,
        from_ms: u64,
        to_ms: u64,
        event_type_filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError>;

    /// Aggregate a time-bucketed count over a `[from_ms, to_ms)` window
    /// for the bound tenant, with bucket size = `granularity_ms`.
    ///
    /// # Errors
    ///
    /// Returns [`NeonShadowError::Backend`] on driver failure.
    fn aggregate_timeline(
        &self,
        from_ms: u64,
        to_ms: u64,
        granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError>;
}

/// One row of the event-count aggregate query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EventCountBucket {
    /// Event type bucket (matches the CloudEvents `type`).
    pub event_type: String,
    /// Count of rows in the `[from_ms, to_ms)` window.
    pub count: u64,
}

/// One row of the time-bucketed timeline aggregate query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TimelineBucket {
    /// Bucket start (Unix epoch ms; inclusive). Bucket end is
    /// `bucket_start_ms + granularity_ms` (exclusive).
    pub bucket_start_ms: u64,
    /// Count of rows whose `event_time_ms` falls in `[bucket_start, bucket_end)`.
    pub count: u64,
}

impl EventCountBucket {
    /// Construct an event-count bucket. Public so out-of-crate
    /// [`NeonShadowSink`] implementations (e.g. the container's
    /// D1-backed analytics sink over `customer_audit_events`) can build
    /// the aggregate result — the struct is `#[non_exhaustive]`, so a
    /// struct literal is not constructible outside this crate.
    #[must_use]
    pub fn new(event_type: String, count: u64) -> Self {
        Self { event_type, count }
    }
}

impl TimelineBucket {
    /// Construct a timeline bucket. Public for the same reason as
    /// [`EventCountBucket::new`] — the struct is `#[non_exhaustive]`.
    #[must_use]
    pub fn new(bucket_start_ms: u64, count: u64) -> Self {
        Self {
            bucket_start_ms,
            count,
        }
    }
}

/// In-memory shadow sink for tests + adversarial fixtures. Captures
/// every successfully-persisted row + supports the same aggregate-query
/// API as `RealNeonShadowSink` so end-to-end tests exercise the SQL
/// shape without a live Postgres.
#[derive(Debug)]
pub struct InMemoryNeonShadowSink {
    tenant_id: Uuid,
    region: Region,
    audit_sink: Arc<dyn ShadowSyncAuditSink>,
    rows: Mutex<Vec<ShadowEventRow>>,
    injected_failure: Mutex<Option<String>>,
}

impl InMemoryNeonShadowSink {
    /// Construct a sink bound to `(tenant_id, region)`. Every
    /// `sync_chunk` call asserts both pins.
    #[must_use]
    pub fn new(tenant_id: Uuid, region: Region, audit_sink: Arc<dyn ShadowSyncAuditSink>) -> Self {
        Self {
            tenant_id,
            region,
            audit_sink,
            rows: Mutex::new(Vec::new()),
            injected_failure: Mutex::new(None),
        }
    }

    /// Snapshot every persisted row.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ShadowEventRow> {
        match self.rows.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of persisted rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        match self.rows.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Inject a backend failure for the next `sync_chunk` calls.
    /// Setting to `None` clears the injection.
    ///
    /// # Errors
    ///
    /// Returns [`NeonShadowError::Internal`] when the mutex is poisoned.
    pub fn inject_failure(&self, msg: Option<String>) -> Result<(), NeonShadowError> {
        let mut g = self
            .injected_failure
            .lock()
            .map_err(|_| NeonShadowError::Internal("inject_failure mutex poisoned".into()))?;
        *g = msg;
        Ok(())
    }
}

impl NeonShadowSink for InMemoryNeonShadowSink {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn region(&self) -> Region {
        self.region
    }

    fn sync_chunk(
        &self,
        receipt: &ArchiveReceipt,
        rows: &[ShadowEventRow],
        now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError> {
        // 1. Tenant + residency pre-checks. The chunk receipt's tenant
        //    id MUST match the sink's bound tenant; every row's tenant
        //    id MUST match too.
        //
        // Wave-21 (B-P1-02 closure): audit-emit failure on the fail-
        // CLOSED arms is NO LONGER swallowed — emit failure surfaces
        // `NeonShadowError::AuditEmitFailed` (SEV-1 over SEV-2; the
        // route layer translates to 503 so the security team's pager
        // fires loudly when the audit sink is flapping).
        if receipt.tenant_id != self.tenant_id {
            // Audit-emit BEFORE returning (fail-CLOSED ordering mirrors
            // `archive_producer::sink_failure`).
            self.audit_sink
                .emit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms: 0,
                    failure_reason: "tenant isolation violation".to_string(),
                    sev: "sev-2",
                })
                .map_err(NeonShadowError::AuditEmitFailed)?;
            return Err(NeonShadowError::TenantIsolationViolation {
                sink_tenant: self.tenant_id.to_string(),
                sink_tenant_redacted: redact_tenant_uuid(&self.tenant_id),
                observed_tenant: receipt.tenant_id.to_string(),
                observed_tenant_redacted: redact_tenant_uuid(&receipt.tenant_id),
            });
        }
        for row in rows {
            if row.tenant_id != self.tenant_id {
                self.audit_sink
                    .emit(ShadowSyncAuditRow {
                        event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                        tenant_id: self.tenant_id,
                        first_seq: receipt.first_sequence_number,
                        last_seq: receipt.last_sequence_number,
                        region: self.region,
                        observed_lag_ms: 0,
                        failure_reason: "row tenant isolation violation".to_string(),
                        sev: "sev-2",
                    })
                    .map_err(NeonShadowError::AuditEmitFailed)?;
                return Err(NeonShadowError::TenantIsolationViolation {
                    sink_tenant: self.tenant_id.to_string(),
                    sink_tenant_redacted: redact_tenant_uuid(&self.tenant_id),
                    observed_tenant: row.tenant_id.to_string(),
                    observed_tenant_redacted: redact_tenant_uuid(&row.tenant_id),
                });
            }
            if row.region != self.region {
                self.audit_sink
                    .emit(ShadowSyncAuditRow {
                        event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                        tenant_id: self.tenant_id,
                        first_seq: receipt.first_sequence_number,
                        last_seq: receipt.last_sequence_number,
                        region: self.region,
                        observed_lag_ms: 0,
                        failure_reason: "row residency violation".to_string(),
                        sev: "sev-2",
                    })
                    .map_err(NeonShadowError::AuditEmitFailed)?;
                return Err(NeonShadowError::ResidencyViolation {
                    sink_region: self.region.as_str(),
                    observed_region: row.region.as_str(),
                });
            }
        }
        if rows.is_empty() {
            return Err(NeonShadowError::Internal("empty rows slice".to_string()));
        }

        // 2. Compute observed lag from the first event's `event_time_ms`
        //    to the caller-supplied `now_ms`. The lag is informational
        //    on the failure paths; load-bearing on the success path
        //    (drives the SEV-2 alert).
        let first_event_time_ms = rows.first().map(|r| r.event_time_ms).unwrap_or(0);
        let observed_lag_ms = now_ms.saturating_sub(first_event_time_ms);

        // 3. Injected failure check (after pre-validation, before
        //    persisting — mirrors a Neon driver transport failure).
        {
            let inj = self
                .injected_failure
                .lock()
                .map_err(|_| NeonShadowError::Internal("inject mutex poisoned".to_string()))?
                .clone();
            if let Some(msg) = inj {
                self.audit_sink
                    .emit(ShadowSyncAuditRow {
                        event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                        tenant_id: self.tenant_id,
                        first_seq: receipt.first_sequence_number,
                        last_seq: receipt.last_sequence_number,
                        region: self.region,
                        observed_lag_ms,
                        failure_reason: msg.clone(),
                        sev: "sev-2",
                    })
                    .map_err(NeonShadowError::AuditEmitFailed)?;
                return Err(NeonShadowError::Backend(msg));
            }
        }

        // 4. Persist the rows. INSERT-only (INV-AUDIT-APPEND-ONLY).
        //    The PRIMARY KEY (tenant_id, seq) deduplicates retries.
        {
            let mut g = self
                .rows
                .lock()
                .map_err(|_| NeonShadowError::Internal("rows mutex poisoned".to_string()))?;
            for row in rows {
                // Idempotent upsert: drop if (tenant, seq) already
                // present (mirrors `INSERT ... ON CONFLICT DO NOTHING`
                // on the production driver).
                let already_present = g
                    .iter()
                    .any(|r| r.tenant_id == row.tenant_id && r.seq == row.seq);
                if !already_present {
                    g.push(row.clone());
                }
            }
        }

        // 5. Audit-emit (success).
        let sev = if observed_lag_ms >= SHADOW_LAG_SEV2_THRESHOLD_MS {
            "sev-2"
        } else {
            "info"
        };
        self.audit_sink
            .emit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNCED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: String::new(),
                sev,
            })
            .map_err(NeonShadowError::AuditEmitFailed)?;

        Ok(ShadowSyncReceipt {
            tenant_id: self.tenant_id,
            first_seq: receipt.first_sequence_number,
            last_seq: receipt.last_sequence_number,
            rows_persisted: rows.len() as u64,
            observed_lag_ms,
            region: self.region,
        })
    }

    fn aggregate_event_count(
        &self,
        from_ms: u64,
        to_ms: u64,
        event_type_filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
        let g = self
            .rows
            .lock()
            .map_err(|_| NeonShadowError::Internal("rows mutex poisoned".to_string()))?;
        // RLS-equivalent: only the bound tenant's rows are visible.
        let mut counts: std::collections::BTreeMap<String, u64> = Default::default();
        for row in g.iter() {
            if row.tenant_id != self.tenant_id {
                continue;
            }
            if row.event_time_ms < from_ms || row.event_time_ms >= to_ms {
                continue;
            }
            if let Some(filter) = event_type_filter {
                if row.event_type != filter {
                    continue;
                }
            }
            *counts.entry(row.event_type.clone()).or_insert(0) += 1;
        }
        Ok(counts
            .into_iter()
            .map(|(event_type, count)| EventCountBucket { event_type, count })
            .collect())
    }

    fn aggregate_timeline(
        &self,
        from_ms: u64,
        to_ms: u64,
        granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
        if granularity_ms == 0 {
            return Err(NeonShadowError::Internal(
                "granularity_ms must be > 0".to_string(),
            ));
        }
        let g = self
            .rows
            .lock()
            .map_err(|_| NeonShadowError::Internal("rows mutex poisoned".to_string()))?;
        let mut counts: std::collections::BTreeMap<u64, u64> = Default::default();
        for row in g.iter() {
            if row.tenant_id != self.tenant_id {
                continue;
            }
            if row.event_time_ms < from_ms || row.event_time_ms >= to_ms {
                continue;
            }
            let offset = row.event_time_ms.saturating_sub(from_ms);
            let bucket_index = offset / granularity_ms;
            let bucket_start = from_ms.saturating_add(bucket_index.saturating_mul(granularity_ms));
            *counts.entry(bucket_start).or_insert(0) += 1;
        }
        Ok(counts
            .into_iter()
            .map(|(bucket_start_ms, count)| TimelineBucket {
                bucket_start_ms,
                count,
            })
            .collect())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::assertions_on_constants,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn dummy_receipt(tenant: Uuid, first: u64, last: u64) -> ArchiveReceipt {
        ArchiveReceipt {
            r2_key: format!("audit/2026/05/15/{first:08}.ndjson"),
            tenant_id: tenant,
            first_event_time_ms: 1_000,
            last_event_time_ms: 2_000,
            first_sequence_number: first,
            last_sequence_number: last,
            prev_hash_anchor: ChainHash::genesis(),
            chain_head_after: ChainHash([0xAB; 32]),
            bytes_written: 100,
            events_written: last - first + 1,
        }
    }

    fn dummy_row(tenant: Uuid, region: Region, seq: u64, time_ms: u64, ty: &str) -> ShadowEventRow {
        ShadowEventRow {
            tenant_id: tenant,
            seq,
            event_time_ms: time_ms,
            event_type: ty.to_string(),
            prev_hash: ChainHash::genesis(),
            link_hash: ChainHash([(seq as u8); 32]),
            region,
            payload_json: format!("{{\"i\":{seq}}}"),
        }
    }

    #[test]
    fn lag_constants_pin_to_canonical_slo() {
        // Wave-18 spec: nominal 5 min, SEV-2 60 min.
        assert_eq!(SHADOW_LAG_NOMINAL_MAX_MS, 5 * 60 * 1_000);
        assert_eq!(SHADOW_LAG_SEV2_THRESHOLD_MS, 60 * 60 * 1_000);
        assert!(SHADOW_LAG_SEV2_THRESHOLD_MS > SHADOW_LAG_NOMINAL_MAX_MS);
    }

    #[test]
    fn audit_event_type_constants_match_spec() {
        assert_eq!(
            EVENT_TYPE_SHADOW_SYNCED,
            "corelink.audit.neon_shadow_synced.v1"
        );
        assert_eq!(
            EVENT_TYPE_SHADOW_SYNC_FAILED,
            "corelink.audit.neon_shadow_sync_failed.v1"
        );
    }

    #[test]
    fn shadow_sync_receipt_lag_classifiers() {
        let tenant = Uuid::now_v7();
        let r = ShadowSyncReceipt {
            tenant_id: tenant,
            first_seq: 0,
            last_seq: 5,
            rows_persisted: 6,
            observed_lag_ms: 60_000, // 1 min
            region: Region::Iad,
        };
        assert!(r.within_nominal_lag());
        assert!(!r.breaches_sev2_threshold());

        let r_breach = ShadowSyncReceipt {
            observed_lag_ms: SHADOW_LAG_SEV2_THRESHOLD_MS,
            ..r.clone()
        };
        assert!(!r_breach.within_nominal_lag());
        assert!(r_breach.breaches_sev2_threshold());
    }

    #[test]
    fn sync_chunk_persists_rows_and_emits_success_audit() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
        let rows = vec![
            dummy_row(
                tenant,
                Region::Iad,
                0,
                1_000,
                "dev.hugr.corelink.cas.put.v1",
            ),
            dummy_row(
                tenant,
                Region::Iad,
                1,
                2_000,
                "dev.hugr.corelink.cas.get.v1",
            ),
        ];
        let receipt = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 1), &rows, 3_000)
            .expect("sync ok");
        assert_eq!(receipt.rows_persisted, 2);
        assert_eq!(receipt.tenant_id, tenant);
        assert_eq!(sink.row_count(), 2);
        // Lag = now_ms - first_event_time_ms = 3000 - 1000 = 2000ms
        assert_eq!(receipt.observed_lag_ms, 2_000);
        assert!(receipt.within_nominal_lag());

        let audit_snap = audit.snapshot().expect("snap");
        assert_eq!(audit_snap.len(), 1);
        assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNCED);
        assert_eq!(audit_snap[0].sev, "info");
    }

    #[test]
    fn sync_chunk_rejects_cross_tenant_fail_closed() {
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant_a, Region::Iad, audit.clone());
        let rows = vec![dummy_row(tenant_b, Region::Iad, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant_b, 0, 0), &rows, 2_000)
            .expect_err("must reject cross-tenant");
        assert!(matches!(
            err,
            NeonShadowError::TenantIsolationViolation { .. }
        ));
        // No rows persisted.
        assert_eq!(sink.row_count(), 0);
        // Failure audit emitted.
        let audit_snap = audit.snapshot().expect("snap");
        assert_eq!(audit_snap.len(), 1);
        assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNC_FAILED);
        assert_eq!(audit_snap[0].sev, "sev-2");
    }

    #[test]
    fn sync_chunk_rejects_cross_region_fail_closed() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
        let rows = vec![dummy_row(tenant, Region::Fra, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
            .expect_err("must reject cross-region");
        assert!(matches!(err, NeonShadowError::ResidencyViolation { .. }));
    }

    #[test]
    fn sync_chunk_injected_failure_emits_sev2() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
        sink.inject_failure(Some("neon connection lost".into()))
            .unwrap();
        let rows = vec![dummy_row(tenant, Region::Iad, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
            .expect_err("injected failure must propagate");
        assert!(matches!(err, NeonShadowError::Backend(_)));
        assert_eq!(sink.row_count(), 0);
        let audit_snap = audit.snapshot().unwrap();
        assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNC_FAILED);
    }

    #[test]
    fn aggregate_event_count_basic_window() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let rows = vec![
            dummy_row(tenant, Region::Iad, 0, 1_000, "cas.put"),
            dummy_row(tenant, Region::Iad, 1, 2_000, "cas.put"),
            dummy_row(tenant, Region::Iad, 2, 3_000, "cas.get"),
        ];
        sink.sync_chunk(&dummy_receipt(tenant, 0, 2), &rows, 4_000)
            .unwrap();
        let buckets = sink.aggregate_event_count(0, 10_000, None).unwrap();
        let put = buckets.iter().find(|b| b.event_type == "cas.put").unwrap();
        let get = buckets.iter().find(|b| b.event_type == "cas.get").unwrap();
        assert_eq!(put.count, 2);
        assert_eq!(get.count, 1);
    }

    #[test]
    fn aggregate_timeline_buckets_correctly() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let rows = vec![
            dummy_row(tenant, Region::Iad, 0, 1_000, "x"),
            dummy_row(tenant, Region::Iad, 1, 1_500, "x"),
            dummy_row(tenant, Region::Iad, 2, 5_000, "x"),
        ];
        sink.sync_chunk(&dummy_receipt(tenant, 0, 2), &rows, 6_000)
            .unwrap();
        // 2s granularity → bucket [0..2000) has 2 rows, [4000..6000) has 1.
        let timeline = sink.aggregate_timeline(0, 6_000, 2_000).unwrap();
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].bucket_start_ms, 0);
        assert_eq!(timeline[0].count, 2);
        assert_eq!(timeline[1].bucket_start_ms, 4_000);
        assert_eq!(timeline[1].count, 1);
    }

    #[test]
    fn aggregate_zero_granularity_returns_internal_error() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let err = sink
            .aggregate_timeline(0, 1, 0)
            .expect_err("zero granularity");
        assert!(matches!(err, NeonShadowError::Internal(_)));
    }

    #[test]
    fn shadow_event_row_from_persisted_line_extracts_fields() {
        let tenant = Uuid::now_v7();
        let prev_hash_hex = "00".repeat(32);
        let ndjson = format!(
            "{{\"type\":\"dev.hugr.corelink.cas.put.v1\",\"time_ms\":1700,\"prev_hash\":\"{prev_hash_hex}\"}}"
        );
        let line = PersistedAuditLine {
            r2_key: "k".into(),
            ndjson,
            tenant_id: tenant,
            sequence_number: 7,
            link_hash: ChainHash([0x11; 32]),
        };
        let row = ShadowEventRow::from_persisted_line(&line, Region::Iad)
            .expect("well-formed NDJSON parses");
        assert_eq!(row.tenant_id, tenant);
        assert_eq!(row.seq, 7);
        assert_eq!(row.event_time_ms, 1_700);
        assert_eq!(row.event_type, "dev.hugr.corelink.cas.put.v1");
        assert_eq!(row.region, Region::Iad);
        assert_eq!(row.link_hash, ChainHash([0x11; 32]));
    }

    #[test]
    fn from_persisted_line_rejects_malformed_input() {
        // Wave-21 (B-P1-05 closure) — every malformed NDJSON shape
        // surfaces a typed `ParseError`, never silently admits a row
        // with `event_time_ms = 0` / `event_type = ""`.
        let tenant = Uuid::now_v7();
        let mk = |ndjson: &str| PersistedAuditLine {
            r2_key: "k".into(),
            ndjson: ndjson.to_string(),
            tenant_id: tenant,
            sequence_number: 0,
            link_hash: ChainHash::genesis(),
        };

        // 1. Garbage (not valid JSON at all).
        let garbage = ShadowEventRow::from_persisted_line(&mk("this is not json"), Region::Iad)
            .expect_err("non-JSON must reject");
        assert!(matches!(garbage, ParseError::InvalidJson(_)));

        // 2. JSON missing `time_ms`.
        let no_time = ShadowEventRow::from_persisted_line(
            &mk("{\"type\":\"x\",\"prev_hash\":\"00000000000000000000000000000000000000000000000000000000000000ab\"}"),
            Region::Iad,
        )
        .expect_err("missing time_ms must reject");
        assert!(matches!(no_time, ParseError::MissingField("time_ms")));

        // 3. JSON missing `type`.
        let no_type = ShadowEventRow::from_persisted_line(
            &mk("{\"time_ms\":1,\"prev_hash\":\"00000000000000000000000000000000000000000000000000000000000000ab\"}"),
            Region::Iad,
        )
        .expect_err("missing type must reject");
        assert!(matches!(no_type, ParseError::MissingField("type")));

        // 4. JSON missing `prev_hash`.
        let no_prev =
            ShadowEventRow::from_persisted_line(&mk("{\"time_ms\":1,\"type\":\"x\"}"), Region::Iad)
                .expect_err("missing prev_hash must reject");
        assert!(matches!(no_prev, ParseError::MissingField("prev_hash")));

        // 5. Invalid hex in prev_hash.
        let bad_hex = ShadowEventRow::from_persisted_line(
            &mk("{\"time_ms\":1,\"type\":\"x\",\"prev_hash\":\"NOT_HEX\"}"),
            Region::Iad,
        )
        .expect_err("non-hex prev_hash must reject");
        assert!(matches!(bad_hex, ParseError::InvalidPrevHash(_)));
    }

    /// Audit sink that fails every emit — exercises the wave-21
    /// `AuditEmitFailed` lift for the in-memory shadow sink.
    #[derive(Debug, Default)]
    struct AlwaysFailAuditSink;

    impl ShadowSyncAuditSink for AlwaysFailAuditSink {
        fn emit(&self, _row: ShadowSyncAuditRow) -> Result<(), &'static str> {
            Err("synthetic audit-emit failure")
        }
    }

    #[test]
    fn in_memory_sink_propagates_audit_emit_failure_on_success_path() {
        // Wave-21 (B-P1-02 closure) — happy-path audit emit failure no
        // longer silently swallowed; surfaces `AuditEmitFailed`.
        let tenant = Uuid::now_v7();
        let audit: Arc<dyn ShadowSyncAuditSink> = Arc::new(AlwaysFailAuditSink);
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let rows = vec![dummy_row(tenant, Region::Iad, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
            .expect_err("audit emit failure must propagate");
        assert!(matches!(err, NeonShadowError::AuditEmitFailed(_)));
    }

    #[test]
    fn aggregate_event_count_filter_narrows_to_one_type() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let rows = vec![
            dummy_row(tenant, Region::Iad, 0, 100, "a"),
            dummy_row(tenant, Region::Iad, 1, 200, "b"),
        ];
        sink.sync_chunk(&dummy_receipt(tenant, 0, 1), &rows, 300)
            .unwrap();
        let only_a = sink.aggregate_event_count(0, 1_000, Some("a")).unwrap();
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].event_type, "a");
        assert_eq!(only_a[0].count, 1);
    }

    #[test]
    fn sync_chunk_emits_sev2_when_lag_breaches_threshold() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit.clone());
        let first_event_time = 1_000u64;
        // now_ms = first_event_time + 60 min (3_600_000 ms)
        let now_ms = first_event_time + SHADOW_LAG_SEV2_THRESHOLD_MS;
        let rows = vec![dummy_row(tenant, Region::Iad, 0, first_event_time, "x")];
        let receipt = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, now_ms)
            .unwrap();
        assert!(receipt.breaches_sev2_threshold());
        let audit_snap = audit.snapshot().unwrap();
        assert_eq!(audit_snap[0].event_type, EVENT_TYPE_SHADOW_SYNCED);
        assert_eq!(audit_snap[0].sev, "sev-2");
    }

    #[test]
    fn sync_chunk_is_idempotent_on_duplicate_seq() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let rows = vec![dummy_row(tenant, Region::Iad, 0, 100, "x")];
        sink.sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 200)
            .unwrap();
        sink.sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 250)
            .unwrap();
        // PRIMARY KEY (tenant_id, seq) dedup: still 1 row.
        assert_eq!(sink.row_count(), 1);
    }

    #[test]
    fn empty_rows_slice_rejected_internal() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = InMemoryNeonShadowSink::new(tenant, Region::Iad, audit);
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &[], 100)
            .expect_err("empty rows");
        assert!(matches!(err, NeonShadowError::Internal(_)));
    }

    #[test]
    fn audit_sink_trait_is_object_safe() {
        let sinks: Vec<Arc<dyn ShadowSyncAuditSink>> =
            vec![Arc::new(InMemoryShadowSyncAuditSink::new())];
        assert_eq!(sinks.len(), 1);
    }

    #[test]
    fn neon_shadow_sink_trait_is_object_safe() {
        let tenant = Uuid::now_v7();
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sinks: Vec<Arc<dyn NeonShadowSink>> = vec![Arc::new(InMemoryNeonShadowSink::new(
            tenant,
            Region::Iad,
            audit,
        ))];
        assert_eq!(sinks.len(), 1);
    }
}
