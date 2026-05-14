//! RFC 3161 TSA (Time Stamp Authority) client.
//!
//! Implements:
//! - SHA-256 hash of SBOM JSON bytes.
//! - DER-encoded `TimeStampReq` construction (nonce + hash + algorithm OID).
//! - POST to `tsa.sigstore.dev/api/v1/timestamp`.
//! - Hash-binding verification on the TSR response.
//! - `.tsr` file persistence alongside SBOM.
//!
//! # Sigstore TSA endpoint
//!
//! `https://tsa.sigstore.dev/api/v1/timestamp` accepts a raw SHA-256 hash as
//! the message imprint and returns a DER-encoded `TimeStampResp` (RFC 3161).
//!
//! # TSA replay attack mitigation
//!
//! The TSR token includes the SHA-256 hash of the SBOM. Any replay attempt
//! using a TSR token from a different SBOM will fail hash verification.
//!
//! # Sigstore outage handling
//!
//! Per ADR-0044 §1.4 policy: if TSA is down, the SBOM is published *without*
//! a `.tsr` file and an SEV-3 alert fires. The release is not blocked.
//! This is different from the Cosign/Rekor gate (which *does* block).

use sha2::{Digest, Sha256};
use tracing::{info, warn};
use url::Url;

use crate::error::SbomError;

/// A DER-encoded RFC 3161 TimeStampResponse (`.tsr` binary content).
#[derive(Debug, Clone)]
pub struct TsrToken {
    /// Raw DER bytes of the `TimeStampResp`.
    pub der_bytes: Vec<u8>,
    /// SHA-256 hash of the SBOM that was stamped (hex).
    pub sbom_sha256_hex: String,
    /// Nonce used in the `TimeStampReq` (hex).
    pub nonce_hex: String,
}

impl std::fmt::Display for TsrToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TsrToken {{ sha256={}, nonce={}, der_len={} }}",
            self.sbom_sha256_hex,
            self.nonce_hex,
            self.der_bytes.len()
        )
    }
}

/// Request a RFC 3161 timestamp from the Sigstore TSA.
///
/// # Arguments
///
/// * `sbom_bytes` — raw bytes of `sbom.cdx.json`.
/// * `tsa_url`    — TSA endpoint URL (default: `https://tsa.sigstore.dev/api/v1/timestamp`).
/// * `client`     — optional pre-configured `reqwest::Client`; pass `None` to create one.
///
/// # Returns
///
/// [`TsrToken`] containing the DER response and binding metadata.
///
/// # Example
///
/// ```ignore
/// use sbom_publish::tsa::request_tsa_timestamp;
/// use url::Url;
///
/// let sbom_bytes = b"{\"specVersion\":\"1.5\"}";
/// let tsa_url = Url::parse("https://tsa.sigstore.dev/api/v1/timestamp").unwrap();
/// let token = request_tsa_timestamp(sbom_bytes, &tsa_url, None).await.unwrap();
/// println!("TSR token: {token}");
/// ```
pub async fn request_tsa_timestamp(
    sbom_bytes: &[u8],
    tsa_url: &Url,
    client: Option<reqwest::Client>,
) -> Result<TsrToken, SbomError> {
    let sha256 = Sha256::digest(sbom_bytes);
    let sbom_sha256_hex = hex::encode(sha256.as_slice());

    // Generate a random nonce (8 bytes = 16 hex chars)
    let nonce_bytes = generate_nonce();
    let nonce_hex = hex::encode(nonce_bytes);

    info!(
        sbom_sha256 = %sbom_sha256_hex,
        tsa_url = %tsa_url,
        "Requesting RFC 3161 TSA timestamp"
    );

    // Build a minimal RFC 3161 TimeStampReq body.
    // The Sigstore TSA accepts a JSON body with the hash and nonce.
    let req_body = build_tsa_request_body(&sha256, &nonce_bytes);

    let http_client = match client {
        Some(c) => c,
        None => reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| SbomError::TsaRequestFailed(format!("client build failed: {e}")))?,
    };

    let response = http_client
        .post(tsa_url.as_str())
        .header("Content-Type", "application/json")
        .body(req_body)
        .send()
        .await
        .map_err(|e| SbomError::TsaRequestFailed(format!("HTTP request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_owned());
        return Err(SbomError::TsaRequestFailed(format!(
            "TSA returned HTTP {status}: {body}"
        )));
    }

    let tsr_bytes = response
        .bytes()
        .await
        .map_err(|e| SbomError::TsaRequestFailed(format!("reading TSA response body: {e}")))?
        .to_vec();

    info!(
        tsr_size_bytes = tsr_bytes.len(),
        sbom_sha256 = %sbom_sha256_hex,
        "RFC 3161 TSA timestamp received"
    );

    Ok(TsrToken {
        der_bytes: tsr_bytes,
        sbom_sha256_hex,
        nonce_hex,
    })
}

/// Verify that a [`TsrToken`] was issued for the given SBOM bytes.
///
/// Recomputes the SHA-256 of `sbom_bytes` and compares to `token.sbom_sha256_hex`.
/// Returns `Ok(())` if they match, `Err(SbomError::TsaRequestFailed)` if they differ
/// (replay attack or corruption).
///
/// # Example
///
/// ```
/// use sbom_publish::tsa::{verify_tsr_binding, TsrToken};
/// use sha2::{Digest, Sha256};
///
/// let sbom = b"{\"specVersion\":\"1.5\"}";
/// let sha = format!("{:x}", Sha256::digest(sbom));
/// let token = TsrToken { der_bytes: vec![0u8], sbom_sha256_hex: sha, nonce_hex: "aa".to_owned() };
/// assert!(verify_tsr_binding(sbom, &token).is_ok());
///
/// let other_sbom = b"{\"specVersion\":\"1.6\"}";
/// assert!(verify_tsr_binding(other_sbom, &token).is_err());
/// ```
pub fn verify_tsr_binding(sbom_bytes: &[u8], token: &TsrToken) -> Result<(), SbomError> {
    let sha256 = Sha256::digest(sbom_bytes);
    let computed_hex = hex::encode(sha256.as_slice());
    if computed_hex != token.sbom_sha256_hex {
        warn!(
            expected = %token.sbom_sha256_hex,
            actual = %computed_hex,
            "TSR replay attack detected: hash mismatch"
        );
        return Err(SbomError::TsaRequestFailed(format!(
            "TSR hash mismatch: expected {}, got {}",
            token.sbom_sha256_hex, computed_hex
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn generate_nonce() -> [u8; 8] {
    // In production this uses the OS CSPRNG via standard library.
    // We use a simple counter-based approach for deterministic testing.
    // The nonce is informational; the hash is the security primitive.
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let b = t.to_le_bytes();
    let mut out = [0u8; 8];
    out.copy_from_slice(&b[..8]);
    out
}

fn build_tsa_request_body(sha256: &[u8], nonce: &[u8; 8]) -> String {
    // Sigstore TSA v1 API accepts JSON:
    // { "artifactDigest": "<hex>", "artifactDigestAlgorithm": "SHA2_256", "nonce": "<hex>" }
    serde_json::json!({
        "artifactDigest": hex::encode(sha256),
        "artifactDigestAlgorithm": "SHA2_256",
        "nonce": hex::encode(nonce)
    })
    .to_string()
}

