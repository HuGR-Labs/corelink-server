/// In-memory blob eraser (tests). Records erased `(tenant, digest)` keys; a
/// real R2 deletion is composed via the #254 adapter in prod.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct InMemoryBlobEraser {
    erased: Mutex<HashSet<(String, String)>>,
}

impl InMemoryBlobEraser {
    /// Construct an empty eraser.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Was `(tenant, digest)` erased? (test assertion helper).
    #[must_use]
    pub fn was_erased(&self, tenant: &str, digest: &str) -> bool {
        let g = lock_or_recover(&self.erased);
        g.contains(&(tenant.to_owned(), digest.to_owned()))
    }
}

#[async_trait]
impl CasBlobEraser for InMemoryBlobEraser {
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String> {
        let mut g = lock_or_recover(&self.erased);
        g.insert((tenant.to_owned(), digest.to_owned()));
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Production R2 blob eraser (the #254 seam, now filled)
// ──────────────────────────────────────────────────────────────────────────────

// Canonical CAS storage regions — the `<region>/` key-prefix segment.
//
// Re-used from the SINGLE SOURCE OF TRUTH (`crate::storage::region_map::CAS_REGIONS`)
// so the per-hash erase sweep, the DSR CAS adapter, and the DSR AC adapter can
// never drift. CAS is **one bucket**; the storage region lives in the key prefix
// `<region>/<tenant_prefix>/<digest>`. The container writes through a single
// global `R2_CAS_REGION`, but a per-hash erase sweeps all regions so it is
// robust to a deployment whose write region changed over time (cheap — a
// once-per-erase LIST). Superset-safety vs the colo map is gated by
// `region_map::tests::cas_regions_superset_of_all_colos`.
use crate::storage::region_map::CAS_REGIONS;

/// Default single CAS bucket; overridable via `R2_CAS_BUCKET` (non-prod).
/// Mirrors `routes::dsr::adapter_r2_cas::DEFAULT_CAS_BUCKET`.
const DEFAULT_CAS_BUCKET: &str = "corelink-cas-prod";

/// Length (chars) of the materialised tenant prefix in an R2 key.
#[cfg(test)]
const TENANT_PREFIX_LEN: usize = 16;

/// Production [`CasBlobEraser`] over R2.
///
/// Given `(tenant, digest)`, derives the tenant prefix the **same way the CAS
/// writer did** ([`crate::storage::r2_s3::R2CasHandler`]'s `r2_key`: parse the
/// tenant as a UUID and `derive_prefix(tdk, uuid)`, else the raw-padded
/// fallback) and, for each of the five CAS regions, LISTs
/// `<region>/<tenant_prefix>/<digest>` and DELETEs the matching object(s) via
/// [`crate::storage::r2_s3::R2S3Client`]. Idempotent — re-erasing an absent
/// blob is a no-op success (S3 `DeleteObject` semantics).
///
/// Key layout matches by construction: the LIST prefix is built with the SAME
/// `R2S3Client::blob_key(region, prefix, digest)` leading path the writer
/// keys under, so a key-derivation mismatch (the earlier `R2Ac` silent-no-op
/// class of bug) is impossible.
#[non_exhaustive]
pub struct R2CasBlobEraser {
    /// Single CAS bucket name (e.g. `corelink-cas-prod`).
    cas_bucket: String,
    /// Tenant derivation key — required (fail-CLOSED without it).
    tdk: Arc<TenantDerivationKey>,
}

impl std::fmt::Debug for R2CasBlobEraser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact the TDK; surface only the bucket.
        f.debug_struct("R2CasBlobEraser")
            .field("cas_bucket", &self.cas_bucket)
            .field("tdk", &"[REDACTED]")
            .finish()
    }
}

impl R2CasBlobEraser {
    /// Construct over an explicit TDK + CAS bucket.
    #[must_use]
    pub fn new(tdk: Arc<TenantDerivationKey>, cas_bucket: String) -> Self {
        Self { cas_bucket, tdk }
    }

