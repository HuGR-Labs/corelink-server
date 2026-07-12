//! Real R2 CAS erase adapter (`BackendKind::R2Cas`), WI-S11-008 Wave 1
//! increment 3. Hard-deletes a tenant's content-addressed bytes from the
//! single `corelink-cas-prod` R2 bucket, then any CAS storage-layer D1 rows.
//!
//! CAS topology (cold-verified 2026-06-11 — the investigation logged in
//! ADR-S11-013 §R2-erasure):
//! - CAS is **one bucket** (`corelink-cas-prod`); the storage region is carried
//!   in the key prefix `<region>/<tenant_prefix>/<digest>` (contrast AC, which
//!   is region-per-bucket).
//! - The live prod write path is **native whole-blob only**: `R2CasHandler`
//!   PUTs the whole blob at `<cas_region>/<tenant_prefix>/<digest>`. The
//!   multipart/chunk path (`chunks`/`manifest_chunks`/`multipart_sessions`
//!   tables) is **NOT shipped** (handler is an in-memory sim; zero prod write
//!   sites), and there is **no durable D1 index** of native blobs.
//! - Therefore the only **complete** enumeration is **LIST-by-prefix**: delete
//!   everything under `<region>/<tenant_prefix>/` — complete by construction,
//!   independent of which component wrote the object. We LIST all five storage
//!   regions (the container's write region is a single global `R2_CAS_REGION`,
//!   but listing all five is cheap for a once-per-account erase and is robust
//!   to a deployment whose region changed over time).
//! - `tenant_prefix` is `derive_prefix(tdk, tenant).to_string()` — the SAME
//!   derivation the writer used, so the keys match by construction (see
//!   [`super::d1util::load_tdk`]).
//!
//! `DeleteObject` is idempotent, so a replayed erasure is a safe no-op.

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

use super::d1util::{d1_query_blocking, load_tdk};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;
use crate::storage::StorageEnv;

/// Canonical CAS storage regions (the `<region>/` key prefix; per
/// `migrations/d1/0003` chunks `region` CHECK + WI-S04-002 §1).
const CAS_REGIONS: &[&str] = &["sam", "iad", "lhr", "nrt", "syd"];

/// CAS storage-layer D1 tables to clean alongside the R2 objects. All are
/// `tenant_id`-keyed and (per the cold-verify) un-written in prod today — the
/// DELETE is a defensive no-op now and future-proofs the multipart path. NOT
/// in the `adapter_d1` control-plane erase-set (this adapter owns them).
const CAS_D1_TABLES: &[&str] = &[
    "chunks",
    "manifest_chunks",
    "multipart_sessions",
    "blob_meta",
];

/// Default single CAS bucket; overridable via `R2_CAS_BUCKET` (non-prod).
const DEFAULT_CAS_BUCKET: &str = "corelink-cas-prod";

/// Real R2 CAS erase adapter.
pub(super) struct R2CasEraseAdapter {
    d1: Arc<D1HttpClient>,
    cas_bucket: String,
}

impl std::fmt::Debug for R2CasEraseAdapter {
    // Redact the inner client (holds the CF API token).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("R2CasEraseAdapter")
            .field("d1", &"[D1HttpClient]")
            .field("cas_bucket", &self.cas_bucket)
            .finish()
    }
}

impl R2CasEraseAdapter {
    /// Construct over a shared [`D1HttpClient`].
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        let cas_bucket = crate::storage::env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
        Self { d1, cas_bucket }
    }
}

/// The per-region LIST prefix for a tenant: `<region>/<tenant_prefix>/`.
fn cas_list_prefix(region: &str, tenant_prefix: &str) -> String {
    format!("{region}/{tenant_prefix}/")
}

