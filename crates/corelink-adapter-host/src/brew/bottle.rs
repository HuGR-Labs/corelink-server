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
use crate::brew::ports::{CacheFetch, CasError, SharedCasStore};
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

    // Drop the `brew/<tenant>/` route-mount prefix. The Worker forwards the FULL
    // request path (`pathSuffix: path`) and the container nests the adapter with
    // `nest_service` (NOT `nest`), which PRESERVES the full path — so the handler
    // receives `/brew/<tenant>/v2/…`, not `/v2/…`. Without this strip the upstream
    // URL became `ghcr.io/brew/<tenant>/v2/…` → 404 → `Upstream` → HTTP 502 (every
    // bottle fetch failed), and the CAS key embedded the tenant (defeating the
    // `_public` cross-tenant bottle dedup). Only strips when the path actually
    // starts with `brew/<seg>/`, so already-relative inputs (tests, internal
    // callers) are unchanged.
    let trimmed = trimmed
        .strip_prefix("brew/")
        .and_then(|after| after.split_once('/').map(|(_tenant, rest)| rest))
        .unwrap_or(trimmed);

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

/// Allowed Homebrew OCI repo namespaces (canonical, lowercase). A bottle
/// request MUST resolve under one of these `v2/<repo>/…` prefixes or it is
/// refused before any upstream fetch (F-005).
///
/// Homebrew serves bottles and casks exclusively from these two ghcr.io repos.
/// Without this allowlist the host-pinned-but-path-unrestricted SSRF guard
/// (`upstream::join_within_upstream`) would let any authenticated free tenant
/// drive CoreLink to fetch ARBITRARY ghcr.io content (`v2/<attacker>/<repo>/…`)
/// and — for tag-addressed paths — pin those attacker-chosen bytes into the
/// shared cross-tenant `_public` namespace, turning brew into an unrestricted
/// authenticated ghcr proxy + a cross-tenant cache-poisoning primitive.
const ALLOWED_REPO_PREFIXES: [&str; 2] = ["v2/homebrew/core/", "v2/homebrew/cask/"];

