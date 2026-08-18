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
//!   DROPPED for failing per-record validation (bad `billing_period`, non-uuid
//!   `tenant_id`, unknown `event_kind`, non-letter `region`/`idem_key`); they
//!   are never persisted and never retried (a malformed record cannot become
//!   valid). A non-zero `rejected` is an emitter/config defect to chase, not a
//!   retry signal — the runner can retry the whole batch freely, valid records
//!   dedup and bad ones re-reject identically.
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
    /// Billable quantity (slot-seconds for `runner_slot_seconds`).
    pub qty: u64,
    /// Canonical `YYYY-MM` UTC month bucket.
    pub billing_period: String,
    /// Canonical 3-char CF colocode (per-region R2 drain routing).
    pub region: String,
    /// Originating source URI (e.g. `corelink/runner/iad`). CloudEvents
    /// `source`; recorded as the staged `event_id` correlation root.
    pub source: String,
    /// Wall-clock instant of the lease close (Unix epoch ms).
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
}

/// Persistence seam for the runner usage-push ingest. Production wiring
/// binds [`D1UsageStagingStore`] (the canonical `usage_event_staging` D1
/// table the aggregator drains); tests bind an in-memory fake so the dedup
/// + count logic is unit-testable without a network (mirrors the
/// trait-abstraction-defer idiom used across the billing crates).
#[async_trait]
pub trait UsageStagingStore: Send + Sync + core::fmt::Debug {
    /// Idempotently stage one record into the canonical usage store, keyed
    /// by `(tenant_id, idem_key)`. A first-sight coordinate inserts +
    /// returns [`StageOutcome::Inserted`]; an existing coordinate is a
    /// no-op + returns [`StageOutcome::Deduped`].
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
/// INV-BILLING-NO-DUP dedup coordinate). `ON CONFLICT DO NOTHING` makes a re-push
/// a pure no-op; the `changes()` count distinguishes a fresh insert (1) from a
/// dedup (0). `drained_to_r2_at` is left NULL — the drain Worker advances that
/// watermark when it commits the R2 PutObject.
///
/// `qty` / `event_kind` / `billing_period` are persisted (not just hashed) so the
/// billable quantity is durably aggregatable server-side (WI-S10-007); the
/// `event_payload_hash` still binds all three, so a tampered stored `qty` is
/// detectable.
const STAGE_INSERT_SQL: &str = "INSERT INTO usage_event_staging \
     (tenant_id, region, request_id, event_type, event_payload_hash, event_id, emitted_at, \
      qty, event_kind, billing_period) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
     ON CONFLICT (tenant_id, request_id) DO NOTHING";

/// SQL: read back whether the `(tenant_id, request_id)` coordinate now
/// exists. D1's HTTP API does not surface `changes()` in the row results,
/// so the insert + this existence probe together give a deterministic
/// inserted-vs-deduped signal: we probe BEFORE the insert to classify the
/// outcome (a pre-existing row ⇒ dedup; absent ⇒ insert). The insert is
/// still `ON CONFLICT DO NOTHING` so a concurrent racer can never
/// double-insert — the probe only classifies the count, never gates
/// correctness.
const STAGE_EXISTS_SQL: &str =
    "SELECT 1 AS present FROM usage_event_staging WHERE tenant_id = ?1 AND request_id = ?2 LIMIT 1";

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
}