    /// Build the production eraser from env (`R2_TDK_HEX` + `R2_CAS_BUCKET`).
    /// `None` when the TDK is absent — fail-CLOSED, since without it the tenant
    /// (or `_public` sentinel) prefix cannot be derived and the erase could not
    /// address the stored objects. Reused by the `_public` revocation route
    /// ([`crate::routes::public_revoke`]) so both erase surfaces share one
    /// TDK-keyed derivation.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let tdk = load_tdk_from_env()?;
        let cas_bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
        let cas_region = crate::storage::env_or("R2_CAS_REGION", "iad");
        crate::storage::r2_s3::validate_cas_bucket_for_region(&cas_bucket, &cas_region).ok()?;
        Some(Self::new(Arc::new(tdk), cas_bucket))
    }

    /// Derive the 16-char tenant prefix the SAME way the CAS writer
    /// ([`crate::storage::r2_s3::R2CasHandler`]'s `r2_key`) did: parse the
    /// tenant as a UUID and `derive_prefix(tdk, uuid)`, else raw-pad/truncate
    /// the tenant string to 16 chars (the writer's non-UUID/dev fallback).
    /// Keeping the two derivations identical is what makes the erase key match
    /// the stored object **by construction**.
    fn tenant_prefix(&self, tenant: &str) -> Result<String, String> {
        // `_public` shared-dedup namespace (F3.2): NOT a UUID tenant. Its bytes
        // are stored under the reserved TDK-keyed sentinel prefix, NOT a
        // per-tenant HMAC. Derive via the SINGLE SOURCE the CAS writer uses
        // (`r2_s3::public_namespace_prefix`) so the public-revocation eraser
        // addresses the exact key the `_public` write path created. Placed
        // BEFORE the UUID parse (the sentinel is not a parseable UUID).
        if tenant == crate::adapter_cache::PUBLIC_NAMESPACE {
            return Ok(crate::storage::r2_s3::public_namespace_prefix(&self.tdk));
        }
        if let Ok(uid) = Uuid::try_parse(tenant) {
            return Ok(derive_prefix(&self.tdk, uid).to_string());
        }
        // Non-UUID tenant: FAIL CLOSED on the prod erasure path. A degraded
        // truncate+pad prefix collapses non-derivable tenants into a SHARED
        // keyspace — on the GDPR erasure path that risks erasing under (or
        // missing) the wrong tenant's prefix. Same fail-closed posture as the
        // CAS/AC storage layer (audit 2026-06-15 #2/#8). cfg(test) keeps the
        // deterministic pad for fixtures only.
        #[cfg(not(test))]
        {
            Err(format!(
                "non-derivable tenant '{tenant}' on R2 CAS erase — refusing degraded prefix (fail-closed)"
            ))
        }
        #[cfg(test)]
        {
            let mut p = tenant.to_owned();
            p.truncate(TENANT_PREFIX_LEN);
            while p.len() < TENANT_PREFIX_LEN {
                p.push('0');
            }
            Ok(p)
        }
    }
}

#[async_trait]
impl CasBlobEraser for R2CasBlobEraser {
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String> {
        let prefix = self.tenant_prefix(tenant)?;
        let env = crate::storage::StorageEnv::from_env()
            .ok_or_else(|| "StorageEnv unavailable for R2 CAS erase".to_owned())?;
        let client = crate::storage::r2_s3::R2S3Client::new(&env, self.cas_bucket.clone()).await?;
        for region in CAS_REGIONS {
            // The LIST prefix is the EXACT whole-blob key for this digest:
            // `<region>/<tenant_prefix>/<digest>` (per `R2S3Client::blob_key`).
            // LISTing it (rather than a bare DELETE) lets a re-erase of an
            // absent blob short-circuit and stays robust if a future multipart
            // path ever keys companion objects under the same digest prefix.
            // Native BLAKE3 keyspace — correct by design for this per-blob
            // erase: it is the native-CAS single-blob DELETE surface (clw D-1),
            // reached only with a BLAKE3 digest. Bazel REAPI exposes no
            // per-blob delete, and the Bazel `bazel/sha256/` keyspace is fully
            // covered by the GDPR Art.17 full-tenant erasure, which is
            // prefix-wide (`<region>/<tenant_prefix>/` → deletes everything
            // beneath, including `…/bazel/sha256/*`; see
            // `dsr/adapter_r2_cas.rs::list_and_delete_cas`). So a tenant wipe
            // leaves no Bazel residue; this path stays native-keyspace-scoped.
            let key = crate::storage::r2_s3::R2S3Client::blob_key(
                region,
                &prefix,
                digest,
                corelink_handler_cas::DigestAlgo::Blake3,
            );
            let keys = client.list_objects_v2(&key).await?;
            for k in keys {
                client.delete(&k).await?;
            }
        }
        Ok(())
    }
}

