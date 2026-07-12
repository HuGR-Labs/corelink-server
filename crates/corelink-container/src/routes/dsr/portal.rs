//! Customer-facing DSR self-service portal — `/v1/privacy/dsr/*`.
//!
//! This is the CUSTOMER intake surface for the data-subject rights the
//! admin-ui `apps/admin-ui/src/lib/dsr-client.ts` already posts to. It mounts
//! the six rights + status + MFA verify, mirrors the [`crate::routes::customer`]
//! auth model (tenant derived EXCLUSIVELY from the Worker-injected
//! `x-corelink-tenant-id`, PAT possession backstop, Clerk-session gate), and
//! DRIVES THE EXISTING LIVE D1 PIPELINE — it is NOT a second erasure/gather
//! engine:
//!
//! - **Access** (Art.15) → [`super::access::run_access`] (live D1 gather).
//! - **Portability** (Art.20) → [`super::access::run_portability`] (live gather
//!   + best-effort signed R2 export).
//! - **Rectification** (Art.16) → [`super::access::run_rectification`] (live,
//!   allowlisted-field UPDATE, hashed value).
//! - **Erasure** (Art.17) → the SAME in-process, D1-backed erasure worker the
//!   Clerk `user.deleted` + `/v1/customer/account/delete` paths use, via the
//!   [`crate::routes::customer::AccountDeletionRequester`] seam (writes the
//!   `dsr_requested` legitimacy anchor, then drives the worker).
//! - **Restriction** (Art.18) / **Objection** (Art.21) are policy-only: the
//!   ticket is durably recorded `pending` for the operator's manual disposition
//!   (there is no automatic data mutation for these arms).
//!
//! ## Auth (mirrors `routes/customer.rs`)
//!
//! Every route is a Clerk-session dashboard surface: the Worker edge-verifies
//! the Clerk JWT, resolves the tenant, strips the bearer, and forwards
//! `x-corelink-tenant-id` + `x-corelink-token-prefix: clerk`. A missing/sentinel
//! tenant → 401 (fail-CLOSED); a cache PAT caller (`token-prefix != clerk`) →
//! 403 (a data-plane credential must never drive a data-subject-rights request,
//! matching the `/v1/customer/account/delete` posture). The native PAT
//! possession backstop runs first and is skipped for Clerk callers.
//!
//! ## MFA step-up (CTRL-AUTH-010)
//!
//! The destructive arms (Erasure + Rectification) are gated on the
//! Worker-trusted freshness header `x-corelink-mfa-verified`. The Worker is the
//! SOLE setter (the header is in the strip list, so a client can never forge
//! it); it stamps `1` on the privacy plane for an edge-verified Clerk session.
//! When present the destructive op runs inline; when ABSENT the ticket is
//! recorded `pending` with `mfa_required=true` and NO data is mutated — the
//! customer completes it via `POST /v1/privacy/dsr/{request_id}/verify-mfa`.
//! The cryptographic WebAuthn step-up binding is the deferred edge wiring (per
//! `corelink-dsr`); tightening the header condition to require a real step-up
//! assertion is a Worker-side change, not a container rewire.
//!
//! ## Isolation, fail-CLOSED, rate-limited, audited
//!
//! The ticket store is tenant-leftmost `(tenant_id, request_id)` so a
//! cross-tenant status read is structurally impossible; a miss and a
//! cross-tenant lookup are indistinguishable (constant-time 404). Every
//! data-mutating pipeline call already emits its `audit_outbox` envelope BEFORE
//! the mutation (ADR-S11-002); the durable ticket is the intake evidence.
//! Submissions are capped at [`DSR_DAILY_LIMIT`]/day per tenant (LGPD Art.20).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use corelink_dsr::{sla_for, DsrJurisdiction, DsrRequestKind};

use crate::customer_d1::ms_to_iso8601;
use crate::routes::customer::{AccountDeletionError, AccountDeletionRequester};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;

/// Milliseconds in a rolling 24h rate-limit window.
const DAY_MS: u64 = 86_400_000;

/// Per-tenant DSR submissions allowed per rolling 24h window (LGPD Art.20 humane
/// cap; mirrors the `corelink-dsr` production rate-limit note).
const DSR_DAILY_LIMIT: u64 = 10;

/// Worker-set token-prefix for an edge-verified Clerk session (mirrors
/// `routes/customer.rs`). Only Clerk callers may drive a DSR request.
const CLERK_TOKEN_PREFIX: &str = "clerk";

/// Worker-trusted MFA step-up freshness marker (in the Worker strip list — a
/// client can never forge it). `1` ⇒ the destructive arm may run inline.
const MFA_VERIFIED_HEADER: &str = "x-corelink-mfa-verified";

/// Optional Worker-set jurisdiction macro (from the tenant's residency); absent
/// ⇒ default GDPR (the strictest calendar-month SLA).
const JURISDICTION_HEADER: &str = "x-corelink-jurisdiction";

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant
/// (mirrors `auth_tenant::AuthTenant` + `routes/customer.rs`).
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

// ─── Ticket model ──────────────────────────────────────────────────────────────

/// Lifecycle status of a DSR ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketStatus {
    /// Recorded, awaiting execution (MFA step-up or operator disposition).
    Pending,
    /// Execution in flight.
    InProgress,
    /// Terminal success.
    Completed,
    /// Terminal failure / refusal.
    Rejected,
}

impl TicketStatus {
    /// Wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Rejected => "rejected",
        }
    }

    /// Parse a persisted label (unknown ⇒ `Pending`, the safe non-terminal).
    #[must_use]
    fn parse(s: &str) -> Self {
        match s {
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "rejected" => Self::Rejected,
            _ => Self::Pending,
        }
    }
}

/// One status-transition timeline entry.
#[derive(Clone, Debug)]
pub struct TimelineEvent {
    /// Transition instant (Unix epoch ms).
    pub at_ms: u64,
    /// Prior status (`None` for the initial `received` row).
    pub from: Option<TicketStatus>,
    /// New status.
    pub to: TicketStatus,
    /// Optional human note (non-PII).
    pub note: Option<String>,
}

impl TimelineEvent {
    fn to_json(&self) -> Value {
        json!({
            "at": ms_to_iso8601(clamp_i64(self.at_ms)),
            "from": self.from.map(TicketStatus::as_str),
            "to": self.to.as_str(),
            "note": self.note,
        })
    }
}

