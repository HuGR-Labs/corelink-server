//! Real R2 Action Cache erase adapter (`BackendKind::R2Ac`), WI-S11-008
//! Wave 1 increment 3. Hard-deletes a tenant's REAPI Action-Cache result
//! envelopes from the per-region `corelink-ac-<region>` R2 buckets.
//!
//! AC topology (cold-verified 2026-06-11, ADR-S11-013 §R2-erasure; corrected
//! 2026-06-22 — F-003; endpoint-scoped 2026-08-18 — EU false-completion fix):
//! - AC is stored as **per-region R2 buckets** whose name is the container's
//!   `R2_AC_BUCKET` env (`corelink-ac-iad`/`-sam`/`-nrt`/`-syd` on the US
//!   endpoint, **`corelink-ac-eu`** on the physically separate EU endpoint) —
//!   unlike CAS (one bucket, region in the key). The R2 object key is
//!   `<region>/<tenant_prefix_hex>/<action_digest>` ([`R2S3Client::blob_key`]).
//! - **Erasure is LIST-by-prefix, NOT index-driven.** The live Bazel REAPI AC
//!   write path (`R2AcHandler::update`) PUTs the AC envelope to R2 but writes
//!   **no `ac_meta` D1 index row** (repo-wide `ac_meta` writers = 0). The old
//!   `ac_meta`-driven erase therefore ALWAYS short-circuited on an empty index
//!   (`NotApplicable`, no R2 delete) while still signing a `VerifiedComplete`
//!   attestation — a GDPR Art.17 false-completion (F-003).
//! - **Each container erases its OWN `R2_AC_BUCKET`** — the exact bucket its
//!   WRITE path used (`routes/ac.rs` reads the same `env_or("R2_AC_BUCKET",
//!   "corelink-ac-iad")`), always reachable from this container's S3 endpoint.
//!   The multi-region erase fan-out (`worker/src/index.ts`, fail-CLOSED — any
//!   regional leg non-2xx ⇒ 502 retry) sends the erase to EVERY region's
//!   container, so each container erasing its own bucket makes the **union
//!   complete**. `tenant.primary_region` is immutable (`migrations/d1/0028`), so
//!   a tenant's AC bytes only ever live in its home region's bucket.
//!   ⚠️ Pre-2026-08-18 this swept hardcoded `corelink-ac-<region>` names for all
//!   five regions, which (a) NEVER listed the EU bucket `corelink-ac-eu`
//!   (⇒ every EU-tenant erase falsely signed VerifiedComplete while EU AC bytes
//!   survived) and (b) tried US bucket names against the EU endpoint. Reading
//!   `R2_AC_BUCKET` — as CAS already does with `R2_CAS_BUCKET` — closes both.
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

/// Default AC bucket when `R2_AC_BUCKET` is unset (dev/CI + the iad default) —
/// matches the `routes/ac.rs` WRITE path default (`env_or("R2_AC_BUCKET",
/// "corelink-ac-iad")`), so erase targets the SAME bucket the writer used.
const DEFAULT_AC_BUCKET: &str = "corelink-ac-iad";

// Canonical AC region key-prefixes — the leading `<region>/` segment of every AC
// object key `<region>/<tenant_prefix>/<digest>`. Shared SINGLE SOURCE OF TRUTH
// with the CAS sweep (`crate::storage::region_map::CAS_REGIONS`). We sweep ALL of
// these prefixes WITHIN THIS container's own bucket (below) — cheap belt-and-
// suspenders that stays complete even if a legacy object was written under a
// different region key; `tenant.primary_region` is immutable
// (`migrations/d1/0028`), so in practice only the home prefix is populated.
use crate::storage::region_map::CAS_REGIONS as AC_REGIONS;

/// Real R2 Action-Cache erase adapter.
pub(super) struct R2AcEraseAdapter {
    // Retained for interface symmetry with the other DSR adapters (they all take
    // a shared D1 client); the AC erase itself is now purely R2 LIST-by-prefix
    // (no `ac_meta` index read — see the module doc / F-003).
    #[allow(
        dead_code,
        reason = "kept for adapter-construction symmetry with the sibling DSR adapters"
    )]
    d1: Arc<D1HttpClient>,
    /// This container's OWN AC bucket (`R2_AC_BUCKET`) — the SAME bucket its
    /// write path targets (`routes/ac.rs`), always reachable from this
    /// container's S3 endpoint. The multi-region erase fan-out (fail-CLOSED)
    /// sends the erase to EVERY region's container, so each container erasing
    /// its own bucket makes the union complete — while never listing a bucket on
    /// a different jurisdiction's endpoint (the pre-2026-08-18 bug: it swept
    /// hardcoded `corelink-ac-<region>` names, missing the EU bucket
    /// `corelink-ac-eu` entirely AND trying US buckets from the EU endpoint).
    write_bucket: String,
}

