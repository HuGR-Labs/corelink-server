//! `GET /<pkg>/-/<tarball>.tgz` — tarball CAS-backed cache.
//!
//! Tarballs are immutable in the npm registry (republish blocked
//! upstream). They are stored in CAS keyed by BLAKE3 of the tarball
//! URL (URL is the canonical identity for npm tarballs; the URL
//! itself contains `<pkg>/-/<pkg>-<version>.tgz`).
//!
//! Mandatory integrity check (npm.md §3 / §9):
//!
//! > Tarball integrity verified post-download (hash matches
//! > metadata-published hash) BEFORE storing in CAS; fail-CLOSED
//! > with audit emit on mismatch.
//!
//! is enforced by [`verify_sha1`] before the CAS `put`. On mismatch
//! the adapter emits `corelink.npm.tarball.integrity_mismatch.v1`
//! (fail-CLOSED) and returns [`NpmAdapterError::IntegrityMismatch`].
//!
//! npm's `dist.shasum` is a SHA1 hex string. This module uses SHA1
//! to match upstream semantics; the `subtle::ConstantTimeEq` compare
//! prevents timing leaks.

use std::sync::Arc;

use bytes::Bytes;
use corelink_audit::ports::AuditEmitter;
use corelink_core::types::digest::Digest;
use corelink_core::types::tenant::TenantId;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;
use url::Url;

use crate::npm::audit::{emit_npm_audit, event_types, now_unix_ms};
use crate::npm::error::NpmAdapterError;
use crate::npm::ports::CasStoreHandle;
use crate::npm::upstream::UpstreamClient;

/// Compute SHA256 of `bytes` and return it as a 64-char lowercase hex
/// string. Used for CAS key derivation.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Compute SHA1 of `bytes` and return it as a 40-char lowercase hex
/// string. Used to verify against npm `dist.shasum`.
#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> String {
    // SHA1 via sha2 crate is not available; use a manual fallback
    // via the `sha1` algorithm from the `sha2`-compatible interface.
    // We use SHA256 stored in npm's `dist.integrity` field when
    // available, but `dist.shasum` is always SHA1.
    // Here we implement a pure-Rust SHA1 using the `sha1` digest.
    // Since the `sha2` workspace dep doesn't include sha1, we compute
    // SHA256 of the URL as CAS key and verify against dist.shasum
    // using the sha1 algorithm implemented inline below.
    sha1_digest(bytes)
}

/// Minimal SHA1 digest over `data`. Returns 40-char lowercase hex.
/// This avoids an extra crate dep while satisfying the npm `dist.shasum`
/// verification requirement.
///
/// Implemented according to FIPS PUB 180-4 §6.1 using only safe
/// array access via `get` / slice patterns to satisfy
/// `clippy::indexing_slicing = "deny"`.
fn sha1_digest(data: &[u8]) -> String {
    // SHA1 initial hash values (FIPS PUB 180-4 §5.3.1).
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    // Pad: append 0x80, then zeros, then 8-byte big-endian length.
    let mut msg: Vec<u8> = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for block in msg.chunks_exact(64) {
        // Message schedule W[0..=79].
        let mut w = [0u32; 80];
        for (slot, chunk) in w.iter_mut().zip(block.chunks_exact(4)).take(16) {
            if let [b0, b1, b2, b3] = *chunk {
                *slot = u32::from_be_bytes([b0, b1, b2, b3]);
            }
        }
        // Expand W[16..=79].
        for i in 16..80usize {
            // All indices in bounds because w has len 80 and i ≥ 16.
            let val = *w.get(i.wrapping_sub(3)).unwrap_or(&0)
                ^ *w.get(i.wrapping_sub(8)).unwrap_or(&0)
                ^ *w.get(i.wrapping_sub(14)).unwrap_or(&0)
                ^ *w.get(i.wrapping_sub(16)).unwrap_or(&0);
            if let Some(slot) = w.get_mut(i) {
                *slot = val.rotate_left(1);
            }
        }

        let (mut a, mut b, mut c, mut d, mut e) = (
            *h.first().unwrap_or(&0),
            *h.get(1).unwrap_or(&0),
            *h.get(2).unwrap_or(&0),
            *h.get(3).unwrap_or(&0),
            *h.get(4).unwrap_or(&0),
        );

        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDCu32),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        if let Some(h0) = h.get_mut(0) {
            *h0 = h0.wrapping_add(a);
        }
        if let Some(h1) = h.get_mut(1) {
            *h1 = h1.wrapping_add(b);
        }
        if let Some(h2) = h.get_mut(2) {
            *h2 = h2.wrapping_add(c);
        }
        if let Some(h3) = h.get_mut(3) {
            *h3 = h3.wrapping_add(d);
        }
        if let Some(h4) = h.get_mut(4) {
            *h4 = h4.wrapping_add(e);
        }
    }

    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}",
        h.first().copied().unwrap_or(0),
        h.get(1).copied().unwrap_or(0),
        h.get(2).copied().unwrap_or(0),
        h.get(3).copied().unwrap_or(0),
        h.get(4).copied().unwrap_or(0),
    )
}

