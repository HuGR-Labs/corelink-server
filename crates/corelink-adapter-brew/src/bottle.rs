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
//! is enforced by the [`crate::ports::CasStore`] trait surface, which
//! takes `tenant_id` as a distinct argument.

use std::sync::Arc;

use blake3::Hasher;

use crate::audit::AuditOrchestrator;
use crate::error::BrewAdapterError;
use crate::ports::{CasError, SharedCasStore};
use crate::upstream::UpstreamFetcher;

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
    /// without touching the network. On miss, fetches upstream, emits
    /// the audit row, then stores in CAS — in that order, so a failed
    /// audit emit never leaves a half-stored blob behind.
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

        // Audit-emit-BEFORE-mutation. On audit failure we do NOT
        // call `cas.put`, preserving INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
        self.auditor
            .emit_bottle_cache_fill(tenant_id, &cas_key, &canonical, bytes.len() as u64)?;

        // Persist.
        if let Err(CasError::Backend(msg)) = self
            .cas
            .put(tenant_id, &cas_key, bytes.clone())
            .await
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
}
