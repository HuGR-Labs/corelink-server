//! Scenario 1: cross-region network partition.
//!
//! Steady-state hypothesis: after a 5-minute cross-region link drop,
//! the surviving region serves reads from its local replica, **writes
//! against the partitioned region fail CLOSED (503)**, and the audit
//! taxonomy + SEV-1 alert fire exactly once.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignFailoverModel, CampaignRegion,
    RouteOutcome,
};

#[test]
fn partition_routes_reads_to_partner_and_fails_writes_closed() {
    let mut model = CampaignFailoverModel::new();

    // Baseline — both regions healthy, writes & reads succeed locally.
    assert_eq!(
        model.route_write(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::UsEast)
    );
    assert_eq!(
        model.route_read(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::UsEast)
    );

    // Inject partition affecting UsEast for 5 minutes.
    model.inject_partition(CampaignRegion::UsEast);

    // (1) Writes against the partitioned region fail CLOSED.
    assert_eq!(
        model.route_write(CampaignRegion::UsEast),
        RouteOutcome::FailedClosed503,
        "writes against a partitioned region must fail CLOSED with 503"
    );

    // Reads fail over to the surviving partner.
    assert_eq!(
        model.route_read(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::EuWest),
        "reads must fail over to a healthy partner region"
    );

    // (2) Audit event emitted exactly once.
    assert_audit_emitted_once(model.audit_events(), "corelink.failover.region.degraded").unwrap();

    // (3) SEV-1 alert fired.
    assert_alert_fired(model.sev1_alerts(), "region_isolated").unwrap();

    // Heal — writes resume.
    model.heal(CampaignRegion::UsEast);
    assert_eq!(
        model.route_write(CampaignRegion::UsEast),
        RouteOutcome::Served(CampaignRegion::UsEast)
    );
}