#[async_trait]
impl UsageStagingStore for D1UsageStagingStore {
    async fn stage(&self, record: &StagedUsageRecord) -> Result<StageOutcome, String> {
        let tenant = record.tenant_id.to_string();

        // Classify the outcome by probing for the dedup coordinate FIRST.
        let existing = self
            .d1
            .query(
                STAGE_EXISTS_SQL,
                &[
                    serde_json::Value::String(tenant.clone()),
                    serde_json::Value::String(record.idem_key.clone()),
                ],
            )
            .await?;
        if !existing.is_empty() {
            return Ok(StageOutcome::Deduped);
        }

        // Insert idempotently. `ON CONFLICT DO NOTHING` keeps a concurrent
        // racer from double-inserting; if the racer won between our probe
        // and this insert, the conflict is silently absorbed and the row is
        // still present exactly once — we report `Inserted` here (the
        // racer reports `Deduped`), so the count is never inflated.
        let payload_hash = Self::payload_hash(record);
        let i64_time = i64::try_from(record.time_ms)
            .map_err(|_| format!("time_ms out of i64 range: {}", record.time_ms))?;
        let i64_qty = i64::try_from(record.qty)
            .map_err(|_| format!("qty out of i64 range: {}", record.qty))?;
        self.d1
            .query(
                STAGE_INSERT_SQL,
                &[
                    serde_json::Value::String(tenant),
                    serde_json::Value::String(record.region.clone()),
                    serde_json::Value::String(record.idem_key.clone()),
                    serde_json::Value::String(USAGE_EVENT_TYPE.to_owned()),
                    serde_json::Value::String(payload_hash),
                    serde_json::Value::String(record.source.clone()),
                    serde_json::Value::Number(i64_time.into()),
                    serde_json::Value::Number(i64_qty.into()),
                    serde_json::Value::String(record.event_kind.as_str().to_owned()),
                    serde_json::Value::String(record.billing_period.clone()),
                ],
            )
            .await?;
        Ok(StageOutcome::Inserted)
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

/// JSON response body: the per-batch accepted / deduped / rejected tally.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
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

/// Validation failure for a single record (mapped to a 400 with a stable,
/// non-leaking reason code).
#[derive(Debug, PartialEq, Eq)]
enum RecordError {
    /// `tenant_id` is not a canonical UUID.
    BadTenantId,
    /// `billing_period` is not the canonical `YYYY-MM` shape.
    BadBillingPeriod,
    /// `region` is not 3 ASCII letters (checked AFTER lowercase-canonicalization,
    /// so a mixed/upper-case colo like `IAD` is accepted, not rejected).
    BadRegion,
    /// `idem_key` is not exactly 64 lowercase hex chars (BLAKE3-256 form).
    BadIdemKey,
    /// `source` is empty.
    EmptySource,
    /// `source` exceeds [`MAX_SOURCE_LEN`].
    SourceTooLong,
}

impl RecordError {
    /// Stable reason code (no caller data echoed — no injection / oracle).
    const fn code(&self) -> &'static str {
        match self {
            Self::BadTenantId => "bad_tenant_id",
            Self::BadBillingPeriod => "bad_billing_period",
            Self::BadRegion => "bad_region",
            Self::BadIdemKey => "bad_idem_key",
            Self::EmptySource => "empty_source",
            Self::SourceTooLong => "source_too_long",
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
    let wire_records: Vec<UsageRecordWire> = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "billing_ingest: invalid request body");
            return bad_request("invalid_body");
        }
    };

