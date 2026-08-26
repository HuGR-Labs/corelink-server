//! At-rest CAS integrity scrubber — `POST /_internal/cas/scrub`.
//!
//! ## What this closes
//!
//! The only integrity coverage CoreLink has today is the read-path re-verify in
//! [`crate::storage::r2_s3::R2CasHandler::read`], which re-hashes the bytes R2
//! returned and refuses to serve a mismatch. That check is real and it is the
//! right check — it catches R2 bitrot, storage-tier tampering and historically
//! mis-keyed blobs, failure modes below our own API that no write-path
//! validation can see. But it is a **sampling function driven by traffic**: an
//! object is verified exactly when a client asks for it, so a cold object is
//! never checked at all. This route is the sweep that covers the cold set.
//!
//! Full rationale, including why this is a hard prerequisite for streaming CAS
//! reads:
//! `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md`.
//!
//! ## Enumerate R2, never `blob_meta`
//!
//! `blob_meta` is EMPTY in production and no code inserts into it — the schema
//! is perfect and unmaintained. A scrubber keyed on it enumerates zero objects,
//! verifies nothing, and reports success: a green integrity dashboard over an
//! unverified store. Enumeration therefore goes through
//! [`crate::storage::r2_s3::R2S3Client::list_objects_page`], which is
//! cursor-driven and bounded.
//!
//! ## Why the sweep is per TENANT, and why the tenant list matters
//!
//! An R2 CAS key is `<region>/<tenant_prefix>/<digest>` (BLAKE3) or
//! `<region>/<tenant_prefix>/bazel/sha256/<digest>` (REAPI v2), where
//! `tenant_prefix` is a **secret-keyed HMAC** of the tenant UUID. A key cannot
//! be mapped back to a tenant, so the sweep cannot walk the bucket blind — it
//! walks tenants and derives each prefix, exactly as the erase path does
//! (`routes::cas_erase::R2CasBlobEraser::tenant_prefix`).
//!
//! That makes the choice of tenant list load-bearing. Measured against
//! `corelink-config-prod` on 2026-08-26: `tenant` holds 262 rows,
//! `tenant_storage_state` 74, `blob_meta` 0. `tenant_storage_state` only
//! carries tenants that have accrued byte accounting, so driving the sweep off
//! it would omit 188 of 262 tenants and still report a clean run — the
//! `blob_meta` silent-success shape, one table over. This module enumerates
//! `tenant`.
//!
//! ## Three counters, never two
//!
//! `examined`, `skipped_encrypted` and `failed` are three DISTINCT numbers. A
//! sweep that enumerated nothing and found nothing must be distinguishable from
//! a healthy one; folding skips into examined reports full coverage over a store
//! that was never checked. This is the load-bearing requirement of the whole
//! endpoint, not a reporting nicety.
//!
//! ## BYOK: skip the TENANT, not the object
//!
//! For a BYOK-`active` tenant the stored object is CIPHERTEXT, and the read path
//! decrypts BEFORE hashing precisely because the integrity re-verify must run on
//! the plaintext, never on ciphertext. Re-hashing raw bytes for such a tenant
//! produces a FALSE `CorrectnessViolation` against perfectly intact data.
//!
//! The question is asked ONCE PER TENANT through the public config API —
//! [`crate::storage::byok_cas::ByokConfigCache::get`] plus
//! [`crate::storage::byok_cas::engagement_for`], the single source of truth the
//! read and write paths already share. Notably NOT through `resolve_byok`: that
//! is private to `impl R2CasHandler` / `impl R2AcHandler`, is not a method on
//! `R2S3Client`, and maps a LOGICAL digest to a physical one — the opposite of
//! the direction a sweep travels (ADR-S34-001 addendum 2).
//!
//! BYOK is gated-inert today, which is exactly the trap: written naively this
//! module would be correct today and silently wrong the day BYOK flips, with no
//! compile error and no failing test.
//!
//! ## Fail loud, never fail open
//!
//! A tenant whose BYOK engagement cannot be determined is counted `failed`, not
//! skipped and never assumed plaintext. A sweep that truncates returns
//! `incomplete: true` plus a cursor. Nothing here deletes, rewrites or repairs
//! anything — the scrubber only reads and reports.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use corelink_handler_cas::DigestAlgo;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::storage::byok_cas::{engagement_for, ByokConfigCache, ByokEngagement};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::{public_namespace_prefix, verify_content_hash, R2S3Client};
use crate::storage::region_map::CAS_REGIONS;

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Mirrors `routes::cas_erase::DEFAULT_CAS_BUCKET`.
const DEFAULT_CAS_BUCKET: &str = "corelink-cas-prod";

