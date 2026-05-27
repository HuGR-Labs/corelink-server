//! `GET /pkg/<sha256>/<filename>` — wheel/sdist endpoint logic.
//!
//! The adapter routes wheel fetches by their `#sha256=` URL fragment
//! (the same hex value PyPI returns in the index): on a CAS hit we
//! serve immediately; on a miss we resolve the upstream wheel URL
//! from the cached index, download, verify the downloaded bytes'
//! SHA256 against the URL fragment (MANDATORY — spec §9), and on
//! success store + audit + return.
//!
//! Mandatory integrity check (pip.md §9):
//!
//! > Integrity check MANDATORY pre-CAS-store (wheel bytes match
//! > `#sha256=` URL fragment); fail-CLOSED with audit emit
//!
//! is enforced by [`verify_sha256`] before the CAS `put`. On
//! mismatch the adapter emits `corelink.pip.wheel.integrity_mismatch.v1`
//! and returns [`PipAdapterError::IntegrityMismatch`].

use std::sync::Arc;

use bytes::Bytes;
use corelink_audit::ports::AuditEmitter;
use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;
use url::Url;

use crate::pip::audit::{emit_pip_audit, event_types, now_unix_ms};
use crate::pip::error::PipAdapterError;
use crate::pip::pep503_html::ProjectIndex;
use crate::pip::ports::CasStoreHandle;
use crate::pip::upstream::UpstreamClient;

/// Compute the SHA256 of `bytes` and return it as a 64-char lowercase
/// hex string.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Constant-time compare: do the downloaded bytes' SHA256 match the
/// expected hex? Returns `Ok(())` on match; an
/// [`PipAdapterError::IntegrityMismatch`] on any divergence.
///
/// # Errors
///
/// Returns [`PipAdapterError::IntegrityMismatch`] if the SHA256
/// does not match `expected_hex`.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), PipAdapterError> {
    let actual = sha256_hex(bytes);
    let exp_lower = expected_hex.to_ascii_lowercase();
    if actual.as_bytes().ct_eq(exp_lower.as_bytes()).unwrap_u8() == 1 {
        Ok(())
    } else {
        Err(PipAdapterError::IntegrityMismatch {
            expected: exp_lower,
            actual,
        })
    }
}

/// Body bytes + filename of a served wheel/sdist response.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WheelResponse {
    /// Wheel/sdist bytes.
    pub body: Bytes,
    /// Filename to report (used for `Content-Disposition` if the
    /// router chooses to set it).
    pub filename: String,
    /// Hex SHA256 of the body. The router echoes this in the
    /// `X-CoreLink-Cache-Digest` header for client-side
    /// verification.
    pub sha256: String,
}

