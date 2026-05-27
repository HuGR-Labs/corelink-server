//! Metrics observer trait + in-memory recorder.
//!
//! Per `WI-S03-006 §6.1.7 + §21` the canonical metric set is:
//!
//! - `corelink.auth.webauthn.registration_total{result}`
//! - `corelink.auth.webauthn.authentication_total{result}`
//! - `corelink.auth.webauthn.duration_ms_bucket{ceremony, browser}`
//! - `corelink.auth.webauthn.aaguid_count{aaguid}`
//! - `corelink.auth.webauthn.sign_count_regression_total`
//! - `corelink.auth.webauthn.admin_op_step_up_total{op}`
//!
//! Underscore form is canonical per `observability_model.md §4.1`;
//! the dot-form labels are the documentation surface only.

use std::collections::HashMap;
use std::sync::Mutex;

use super::Aaguid;

/// Result label for a ceremony (matches the canonical
/// `result ∈ {ok, attestation_failed, aaguid_denied, …}`
/// taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CeremonyResult {
    /// Ceremony succeeded.
    Ok,
    /// Attestation chain validation failed.
    AttestationFailed,
    /// AAGUID denylisted.
    AaguidDenied,
    /// AAGUID missing from allowlist.
    AaguidNotAllowed,
    /// Origin not in allowlist.
    OriginMismatch,
    /// RP-ID mismatch.
    RpIdMismatch,
    /// User-Verification flag missing in admin step-up.
    UvRequired,
    /// User-Presence flag missing.
    UpRequired,
    /// Sign-count regression.
    SignCountRegression,
    /// Challenge expired or unknown.
    ChallengeInvalid,
    /// Signature verify failed.
    SignatureInvalid,
    /// Unknown / catch-all.
    Other,
}

impl CeremonyResult {
    /// Canonical label string.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::AttestationFailed => "attestation_failed",
            Self::AaguidDenied => "aaguid_denied",
            Self::AaguidNotAllowed => "aaguid_not_allowed",
            Self::OriginMismatch => "origin_mismatch",
            Self::RpIdMismatch => "rp_id_mismatch",
            Self::UvRequired => "uv_required",
            Self::UpRequired => "up_required",
            Self::SignCountRegression => "sign_count_regression",
            Self::ChallengeInvalid => "challenge_invalid",
            Self::SignatureInvalid => "signature_invalid",
            Self::Other => "other",
        }
    }
}

/// Sink trait.
pub trait MetricsObserver: Send + Sync + std::fmt::Debug {
    /// Increment `registration_total{result}`.
    fn record_registration(&self, result: CeremonyResult);
    /// Increment `authentication_total{result}`.
    fn record_authentication(&self, result: CeremonyResult);
    /// Record a duration sample (ms; bucket bounds owned by the
    /// downstream Prometheus exporter).
    fn record_duration_ms(&self, ceremony_label: &str, duration_ms: u64);
    /// Increment `aaguid_count{aaguid}`.
    fn record_aaguid(&self, aaguid: Aaguid);
    /// Increment `sign_count_regression_total`.
    fn record_sign_count_regression(&self);
    /// Increment `admin_op_step_up_total{op}`.
    fn record_admin_step_up(&self, op_label: &str);
}

/// No-op recorder (canonical default).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMetrics;

impl MetricsObserver for NoopMetrics {
    fn record_registration(&self, _: CeremonyResult) {}
    fn record_authentication(&self, _: CeremonyResult) {}
    fn record_duration_ms(&self, _: &str, _: u64) {}
    fn record_aaguid(&self, _: Aaguid) {}
    fn record_sign_count_regression(&self) {}
    fn record_admin_step_up(&self, _: &str) {}
}

/// In-memory recorder used by tests + dashboard fixtures.
#[derive(Debug, Default)]
pub struct MetricsRecorder {
    inner: Mutex<MetricsRecorderInner>,
}

#[derive(Debug, Default)]
struct MetricsRecorderInner {
    registration_total: HashMap<CeremonyResult, u64>,
    authentication_total: HashMap<CeremonyResult, u64>,
    duration_samples: Vec<(String, u64)>,
    aaguid_count: HashMap<Aaguid, u64>,
    sign_count_regression_total: u64,
    admin_op_step_up_total: HashMap<String, u64>,
}

impl MetricsRecorder {
    /// Construct an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of `registration_total`.
    #[must_use]
    pub fn registration_total(&self, result: CeremonyResult) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.registration_total.get(&result).copied().unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Snapshot of `authentication_total`.
    #[must_use]
    pub fn authentication_total(&self, result: CeremonyResult) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.authentication_total.get(&result).copied().unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Snapshot of `aaguid_count`.
    #[must_use]
    pub fn aaguid_count(&self, aaguid: Aaguid) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.aaguid_count.get(&aaguid).copied().unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Snapshot of `sign_count_regression_total`.
    #[must_use]
    pub fn sign_count_regression_total(&self) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.sign_count_regression_total,
            Err(_) => 0,
        }
    }

    /// Snapshot of `admin_op_step_up_total{op}`.
    #[must_use]
    pub fn admin_op_step_up_total(&self, op: &str) -> u64 {
        match self.inner.lock() {
            Ok(g) => g.admin_op_step_up_total.get(op).copied().unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Number of duration samples recorded (test helper).
    #[must_use]
    pub fn duration_sample_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.duration_samples.len(),
            Err(_) => 0,
        }
    }
}

impl MetricsObserver for MetricsRecorder {
    fn record_registration(&self, result: CeremonyResult) {
        if let Ok(mut g) = self.inner.lock() {
            *g.registration_total.entry(result).or_insert(0) += 1;
        }
    }

    fn record_authentication(&self, result: CeremonyResult) {
        if let Ok(mut g) = self.inner.lock() {
            *g.authentication_total.entry(result).or_insert(0) += 1;
        }
    }

    fn record_duration_ms(&self, ceremony_label: &str, duration_ms: u64) {
        if let Ok(mut g) = self.inner.lock() {
            g.duration_samples.push((ceremony_label.to_owned(), duration_ms));
        }
    }

    fn record_aaguid(&self, aaguid: Aaguid) {
        if let Ok(mut g) = self.inner.lock() {
            *g.aaguid_count.entry(aaguid).or_insert(0) += 1;
        }
    }

    fn record_sign_count_regression(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.sign_count_regression_total += 1;
        }
    }

    fn record_admin_step_up(&self, op_label: &str) {
        if let Ok(mut g) = self.inner.lock() {
            *g.admin_op_step_up_total.entry(op_label.to_owned()).or_insert(0) += 1;
        }
    }
}
