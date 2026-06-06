//! Privacy-notice emitter orchestrator.
//!
//! ## Canonical pipeline ordering (AC-008 + INV-AUDIT-APPEND-ONLY)
//!
//! ```text
//! lookup current_published()
//!   → validate semver bump (no-bump → Err::NoBumpDetected)
//!   → validate 3-locales sync (missing locale → Err::LocaleSyncViolation)
//!   → emit_audit(published_record)    ← BEFORE state mutation
//!   → [if major bump] emit_audit(deprecated_record)   ← BEFORE state mutation
//!   → mutate_state: store.set_published(new_version)
//!   → return NoticeEmitDecision
//! ```
//!
//! Audit failure at any step aborts immediately (fail-CLOSED). State is
//! UNCHANGED on audit failure (test: `cargo test chaos_audit_emit_failure`).
//!
//! ## F-001 closure
//!
//! The [`InMemoryNoticeEmitter`] holds per-instance `Arc<Mutex<>>` over
//! the store reference. NEVER `static LazyLock<Mutex<>>`.

use super::audit::{NoticeAuditRecord, NoticeAuditSink};
use super::error::{NoticeEmitterError, NoticeStoreError};
use super::event::{
    canonical_notice_locales, NoticeDeprecatedPayload, NoticeLocale, NoticePublishedPayload,
    NoticeVersion, VersionBump,
};
use super::store::{NoticePublicationState, NoticeStateStore};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Publication request carrying all fields required by the CI hook + emitter
/// validation per WI-S11-004 §6.1.
#[derive(Clone, Debug)]
pub struct NoticePublishRequest {
    /// New version to publish.
    pub new_version: NoticeVersion,
    /// Raw markdown content per locale (must contain all 3 canonical locales).
    pub locale_contents: BTreeMap<NoticeLocale, alloc::string::String>,
    /// Wall-clock instant of the publication request (Unix epoch ms).
    pub requested_at_ms: u64,
    /// UUIDv7 event id for the published CloudEvent (provided by caller
    /// so the production wiring can pass a deterministic UUIDv7 from
    /// the CF Worker request context).
    pub published_event_id: Uuid,
    /// UUIDv7 event id for the deprecated CloudEvent (only used on major bump).
    pub deprecated_event_id: Uuid,
    /// Native speaker review flags per locale (from metadata.yaml).
    pub native_speaker_reviewed: BTreeMap<NoticeLocale, bool>,
}

impl NoticePublishRequest {
    /// Construct a publication request.
    #[must_use]
    pub fn new(
        new_version: NoticeVersion,
        locale_contents: BTreeMap<NoticeLocale, alloc::string::String>,
        requested_at_ms: u64,
        published_event_id: Uuid,
        deprecated_event_id: Uuid,
        native_speaker_reviewed: BTreeMap<NoticeLocale, bool>,
    ) -> Self {
        Self {
            new_version,
            locale_contents,
            requested_at_ms,
            published_event_id,
            deprecated_event_id,
            native_speaker_reviewed,
        }
    }
}

/// Decision outcome from [`NoticeEmitter::publish`].
///
/// `#[non_exhaustive]` per Lote 10.9-quinquies.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoticeEmitDecision {
    /// Minor bump published successfully (no re-consent triggered).
    MinorBumpPublished {
        /// The published version.
        version: NoticeVersion,
        /// Per-locale hashes for downstream INV-CONSENT-PROOF-VERIFIABLE
        /// cross-validation.
        notice_text_hashes: BTreeMap<alloc::string::String, alloc::string::String>,
    },
    /// Major bump published (material change; CTRL-PRIV-CONSENT-005:
    /// stale_consent_check must be triggered in CD pipeline).
    MajorBumpPublished {
        /// The new published version.
        version: NoticeVersion,
        /// The deprecated previous version.
        deprecated_version: NoticeVersion,
        /// Per-locale hashes.
        notice_text_hashes: BTreeMap<alloc::string::String, alloc::string::String>,
    },
    /// Initial publication (v1.0 — no predecessor; no deprecation event).
    InitialPublication {
        /// The first published version.
        version: NoticeVersion,
        /// Per-locale hashes.
        notice_text_hashes: BTreeMap<alloc::string::String, alloc::string::String>,
    },
}

impl NoticeEmitDecision {
    /// Version that was published in this decision.
    #[must_use]
    pub fn published_version(&self) -> &NoticeVersion {
        match self {
            Self::MinorBumpPublished { version, .. }
            | Self::MajorBumpPublished { version, .. }
            | Self::InitialPublication { version, .. } => version,
        }
    }