/// Objects verified per call before the sweep truncates and hands back a
/// cursor. Bounds ONE call's work: the scrubber GETs every object it examines,
/// so an unbounded sweep exceeds the edge subrequest timeout and dies having
/// recorded nothing.
const DEFAULT_OBJECT_BUDGET: usize = 500;

/// Keys fetched per `list_objects_page` call. `list_objects_page` clamps into
/// `1..=1000` itself; this is the sweep's own, smaller default.
const LIST_PAGE_SIZE: u32 = 200;

/// Tenants read from `tenant` per page.
const TENANT_PAGE_SIZE: i64 = 100;

/// Shared state for the scrub route.
#[derive(Clone)]
pub struct CasScrubState {
    internal_auth_key: String,
    d1: Arc<D1HttpClient>,
    r2: Arc<R2S3Client>,
    tdk: Arc<TenantDerivationKey>,
    /// `None` ⇒ BYOK is not wired in this deployment at all. Reported in the
    /// summary rather than silently treated as "every tenant is plaintext".
    byok: Option<Arc<ByokConfigCache>>,
    bucket: String,
    object_budget: usize,
}

impl std::fmt::Debug for CasScrubState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CasScrubState")
            .field("internal_auth_key", &"<redacted>")
            .field("d1", &"[D1HttpClient]")
            .field("tdk", &"<redacted>")
            .field("byok_wired", &self.byok.is_some())
            .field("bucket", &self.bucket)
            .field("object_budget", &self.object_budget)
            .finish()
    }
}

/// Where a truncated sweep resumes.
///
/// Carries the tenant offset AND the in-tenant position, because a sweep can
/// run out of budget in the middle of a tenant and must not restart that
/// tenant from the top on the next call (it would re-examine the same head
/// forever and never reach the tail).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScrubCursor {
    /// Rows already consumed from `tenant`, `ORDER BY tenant_id`.
    #[serde(default)]
    pub tenant_offset: i64,
    /// The tenant the previous call stopped inside, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Index into [`CAS_REGIONS`] for that tenant.
    #[serde(default)]
    pub region_idx: usize,
    /// Opaque R2 continuation token within that region's prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r2_cursor: Option<String>,
}

/// The three counters plus the resume state.
#[derive(Clone, Debug, Default, Serialize)]
struct ScrubSummary {
    /// Objects READ and re-hashed. Not "objects seen".
    examined: u64,
    /// Objects belonging to a BYOK-encrypting tenant, skipped whole. Their data
    /// is intact — merely opaque to a re-hash — so this is never `failed`.
    skipped_encrypted: u64,
    /// Digest mismatches, unreadable objects, unclassifiable tenants and
    /// non-derivable tenant prefixes. Anything the sweep could not turn into a
    /// clean verdict.
    failed: u64,
    /// The mismatching keys, capped so one bad prefix cannot produce an
    /// unbounded response body.
    violations: Vec<String>,
    /// True when the object budget ran out before the tenant list did.
    incomplete: bool,
}

/// Cap on `violations` in one response. The counters stay exact; only the
/// enumerated list is truncated.
const MAX_REPORTED_VIOLATIONS: usize = 50;