/// Derive the CAS `Digest` for a tarball URL. CAS key = BLAKE3 of the
/// tarball URL string (URL is immutable in npm; the URL itself is the
/// canonical identity).
///
/// We use SHA256 of the URL as a stable CAS key because BLAKE3 is
/// not yet a workspace dep; SHA256 provides sufficient collision
/// resistance for a URL-based key.
///
/// # Errors
///
/// Returns [`NpmAdapterError::Cas`] if the resulting hex string
/// cannot be parsed into a [`Digest`].
pub fn tarball_url_digest(tarball_url: &str) -> Result<Digest, NpmAdapterError> {
    let key_hex = sha256_hex(tarball_url.as_bytes());
    Digest::from_hex(&key_hex).map_err(|e| NpmAdapterError::Cas(format!("digest parse: {e:?}")))
}

/// Constant-time compare: do the downloaded tarball bytes' SHA1 match
/// the expected `dist.shasum` hex?
///
/// # Errors
///
/// Returns [`NpmAdapterError::IntegrityMismatch`] on mismatch.
pub fn verify_sha1(bytes: &[u8], expected_hex: &str) -> Result<(), NpmAdapterError> {
    let actual = sha1_hex(bytes);
    let exp_lower = expected_hex.to_ascii_lowercase();
    if actual.as_bytes().ct_eq(exp_lower.as_bytes()).unwrap_u8() == 1 {
        Ok(())
    } else {
        Err(NpmAdapterError::IntegrityMismatch {
            expected: exp_lower,
            actual,
        })
    }
}

/// Body bytes of a served tarball response.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TarballResponse {
    /// Tarball bytes.
    pub body: Bytes,
    /// CAS digest key (hex-encoded SHA256 of the tarball URL).
    pub digest_hex: String,
}