/// True when `canonical_path` targets an allowed Homebrew repo namespace
/// (`v2/homebrew/core/…` or `v2/homebrew/cask/…`). The path is already
/// canonicalized (leading slash + route prefix stripped, lowercased) by
/// [`canonical_bottle_path`], so a plain prefix match is exact and
/// case-insensitive. See [`ALLOWED_REPO_PREFIXES`].
#[must_use]
fn is_allowed_repo_path(canonical_path: &str) -> bool {
    ALLOWED_REPO_PREFIXES
        .iter()
        .any(|prefix| canonical_path.starts_with(prefix))
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
    /// Two F-005 gates protect the shared `_public` namespace:
    ///
    /// 1. **Repo-path allowlist** — the request MUST target an allowed Homebrew
    ///    repo (`v2/homebrew/core/…` or `v2/homebrew/cask/…`) or it is refused
    ///    with [`BrewAdapterError::ForbiddenRepoPath`] BEFORE any cache or
    ///    network access (brew is not an unrestricted authed ghcr.io proxy).
    /// 2. **Digest-only caching** — only digest-verified bytes
    ///    (`…/sha256:<hex>`) are pinned into `_public`. Mutable tag-addressed
    ///    paths are served for the current request but NEVER cached, so unverified
    ///    bytes cannot poison the cross-tenant namespace.
    ///
    /// `raw_path` is the brew client's request path (with optional
    /// query). `tenant_id` MUST be the value returned by the
    /// configured PAT resolver.
    ///
    /// Returns a [`CacheFetch`] carrying the served bytes plus the moat-proof
    /// `is_hit` signal (C-MOAT): `true` on a CAS hit (cross-tenant `_public`
    /// serve, no network), `false` on an upstream fill or a served-but-not-
    /// cached tag-addressed path. The error path is UNCHANGED.
    pub async fn fetch(
        &self,
        tenant_id: &str,
        raw_path: &str,
    ) -> Result<CacheFetch, BrewAdapterError> {
        let canonical = canonical_bottle_path(raw_path);

        // Repo-path allowlist (F-005). The SSRF guard pins the upstream HOST to
        // ghcr.io but does NOT restrict the repo PATH — so reject anything that
        // is not under an allowed Homebrew repo namespace BEFORE touching the
        // cache or the network. This closes the unrestricted-ghcr-proxy and the
        // cross-tenant `_public` poisoning vectors at the same chokepoint.
        if !is_allowed_repo_path(&canonical) {
            return Err(BrewAdapterError::ForbiddenRepoPath(canonical));
        }

        let cas_key = cas_key_for(&canonical);

        // Read-through: CAS hit short-circuits the upstream call.
        match self.cas.get(tenant_id, &cas_key).await {
            Ok(Some(bytes)) => {
                tracing::debug!(
                    tenant_id = %tenant_id,
                    cas_key = %cas_key,
                    "brew bottle CAS hit"
                );
                return Ok(CacheFetch {
                    bytes,
                    is_hit: true,
                });
            }
            Ok(None) => {}
            Err(CasError::Backend(msg)) => return Err(BrewAdapterError::Cas(msg)),
        }

        // Upstream fetch (bounded by `bottle_size_limit_bytes`).
        let bytes = self
            .upstream
            .fetch_bottle(&canonical, self.bottle_size_limit_bytes)
            .await?;

        // Caching policy for the shared `_public` namespace (F-005): ONLY
        // digest-verified bytes may be pinned. A tag-addressed (mutable) path
        // has no URL-declared digest to verify against, so caching it would let
        // one tenant's fetch be served verbatim to every later tenant requesting
        // the same path — the cross-tenant cache-poisoning vector. We therefore
        // fetch+serve mutable-tag bytes for THIS request but DO NOT cache them.
        let Some(expected) = expected_sha256(&canonical) else {
            let computed = sha256_hex(&bytes);
            tracing::warn!(
                tenant_id = %tenant_id,
                canonical_path = %canonical,
                cas_key = %cas_key,
                computed_sha256 = %computed,
                size_bytes = bytes.len(),
                "brew: tag-addressed manifest served WITHOUT caching \
                 (no URL-declared digest → refused for the shared _public \
                 namespace, F-005); sha256 logged for audit trail"
            );
            // Served from upstream this request (never cached) ⇒ a MISS.
            return Ok(CacheFetch {
                bytes,
                is_hit: false,
            });
        };

        // Pre-store integrity: the request path is content-addressed (ghcr.io
        // OCI blob / by-digest manifest, `…/sha256:<hex>`), so the fetched bytes
        // MUST match the URL-declared digest BEFORE the store. The store is the
        // shared `_public` namespace, so a MITMed/corrupt upstream response would
        // otherwise be persisted once and served to every tenant until the read
        // path self-heals. Mismatch → audit row, no store, no serve (the bytes
        // are corrupt; the brew client would reject them against the DSL anyway).
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

        // Audit-emit-BEFORE-mutation. On audit failure we do NOT call `cas.put`,
        // preserving INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER. Only reached for
        // digest-verified (content-addressed) paths — the tag-addressed case
        // returned early above without caching (F-005). `integrity = verified`.
        self.auditor.emit_bottle_cache_fill(
            tenant_id,
            &cas_key,
            &canonical,
            bytes.len() as u64,
            true,
        )?;

        // Persist.
        if let Err(CasError::Backend(msg)) = self.cas.put(tenant_id, &cas_key, bytes.clone()).await
        {
            return Err(BrewAdapterError::Cas(msg));
        }

        // Upstream fill (this request fetched + stored) ⇒ a MISS.
        Ok(CacheFetch {
            bytes,
            is_hit: false,
        })
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
    fn canonical_strips_brew_tenant_route_prefix() {
        // The PROD input shape: the Worker forwards the full path and the
        // adapter is nested with `nest_service`, so the handler gets
        // `/brew/<tenant>/v2/…`. The upstream-relative path must drop
        // `brew/<tenant>/` (else ghcr.io/brew/<tenant>/… → 404 → 502).
        assert_eq!(
            canonical_bottle_path(
                "/brew/00000000-0000-4000-8000-0000000f0002/v2/homebrew/core/jq/manifests/1.7"
            ),
            "v2/homebrew/core/jq/manifests/1.7"
        );
        // Cross-tenant dedup: the SAME bottle under a DIFFERENT tenant
        // canonicalizes identically (the `_public` moat).
        assert_eq!(
            canonical_bottle_path("/brew/tenant-a/v2/homebrew/core/jq/blobs/sha256:abc"),
            canonical_bottle_path("/brew/tenant-b/v2/homebrew/core/jq/blobs/sha256:abc")
        );
        // Already-relative input (internal callers / tests) is unchanged.
        assert_eq!(canonical_bottle_path("/v2/foo"), "v2/foo");
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
        assert_eq!(
            expected_sha256("v2/homebrew/core/curl/manifests/8.5.0"),
            None
        );
        assert_eq!(
            expected_sha256("v2/homebrew/core/curl-8.5.0.bottle.tar.gz"),
            None
        );
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
    fn allowed_repo_paths_accept_homebrew_core_and_cask() {
        assert!(is_allowed_repo_path(
            "v2/homebrew/core/curl/blobs/sha256:abc"
        ));
        assert!(is_allowed_repo_path(
            "v2/homebrew/cask/firefox/manifests/1.0"
        ));
        assert!(is_allowed_repo_path("v2/homebrew/core/jq"));
    }

    #[test]
    fn forbidden_repo_paths_reject_off_allowlist_repos() {
        // F-005: arbitrary ghcr.io repos, near-misses, and traversal-ish paths
        // must NOT be allowed — brew is not an unrestricted ghcr proxy.
        for p in [
            "v2/attacker/evil/blobs/sha256:abc",
            "v2/homebrew/evil/manifests/latest", // wrong sub-repo
            "v2/library/ubuntu/manifests/latest",
            "v2/homebrew/coreextra/x", // prefix-confusion: needs the trailing `/`
            "v2/homebrew", // too short
            "homebrew/core/curl", // missing the v2/ segment
            "",
        ] {
            assert!(!is_allowed_repo_path(p), "{p} must be rejected");
        }
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