/// Build the route state from the environment.
///
/// Mirrors [`crate::routes::audit_archive::build_state_from_env`]: the
/// DEDICATED `CORELINK_ERASE_AUTH_KEY` only (no shared fallback), ≥32 chars,
/// plus D1, R2 and the TDK. Any missing piece leaves the route UNMOUNTED
/// (fail-CLOSED) rather than mounted-and-broken.
///
/// No new env var is introduced on purpose: a new secret would need its own row
/// in `docs/internal/secrets-checklist.md` and this endpoint is the same
/// internal-operator surface the erase and archive routes already gate.
pub async fn build_state_from_env() -> Option<CasScrubState> {
    let internal_auth_key = match crate::routes::admin::erase_auth_key_from_env() {
        Some(k) if k.len() >= 32 => k.to_string(),
        _ => {
            tracing::warn!(
                "no usable CORELINK_ERASE_AUTH_KEY (dedicated; NO shared fallback) \
                 (< 32 chars); /_internal/cas/scrub NOT mounted (fail-CLOSED)"
            );
            return None;
        }
    };
    let Some(tdk) = load_tdk_from_env() else {
        tracing::warn!(
            "cas_scrub: R2_TDK_HEX absent/invalid; /_internal/cas/scrub NOT mounted \
             (fail-CLOSED) — without it no tenant prefix can be derived"
        );
        return None;
    };
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(crate::storage::d1_http::D1HttpClient::new(&storage_env).ok()?);
    let bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
    let r2 = match R2S3Client::new(&storage_env, bucket.clone()).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::warn!(
                error = %e,
                bucket = %bucket,
                "cas/scrub: R2 client build failed; route NOT mounted (fail-CLOSED)"
            );
            return None;
        }
    };
    let object_budget = std::env::var("CAS_SCRUB_OBJECT_BUDGET")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(DEFAULT_OBJECT_BUDGET);
    Some(CasScrubState {
        internal_auth_key,
        d1: Arc::clone(&d1),
        r2,
        tdk: Arc::new(tdk),
        // Wired by the caller via `with_byok` when the deployment has a BYOK
        // config source; absent means BYOK is not deployed here at all.
        byok: None,
        bucket,
        object_budget,
    })
}

impl CasScrubState {
    /// Attach the SAME [`ByokConfigCache`] the storage handlers hold, so the
    /// scrubber's engagement lookup and the read path's cannot disagree.
    #[must_use]
    pub fn with_byok(mut self, byok: Arc<ByokConfigCache>) -> Self {
        self.byok = Some(byok);
        self
    }
}

/// Mount `POST /_internal/cas/scrub`.
pub fn router(state: CasScrubState) -> Router {
    Router::new()
        .route("/_internal/cas/scrub", post(handle_scrub))
        .with_state(state)
}

/// Load the tenant derivation key from `R2_TDK_HEX` (64 hex chars = 32 bytes).
///
/// Mirrors `routes::cas_erase::load_tdk_from_env` byte-for-byte (same env var,
/// same length gate, same hex decode) so the prefix this sweep derives matches
/// the one the writer and the eraser derive.
fn load_tdk_from_env() -> Option<TenantDerivationKey> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        tracing::warn!(
            len = hex_str.len(),
            "cas_scrub: R2_TDK_HEX has wrong length; scrubber NOT built (route unmounted)"
        );
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    if hex::decode_to_slice(hex_str, bytes.as_mut()).is_err() {
        tracing::warn!("cas_scrub: R2_TDK_HEX is not valid hex; scrubber NOT built (unmounted)");
        return None;
    }
    Some(TenantDerivationKey::from_bytes(bytes))
}

/// Constant-time internal-auth check — mirrors
/// [`crate::routes::audit_archive`] so the internal-auth surface has ONE
/// behaviour, not one per module.
fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let Some(provided) = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let provided = provided.as_bytes();
    if provided.len() != expected.len() {
        return false;
    }
    provided.ct_eq(expected).into()
}

/// Derive the 16-char tenant prefix the SAME way the CAS writer and the eraser
/// do (`routes::cas_erase::R2CasBlobEraser::tenant_prefix`).
///
/// `_public` goes through [`public_namespace_prefix`] BEFORE the UUID parse —
/// the sentinel is not a parseable UUID and its bytes live under the reserved
/// TDK-keyed prefix, not a per-tenant HMAC. A non-UUID tenant fails CLOSED on
/// the production path: a truncate-and-pad fallback collapses non-derivable
/// tenants into one SHARED keyspace, and a sweep reading under the wrong
/// tenant's prefix is a tenant-isolation violation even though it only reads.
fn tenant_prefix(tdk: &TenantDerivationKey, tenant: &str) -> Result<String, String> {
    if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
        return Ok(public_namespace_prefix(tdk));
    }
    if let Ok(uid) = Uuid::try_parse(tenant) {
        return Ok(derive_prefix(tdk, uid).to_string());
    }
    Err(format!(
        "non-derivable tenant '{tenant}' on the CAS scrub path — refusing degraded prefix \
         (fail-closed)"
    ))
}

/// What the sweep decided to do with one tenant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TenantPlan {
    /// Plaintext at rest — re-hash its objects.
    Scrub,
    /// BYOK-encrypting — count its objects `skipped_encrypted`, read none.
    SkipEncrypted,
    /// Could not be classified. Counted `failed`; NEVER assumed plaintext.
    Unclassifiable,
}

