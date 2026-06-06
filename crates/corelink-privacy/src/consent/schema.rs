//! Schema types for the Consent Ledger.
//!
//! - [`ConsentPurpose`] — 12-arm closed enum (privacy_model.md §5.6.1;
//!   ADR-S11-006 cardinality discipline). Any new purpose requires an ADR.
//! - [`LegalBasis`] — 4-arm enum; mapping per purpose is **fixed** (NO
//!   dynamic swap; corrige GPT P0-1 round-1).
//! - [`LocaleBcp47`] — 3-locale closed enum (pt-BR / en-US / es-MX at GA).
//! - [`ConsentProofPayload`] — 6-field canonical proof of informed consent
//!   (notice_text_hash + notice_version + locale + wording_id +
//!   ui_capture_ts + submission_ts). Schema used **symmetrically** in
//!   grant and revoke (Lote 9.4 Opus H-05).

use serde::{Deserialize, Serialize};

// ── Locale ──────────────────────────────────────────────────────────────────

/// BCP-47 locale closed enum. 3 canonical at GA.
///
/// Strict locale enforcement (CTRL-PRIV-CONSENT-005): HTTP
/// `Accept-Language` header MUST match `payload.locale`; mismatch → 422.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum LocaleBcp47 {
    /// Brazilian Portuguese.
    #[serde(rename = "pt-BR")]
    PtBr,
    /// US English.
    #[serde(rename = "en-US")]
    EnUs,
    /// Mexican Spanish.
    #[serde(rename = "es-MX")]
    EsMx,
}

impl LocaleBcp47 {
    /// BCP-47 string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PtBr => "pt-BR",
            Self::EnUs => "en-US",
            Self::EsMx => "es-MX",
        }
    }
}

impl std::fmt::Display for LocaleBcp47 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Legal basis ─────────────────────────────────────────────────────────────

/// Canonical legal basis enum (privacy_model.md §5.6.1).
///
/// Mapping per [`ConsentPurpose`] is **fixed**. NO dynamic swap between
/// `consent` and `legitimate_interest` on consent expiry — corrige GPT
/// P0-1 round-1 regression. When consent lapses, processing STOPS;
/// purpose enters `consent_lapsed` state (AC-006).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LegalBasis {
    /// LGPD Art. 7§V / GDPR 6(1)(b).
    Contract,
    /// LGPD Art. 7§II / GDPR 6(1)(c).
    LegalObligation,
    /// LGPD Art. 10 / GDPR 6(1)(f). LIA required.
    LegitimateInterest,
    /// LGPD Art. 7§I / GDPR 6(1)(a). Revocable; ≤5min p95.
    Consent,
}

impl std::fmt::Display for LegalBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Contract => "contract",
            Self::LegalObligation => "legal_obligation",
            Self::LegitimateInterest => "legitimate_interest",
            Self::Consent => "consent",
        };
        f.write_str(s)
    }
}

// ── Consent purpose ─────────────────────────────────────────────────────────

/// 12 canonical consent purposes (privacy_model.md §5.6.1 source-of-truth
/// pós Lote 10.11.0-bis).
///
/// Ordered: 3 contract + 1 legal_obligation + 2 legitimate_interest +
/// 6 consent (revocable).
///
/// Adding new purposes requires an ADR (ADR-S11-006 cardinality discipline).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsentPurpose {
    // ── basis = contract ────────────────────────────────────────────────────
    /// Core CAS/AC/exec operations (no opt-out without contract termination).
    ServiceDelivery,
    /// Auth, billing, tenant admin (no opt-out without contract termination).
    AccountManagement,
    // ── basis = legal_obligation ────────────────────────────────────────────
    /// Audit retention, DSR fulfillment, breach reporting, sub-processor
    /// notifications.
    RegulatoryCompliance,
    // ── basis = legitimate_interest ─────────────────────────────────────────
    /// Anomaly detection, abuse prevention, fraud prevention.
    SecurityMonitoring,
    /// Anonymised cross-tenant metrics (k≥50, privacy budget).
    AnalyticsAggregated,
    // ── basis = consent (revocable ≤5min) ──────────────────────────────────
    /// Per-tenant dashboards with re-identifiable PII.
    AnalyticsPersonalized,
    /// Newsletter, product updates.
    MarketingEmail,
    /// Surveys, NPS, user research.
    MarketingResearch,
    /// Preview/experimental features (enriched telemetry).
    BetaFeatures,
    /// Stripe, GitHub, custom webhooks (data egress controlled; per-integration).
    ThirdPartyIntegrations,
    /// Share aggregated metrics in leaderboards/benchmarks.
    CrossTenantBenchmarks,
    /// Opt-in for ML cache prediction (data minimised + k-anon).
    TrainingMlModels,
}

