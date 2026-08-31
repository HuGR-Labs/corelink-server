//! The property: **an active `$`-ceiling gate with no cost-attribution
//! tenant fails CLOSED (503) — it never silently skips the charge.**
//!
//! This is the REV-S3 regression, and it is a money property. The earlier
//! code treated "no tenant id" as "nothing to charge" and let the request
//! through unmetered, which turns a Worker header-injection regression (or
//! any direct-to-container access) into an unbounded `$`-ceiling bypass that
//! shows up as revenue loss, not as an error.
//!
//! The fixture is built so the 503 can only mean one thing: `router_with_
//! quota` charges $1/op against a FRESH tenant, comfortably under the
//! tripwire, so a tenant that WAS attributed would be admitted. A 402
//! (over-ceiling) is therefore excluded by construction, and the only path
//! that produces 503 is the missing-label fail-closed arm.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use axum::http::StatusCode;
use tower::ServiceExt; // for `.oneshot`

use super::tests_support::{
    get, router_with_quota, test_key, EmptyLookup, FakeKv, FakeMap, StubCas, SCOPE_RW,
};
use super::{CasReadHandler, CasWriteHandler, PatVerifier};

#[tokio::test]
async fn quota_gate_without_tenant_header_fails_closed_not_skipped() {
    // REV-S3 regression: a billable op (scope-valid GET) that reaches an
    // ACTIVE quota gate with NO `x-corelink-tenant-id` and no PAT-resolved
    // tenant must FAIL CLOSED (503) — the prior code silently skipped the
    // charge (fail-OPEN), an unmetered $-ceiling bypass.
    let cas: Arc<StubCas> = Arc::new(StubCas::default());
    let verifier = Arc::new(PatVerifier::new(Arc::new(EmptyLookup), test_key()));
    let app = router_with_quota(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(FakeKv::default()),
        verifier,
    );
    let resp = app
        .oneshot(get(
            "/pip/t/simple/requests/",
            Some("corelink_whatever"),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "no-tenant billable op must fail closed (503), not skip the charge"
    );
}