/// Serve a tarball for `(tenant, pkg, version, tarball_filename)`.
///
/// Algorithm:
/// 1. Derive CAS key from tarball URL.
/// 2. CAS `get`. Hit → emit `tarball.cache_hit.v1`, return.
/// 3. Miss → fetch from upstream with size cap.
/// 4. **MANDATORY** SHA1 verify vs `dist_shasum` (constant time). Mismatch
///    → emit `tarball.integrity_mismatch.v1` (fail-CLOSED) + return error.
/// 5. Emit `tarball.stored.v1` BEFORE CAS `put`.
/// 6. CAS `put`. Return.
///
/// # Errors
///
/// Surfaces any [`NpmAdapterError`] from the inner steps.
#[allow(
    clippy::too_many_arguments,
    reason = "tarball-serve flow requires all handles + scalar limits"
)]
pub async fn serve_tarball(
    tarball_url_str: &str,
    pkg: &str,
    version: &str,
    dist_shasum: &str,
    tenant: &TenantId,
    cas: &CasStoreHandle,
    upstream: &UpstreamClient,
    auditor: &Arc<dyn AuditEmitter>,
    tarball_size_limit_bytes: u64,
) -> Result<TarballResponse, NpmAdapterError> {
    let digest = tarball_url_digest(tarball_url_str)?;
    let digest_hex = hex::encode(digest.as_bytes());

    if let Some(bytes) = cas.get(tenant, &digest).await? {
        emit_npm_audit(
            auditor,
            event_types::TARBALL_CACHE_HIT,
            tenant,
            now_unix_ms(),
            serde_json::json!({
                "pkg": pkg,
                "version": version,
                "digest": digest_hex,
            }),
        )?;
        return Ok(TarballResponse {
            body: Bytes::from(bytes),
            digest_hex,
        });
    }

    let tarball_url = Url::parse(tarball_url_str)
        .map_err(|e| NpmAdapterError::Upstream(format!("tarball URL parse: {e}")))?;

    let downloaded = upstream
        .fetch_tarball(&tarball_url, tarball_size_limit_bytes)
        .await
        .map_err(|e| match e {
            NpmAdapterError::TarballOversized(n) => {
                // Best-effort audit emit on the oversize path.
                let _ = emit_npm_audit(
                    auditor,
                    event_types::TARBALL_OVERSIZED,
                    tenant,
                    now_unix_ms(),
                    serde_json::json!({
                        "pkg": pkg,
                        "version": version,
                        "size_bytes": n,
                    }),
                );
                NpmAdapterError::TarballOversized(n)
            }
            other => other,
        })?;

    // MANDATORY integrity check pre-CAS-store (fail-CLOSED).
    if let Err(err) = verify_sha1(&downloaded, dist_shasum) {
        if let NpmAdapterError::IntegrityMismatch { expected, actual } = &err {
            emit_npm_audit(
                auditor,
                event_types::TARBALL_INTEGRITY_MISMATCH,
                tenant,
                now_unix_ms(),
                serde_json::json!({
                    "pkg": pkg,
                    "version": version,
                    "expected_shasum": expected,
                    "actual_shasum": actual,
                }),
            )?;
        }
        return Err(err);
    }

    let bytes_vec = downloaded.to_vec();
    // Audit BEFORE the CAS put (audit-fail-CLOSED contract).
    emit_npm_audit(
        auditor,
        event_types::TARBALL_STORED,
        tenant,
        now_unix_ms(),
        serde_json::json!({
            "pkg": pkg,
            "version": version,
            "digest": digest_hex,
            "size_bytes": bytes_vec.len(),
        }),
    )?;
    cas.put(tenant, &digest, bytes_vec.clone()).await?;
    Ok(TarballResponse {
        body: Bytes::from(bytes_vec),
        digest_hex,
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
    fn sha1_known_vector_empty() {
        // SHA1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        let got = sha1_hex(b"");
        assert_eq!(got, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn sha1_known_vector_abc() {
        // SHA1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
        let got = sha1_hex(b"abc");
        assert_eq!(got, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn verify_sha1_accepts_match() {
        let bytes = b"hello world";
        let hex = sha1_hex(bytes);
        assert!(verify_sha1(bytes, &hex).is_ok());
    }

    #[test]
    fn verify_sha1_accepts_uppercase_expected() {
        let bytes = b"hello world";
        let hex = sha1_hex(bytes).to_ascii_uppercase();
        assert!(verify_sha1(bytes, &hex).is_ok());
    }

    #[test]
    fn verify_sha1_rejects_mismatch() {
        let bytes = b"hello world";
        let wrong = "0".repeat(40);
        let result = verify_sha1(bytes, &wrong);
        assert!(matches!(
            result,
            Err(NpmAdapterError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn tarball_url_digest_is_stable() {
        let url = "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz";
        let d1 = tarball_url_digest(url).expect("d1");
        let d2 = tarball_url_digest(url).expect("d2");
        assert_eq!(d1.as_bytes(), d2.as_bytes());
    }

    #[test]
    fn tarball_url_digest_differs_for_different_urls() {
        let d1 = tarball_url_digest("https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz")
            .expect("d1");
        let d2 = tarball_url_digest("https://registry.npmjs.org/lodash/-/lodash-4.17.20.tgz")
            .expect("d2");
        assert_ne!(d1.as_bytes(), d2.as_bytes());
    }
}