/// A durable DSR request record (one row of `dsr_tickets`).
#[derive(Clone, Debug)]
pub struct DsrTicket {
    /// Owning tenant (Worker-resolved).
    pub tenant_id: String,
    /// Canonical UUIDv7 request id (idempotency + status key).
    pub request_id: String,
    /// Which right was exercised.
    pub action: DsrRequestKind,
    /// Lifecycle status.
    pub status: TicketStatus,
    /// SLA jurisdiction.
    pub jurisdiction: DsrJurisdiction,
    /// Optional reason (erasure/restriction/objection).
    pub reason: Option<String>,
    /// Whether the arm is blocked on an MFA step-up.
    pub mfa_required: bool,
    /// Submission instant (Unix epoch ms).
    pub submitted_at_ms: u64,
    /// SLA deadline (Unix epoch ms).
    pub sla_deadline_ms: u64,
    /// HS256 proof-of-submission receipt (customer-held).
    pub receipt: String,
    /// Export handle once an Access/Portability bundle is persisted.
    pub data_download_url: Option<String>,
    /// Status-transition timeline.
    pub timeline: Vec<TimelineEvent>,
    /// Last-mutation instant (Unix epoch ms).
    pub updated_at_ms: u64,
}

impl DsrTicket {
    fn timeline_json(&self) -> String {
        let arr: Vec<Value> = self.timeline.iter().map(TimelineEvent::to_json).collect();
        serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_owned())
    }

    /// Client-facing `DsrRequestDetail` shape (dsr-types.ts).
    fn detail_json(&self) -> Value {
        json!({
            "request_id": self.request_id,
            "action": self.action.as_str(),
            "status": self.status.as_str(),
            "submitted_at": ms_to_iso8601(clamp_i64(self.submitted_at_ms)),
            "sla_deadline": ms_to_iso8601(clamp_i64(self.sla_deadline_ms)),
            "jurisdiction": self.jurisdiction.as_str(),
            "timeline": self.timeline.iter().map(TimelineEvent::to_json).collect::<Vec<_>>(),
            "data_download_url": self.data_download_url,
        })
    }

    /// Client-facing `DsrRequestSummary` shape (list view).
    fn summary_json(&self) -> Value {
        json!({
            "request_id": self.request_id,
            "action": self.action.as_str(),
            "status": self.status.as_str(),
            "submitted_at": ms_to_iso8601(clamp_i64(self.submitted_at_ms)),
            "sla_deadline": ms_to_iso8601(clamp_i64(self.sla_deadline_ms)),
            "jurisdiction": self.jurisdiction.as_str(),
        })
    }
}

// ─── Ticket store (D1 + in-memory) ──────────────────────────────────────────────

/// Durable, tenant-scoped ticket store. Production wires [`D1DsrTicketStore`];
/// tests supply [`InMemoryDsrTicketStore`].
pub trait DsrTicketStore: Send + Sync + core::fmt::Debug {
    /// Insert a fresh ticket (`INSERT OR IGNORE` on the `(tenant_id,
    /// request_id)` PK — idempotent).
    ///
    /// # Errors
    /// Any storage/transport fault (the route maps it to 500 fail-CLOSED).
    fn insert(&self, ticket: &DsrTicket) -> Result<(), String>;

    /// Fetch by `(tenant_id, request_id)`. `None` ⇒ miss OR cross-tenant (the
    /// caller renders both as a constant-time 404).
    ///
    /// # Errors
    /// Any storage/transport fault.
    fn get(&self, tenant_id: &str, request_id: &str) -> Result<Option<DsrTicket>, String>;

    /// List a tenant's tickets, newest-first, capped at `limit`.
    ///
    /// # Errors
    /// Any storage/transport fault.
    fn list(&self, tenant_id: &str, limit: usize) -> Result<Vec<DsrTicket>, String>;

    /// Persist a status/timeline transition (full-row upsert).
    ///
    /// # Errors
    /// Any storage/transport fault.
    fn update(&self, ticket: &DsrTicket) -> Result<(), String>;

    /// Count a tenant's submissions since `since_ms` (rate-limit window).
    ///
    /// # Errors
    /// Any storage/transport fault.
    fn count_since(&self, tenant_id: &str, since_ms: u64) -> Result<u64, String>;
}

/// In-memory ticket store (tests / dev-CI fallback). Tenant isolation is
/// enforced the same way the D1 store is: every read binds `tenant_id`.
#[derive(Debug, Default)]
pub struct InMemoryDsrTicketStore {
    rows: std::sync::Mutex<Vec<DsrTicket>>,
}

impl InMemoryDsrTicketStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl DsrTicketStore for InMemoryDsrTicketStore {
    fn insert(&self, ticket: &DsrTicket) -> Result<(), String> {
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "ticket store poisoned".to_owned())?;
        // INSERT OR IGNORE semantics: a duplicate (tenant, request_id) is a no-op.
        if rows
            .iter()
            .any(|r| r.tenant_id == ticket.tenant_id && r.request_id == ticket.request_id)
        {
            return Ok(());
        }
        rows.push(ticket.clone());
        Ok(())
    }

    fn get(&self, tenant_id: &str, request_id: &str) -> Result<Option<DsrTicket>, String> {
        let rows = self
            .rows
            .lock()
            .map_err(|_| "ticket store poisoned".to_owned())?;
        Ok(rows
            .iter()
            .find(|r| r.tenant_id == tenant_id && r.request_id == request_id)
            .cloned())
    }

    fn list(&self, tenant_id: &str, limit: usize) -> Result<Vec<DsrTicket>, String> {
        let rows = self
            .rows
            .lock()
            .map_err(|_| "ticket store poisoned".to_owned())?;
        let mut mine: Vec<DsrTicket> = rows
            .iter()
            .filter(|r| r.tenant_id == tenant_id)
            .cloned()
            .collect();
        mine.sort_by(|a, b| b.submitted_at_ms.cmp(&a.submitted_at_ms));
        mine.truncate(limit);
        Ok(mine)
    }

    fn update(&self, ticket: &DsrTicket) -> Result<(), String> {
        let mut rows = self
            .rows
            .lock()
            .map_err(|_| "ticket store poisoned".to_owned())?;
        if let Some(slot) = rows
            .iter_mut()
            .find(|r| r.tenant_id == ticket.tenant_id && r.request_id == ticket.request_id)
        {
            *slot = ticket.clone();
        }
        Ok(())
    }

    fn count_since(&self, tenant_id: &str, since_ms: u64) -> Result<u64, String> {
        let rows = self
            .rows
            .lock()
            .map_err(|_| "ticket store poisoned".to_owned())?;
        Ok(rows
            .iter()
            .filter(|r| r.tenant_id == tenant_id && r.submitted_at_ms >= since_ms)
            .count() as u64)
    }
}

/// D1-backed ticket store (production). Every statement binds `tenant_id` so the
/// tenant-leftmost PK makes a cross-tenant read impossible by construction.
pub struct D1DsrTicketStore {
    d1: Arc<D1HttpClient>,
}

