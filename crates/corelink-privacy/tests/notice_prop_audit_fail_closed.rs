//! Property test: audit fail-CLOSED ordering.
//!
//! AC-008: "audit emit failure → state UNCHANGED (fail-CLOSED)".
//! Per Lote 10.6bis split-tier discipline + ADR-S11-002 lesson absorbed:
//! `lookup → emit_audit → mutate_state` ordering; state NEVER mutated on
//! audit failure.
//!
//! In CI: `PROPTEST_CASES=10000 cargo test -p corelink-privacy-notice-emit
//! --test prop_audit_fail_closed`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "property tests are allowed to use these primitives"
)]

use corelink_privacy::notice::{
    emitter::{InMemoryNoticeEmitter, NoticeEmitter, NoticePublishRequest},
    audit::FailingNoticeAuditSink,
    error::NoticeEmitterError,
    event::{NoticeLocale, NoticeVersion},
    store::{InMemoryNoticeStateStore, NoticePublicationState, NoticeStateStore},
};
use proptest::prelude::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn make_request_all_locales(major: u32, minor: u32) -> NoticePublishRequest {
    let mut contents = BTreeMap::new();
    contents.insert(NoticeLocale::PtBr, "PT content".to_string());
    contents.insert(NoticeLocale::EnUs, "EN content".to_string());
    contents.insert(NoticeLocale::EsMx, "ES content".to_string());
    let mut reviewed = BTreeMap::new();
    reviewed.insert(NoticeLocale::PtBr, true);
    reviewed.insert(NoticeLocale::EnUs, true);
    reviewed.insert(NoticeLocale::EsMx, true);
    NoticePublishRequest::new(
        NoticeVersion::new(major, minor),
        contents,
        1_000_000,
        Uuid::nil(),
        Uuid::nil(),
        reviewed,
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// AC-008: state UNCHANGED when audit sink always fails.
    #[test]
    fn state_unchanged_on_audit_failure(
        pre_major in 0_u32..=5,
        pre_minor in 0_u32..=10,
        new_minor in 1_u32..=20,
    ) {
        let seed = NoticePublicationState::new(
            NoticeVersion::new(pre_major, pre_minor),
            0,
        );
        let store = InMemoryNoticeStateStore::with_state(seed.clone());
        let emitter = InMemoryNoticeEmitter::new(
            Arc::new(Mutex::new(store.clone())) as Arc<Mutex<dyn corelink_privacy::notice::store::NoticeStateStore>>,
            Arc::new(FailingNoticeAuditSink),
        );

        // Bump minor so there's a valid bump (avoid NoBumpDetected swamping the test).
        let new_v = NoticeVersion::new(pre_major, pre_minor.saturating_add(new_minor));
        let req = make_request_all_locales(new_v.major, new_v.minor);
        let result = emitter.publish(req);

        // Must fail.
        prop_assert!(result.is_err(), "expected error from failing audit sink");
        let err_is_audit = matches!(result, Err(NoticeEmitterError::Audit(_)));
        prop_assert!(err_is_audit, "error must be Audit variant");

        // State must be unchanged.
        let current = store.current_published().unwrap();
        prop_assert_eq!(
            current.as_ref().map(|s| &s.current_version),
            Some(&seed.current_version),
            "state must be unchanged on audit failure"
        );
    }

    /// Audit fail-CLOSED: 2 records attempted on major bump; first failure
    /// must not leave partial state.
    #[test]
    fn major_bump_audit_fail_closed_no_partial_state(
        pre_major in 0_u32..=4,
        pre_minor in 0_u32..=10,
    ) {
        let seed = NoticePublicationState::new(
            NoticeVersion::new(pre_major, pre_minor),
            0,
        );
        let store = InMemoryNoticeStateStore::with_state(seed.clone());
        let emitter = InMemoryNoticeEmitter::new(
            Arc::new(Mutex::new(store.clone())) as Arc<Mutex<dyn corelink_privacy::notice::store::NoticeStateStore>>,
            Arc::new(FailingNoticeAuditSink),
        );

        // Major bump.
        let req = make_request_all_locales(pre_major + 1, 0);
        let result = emitter.publish(req);
        prop_assert!(result.is_err());

        // State must still be the seed version.
        let current = store.current_published().unwrap();
        prop_assert_eq!(
            current.as_ref().map(|s| &s.current_version),
            Some(&seed.current_version),
            "state must be unchanged after major-bump audit failure"
        );
    }
}