/// Build the route state from env.
///
/// Returns `Some` only when ALL of the prod transports build from env: the
/// **R2 TDK** (`R2_TDK_HEX`), the **D1 tombstone store**, the **D1 DSR
/// legitimacy store**, and the supplied **internal-auth key**. Any missing
/// piece ⇒ `None` and the route is NOT
/// mounted (fail-CLOSED): the container never runs a half-built erase that
/// could drop the tombstone without deleting the bytes, or — the load-bearing
/// failure mode — derive the WRONG R2 key and silently no-op the deletion
/// while writing a 410 tombstone (bytes-still-resident DSR breach). Without a
/// TDK the eraser cannot address the tenant's objects, so it MUST NOT be
/// constructed (mirrors the DSR `adapter_r2_cas` `load_tdk()` fail-CLOSED).
#[must_use]
pub fn build_state_from_env(internal_auth_key: Option<Arc<str>>) -> Option<CasEraseRouteState> {
    let internal_auth_key = internal_auth_key?;

    // Fail-CLOSED without a TDK: we cannot derive the tenant prefix the writer
    // used, so we cannot prove which R2 objects to delete (mirrors DSR).
    let tdk = load_tdk_from_env()?;

    // D1-backed tombstone store; absent D1 env ⇒ unmounted.
    let tombstones: Arc<dyn TombstoneStore> = Arc::new(D1TombstoneStore::from_env()?);

    // D1-backed DSR legitimacy gate (rt-nuclear #18/#19, r34 #8/#9). Fail
    // CLOSED in prod: if the legitimacy store cannot be built (no D1), the
    // route is NOT mounted — same posture as the erase-key/TDK gate above. We
    // MUST NOT mount the irreversible erase route without a legitimacy anchor,
    // or a leaked internal key would be sufficient to erase arbitrary blobs.
    let legitimacy: Arc<dyn DsrLegitimacyStore> = Arc::new(D1DsrLegitimacyStore::from_env()?);

    let cas_bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
    let cas_region = crate::storage::env_or("R2_CAS_REGION", "iad");
    crate::storage::r2_s3::validate_cas_bucket_for_region(&cas_bucket, &cas_region).ok()?;
    let eraser: Arc<dyn CasBlobEraser> = Arc::new(R2CasBlobEraser::new(Arc::new(tdk), cas_bucket));

    Some(CasEraseRouteState {
        tombstones,
        eraser,
        internal_auth_key,
        legitimacy: Some(legitimacy),
    })
}

/// Load the tenant derivation key from `R2_TDK_HEX` (64 hex chars = 32 bytes).
///
/// Mirrors `crate::storage::r2_s3::load_tdk_from_env` /
/// `routes::dsr::d1util::load_tdk` byte-for-byte (same env var, same length
/// gate, same hex decode) so the prefix this eraser derives matches the one
/// the writer/DSR adapter derive. Returns `None` (fail-CLOSED) when the var is
/// absent, the wrong length, or not valid hex.
fn load_tdk_from_env() -> Option<TenantDerivationKey> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        tracing::warn!(
            len = hex_str.len(),
            "cas_erase: R2_TDK_HEX has wrong length; eraser NOT built (route unmounted)"
        );
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    if hex::decode_to_slice(hex_str, bytes.as_mut()).is_err() {
        tracing::warn!(
            "cas_erase: R2_TDK_HEX is not valid hex; eraser NOT built (route unmounted)"
        );
        return None;
    }
    Some(TenantDerivationKey::from_bytes(bytes))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_1_1.rs");
    include!("b126_m2_test_1_1_part2.rs");
    include!("b126_m2_test_1_2.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_1_1_REANCHOR, B126_M2_TEST_1_2_REANCHOR];
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