impl core::fmt::Debug for D1DsrTicketStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1DsrTicketStore")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl D1DsrTicketStore {
    /// Wire over a shared D1 client.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    fn row_to_ticket(row: &crate::storage::d1_http::D1Row) -> Option<DsrTicket> {
        let get_str = |k: &str| row.get(k).and_then(Value::as_str).map(str::to_owned);
        let get_ms = |k: &str| {
            row.get(k)
                .and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .map(|n| u64::try_from(n).unwrap_or(0))
        };
        let tenant_id = get_str("tenant_id")?;
        let request_id = get_str("request_id")?;
        let action = parse_action(&get_str("action").unwrap_or_default())?;
        let jurisdiction = parse_jurisdiction(&get_str("jurisdiction").unwrap_or_default());
        let timeline = get_str("timeline")
            .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
            .map(|arr| arr.iter().filter_map(parse_timeline_event).collect())
            .unwrap_or_default();
        let mfa_required = row
            .get("mfa_required")
            .and_then(|v| v.as_i64().or_else(|| v.as_bool().map(i64::from)))
            .unwrap_or(0)
            != 0;
        Some(DsrTicket {
            tenant_id,
            request_id,
            action,
            status: TicketStatus::parse(&get_str("status").unwrap_or_default()),
            jurisdiction,
            reason: get_str("reason"),
            mfa_required,
            submitted_at_ms: get_ms("submitted_at_ms").unwrap_or(0),
            sla_deadline_ms: get_ms("sla_deadline_ms").unwrap_or(0),
            receipt: get_str("receipt").unwrap_or_default(),
            data_download_url: get_str("data_download_url"),
            timeline,
            updated_at_ms: get_ms("updated_at_ms").unwrap_or(0),
        })
    }

    fn upsert_sql() -> &'static str {
        "INSERT INTO dsr_tickets \
         (tenant_id, request_id, action, status, jurisdiction, reason, mfa_required, \
          submitted_at_ms, sla_deadline_ms, receipt, data_download_url, timeline, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) \
         ON CONFLICT(tenant_id, request_id) DO UPDATE SET \
          status = excluded.status, reason = excluded.reason, \
          mfa_required = excluded.mfa_required, data_download_url = excluded.data_download_url, \
          timeline = excluded.timeline, updated_at_ms = excluded.updated_at_ms"
    }

    fn upsert_params(ticket: &DsrTicket) -> Vec<Value> {
        vec![
            json!(ticket.tenant_id),
            json!(ticket.request_id),
            json!(ticket.action.as_str()),
            json!(ticket.status.as_str()),
            json!(ticket.jurisdiction.as_str()),
            json!(ticket.reason),
            json!(i64::from(ticket.mfa_required)),
            json!(clamp_i64(ticket.submitted_at_ms)),
            json!(clamp_i64(ticket.sla_deadline_ms)),
            json!(ticket.receipt),
            json!(ticket.data_download_url),
            json!(ticket.timeline_json()),
            json!(clamp_i64(ticket.updated_at_ms)),
        ]
    }
}

impl DsrTicketStore for D1DsrTicketStore {
    fn insert(&self, ticket: &DsrTicket) -> Result<(), String> {
        super::d1util::d1_query_blocking(&self.d1, Self::upsert_sql(), Self::upsert_params(ticket))
            .map(|_| ())
    }

    fn get(&self, tenant_id: &str, request_id: &str) -> Result<Option<DsrTicket>, String> {
        let rows = super::d1util::d1_query_blocking(
            &self.d1,
            "SELECT * FROM dsr_tickets WHERE tenant_id = ?1 AND request_id = ?2 LIMIT 1",
            vec![json!(tenant_id), json!(request_id)],
        )?;
        Ok(rows.first().and_then(Self::row_to_ticket))
    }

    fn list(&self, tenant_id: &str, limit: usize) -> Result<Vec<DsrTicket>, String> {
        let rows = super::d1util::d1_query_blocking(
            &self.d1,
            "SELECT * FROM dsr_tickets WHERE tenant_id = ?1 \
             ORDER BY submitted_at_ms DESC LIMIT ?2",
            vec![json!(tenant_id), json!(i64::try_from(limit).unwrap_or(50))],
        )?;
        Ok(rows.iter().filter_map(Self::row_to_ticket).collect())
    }

    fn update(&self, ticket: &DsrTicket) -> Result<(), String> {
        self.insert(ticket)
    }

    fn count_since(&self, tenant_id: &str, since_ms: u64) -> Result<u64, String> {
        let rows = super::d1util::d1_query_blocking(
            &self.d1,
            "SELECT COUNT(*) AS n FROM dsr_tickets WHERE tenant_id = ?1 AND submitted_at_ms >= ?2",
            vec![json!(tenant_id), json!(clamp_i64(since_ms))],
        )?;
        Ok(rows
            .first()
            .and_then(|r| r.get("n"))
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .map(|n| u64::try_from(n).unwrap_or(0))
            .unwrap_or(0))
    }
}

// ─── Live pipeline seam ──────────────────────────────────────────────────────────

/// The live data-operation driver. Production wires [`LivePipeline`] over the
/// EXISTING Wave-1 engine (`super::access::*` + the erasure worker); tests
/// supply a fake.
pub trait DsrPipeline: Send + Sync + core::fmt::Debug {
    /// Art.15 — gather the subject's data (machine-readable JSON).
    ///
    /// # Errors
    /// Any gather/D1 fault (fail-CLOSED: never a partial export).
    fn access(&self, tenant_id: &str, dsr_id: &str, now_ms: u64) -> Result<Value, String>;

    /// Art.20 — gather + best-effort durable export; returns the bundle and the
    /// persisted export handle (`None` when not persisted).
    ///
    /// # Errors
    /// Any gather/D1 fault (fail-CLOSED).
    fn portability(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        now_ms: u64,
    ) -> Result<(Value, Option<String>), String>;

    /// Art.16 — correct the allowlisted contact email (hashed by the live
    /// pipeline). `Ok(Ok(()))` applied; `Ok(Err(msg))` is a fail-CLOSED 4xx
    /// refusal (immutable/non-editable/invalid); `Err` is an engine fault.
    ///
    /// # Errors
    /// Any engine/D1 fault.
    fn rectification(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        email: &str,
        now_ms: u64,
    ) -> Result<Result<(), String>, String>;

    /// Art.17 — drive the live erasure worker for the whole subject/tenant.
    /// `Ok(())` on accept OR no-account (idempotent).
    ///
    /// # Errors
    /// Any engine/D1 fault (or the erasure path being unconfigured).
    fn erasure(&self, tenant_id: &str) -> Result<(), String>;
}

/// Production pipeline over the live Wave-1 engine.
pub struct LivePipeline {
    d1: Arc<D1HttpClient>,
    r2_audit: Option<Arc<R2S3Client>>,
    /// Reuses the account-deletion requester (writes the `dsr_requested` anchor
    /// then drives the in-process erasure worker). `None` ⇒ erasure returns a
    /// fail-CLOSED error (never a silent ack).
    erasure: Option<Arc<dyn AccountDeletionRequester>>,
}

