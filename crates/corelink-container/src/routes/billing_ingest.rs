//! `POST /internal/v1/billing/usage` — runner billing usage-push INGEST
//! endpoint (ASK-2).
//!
//! # Why this exists
//!
//! The **corelink-runners fabric** charges a tenant on the per-tenant
//! CONCURRENCY axis (the `runners_entitlement.max_concurrency` cap — the
//! owner-ratified "concurrency priced, minutes unlimited" model), so the
//! wall-clock SLOT-SECONDS a runner lease burns are NOT a Stripe-billable
//! meter. They are still load-bearing for the dashboard / capacity
//! reconciliation surface, so the fabric pushes a per-lease usage record
//! here on every finished lease. This endpoint is the durable INGEST seam:
//! it stages the raw records into the canonical
//! [`usage_event_staging`](https://) D1 table that the
//! [`corelink-billing-aggregator`] cron drains + rolls up.
//!
//! This endpoint does the **raw, idempotent persist ONLY**. It does NOT
//! compute an `AggregatedCounter`, advance the hash chain, or touch any
//! Stripe surface — the aggregator owns the rollup + the BLAKE3 chain
//! (WI-S10-002). The split mirrors the WI-S10-001 emit surface: emit/stage
//! here, aggregate downstream.
//!
//! # Security model
//!
//! - This route is **NOT** reachable from the public internet. It is
//!   mounted on the container's internal HTTP listener (port 50051),
//!   reached only via the Cloudflare Durable Object / fabric forwarder
//!   (worker route `fabric` arm in `worker/src/index.ts`).
//! - **Caller auth (fail-CLOSED):** every request must carry an
//!   `X-Corelink-Internal-Auth` header whose value matches a **DEDICATED**
//!   secret, `BILLING_INGEST_AUTH_KEY` — NOT the Worker↔container
//!   `CORELINK_INTERNAL_AUTH_KEY` and NOT the `FABRIC_INTROSPECT_AUTH_KEY`.
//!   A separate secret keeps the blast radius tight: a leak of the ingest
//!   secret cannot mint PATs, introspect, nor erase. The compare reuses the
//!   exact constant-time, length-padded gate from
//!   [`crate::routes::internal_pat::internal_auth_ok`] — it is NOT
//!   reinvented here (mirrors [`crate::routes::auth_introspect`]).
//! - If `BILLING_INGEST_AUTH_KEY` is absent or shorter than 32 chars, the
//!   route is **NOT mounted** ([`build_state_from_env`] returns `None`,
//!   warn log) — the same fail-CLOSED posture as `auth_introspect`.
//!
//! # Request shape
//!
//! ```text
//! POST /internal/v1/billing/usage
//! X-Corelink-Internal-Auth: <BILLING_INGEST_AUTH_KEY>
//! Content-Type: application/json
//!
//! [
//!   {
//!     "tenant_id":     "<uuid>",
//!     "event_kind":    "runner_slot_seconds",
//!     "qty":           7200,
//!     "billing_period":"2026-06",
//!     "region":        "iad",
//!     "source":        "corelink/runner/iad",
//!     "time_ms":       1718000000000,
//!     "idem_key":      "<64-char-blake3-hex>"
//!   },
//!   ...
//! ]
//! ```
//!
//! # Response shapes
//!
//! - Batch accepted (202):
//!   ```text
//!   { "accepted": <n>, "deduped": <m>, "rejected": <r>, "total": <n+m> }
//!   ```
//!   `accepted` = first-sight rows inserted; `deduped` = rows whose
//!   `idem_key` (the `(tenant_id, request_id)` staging coordinate) already
//!   existed (idempotent no-op) — both are success. `rejected` = records
//!   DROPPED for failing per-record validation (including `qty` or `time_ms`
//!   outside the nonnegative signed-64-bit storage domain, bad
//!   `billing_period`, non-uuid `tenant_id`, unknown `event_kind`, or
//!   non-letter `region`/`idem_key`); they are never persisted and never
//!   retried (a malformed record cannot become valid). A non-zero `rejected`
//!   is an emitter/config defect to chase, not a retry signal — the runner can
//!   retry the whole batch freely, valid records dedup and bad ones re-reject
//!   identically.
//! - Missing / wrong service secret (401).
//! - Malformed JSON, empty batch, or an over-limit batch (400) — a BATCH-level
//!   fault, distinct from a single bad record (which is skipped, not a 400).
//! - A genuine D1 backend fault mid-persist (503) — fail-CLOSED; the runner
//!   retries the batch (idempotent by `idem_key`).
//!
//! # Hard rules
//!
//! - `idem_key` is the dedup coordinate; re-pushing the same record is a
//!   no-op (`deduped`), never a double-count.
//! - A per-record validation failure SKIPS that record (counted in `rejected`)
//!   and never fails the batch — one poison record must never block its
//!   batch-mates or trigger the runner's infinite retain-and-retry.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_billing_emit::{validate_billing_period, UsageEventKind};
use corelink_hash::Digest;

