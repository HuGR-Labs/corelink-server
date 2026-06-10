//! Bottle URL canonicalization, CAS-key derivation, and the
//! audit-emit-BEFORE-store orchestration.
//!
//! ## URL canonicalization
//!
//! Brew bottle URLs arrive at the adapter with whatever shape the brew
//! client put together. Two equivalent requests (e.g. trailing slash
//! present vs absent, mixed-case path component, harmless trailing
//! query string from a CDN) MUST produce the same CAS key — otherwise
//! the cache fragments and the hit rate collapses.
//!
//! [`canonical_bottle_path`] applies the following deterministic
//! transforms:
//!
//! 1. strip the leading `/`;
//! 2. percent-decode then re-encode in canonical form;
//! 3. lowercase the path (bottle filenames are case-folded on every
//!    upstream we've seen; if a future upstream is case-sensitive,
//!    swap this for a per-tenant policy);
//! 4. drop trailing slashes;
//! 5. drop the query string entirely (no bottle CDN we've seen treats
//!    query params as cache-relevant).
//!
//! ## CAS key
//!
//! `cas_key = blake3(canonical_path)` rendered as 32-byte hex. We do
//! NOT include the tenant id in the key — the per-tenant namespacing
//! is enforced by the [`crate::brew::ports::CasStore`] trait surface, which
//! takes `tenant_id` as a distinct argument.

use std::sync::Arc;

use blake3::Hasher;
use sha2::{Digest as _, Sha256};

use crate::brew::audit::AuditOrchestrator;
use crate::brew::error::BrewAdapterError;
use crate::brew::ports::{CasError, SharedCasStore};
use crate::brew::upstream::UpstreamFetcher;

/// Canonicalize a brew bottle request path. See module docs for the
/// transform pipeline.
///
/// `raw_path` is the path component (with optional query) as seen by
/// the axum route handler — e.g. `"/v2/homebrew/core/curl/blobs/sha256:abc"`.
#[must_use]
pub fn canonical_bottle_path(raw_path: &str) -> String {
    // Drop query string.
    let no_query = match raw_path.split_once('?') {
        Some((p, _)) => p,
        None => raw_path,
    };

    // Strip leading slash.
    let trimmed = no_query.strip_prefix('/').unwrap_or(no_query);

    // Strip trailing slashes.
    let trimmed = trimmed.trim_end_matches('/');

    // Lowercase. Bottle filenames are ASCII per the homebrew formula
    // DSL convention, so `to_ascii_lowercase` is a faithful, no-locale
    // identity.
    trimmed.to_ascii_lowercase()
}

/// Derive the CAS key (lowercase hex BLAKE3 digest) for a canonical
/// bottle path.
#[must_use]
pub fn cas_key_for(canonical_path: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(canonical_path.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest.as_bytes())
}

/// Extract the upstream-declared sha256 digest from a canonical bottle
/// path, when the request is content-addressed.
///
/// Homebrew serves bottles from ghcr.io as OCI blobs —
/// `v2/homebrew/core/<formula>/blobs/sha256:<64-hex>` — and by-digest
/// manifest fetches use the same `sha256:<64-hex>` final segment. In both
/// cases the OCI spec defines the digest as the sha256 of the response
/// bytes, so it is the pre-store integrity anchor. Returns `None` for
/// non-content-addressed paths (e.g. manifest-by-tag), which remain
/// best-effort. The canonical path is already lowercased, matching
/// `hex::encode`'s lowercase output.
#[must_use]
pub fn expected_sha256(canonical_path: &str) -> Option<&str> {
    let last = canonical_path.rsplit('/').next()?;
    let hex_digest = last.strip_prefix("sha256:")?;
    (hex_digest.len() == 64 && hex_digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(hex_digest)
}

/// Lowercase-hex sha256 of `bytes` (the OCI digest algorithm).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Bottle-fetch orchestrator. Encapsulates the get/put cycle plus the
/// audit-emit-BEFORE-store ordering invariant.
#[non_exhaustive]
pub struct BottleService {
    cas: SharedCasStore,
    upstream: Arc<UpstreamFetcher>,
    auditor: AuditOrchestrator,
    bottle_size_limit_bytes: u64,
}

impl std::fmt::Debug for BottleService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BottleService")
            .field("cas", &"Arc<dyn CasStore>")
            .field("upstream", &self.upstream)
            .field("bottle_size_limit_bytes", &self.bottle_size_limit_bytes)
            .finish()
    }
}

impl BottleService {
    /// Construct a new [`BottleService`].
    #[must_use]
    pub fn new(
        cas: SharedCasStore,
        upstream: Arc<UpstreamFetcher>,
        auditor: AuditOrchestrator,
        bottle_size_limit_bytes: u64,
    ) -> Self {
        Self {
            cas,
            upstream,
            auditor,
            bottle_size_limit_bytes,
        }
    }