impl core::fmt::Debug for LivePipeline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LivePipeline")
            .field("d1", &"[D1HttpClient]")
            .field("r2_audit", &self.r2_audit.as_ref().map(|_| "[R2S3Client]"))
            .field("erasure", &self.erasure.is_some())
            .finish()
    }
}

impl DsrPipeline for LivePipeline {
    fn access(&self, tenant_id: &str, dsr_id: &str, now_ms: u64) -> Result<Value, String> {
        let export = super::access::run_access(&self.d1, dsr_id, tenant_id, now_ms)?;
        serde_json::to_value(export).map_err(|e| format!("export serialize: {e}"))
    }

    fn portability(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        now_ms: u64,
    ) -> Result<(Value, Option<String>), String> {
        let (export, receipt) = super::access::run_portability(
            &self.d1,
            self.r2_audit.as_ref(),
            dsr_id,
            tenant_id,
            now_ms,
        )?;
        let handle = if receipt.persisted {
            receipt.r2_key
        } else {
            None
        };
        let value = serde_json::to_value(export).map_err(|e| format!("export serialize: {e}"))?;
        Ok((value, handle))
    }

    fn rectification(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        email: &str,
        now_ms: u64,
    ) -> Result<Result<(), String>, String> {
        // The only live-rectifiable subject field is the account contact email
        // (`tenant.email_hash`), corrected + hashed by the live pipeline.
        match super::access::run_rectification(
            &self.d1,
            dsr_id,
            tenant_id,
            "tenant",
            "email_hash",
            email,
            now_ms,
        )? {
            Ok(_result) => Ok(Ok(())),
            Err(reject) => Ok(Err(reject.message().to_owned())),
        }
    }

    fn erasure(&self, tenant_id: &str) -> Result<(), String> {
        let requester = self
            .erasure
            .as_ref()
            .ok_or_else(|| "erasure pipeline not configured".to_owned())?;
        match requester.request_erasure(tenant_id) {
            // No provisioned account (already erased / never provisioned) — the
            // Art.17 obligation is satisfied by construction (idempotent).
            Ok(()) | Err(AccountDeletionError::NotFound) => Ok(()),
            Err(AccountDeletionError::Internal(e)) => Err(e),
        }
    }
}

// ─── Route state ─────────────────────────────────────────────────────────────────

/// Shared state for the `/v1/privacy/dsr/*` router.
#[derive(Clone)]
pub struct PrivacyDsrRouteState {
    /// Live data-op driver. `None` (dev/CI, storage unconfigured) ⇒ the data
    /// rights fail CLOSED (503) — never a silent ack of an unhonored right.
    pub pipeline: Option<Arc<dyn DsrPipeline>>,
    /// Durable, tenant-scoped ticket store (D1 in prod, in-memory in dev/CI).
    pub tickets: Arc<dyn DsrTicketStore>,
    /// Native PAT possession backstop (mirrors `customer::pat_gate`). Skipped for
    /// Clerk callers; `None` in dev/CI.
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
    /// HS256 receipt-signing key (proof-of-submission).
    pub receipt_key: Arc<Vec<u8>>,
}

impl core::fmt::Debug for PrivacyDsrRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PrivacyDsrRouteState")
            .field("pipeline", &self.pipeline.is_some())
            .field("tickets", &self.tickets)
            .field("pat_gate", &self.pat_gate.is_some())
            .field("receipt_key", &"<redacted>")
            .finish()
    }
}

/// Build production state from env. The router is ALWAYS mountable (so the
/// dashboard surface exists), but the data rights fail CLOSED (503) until the D1
/// storage env is configured. Mirrors `customer::build_handlers_from_env`.
#[must_use]
pub fn build_state_from_env() -> PrivacyDsrRouteState {
    let (pipeline, tickets): (Option<Arc<dyn DsrPipeline>>, Arc<dyn DsrTicketStore>) =
        match build_live() {
            Some((pipe, store)) => (Some(pipe), store),
            None => {
                tracing::warn!(
                    "StorageEnv unset/invalid; /v1/privacy/dsr/* data rights fail CLOSED \
                     (503) — ticket store is in-memory (dev/CI)"
                );
                (None, Arc::new(InMemoryDsrTicketStore::new()))
            }
        };
    PrivacyDsrRouteState {
        pipeline,
        tickets,
        // Wired by `routes.rs` from `native_pat_gate_from_env()` (mirrors the
        // customer plane); `None` here keeps the factory env-pure.
        pat_gate: None,
        receipt_key: Arc::new(receipt_key_from_env()),
    }
}

/// Build the live pipeline + D1 ticket store when storage is configured.
fn build_live() -> Option<(Arc<dyn DsrPipeline>, Arc<dyn DsrTicketStore>)> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(D1HttpClient::new(&storage_env).ok()?);
    // R2 audit bucket for the portability signed export (best-effort; `None`
    // unless the operator asserted a single attestation region).
    let r2_audit = super::build_audit_r2_client();
    // Reuse the account-deletion requester (anchor + in-process erasure worker).
    let erasure = crate::routes::customer::account_deletion_from_env();
    let pipeline: Arc<dyn DsrPipeline> = Arc::new(LivePipeline {
        d1: Arc::clone(&d1),
        r2_audit,
        erasure,
    });
    let tickets: Arc<dyn DsrTicketStore> = Arc::new(D1DsrTicketStore::new(d1));
    Some((pipeline, tickets))
}

/// Resolve the HS256 receipt-signing key: the dedicated
/// `DSR_RECEIPT_SIGNING_KEY`, else the shared erase/internal-auth key (so prod
/// receipts are genuine without a new secret), else a dev placeholder.
fn receipt_key_from_env() -> Vec<u8> {
    if let Ok(k) = std::env::var("DSR_RECEIPT_SIGNING_KEY") {
        if k.len() >= 32 {
            return k.into_bytes();
        }
    }
    if let Some(k) = crate::routes::admin::erase_auth_key_from_env() {
        if k.len() >= 32 {
            return k.as_bytes().to_vec();
        }
    }
    tracing::warn!(
        "no DSR_RECEIPT_SIGNING_KEY / erase-auth key (>=32 chars); DSR receipts \
         signed with a dev placeholder (non-prod)"
    );
    b"corelink-dev-dsr-receipt-placeholder-key".to_vec()
}

// ─── Router ──────────────────────────────────────────────────────────────────────

