//! Production **D1-backed customer handler** (dashboard revival WP-3):
//! [`D1CustomerHandler`] implements all 6 `corelink-handler-customer`
//! traits over the live Cloudflare D1 database, replacing the
//! `InMemoryCustomerHandler` 404-stub so real tenants get real
//! dashboard data.
//!
//! # HONEST v1 contract
//!
//! Real data where a deployed table exists; explicit empty / zero /
//! `501 Not Implemented` where it doesn't. NEVER fabricated data.
//! Per-endpoint matrix (deployed schema in parentheses):
//!
//! | Endpoint            | Source of truth                                            |
//! |---------------------|------------------------------------------------------------|
//! | overview            | `tenant` (0023/0031/0056/0057) + `tenant_storage_state` (0008) `SUM(bytes_used)` / `MAX(bytes_quota)` + `tenant_billing` (0055) + `byok_envelope` (0030) existence; `tenant_name` = `tenant_id`; `recent_activity` = newest 8 `customer_audit_events` (0077) via [`D1CustomerHandler::recent_activity`] (BE-3), honestly empty for a new tenant |
//! | usage               | real `cas_bytes`/`quota_bytes` from `tenant_storage_state` + real `request_count` from `monthly_request_counts` (0071) via [`D1CustomerHandler::monthly_request_count`] (BE-1a); `reads`/`writes` = 0 + `daily` = `[]` (no per-op table) — only the CURRENT period retains bytes, an earlier period honestly reports 0 bytes |
//! | audit               | `customer_audit_events` (migration 0077): newest-first, tenant-scoped, bounded; rows written UNSKIPPABLE / fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER) by `keys create` (`pat.created`) + `team invite` (`team.invited`) |
//! | billing             | `tenant_billing` (0055) + `tier_selections` (0039/0062); status map FROZEN (see [`map_billing_status`]); `invoices` = `[]` (no invoice-history surface yet) |
//! | billing/portal      | `tenant_billing.stripe_customer_id` → Stripe billing-portal session; no customer id → 404 "no billing account" |
//! | keys list           | `pat` (0037/0054 + WP-2 columns `name`, `revoked_at_ms` — migration 0063, parallel PR); `last_used_at` = `None` (not tracked) |
//! | keys create         | `corelink_pat::mint` + D1 `INSERT`; scope map FROZEN (`["cache:read"]`→`read-only`, anything-with-write→`read-write`, `admin` NEVER grantable); token returned once, never logged |
//! | keys revoke         | `UPDATE pat SET revoked_at_ms=?` tenant-scoped; idempotent |
//! | team                | single synthesized **Owner** row from `tenant.clerk_user_id`, email `"—"` |
//! | team/invite         | INSERT a `team_member` row (status `invited`, pseudonymized `email_hash` plus one-time SHA-256 invitation-token digest, migrations 0074/0108); token plaintext is returned once and redemption is edge-gated |
//!
//! # The sync↔async bridge
//!
//! The 6 handler traits are synchronous (shared shape with the wasm32
//! CF-Worker target); on the native container D1 is reachable only via
//! the **async** [`D1HttpClient`]. Exactly like
//! [`crate::billing_d1_http::D1HttpBillingWriter::run`], each sync call
//! drives the async client through `tokio::task::block_in_place` +
//! `Handle::current().block_on(…)` — valid because the native server is
//! `#[tokio::main]` (multi-thread). The bridge lives behind the
//! [`CustomerD1`] seam so unit tests exercise the handler hermetically
//! with a mock row source (mirroring the billing writer's test layout).
//! The Stripe portal call (blocking `reqwest`) crosses the same bridge
//! behind [`PortalSessions`].
//!
//! # SECURITY / fail-CLOSED
//!
//! - **Audit-before-lookup + SLI on every return path** per the trait
//!   docs (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` +
//!   `INV-HANDLER-SLI-EMIT-ENTRY`); audit emit failure aborts with
//!   [`CustomerHandlerError::AuditFailed`] BEFORE any D1 round-trip.
//! - **Fail-CLOSED on D1 transport error:** any transport / non-2xx /
//!   decode error maps to [`CustomerHandlerError::Internal`] (→ 500),
//!   never to fabricated empty data.
//! - **Parameterised SQL only** — every dynamic value is a positional
//!   bind; the tenant scope rides in `WHERE tenant_id = ?` on every
//!   statement (INV-TENANT-ISOLATION).
//! - **Token returned once, never logged**; the PAT plaintext only
//!   exists in the [`KeyCreateResponse`].
//! - **Wall clock, never the request timestamp:** the routes pass
//!   `at_unix_ms = 0` (see `routes/customer.rs::now_ms`), so all
//!   timestamps come from the injected [`WallClock`].
//!
//! Re-anchor ledger (all included units intentionally remain in this module's
//! namespace, preserving private visibility and the public API):
//! - `customer_d1_seams.rs`: `CustomerD1`, `D1HttpCustomerDb`,
//!   `PortalSessions`, `StripePortalSessions`, and tracing collaborators.
//! - `customer_d1_maps_calendar.rs`: frozen scope/status maps, calendar, and
//!   row-value helpers (`map_billing_status`, `ms_to_iso8601`, `period_from_ms`).
//! - `customer_d1_handler_state.rs`: `D1CustomerHandler`, `UsageRollup`, and
//!   tenant/storage/billing/BYOK/activity/request-count readers.
//! - `customer_d1_overview_usage.rs`: `CustomerOverviewHandler` and
//!   `CustomerUsageHandler` implementations.
//! - `customer_d1_billing_keys.rs`: billing/portal and
//!   `CustomerKeysHandler` implementations.
//! - `customer_d1_team_audit.rs`: `CustomerTeamHandler` and
//!   `CustomerAuditHandler` implementations.
//! - `customer_d1_byok_config.rs`: `TenantByokConfig`, BYOK enums, row seam,
//!   and `D1ByokConfigReader`.
//! - `customer_d1_byok_writer.rs`: `ByokActivation` and
//!   `D1ByokConfigWriter` state-machine writer.
//! - `customer_d1_tests_*.rs`: original hermetic test items, split at existing
//!   section boundaries only.

