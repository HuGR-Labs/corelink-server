//! Regression test suite: 3-locales sync enforcement + minor/major bump
//! classification + native speaker review enforcement.
//!
//! AC-004: PR updating only pt-BR → blocked.
//! AC-005: native_speaker_review.es-MX = false → blocked.
//! AC-002 / AC-003: major bump → re-consent; minor bump → silent.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed to use these primitives"
)]

use corelink_privacy::notice::{
    audit::InMemoryNoticeAuditSink,
    emitter::{InMemoryNoticeEmitter, NoticeEmitDecision, NoticeEmitter, NoticePublishRequest},
    error::NoticeEmitterError,
    event::{NoticeLocale, NoticeVersion},
    store::{InMemoryNoticeStateStore, NoticePublicationState},
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn make_full_request(new_version: NoticeVersion) -> NoticePublishRequest {
    let mut contents = BTreeMap::new();
    contents.insert(NoticeLocale::PtBr, "PT notice content.".to_string());
    contents.insert(NoticeLocale::EnUs, "EN notice content.".to_string());
    contents.insert(NoticeLocale::EsMx, "ES notice content.".to_string());
    let mut reviewed = BTreeMap::new();
    reviewed.insert(NoticeLocale::PtBr, true);
    reviewed.insert(NoticeLocale::EnUs, true);
    reviewed.insert(NoticeLocale::EsMx, true);
    NoticePublishRequest::new(
        new_version,
        contents,
        1_000,
        Uuid::nil(),
        Uuid::nil(),
        reviewed,
    )
}

fn make_emitter(
    store: InMemoryNoticeStateStore,
) -> (InMemoryNoticeEmitter, Arc<InMemoryNoticeAuditSink>) {
    let sink = Arc::new(InMemoryNoticeAuditSink::new());
    let emitter = InMemoryNoticeEmitter::new(
        Arc::new(Mutex::new(store))
            as Arc<Mutex<dyn corelink_privacy::notice::store::NoticeStateStore>>,
        sink.clone() as Arc<dyn corelink_privacy::notice::audit::NoticeAuditSink>,
    );
    (emitter, sink)
}

/// AC-004: Only pt-BR.md provided → LocaleSyncViolation.
#[test]
fn ac004_single_locale_blocked() {
    let (emitter, _) = make_emitter(InMemoryNoticeStateStore::new());
    let mut contents = BTreeMap::new();
    contents.insert(NoticeLocale::PtBr, "PT only".to_string());
    let mut reviewed = BTreeMap::new();
    reviewed.insert(NoticeLocale::PtBr, true);
    reviewed.insert(NoticeLocale::EnUs, true);
    reviewed.insert(NoticeLocale::EsMx, true);
    let req = NoticePublishRequest::new(
        NoticeVersion::new(1, 0),
        contents,
        0,
        Uuid::nil(),
        Uuid::nil(),
        reviewed,
    );
    let result = emitter.publish(req);
    assert!(
        matches!(result, Err(NoticeEmitterError::LocaleSyncViolation { .. })),
        "expected LocaleSyncViolation; got: {result:?}"
    );
}

/// AC-004: en-US + es-MX missing (only pt-BR) → error message mentions sync.
#[test]
fn ac004_error_message_mentions_ctrl_priv_consent_005() {
    let (emitter, _) = make_emitter(InMemoryNoticeStateStore::new());
    let mut contents = BTreeMap::new();
    contents.insert(NoticeLocale::PtBr, "PT only".to_string());
    let mut reviewed = BTreeMap::new();
    reviewed.insert(NoticeLocale::PtBr, true);
    reviewed.insert(NoticeLocale::EnUs, true);
    reviewed.insert(NoticeLocale::EsMx, true);
    let req = NoticePublishRequest::new(
        NoticeVersion::new(1, 0),
        contents,
        0,
        Uuid::nil(),
        Uuid::nil(),
        reviewed,
    );
    let err = emitter.publish(req).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("CTRL-PRIV-CONSENT-005") || msg.contains("3 locales"),
        "error message must mention CTRL-PRIV-CONSENT-005 or 3 locales: {msg}"
    );
}

/// AC-005: es-MX native speaker review checkbox = false → blocked.
#[test]
fn ac005_missing_native_speaker_review_es_mx() {
    let (emitter, _) = make_emitter(InMemoryNoticeStateStore::new());
    let mut contents = BTreeMap::new();
    contents.insert(NoticeLocale::PtBr, "PT".to_string());
    contents.insert(NoticeLocale::EnUs, "EN".to_string());
    contents.insert(NoticeLocale::EsMx, "ES".to_string());
    let mut reviewed = BTreeMap::new();
    reviewed.insert(NoticeLocale::PtBr, true);
    reviewed.insert(NoticeLocale::EnUs, true);
    reviewed.insert(NoticeLocale::EsMx, false); // Missing!
    let req = NoticePublishRequest::new(
        NoticeVersion::new(1, 0),
        contents,
        0,
        Uuid::nil(),
        Uuid::nil(),
        reviewed,
    );
    let result = emitter.publish(req);
    let valid = matches!(result, Err(NoticeEmitterError::NativeSpeakerReviewMissing { ref locale }) if locale.contains("es-MX"));
    assert!(valid, "expected NativeSpeakerReviewMissing for es-MX");
}