    /// Whether a re-consent trigger is required (major bump only).
    #[must_use]
    pub fn requires_re_consent_trigger(&self) -> bool {
        matches!(self, Self::MajorBumpPublished { .. })
    }

    /// The per-locale notice_text_hashes for downstream consent ledger
    /// cross-validation (INV-CONSENT-PROOF-VERIFIABLE).
    #[must_use]
    pub fn notice_text_hashes(&self) -> &BTreeMap<alloc::string::String, alloc::string::String> {
        match self {
            Self::MinorBumpPublished {
                notice_text_hashes, ..
            }
            | Self::MajorBumpPublished {
                notice_text_hashes, ..
            }
            | Self::InitialPublication {
                notice_text_hashes, ..
            } => notice_text_hashes,
        }
    }
}

/// Trait for the privacy-notice publication emitter.
pub trait NoticeEmitter: core::fmt::Debug + Send + Sync {
    /// Publish a new notice version. Follows the canonical pipeline:
    /// lookup → emit_audit → mutate_state.
    ///
    /// # Errors
    ///
    /// Returns [`NoticeEmitterError`] on:
    /// - Audit emit failure (fail-CLOSED; state unchanged).
    /// - Semver validation failure.
    /// - 3-locales sync violation.
    /// - Native speaker review missing.
    /// - No bump detected.
    /// - Store failure.
    fn publish(
        &self,
        request: NoticePublishRequest,
    ) -> Result<NoticeEmitDecision, NoticeEmitterError>;
}

/// In-memory orchestrator implementing [`NoticeEmitter`]. Holds per-instance
/// `Arc<Mutex<>>` over both the store and audit sink (F-001).
#[derive(Debug, Clone)]
pub struct InMemoryNoticeEmitter {
    store: Arc<Mutex<dyn NoticeStateStore>>,
    audit_sink: Arc<dyn NoticeAuditSink>,
}

impl InMemoryNoticeEmitter {
    /// Construct the emitter with a store + audit sink.
    ///
    /// The `Arc<Mutex<>>` wrapping of the store is the F-001 per-instance
    /// closure (NOT `static LazyLock`).
    #[must_use]
    pub fn new(
        store: Arc<Mutex<dyn NoticeStateStore>>,
        audit_sink: Arc<dyn NoticeAuditSink>,
    ) -> Self {
        Self { store, audit_sink }
    }
}

