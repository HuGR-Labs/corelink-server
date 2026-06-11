//! Real R2 Action Cache erase adapter (`BackendKind::R2Ac`), WI-S11-008
//! Wave 1 increment 3. Hard-deletes a tenant's REAPI Action-Cache result
//! envelopes from the per-region `corelink-ac-<region>` R2 buckets, then the
//! `ac_meta` D1 index rows.
//!
//! AC topology (cold-verified 2026-06-11, ADR-S11-013 §R2-erasure):
//! - AC is stored as **per-region R2 buckets** `corelink-ac-{sam,iad,lhr,nrt,syd}`
//!   (region in the bucket NAME) — unlike CAS (one bucket, region in the key).
//! - `ac_meta` (`migrations/d1/0002`) is the **authoritative per-tenant index**:
//!   PK `(tenant_id, action_digest)` + `region` + a materialised `tenant_prefix`
//!   BLOB. So AC erase is fully D1-driven — **no S3 LIST needed** (contrast CAS,
//!   whose native whole-blob path has no durable index — see the ADR).
//! - The R2 object key is `<region>/<tenant_prefix_hex>/<action_digest>`
//!   ([`R2S3Client::blob_key`]). Reading the **materialised** `tenant_prefix` is
//!   rotation-correct: it is frozen at write time, so an entry written under a
//!   now-rotated `path_key_id` still resolves to the bytes it was stored under
//!   (re-deriving from the current TDK would not).
//!
//! `DeleteObject` is idempotent, so a replayed erasure is a safe no-op.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use corelink_privacy_erasure_worker::error::ErasureBackendError;
use corelink_privacy_erasure_worker::event::{BackendErasureOutcome, BackendKind};

use corelink_tenant_path::derive_prefix;

use super::d1util::{col_str, d1_query_blocking, load_tdk, scalar_count};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;
use crate::storage::StorageEnv;

/// Default per-region AC bucket-name prefix: `<prefix><region>` =
/// `corelink-ac-iad`, …, matching the `[[r2_buckets]]` bindings in
/// `wrangler.toml`. Overridable via `R2_AC_BUCKET_PREFIX` (non-prod envs).
const DEFAULT_AC_BUCKET_PREFIX: &str = "corelink-ac-";

/// Real R2 Action-Cache erase adapter.
pub(super) struct R2AcEraseAdapter {
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

    /// `corelink-ac-<region>` bucket name for a storage region.
    fn bucket_for(&self, region: &str) -> String {
        ac_bucket_name(&self.bucket_prefix, region)
    }
}

/// `<prefix><region>` → `corelink-ac-iad`, … (free fn for unit testing).
fn ac_bucket_name(prefix: &str, region: &str) -> String {
    format!("{prefix}{region}")
}

/// Delete every `(bucket, key)` R2 target, building one client per distinct
/// bucket. Driven inside a single `block_in_place`/`block_on` bridge — same
/// safety envelope as [`super::d1util::d1_query_blocking`]. Returns the count
/// of objects deleted (`DeleteObject` is idempotent, so a missing object still
/// counts as erased).
fn delete_r2_targets(targets: &[(String, String)]) -> Result<u64, ErasureBackendError> {
    if targets.is_empty() {
        return Ok(0);
    }
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport(
                    "StorageEnv unavailable for R2 AC erase".to_owned(),
                )
            })?;

            // Pre-build one client per distinct bucket (avoids a conditional
            // insert mid-loop; clients are cheap to construct).
            let mut buckets: Vec<&str> = targets.iter().map(|(b, _)| b.as_str()).collect();
            buckets.sort_unstable();
            buckets.dedup();
            let mut clients: HashMap<&str, R2S3Client> = HashMap::new();
            for b in buckets {
                let c = R2S3Client::new(&env, b.to_owned())
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                clients.insert(b, c);
            }

            let mut deleted = 0u64;
            for (bucket, key) in targets {
                let Some(client) = clients.get(bucket.as_str()) else {
                    return Err(ErasureBackendError::Transport(format!(
                        "no R2 client for bucket {bucket}"
                    )));
                };
                client
                    .delete(key)
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                deleted = deleted.saturating_add(1);
            }
            Ok(deleted)
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
        let tid = tenant_id.to_string();

        // The R2 object prefix is derived ONCE per tenant — identical to the
        // CAS/AC write path `derive_prefix(tdk, tenant).to_string()` (16-char
        // `URL_SAFE_NO_PAD(HMAC)[..16]`), so the erase keys match the stored
        // objects by construction. No TDK ⇒ fail CLOSED (cannot address them).
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive AC tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, tenant_id).to_string();

        // 1. Read the authoritative AC index for this tenant.
        let rows = d1_query_blocking(
            &self.d1,
            "SELECT region, action_digest FROM ac_meta WHERE tenant_id = ?1",
            vec![json!(tid)],
        )
        .map_err(ErasureBackendError::Transport)?;
        if rows.is_empty() {
            // No AC entries for this tenant → nothing to erase.
            return Ok(BackendErasureOutcome::NotApplicable);
        }

        // 2. Resolve each index row to its (bucket, R2 key)
        //    `<region>/<prefix>/<action_digest>`. A row that cannot be resolved
        //    is a SEV-1 data anomaly — fail CLOSED (Transport error →
        //    orchestrator retry) rather than silently skip a PII object.
        let mut targets: Vec<(String, String)> = Vec::with_capacity(rows.len());
        for row in &rows {
            let region = col_str(row, "region").ok_or_else(|| {
                ErasureBackendError::Transport("ac_meta.region missing/non-text".to_owned())
            })?;
            let digest = col_str(row, "action_digest").ok_or_else(|| {
                ErasureBackendError::Transport("ac_meta.action_digest missing/non-text".to_owned())
            })?;
            let key = R2S3Client::blob_key(&region, &prefix, &digest);
            targets.push((self.bucket_for(&region), key));
        }

        // 3. Delete the R2 envelopes, THEN the D1 index rows (so a failure
        //    mid-R2 leaves the index intact for an idempotent retry).
        let deleted = delete_r2_targets(&targets)?;
        d1_query_blocking(
            &self.d1,
            "DELETE FROM ac_meta WHERE tenant_id = ?1",
            vec![json!(tid)],
        )
        .map_err(ErasureBackendError::Transport)?;

        Ok(BackendErasureOutcome::Erased {
            records_deleted: deleted,
        })
    }

    fn verification_hash(
        &self,
        ctx: VerificationContext,
    ) -> Result<[u8; 32], ErasureBackendError> {
        // The ac_meta index is deleted last; a fully-erased tenant has zero
        // remaining rows. (The R2 envelopes are keyed only via this index, so
        // an empty index ⇒ no orphan can be reached.)
        let rows = d1_query_blocking(
            &self.d1,
            "SELECT COUNT(*) AS n FROM ac_meta WHERE tenant_id = ?1",
            vec![json!(ctx.tenant_id.to_string())],
        )
        .map_err(ErasureBackendError::Transport)?;
        let remaining = scalar_count(&rows, "n");
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
    fn ac_key_layout_matches_handler() {
        // The erase key MUST equal what R2AcHandler wrote:
        // <region>/<tenant_prefix_hex>/<action_digest>.
        let key = R2S3Client::blob_key("iad", "abcdef1234567890", "a".repeat(64).as_str());
        assert_eq!(key, format!("iad/abcdef1234567890/{}", "a".repeat(64)));
    }

    #[test]
    fn empty_target_set_deletes_nothing() {
        assert_eq!(delete_r2_targets(&[]).unwrap(), 0);
    }
}
