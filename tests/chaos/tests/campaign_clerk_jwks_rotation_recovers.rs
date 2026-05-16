//! Scenario 7: Clerk JWKS rotation mid-request.
//!
//! Hypothesis: when Clerk rotates its signing key mid-request, the
//! verifier transparently re-fetches the JWKS, verifies the token,
//! emits an info-level rotation event, and never falls back to a
//! cached-stale verdict.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignClerkJwks, JwksOutcome,
};

#[test]
fn rotation_triggers_refetch_and_succeeds() {
    let mut jwks = CampaignClerkJwks::new("kid-old");

    // Baseline — old kid verifies from cache.
    assert_eq!(
        jwks.verify("kid-old"),
        JwksOutcome::VerifiedCached("kid-old".into())
    );

    // Inject rotation: upstream now serves kid-new; request still
    // signed by kid-new arrives → cache miss triggers re-fetch.
    jwks.inject_rotation("kid-new");

    assert_eq!(
        jwks.verify("kid-new"),
        JwksOutcome::VerifiedAfterRefetch("kid-new".into()),
        "rotation must re-fetch and verify rather than fail closed silently"
    );

    // (2) Audit event emitted.
    assert_audit_emitted_once(jwks.audit_events(), "corelink.clerk.jwks.rotated").unwrap();

    // (3) INFO event surfaced.
    assert_alert_fired(jwks.info_events(), "clerk_jwks_rotation").unwrap();
}

#[test]
fn token_signed_by_unknown_kid_fails_closed_after_refetch() {
    let mut jwks = CampaignClerkJwks::new("kid-old");
    jwks.inject_rotation("kid-new");

    // Attacker presents a kid neither cached nor upstream.
    assert_eq!(jwks.verify("kid-bogus"), JwksOutcome::Unverified);
}
