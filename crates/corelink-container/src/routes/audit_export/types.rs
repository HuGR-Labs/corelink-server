//! Canonical constants + `ExportAuditRow` data shape for the
//! `/v1/audit/export` route.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Verbatim move of the constants + audit-row struct; no behavioural
//! change.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical audit-export route path.
///
/// The `:tenant` path segment mirrors the `/v1/cas/:tenant/:hash` and
/// `/v1/ac/:tenant/:key` patterns. The Worker extracts the PAT-resolved
/// tenant from the path, routes the DO, and forwards the full path to
/// the container — so the container MUST declare `:tenant` here or
/// every `/v1/audit/<tenant>/export` request returns 404.
pub const AUDIT_EXPORT_ROUTE: &str = "/v1/audit/:tenant/export";

/// Header name production wiring uses to inject the
/// JWT-validated tenant id (see module-level docs §Auth model).
pub const TENANT_ID_HEADER: &str = "x-tenant-id";

/// Canonical CloudEvents-1.0 `type` literal for the export-request
/// audit emit.
pub const EVENT_TYPE_EXPORT_REQUEST: &str = "corelink.audit.export_request.v1";

/// Canonical event type for the cross-tenant audit-export attempt
/// security emit.
pub const EVENT_TYPE_CROSS_TENANT_ATTEMPT: &str =
    "corelink.security.audit_export_cross_tenant_attempt.v1";

/// Canonical event type for the SEV-0 audit-export verify failure
/// emit.
pub const EVENT_TYPE_VERIFY_FAILED: &str =
    "corelink.audit.export_verify_failed.v1";

/// Canonical chain-head anchor response header (mirrors the
/// wave-17 customer-CLI doc reference). Lower-case per HTTP/2 wire
/// convention.
pub const HEADER_CHAIN_HEAD_ANCHOR: &str =
    "x-corelink-audit-export-chain-head-anchor";

/// Wave-18 HTTP trailer name surfaced on the response when the
/// streaming verifier detects a chain-break MID-STREAM. The trailer
/// value is a compact JSON object
/// `{"break_at_seq":<u64>,"break_at_chunk":<u64>,"observed":"<hex>","expected":"<hex>"}`
/// — the customer-CLI parses it to surface the actionable diagnostic
/// "Export aborted mid-stream — server detected chain break at seq N
/// chunk X" (see `tools/cli/src/commands/verify_ndjson.rs`).
///
/// Per HTTP/1.1 (RFC 7230 §4.4) trailers MUST be advertised up-front
/// via the `Trailer:` response header so intermediaries that strip
/// unknown trailers know to preserve this one; we always advertise
/// the trailer name even on the happy path so the wire shape is
/// stable.
pub const HEADER_EXPORT_ABORTED: &str =
    "x-corelink-audit-export-aborted";

/// Wave-19 — environment variable tuning the maximum per-row body
/// buffer size (in bytes) the async streaming generator will hold
/// in memory before flushing the row's `Frame::data`. The bound is
/// load-bearing for the wave-19 "true page-by-page yield" lift: per
/// the audit doc §4 wave-19 row, we MUST not buffer more than one
/// page's worth of rows in flight, AND each row MUST not exceed this
/// per-row ceiling (rows above the ceiling are emitted in multiple
/// data frames). Default = 64 KiB which fits 32 typical
/// `{event, proof}` envelopes; override via
/// `EXPORT_ROW_BUFFER_BYTES=<usize>` at boot.
pub const ENV_EXPORT_ROW_BUFFER_BYTES: &str = "EXPORT_ROW_BUFFER_BYTES";
/// Default value for [`ENV_EXPORT_ROW_BUFFER_BYTES`] — 64 KiB. The
/// constant is `pub` so the wave-19 test harness asserts the canonical
/// default without re-importing the env-parsing helper.
pub const DEFAULT_EXPORT_ROW_BUFFER_BYTES: usize = 65_536;
/// Wave-19 — canonical R2 list page size (keys per page). Mirrors the
/// CF API v4 `per_page=1000` default the wave-18 7-day matrix lift
/// adopted for the daily-verify cron (`audit-chain-daily-verify.yml`).
/// `pub` so the wave-19 integration test pins the contract.
pub const R2_LIST_PAGE_SIZE: usize = 1000;

/// Audit emit record captured by the route. Carries the canonical
/// CloudEvents `type` + the load-bearing tenant + window + byte
/// count + exit status fields per WI-S09-008 §4 (DELIVERABLES).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportAuditRow {
    /// Canonical CloudEvents `type` (one of `EVENT_TYPE_*` constants).
    pub event_type: String,
    /// Authenticated tenant (from the `X-Tenant-Id` header) — `None`
    /// if the request never reached the auth step.
    pub authenticated_tenant: Option<Uuid>,
    /// Attempted tenant (from the optional `tenant` query parameter).
    /// Populated on the cross-tenant-attempt arm.
    pub attempted_tenant: Option<Uuid>,
    /// Window lower bound (Unix epoch ms; inclusive).
    pub from_ms: u64,
    /// Window upper bound (Unix epoch ms; exclusive).
    pub to_ms: u64,
    /// NDJSON byte count INTENDED for the response stream — populated
    /// before the body is flushed so the audit-emit can land BEFORE the
    /// first byte (per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). On an
    /// HTTP-2 RST_STREAM mid-flush the value over-reports bytes shipped;
    /// the `bytes_emitted = bytes_acked` SLO is ground-truthed via a
    /// post-flush wall-clock metric, NOT this field. `0` for reject
    /// paths (we never wrote bytes). Wave-20 (A-P2-02 closure):
    /// documented the intent-to-flush semantic surfaced in the wave-18
    /// adversarial review.
    pub bytes_written: u64,
    /// Number of audit events flushed (matches manifest.event_count
    /// on the happy path; `0` on reject / empty range).
    pub events_written: u64,
    /// Canonical exit status: `"ok"` / `"empty"` / `"cross_tenant_reject"` /
    /// `"verify_failed"` / `"verify_failed_mid_stream"` / `"unauthorized"` /
    /// `"rate_limited"` / `"bad_request"` / `"audit_failed"`. Wave-19
    /// lifts the mid-stream variant out of the colon-prefix encoding into
    /// the stable enum + structured [`Self::payload`] field.
    pub exit_status: String,
    /// Wave-19 structured payload (mid-stream chain-break diagnostic).
    /// `#[serde(default)]` keeps wave-18 row JSON parseable; `skip_serializing_if`
    /// keeps the on-wire shape unchanged for emit arms that don't populate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Wave-19 stable enum for the mid-stream chain-break exit-status (replaces
/// the wave-18 colon-prefix `verify_failed_mid_stream:<json>` encoding).
pub const EXIT_STATUS_VERIFY_FAILED_MID_STREAM: &str = "verify_failed_mid_stream";