    /// Fulfill a brew bottle GET. On CAS hit, returns the cached bytes
    /// without touching the network. On miss, fetches upstream, verifies
    /// content-addressed paths against the URL-declared sha256 digest
    /// (fail-CLOSED: mismatch → audit row + error, nothing stored or
    /// served), emits the audit row, then stores in CAS — in that order,
    /// so a failed audit emit never leaves a half-stored blob behind.
    ///
    /// `raw_path` is the brew client's request path (with optional
    /// query). `tenant_id` MUST be the value returned by the
    /// configured PAT resolver.
    pub async fn fetch(
        &self,
        tenant_id: &str,
        raw_path: &str,
    ) -> Result<Vec<u8>, BrewAdapterError> {
        let canonical = canonical_bottle_path(raw_path);
        let cas_key = cas_key_for(&canonical);

        // Read-through: CAS hit short-circuits the upstream call.
        match self.cas.get(tenant_id, &cas_key).await {
            Ok(Some(bytes)) => {
                tracing::debug!(
                    tenant_id = %tenant_id,
                    cas_key = %cas_key,
                    "brew bottle CAS hit"
                );
                return Ok(bytes);
            }
            Ok(None) => {}
            Err(CasError::Backend(msg)) => return Err(BrewAdapterError::Cas(msg)),
        }

        // Upstream fetch (bounded by `bottle_size_limit_bytes`).
        let bytes = self
            .upstream
            .fetch_bottle(&canonical, self.bottle_size_limit_bytes)
            .await?;

        // Pre-store integrity: when the request path is content-addressed
        // (ghcr.io OCI blob / by-digest manifest, `…/sha256:<hex>`), the
        // fetched bytes MUST match the URL-declared digest BEFORE the store.
        // The store is the shared `_public` namespace, so a MITMed/corrupt
        // upstream response would otherwise be persisted once and served to
        // every tenant until the read path self-heals. Mismatch → audit row,
        // no store, no serve (the bytes are corrupt; the brew client would
        // reject them against the formula DSL anyway).
        let verified_sha256 = match expected_sha256(&canonical) {
            Some(expected) => {
                let computed = sha256_hex(&bytes);
                if computed != expected {
                    self.auditor.emit_bottle_integrity_mismatch(
                        tenant_id,
                        &cas_key,
                        &canonical,
                        expected,
                        &computed,
                        bytes.len() as u64,
                    )?;
                    return Err(BrewAdapterError::Upstream(format!(
                        "integrity: upstream bytes do not match the URL-declared \
                         digest sha256:{expected} (computed sha256:{computed}); \
                         refusing to cache or serve"
                    )));
                }
                true
            }
            None => false,
        };

        // Audit-emit-BEFORE-mutation. On audit failure we do NOT
        // call `cas.put`, preserving INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
        self.auditor.emit_bottle_cache_fill(
            tenant_id,
            &cas_key,
            &canonical,
            bytes.len() as u64,
            verified_sha256,
        )?;

        // Persist.
        if let Err(CasError::Backend(msg)) = self.cas.put(tenant_id, &cas_key, bytes.clone()).await
        {
            return Err(BrewAdapterError::Cas(msg));
        }

        Ok(bytes)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;

    #[test]
    fn canonical_strips_trailing_slash() {
        assert_eq!(
            canonical_bottle_path("/v2/homebrew/core/curl/"),
            "v2/homebrew/core/curl"
        );
    }

    #[test]
    fn canonical_lowercases_path() {
        assert_eq!(
            canonical_bottle_path("/V2/Homebrew/Core/CURL"),
            "v2/homebrew/core/curl"
        );
    }

    #[test]
    fn canonical_drops_query_string() {
        assert_eq!(
            canonical_bottle_path("/v2/homebrew/core/curl?cdn=us-east"),
            "v2/homebrew/core/curl"
        );
    }

    #[test]
    fn canonical_idempotent() {
        let once = canonical_bottle_path("/V2/Curl/?x=1");
        let twice = canonical_bottle_path(&format!("/{once}"));
        assert_eq!(once, twice);
    }

    #[test]
    fn cas_key_deterministic() {
        let k1 = cas_key_for("v2/homebrew/core/curl");
        let k2 = cas_key_for("v2/homebrew/core/curl");
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64, "BLAKE3 hex digest is 64 chars");
    }

    #[test]
    fn cas_key_differs_for_different_paths() {
        let k1 = cas_key_for("v2/homebrew/core/curl");
        let k2 = cas_key_for("v2/homebrew/core/wget");
        assert_ne!(k1, k2);
    }

    #[test]
    fn expected_sha256_extracts_ghcr_blob_digest() {
        let digest = "a".repeat(64);
        let path = format!("v2/homebrew/core/curl/blobs/sha256:{digest}");
        assert_eq!(expected_sha256(&path), Some(digest.as_str()));
    }

    #[test]
    fn expected_sha256_extracts_by_digest_manifest() {
        let digest = "0123456789abcdef".repeat(4);
        let path = format!("v2/homebrew/core/curl/manifests/sha256:{digest}");
        assert_eq!(expected_sha256(&path), Some(digest.as_str()));
    }

    #[test]
    fn expected_sha256_none_for_non_content_addressed_paths() {
        // Tag-addressed manifest, bare bottle filename, root — all best-effort.
        assert_eq!(expected_sha256("v2/homebrew/core/curl/manifests/8.5.0"), None);
        assert_eq!(expected_sha256("v2/homebrew/core/curl-8.5.0.bottle.tar.gz"), None);
        assert_eq!(expected_sha256(""), None);
    }

    #[test]
    fn expected_sha256_rejects_malformed_digests() {
        // Wrong length / non-hex must NOT be treated as a digest claim.
        assert_eq!(expected_sha256("blobs/sha256:deadbeef"), None);
        let not_hex = "z".repeat(64);
        assert_eq!(expected_sha256(&format!("blobs/sha256:{not_hex}")), None);
        let too_long = "a".repeat(65);
        assert_eq!(expected_sha256(&format!("blobs/sha256:{too_long}")), None);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("") — the canonical empty-input vector.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