use std::sync::Arc;
use std::time::Duration;

use corelink_handler_customer::observer::Sli;
use corelink_handler_customer::request::{
    canonical_invite_role, ByokStatus, CustomerAuditEventRow, DailyUsageBucket, InvoiceRow,
    OverviewBilling, OverviewUsage, PatRow, TeamMemberRow,
};
use corelink_handler_customer::{
    AuditEvent, AuditEventKind, AuditQueryRequest, AuditQueryResponse, AuditSink, BillingRequest,
    BillingResponse, CustomerAuditHandler, CustomerBillingHandler, CustomerHandlerError,
    CustomerKeysHandler, CustomerOverviewHandler, CustomerTeamHandler, CustomerUsageHandler,
    KeyCreateRequest, KeyCreateResponse, KeyRevokeRequest, KeyRevokeResponse, KeysListRequest,
    KeysListResponse, OverviewRequest, OverviewResponse, PortalRequest, PortalResponse,
    SliObservation, SliObserver, TeamInviteRequest, TeamInviteResponse, TeamListRequest,
    TeamListResponse, TeamRemoveRequest, TeamRemoveResponse, UsageRequest, UsageResponse,
};
use corelink_pat::{
    mint::mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_FIND,
    SCOPE_CACHE_R, SCOPE_CACHE_RW,
};
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::storage::d1_http::{D1HttpClient, D1Row};
use crate::wall_clock::WallClock;

include!("customer_d1_seams.rs");
include!("customer_d1_maps_calendar.rs");
include!("customer_d1_handler_state.rs");
include!("customer_d1_overview_usage.rs");
include!("customer_d1_billing_keys.rs");
include!("customer_d1_team_audit.rs");
include!("customer_d1_byok_config.rs");
include!("customer_d1_byok_writer.rs");

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::sync::Mutex;

    use corelink_handler_customer::{InMemoryAuditSink, InMemorySliObserver};

    use super::*;
    use crate::wall_clock::InMemoryFakeWallClock;

    /// 2023-11-14T22:13:20Z — a fixed, verifiable instant.
    const NOW_MS: u64 = 1_700_000_000_000;
    const TENANT: &str = "0192f0c1-2345-7890-abcd-ef0123456789";
    const OTHER_TENANT: &str = "0192f0c1-9999-7890-abcd-ef0123456789";

    include!("customer_d1_tests_support.rs");
    include!("customer_d1_tests_cross_tenant.rs");
    include!("customer_d1_tests_maps_calendar.rs");
    include!("customer_d1_tests_overview_usage.rs");
    include!("customer_d1_tests_billing_keys.rs");
    include!("customer_d1_tests_billing_keys_part2.rs");
    include!("customer_d1_tests_team_audit_misc.rs");
    include!("customer_d1_tests_team_audit_misc_part2.rs");
    include!("customer_d1_tests_byok_read.rs");
    include!("customer_d1_tests_byok_write.rs");
}
