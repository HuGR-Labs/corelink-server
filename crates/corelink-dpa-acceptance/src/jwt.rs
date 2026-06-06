//! RS256-signed JWT receipt for DPA acceptance.
//!
//! The signing key family is **disjoint** from the PAT format S-03
//! hybrid HMAC + Argon2id key family per `data_model.md §4.1` (key
//! confusion would couple rotation cadences and break either receipt
//! verify or PAT auth). Each tenant region carries a separate `kid`
//! provisioned via the secret rotation worker (S-13).

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::DpaAcceptanceError;
use crate::schema::{Jurisdiction, TenantId};

/// PEM-encoded RSA private key for signing receipts.
///
/// PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) or PKCS#8
/// (`-----BEGIN PRIVATE KEY-----`); `jsonwebtoken` accepts both.
#[derive(Clone, Debug)]
pub struct RsaPrivateKeyPem(pub String);

/// PEM-encoded RSA public key for verifying receipts.
#[derive(Clone, Debug)]
pub struct RsaPublicKeyPem(pub String);

/// Canonical JWT receipt claim set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JwtReceiptClaims {
    /// Subject — `tenant_id` as the canonical actor.
    pub sub: String,
    /// DPA version accepted.
    pub dpa_version: String,
    /// Server-assigned acceptance time (ms since epoch).
    pub accepted_at: i64,
    /// Jurisdiction (`EU`, `BR`, `LATAM`, `US`).
    pub jurisdiction: String,
    /// JWT ID — unique per receipt (replay protection sentinel for the
    /// verify path).
    pub jti: String,
    /// `iat` claim (seconds; required by `jsonwebtoken` validation).
    pub iat: i64,
    /// `exp` claim (seconds; 10y per receipt persistence policy).
    pub exp: i64,
}

impl JwtReceiptClaims {
    /// Build a receipt claim set with `exp = iat + 10y` (legal evidence
    /// retention window matches the R2 Object Lock 7y + grace).
    #[must_use]
    pub fn new(
        tenant_id: &TenantId,
        dpa_version: &str,
        accepted_at_ms: i64,
        jurisdiction: Jurisdiction,
        jti: &str,
    ) -> Self {
        let iat = accepted_at_ms / 1000;
        // 10 years in seconds (calendar approx; 315_360_000 = 10 * 365 * 86_400).
        let exp = iat.saturating_add(315_360_000);
        Self {
            sub: tenant_id.0.clone(),
            dpa_version: dpa_version.to_owned(),
            accepted_at: accepted_at_ms,
            jurisdiction: jurisdiction_str(jurisdiction).to_owned(),
            jti: jti.to_owned(),
            iat,
            exp,
        }
    }
}

fn jurisdiction_str(j: Jurisdiction) -> &'static str {
    match j {
        Jurisdiction::Eu => "EU",
        Jurisdiction::Br => "BR",
        Jurisdiction::Latam => "LATAM",
        Jurisdiction::Us => "US",
    }
}

/// Sign a receipt claim set with `RS256` using `kid` in the header.
///
/// # Errors
///
/// Returns [`DpaAcceptanceError::JwtSign`] on key parse or signing
/// failures.
pub fn sign_receipt(
    private_pem: &RsaPrivateKeyPem,
    kid: &str,
    claims: &JwtReceiptClaims,
) -> Result<String, DpaAcceptanceError> {
    let key = EncodingKey::from_rsa_pem(private_pem.0.as_bytes())
        .map_err(|e| DpaAcceptanceError::JwtSign(e.to_string()))?;
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    encode(&header, claims, &key).map_err(|e| DpaAcceptanceError::JwtSign(e.to_string()))
}

/// Verify a receipt JWT with `RS256` using the supplied PEM public key.
///
/// Returns the verified claims on success. `kid` mismatch / signature
/// mismatch / malformed token / expiry → [`DpaAcceptanceError::JwtVerify`].
///
/// # Errors
///
/// Returns [`DpaAcceptanceError::JwtVerify`] on any verification
/// failure (signature mismatch, malformed header, claim mismatch).
pub fn verify_receipt(
    public_pem: &RsaPublicKeyPem,
    expected_kid: Option<&str>,
    token: &str,
) -> Result<JwtReceiptClaims, DpaAcceptanceError> {
    let key = DecodingKey::from_rsa_pem(public_pem.0.as_bytes())
        .map_err(|e| DpaAcceptanceError::JwtVerify(e.to_string()))?;
    // Header inspection for kid match (fail-fast before signature
    // verify).
    if let Some(want_kid) = expected_kid {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| DpaAcceptanceError::JwtVerify(e.to_string()))?;
        match header.kid.as_deref() {
            Some(got) if got == want_kid => {}
            Some(other) => {
                return Err(DpaAcceptanceError::JwtVerify(format!(
                    "kid mismatch: expected={} got={}",
                    want_kid, other
                )));
            }
            None => {
                return Err(DpaAcceptanceError::JwtVerify(
                    "missing kid in header".to_owned(),
                ));
            }
        }
    }
    let validation = Validation::new(Algorithm::RS256);
    let data = decode::<JwtReceiptClaims>(token, &key, &validation)
        .map_err(|e| DpaAcceptanceError::JwtVerify(e.to_string()))?;
    Ok(data.claims)
}
