//! Scenario 5: every BYOK provider returns 503.
//!
//! Hypothesis: when AWS KMS, GCP KMS, Azure Key Vault, and Vault are
//! all 503, the route layer fails CLOSED — no default key, no
//! plaintext fallback, an audit event is emitted, SEV-1 fires.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, ByokOutcome, ByokProvider, CampaignByokModel,
};

#[test]
fn all_providers_unavailable_fails_closed_with_503() {
    let mut byok = CampaignByokModel::new();

    // Baseline — preferred provider returns a key.
    assert_eq!(
        byok.acquire_data_key(ByokProvider::AwsKms),
        ByokOutcome::KeyAcquired(ByokProvider::AwsKms)
    );

    // Inject 503 across every provider.
    for p in [
        ByokProvider::AwsKms,
        ByokProvider::GcpKms,
        ByokProvider::AzureKv,
        ByokProvider::Vault,
    ] {
        byok.inject_provider_503(p);
    }

    // (1) Fail CLOSED.
    assert_eq!(
        byok.acquire_data_key(ByokProvider::AwsKms),
        ByokOutcome::ProviderUnavailable503,
        "no plaintext / default-key fallback under chaos"
    );

    // (2) Audit event emitted.
    assert_audit_emitted_once(byok.audit_events(), "corelink.byok.provider.unavailable").unwrap();

    // (3) SEV-1 alert fired.
    assert_alert_fired(byok.sev1_alerts(), "byok_provider_unavailable").unwrap();
}

#[test]
fn single_provider_unavailable_falls_through_to_healthy_peer() {
    let mut byok = CampaignByokModel::new();
    byok.inject_provider_503(ByokProvider::AwsKms);

    // Preferred is down but a peer is healthy → succeed against the peer.
    match byok.acquire_data_key(ByokProvider::AwsKms) {
        ByokOutcome::KeyAcquired(p) => assert_ne!(p, ByokProvider::AwsKms),
        ByokOutcome::ProviderUnavailable503 => {
            panic!("must fail over to a healthy peer when one is available")
        }
    }

    // No SEV-1 should have fired because the operation succeeded.
    assert!(byok.sev1_alerts().is_empty());
}
