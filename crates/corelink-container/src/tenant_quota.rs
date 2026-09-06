//! Per-tenant monthly **$-ceiling** middleware (WP-FOUND-2 / G1) — the
//! economic fail-CLOSED spend cap mandated by RATIFIED **ADR-0068**
//! (`specs/03_architecture/adrs/ADR-0068-per-tenant-monthly-dollar-ceiling.md`).
//!
//! # Why — `$` is not `rate`
//!
//! The container already enforces a per-tenant **rate** limit
//! (`ratelimit_buckets` + the tier→RPS map; the in-route gate lives at
//! [`crate::routes::audit_analytics`]'s `rate_limit_check`). That bounds
//! **velocity** (req/s) — NOT cumulative **dollars**. A slow-but-steady
//! tenant stays under the rate limit yet can accrue unbounded monthly
//! cost (the real blast-radius risk for the hugit campaign on cheap
//! third-party infra). ADR-0068 closes that gap with a second,
//! orthogonal axis: a per-tenant monthly **cumulative-$** ceiling,
//! checked **before** a billable op is served.
//!
//! # Fail-CLOSED
//!
//! For a COST cap the correct trade-off is the opposite of an SLO
//! limiter: over the ceiling we protect the **business** (reject), not
//! availability (serve + eat cost). Concretely the gate fail-CLOSES on
//! every uncertain path:
//!
//! - over the ceiling → `402 Payment Required`;
//! - quota-store transport / decode error → `503 Service Unavailable`
//!   (we do NOT serve a billable op we could not cost-check);
//! - wall clock unavailable (`now_ms == 0`) → `503`.
//!
//! Only an explicit `Allow` (room under the ceiling, store reachable)
//! returns `None` and lets the request proceed.
//!
//! # Units — integer micro-dollars
//!
//! All amounts are signed `i64` **micro-dollars** (USD * 1_000_000), to
//! match the `tenant_quota` D1 table (migration 0066). Floating point is
//! never used for money. The default ceiling is [`DEFAULT_MONTHLY_BUDGET_USD_MICROS`]
//! (effectively-unlimited `$1,000,000/mo`; ADR-0068 reconciliation — see the const).
//!
//! # Wiring (the seam the lead registers)
//!
//! Production builds a [`QuotaGuard`] from an `Arc<dyn QuotaStore>`
//! ([`D1QuotaStore`] over [`crate::storage::d1_http::D1HttpClient`]) +
//! an `Arc<dyn WallClock>` ([`crate::wall_clock::SystemWallClock`]),
//! stashes it in each billable route's state, and calls
//! [`QuotaGuard::check`] at the TOP of the billable handler — alongside
//! the existing rate-limit gate — returning the response on `Some`.
//! Tests inject [`InMemoryQuotaStore`] + [`crate::wall_clock::InMemoryFakeWallClock`].

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::response::Response;

// B-007 — the canonical alert-dispatch primitive reused as the sink for the
// near-$-ceiling early-warning signal (see `emit_near_ceiling`). Pure-logic;
// no HTTPS egress ships yet (the real dispatcher + routing_key are B-008).
use corelink_slo::pagerduty::{
    InMemoryPagerDutyDispatcher, PagerDutyDispatcher, PagerDutyEvent, PagerDutyEventAction,
    PagerDutyServiceKey,
};

use crate::wall_clock::WallClock;

// B126-M2 REANCHOR MANIFEST (tenant_quota).
// Ordered include fragments below are the sole composition point; this keeps
// the module namespace/API and execution order unchanged while preventing
// recomposition into a god-file. Symbols moved: NEAR_CEILING_RUNBOOK_URL, NEAR_CEILING_ALERT_SINK_ENV, DEFAULT_MONTHLY_BUDGET_USD_MICROS, CYCLE_LENGTH_MS, COST_PER_OP_MICROS_ENV, DEFAULT_COST_PER_OP_MICROS, cost_per_op_micros, quota_guard_from_env, QuotaState, QuotaStore, DEFAULT_LEASE_OPS, LeasedQuotaStore, Lease, build_near_ceiling_event, QuotaGuard, InMemoryQuotaStore, D1QuotaStore.
include!("tenant_quota/b126_m2_impl_01.rs");
include!("tenant_quota/b126_m2_impl_02.rs");

#[cfg(test)]
mod b126_m2_reanchor {
    #[test]
    fn implementation_fragments_are_wired() {
        let _ = [
            super::B126_M2_IMPL_1_REANCHOR,
            super::B126_M2_IMPL_2_REANCHOR,
        ];
    }
}