impl ConsentPurpose {
    /// Fixed legal basis for this purpose.
    ///
    /// Mapping is canonical and MUST NOT be changed dynamically.
    #[must_use]
    pub fn legal_basis(&self) -> LegalBasis {
        match self {
            Self::ServiceDelivery | Self::AccountManagement => LegalBasis::Contract,
            Self::RegulatoryCompliance => LegalBasis::LegalObligation,
            Self::SecurityMonitoring | Self::AnalyticsAggregated => LegalBasis::LegitimateInterest,
            Self::AnalyticsPersonalized
            | Self::MarketingEmail
            | Self::MarketingResearch
            | Self::BetaFeatures
            | Self::ThirdPartyIntegrations
            | Self::CrossTenantBenchmarks
            | Self::TrainingMlModels => LegalBasis::Consent,
        }
    }

    /// Returns `true` if and only if the purpose's legal basis is
    /// `Consent` and hence the purpose is revocable.
    ///
    /// Non-revocable purposes (Contract / LegalObligation /
    /// LegitimateInterest) MUST NOT accept a revoke request via the
    /// DELETE endpoint.
    #[must_use]
    pub fn is_revocable(&self) -> bool {
        matches!(self.legal_basis(), LegalBasis::Consent)
    }

    /// SQL `CHECK` string value used in Neon migrations.
    #[must_use]
    pub fn as_sql_value(&self) -> &'static str {
        match self {
            Self::ServiceDelivery => "service_delivery",
            Self::AccountManagement => "account_management",
            Self::RegulatoryCompliance => "regulatory_compliance",
            Self::SecurityMonitoring => "security_monitoring",
            Self::AnalyticsAggregated => "analytics_aggregated",
            Self::AnalyticsPersonalized => "analytics_personalized",
            Self::MarketingEmail => "marketing_email",
            Self::MarketingResearch => "marketing_research",
            Self::BetaFeatures => "beta_features",
            Self::ThirdPartyIntegrations => "third_party_integrations",
            Self::CrossTenantBenchmarks => "cross_tenant_benchmarks",
            Self::TrainingMlModels => "training_ml_models",
        }
    }
}

impl std::fmt::Display for ConsentPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_sql_value())
    }
}

/// Returns all 12 canonical purposes in canonical order.
#[must_use]
pub fn canonical_consent_purposes() -> Vec<ConsentPurpose> {
    vec![
        ConsentPurpose::ServiceDelivery,
        ConsentPurpose::AccountManagement,
        ConsentPurpose::RegulatoryCompliance,
        ConsentPurpose::SecurityMonitoring,
        ConsentPurpose::AnalyticsAggregated,
        ConsentPurpose::AnalyticsPersonalized,
        ConsentPurpose::MarketingEmail,
        ConsentPurpose::MarketingResearch,
        ConsentPurpose::BetaFeatures,
        ConsentPurpose::ThirdPartyIntegrations,
        ConsentPurpose::CrossTenantBenchmarks,
        ConsentPurpose::TrainingMlModels,
    ]
}

// ── Proof payload ────────────────────────────────────────────────────────────