    if wire_records.is_empty() {
        return bad_request("empty_batch");
    }
    if wire_records.len() > MAX_BATCH_RECORDS {
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
    let mut staged: Vec<StagedUsageRecord> = Vec::with_capacity(wire_records.len());
    let mut rejected: u32 = 0;
    for wire in wire_records {
        match validate_record(wire) {
            Ok(rec) => staged.push(rec),
            Err(e) => {
                rejected = rejected.saturating_add(1);
                tracing::warn!(
                    reason = e.code(),
                    "billing_ingest: record validation failed; skipping (batch not rejected)"
                );
            }
        }
    }

    // ── 4. Persist idempotently; tally accepted vs deduped ──────────────────
    let mut accepted: u32 = 0;
    let mut deduped: u32 = 0;
    for rec in &staged {
        match state.store.stage(rec).await {
            Ok(StageOutcome::Inserted) => accepted = accepted.saturating_add(1),
            Ok(StageOutcome::Deduped) => deduped = deduped.saturating_add(1),
            Err(e) => {
                // Fail-CLOSED: a backend fault → 503; the runner retries the
                // batch (idempotent by idem_key — no double-count on retry).
                tracing::error!(error = %e, "billing_ingest: staging persist failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    }

    let total = accepted.saturating_add(deduped);
    (
        StatusCode::ACCEPTED,
        Json(IngestResponse {
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
    let auth_key = std::env::var("BILLING_INGEST_AUTH_KEY").ok()?;
    if auth_key.len() < MIN_INGEST_AUTH_KEY_LEN {
        tracing::warn!(
            "BILLING_INGEST_AUTH_KEY absent or too short (< 32 chars); \
             /internal/v1/billing/usage route NOT mounted"
        );
        return None;
    }

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

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    use super::*;

    const TEST_AUTH_KEY: &str = "billing-ingest-test-key-32-chars!!!!";

    /// In-memory staging store: dedups by `(tenant_id, idem_key)` exactly
    /// like the D1 PRIMARY KEY. `backend_err` forces a 503 path.
    #[derive(Debug, Default)]
    struct FakeStore {
        seen: Mutex<HashSet<(Uuid, String)>>,
        backend_err: Option<String>,
    }

    impl FakeStore {
        fn new() -> Self {
            Self::default()
        }
        fn failing(err: &str) -> Self {
            Self {
                seen: Mutex::new(HashSet::new()),
                backend_err: Some(err.to_owned()),
            }
        }
    }

    #[async_trait]
    impl UsageStagingStore for FakeStore {
        async fn stage(&self, record: &StagedUsageRecord) -> Result<StageOutcome, String> {
            if let Some(e) = &self.backend_err {
                return Err(e.clone());
            }
            let mut g = self.seen.lock().unwrap();
            let key = (record.tenant_id, record.idem_key.clone());
            if g.insert(key) {
                Ok(StageOutcome::Inserted)
            } else {
                Ok(StageOutcome::Deduped)
            }
        }
    }

    fn state_with(store: Arc<dyn UsageStagingStore>) -> BillingIngestRouteState {
        BillingIngestRouteState::new(Arc::from(TEST_AUTH_KEY), store)
    }

    fn record_json(tenant: &str, idem: &str) -> serde_json::Value {
        serde_json::json!({
            "tenant_id": tenant,
            "event_kind": "runner_slot_seconds",
            "qty": 7200u64,
            "billing_period": "2026-06",
            "region": "iad",
            "source": "corelink/runner/iad",
            "time_ms": 1_718_000_000_000u64,
            "idem_key": idem,
        })
    }

    fn tenant_a() -> String {
        "11111111-1111-4111-8111-111111111111".to_owned()
    }

    fn hex64(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn ingest_request(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(http::Method::POST)
            .uri("/internal/v1/billing/usage")
            .header("content-type", "application/json");
        if let Some(a) = auth {
            b = b.header("x-corelink-internal-auth", a);
        }
        b.body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 16_384)
            .await
            .unwrap();
        if bytes.is_empty() {
            return serde_json::Value::Null;
        }
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── Caller auth ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn missing_service_secret_returns_401() {
        let app = router(state_with(Arc::new(FakeStore::new())));
        let req = ingest_request(
            None,
            serde_json::json!([record_json(&tenant_a(), &hex64(0xAB))]),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_service_secret_returns_401() {
        let app = router(state_with(Arc::new(FakeStore::new())));
        let req = ingest_request(
            Some("wrong-secret-which-is-also-32-chars!!"),
            serde_json::json!([record_json(&tenant_a(), &hex64(0xAB))]),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── Happy path + idempotent dedup ────────────────────────────────────────

    #[tokio::test]
    async fn fresh_batch_all_accepted() {
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(store));
        let body = serde_json::json!([
            record_json(&tenant_a(), &hex64(0x01)),
            record_json(&tenant_a(), &hex64(0x02)),
        ]);
        let resp = app
            .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let v = body_json(resp).await;
        assert_eq!(v["accepted"], serde_json::json!(2));
        assert_eq!(v["deduped"], serde_json::json!(0));
        assert_eq!(v["total"], serde_json::json!(2));
    }

    #[tokio::test]
    async fn repush_same_idem_key_is_deduped() {
        // Push a record, then re-push the SAME (tenant, idem_key): the second
        // push must be a no-op (deduped), never a double-count. The store is
        // shared across both requests (clone shares the Arc).
        let store: Arc<dyn UsageStagingStore> = Arc::new(FakeStore::new());
        let idem = hex64(0x07);

        let app1 = router(state_with(Arc::clone(&store)));
        let resp1 = app1
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([record_json(&tenant_a(), &idem)]),
            ))
            .await
            .unwrap();
        assert_eq!(resp1.status(), StatusCode::ACCEPTED);
        assert_eq!(body_json(resp1).await["accepted"], serde_json::json!(1));

        let app2 = router(state_with(Arc::clone(&store)));
        let resp2 = app2
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([record_json(&tenant_a(), &idem)]),
            ))
            .await
            .unwrap();
        assert_eq!(resp2.status(), StatusCode::ACCEPTED);
        let v = body_json(resp2).await;
        assert_eq!(
            v["accepted"],
            serde_json::json!(0),
            "re-push must not insert again"
        );
        assert_eq!(v["deduped"], serde_json::json!(1), "re-push must dedup");
    }

    #[tokio::test]
    async fn intra_batch_duplicate_idem_key_deduped() {
        // The SAME idem_key twice WITHIN one batch: first inserts, second
        // dedups — the tally is accepted=1, deduped=1.
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(store));
        let idem = hex64(0x09);
        let body = serde_json::json!([
            record_json(&tenant_a(), &idem),
            record_json(&tenant_a(), &idem),
        ]);
        let resp = app
            .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let v = body_json(resp).await;
        assert_eq!(v["accepted"], serde_json::json!(1));
        assert_eq!(v["deduped"], serde_json::json!(1));
    }

    // ── Bad body / validation → 400 ──────────────────────────────────────────

    #[tokio::test]
    async fn empty_batch_is_400() {
        let app = router(state_with(Arc::new(FakeStore::new())));
        let resp = app
            .oneshot(ingest_request(Some(TEST_AUTH_KEY), serde_json::json!([])))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn malformed_json_is_400() {
        let req = Request::builder()
            .method(http::Method::POST)
            .uri("/internal/v1/billing/usage")
            .header("content-type", "application/json")
            .header("x-corelink-internal-auth", TEST_AUTH_KEY)
            .body(Body::from("{not-json"))
            .unwrap();
        let app = router(state_with(Arc::new(FakeStore::new())));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_event_kind_is_400() {
        let mut rec = record_json(&tenant_a(), &hex64(0x11));
        rec["event_kind"] = serde_json::json!("not_a_real_kind");
        let app = router(state_with(Arc::new(FakeStore::new())));
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([rec]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn bad_tenant_id_is_skipped_not_fatal() {
        // A single malformed record is SKIPPED (202, rejected:1, nothing staged),
        // NOT a 400 — the per-record-skip contract. The reason-code mapping is
        // pinned separately in `validate_record_reasons_pinned`.
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([record_json("not-a-uuid", &hex64(0x12))]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
        assert_eq!((ir.accepted, ir.rejected, ir.total), (0, 1, 0));
        assert!(store.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn bad_billing_period_is_skipped_not_fatal() {
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
        let mut rec = record_json(&tenant_a(), &hex64(0x13));
        rec["billing_period"] = serde_json::json!("2026-13");
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([rec]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
        assert_eq!((ir.accepted, ir.rejected, ir.total), (0, 1, 0));
        assert!(store.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn uppercase_region_is_canonicalized_and_accepted() {
        // Regression for the live `bad_region` flood: a box misconfigured with
        // BILLING_REGION="IAD" (uppercase) emits the SAME colo as "iad". It must
        // be canonicalized + ACCEPTED (202), staged as "iad" — never a 400 that
        // makes the runner retain-and-retry the batch forever.
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
        let mut rec = record_json(&tenant_a(), &hex64(0x14));
        rec["region"] = serde_json::json!("IAD");
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([rec]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert_eq!(
            store.seen.lock().unwrap().len(),
            1,
            "the upper-cased colo must be staged, not dropped"
        );
    }

    #[test]
    fn region_uppercase_canonicalizes_to_lowercase() {
        let wire: UsageRecordWire = serde_json::from_value({
            let mut r = record_json(&tenant_a(), &hex64(0x14));
            r["region"] = serde_json::json!("IaD");
            r
        })
        .unwrap();
        assert_eq!(validate_record(wire).unwrap().region, "iad");
    }

    #[test]
    fn region_with_non_letters_is_bad_region_even_after_lowercasing() {
        for bad in ["i2d", "us", "iada", "i-d"] {
            let wire: UsageRecordWire = serde_json::from_value({
                let mut r = record_json(&tenant_a(), &hex64(0x14));
                r["region"] = serde_json::json!(bad);
                r
            })
            .unwrap();
            assert_eq!(
                validate_record(wire),
                Err(RecordError::BadRegion),
                "region {bad:?} must fail-closed"
            );
        }
    }

    #[tokio::test]
    async fn bad_idem_key_is_skipped_not_fatal() {
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([record_json(&tenant_a(), "too-short")]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
        assert_eq!((ir.accepted, ir.rejected, ir.total), (0, 1, 0));
        assert!(store.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_record_is_skipped_good_record_staged() {
        // A batch with one GOOD then one BAD record must stage the GOOD one and
        // SKIP the bad one (202, `rejected:1`) — NOT 400 the whole batch. The old
        // all-or-nothing behaviour let one poison record block its batch-mates
        // AND flood the ingest (the runner retains + re-POSTs a non-2xx forever).
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
        let mut bad = record_json(&tenant_a(), &hex64(0x15));
        bad["billing_period"] = serde_json::json!("nope");
        let body = serde_json::json!([record_json(&tenant_a(), &hex64(0x16)), bad]);
        let resp = app
            .oneshot(ingest_request(Some(TEST_AUTH_KEY), body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
        assert_eq!(ir.accepted, 1, "the good record is staged");
        assert_eq!(ir.rejected, 1, "the bad record is counted, not fatal");
        assert_eq!(ir.total, 1, "total counts only successfully-staged rows");
        assert_eq!(
            store.seen.lock().unwrap().len(),
            1,
            "exactly the good record is staged"
        );
    }

    #[tokio::test]
    async fn all_records_invalid_still_drains_202() {
        // Even an ALL-bad batch must drain (202, `rejected:N`, nothing staged),
        // so a runner buffer full of poison records empties instead of re-POSTing
        // forever. Batch-LEVEL faults (empty / oversized / unparseable) stay 400.
        let store = Arc::new(FakeStore::new());
        let app = router(state_with(Arc::clone(&store) as Arc<dyn UsageStagingStore>));
        let mut b1 = record_json(&tenant_a(), &hex64(0x18));
        b1["region"] = serde_json::json!("nope4"); // > 3 chars → BadRegion
        let mut b2 = record_json(&tenant_a(), &hex64(0x19));
        b2["billing_period"] = serde_json::json!("2026-13");
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([b1, b2]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let ir: IngestResponse = serde_json::from_value(body_json(resp).await).unwrap();
        assert_eq!(
            (ir.accepted, ir.deduped, ir.rejected, ir.total),
            (0, 0, 2, 0)
        );
        assert!(store.seen.lock().unwrap().is_empty());
    }

    // ── Backend fault → 503 (fail-CLOSED) ────────────────────────────────────

    #[tokio::test]
    async fn backend_fault_returns_503() {
        let app = router(state_with(Arc::new(FakeStore::failing("d1 unreachable"))));
        let resp = app
            .oneshot(ingest_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!([record_json(&tenant_a(), &hex64(0x17))]),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ── Pure validation unit tests ───────────────────────────────────────────

    #[test]
    fn validate_record_accepts_canonical() {
        let wire: UsageRecordWire =
            serde_json::from_value(record_json(&tenant_a(), &hex64(0x20))).unwrap();
        let rec = validate_record(wire).unwrap();
        assert_eq!(rec.event_kind, UsageEventKind::RunnerSlotSeconds);
        assert_eq!(rec.region, "iad");
        assert_eq!(rec.qty, 7200);
    }

    #[test]
    fn idem_key_is_canonicalized_lowercase() {
        // The SAME BLAKE3 key spelled upper- vs lower-case must canonicalize to
        // ONE coordinate, else the `(tenant_id, idem_key)` dedup is bypassable.
        let lower = hex64(0xab); // "abab…" (32×"ab")
        let upper = lower.to_ascii_uppercase(); // "ABAB…"
        assert_ne!(lower, upper, "fixture must actually differ in case");

        let wire_lower: UsageRecordWire =
            serde_json::from_value(record_json(&tenant_a(), &lower)).unwrap();
        let wire_upper: UsageRecordWire =
            serde_json::from_value(record_json(&tenant_a(), &upper)).unwrap();

        let rec_lower = validate_record(wire_lower).unwrap();
        let rec_upper = validate_record(wire_upper).unwrap();

        // Both collapse to the same canonical (lowercase) idem_key → one row.
        assert_eq!(rec_lower.idem_key, lower);
        assert_eq!(rec_upper.idem_key, lower);
        assert_eq!(rec_lower.idem_key, rec_upper.idem_key);
    }

    #[test]
    fn source_over_cap_is_rejected() {
        let too_long: UsageRecordWire = serde_json::from_value({
            let mut r = record_json(&tenant_a(), &hex64(0x23));
            r["source"] = serde_json::json!("x".repeat(MAX_SOURCE_LEN + 1));
            r
        })
        .unwrap();
        assert_eq!(validate_record(too_long), Err(RecordError::SourceTooLong));

        // A source exactly at the cap is still accepted.
        let at_cap: UsageRecordWire = serde_json::from_value({
            let mut r = record_json(&tenant_a(), &hex64(0x24));
            r["source"] = serde_json::json!("x".repeat(MAX_SOURCE_LEN));
            r
        })
        .unwrap();
        assert!(validate_record(at_cap).is_ok());
    }

    #[test]
    fn validate_record_reasons_pinned() {
        let bad_region: UsageRecordWire = serde_json::from_value({
            let mut r = record_json(&tenant_a(), &hex64(0x21));
            r["region"] = serde_json::json!("us");
            r
        })
        .unwrap();
        assert_eq!(validate_record(bad_region), Err(RecordError::BadRegion));
    }

    #[test]
    fn payload_hash_is_64_hex_and_deterministic() {
        let wire: UsageRecordWire =
            serde_json::from_value(record_json(&tenant_a(), &hex64(0x22))).unwrap();
        let rec = validate_record(wire).unwrap();
        let h1 = D1UsageStagingStore::payload_hash(&rec);
        let h2 = D1UsageStagingStore::payload_hash(&rec);
        assert_eq!(h1.len(), 64, "BLAKE3-256 hex must be 64 chars");
        assert!(h1.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(
            h1, h2,
            "the same record must reproduce the same payload hash"
        );
    }

    #[test]
    fn build_state_returns_none_when_secret_absent() {
        if std::env::var("BILLING_INGEST_AUTH_KEY").is_err() {
            assert!(build_state_from_env().is_none());
        }
    }
}
