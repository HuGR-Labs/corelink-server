//! Property tests for [`corelink_stripe_real::portal`].
//!
//! Invariants under test:
//!
//! - **INV-BILLING-PORTAL-URL-SINGLE-USE**: every successful
//!   `create_session` call returns a URL that has never been returned
//!   before, even for the same `(customer_id, return_url)` input.
//! - **INV-BILLING-PORTAL-URL-HTTPS**: every URL is HTTPS and embeds an
//!   opaque session id (the URL is bearer-equivalent — leaking it MUST
//!   imply leaking a session).
//! - **INV-BILLING-PORTAL-AUDIT-FAIL-CLOSED**: any path that does NOT
//!   return `Ok(url)` MUST NOT have recorded an audit row; any path
//!   that returns `Ok(url)` MUST have exactly one audit row for that
//!   call.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use corelink_stripe_real::portal::{
    BillingPortalSessionCreator, InMemoryPortalAuditSink, InMemoryPortalSessionCreator,
    PortalSessionError,
};
use proptest::prelude::*;

fn make() -> (Arc<InMemoryPortalAuditSink>, InMemoryPortalSessionCreator) {
    let sink = Arc::new(InMemoryPortalAuditSink::new());
    let creator = InMemoryPortalSessionCreator::new(sink.clone());
    (sink, creator)
}

proptest! {
    /// Single property covering all three invariants for batches of
    /// up to 32 sequential `create_session` calls against the same
    /// `(customer, return_url)`. Uniqueness, HTTPS shape, and audit
    /// row count must hold for every batch.
    #[test]
    fn portal_urls_unique_https_audit_consistent(
        n in 1usize..32,
        suffix in "[a-z0-9]{4,16}",
    ) {
        let (sink, creator) = make();
        let customer = format!("cus_{suffix}");
        let return_url = "https://app.corelink.humangr.com/customer/billing";
        let tenant = "tenant_acme";

        let mut urls = Vec::with_capacity(n);
        for _ in 0..n {
            let url = creator
                .create_session(&customer, return_url, tenant)
                .expect("happy path must succeed");
            prop_assert!(url.as_str().starts_with("https://"));
            prop_assert!(
                url.as_str().contains("/p/session/bps_"),
                "URL must embed opaque session id: {}",
                url.as_str()
            );
            urls.push(url);
        }

        // INV-BILLING-PORTAL-URL-SINGLE-USE: all distinct.
        let mut seen = std::collections::HashSet::new();
        for u in &urls {
            prop_assert!(seen.insert(u.as_str().to_string()), "duplicate URL {}", u.as_str());
        }

        // INV-BILLING-PORTAL-AUDIT-FAIL-CLOSED happy path: exactly one event per Ok.
        let events = sink.events();
        prop_assert_eq!(events.len(), n);
        for e in &events {
            prop_assert_eq!(&e.customer_id, &customer);
            prop_assert_eq!(&e.return_url, &return_url.to_string());
            prop_assert_eq!(&e.tenant_id, &tenant.to_string());
        }
    }

    /// Audit failure MUST drop the URL — `Err(AuditFailed)` AND zero
    /// rows recorded.
    #[test]
    fn audit_failure_drops_url_zero_rows(
        suffix in "[a-z0-9]{4,16}",
    ) {
        let (sink, creator) = make();
        creator.arm_audit_failure();
        let err = creator
            .create_session(&format!("cus_{suffix}"), "https://x.example/", "t_1")
            .unwrap_err();
        prop_assert!(matches!(err, PortalSessionError::AuditFailed(_)));
        prop_assert!(sink.events().is_empty());
        prop_assert!(creator.issued().is_empty());
    }
}
