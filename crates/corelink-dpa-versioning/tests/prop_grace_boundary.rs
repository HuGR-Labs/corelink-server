//! Property: grace boundary invariant.
//!
//! At the canonical 30d UTC boundary, `is_grace_expired(now)` flips
//! exactly at `grace_expires_at`. Generated `(publish_at, now)` pairs
//! over a wide range never violate that flip.
//!
//! In CI: `PROPTEST_CASES=10000 cargo test -p corelink-dpa-versioning
//! --test prop_grace_boundary`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use proptest::prelude::*;
use uuid::Uuid;

use corelink_dpa_versioning::schema::{TenantDpaState, GRACE_PERIOD_SECONDS};
use corelink_dpa_versioning::version::SemverVersion;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_024)
}

fn pending_tenant(publish_at: i64) -> TenantDpaState {
    TenantDpaState {
        tenant_id: Uuid::nil(),
        current_dpa_version: SemverVersion::new(1, 0, 0),
        grace_expires_at: Some(publish_at + GRACE_PERIOD_SECONDS),
        re_acceptance_pending: true,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Boundary: `now < grace_expires_at` ⇒ not expired;
    /// `now >= grace_expires_at` ⇒ expired.
    #[test]
    fn grace_boundary_canonical(
        publish_at in 0_i64..1_000_000_000,
        offset in -GRACE_PERIOD_SECONDS..(2 * GRACE_PERIOD_SECONDS),
    ) {
        let tenant = pending_tenant(publish_at);
        let now = publish_at + offset;
        let deadline = publish_at + GRACE_PERIOD_SECONDS;

        let expired = tenant.is_grace_expired(now);
        prop_assert_eq!(expired, now >= deadline,
            "publish_at={} now={} deadline={}", publish_at, now, deadline);
    }

    /// A tenant with no pending bump is NEVER grace-expired, even at
    /// `now = i64::MAX`.
    #[test]
    fn settled_tenant_never_expires(
        now in any::<i64>(),
        major in 0_u32..10,
    ) {
        let tenant = TenantDpaState::fresh(
            Uuid::nil(),
            SemverVersion::new(major, 0, 0),
        );
        prop_assert!(!tenant.is_grace_expired(now));
    }

    /// `grace_remaining_days` saturates to 0 (no negative days
    /// surfaced to dashboards / API responses).
    #[test]
    fn grace_remaining_days_saturating(
        publish_at in 0_i64..1_000_000_000,
        offset in -GRACE_PERIOD_SECONDS..(2 * GRACE_PERIOD_SECONDS),
    ) {
        let tenant = pending_tenant(publish_at);
        let now = publish_at + offset;
        let days = tenant.grace_remaining_days(now).unwrap();
        prop_assert!(days >= 0);
    }
}
