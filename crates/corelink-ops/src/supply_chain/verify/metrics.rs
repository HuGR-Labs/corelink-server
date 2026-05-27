//! Prometheus metric counters and histograms for SLSA L3 provenance operations (WI-S12-001).
//!
//! Four metrics per `observability_model.md §4.1` (label `plan` per §3.1):
//!
//! | Metric | Type | Labels |
//! |--------|------|--------|
//! | `corelink_supply_slsa_attestations_total` | Counter | `outcome`, `plan` |
//! | `corelink_supply_slsa_attestation_generation_duration_seconds_bucket` | Histogram | `plan` |
//! | `corelink_supply_rekor_inclusion_proof_verify_total` | Counter | `outcome` |
//! | `corelink_supply_fulcio_chain_validate_total` | Counter | `outcome` |
//!
//! Metrics are emitted via structured tracing events at INFO level (Prometheus scrape
//! adapter on the collector side parses the structured fields). In production, the OTel
//! collector translates these to Prometheus-format counters.
//!
//! This module is intentionally free of ring/C dependencies to remain wasm32-clean.

/// Outcome label for `corelink_supply_slsa_attestations_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlsaAttestationOutcome {
    /// Attestation generated or verified successfully.
    Ok,
    /// Fulcio chain validation failed.
    FulcioFail,
    /// Rekor inclusion proof missing or invalid.
    RekorFail,
    /// DSSE envelope signature invalid (includes alg=none).
    SignFail,
    /// SLSA L3 generator detected non-hermetic build (network call during compile).
    HermeticViolation,
    /// Builder identity mismatch (forge attempt from fork).
    BuilderMismatch,
    /// in-toto schema drift (old predicateType).
    SchemaDrift,
}

impl SlsaAttestationOutcome {
    /// String representation for use as a Prometheus label value.
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::FulcioFail => "fulcio_fail",
            Self::RekorFail => "rekor_fail",
            Self::SignFail => "sign_fail",
            Self::HermeticViolation => "hermetic_violation",
            Self::BuilderMismatch => "builder_mismatch",
            Self::SchemaDrift => "schema_drift",
        }
    }
}

/// Outcome label for `corelink_supply_rekor_inclusion_proof_verify_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RekorVerifyOutcome {
    /// Inclusion proof verified successfully.
    Ok,
    /// Bundle not present in attestation.
    Missing,
    /// Bundle present but Merkle proof invalid.
    Invalid,
    /// Merkle root hash mismatch.
    MerkleMismatch,
}

impl RekorVerifyOutcome {
    /// String representation for use as a Prometheus label value.
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::MerkleMismatch => "merkle_mismatch",
        }
    }
}

/// Outcome label for `corelink_supply_fulcio_chain_validate_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FulcioValidateOutcome {
    /// Chain validated successfully.
    Ok,
    /// Root certificate does not match TUF-pinned root.
    RootPinFail,
    /// Intermediate certificate chain broken.
    IntermediateFail,
    /// Certificate validity window has passed.
    Expired,
}

impl FulcioValidateOutcome {
    /// String representation for use as a Prometheus label value.
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::RootPinFail => "root_pin_fail",
            Self::IntermediateFail => "intermediate_fail",
            Self::Expired => "expired",
        }
    }
}

/// Emit `corelink_supply_slsa_attestations_total` counter increment.
///
/// Structured tracing event at INFO level, with fields matching the Prometheus counter.
/// The `plan` label is the CoreLink tenant plan identifier (e.g., `"free"`, `"team"`, `"plus"`).
pub fn record_slsa_attestation(outcome: SlsaAttestationOutcome, plan: &str) {
    tracing::info!(
        metric = "corelink_supply_slsa_attestations_total",
        outcome = outcome.as_label(),
        plan = plan,
        value = 1u64,
        "SLSA attestation counter increment"
    );
}

/// Emit `corelink_supply_rekor_inclusion_proof_verify_total` counter increment.
pub fn record_rekor_verify(outcome: RekorVerifyOutcome) {
    tracing::info!(
        metric = "corelink_supply_rekor_inclusion_proof_verify_total",
        outcome = outcome.as_label(),
        value = 1u64,
        "Rekor inclusion proof verify counter increment"
    );
}

/// Emit `corelink_supply_fulcio_chain_validate_total` counter increment.
pub fn record_fulcio_validate(outcome: FulcioValidateOutcome) {
    tracing::info!(
        metric = "corelink_supply_fulcio_chain_validate_total",
        outcome = outcome.as_label(),
        value = 1u64,
        "Fulcio chain validate counter increment"
    );
}

/// Emit `corelink_supply_slsa_attestation_generation_duration_seconds_bucket` histogram sample.
///
/// `duration_ms` is the elapsed milliseconds for the attestation generation or verify operation.
/// The `plan` label is the CoreLink tenant plan identifier.
pub fn record_attestation_duration_ms(duration_ms: u64, plan: &str) {
    tracing::info!(
        metric = "corelink_supply_slsa_attestation_generation_duration_seconds_bucket",
        duration_ms = duration_ms,
        plan = plan,
        "SLSA attestation duration sample"
    );
}