impl NoticeEmitter for InMemoryNoticeEmitter {
    fn publish(
        &self,
        request: NoticePublishRequest,
    ) -> Result<NoticeEmitDecision, NoticeEmitterError> {
        // --- VALIDATE 3-locales sync ---
        let canonical = canonical_notice_locales();
        for locale in canonical {
            if !request.locale_contents.contains_key(locale) {
                return Err(NoticeEmitterError::LocaleSyncViolation {
                    reason: alloc::format!(
                        "3 locales sync violation: {} not present in publication request; \
                         per CTRL-PRIV-CONSENT-005, all 3 locales must be synced",
                        locale.as_str()
                    ),
                });
            }
        }

        // --- VALIDATE native speaker review ---
        for locale in canonical {
            let reviewed = request
                .native_speaker_reviewed
                .get(locale)
                .copied()
                .unwrap_or(false);
            if !reviewed {
                return Err(NoticeEmitterError::NativeSpeakerReviewMissing {
                    locale: locale.as_str().into(),
                });
            }
        }

        // --- LOOKUP current published state (step 1: lookup) ---
        let current = {
            self.store
                .lock()
                .map_err(|_| {
                    NoticeEmitterError::Store(NoticeStoreError::Internal {
                        reason: "store mutex poisoned".into(),
                    })
                })?
                .current_published()
                .map_err(NoticeEmitterError::Store)?
        };

        // --- VALIDATE semver bump ---
        let bump = match &current {
            None => None, // Initial publication — no predecessor.
            Some(state) => {
                let bump = request.new_version.bump_vs(&state.current_version);
                if bump.is_none() {
                    return Err(NoticeEmitterError::NoBumpDetected {
                        current: state.current_version.as_version_str(),
                        new: request.new_version.as_version_str(),
                    });
                }
                bump
            }
        };

        // --- COMPUTE notice_text_hashes (deterministic per AC-006) ---
        let notice_text_hashes: BTreeMap<alloc::string::String, alloc::string::String> = request
            .locale_contents
            .iter()
            .map(|(locale, content)| {
                (
                    locale.as_str().to_owned(),
                    super::event::notice_text_hash(content),
                )
            })
            .collect();

        // --- EMIT AUDIT (step 2: emit_audit BEFORE state mutation) ---
        // Determine the effective bump type for the published payload.
        let effective_bump = match bump {
            None => VersionBump::Minor, // Initial publication uses Minor (no material change).
            Some(b) => b,
        };

        let published_payload = NoticePublishedPayload::new(
            request.published_event_id,
            request.new_version.clone(),
            effective_bump,
            notice_text_hashes.clone(),
            request.requested_at_ms,
        );
        self.audit_sink
            .emit(NoticeAuditRecord::published(published_payload))
            .map_err(NoticeEmitterError::Audit)?;

        // On major bump: also emit the deprecated record BEFORE state mutation.
        let deprecated_version: Option<NoticeVersion> =
            if matches!(effective_bump, VersionBump::Major) {
                if let Some(ref state) = current {
                    let dep_payload = NoticeDeprecatedPayload::new(
                        request.deprecated_event_id,
                        state.current_version.clone(),
                        request.new_version.clone(),
                        request.requested_at_ms,
                    );
                    self.audit_sink
                        .emit(NoticeAuditRecord::deprecated(dep_payload))
                        .map_err(NoticeEmitterError::Audit)?;
                    Some(state.current_version.clone())
                } else {
                    None
                }
            } else {
                None
            };

        // --- MUTATE STATE (step 3: mutate_state; only reached if audits succeeded) ---
        {
            self.store
                .lock()
                .map_err(|_| {
                    NoticeEmitterError::Store(NoticeStoreError::Internal {
                        reason: "store mutex poisoned (mutate)".into(),
                    })
                })?
                .set_published(NoticePublicationState::new(
                    request.new_version.clone(),
                    request.requested_at_ms,
                ))
                .map_err(NoticeEmitterError::Store)?;
        }

        // --- RETURN DECISION ---
        let decision = match (bump, deprecated_version) {
            (None, _) => NoticeEmitDecision::InitialPublication {
                version: request.new_version,
                notice_text_hashes,
            },
            (Some(VersionBump::Major), Some(dep_ver)) => NoticeEmitDecision::MajorBumpPublished {
                version: request.new_version,
                deprecated_version: dep_ver,
                notice_text_hashes,
            },
            (Some(VersionBump::Major), None) => {
                // Major bump with no predecessor (shouldn't happen; guard).
                NoticeEmitDecision::InitialPublication {
                    version: request.new_version,
                    notice_text_hashes,
                }
            }
            (Some(VersionBump::Minor), _) => NoticeEmitDecision::MinorBumpPublished {
                version: request.new_version,
                notice_text_hashes,
            },
        };

        Ok(decision)
    }
}