/// LIST every object under each region prefix and DELETE it; returns the count
/// deleted. Single `block_in_place`/`block_on` bridge (same envelope as
/// [`super::d1util::d1_query_blocking`]).
///
/// M-1 (go-live GDPR audit) — SCOPE GUARD FOR A FUTURE MULTIPART SHIP: this
/// sweeps the CAS bucket (`corelink-cas-*`) only. It does NOT touch the separate
/// `corelink-chunk-*` / `corelink-manifest-*` R2 buckets. That is correct +
/// complete TODAY because the container has zero multipart write-sites (no
/// `corelink-r2-multipart` dep; `R2_CHUNK_BUCKET` is never written). **If
/// multipart/chunked CAS writes are ever enabled, THIS function MUST be extended
/// to LIST-and-delete those buckets too** — otherwise chunked content survives a
/// "complete" erasure AND the Ed25519 attestation would falsely sign Complete.
/// Gate the multipart-enabling PR on extending this.
fn list_and_delete_cas(cas_bucket: &str, tenant_prefix: &str) -> Result<u64, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport("StorageEnv unavailable for R2 CAS erase".to_owned())
            })?;
            let client = R2S3Client::new(&env, cas_bucket.to_owned())
                .await
                .map_err(ErasureBackendError::Transport)?;
            let mut deleted = 0u64;
            for region in CAS_REGIONS {
                let prefix = cas_list_prefix(region, tenant_prefix);
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

/// Count objects still present under the tenant's prefix across all regions
/// (verification sweep).
fn count_cas_remaining(cas_bucket: &str, tenant_prefix: &str) -> Result<u64, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport(
                    "StorageEnv unavailable for R2 CAS verify".to_owned(),
                )
            })?;
            let client = R2S3Client::new(&env, cas_bucket.to_owned())
                .await
                .map_err(ErasureBackendError::Transport)?;
            let mut remaining = 0u64;
            for region in CAS_REGIONS {
                let prefix = cas_list_prefix(region, tenant_prefix);
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

impl BackendErasureAdapter for R2CasEraseAdapter {
    fn kind(&self) -> BackendKind {
        BackendKind::R2Cas
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
        // Derive the tenant prefix the SAME way the writer did. No TDK ⇒ fail
        // CLOSED (cannot address the objects ⇒ must not claim success).
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive CAS tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, tenant_id).to_string();

        // 1. LIST + DELETE every R2 object under the tenant's prefix.
        let deleted = list_and_delete_cas(&self.cas_bucket, &prefix)?;

        // 2. Defensive D1 cleanup (un-written in prod today; future-proofs the
        //    multipart path). Constant table names → injection-safe.
        let tid = tenant_id.to_string();
        for t in CAS_D1_TABLES {
            let sql = format!("DELETE FROM {t} WHERE tenant_id = ?1");
            d1_query_blocking(&self.d1, &sql, vec![json!(tid)])
                .map_err(ErasureBackendError::Transport)?;
        }

        Ok(BackendErasureOutcome::Erased {
            records_deleted: deleted,
        })
    }

    fn verification_hash(&self, ctx: VerificationContext) -> Result<[u8; 32], ErasureBackendError> {
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive CAS tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, ctx.tenant_id).to_string();
        let remaining = count_cas_remaining(&self.cas_bucket, &prefix)?;
        if remaining == 0 {
            Ok(CANONICAL_EMPTY_TENANT_HASH)
        } else {
            let mut h = Sha256::new();
            h.update(b"corelink/v1/r2-cas-erasure-remaining:");
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
    fn kind_is_r2_cas() {
        assert_eq!(BackendKind::R2Cas.as_str(), "r2_cas");
    }

    #[test]
    fn five_canonical_regions() {
        assert_eq!(CAS_REGIONS, &["sam", "iad", "lhr", "nrt", "syd"]);
    }

    #[test]
    fn list_prefix_layout_matches_blob_key() {
        // The LIST prefix MUST be the leading path of the whole-blob key
        // <region>/<prefix>/<digest>.
        let key = R2S3Client::blob_key(
            "iad",
            "abcdef1234567890",
            "deadbeef",
            corelink_handler_cas::DigestAlgo::Blake3,
        );
        let prefix = cas_list_prefix("iad", "abcdef1234567890");
        assert!(key.starts_with(&prefix), "key={key} prefix={prefix}");
    }

    #[test]
    fn cas_d1_tables_disjoint_from_control_plane() {
        // These storage-layer tables must NOT also be in the D1 adapter's
        // control-plane set (avoid double-ownership confusion). hot_blobs IS
        // in the D1 set and is intentionally NOT here.
        assert!(!CAS_D1_TABLES.contains(&"hot_blobs"));
        assert!(!CAS_D1_TABLES.contains(&"tenant"));
    }
}