/// 6-field canonical proof of informed consent (CTRL-PRIV-CONSENT-001).
///
/// Schema used **symmetrically** in grant and revoke (Lote 9.4 Opus H-05).
/// Grant proof carries the consent notice fields; revoke proof carries the
/// unsubscribe notice fields in force at revocation time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConsentProofPayload {
    /// SHA-256 hex-64 of the exact notice text shown to the subject.
    pub notice_text_hash: String,
    /// Semver `"1.2.0"` — major bump forces re-consent.
    pub notice_version: String,
    /// BCP-47 locale the subject saw the notice in.
    pub locale: LocaleBcp47,
    /// UUIDv7 of the A/B wording variant (e.g. `"consent-analytics-v3"`).
    pub wording_id: String,
    /// ISO 8601 UTC timestamp when the UI rendered the notice (browser clock).
    pub ui_capture_ts: String,
    /// ISO 8601 UTC timestamp when the server received the submission
    /// (HuGR clock authoritative).
    pub submission_ts: String,
}

// ── HMAC signature wrapper ───────────────────────────────────────────────────

/// HMAC-SHA256 hex-64 per-consent signature (tenant-scoped via HKDF).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HmacSignatureValue(pub String);

/// Revocation record identifier (ULID 26 chars).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RevocationId(pub String);

// ── Receipts ──────────────────────────────────────────────────────────────────

/// Response returned by `POST /v1/consent/<purpose>` (grant).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsentGrantReceipt {
    /// ULID of the newly inserted (or existing, on replay) consent record.
    pub consent_id: String,
    /// HMAC-SHA256 hex-64 tenant-scoped signature.
    pub signature: String,
    /// Stateless verify URL for subject self-service audit.
    pub verify_url: String,
    /// `true` if this is a replay of an already-captured consent.
    pub replay: bool,
}

/// Response returned by `DELETE /v1/consent/<purpose>` (revoke).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsentRevokeReceipt {
    /// ULID of the newly inserted revocation record.
    pub revocation_id: String,
    /// HMAC-SHA256 hex-64 tenant-scoped signature.
    pub signature: String,
    /// ISO 8601 UTC timestamp ≤24h after which cascade is guaranteed complete.
    pub cascade_eta: String,
}

/// Response returned by `GET /v1/consent/verify` or
/// `GET /v1/consent/revocation/verify`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerifyResponse {
    /// `true` if the HMAC signature is valid.
    pub valid: bool,
    /// Consent or revocation record ID.
    pub record_id: String,
    /// Purpose string.
    pub purpose: String,
    /// Notice semver.
    pub notice_version: String,
    /// BCP-47 locale.
    pub locale: String,
    /// ISO 8601 UTC submission timestamp.
    pub submission_ts: String,
    /// Tenant short identifier (from the HMAC KID).
    pub tenant_short_id: String,
    /// Error message if `valid = false`.
    pub error: Option<String>,
}

/// Single consent entry in the list response.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsentEntry {
    /// Consent record ID (ULID).
    pub consent_id: String,
    /// Purpose.
    pub purpose: String,
    /// Legal basis.
    pub basis_legal: String,
    /// 6-field proof.
    pub proof: ConsentProofPayload,
    /// HMAC signature.
    pub signature: String,
    /// Stateless verify URL.
    pub verify_url: String,
}

/// Single revocation entry in the list response.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RevocationEntry {
    /// Revocation record ID (ULID).
    pub revocation_id: String,
    /// Purpose.
    pub purpose: String,
    /// 6-field revoke proof.
    pub proof: ConsentProofPayload,
    /// HMAC signature.
    pub signature: String,
    /// Cascade status.
    pub cascade_status: String,
}

/// Response returned by `GET /v1/consent`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsentListResponse {
    /// All grant records for the subject, ordered submission_ts DESC.
    pub consents: Vec<ConsentEntry>,
    /// All revocation records for the subject, ordered submission_ts DESC.
    pub revocations: Vec<RevocationEntry>,
}