extern crate alloc;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::super::audit::{FailingNoticeAuditSink, InMemoryNoticeAuditSink};
    use super::super::store::InMemoryNoticeStateStore;
    use super::*;
    use uuid::Uuid;

    fn make_request(
        new_version: NoticeVersion,
        bump_type_hint: Option<&str>,
    ) -> NoticePublishRequest {
        let _ = bump_type_hint;
        let mut contents = BTreeMap::new();
        contents.insert(NoticeLocale::PtBr, "PT content".to_string());
        contents.insert(NoticeLocale::EnUs, "EN content".to_string());
        contents.insert(NoticeLocale::EsMx, "ES content".to_string());

        let mut reviewed = BTreeMap::new();
        reviewed.insert(NoticeLocale::PtBr, true);
        reviewed.insert(NoticeLocale::EnUs, true);
        reviewed.insert(NoticeLocale::EsMx, true);

        NoticePublishRequest::new(
            new_version,
            contents,
            1_000_000,
            Uuid::nil(),
            Uuid::nil(),
            reviewed,
        )
    }

    fn make_emitter_with_inmem_sink(
        store: InMemoryNoticeStateStore,
    ) -> (InMemoryNoticeEmitter, Arc<InMemoryNoticeAuditSink>) {
        let sink = Arc::new(InMemoryNoticeAuditSink::new());
        let emitter = InMemoryNoticeEmitter::new(
            Arc::new(Mutex::new(store)) as Arc<Mutex<dyn NoticeStateStore>>,
            sink.clone(),
        );
        (emitter, sink)
    }

    #[test]
    fn initial_publication_happy_path() {
        let (emitter, sink) = make_emitter_with_inmem_sink(InMemoryNoticeStateStore::new());
        let req = make_request(NoticeVersion::new(1, 0), None);
        let decision = emitter.publish(req).unwrap();
        let valid = matches!(decision, NoticeEmitDecision::InitialPublication { .. });
        assert!(valid);
        // One published audit record emitted.
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn minor_bump_emits_one_audit_record_no_deprecation() {
        let store = InMemoryNoticeStateStore::with_state(NoticePublicationState::new(
            NoticeVersion::new(1, 5),
            0,
        ));
        let (emitter, sink) = make_emitter_with_inmem_sink(store);
        let req = make_request(NoticeVersion::new(1, 6), None);
        let decision = emitter.publish(req).unwrap();
        let valid = matches!(decision, NoticeEmitDecision::MinorBumpPublished { .. });
        assert!(valid);
        assert!(!decision.requires_re_consent_trigger());
        // Only 1 audit record (published; no deprecated on minor).
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn major_bump_emits_two_audit_records_and_deprecated_decision() {
        let store = InMemoryNoticeStateStore::with_state(NoticePublicationState::new(
            NoticeVersion::new(1, 5),
            0,
        ));
        let (emitter, sink) = make_emitter_with_inmem_sink(store);
        let req = make_request(NoticeVersion::new(2, 0), None);
        let decision = emitter.publish(req).unwrap();
        let valid = matches!(decision, NoticeEmitDecision::MajorBumpPublished { .. });
        assert!(valid);
        assert!(decision.requires_re_consent_trigger());
        // 2 audit records: published + deprecated.
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn audit_fail_closed_state_unchanged() {
        // AC-008: audit emit failure → state UNCHANGED.
        let store = InMemoryNoticeStateStore::new();
        let emitter = InMemoryNoticeEmitter::new(
            Arc::new(Mutex::new(store.clone())) as Arc<Mutex<dyn NoticeStateStore>>,
            Arc::new(FailingNoticeAuditSink),
        );
        let req = make_request(NoticeVersion::new(1, 0), None);
        let result = emitter.publish(req);
        assert!(result.is_err());
        // State must be unchanged — store still empty.
        let state = store.current_published().unwrap();
        assert!(state.is_none(), "state must be unchanged on audit failure");
    }

    #[test]
    fn no_bump_returns_err() {
        let store = InMemoryNoticeStateStore::with_state(NoticePublicationState::new(
            NoticeVersion::new(1, 5),
            0,
        ));
        let (emitter, _) = make_emitter_with_inmem_sink(store);
        // Same version as current → no bump.
        let req = make_request(NoticeVersion::new(1, 5), None);
        let result = emitter.publish(req);
        let valid = matches!(result, Err(NoticeEmitterError::NoBumpDetected { .. }));
        assert!(valid);
    }

    #[test]
    fn missing_locale_returns_sync_violation() {
        let store = InMemoryNoticeStateStore::new();
        let (emitter, _) = make_emitter_with_inmem_sink(store);
        // Only PT-BR provided — missing EN + ES.
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
        let valid = matches!(result, Err(NoticeEmitterError::LocaleSyncViolation { .. }));
        assert!(valid);
    }

    #[test]
    fn missing_native_speaker_review_returns_err() {
        let store = InMemoryNoticeStateStore::new();
        let (emitter, _) = make_emitter_with_inmem_sink(store);
        let mut contents = BTreeMap::new();
        contents.insert(NoticeLocale::PtBr, "PT".to_string());
        contents.insert(NoticeLocale::EnUs, "EN".to_string());
        contents.insert(NoticeLocale::EsMx, "ES".to_string());
        // es-MX review missing.
        let mut reviewed = BTreeMap::new();
        reviewed.insert(NoticeLocale::PtBr, true);
        reviewed.insert(NoticeLocale::EnUs, true);
        reviewed.insert(NoticeLocale::EsMx, false);
        let req = NoticePublishRequest::new(
            NoticeVersion::new(1, 0),
            contents,
            0,
            Uuid::nil(),
            Uuid::nil(),
            reviewed,
        );
        let result = emitter.publish(req);
        let valid = matches!(
            result,
            Err(NoticeEmitterError::NativeSpeakerReviewMissing { .. })
        );
        assert!(valid);
    }

    #[test]
    fn notice_text_hashes_deterministic_in_decision() {
        let (emitter, _) = make_emitter_with_inmem_sink(InMemoryNoticeStateStore::new());
        let req = make_request(NoticeVersion::new(1, 0), None);
        let decision = emitter.publish(req).unwrap();
        // All 3 locales present in hashes.
        let hashes = decision.notice_text_hashes();
        assert_eq!(hashes.len(), 3);
        for locale in canonical_notice_locales() {
            assert!(
                hashes.contains_key(locale.as_str()),
                "missing locale hash: {locale}"
            );
        }
    }
}
