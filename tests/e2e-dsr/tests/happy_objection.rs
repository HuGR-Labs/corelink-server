//! Happy-path 6 — Objection (GDPR Art. 21 objection to processing).
//!
//! Customer objects to a specific processing purpose (e.g.
//! `marketing_analytics`) → API accepts → policy ledger records the
//! objection. Matching pipelines (same tenant + purpose) now pause;
//! unrelated purposes continue to process for the same tenant.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{DsrDecision, DsrEndpoint, DsrJurisdiction, DsrRequestKind};
use e2e_dsr::{canonical_dsr_for, make_test_tenant, setup_test_env, PolicyDecision};

#[test]
fn objection_pauses_matching_purpose_only() {
    let env = setup_test_env();
    let tenant = make_test_tenant("objection-tenant");

    let request = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Objection,
        DsrJurisdiction::Gdpr,
        env.now_ms,
    );
    let decision = match env.dsr.submit(&request) {
        Ok(d) => d,
        Err(e) => panic!("dsr submit failed: {e:?}"),
    };
    assert!(matches!(decision, DsrDecision::RequestAccepted { .. }));

    // Pre-objection: both purposes Allowed.
    assert_eq!(
        env.policy.evaluate_purpose(tenant.tenant_id, "marketing_analytics"),
        PolicyDecision::Allowed
    );
    assert_eq!(
        env.policy.evaluate_purpose(tenant.tenant_id, "aggregate_telemetry"),
        PolicyDecision::Allowed
    );

    // Record objection for marketing_analytics only.
    env.policy
        .record_objection(tenant.tenant_id, "marketing_analytics");

    match env
        .policy
        .evaluate_purpose(tenant.tenant_id, "marketing_analytics")
    {
        PolicyDecision::Objected { purpose } => {
            assert_eq!(purpose, "marketing_analytics");
        }
        other => panic!("expected Objected, got {other:?}"),
    }
    // Other purposes for the same tenant: still Allowed.
    assert_eq!(
        env.policy.evaluate_purpose(tenant.tenant_id, "aggregate_telemetry"),
        PolicyDecision::Allowed
    );

    // Snapshot the canonical objections list.
    let objections = env.policy.snapshot_objections(tenant.tenant_id);
    assert_eq!(objections.len(), 1);
    assert_eq!(objections.first().map(String::as_str), Some("marketing_analytics"));
}