/// Mount `/v1/privacy/dsr/*` (the six rights + status + list + verify-mfa).
pub fn router(state: PrivacyDsrRouteState) -> Router {
    Router::new()
        .route("/v1/privacy/dsr/access", post(handle_access))
        .route("/v1/privacy/dsr/portability", post(handle_portability))
        .route("/v1/privacy/dsr/rectification", post(handle_rectification))
        .route("/v1/privacy/dsr/erasure", post(handle_erasure))
        .route("/v1/privacy/dsr/restriction", post(handle_restriction))
        .route("/v1/privacy/dsr/objection", post(handle_objection))
        .route("/v1/privacy/dsr", get(handle_list))
        .route("/v1/privacy/dsr/{request_id}/status", get(handle_status))
        .route(
            "/v1/privacy/dsr/{request_id}/verify-mfa",
            post(handle_verify_mfa),
        )
        .with_state(state)
}

// ─── Request body shapes (dsr-client.ts contract) ──────────────────────────────

/// `POST /v1/privacy/dsr/{action}` body (the `action` field is in the URL and
/// stripped by the client, so it is NOT in the body).
#[derive(Debug, Default, Deserialize)]
struct DsrSubmitBody {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    rectification: Option<RectificationBody>,
}

/// Rectification target fields (only the contact email is live-rectifiable).
#[derive(Debug, Default, Deserialize)]
struct RectificationBody {
    #[serde(default)]
    email: Option<String>,
}

/// `POST /v1/privacy/dsr/{request_id}/verify-mfa` body (optional step-up token;
/// the load-bearing gate is the Worker-trusted freshness header).
#[derive(Debug, Default, Deserialize)]
struct VerifyMfaBody {
    #[serde(default)]
    #[allow(dead_code)]
    token: Option<String>,
}

/// `GET /v1/privacy/dsr` query params.
#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(default)]
    limit: Option<usize>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────────

async fn handle_access(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(state, headers, DsrRequestKind::Access, unwrap_body(body)).await
}

async fn handle_portability(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(
        state,
        headers,
        DsrRequestKind::Portability,
        unwrap_body(body),
    )
    .await
}

async fn handle_rectification(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(
        state,
        headers,
        DsrRequestKind::Rectification,
        unwrap_body(body),
    )
    .await
}

async fn handle_erasure(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(state, headers, DsrRequestKind::Erasure, unwrap_body(body)).await
}

async fn handle_restriction(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(
        state,
        headers,
        DsrRequestKind::Restriction,
        unwrap_body(body),
    )
    .await
}

async fn handle_objection(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    body: Option<Json<DsrSubmitBody>>,
) -> Response {
    submit(state, headers, DsrRequestKind::Objection, unwrap_body(body)).await
}

fn unwrap_body(body: Option<Json<DsrSubmitBody>>) -> DsrSubmitBody {
    body.map(|Json(b)| b).unwrap_or_default()
}

/// The shared submit pipeline: auth → rate-limit → receipt → dispatch → persist.
async fn submit(
    state: PrivacyDsrRouteState,
    headers: HeaderMap,
    action: DsrRequestKind,
    body: DsrSubmitBody,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let now = now_ms();

    // Rate limit (LGPD Art.20 humane cap) — fail-CLOSED on a store fault.
    match state
        .tickets
        .count_since(&tenant, now.saturating_sub(DAY_MS))
    {
        Ok(n) if n >= DSR_DAILY_LIMIT => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "daily DSR request limit reached",
            )
                .into_response()
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!(error = %e, "dsr/submit: rate-limit count failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "rate-limit check failed").into_response();
        }
    }

    let jurisdiction = jurisdiction_from(&headers);
    let request_id = Uuid::now_v7().to_string();
    let sla_deadline_ms = sla_for(jurisdiction, now);
    let receipt = issue_receipt(
        &state.receipt_key,
        &request_id,
        action,
        jurisdiction,
        sla_deadline_ms,
        now,
        &tenant,
    );

    let Some(pipeline) = state.pipeline.as_ref() else {
        // Fail-CLOSED: the live pipeline is not wired (dev/CI / unconfigured).
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "DSR pipeline not configured",
        )
            .into_response();
    };

    let mut ticket = DsrTicket {
        tenant_id: tenant.clone(),
        request_id: request_id.clone(),
        action,
        status: TicketStatus::Pending,
        jurisdiction,
        reason: body.reason.clone(),
        mfa_required: false,
        submitted_at_ms: now,
        sla_deadline_ms,
        receipt: receipt.clone(),
        data_download_url: None,
        timeline: vec![TimelineEvent {
            at_ms: now,
            from: None,
            to: TicketStatus::Pending,
            note: Some("received".to_owned()),
        }],
        updated_at_ms: now,
    };

    match action {
        DsrRequestKind::Access => match pipeline.access(&tenant, &request_id, now) {
            Ok(_export) => complete(&mut ticket, now, None, "access gathered"),
            Err(e) => return pipeline_error(&e, "access"),
        },
        DsrRequestKind::Portability => match pipeline.portability(&tenant, &request_id, now) {
            Ok((_export, handle)) => complete(&mut ticket, now, handle, "portability export ready"),
            Err(e) => return pipeline_error(&e, "portability"),
        },
        DsrRequestKind::Rectification | DsrRequestKind::Erasure => {
            // Destructive arms: gate on the Worker-trusted MFA freshness header.
            if !mfa_fresh(&headers) {
                ticket.mfa_required = true;
                transition(
                    &mut ticket,
                    now,
                    TicketStatus::Pending,
                    "awaiting MFA step-up",
                );
            } else {
                match run_destructive(
                    pipeline.as_ref(),
                    action,
                    &tenant,
                    &request_id,
                    &body,
                    &mut ticket,
                    now,
                ) {
                    // Ticket mutated in place (completed / no-op) → fall through to
                    // the shared insert + 200 OK below.
                    Destructive::Continue => {}
                    // Rejected: the ticket carries a durable Rejected disposition
                    // that MUST be persisted (compliance record) BEFORE the 4xx is
                    // surfaced — the shared insert below is skipped on this return.
                    Destructive::RejectPersist(resp) => {
                        if let Err(e) = state.tickets.insert(&ticket) {
                            tracing::error!(
                                error = %e,
                                request_id = %request_id,
                                "dsr/submit: rejected ticket insert failed",
                            );
                            return (StatusCode::INTERNAL_SERVER_ERROR, "ticket persist failed")
                                .into_response();
                        }
                        return resp;
                    }
                    // Pipeline fault → fail-CLOSED: return the error WITHOUT persisting.
                    Destructive::Abort(resp) => return resp,
                }
            }
        }
        DsrRequestKind::Restriction | DsrRequestKind::Objection => {
            // Policy-only: durably recorded for operator disposition (no auto op).
            transition(
                &mut ticket,
                now,
                TicketStatus::Pending,
                "recorded for operator review",
            );
        }
        // `DsrRequestKind` is `#[non_exhaustive]`.
        _ => return (StatusCode::BAD_REQUEST, "unsupported DSR action").into_response(),
    }

    if let Err(e) = state.tickets.insert(&ticket) {
        tracing::error!(error = %e, request_id = %request_id, "dsr/submit: ticket insert failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "ticket persist failed").into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "request_id": request_id,
            "action": action.as_str(),
            "jurisdiction": jurisdiction.as_str(),
            "sla_deadline": ms_to_iso8601(clamp_i64(sla_deadline_ms)),
            "jwt_receipt": receipt,
        })),
    )
        .into_response()
}