/// AC-002: major bump (v1.5.0 → v2.0.0) → MajorBumpPublished + re-consent required.
#[test]
fn ac002_major_bump_force_re_consent() {
    let store = InMemoryNoticeStateStore::with_state(NoticePublicationState::new(
        NoticeVersion::new(1, 5),
        0,
    ));
    let (emitter, sink) = make_emitter(store);
    let req = make_full_request(NoticeVersion::new(2, 0));
    let decision = emitter.publish(req).unwrap();
    assert!(
        matches!(decision, NoticeEmitDecision::MajorBumpPublished { .. }),
        "expected MajorBumpPublished; got: {decision:?}"
    );
    assert!(
        decision.requires_re_consent_trigger(),
        "major bump must require re-consent trigger"
    );
    // 2 audit records: published + deprecated.
    assert_eq!(sink.len(), 2, "major bump must emit 2 audit records");
}

/// AC-003: minor bump (v1.5.0 → v1.6.0) → MinorBumpPublished + no re-consent.
#[test]
fn ac003_minor_bump_silent_no_re_consent() {
    let store = InMemoryNoticeStateStore::with_state(NoticePublicationState::new(
        NoticeVersion::new(1, 5),
        0,
    ));
    let (emitter, sink) = make_emitter(store);
    let req = make_full_request(NoticeVersion::new(1, 6));
    let decision = emitter.publish(req).unwrap();
    assert!(
        matches!(decision, NoticeEmitDecision::MinorBumpPublished { .. }),
        "expected MinorBumpPublished; got: {decision:?}"
    );
    assert!(
        !decision.requires_re_consent_trigger(),
        "minor bump must NOT require re-consent trigger"
    );
    // 1 audit record: published only (no deprecated).
    assert_eq!(sink.len(), 1, "minor bump must emit 1 audit record only");
}

/// AC-001: initial publication v1.0.0 happy path.
#[test]
fn ac001_initial_publication_v1_0() {
    let (emitter, sink) = make_emitter(InMemoryNoticeStateStore::new());
    let req = make_full_request(NoticeVersion::new(1, 0));
    let decision = emitter.publish(req).unwrap();
    assert!(
        matches!(decision, NoticeEmitDecision::InitialPublication { .. }),
        "expected InitialPublication; got: {decision:?}"
    );
    // No deprecation on initial publish.
    assert!(!decision.requires_re_consent_trigger());
    // 1 audit record (published).
    assert_eq!(sink.len(), 1);
}

/// AC-006: notice_text_hash deterministic — same content in CRLF and LF forms
/// produces identical hashes.
#[test]
fn ac006_notice_text_hash_crlf_lf_parity() {
    use corelink_privacy::notice::notice_text_hash;
    let content_lf = "# Privacy Notice\n\nWe collect your data.\n\nContact: dpo@hugr.dev\n";
    let content_crlf =
        "# Privacy Notice\r\n\r\nWe collect your data.\r\n\r\nContact: dpo@hugr.dev\r\n";
    assert_eq!(
        notice_text_hash(content_lf),
        notice_text_hash(content_crlf),
        "CRLF and LF forms must produce the same hash"
    );
}

/// Semver monotonic: down-versioning blocked per notice versioning discipline.
#[test]
fn semver_downgrade_blocked() {
    let store = InMemoryNoticeStateStore::with_state(NoticePublicationState::new(
        NoticeVersion::new(2, 0),
        0,
    ));
    let (emitter, _) = make_emitter(store);
    // Try to publish v1.0 when current is v2.0 — no bump.
    let req = make_full_request(NoticeVersion::new(1, 0));
    let result = emitter.publish(req);
    assert!(
        matches!(result, Err(NoticeEmitterError::NoBumpDetected { .. })),
        "down-versioning must be blocked; got: {result:?}"
    );
}

/// notice_text_hashes present in decision for all 3 locales.
#[test]
fn decision_carries_all_3_locale_hashes() {
    let (emitter, _) = make_emitter(InMemoryNoticeStateStore::new());
    let req = make_full_request(NoticeVersion::new(1, 0));
    let decision = emitter.publish(req).unwrap();
    let hashes = decision.notice_text_hashes();
    assert_eq!(hashes.len(), 3, "expected 3 locale hashes");
    for locale in corelink_privacy::notice::canonical_notice_locales() {
        assert!(
            hashes.contains_key(locale.as_str()),
            "missing hash for locale: {}",
            locale.as_str()
        );
    }
}
