// Tests are split by route family so the customer route implementation and
// its test suites remain independently reviewable under the B-126 file-size ratchet.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::*;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use corelink_handler_customer::request::{
    ByokStatus, InvoiceRow, OverviewBilling, OverviewUsage, PatRow,
};
use corelink_handler_customer::{
    BillingResponse, CustomerKeysHandler, OverviewResponse, UsageResponse,
};
use corelink_ratelimit::FailingRateLimitAuditSink;
use tower::ServiceExt;

fn fixture() -> (CustomerRouteState, Arc<InMemoryCustomerHandler>) {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared: Arc<InMemoryCustomerHandler> = Arc::new(InMemoryCustomerHandler::new(audit, sli));
    let state = CustomerRouteState {
        overview: shared.clone(),
        usage: shared.clone(),
        billing: shared.clone(),
        keys: shared.clone(),
        team: shared.clone(),
        audit: shared.clone(),
        pat_gate: None,
        account_deletion: None,
        export: None,
        export_rate_limiter: crate::routes::customer_export::build_export_rate_limiter(),
        pat_issue_rate_limiter: build_pat_issue_rate_limiter(),
    };
    (state, shared)
}

fn fixture_with_observable_audit() -> (
    CustomerRouteState,
    Arc<InMemoryCustomerHandler>,
    Arc<InMemoryAuditSink>,
) {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared: Arc<InMemoryCustomerHandler> =
        Arc::new(InMemoryCustomerHandler::new(audit.clone(), sli));
    let state = CustomerRouteState {
        overview: shared.clone(),
        usage: shared.clone(),
        billing: shared.clone(),
        keys: shared.clone(),
        team: shared.clone(),
        audit: shared.clone(),
        pat_gate: None,
        account_deletion: None,
        export: None,
        export_rate_limiter: crate::routes::customer_export::build_export_rate_limiter(),
        pat_issue_rate_limiter: build_pat_issue_rate_limiter(),
    };
    (state, shared, audit)
}

#[path = "tests_account.rs"]
mod tests_account;
#[path = "tests_auth.rs"]
mod tests_auth;
#[path = "tests_basic.rs"]
mod tests_basic;
#[path = "tests_export.rs"]
mod tests_export;
#[path = "tests_keys.rs"]
mod tests_keys;
#[path = "tests_team.rs"]
mod tests_team;