/// Classify one tenant, once, through the public BYOK config API.
///
/// Deliberately NOT `resolve_byok`: see the module docs. `_public` is
/// plaintext by design (cross-tenant dedup depends on the raw key), which
/// matches what `resolve_byok` does for the same namespace.
async fn classify_tenant(byok: Option<&Arc<ByokConfigCache>>, tenant: &str) -> TenantPlan {
    let Some(cache) = byok else {
        // BYOK is not wired in this deployment; every object is plaintext. The
        // response says `byok_wired: false` so a reader can tell this apart
        // from "checked and found plaintext".
        return TenantPlan::Scrub;
    };
    if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
        return TenantPlan::Scrub;
    }
    match cache.get(tenant).await {
        Ok(None) => TenantPlan::Scrub,
        Ok(Some(cfg)) => match engagement_for(&cfg) {
            ByokEngagement::Plaintext => TenantPlan::Scrub,
            ByokEngagement::Encrypt(_) => TenantPlan::SkipEncrypted,
            ByokEngagement::FailClosed(why) => {
                tracing::warn!(
                    tenant = %tenant,
                    reason = %why,
                    "cas/scrub: tenant is BYOK-active but unresolvable; counting FAILED \
                     (never plaintext, never a silent skip)"
                );
                TenantPlan::Unclassifiable
            }
        },
        Err(e) => {
            tracing::warn!(
                tenant = %tenant,
                error = %e,
                "cas/scrub: BYOK config read failed; counting FAILED (never plaintext)"
            );
            TenantPlan::Unclassifiable
        }
    }
}