impl std::fmt::Debug for R2AcEraseAdapter {
    // Redact the inner client (holds the CF API token).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("R2AcEraseAdapter")
            .field("d1", &"[D1HttpClient]")
            .field("write_bucket", &self.write_bucket)
            .finish()
    }
}

impl R2AcEraseAdapter {
    /// Construct over a shared [`D1HttpClient`]. The S3 client is built lazily
    /// inside `erase`/`verification_hash` (request-time, where the
    /// `block_in_place` runtime context is guaranteed — `StorageEnv` is not
    /// `Clone`, so it is re-read from env there rather than stored). The AC
    /// bucket is resolved from `R2_AC_BUCKET` — byte-for-byte the same env the
    /// WRITE path reads (`routes/ac.rs` `env_or("R2_AC_BUCKET",
    /// "corelink-ac-iad")`) — so the erase can never miss a bucket the writer
    /// used (the EU `corelink-ac-eu` false-completion, fixed 2026-08-18).
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        let write_bucket = crate::storage::env_or("R2_AC_BUCKET", DEFAULT_AC_BUCKET);
        Self { d1, write_bucket }
    }
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
fn list_and_delete_ac(write_bucket: &str, tenant_prefix: &str) -> Result<u64, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport("StorageEnv unavailable for R2 AC erase".to_owned())
            })?;
            // ONE bucket — THIS container's own `R2_AC_BUCKET`, reachable from its
            // own S3 endpoint (the fan-out reaches every region's container, so the
            // union is complete). Sweep every region KEY-prefix within it (cheap;
            // immutable primary_region ⇒ only the home prefix is ever populated).
            let client = R2S3Client::new(&env, write_bucket.to_owned())
                .await
                .map_err(ErasureBackendError::Transport)?;
            let mut deleted = 0u64;
            for region in AC_REGIONS {
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
fn count_ac_remaining(write_bucket: &str, tenant_prefix: &str) -> Result<u64, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport("StorageEnv unavailable for R2 AC verify".to_owned())
            })?;
            // Count the ACTUAL write bucket (this container's `R2_AC_BUCKET`) so a
            // non-empty EU `corelink-ac-eu` BLOCKS VerifiedComplete (fail-CLOSED →
            // VerifiedPartial) instead of silently passing against a wrong/empty
            // bucket name — the false-completion this fix closes.
            let client = R2S3Client::new(&env, write_bucket.to_owned())
                .await
                .map_err(ErasureBackendError::Transport)?;
            let mut remaining = 0u64;
            for region in AC_REGIONS {
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
        let deleted = list_and_delete_ac(&self.write_bucket, &prefix)?;

        Ok(BackendErasureOutcome::Erased {
            records_deleted: deleted,
        })
    }

    fn verification_hash(&self, ctx: VerificationContext) -> Result<[u8; 32], ErasureBackendError> {
        // Verify against ACTUAL R2 objects (NOT the `ac_meta` D1 index, which the
        // live write path never populates — F-003): count everything still under
        // the tenant's `<region>/<prefix>/` across the regional buckets.
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive AC tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, ctx.tenant_id).to_string();
        let remaining = count_ac_remaining(&self.write_bucket, &prefix)?;
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
    fn erase_targets_this_containers_own_write_bucket() {
        // The erase must read the SAME env the WRITE path (routes/ac.rs) reads —
        // `R2_AC_BUCKET` — so it can never miss a bucket the writer used. On the
        // EU container that env is `corelink-ac-eu`; the pre-fix code hardcoded
        // `corelink-ac-<region>` and never listed it (the false-completion bug).
        // Uses a UNIQUELY-named env var to stay parallel-test-safe (env_or is a
        // pure fn of the var it is given).
        assert_eq!(DEFAULT_AC_BUCKET, "corelink-ac-iad");
        const PROBE: &str = "R2_AC_BUCKET_PROBE_ACTEST";
        std::env::set_var(PROBE, "corelink-ac-eu");
        assert_eq!(
            crate::storage::env_or(PROBE, DEFAULT_AC_BUCKET),
            "corelink-ac-eu",
            "EU container erases its own R2_AC_BUCKET (corelink-ac-eu), not a hardcoded name"
        );
        std::env::remove_var(PROBE);
        assert_eq!(
            crate::storage::env_or(PROBE, DEFAULT_AC_BUCKET),
            "corelink-ac-iad",
            "unset ⇒ the write-path default (corelink-ac-iad)"
        );
    }

    #[test]
    fn five_canonical_ac_region_key_prefixes() {
        // The region KEY-prefixes swept within the single own-bucket — same set +
        // order as the DSR CAS adapter (adapter_r2_cas::CAS_REGIONS).
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
