//! Pseudonymization re-export for the canonical
//! `corelink-privacy-pseudonymize` helper. Per WI-S11-002 §6.3 the
//! helper lives in a separate crate so the S-14 BYOK refactor can
//! swap in a customer-controlled erasure_salt vault KMS surface
//! without touching the orchestrator.
//!
//! This module re-exports the public surface so internal modules
//! (`backends/*`, `report.rs`, `verification_job.rs`) can use the
//! canonical helper via `crate::pseudonymize::*` without depending on
//! the underlying crate path directly.

pub use corelink_privacy_pseudonymize::{
    pseudonymize, pseudonymize_subject_id, verify_pseudonym, PseudonymHash,
    PseudonymizationMarker, ERASURE_SALT_LEN, PII_REDACTED_MARKER_KEY,
    PII_REDACTED_MARKER_VALUE, PSEUDONYM_HEX_LEN,
};