/// Read one page of tenant ids from `tenant`.
///
/// `tenant`, NOT `tenant_storage_state`: the latter only carries tenants that
/// have accrued byte accounting and would silently omit most of the estate
/// (see the module docs for the measured row counts).
async fn read_tenant_page(
    d1: &D1HttpClient,
    offset: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    let rows = d1
        .query(
            // CAST: the D1 REST binder sends JSON numbers as REAL, and SQLite
            // rejects a REAL where LIMIT/OFFSET want an INTEGER.
            "SELECT tenant_id FROM tenant ORDER BY tenant_id \
             LIMIT CAST(?1 AS INTEGER) OFFSET CAST(?2 AS INTEGER)",
            &[json!(limit), json!(offset)],
        )
        .await?;
    rows.iter()
        .map(|row| {
            row.get("tenant_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "tenant.tenant_id missing/non-text".to_owned())
        })
        .collect()
}

/// Split an R2 CAS key into `(digest, algo)`.
///
/// BLAKE3 keys are `<region>/<prefix>/<digest>`; REAPI v2 keys are
/// `<region>/<prefix>/bazel/sha256/<digest>`. Both put the digest last, but the
/// digest FUNCTION differs, and re-hashing a SHA-256 blob with BLAKE3 would
/// report a violation against intact data — so the sub-prefix decides the algo.
/// Anything else is not a CAS blob key and is left alone.
fn key_to_digest(key: &str) -> Option<(&str, DigestAlgo)> {
    let parts: Vec<&str> = key.split('/').collect();
    match parts.as_slice() {
        [_region, _prefix, digest] => Some((*digest, DigestAlgo::Blake3)),
        [_region, _prefix, "bazel", "sha256", digest] => Some((*digest, DigestAlgo::Sha256)),
        _ => None,
    }
}

/// `POST /_internal/cas/scrub` — verify a bounded slice of the stored CAS set.
async fn handle_scrub(
    State(state): State<CasScrubState>,
    headers: HeaderMap,
    body: Option<Json<ScrubCursor>>,
) -> Response {
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let cursor = body.map(|Json(c)| c).unwrap_or_default();
    match sweep(&state, cursor).await {
        Ok((summary, next)) => {
            let payload = json!({
                "bucket": state.bucket,
                "byok_wired": state.byok.is_some(),
                "examined": summary.examined,
                "skipped_encrypted": summary.skipped_encrypted,
                "failed": summary.failed,
                "violations": summary.violations,
                "incomplete": summary.incomplete,
                "cursor": next,
            });
            // A digest mismatch is a correctness incident, not a 200. The
            // counters travel in the body either way so the cron logs both.
            let status = if summary.failed > 0 {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::OK
            };
            (status, Json(payload)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "cas/scrub: sweep failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            )
                .into_response()
        }
    }
}

/// Walk tenants from `cursor` until the object budget runs out or the tenant
/// list is exhausted.
async fn sweep(
    state: &CasScrubState,
    cursor: ScrubCursor,
) -> Result<(ScrubSummary, Option<ScrubCursor>), String> {
    let mut summary = ScrubSummary::default();
    let mut tenant_offset = cursor.tenant_offset;
    // The tenant the caller stopped inside, if any — resumed before the list
    // advances, so a partly-swept tenant is never restarted from its head.
    let mut resume: Option<(String, usize, Option<String>)> = cursor
        .tenant_id
        .clone()
        .map(|t| (t, cursor.region_idx, cursor.r2_cursor.clone()));

    loop {
        let tenants = if let Some((t, idx, r2c)) = resume.take() {
            vec![(t, idx, r2c)]
        } else {
            let page = read_tenant_page(&state.d1, tenant_offset, TENANT_PAGE_SIZE).await?;
            if page.is_empty() {
                return Ok((summary, None));
            }
            page.into_iter().map(|t| (t, 0usize, None)).collect()
        };

        for (tenant, start_region, start_r2) in tenants {
            let plan = classify_tenant(state.byok.as_ref(), &tenant).await;
            let prefix = match tenant_prefix(&state.tdk, &tenant) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(tenant = %tenant, error = %e, "cas/scrub: prefix underivable");
                    summary.failed = summary.failed.saturating_add(1);
                    tenant_offset = tenant_offset.saturating_add(1);
                    continue;
                }
            };
            if plan == TenantPlan::Unclassifiable {
                summary.failed = summary.failed.saturating_add(1);
                tenant_offset = tenant_offset.saturating_add(1);
                continue;
            }

            let mut region_idx = start_region;
            let mut r2_cursor = start_r2;
            while let Some(region) = CAS_REGIONS.get(region_idx) {
                let scan_prefix = format!("{region}/{prefix}/");
                let (keys, next) = state
                    .r2
                    .list_objects_page(&scan_prefix, LIST_PAGE_SIZE, r2_cursor.as_deref())
                    .await?;
                for (key, _size, _modified) in keys {
                    if plan == TenantPlan::SkipEncrypted {
                        summary.skipped_encrypted = summary.skipped_encrypted.saturating_add(1);
                        continue;
                    }
                    let Some((digest, algo)) = key_to_digest(&key) else {
                        continue;
                    };
                    match state.r2.get(&key).await {
                        Ok(Some(bytes)) => match verify_content_hash(algo, digest, &bytes) {
                            Ok(()) => summary.examined = summary.examined.saturating_add(1),
                            Err(actual) => {
                                tracing::error!(
                                    key = %key,
                                    actual = %actual,
                                    "cas/scrub: CONTENT MISMATCH at rest"
                                );
                                summary.failed = summary.failed.saturating_add(1);
                                if summary.violations.len() < MAX_REPORTED_VIOLATIONS {
                                    summary.violations.push(key.clone());
                                }
                            }
                        },
                        // Deleted between LIST and GET — a concurrent erase, not
                        // a defect. Neither examined nor failed.
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(key = %key, error = %e, "cas/scrub: object unreadable");
                            summary.failed = summary.failed.saturating_add(1);
                        }
                    }
                }
                r2_cursor = next;
                if r2_cursor.is_none() {
                    region_idx = region_idx.saturating_add(1);
                }
                // Budget is counted over objects the sweep RESOLVED — examined
                // plus skipped — so a huge encrypting tenant cannot make one
                // call walk the whole estate for free.
                let resolved = summary
                    .examined
                    .saturating_add(summary.skipped_encrypted)
                    .try_into()
                    .unwrap_or(usize::MAX);
                if resolved >= state.object_budget {
                    summary.incomplete = true;
                    return Ok((
                        summary,
                        Some(ScrubCursor {
                            tenant_offset,
                            tenant_id: Some(tenant),
                            region_idx,
                            r2_cursor,
                        }),
                    ));
                }
            }
            tenant_offset = tenant_offset.saturating_add(1);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;
    use crate::customer_d1::{
        ByokConfigError, ByokCryptoMode, ByokMode, ByokState, TenantByokConfig,
    };
    use crate::storage::byok_cas::ByokConfigSource;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct FixedSource(Result<Option<TenantByokConfig>, ()>);

    #[async_trait]
    impl ByokConfigSource for FixedSource {
        async fn get_byok_config(
            &self,
            _tenant: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            match &self.0 {
                Ok(cfg) => Ok(cfg.clone()),
                Err(()) => Err(ByokConfigError::Transport("boom".to_owned())),
            }
        }
    }

    fn cfg(state: ByokState) -> TenantByokConfig {
        TenantByokConfig {
            tenant_id: "11111111-1111-1111-1111-111111111111".to_owned(),
            mode: ByokMode::Byok,
            crypto_mode: ByokCryptoMode::Convergent,
            cmk_provider: Some("aws".to_owned()),
            cmk_key_id: Some("key-1".to_owned()),
            cmk_region: Some("us-east-1".to_owned()),
            state,
        }
    }

    fn cache_of(src: FixedSource) -> Arc<ByokConfigCache> {
        Arc::new(ByokConfigCache::new(Arc::new(src), 60))
    }

    // ── BYOK classification: the skip must be a SKIP, never a failure ──────

    #[tokio::test]
    async fn active_tenant_is_skipped_not_failed() {
        let cache = cache_of(FixedSource(Ok(Some(cfg(ByokState::Active)))));
        assert_eq!(
            classify_tenant(Some(&cache), "11111111-1111-1111-1111-111111111111").await,
            TenantPlan::SkipEncrypted,
            "a BYOK-active tenant's data is intact, merely opaque to a re-hash"
        );
    }

    #[tokio::test]
    async fn inactive_and_unconfigured_tenants_are_scrubbed() {
        for src in [
            FixedSource(Ok(None)),
            FixedSource(Ok(Some(cfg(ByokState::Inactive)))),
        ] {
            let cache = cache_of(src);
            assert_eq!(
                classify_tenant(Some(&cache), "11111111-1111-1111-1111-111111111111").await,
                TenantPlan::Scrub
            );
        }
    }

    #[tokio::test]
    async fn unresolvable_and_erroring_tenants_are_failed_never_plaintext() {
        // `Partial` is the fail-closed arm of `engagement_for`.
        let cache = cache_of(FixedSource(Ok(Some(cfg(ByokState::Partial)))));
        assert_eq!(
            classify_tenant(Some(&cache), "11111111-1111-1111-1111-111111111111").await,
            TenantPlan::Unclassifiable,
            "an active-but-unresolvable tenant must be visible, not silently skipped"
        );
        let cache = cache_of(FixedSource(Err(())));
        assert_eq!(
            classify_tenant(Some(&cache), "11111111-1111-1111-1111-111111111111").await,
            TenantPlan::Unclassifiable,
            "a config-read failure must never be read as `encryption off`"
        );
    }

    #[tokio::test]
    async fn public_namespace_stays_plaintext_even_for_an_active_config() {
        // `_public` is deterministic public content with no secret and is kept
        // plaintext by the storage layer to preserve cross-tenant dedup; the
        // sweep must agree or it would skip the one namespace it can verify.
        let cache = cache_of(FixedSource(Ok(Some(cfg(ByokState::Active)))));
        assert_eq!(
            classify_tenant(Some(&cache), crate::adapter_cache::PUBLIC_NAMESPACE).await,
            TenantPlan::Scrub
        );
    }

    #[tokio::test]
    async fn no_byok_cache_means_plaintext_everywhere() {
        assert_eq!(
            classify_tenant(None, "11111111-1111-1111-1111-111111111111").await,
            TenantPlan::Scrub
        );
    }

    // ── Key parsing: the algo comes from the key, not from an assumption ────

    #[test]
    fn blake3_and_sha256_keys_carry_their_own_algo() {
        let (d, a) = key_to_digest("iad/0123456789abcdef/deadbeef").unwrap();
        assert_eq!((d, a), ("deadbeef", DigestAlgo::Blake3));
        let (d, a) = key_to_digest("iad/0123456789abcdef/bazel/sha256/cafebabe").unwrap();
        assert_eq!(
            (d, a),
            ("cafebabe", DigestAlgo::Sha256),
            "re-hashing a REAPI blob with BLAKE3 would report a violation against intact data"
        );
    }

    #[test]
    fn non_blob_keys_are_not_treated_as_digests() {
        for key in [
            "iad/0123456789abcdef",
            "iad/0123456789abcdef/bazel/sha512/x",
            "iad/0123456789abcdef/a/b/c/d",
            "",
        ] {
            assert!(key_to_digest(key).is_none(), "unexpected parse of {key:?}");
        }
    }

    // ── Prefix derivation: fail CLOSED, never a shared keyspace ────────────

    #[test]
    fn non_uuid_tenant_fails_closed() {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([7u8; 32]));
        let err = tenant_prefix(&tdk, "not-a-uuid").unwrap_err();
        assert!(
            err.contains("refusing degraded prefix"),
            "a truncate-and-pad fallback collapses tenants into one keyspace: {err}"
        );
    }

    #[test]
    fn public_prefix_matches_the_storage_layer_exactly() {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([7u8; 32]));
        assert_eq!(
            tenant_prefix(&tdk, crate::adapter_cache::PUBLIC_NAMESPACE).unwrap(),
            public_namespace_prefix(&tdk),
            "the sweep must address the same prefix the `_public` writer created"
        );
    }

    #[test]
    fn uuid_tenant_derives_the_writer_prefix() {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([7u8; 32]));
        let uid = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(
            tenant_prefix(&tdk, "11111111-1111-1111-1111-111111111111").unwrap(),
            derive_prefix(&tdk, uid).to_string()
        );
    }

    // ── The counters ───────────────────────────────────────────────────────

    #[test]
    fn the_three_counters_are_independent() {
        // The load-bearing property of the whole endpoint: a sweep that
        // enumerated nothing must not look like a healthy one. Folding skips
        // into `examined` would report full coverage over an unchecked store.
        let s = ScrubSummary {
            examined: 3,
            skipped_encrypted: 5,
            failed: 1,
            ..ScrubSummary::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["examined"], 3);
        assert_eq!(v["skipped_encrypted"], 5);
        assert_eq!(v["failed"], 1);
    }

    #[test]
    fn a_cursor_round_trips_including_the_in_tenant_position() {
        // A sweep that runs out of budget mid-tenant must resume INSIDE that
        // tenant; restarting it from the top would re-examine the same head
        // forever and never reach the tail.
        let c = ScrubCursor {
            tenant_offset: 12,
            tenant_id: Some("11111111-1111-1111-1111-111111111111".to_owned()),
            region_idx: 2,
            r2_cursor: Some("opaque-token".to_owned()),
        };
        let back: ScrubCursor = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn an_absent_cursor_starts_from_the_beginning() {
        let c: ScrubCursor = serde_json::from_str("{}").unwrap();
        assert_eq!(c, ScrubCursor::default());
        assert_eq!(c.tenant_offset, 0);
        assert!(c.tenant_id.is_none());
    }

    // ── Auth ───────────────────────────────────────────────────────────────

    #[test]
    fn internal_auth_rejects_absent_wrong_and_prefix_keys() {
        let expected = b"k".repeat(32);
        let mut h = HeaderMap::new();
        assert!(!internal_auth_ok(&expected, &h), "absent header must fail");
        h.insert(INTERNAL_AUTH_HEADER, "k".repeat(31).parse().unwrap());
        assert!(!internal_auth_ok(&expected, &h), "a prefix must not pass");
        h.insert(INTERNAL_AUTH_HEADER, "x".repeat(32).parse().unwrap());
        assert!(!internal_auth_ok(&expected, &h));
        h.insert(INTERNAL_AUTH_HEADER, "k".repeat(32).parse().unwrap());
        assert!(internal_auth_ok(&expected, &h));
    }

    // ── The sweep never mutates ────────────────────────────────────────────

    #[test]
    fn the_scrubber_only_reads() {
        // A scrubber that can write is a scrubber that can destroy the evidence
        // it exists to find. Re-read this module's own source and prove no
        // mutating storage call appears in it.
        let src = include_str!("cas_scrub.rs");
        // Only the NON-test half is scanned: this assertion's own needles live
        // in the test module, and a whole-file scan would match itself.
        let prod = src
            .split_once("#[cfg(test)]")
            .map_or(src, |(before, _)| before);
        // Built at runtime for the same reason — a literal here would appear in
        // the scanned text if this test ever moves above the split.
        for method in ["put", "put_if_absent", "delete", "delete_if_present"] {
            let banned = format!(".{method}(");
            assert!(
                !prod.contains(&banned),
                "cas_scrub must never call {banned} — it reads and reports only"
            );
        }
        assert!(
            prod.contains(".list_objects_page(") && prod.contains(".get("),
            "the scan would pass vacuously if the read calls were renamed away"
        );
    }
}