/// Outcome of an inline destructive arm (the ticket is mutated in place).
enum Destructive {
    /// Completed / no-op — the caller persists the ticket and returns 200 OK.
    Continue,
    /// Rejected — the caller PERSISTS the (Rejected) ticket, then returns this 4xx.
    RejectPersist(Response),
    /// Pipeline fault — the caller returns this WITHOUT persisting (fail-CLOSED).
    Abort(Response),
}

/// Execute a destructive arm inline (MFA already fresh). The returned
/// [`Destructive`] tells the caller whether to persist the mutated ticket and
/// which response to surface.
fn run_destructive(
    pipeline: &dyn DsrPipeline,
    action: DsrRequestKind,
    tenant: &str,
    request_id: &str,
    body: &DsrSubmitBody,
    ticket: &mut DsrTicket,
    now: u64,
) -> Destructive {
    transition(ticket, now, TicketStatus::InProgress, "executing");
    match action {
        DsrRequestKind::Erasure => match pipeline.erasure(tenant) {
            Ok(()) => {
                complete(ticket, now, None, "erasure requested");
                Destructive::Continue
            }
            Err(e) => Destructive::Abort(pipeline_error(&e, "erasure")),
        },
        DsrRequestKind::Rectification => {
            let email = body
                .rectification
                .as_ref()
                .and_then(|r| r.email.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(email) = email else {
                // No live-rectifiable field supplied → record as completed no-op
                // (only the contact email is correctable in the live pipeline).
                complete(ticket, now, None, "no live-rectifiable field supplied");
                return Destructive::Continue;
            };
            match pipeline.rectification(tenant, request_id, email, now) {
                Ok(Ok(())) => {
                    complete(ticket, now, None, "rectification applied");
                    Destructive::Continue
                }
                Ok(Err(reject)) => {
                    reject_ticket(ticket, now, &reject);
                    // Rejected ticket must be persisted (durable disposition) by the
                    // caller BEFORE the 4xx is surfaced.
                    Destructive::RejectPersist(
                        (StatusCode::UNPROCESSABLE_ENTITY, reject).into_response(),
                    )
                }
                Err(e) => Destructive::Abort(pipeline_error(&e, "rectification")),
            }
        }
        _ => Destructive::Abort((StatusCode::BAD_REQUEST, "not a destructive arm").into_response()),
    }
}

/// `GET /v1/privacy/dsr/{request_id}/status`
async fn handle_status(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    match state.tickets.get(&tenant, &request_id) {
        // Constant-time confidentiality: a miss AND a cross-tenant lookup both
        // 404 — never disclose whether a request_id exists for another tenant.
        Ok(Some(ticket)) => (StatusCode::OK, Json(ticket.detail_json())).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dsr/status: lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "status lookup failed").into_response()
        }
    }
}

