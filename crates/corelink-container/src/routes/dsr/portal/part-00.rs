// Customer-facing DSR self-service portal — `/v1/privacy/dsr/*`.
//
// This is the CUSTOMER intake surface for the data-subject rights the
// admin-ui `apps/admin-ui/src/lib/dsr-client.ts` already posts to. It mounts
// the six rights + status + MFA verify, mirrors the [`crate::routes::customer`]
// auth model (tenant derived EXCLUSIVELY from the Worker-injected
// `x-corelink-tenant-id`, PAT possession backstop, Clerk-session gate), and
// DRIVES THE EXISTING LIVE D1 PIPELINE — it is NOT a second erasure/gather
// engine:
//
// - **Access** (Art.15) → [`super::super::access::run_access`] (live D1 gather).
// - **Portability** (Art.20) → [`super::super::access::run_portability`] (live gather
//   + best-effort signed R2 export).
// - **Rectification** (Art.16) → [`super::super::access::run_rectification`] (live,
//   allowlisted-field UPDATE, hashed value).
// - **Erasure** (Art.17) → the SAME in-process, D1-backed erasure worker the
//   Clerk `user.deleted` + `/v1/customer/account/delete` paths use, via the
//   [`crate::routes::customer::AccountDeletionRequester`] seam (writes the
//   `dsr_requested` legitimacy anchor, then drives the worker).
// - **Restriction** (Art.18) / **Objection** (Art.21) are policy-only: the
//   ticket is durably recorded `pending` for the operator's manual disposition
//   (there is no automatic data mutation for these arms).
//
// ## Auth (mirrors `routes/customer.rs`)
//
// Every route is a Clerk-session dashboard surface: the Worker edge-verifies
// the Clerk JWT, resolves the tenant, strips the bearer, and forwards
// `x-corelink-tenant-id` + `x-corelink-token-prefix: clerk`. A missing/sentinel
// tenant → 401 (fail-CLOSED); a cache PAT caller (`token-prefix != clerk`) →
// 403 (a data-plane credential must never drive a data-subject-rights request,
// matching the `/v1/customer/account/delete` posture). The native PAT
// possession backstop runs first and is skipped for Clerk callers.
//
// ## MFA step-up (CTRL-AUTH-010)
//
// The destructive arms (Erasure + Rectification) are gated on the
// Worker-trusted freshness header `x-corelink-mfa-verified`. The Worker is the
// SOLE setter (the header is in the strip list, so a client can never forge
// it); it stamps `1` on the privacy plane for an edge-verified Clerk session.
// When present the destructive op runs inline; when ABSENT the ticket is
// recorded `pending` with `mfa_required=true` and NO data is mutated — the
// customer completes it via `POST /v1/privacy/dsr/{request_id}/verify-mfa`.
// The cryptographic WebAuthn step-up binding is the deferred edge wiring (per
// `corelink-dsr`); tightening the header condition to require a real step-up
// assertion is a Worker-side change, not a container rewire.
//
// ## Isolation, fail-CLOSED, rate-limited, audited
//
// The ticket store is tenant-leftmost `(tenant_id, request_id)` so a
// cross-tenant status read is structurally impossible; a miss and a
// cross-tenant lookup are indistinguishable (constant-time 404). Every
// data-mutating pipeline call already emits its `audit_outbox` envelope BEFORE
// the mutation (ADR-S11-002); the durable ticket is the intake evidence.
// Submissions are capped at [`DSR_DAILY_LIMIT`]/day per tenant (LGPD Art.20).

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
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
#[derive(Clone, Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
#[derive(Clone, Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
#[derive(Debug, Default)]
#[non_exhaustive]
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
#[non_exhaustive]
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
        super::super::d1util::d1_query_blocking(
            &self.d1,
            Self::upsert_sql(),
            Self::upsert_params(ticket),
        )
        .map(|_| ())
    }

    fn get(&self, tenant_id: &str, request_id: &str) -> Result<Option<DsrTicket>, String> {
        let rows = super::super::d1util::d1_query_blocking(
            &self.d1,
            "SELECT * FROM dsr_tickets WHERE tenant_id = ?1 AND request_id = ?2 LIMIT 1",
            vec![json!(tenant_id), json!(request_id)],
        )?;
        Ok(rows.first().and_then(Self::row_to_ticket))
    }

    fn list(&self, tenant_id: &str, limit: usize) -> Result<Vec<DsrTicket>, String> {
        let rows = super::super::d1util::d1_query_blocking(
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
        let rows = super::super::d1util::d1_query_blocking(
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
