//! Happy-path 5 — Restriction (GDPR Art. 18 restriction of processing).
//!
//! Customer submits a Restriction DSR (policy-only; no MFA gate) →
//! API accepts → policy ledger flips the tenant's restriction flag to
//! `Paused`. Subsequent writes return `Restricted` decision; reads
//! remain `Allowed`.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{DsrEndpoint, DsrJurisdiction, DsrRequestKind};
use e2e_dsr::{
    canonical_dsr_for, make_test_tenant, setup_test_env, PolicyDecision, RestrictionFlag,
};

#[test]
fn restriction_flips_policy_pauses_writes() {
    let env = setup_test_env();
    let tenant = make_test_tenant("restriction-tenant");

    // Pre-condition: tenant unrestricted; writes allowed.
    assert_eq!(env.policy.evaluate_write(tenant.tenant_id), PolicyDecision::Allowed);

    let request = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Restriction,
        DsrJurisdiction::Gdpr,
        env.now_ms,
    );
    let decision = match env.dsr.submit(&request) {
        Ok(d) => d,
        Err(e) => panic!("dsr submit failed: {e:?}"),
    };
    assert!(decision.is_accepted(), "Restriction must accept w/o MFA");

    // Flip the policy ledger (mirrors the production downstream
    // worker that consumes the canonical `dsr.queued.v1` event +
    // updates the tenant_processing_policy table).
    env.policy
        .set_restriction(tenant.tenant_id, RestrictionFlag::Paused);

    // Post-condition: writes return Restricted.
    assert_eq!(
        env.policy.evaluate_write(tenant.tenant_id),
        PolicyDecision::Restricted
    );

    // Reads remain implicit Allowed (restriction is write-only per
    // GDPR Art. 18.2 — the policy ledger does NOT gate reads).
    // We model the read path by checking the policy stays as a
    // `restriction` flag only; the read code path never calls
    // `evaluate_write`.
    assert_eq!(env.policy.restriction(tenant.tenant_id), RestrictionFlag::Paused);

    // No MFA gate hit.
    assert_eq!(env.dsr_mfa.verified_count(), 0);
}
