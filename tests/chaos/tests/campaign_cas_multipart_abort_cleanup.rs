//! Scenario 8: CAS multipart abort.
//!
//! Hypothesis: when a client aborts a multipart upload mid-stream,
//! `complete` is no longer callable, staged parts are GC-able, and an
//! audit + INFO event are emitted.

#![cfg(feature = "chaos")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use chaos_campaign::{
    assert_alert_fired, assert_audit_emitted_once, CampaignMultipart, MultipartState,
};

#[test]
fn client_abort_mid_stream_blocks_complete_and_gcs_parts() {
    let mut up = CampaignMultipart::open();

    up.stage_part(1);
    up.stage_part(2);
    up.stage_part(3);
    assert_eq!(up.staged_parts().len(), 3);
    assert_eq!(up.state(), &MultipartState::Open);

    // Inject client abort mid-stream.
    up.abort();
    assert_eq!(up.state(), &MultipartState::Aborted);

    // (1) `complete` rejected — no manifest seal possible after abort.
    assert!(
        up.complete().is_err(),
        "complete must fail CLOSED after abort"
    );
    assert_eq!(up.state(), &MultipartState::Aborted);

    // GC reclaims staged parts.
    assert_eq!(up.gc(), 3);
    assert!(up.staged_parts().is_empty());

    // (2) Audit event emitted.
    assert_audit_emitted_once(up.audit_events(), "corelink.cas.multipart.aborted").unwrap();

    // (3) INFO event surfaced.
    assert_alert_fired(up.info_events(), "cas_multipart_aborted").unwrap();
}

#[test]
fn abort_is_idempotent() {
    let mut up = CampaignMultipart::open();
    up.stage_part(1);
    up.abort();
    up.abort(); // second call is no-op
    assert_audit_emitted_once(up.audit_events(), "corelink.cas.multipart.aborted").unwrap();
}