use crate::routes::internal_pat::internal_auth_ok;
use crate::storage::d1_http::D1HttpClient;

/// Canonical CloudEvents 1.0 `type` of every staged usage event. Pinned by
/// the `usage_event_staging.event_type` CHECK constraint (migration 0017);
/// the ingest path stages records under the SAME canonical type so the
/// downstream drain Worker + aggregator see a uniform `event_type` (the
/// per-kind dispatch lives in the `event_kind` field, not the CE type).
const USAGE_EVENT_TYPE: &str = corelink_billing_emit::USAGE_EVENT_TYPE;

/// Maximum number of records accepted in a single push batch. A runner
/// dispatcher pushes per-lease records; this bounds the per-request work
/// (and the JSON parse heap) so a single caller cannot stage an unbounded
/// batch in one request. A larger backlog is pushed across multiple
/// requests (each idempotent by `idem_key`).
const MAX_BATCH_RECORDS: usize = 1024;

/// Minimum length (chars) of the dedicated ingest secret.
const MIN_INGEST_AUTH_KEY_LEN: usize = 32;

/// Maximum value accepted by the end-to-end staging contract for nonnegative
/// integer fields. JSON and the DTO use `u64`, but D1 binds these columns as
/// signed integers. Keeping this as an explicit validation bound prevents an
/// out-of-domain value from becoming a backend-looking retryable failure.
const MAX_PERSISTED_NONNEGATIVE: u64 = i64::MAX as u64;

// ──────────────────────────────────────────────────────────────────────────────
// Persistence seam (trait + D1 impl + fake) — testable without a network
// ──────────────────────────────────────────────────────────────────────────────

/// A single raw usage record staged into `usage_event_staging`, already
/// validated (canonical `event_kind`, `YYYY-MM` `billing_period`, uuid
/// `tenant_id`, 3-char `region`, 64-hex `idem_key`). The persistence layer
/// maps it to the staging row coordinate `(tenant_id, request_id)` with
/// `request_id == idem_key` (the dedup coordinate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedUsageRecord {
    /// Owning tenant (canonical UUIDv7 hyphenated lowercase).
    pub tenant_id: Uuid,
    /// Canonical usage event kind (e.g. [`UsageEventKind::RunnerSlotSeconds`]).
    pub event_kind: UsageEventKind,
    /// Billable quantity (slot-seconds for `runner_slot_seconds`). The wire
    /// type is `u64`, while the supported persisted domain is
    /// `0..=i64::MAX`; this is a domain bound, not a coercion or truncation.
    pub qty: u64,
    /// Canonical `YYYY-MM` UTC month bucket.
    pub billing_period: String,
    /// Canonical 3-char CF colocode (per-region R2 drain routing).
    pub region: String,
    /// Originating source URI (e.g. `corelink/runner/iad`). CloudEvents
    /// `source`; recorded as the staged `event_id` correlation root.
    pub source: String,
    /// Wall-clock instant of the lease close (Unix epoch ms). The supported
    /// persisted domain is `0..=i64::MAX`; the route does not claim that every
    /// value in this storage domain is a semantically current timestamp.
    pub time_ms: u64,
    /// Deterministic idempotency key (64-char BLAKE3 hex). The dedup
    /// coordinate — re-pushing the same `idem_key` is a no-op.
    pub idem_key: String,
}

/// Outcome of staging a single record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageOutcome {
    /// First sight at the `(tenant_id, idem_key)` coordinate → row inserted.
    Inserted,
    /// The coordinate already existed → idempotent no-op (deduped).
    Deduped,
    /// Different or unverifiable payload; downstream WP2 contract seam.
    Conflict(StageConflictReason),
}

/// Why a duplicate coordinate cannot be classified as an idempotent replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageConflictReason {
    /// A valid persisted fingerprint differs from the incoming fingerprint.
    PayloadMismatch,
    /// A legacy or corrupted persisted fingerprint cannot be trusted.
    ExistingFingerprintUnverifiable,
}

