//! Real R2 Action Cache erase adapter (`BackendKind::R2Ac`), WI-S11-008
//! Wave 1 increment 3. Hard-deletes a tenant's REAPI Action-Cache result
//! envelopes from the per-region `corelink-ac-<region>` R2 buckets.
//!
//! AC topology (cold-verified 2026-06-11, ADR-S11-013 §R2-erasure; corrected
//! 2026-06-22 — F-003):
//! - AC is stored as **per-region R2 buckets** `corelink-ac-{sam,iad,lhr,nrt,syd}`
//!   (region in the bucket NAME) — unlike CAS (one bucket, region in the key).
//! - The R2 object key is `<region>/<tenant_prefix_hex>/<action_digest>`
//!   ([`R2S3Client::blob_key`]).
//! - **Erasure is LIST-by-prefix, NOT index-driven.** The live Bazel REAPI AC
//!   write path (`R2AcHandler::update`) PUTs the AC envelope to R2 but writes
//!   **no `ac_meta` D1 index row** (repo-wide `ac_meta` writers = 0). The old
//!   `ac_meta`-driven erase therefore ALWAYS short-circuited on an empty index
//!   (`NotApplicable`, no R2 delete) while still signing a `VerifiedComplete`
//!   attestation — a GDPR Art.17 false-completion (F-003). We now mirror the
//!   CAS adapter (`adapter_r2_cas`): LIST every object under
//!   `<region>/<tenant_prefix>/` across the five regional AC buckets and DELETE
//!   it — **complete by construction**, independent of any D1 index, and robust
//!   to a deployment whose region changed over time.
//! - `tenant_prefix` is `derive_prefix(tdk, tenant).to_string()` — the SAME
//!   derivation the writer used, so the LIST prefix matches the stored objects
//!   by construction. No TDK ⇒ fail CLOSED (cannot address them).
//!
//! `DeleteObject` is idempotent, so a replayed erasure is a safe no-op.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use corelink_privacy_erasure_worker::error::ErasureBackendError;
use corelink_privacy_erasure_worker::event::{BackendErasureOutcome, BackendKind};

use corelink_tenant_path::derive_prefix;

use super::d1util::load_tdk;
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;
use crate::storage::StorageEnv;

/// Default per-region AC bucket-name prefix: `<prefix><region>` =
/// `corelink-ac-iad`, …, matching the `[[r2_buckets]]` bindings in
/// `wrangler.toml`. Overridable via `R2_AC_BUCKET_PREFIX` (non-prod envs).
const DEFAULT_AC_BUCKET_PREFIX: &str = "corelink-ac-";

/// Canonical AC storage regions — the per-region bucket SUFFIX (`corelink-ac-<r>`)
/// AND the leading `<region>/` key segment. Byte-for-byte the same five-region
/// sweep the DSR Wave 1 CAS adapter uses (`adapter_r2_cas::CAS_REGIONS`). The
/// container writes AC through a single global region, but a once-per-account
/// erase sweeps all five buckets so it is robust to a write region that changed
/// over time (cheap — a once-per-erase LIST per bucket).
const AC_REGIONS: &[&str] = &["sam", "iad", "lhr", "nrt", "syd"];

/// Real R2 Action-Cache erase adapter.
pub(super) struct R2AcEraseAdapter {
    // Retained for interface symmetry with the other DSR adapters (they all take
    // a shared D1 client); the AC erase itself is now purely R2 LIST-by-prefix
    // (no `ac_meta` index read — see the module doc / F-003).
    #[allow(dead_code, reason = "kept for adapter-construction symmetry with the sibling DSR adapters")]
    d1: Arc<D1HttpClient>,
    bucket_prefix: String,
}

impl std::fmt::Debug for R2AcEraseAdapter {
    // Redact the inner client (holds the CF API token).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("R2AcEraseAdapter")
            .field("d1", &"[D1HttpClient]")
            .field("bucket_prefix", &self.bucket_prefix)
            .finish()
    }
}

impl R2AcEraseAdapter {
    /// Construct over a shared [`D1HttpClient`]. The per-region S3 clients are
    /// built lazily inside `erase`/`verification_hash` (request-time, where the
    /// `block_in_place` runtime context is guaranteed — `StorageEnv` is not
    /// `Clone`, so it is re-read from env there rather than stored).
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        let bucket_prefix = crate::storage::env_or("R2_AC_BUCKET_PREFIX", DEFAULT_AC_BUCKET_PREFIX);
        Self { d1, bucket_prefix }
    }
}

/// `<prefix><region>` → `corelink-ac-iad`, … (free fn for unit testing).
fn ac_bucket_name(prefix: &str, region: &str) -> String {
    format!("{prefix}{region}")
}

/// The per-region LIST prefix for a tenant: `<region>/<tenant_prefix>/`. This is
/// the leading path of every AC object key `<region>/<tenant_prefix>/<digest>`,
/// so LISTing it enumerates every Action-Cache envelope the tenant wrote in that
/// region — independent of any D1 index.
fn ac_list_prefix(region: &str, tenant_prefix: &str) -> String {
    format!("{region}/{tenant_prefix}/")
}