/// `GET /v1/privacy/dsr`
async fn handle_list(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    match state.tickets.list(&tenant, limit) {
        Ok(items) => (
            StatusCode::OK,
            Json(json!({
                "items": items.iter().map(DsrTicket::summary_json).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dsr/list: lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "list failed").into_response()
        }
    }
}

/// `POST /v1/privacy/dsr/{request_id}/verify-mfa`
async fn handle_verify_mfa(
    State(state): State<PrivacyDsrRouteState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
    _body: Option<Json<VerifyMfaBody>>,
) -> Response {
    let tenant = match authed_tenant(&state, &headers).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    let mut ticket = match state.tickets.get(&tenant, &request_id) {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "dsr/verify-mfa: lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    // Already terminal → idempotent detail (a repeat verify is a no-op).
    if matches!(
        ticket.status,
        TicketStatus::Completed | TicketStatus::Rejected
    ) {
        return (StatusCode::OK, Json(ticket.detail_json())).into_response();
    }
    // Fail-CLOSED: the destructive op runs only under a fresh Worker-trusted
    // MFA step-up.
    if !mfa_fresh(&headers) {
        return (StatusCode::UNAUTHORIZED, "MFA step-up required").into_response();
    }
    let Some(pipeline) = state.pipeline.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "DSR pipeline not configured",
        )
            .into_response();
    };
    let now = now_ms();
    ticket.mfa_required = false;
    let body = DsrSubmitBody {
        reason: ticket.reason.clone(),
        rectification: None,
    };
    let outcome = run_destructive(
        pipeline.as_ref(),
        ticket.action,
        &tenant,
        &request_id,
        &body,
        &mut ticket,
        now,
    );
    // Persist the transition regardless of the op outcome (durable evidence): the
    // ticket already exists (Pending), so this is an UPDATE — a Rejected or faulted
    // disposition is recorded either way.
    if let Err(e) = state.tickets.update(&ticket) {
        tracing::error!(error = %e, request_id = %request_id, "dsr/verify-mfa: ticket update failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "ticket persist failed").into_response();
    }
    match outcome {
        Destructive::Continue => (StatusCode::OK, Json(ticket.detail_json())).into_response(),
        Destructive::RejectPersist(err_resp) | Destructive::Abort(err_resp) => err_resp,
    }
}

// ─── Auth + helpers ────────────────────────────────────────────────────────────

/// Resolve the authenticated tenant (fail-CLOSED) and enforce the Clerk-session
/// + PAT-backstop gates. `Err(resp)` ⇒ the caller returns that response.
async fn authed_tenant(
    state: &PrivacyDsrRouteState,
    headers: &HeaderMap,
) -> Result<String, Response> {
    let tenant = tenant(headers).map_err(|()| {
        (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
    })?;
    // Clerk-session ONLY: a data-plane cache PAT must never drive a DSR request
    // (mirrors `/v1/customer/account/delete`).
    if principal(headers) != CLERK_TOKEN_PREFIX {
        return Err((
            StatusCode::FORBIDDEN,
            "data-subject-rights requests require a dashboard (Clerk) session",
        )
            .into_response());
    }
    // Native PAT possession backstop (skipped for Clerk callers + dev/CI).
    if let Some(gate) = state.pat_gate.as_ref() {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        gate.verify(&tenant, bearer).await?;
    }
    Ok(tenant)
}

/// Read the authenticated `x-corelink-tenant-id`, fail-CLOSED (mirrors
/// `routes/customer.rs::tenant`).
fn tenant(headers: &HeaderMap) -> Result<String, ()> {
    let raw = headers
        .get("x-corelink-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() || TENANT_SENTINELS.contains(&raw) {
        return Err(());
    }
    Ok(raw.to_owned())
}

/// Read the Worker-set `x-corelink-token-prefix` (fail-CLOSED to `_unknown`).
fn principal(headers: &HeaderMap) -> String {
    headers
        .get("x-corelink-token-prefix")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "_unknown".to_owned())
}

/// Whether the Worker-trusted MFA freshness marker is set (`x-corelink-mfa-
/// verified: 1`). The header is in the Worker strip list, so a client can never
/// forge it.
fn mfa_fresh(headers: &HeaderMap) -> bool {
    headers
        .get(MFA_VERIFIED_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        == Some("1")
}

/// Resolve the SLA jurisdiction from the optional Worker header; default GDPR.
fn jurisdiction_from(headers: &HeaderMap) -> DsrJurisdiction {
    headers
        .get(JURISDICTION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .map(parse_jurisdiction)
        .unwrap_or(DsrJurisdiction::Gdpr)
}

fn parse_jurisdiction(s: &str) -> DsrJurisdiction {
    match s.to_ascii_lowercase().as_str() {
        "lgpd" => DsrJurisdiction::Lgpd,
        "ccpa" => DsrJurisdiction::Ccpa,
        // Default (incl. "gdpr") is GDPR — the strictest calendar-month SLA.
        _ => DsrJurisdiction::Gdpr,
    }
}

fn parse_action(s: &str) -> Option<DsrRequestKind> {
    Some(match s {
        "access" => DsrRequestKind::Access,
        "portability" => DsrRequestKind::Portability,
        "rectification" => DsrRequestKind::Rectification,
        "erasure" => DsrRequestKind::Erasure,
        "restriction" => DsrRequestKind::Restriction,
        "objection" => DsrRequestKind::Objection,
        _ => return None,
    })
}

fn parse_timeline_event(v: &Value) -> Option<TimelineEvent> {
    let to = v.get("to").and_then(Value::as_str)?;
    Some(TimelineEvent {
        // The persisted `at` is ISO; the in-memory `at_ms` is only used for
        // re-serialization, so a parse miss falls back to 0 (display-only).
        at_ms: 0,
        from: v.get("from").and_then(Value::as_str).map(str_to_status),
        to: str_to_status(to),
        note: v.get("note").and_then(Value::as_str).map(str::to_owned),
    })
}

fn str_to_status(s: &str) -> TicketStatus {
    TicketStatus::parse(s)
}

/// Transition a ticket to `to`, appending a timeline row.
fn transition(ticket: &mut DsrTicket, now: u64, to: TicketStatus, note: &str) {
    let from = ticket.status;
    ticket.status = to;
    ticket.updated_at_ms = now;
    ticket.timeline.push(TimelineEvent {
        at_ms: now,
        from: Some(from),
        to,
        note: Some(note.to_owned()),
    });
}

/// Mark a ticket completed (+ optional export handle).
fn complete(ticket: &mut DsrTicket, now: u64, download: Option<String>, note: &str) {
    ticket.mfa_required = false;
    if let Some(url) = download {
        ticket.data_download_url = Some(url);
    }
    transition(ticket, now, TicketStatus::Completed, note);
}

/// Mark a ticket rejected with a (non-PII) reason note.
fn reject_ticket(ticket: &mut DsrTicket, now: u64, note: &str) {
    transition(ticket, now, TicketStatus::Rejected, note);
}

/// Map a pipeline engine fault to a fail-CLOSED 500 (no partial success).
fn pipeline_error(err: &str, surface: &str) -> Response {
    tracing::error!(error = %err, surface = %surface, "dsr: live pipeline error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("{surface} failed"),
    )
        .into_response()
}

/// HS256 proof-of-submission receipt (compact JWT). Real + verifiable; the RS256
/// KMS binding is the deferred production hardening (`corelink-dsr`).
fn issue_receipt(
    key: &[u8],
    request_id: &str,
    action: DsrRequestKind,
    jurisdiction: DsrJurisdiction,
    sla_deadline_ms: u64,
    submitted_at_ms: u64,
    tenant_id: &str,
) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = json!({ "alg": "HS256", "typ": "JWT" });
    let iat = submitted_at_ms / 1000;
    let exp = iat.saturating_add(90 * 86_400);
    // Hashed tenant identifier — never the raw tenant_id (CTRL-PRIV-014).
    let tenant_id_hash = hex::encode(Sha256::digest(tenant_id.as_bytes()));
    let claims = json!({
        "iss": "corelink.dsr",
        "request_id": request_id,
        "action": action.as_str(),
        "jurisdiction": jurisdiction.as_str(),
        "sla_deadline": ms_to_iso8601(clamp_i64(sla_deadline_ms)),
        "jti": request_id,
        "iat": iat,
        "exp": exp,
        "tenant_id_hash": tenant_id_hash,
    });
    let h = b64.encode(serde_json::to_vec(&header).unwrap_or_default());
    let c = b64.encode(serde_json::to_vec(&claims).unwrap_or_default());
    let signing_input = format!("{h}.{c}");
    // HMAC accepts any key length, so `new_from_slice` never errs here.
    let mut mac = match <Hmac<Sha256> as KeyInit>::new_from_slice(key) {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    mac.update(signing_input.as_bytes());
    let sig = b64.encode(mac.finalize().into_bytes());
    format!("{signing_input}.{sig}")
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

/// Clamp a Unix-ms value to `i64` (D1 stores INTEGER; the `ms_to_iso8601` helper
/// + the params take `i64`).
fn clamp_i64(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    const TENANT_A: &str = "00000000-0000-7000-8000-00000000000a";
    const TENANT_B: &str = "00000000-0000-7000-8000-00000000000b";

    /// A fake pipeline that records calls and returns canned successes.
    #[derive(Debug, Default)]
    struct FakePipeline {
        calls: std::sync::Mutex<Vec<String>>,
        rectify_reject: bool,
        access_err: bool,
    }

    impl DsrPipeline for FakePipeline {
        fn access(&self, tenant: &str, _dsr: &str, _now: u64) -> Result<Value, String> {
            self.calls.lock().unwrap().push(format!("access:{tenant}"));
            if self.access_err {
                return Err("boom".to_owned());
            }
            Ok(json!({ "tenant": tenant, "tables": [] }))
        }
        fn portability(
            &self,
            tenant: &str,
            _dsr: &str,
            _now: u64,
        ) -> Result<(Value, Option<String>), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("portability:{tenant}"));
            Ok((
                json!({ "tenant": tenant }),
                Some("dsr_exports/x.json".to_owned()),
            ))
        }
        fn rectification(
            &self,
            tenant: &str,
            _dsr: &str,
            _email: &str,
            _now: u64,
        ) -> Result<Result<(), String>, String> {
            self.calls.lock().unwrap().push(format!("rectify:{tenant}"));
            if self.rectify_reject {
                return Ok(Err("not a valid email".to_owned()));
            }
            Ok(Ok(()))
        }
        fn erasure(&self, tenant: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("erasure:{tenant}"));
            Ok(())
        }
    }

    fn state_with(pipeline: FakePipeline) -> (PrivacyDsrRouteState, Arc<InMemoryDsrTicketStore>) {
        let tickets = Arc::new(InMemoryDsrTicketStore::new());
        let state = PrivacyDsrRouteState {
            pipeline: Some(Arc::new(pipeline)),
            tickets: tickets.clone(),
            pat_gate: None,
            receipt_key: Arc::new(b"test-key-test-key-test-key-test-key".to_vec()),
        };
        (state, tickets)
    }

    fn clerk_headers(tenant: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-corelink-tenant-id", tenant.parse().unwrap());
        h.insert("x-corelink-token-prefix", "clerk".parse().unwrap());
        h
    }

    fn with_mfa(mut h: HeaderMap) -> HeaderMap {
        h.insert(MFA_VERIFIED_HEADER, "1".parse().unwrap());
        h
    }

    fn status_of(resp: &Response) -> StatusCode {
        resp.status()
    }

    #[tokio::test]
    async fn access_drives_pipeline_and_completes() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let listed = tickets.list(TENANT_A, 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, TicketStatus::Completed);
        assert_eq!(listed[0].action, DsrRequestKind::Access);
    }

    #[tokio::test]
    async fn portability_sets_download_handle() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Portability,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Completed);
        assert_eq!(t.data_download_url.as_deref(), Some("dsr_exports/x.json"));
    }

    #[tokio::test]
    async fn erasure_without_mfa_is_pending() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Erasure,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Pending);
        assert!(t.mfa_required);
    }

    #[tokio::test]
    async fn erasure_with_mfa_drives_pipeline() {
        let (state, tickets) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            with_mfa(clerk_headers(TENANT_A)),
            DsrRequestKind::Erasure,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Completed);
    }

    #[tokio::test]
    async fn rectification_reject_persists_rejected_ticket() {
        let pipe = FakePipeline {
            rectify_reject: true,
            ..Default::default()
        };
        let (state, tickets) = state_with(pipe);
        let body = DsrSubmitBody {
            reason: None,
            rectification: Some(RectificationBody {
                email: Some("bad".to_owned()),
            }),
        };
        let resp = submit(
            state,
            with_mfa(clerk_headers(TENANT_A)),
            DsrRequestKind::Rectification,
            body,
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNPROCESSABLE_ENTITY);
        let t = &tickets.list(TENANT_A, 10).unwrap()[0];
        assert_eq!(t.status, TicketStatus::Rejected);
    }

    #[tokio::test]
    async fn access_pipeline_error_fails_closed_no_ticket() {
        let pipe = FakePipeline {
            access_err: true,
            ..Default::default()
        };
        let (state, tickets) = state_with(pipe);
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::INTERNAL_SERVER_ERROR);
        // Fail-CLOSED: no ticket persisted on a gather fault.
        assert!(tickets.list(TENANT_A, 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_tenant_is_unauthorized() {
        let (state, _t) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            HeaderMap::new(),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn pat_caller_is_forbidden() {
        let (state, _t) = state_with(FakePipeline::default());
        let mut h = HeaderMap::new();
        h.insert("x-corelink-tenant-id", TENANT_A.parse().unwrap());
        h.insert("x-corelink-token-prefix", "corelink_test".parse().unwrap());
        let resp = submit(state, h, DsrRequestKind::Access, DsrSubmitBody::default()).await;
        assert_eq!(status_of(&resp), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn sentinel_tenant_is_unauthorized() {
        let (state, _t) = state_with(FakePipeline::default());
        let resp = submit(
            state,
            clerk_headers("_anonymous"),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_cross_tenant_is_not_found() {
        let (state, tickets) = state_with(FakePipeline::default());
        // Tenant A submits; tenant B must not see it.
        let _ = submit(
            state.clone(),
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        let rid = tickets.list(TENANT_A, 10).unwrap()[0].request_id.clone();
        let resp = handle_status(State(state), clerk_headers(TENANT_B), Path(rid)).await;
        assert_eq!(status_of(&resp), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn status_same_tenant_returns_detail() {
        let (state, tickets) = state_with(FakePipeline::default());
        let _ = submit(
            state.clone(),
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        let rid = tickets.list(TENANT_A, 10).unwrap()[0].request_id.clone();
        let resp = handle_status(State(state), clerk_headers(TENANT_A), Path(rid)).await;
        assert_eq!(status_of(&resp), StatusCode::OK);
    }

    #[tokio::test]
    async fn verify_mfa_completes_pending_erasure() {
        let (state, tickets) = state_with(FakePipeline::default());
        // Submit erasure WITHOUT mfa → pending.
        let _ = submit(
            state.clone(),
            clerk_headers(TENANT_A),
            DsrRequestKind::Erasure,
            DsrSubmitBody::default(),
        )
        .await;
        let rid = tickets.list(TENANT_A, 10).unwrap()[0].request_id.clone();
        // verify-mfa WITHOUT the freshness header → 401.
        let resp = handle_verify_mfa(
            State(state.clone()),
            clerk_headers(TENANT_A),
            Path(rid.clone()),
            None,
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::UNAUTHORIZED);
        // verify-mfa WITH the freshness header → completes.
        let resp = handle_verify_mfa(
            State(state.clone()),
            with_mfa(clerk_headers(TENANT_A)),
            Path(rid.clone()),
            None,
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::OK);
        let t = tickets.get(TENANT_A, &rid).unwrap().unwrap();
        assert_eq!(t.status, TicketStatus::Completed);
    }

    #[tokio::test]
    async fn rate_limit_after_daily_cap() {
        let (state, _t) = state_with(FakePipeline::default());
        for _ in 0..DSR_DAILY_LIMIT {
            let resp = submit(
                state.clone(),
                clerk_headers(TENANT_A),
                DsrRequestKind::Access,
                DsrSubmitBody::default(),
            )
            .await;
            assert_eq!(status_of(&resp), StatusCode::OK);
        }
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn pipeline_unconfigured_fails_closed() {
        let state = PrivacyDsrRouteState {
            pipeline: None,
            tickets: Arc::new(InMemoryDsrTicketStore::new()),
            pat_gate: None,
            receipt_key: Arc::new(b"test-key-test-key-test-key-test-key".to_vec()),
        };
        let resp = submit(
            state,
            clerk_headers(TENANT_A),
            DsrRequestKind::Access,
            DsrSubmitBody::default(),
        )
        .await;
        assert_eq!(status_of(&resp), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn receipt_is_three_part_jwt() {
        let r = issue_receipt(
            b"k",
            "rid",
            DsrRequestKind::Access,
            DsrJurisdiction::Gdpr,
            1000,
            0,
            TENANT_A,
        );
        assert_eq!(r.split('.').count(), 3, "compact JWS = header.claims.sig");
    }
}