/// Persistence seam for the runner usage-push ingest. Production wiring
/// binds [`D1UsageStagingStore`] (the canonical `usage_event_staging` D1
/// table the aggregator drains); tests bind an in-memory fake so the dedup
/// + count logic is unit-testable without a network (mirrors the
/// trait-abstraction-defer idiom used across the billing crates).
#[async_trait]
pub trait UsageStagingStore: Send + Sync + core::fmt::Debug {
    /// Idempotently stage one record into the canonical usage store, keyed
    /// by `(tenant_id, idem_key)`. A first-sight coordinate inserts; only an
    /// exact valid fingerprint replay dedups. A divergent or unverifiable
    /// winner returns a durable [`StageOutcome::Conflict`].
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` only on a genuine backend fault (the route
    /// maps it to 503 — fail-CLOSED; the caller retries the batch).
    async fn stage(&self, record: &StagedUsageRecord) -> Result<StageOutcome, String>;
}

/// SQL: idempotent insert into the canonical `usage_event_staging` table
/// (migration 0017; `qty` / `event_kind` / `billing_period` added by 0095 so the
/// counter aggregator can drain the quantity directly). `request_id` carries the
/// record's `idem_key` (the `(tenant_id, request_id)` PRIMARY KEY is the
/// INV-BILLING-NO-DUP coordinate). `RETURNING` linearizes insertion; a losing
/// caller reads and fingerprint-checks the durable winner.
///
/// `qty` / `event_kind` / `billing_period` are persisted (not just hashed) so the
/// billable quantity is durably aggregatable server-side (WI-S10-007); the
/// `event_payload_hash` still binds all three, so a tampered stored `qty` is
/// detectable.
const STAGE_INSERT_SQL: &str = "INSERT INTO usage_event_staging \
     (tenant_id, region, request_id, event_type, event_payload_hash, event_id, emitted_at, \
      qty, event_kind, billing_period) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
     ON CONFLICT (tenant_id, request_id) DO NOTHING \
     RETURNING event_payload_hash";

const STAGE_WINNER_SQL: &str = "SELECT event_payload_hash FROM usage_event_staging \
     WHERE tenant_id = ?1 AND request_id = ?2 LIMIT 1";

const STAGE_CONFLICT_SQL: &str = "INSERT INTO usage_event_staging_conflicts \
     (tenant_id, request_id, observed_fingerprint, incoming_fingerprint, reason, \
      first_observed_at, last_observed_at, observation_count) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1) \
     ON CONFLICT (tenant_id, request_id, observed_fingerprint, incoming_fingerprint, reason) \
     DO UPDATE SET last_observed_at = excluded.last_observed_at, \
                   observation_count = usage_event_staging_conflicts.observation_count + 1";

/// Production [`UsageStagingStore`] backed by the canonical
/// `usage_event_staging` D1 table (migration 0017) over the D1 HTTP API.
#[derive(Debug)]
pub struct D1UsageStagingStore {
    d1: Arc<D1HttpClient>,
}

impl D1UsageStagingStore {
    /// Construct from a shared D1 HTTP client.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Compute the canonical 64-hex `event_payload_hash` for a record — the
    /// BLAKE3-256 of the record's stable byte image (the same canonical
    /// content hash the CAS write path uses, via [`Digest::compute`]). The
    /// staging CHECK requires exactly 64 hex chars; this satisfies it and
    /// gives a deterministic collision-detection surface (the same
    /// `(tenant_id, idem_key)` MUST reproduce the same hash).
    fn payload_hash(record: &StagedUsageRecord) -> String {
        // A stable, separator-delimited byte image of the load-bearing
        // fields. `\x1f` (ASCII Unit Separator) cannot appear in a uuid,
        // canonical kind/period/region/hex, or a decimal integer, so the
        // join is unambiguous (no field-boundary collision).
        let image = format!(
            "{tenant}\x1f{kind}\x1f{qty}\x1f{period}\x1f{region}\x1f{source}\x1f{time}\x1f{idem}",
            tenant = record.tenant_id,
            kind = record.event_kind.as_str(),
            qty = record.qty,
            period = record.billing_period,
            region = record.region,
            source = record.source,
            time = record.time_ms,
            idem = record.idem_key,
        );
        Digest::compute(image.as_bytes()).to_hex()
    }

    #[rustfmt::skip]
    fn valid_fingerprint(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }

    async fn record_conflict(
        &self,
        tenant: &str,
        record: &StagedUsageRecord,
        observed_fingerprint: String,
        incoming_fingerprint: &str,
        reason: StageConflictReason,
    ) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("system clock before Unix epoch: {e}"))?
            .as_millis();
        let now =
            i64::try_from(now).map_err(|_| "conflict timestamp out of i64 range".to_owned())?;
        let reason = match reason {
            StageConflictReason::PayloadMismatch => "payload_mismatch",
            StageConflictReason::ExistingFingerprintUnverifiable => {
                "existing_fingerprint_unverifiable"
            }
        };
        self.d1
            .query(
                STAGE_CONFLICT_SQL,
                &[
                    serde_json::Value::String(tenant.to_owned()),
                    serde_json::Value::String(record.idem_key.clone()),
                    serde_json::Value::String(observed_fingerprint),
                    serde_json::Value::String(incoming_fingerprint.to_owned()),
                    serde_json::Value::String(reason.to_owned()),
                    serde_json::Value::Number(now.into()),
                ],
            )
            .await
            .map(|_| ())
    }
}
#[async_trait]
impl UsageStagingStore for D1UsageStagingStore {
    async fn stage(&self, record: &StagedUsageRecord) -> Result<StageOutcome, String> {
        let tenant = record.tenant_id.to_string();
        let payload_hash = Self::payload_hash(record);
        let i64_time = i64::try_from(record.time_ms)
            .map_err(|_| format!("time_ms out of i64 range: {}", record.time_ms))?;
        let i64_qty = i64::try_from(record.qty)
            .map_err(|_| format!("qty out of i64 range: {}", record.qty))?;
        let inserted = self
            .d1
            .query(
                STAGE_INSERT_SQL,
                &[
                    serde_json::Value::String(tenant.clone()),
                    serde_json::Value::String(record.region.clone()),
                    serde_json::Value::String(record.idem_key.clone()),
                    serde_json::Value::String(USAGE_EVENT_TYPE.to_owned()),
                    serde_json::Value::String(payload_hash.clone()),
                    serde_json::Value::String(record.source.clone()),
                    serde_json::Value::Number(i64_time.into()),
                    serde_json::Value::Number(i64_qty.into()),
                    serde_json::Value::String(record.event_kind.as_str().to_owned()),
                    serde_json::Value::String(record.billing_period.clone()),
                ],
            )
            .await?;
        if !inserted.is_empty() {
            return Ok(StageOutcome::Inserted);
        }

        let winner = self
            .d1
            .query(
                STAGE_WINNER_SQL,
                &[
                    serde_json::Value::String(tenant.clone()),
                    serde_json::Value::String(record.idem_key.clone()),
                ],
            )
            .await?;
        let observed = winner
            .first()
            .and_then(|row| row.get("event_payload_hash"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                "insert lost but durable winner could not be read or verified".to_owned()
            })?
            .to_owned();
        let reason = if !Self::valid_fingerprint(&observed) {
            StageConflictReason::ExistingFingerprintUnverifiable
        } else if observed == payload_hash {
            return Ok(StageOutcome::Deduped);
        } else {
            StageConflictReason::PayloadMismatch
        };
        self.record_conflict(&tenant, record, observed, &payload_hash, reason)
            .await
            .map_err(|e| format!("required staging conflict record failed: {e}"))?;
        Ok(StageOutcome::Conflict(reason))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// State
// ──────────────────────────────────────────────────────────────────────────────

/// Route state injected at boot time.
#[derive(Clone)]
pub struct BillingIngestRouteState {
    /// The dedicated `BILLING_INGEST_AUTH_KEY` shared secret for the
    /// `X-Corelink-Internal-Auth` gate — independently rotatable so an
    /// ingest-secret leak shares no blast radius with the mint / introspect
    /// / erase consumers.
    internal_auth_key: Arc<str>,
    /// The canonical usage store the aggregator drains.
    store: Arc<dyn UsageStagingStore>,
}

impl std::fmt::Debug for BillingIngestRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingIngestRouteState")
            // Never the secret value.
            .field("internal_auth_key", &"[REDACTED]")
            .field("store", &self.store)
            .finish()
    }
}

impl BillingIngestRouteState {
    /// Construct from explicit collaborators (production wiring + tests).
    #[must_use]
    pub fn new(internal_auth_key: Arc<str>, store: Arc<dyn UsageStagingStore>) -> Self {
        Self {
            internal_auth_key,
            store,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Request / response shapes
// ──────────────────────────────────────────────────────────────────────────────

/// A single inbound usage record (wire shape). `event_kind` deserializes
/// via the canonical [`UsageEventKind`] serde (so an unknown kind is a 400),
/// and the remaining fields are validated in [`validate_record`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageRecordWire {
    /// Owning tenant UUID (validated as a uuid).
    tenant_id: String,
    /// Canonical usage event kind string.
    event_kind: UsageEventKind,
    /// Billable quantity.
    qty: u64,
    /// Canonical `YYYY-MM` billing period (validated via
    /// [`validate_billing_period`]).
    billing_period: String,
    /// Canonical 3-char CF colocode (validated to the staging-table shape).
    region: String,
    /// Originating source URI.
    source: String,
    /// Wall-clock instant of the lease close (Unix epoch ms).
    time_ms: u64,
    /// Deterministic 64-char BLAKE3-hex idempotency key (the dedup
    /// coordinate).
    idem_key: String,
}

/// JSON response body: the ordered outcome of every input record.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    /// One result for each array element, in request order.
    pub outcomes: Vec<IngestRecordOutcome>,
    /// First-sight records inserted into the staging store.
    pub accepted: u32,
    /// Records whose `idem_key` already existed (idempotent no-ops).
    pub deduped: u32,
    /// Records dropped for failing per-record validation (never persisted). A
    /// non-zero value is an emitter/config defect to investigate, NOT a retry
    /// signal — the batch is still drained (2xx) so a poison record cannot flood.
    #[serde(default)]
    pub rejected: u32,
    /// Total records successfully staged (`accepted + deduped`; excludes
    /// `rejected`).
    pub total: u32,
}

/// The durable or validation result for one submitted record.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestRecordOutcome {
    /// Zero-based position in the request array.
    pub index: usize,
    /// The canonical idempotency key, when the input supplied a valid key.
    pub idem_key: Option<String>,
    /// Classification of this input.
    pub outcome: IngestOutcomeKind,
    /// Stable diagnostic for rejected and conflict outcomes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Wire classification for a submitted usage record.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestOutcomeKind {
    /// The submitted record was newly staged.
    Accepted,
    /// The submitted record exactly matched a durable prior record.
    Deduped,
    /// The submitted record failed wire or record validation.
    Rejected,
    /// The submitted record disagreed with its durable identity winner.
    Conflict,
}

/// Validation failure for a single record (mapped to a stable, non-leaking
/// per-record reason code in the 202/422 response).
#[derive(Debug, PartialEq, Eq)]
enum RecordError {
    /// `tenant_id` is not a canonical UUID.
    BadTenantId,
    /// `billing_period` is not the canonical `YYYY-MM` shape.
    BadBillingPeriod,
    /// `qty` cannot be represented by the signed integer storage contract.
    QtyOutOfStorageRange,
    /// `region` is not 3 ASCII letters (checked AFTER lowercase-canonicalization,
    /// so a mixed/upper-case colo like `IAD` is accepted, not rejected).
    BadRegion,
    /// `idem_key` is not exactly 64 lowercase hex chars (BLAKE3-256 form).
    BadIdemKey,
    /// `source` is empty.
    EmptySource,
    /// `source` exceeds [`MAX_SOURCE_LEN`].
    SourceTooLong,
    /// `time_ms` cannot be represented by the signed integer storage contract.
    TimeMsOutOfStorageRange,
}

impl RecordError {
    /// Stable reason code (no caller data echoed — no injection / oracle).
    const fn code(&self) -> &'static str {
        match self {
            Self::BadTenantId => "bad_tenant_id",
            Self::BadBillingPeriod => "bad_billing_period",
            Self::QtyOutOfStorageRange => "qty_out_of_storage_range",
            Self::BadRegion => "bad_region",
            Self::BadIdemKey => "bad_idem_key",
            Self::EmptySource => "empty_source",
            Self::SourceTooLong => "source_too_long",
            Self::TimeMsOutOfStorageRange => "time_ms_out_of_storage_range",
        }
    }
}

impl StageConflictReason {
    /// Stable public conflict reason.
    const fn code(self) -> &'static str {
        match self {
            Self::PayloadMismatch => "payload_mismatch",
            Self::ExistingFingerprintUnverifiable => "existing_fingerprint_unverifiable",
        }
    }
}

/// Upper bound on the free-text `source` field. A `source` is a short,
/// machine-emitted provenance tag (e.g. `corelink/runner/iad`); 256 bytes is
/// generous for any legitimate value while keeping an abusive one a clean 400.
const MAX_SOURCE_LEN: usize = 256;

/// Validate one wire record into a [`StagedUsageRecord`]. Pure logic — no
/// I/O — so it is unit-testable in isolation (mirrors the `decode_*` split
/// in [`crate::routes::auth_introspect`]).
fn validate_record(wire: UsageRecordWire) -> Result<StagedUsageRecord, RecordError> {
    let tenant_id = Uuid::parse_str(&wire.tenant_id).map_err(|_| RecordError::BadTenantId)?;
    validate_billing_period(&wire.billing_period).map_err(|_| RecordError::BadBillingPeriod)?;
    // Keep the wire representation wide enough to decode the JSON integer,
    // then reject values outside the exact D1 signed-integer domain as a
    // permanent record error. This runs before the store, so a poison member
    // cannot become a 503 or prevent valid siblings from progressing.
    if wire.qty > MAX_PERSISTED_NONNEGATIVE {
        return Err(RecordError::QtyOutOfStorageRange);
    }
    if wire.time_ms > MAX_PERSISTED_NONNEGATIVE {
        return Err(RecordError::TimeMsOutOfStorageRange);
    }
    // Canonical CF colocode: 3 ASCII letters (matches the
    // `usage_event_staging` CHECK(length(region) = 3) + the
    // `corelink_analytics::Region::as_str()` shape).
    //
    // CANONICALIZE to lowercase BEFORE validating + storing — exactly as the
    // `idem_key` below. A colo is a case-INSENSITIVE identifier: a box
    // misconfigured with `BILLING_REGION="IAD"` emits the SAME region as `iad`.
    // Rejecting the upper-cased spelling (as this once did) made every such
    // record a `BadRegion` 400; combined with the runner's retain-and-retry on
    // non-2xx that turned ONE bad-cased record into an infinite re-POST flood
    // that also blocked its batch-mates from ingesting. Collapsing case here
    // accepts the legitimate colo; the per-record skip below contains any record
    // that is genuinely non-canonical. Fail-CLOSED on non-letters (a digit /
    // symbol is not `is_ascii_lowercase` even after lowercasing).
    let region = wire.region.to_ascii_lowercase();
    if region.len() != 3 || !region.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(RecordError::BadRegion);
    }
    // 64-char lowercase hex (BLAKE3-256 canonical form) — matches the
    // staging CHECK(length(event_payload_hash) = 64) discipline + gives a
    // well-formed `(tenant_id, request_id)` dedup coordinate.
    //
    // CANONICALIZE to lowercase BEFORE validating + storing: `is_ascii_hexdigit`
    // also accepts `A-F`, so the upper- and lower-case spellings of the SAME
    // BLAKE3 key would otherwise become two distinct `(tenant_id, idem_key)`
    // dedup coordinates → the idempotency dedup is silently bypassable
    // (double-processing). Collapsing case-variants to one canonical key closes
    // that hole.
    let idem_key = wire.idem_key.to_ascii_lowercase();
    if idem_key.len() != 64 || !idem_key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(RecordError::BadIdemKey);
    }
    if wire.source.is_empty() {
        return Err(RecordError::EmptySource);
    }
    // Explicit length cap on the free-text `source`: bound it to a sane value
    // so an abusive/malformed `source` is a CLEAN 400 reject, not something that
    // rides the 10 MiB / ~16 KB edge limits into the staging store.
    if wire.source.len() > MAX_SOURCE_LEN {
        return Err(RecordError::SourceTooLong);
    }
    Ok(StagedUsageRecord {
        tenant_id,
        event_kind: wire.event_kind,
        qty: wire.qty,
        billing_period: wire.billing_period,
        region,
        source: wire.source,
        time_ms: wire.time_ms,
        idem_key,
    })
}

/// Extract an idempotency key only when it is a canonicalizable BLAKE3 hex
/// coordinate. Rejections can therefore be correlated without echoing a
/// malformed caller value.
fn canonical_idem_key(input: &serde_json::Value) -> Option<String> {
    let idem_key = input.get("idem_key")?.as_str()?.to_ascii_lowercase();
    (idem_key.len() == 64 && idem_key.bytes().all(|b| b.is_ascii_hexdigit())).then_some(idem_key)
}

// ──────────────────────────────────────────────────────────────────────────────
// Route handler
// ──────────────────────────────────────────────────────────────────────────────

/// Build the ingest router. Mount at the top level so
/// `/internal/v1/billing/usage` is directly addressable.
pub fn router(state: BillingIngestRouteState) -> Router {
    Router::new()
        .route("/internal/v1/billing/usage", post(handle_ingest))
        .with_state(state)
}

/// `POST /internal/v1/billing/usage` handler.
///
/// The auth gate runs FIRST, on raw [`Bytes`] (the `HeaderMap` is a
/// `FromRequestParts` extractor, evaluated before the body is buffered): an
/// unauthenticated caller is rejected 401 WITHOUT the body ever being
/// JSON-parsed (denying parse CPU/heap to an attacker), exactly as in
/// [`crate::routes::auth_introspect`].
async fn handle_ingest(
    State(state): State<BillingIngestRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // ── 1. Dedicated-secret gate (constant-time; reused gate) ───────────────
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // ── 2. Parse the batch — ONLY after the auth gate passed ────────────────
    let inputs: Vec<serde_json::Value> = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "billing_ingest: invalid request body");
            return bad_request("invalid_body");
        }
    };

    if inputs.is_empty() {
        return bad_request("empty_batch");
    }
    if inputs.len() > MAX_BATCH_RECORDS {
        return bad_request("batch_too_large");
    }

    // ── 3. Validate each record; SKIP (never persist) the ones that fail ────
    // A record that fails validation is PERMANENTLY malformed — no retry will
    // ever make it valid. 400-ing the whole batch on a single poison record
    // (as this once did) made the runner RETAIN the batch and re-POST it every
    // tick forever (`flush_now` bails on any non-2xx), an infinite self-inflicted
    // flood in which the batch's VALID records never ingested either — silent
    // billing-data loss. Per-record skip drains the batch (2xx) while dropping
    // only the bad records, surfaced as `rejected` in the response body + a warn
    // per reason (a non-zero `rejected` is an emitter/config defect to chase, not
    // a retry signal). Batch-level faults (unparseable / empty / oversized) stay
    // 400 above; a genuine backend persist fault stays 503 below (fail-CLOSED,
    // idempotent retry).
    let mut staged = Vec::with_capacity(inputs.len());
    let mut outcomes = Vec::with_capacity(inputs.len());
    let mut rejected: u32 = 0;
    for (index, input) in inputs.into_iter().enumerate() {
        let idem_key = canonical_idem_key(&input);
        match serde_json::from_value::<UsageRecordWire>(input)
            .map_err(|_| "invalid_record")
            .and_then(|wire| validate_record(wire).map_err(|e| e.code()))
        {
            Ok(rec) => {
                staged.push((outcomes.len(), rec));
                outcomes.push(IngestRecordOutcome {
                    index,
                    idem_key,
                    outcome: IngestOutcomeKind::Accepted,
                    reason: None,
                });
            }
            Err(reason) => {
                rejected = rejected.saturating_add(1);
                tracing::warn!(
                    reason,
                    "billing_ingest: record validation failed; skipping (batch not rejected)"
                );
                outcomes.push(IngestRecordOutcome {
                    index,
                    idem_key,
                    outcome: IngestOutcomeKind::Rejected,
                    reason: Some(reason.to_owned()),
                });
            }
        }
    }

    // ── 4. Persist idempotently; tally accepted vs deduped ──────────────────
    let mut accepted: u32 = 0;
    let mut deduped: u32 = 0;
    let mut has_conflict = false;
    for (outcome_index, rec) in &staged {
        match state.store.stage(rec).await {
            Ok(StageOutcome::Inserted) => accepted = accepted.saturating_add(1),
            Ok(StageOutcome::Deduped) => {
                deduped = deduped.saturating_add(1);
                let Some(outcome) = outcomes.get_mut(*outcome_index) else {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                };
                outcome.outcome = IngestOutcomeKind::Deduped;
            }
            Ok(StageOutcome::Conflict(reason)) => {
                has_conflict = true;
                let Some(outcome) = outcomes.get_mut(*outcome_index) else {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                };
                outcome.outcome = IngestOutcomeKind::Conflict;
                outcome.reason = Some(reason.code().to_owned());
            }
            Err(e) => {
                // Fail-CLOSED: a backend fault → 503; the runner retries the
                // batch (idempotent by idem_key — no double-count on retry).
                tracing::error!(error = %e, "billing_ingest: staging persist failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    }

    let total = accepted.saturating_add(deduped);
    let status = if has_conflict {
        StatusCode::CONFLICT
    } else if total == 0 && rejected != 0 {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::ACCEPTED
    };
    (
        status,
        Json(IngestResponse {
            outcomes,
            accepted,
            deduped,
            rejected,
            total,
        }),
    )
        .into_response()
}

/// A 400 with a stable reason code (no caller data echoed).
fn bad_request(reason: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": reason })),
    )
        .into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// State builder
// ──────────────────────────────────────────────────────────────────────────────

/// Build the route state from env at binary boot.
///
/// - `BILLING_INGEST_AUTH_KEY` — DEDICATED shared secret for the auth-header
///   gate (NOT `CORELINK_INTERNAL_AUTH_KEY`, NOT `FABRIC_INTROSPECT_AUTH_KEY`).
///   Must be ≥ 32 chars. Absent / too short → returns `None` (route NOT
///   mounted; warn log) — fail-CLOSED.
/// - The canonical D1-backed [`UsageStagingStore`] is built from the same
///   `StorageEnv` as the cache adapters; absent → `None`.
///
/// Returns `None` when any required input is missing/invalid; the caller
/// logs a warning and skips mounting the route (dev/CI without secrets).
#[must_use]
pub fn build_state_from_env() -> Option<BillingIngestRouteState> {
    let auth_key = configured_ingest_auth_key(std::env::var("BILLING_INGEST_AUTH_KEY").ok())?;
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = D1HttpClient::new(&storage_env)
        .map_err(|e| {
            tracing::warn!(error = %e, "billing_ingest: D1 client init failed; route NOT mounted");
        })
        .ok()?;

    let store: Arc<dyn UsageStagingStore> = Arc::new(D1UsageStagingStore::new(Arc::new(d1)));
    Some(BillingIngestRouteState::new(
        Arc::from(auth_key.as_str()),
        store,
    ))
}

/// Accept only a configured dedicated ingest key with the minimum safe length.
///
/// Kept separate from process-environment lookup so the absence case is
/// testable without mutating or depending on ambient test-process state.
fn configured_ingest_auth_key(auth_key: Option<String>) -> Option<String> {
    let auth_key = auth_key?;
    if auth_key.len() < MIN_INGEST_AUTH_KEY_LEN {
        tracing::warn!(
            "BILLING_INGEST_AUTH_KEY absent or too short (< 32 chars); \
             /internal/v1/billing/usage route NOT mounted"
        );
        return None;
    }
    Some(auth_key)
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

// The tests live in the sibling `billing_ingest/` directory, ONE FILE PER
// PROPERTY — no `mod.rs` (this repo has none), following `origin_timing.rs`
// (#1470). Each file's `//!` names the single property it fixes:
//
//   tests_support         — the in-memory staging store + request/JSON helpers
//                           the others share, here so that no sibling looks
//                           load-bearing for its neighbours.
//   tests_auth            — the dedicated `BILLING_INGEST_AUTH_KEY` is the only
//                           key to this route: absent, the route is not mounted
//                           at all; unmatched, the request is 401.
//   tests_dedup           — a `(tenant_id, idem_key)` coordinate stages exactly
//                           once: across requests, within one batch, and in
//                           either hex case. The double-billing guard.
//   tests_batch_faults    — a fault that makes the BATCH un-interpretable
//                           rejects the whole request with 400 and stages
//                           nothing.
//   tests_record_skip     — one invalid RECORD is counted in `rejected` and
//                           skipped with 202: it never blocks its batch-mates
//                           and never leaves the runner re-POSTing forever.
//   tests_region_canon    — `region` is canonicalized by ASCII case-folding
//                           ONLY, and anything that is not three ASCII letters
//                           fails closed as `BadRegion`.
//   tests_backend_fault   — a staging-backend failure surfaces as 503, never as
//                           a 202 that claims a batch landed when nothing was
//                           written.
//   tests_validate_record — the acceptance boundary of `validate_record`: the
//                           canonical record passes through intact, and a field
//                           is refused the first byte past its bound.
//   tests_payload_hash    — the `event_payload_hash` binding the billable
//                           fields is deterministic 64-hex, so the staged-row
//                           tamper check actually detects tampering.

#[cfg(test)]
mod tests_support;

#[cfg(test)]
mod tests_auth;

#[cfg(test)]
mod tests_dedup;

#[cfg(test)]
mod tests_batch_faults;

#[cfg(test)]
mod tests_record_skip;

#[cfg(test)]
mod tests_region_canon;

#[cfg(test)]
mod tests_backend_fault;

#[cfg(test)]
mod tests_validate_record;

#[cfg(test)]
mod tests_payload_hash;

#[cfg(test)]
mod tests_durable_classification;

#[cfg(test)]
mod tests_http_outcomes;
