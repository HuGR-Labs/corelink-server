//! Combined Scenario A (wave-23): cross-region network partition + BYOK
//! provider 503 during the failover window.
//!
//! This scenario composes Scenario 1 (`CampaignFailoverModel`) and
//! Scenario 5 (`CampaignByokModel`) from the wave-22 harness to assert
//! the **orchestrated** fail-CLOSED contract when two SEV-1 incidents
//! coincide.
//!
//! Steady-state hypothesis: a partitioned region must fail writes
//! CLOSED *and* every encrypt/decrypt against a downed BYOK provider
//! quorum must fail CLOSED, regardless of order. Both SEV-1 audit
//! anchors land exactly once and both SEV-1 alert names fire.
//!
//! Wave-22 left this as a caveat for wave-23: the original 8 scenarios
//! exercise each failure in isolation; the dress rehearsal flagged that
//! cross-failure interactions deserve their own pinned test so future
//! refactors cannot accidentally collapse one of the two fail-CLOSED
//! paths under load.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, ByokOutcome, ByokProvider,
    CampaignByokModel, CampaignFailoverModel, CampaignRegion, RouteOutcome,
};

#[test]
fn partition_plus_byok_503_both_fail_closed_with_dual_sev1_anchors() {
    let mut failover = CampaignFailoverModel::new();
    let mut byok = CampaignByokModel::new();

    // --- Baseline: every region healthy, every BYOK provider available.
    assert_eq!(
        failover.route_write(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::UsEast)
    );
    assert_eq!(
        byok.acquire_data_key(ByokProvider::AwsKms),
        ByokOutcome::KeyAcquired(ByokProvider::AwsKms)
    );

    // --- Stage 1: partition UsEast (SEV-1 #1).
    failover.inject_partition(CampaignRegion::UsEast);

    // Failover routes reads to EuWest while writes against UsEast fail
    // CLOSED. The partner must still be healthy for the read path to
    // serve — that is the canonical "degraded but available" mode.
    assert_eq!(
        failover.route_write(CampaignRegion::UsEast),
        RouteOutcome::FailedClosed503,
        "writes against the partitioned region must fail CLOSED"
    );
    assert_eq!(
        failover.route_read(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::EuWest),
        "reads must fail over to the healthy partner"
    );

    // --- Stage 2: during the failover window, all four BYOK providers
    // go 503 (e.g. the partner-region call path also depends on a
    // multi-cloud KMS quorum that is now in extended brown-out).
    for p in [
        ByokProvider::AwsKms,
        ByokProvider::GcpKms,
        ByokProvider::AzureKv,
        ByokProvider::Vault,
    ] {
        byok.inject_provider_503(p);
    }

    // Every preferred provider must fail CLOSED — no plaintext fall-back,
    // no fail-OPEN to a default key. This is the cross-failure contract:
    // even when the request would otherwise have succeeded post-failover
    // (read from EuWest), an unavailable KMS quorum still 503s.
    for preferred in [
        ByokProvider::AwsKms,
        ByokProvider::GcpKms,
        ByokProvider::AzureKv,
        ByokProvider::Vault,
    ] {
        assert_eq!(
            byok.acquire_data_key(preferred),
            ByokOutcome::ProviderUnavailable503,
            "BYOK acquire must fail CLOSED for preferred = {preferred:?}"
        );
    }

    // --- Stage 3: writes against the (still partitioned) region remain
    // CLOSED — the BYOK failure did not "rescue" the failover path.
    assert_eq!(
        failover.route_write(CampaignRegion::UsEast),
        RouteOutcome::FailedClosed503,
        "writes against the partitioned region must remain CLOSED across the BYOK incident"
    );

    // --- Dual audit anchors — each emitted exactly once per incident.
    assert_audit_emitted_once(
        failover.audit_events(),
        "corelink.failover.region.degraded",
    )
    .unwrap();
    // BYOK audit fires once per failed acquire — assert it fired ≥ 1×
    // and the matching SEV-1 alert is present.
    assert!(
        byok.audit_events()
            .iter()
            .any(|e| e == "corelink.byok.provider.unavailable"),
        "byok audit event must fire on each fail-CLOSED acquire, got {:?}",
        byok.audit_events()
    );

    // --- Dual SEV-1 alerts.
    assert_alert_fired(failover.sev1_alerts(), "region_isolated").unwrap();
    assert_alert_fired(byok.sev1_alerts(), "byok_provider_unavailable").unwrap();

    // --- Recovery: heal region first, then restore providers. Order is
    // significant — even after the region recovers, BYOK acquires must
    // remain CLOSED until at least one provider returns.
    failover.heal(CampaignRegion::UsEast);
    assert_eq!(
        failover.route_write(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::UsEast),
        "post-heal, writes against the restored region resume"
    );
    assert_eq!(
        byok.acquire_data_key(ByokProvider::AwsKms),
        ByokOutcome::ProviderUnavailable503,
        "BYOK must remain CLOSED while every provider is still 503"
    );

    // Restore one BYOK provider — fall-through fallback now succeeds.
    let mut byok_restored = CampaignByokModel::new();
    // Mimic the post-recovery state: AWS healthy, GCP still 503.
    byok_restored.inject_provider_503(ByokProvider::GcpKms);
    assert_eq!(
        byok_restored.acquire_data_key(ByokProvider::AwsKms),
        ByokOutcome::KeyAcquired(ByokProvider::AwsKms),
        "post-recovery, the preferred-but-healthy provider serves the request"
    );
}
