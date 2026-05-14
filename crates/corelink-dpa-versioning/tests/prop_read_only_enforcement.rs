//! Property: PAT-DEGRADE-001 read-only middleware invariant.
//!
//! For a tenant with `is_grace_expired(now) == true`:
//! - GET / HEAD / OPTIONS on `/v1/*` ⇒ Allow.
//! - POST / PUT / PATCH / DELETE on `/v1/*` ⇒ DenyReAcceptanceRequired,
//!   EXCEPT `/v1/dpa/re-accept` ⇒ Allow.
//!
//! For a tenant with `is_grace_expired(now) == false`:
//! - Every method + path ⇒ Allow.
//!
//! In CI: `PROPTEST_CASES=10000 cargo test -p corelink-dpa-versioning
//! --test prop_read_only_enforcement`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use proptest::prelude::*;
use uuid::Uuid;

use corelink_dpa_versioning::middleware::{
    GateDecision, HttpMethod, ReadOnlyDegradeGate, RE_ACCEPT_PATH,
};
use corelink_dpa_versioning::schema::{TenantDpaState, GRACE_PERIOD_SECONDS};
use corelink_dpa_versioning::version::SemverVersion;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_024)
}

fn method_strategy() -> impl Strategy<Value = HttpMethod> {
    prop_oneof![
        Just(HttpMethod::Get),
        Just(HttpMethod::Head),
        Just(HttpMethod::Options),
        Just(HttpMethod::Post),
        Just(HttpMethod::Put),
        Just(HttpMethod::Patch),
        Just(HttpMethod::Delete),
    ]
}

fn v1_path_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/v1/objects/abc".to_string()),
        Just("/v1/manifests/xyz".to_string()),
        Just("/v1/tenants/me".to_string()),
        Just("/v1/dpa/re-accept".to_string()),
        Just("/v1/billing/usage".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Core invariant: deny iff (grace_expired ∧ write ∧ path ≠ re-accept).
    #[test]
    fn write_blocked_iff_grace_expired_and_not_re_accept(
        publish_at in 0_i64..1_000_000_000,
        offset in -GRACE_PERIOD_SECONDS..(2 * GRACE_PERIOD_SECONDS),
        method in method_strategy(),
        path in v1_path_strategy(),
    ) {
        let deadline = publish_at + GRACE_PERIOD_SECONDS;
        let now = publish_at + offset;
        let tenant = TenantDpaState {
            tenant_id: Uuid::nil(),
            current_dpa_version: SemverVersion::new(1, 0, 0),
            grace_expires_at: Some(deadline),
            re_acceptance_pending: true,
        };

        let decision = ReadOnlyDegradeGate::evaluate(&tenant, method, &path, now);
        let expired = now >= deadline;
        let is_re_accept = path == RE_ACCEPT_PATH;
        let should_deny = expired && method.is_write() && !is_re_accept;

        if should_deny {
            prop_assert_eq!(decision, GateDecision::DenyReAcceptanceRequired);
        } else {
            prop_assert_eq!(decision, GateDecision::Allow);
        }
    }

    /// Settled tenants (no pending) are NEVER denied.
    #[test]
    fn settled_tenant_never_denied(
        now in any::<i64>(),
        method in method_strategy(),
        path in v1_path_strategy(),
    ) {
        let tenant = TenantDpaState::fresh(Uuid::nil(), SemverVersion::new(2, 0, 0));
        let decision = ReadOnlyDegradeGate::evaluate(&tenant, method, &path, now);
        prop_assert_eq!(decision, GateDecision::Allow);
    }

    /// Non-`/v1/*` paths are out of scope.
    #[test]
    fn non_v1_paths_always_allowed(
        offset in 0_i64..(2 * GRACE_PERIOD_SECONDS),
        method in method_strategy(),
        suffix in "[a-z]{1,8}",
    ) {
        let publish_at = 1_000_000_i64;
        let tenant = TenantDpaState {
            tenant_id: Uuid::nil(),
            current_dpa_version: SemverVersion::new(1, 0, 0),
            grace_expires_at: Some(publish_at + GRACE_PERIOD_SECONDS),
            re_acceptance_pending: true,
        };
        let path = format!("/internal/{}", suffix);
        let now = publish_at + offset;
        let decision = ReadOnlyDegradeGate::evaluate(&tenant, method, &path, now);
        prop_assert_eq!(decision, GateDecision::Allow);
    }
}