/// LIST every object under each region's `<region>/<tenant_prefix>/` prefix in
/// that region's `corelink-ac-<region>` bucket and DELETE it; returns the count
/// deleted. Single `block_in_place`/`block_on` bridge (same envelope as
/// [`super::d1util::d1_query_blocking`]). A region whose bucket holds nothing for
/// the tenant contributes zero and is a safe no-op (DeleteObject idempotency).
fn list_and_delete_ac(
    bucket_prefix: &str,
    tenant_prefix: &str,
) -> Result<u64, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport("StorageEnv unavailable for R2 AC erase".to_owned())
            })?;
            let mut deleted = 0u64;
            for region in AC_REGIONS {
                let bucket = ac_bucket_name(bucket_prefix, region);
                let client = R2S3Client::new(&env, bucket)
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                let prefix = ac_list_prefix(region, tenant_prefix);
                let keys = client
                    .list_objects_v2(&prefix)
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                for key in keys {
                    client
                        .delete(&key)
                        .await
                        .map_err(ErasureBackendError::Transport)?;
                    deleted = deleted.saturating_add(1);
                }
            }
            Ok(deleted)
        })
    })
}

/// Count AC objects still present under the tenant's prefix across all regional
/// buckets (verification sweep — counts ACTUAL R2 objects, not D1 rows).
fn count_ac_remaining(
    bucket_prefix: &str,
    tenant_prefix: &str,
) -> Result<u64, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport("StorageEnv unavailable for R2 AC verify".to_owned())
            })?;
            let mut remaining = 0u64;
            for region in AC_REGIONS {
                let bucket = ac_bucket_name(bucket_prefix, region);
                let client = R2S3Client::new(&env, bucket)
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                let prefix = ac_list_prefix(region, tenant_prefix);
                let keys = client
                    .list_objects_v2(&prefix)
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                remaining = remaining.saturating_add(keys.len() as u64);
            }
            Ok(remaining)
        })
    })
}

impl BackendErasureAdapter for R2AcEraseAdapter {
    fn kind(&self) -> BackendKind {
        BackendKind::R2Ac
    }

    fn erase(
        &self,
        tenant_id: Uuid,
        _subject_id: Uuid,
        _erasure_salt: &[u8; 32],
        legal_hold: bool,
    ) -> Result<BackendErasureOutcome, ErasureBackendError> {
        // Effective backend under legal hold: preserve (CTRL-PRIV-033).
        if legal_hold {
            return Ok(BackendErasureOutcome::NotApplicable);
        }

        // Derive the R2 object prefix the SAME way the AC write path did
        // (`derive_prefix(tdk, tenant)` → 16-char `URL_SAFE_NO_PAD(HMAC)[..16]`),
        // so the LIST prefix matches the stored objects by construction. No TDK
        // ⇒ fail CLOSED (cannot address them — never claim success).
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive AC tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, tenant_id).to_string();

        // LIST + DELETE every AC envelope under `<region>/<prefix>/` across the
        // five regional buckets. Complete by construction (no dead `ac_meta`
        // index dependency — see the module doc / F-003).
        let deleted = list_and_delete_ac(&self.bucket_prefix, &prefix)?;

        Ok(BackendErasureOutcome::Erased {
            records_deleted: deleted,
        })
    }

    fn verification_hash(
        &self,
        ctx: VerificationContext,
    ) -> Result<[u8; 32], ErasureBackendError> {
        // Verify against ACTUAL R2 objects (NOT the `ac_meta` D1 index, which the
        // live write path never populates — F-003): count everything still under
        // the tenant's `<region>/<prefix>/` across the regional buckets.
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive AC tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, ctx.tenant_id).to_string();
        let remaining = count_ac_remaining(&self.bucket_prefix, &prefix)?;
        if remaining == 0 {
            Ok(CANONICAL_EMPTY_TENANT_HASH)
        } else {
            // Non-empty → deterministic non-canonical fingerprint (Sha256, a
            // container dep) so the verify sweep maps it to VerifiedPartial.
            let mut h = Sha256::new();
            h.update(b"corelink/v1/r2-ac-erasure-remaining:");
            h.update(remaining.to_le_bytes());
            let digest = h.finalize();
            let mut out = [0u8; 32];
            out.copy_from_slice(&digest);
            Ok(out)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn kind_is_r2_ac() {
        assert_eq!(BackendKind::R2Ac.as_str(), "r2_ac");
    }

    #[test]
    fn bucket_name_is_per_region() {
        assert_eq!(ac_bucket_name(DEFAULT_AC_BUCKET_PREFIX, "iad"), "corelink-ac-iad");
        assert_eq!(ac_bucket_name(DEFAULT_AC_BUCKET_PREFIX, "syd"), "corelink-ac-syd");
    }

    #[test]
    fn five_canonical_ac_regions() {
        // Same sweep + order as the DSR CAS adapter (adapter_r2_cas::CAS_REGIONS).
        assert_eq!(AC_REGIONS, &["sam", "iad", "lhr", "nrt", "syd"]);
    }

    #[test]
    fn ac_list_prefix_is_leading_path_of_blob_key() {
        // The LIST prefix MUST be the leading path of the AC envelope key
        // <region>/<tenant_prefix_hex>/<action_digest> — so LISTing it enumerates
        // every envelope the tenant wrote (the F-003 complete-by-construction fix).
        let key = R2S3Client::blob_key(
            "iad",
            "abcdef1234567890",
            "a".repeat(64).as_str(),
            corelink_handler_cas::DigestAlgo::Blake3,
        );
        let prefix = ac_list_prefix("iad", "abcdef1234567890");
        assert!(key.starts_with(&prefix), "key={key} prefix={prefix}");
        assert_eq!(prefix, "iad/abcdef1234567890/");
    }
}