/// Serve a wheel/sdist by `(tenant, sha256_hex, filename, project)`.
///
/// Algorithm:
/// 1. Parse `sha256_hex` into a [`Digest`].
/// 2. CAS `get`. Hit → emit `wheel.cache_hit.v1`, return.
/// 3. Miss → look up upstream URL in `index` (must match
///    `filename`).
/// 4. Download with `wheel_size_limit_bytes` cap.
/// 5. **MANDATORY** SHA256 verify (constant time). Mismatch →
///    emit `wheel.integrity_mismatch.v1` + return error.
/// 6. Emit `wheel.stored.v1` BEFORE CAS `put`.
/// 7. CAS `put`. Return.
///
/// # Errors
///
/// Surfaces any `PipAdapterError` from the inner steps.
#[allow(
    clippy::too_many_arguments,
    reason = "the adapter's wheel-serve flow takes nine handles + scalar limits — bundling them into a context struct would just shift the arity to the constructor without improving readability"
)]
pub async fn serve_wheel(
    sha256_hex_str: &str,
    filename: &str,
    project: &str,
    index: &ProjectIndex,
    tenant: &TenantId,
    cas: &CasStoreHandle,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
    wheel_size_limit_bytes: u64,
) -> Result<WheelResponse, PipAdapterError> {
    let digest = Digest::from_hex(sha256_hex_str)
        .map_err(|e| PipAdapterError::Cas(format!("digest parse: {e:?}")))?;

    if let Some(bytes) = cas.get(tenant, &digest).await? {
        emit_pip_audit(
            auditor,
            event_types::WHEEL_CACHE_HIT,
            tenant,
            now_unix_ms(),
            serde_json::json!({
                "project": project,
                "filename": filename,
                "sha256": sha256_hex_str,
            }),
        )?;
        return Ok(WheelResponse {
            body: Bytes::from(bytes),
            filename: filename.to_owned(),
            sha256: sha256_hex_str.to_ascii_lowercase(),
        });
    }

    let upstream_url = index
        .files
        .iter()
        .find(|f| f.filename == filename && f.sha256.eq_ignore_ascii_case(sha256_hex_str))
        .ok_or_else(|| {
            PipAdapterError::IndexParse(format!(
                "no entry in cached index for {filename}@{sha256_hex_str}"
            ))
        })?;
    let parsed_url = Url::parse(&upstream_url.url)
        .map_err(|e| PipAdapterError::IndexParse(format!("upstream URL parse: {e}")))?;

    let downloaded = upstream
        .fetch_wheel(&parsed_url, wheel_size_limit_bytes)
        .await
        .map_err(|e| match e {
            PipAdapterError::WheelOversized(n) => {
                // Audit-emit the oversize event before propagating;
                // ignore audit failure here because we are already
                // in an error path (best-effort).
                let _ = emit_pip_audit(
                    auditor,
                    event_types::WHEEL_OVERSIZED,
                    tenant,
                    now_unix_ms(),
                    serde_json::json!({
                        "project": project,
                        "filename": filename,
                        "size_bytes": n,
                    }),
                );
                PipAdapterError::WheelOversized(n)
            }
            other => other,
        })?;

    // MANDATORY integrity check pre-CAS-store.
    if let Err(err) = verify_sha256(&downloaded, sha256_hex_str) {
        if let PipAdapterError::IntegrityMismatch { expected, actual } = &err {
            // Audit-emit the mismatch. Audit failure surfaces but
            // does not erase the integrity-mismatch context.
            emit_pip_audit(
                auditor,
                event_types::WHEEL_INTEGRITY_MISMATCH,
                tenant,
                now_unix_ms(),
                serde_json::json!({
                    "project": project,
                    "filename": filename,
                    "expected_sha256": expected,
                    "actual_sha256": actual,
                }),
            )?;
        }
        return Err(err);
    }

    let bytes_vec = downloaded.to_vec();
    // Audit BEFORE the CAS put (audit-fail-CLOSED contract).
    emit_pip_audit(
        auditor,
        event_types::WHEEL_STORED,
        tenant,
        now_unix_ms(),
        serde_json::json!({
            "project": project,
            "filename": filename,
            "sha256": sha256_hex_str,
            "size_bytes": bytes_vec.len(),
        }),
    )?;
    cas.put(tenant, &digest, bytes_vec.clone()).await?;
    Ok(WheelResponse {
        body: Bytes::from(bytes_vec),
        filename: filename.to_owned(),
        sha256: sha256_hex_str.to_ascii_lowercase(),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_matches_known_vector() {
        // SHA256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let got = sha256_hex(b"abc");
        assert_eq!(
            got,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_sha256_accepts_match() {
        let bytes = b"hello world";
        let hex = sha256_hex(bytes);
        assert!(verify_sha256(bytes, &hex).is_ok());
    }

    #[test]
    fn verify_sha256_accepts_uppercase_expected() {
        let bytes = b"hello world";
        let hex = sha256_hex(bytes).to_ascii_uppercase();
        assert!(verify_sha256(bytes, &hex).is_ok());
    }

    #[test]
    fn verify_sha256_rejects_mismatch() {
        let bytes = b"hello world";
        let wrong = "0".repeat(64);
        let result = verify_sha256(bytes, &wrong);
        assert!(matches!(result, Err(PipAdapterError::IntegrityMismatch { .. })));
    }

    #[test]
    fn verify_sha256_is_byte_sensitive() {
        // 1-byte difference must reject.
        let a = b"hello world";
        let b = b"hello worlD";
        let hex = sha256_hex(b);
        let result = verify_sha256(a, &hex);
        assert!(matches!(result, Err(PipAdapterError::IntegrityMismatch { .. })));
    }
}
