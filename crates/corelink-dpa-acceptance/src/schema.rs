//! Canonical schema for DPA acceptance (CTRL-PRIV-CONSENT-001..006).

use serde::{Deserialize, Serialize};

/// Tenant identifier (opaque slug; redacted in logs per CTRL-PRIV-001).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TenantId(pub String);

/// Signup identifier used as the idempotency key for
/// `POST /v1/dpa/accept`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SignupId(pub String);

/// Per-tenant request context (resolved before the orchestrator runs).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TenantCtx {
    /// Tenant id.
    pub tenant_id: TenantId,
    /// Signup id (idempotency key).
    pub signup_id: SignupId,
    /// Jurisdiction string (`"EU"`, `"BR"`, `"LATAM"`, `"US"`); embedded
    /// in the JWT receipt claims for audit.
    pub jurisdiction: Jurisdiction,
    /// Raw client IP (server-only; hashed before persistence per
    /// CTRL-PRIV-001 — never leaves this crate in plaintext).
    pub client_ip: String,
    /// Locale resolved by the upstream Next.js middleware from the
    /// `corelink_locale` cookie (Lote 10.16 canonical, NOT
    /// `Accept-Language`).
    pub resolved_locale: LocaleBcp47,
}

/// Closed jurisdiction enum (cardinality discipline per
/// INV-OBS-CARDINALITY-BUDGET).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Jurisdiction {
    /// European Union (GDPR Art. 28).
    Eu,
    /// Brazil (LGPD Art. 39).
    Br,
    /// Latin America (broader; LATAM Spanish + LGPD overlap).
    Latam,
    /// United States (CCPA §1798.140(v)).
    Us,
}

/// Closed 3-locale enum (BCP-47 subset; GA scope WI-S19-002).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum LocaleBcp47 {
    /// English (United States).
    #[serde(rename = "en-US")]
    EnUs,
    /// Portuguese (Brazil) — LGPD native.
    #[serde(rename = "pt-BR")]
    PtBr,
    /// Spanish (LATAM canonical).
    #[serde(rename = "es-419")]
    Es419,
}

impl LocaleBcp47 {
    /// BCP-47 string representation.
    #[must_use]
    pub const fn as_bcp47(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::PtBr => "pt-BR",
            Self::Es419 => "es-419",
        }
    }
}

/// 6-field canonical consent proof payload per
/// `specs/03_architecture/canonical/privacy_model.md` §5.6 +
/// CTRL-PRIV-CONSENT-001..006.
///
/// Field ↔ Control mapping:
///
/// | Field             | Control               |
/// |-------------------|-----------------------|
/// | `notice_text_hash`| CTRL-PRIV-CONSENT-001 |
/// | `notice_version`  | CTRL-PRIV-CONSENT-002 |
/// | `dpa_version`     | CTRL-PRIV-CONSENT-003 |
/// | `locale`          | CTRL-PRIV-CONSENT-004 / 005 (Lote 10.16) |
/// | `wording_id`      | CTRL-PRIV-CONSENT-006 |
/// | `ui_capture_ts`   | CTRL-PRIV-CONSENT-006 |
/// | `submission_ts`   | server-assigned (HuGR clock authoritative) |
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConsentProofPayload {
    /// SHA-256 hex64 of the rendered DPA notice text
    /// (CTRL-PRIV-CONSENT-001).
    pub notice_text_hash: String,
    /// Notice version (semver) per CTRL-PRIV-CONSENT-002.
    pub notice_version: String,
    /// DPA version (semver) per CTRL-PRIV-CONSENT-003.
    pub dpa_version: String,
    /// Locale captured client-side (from `corelink_locale` cookie per
    /// Lote 10.16 canonical) per CTRL-PRIV-CONSENT-004 / 005.
    pub locale: LocaleBcp47,
    /// Deterministic UUID v7 from `notice_version` hash per
    /// CTRL-PRIV-CONSENT-006.
    pub wording_id: String,
    /// Client-captured timestamp (ms since epoch, `Date.now()`).
    pub ui_capture_ts: i64,
    /// Server-assigned timestamp (ms since epoch); set by the
    /// orchestrator — request payloads carry `0` and are overwritten.
    pub submission_ts: i64,
}

/// Request body for `POST /v1/dpa/accept`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DpaAcceptanceRequest {
    /// 6-field proof payload (client-supplied; `submission_ts`
    /// overwritten server-side).
    pub proof: ConsentProofPayload,
}

/// Response body for `POST /v1/dpa/accept`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DpaAcceptanceReceipt {
    /// JWT receipt (RS256-signed). Returned in the response and emailed
    /// via the notification sink.
    pub jwt_receipt: String,
    /// JWT ID (`jti` claim) — primary key into `dpa_acceptances`.
    pub jti: String,
    /// Server-assigned submission timestamp (ms since epoch).
    pub accepted_at_ms: i64,
}
