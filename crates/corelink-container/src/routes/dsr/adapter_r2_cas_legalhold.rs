//! Governance-mode legal-hold-aware CAS erase adapter
//! (`BackendKind::R2CasLegalHold`), B-009 / ADR-S11-013 §R2-erasure
//! legal-hold arm. Replaces the WI-S11-002 no-op stub that reconciled the
//! legal-hold CAS backend to `NotApplicable` ("no legal-hold CAS partition
//! shipped").
//!
//! ## Behaviour
//!
//! `erase(tenant, subject, salt, legal_hold)`:
//!
//! - **`legal_hold == true`** (litigation / retention freeze, Governance mode):
//!   the content-addressed bytes are FROZEN — we do NOT delete them. We LIST the
//!   tenant's CAS objects and, per object, write a [`cas_retention`] row so a
//!   later hold-release drain can finish the physical erasure. Recording the
//!   object anonymously (the retention row carries NO `subject_id`) is HOW the
//!   subject→object PII linkage is severed: the bytes remain but are no longer
//!   attributable to the data subject (GDPR Recital 26 pseudonymization).
//!   Returns [`BackendErasureOutcome::Pseudonymized`] with `records_redacted` =
//!   the number of held objects re-indexed under retention.
//! - **`legal_hold == false`**: the normal CAS delete — delegated to
//!   [`super::adapter_r2_cas::R2CasEraseAdapter`] (the same LIST+DELETE across
//!   `CAS_REGIONS` + defensive D1 index cleanup).
//!
//! ## Governance mode is CODE-reversible — NOT storage immutability
//!
//! "Governance" here means the freeze is enforced by THIS adapter's logic plus
//! the `cas_retention` record, and an authorised hold-release path can delete
//! the retained bytes after the hold ends. It does **not** assert R2 Object
//! Lock / S3 Object-Lock COMPLIANCE mode or any append-only storage primitive —
//! no such immutability is configured on the CAS bucket. Storage-enforced
//! (Compliance / Object-Lock) mode is explicitly deferred (see
//! `migrations/d1/0102_cas_retention.sql`).
//!
//! ## Verification
//!
//! [`BackendErasureAdapter::verification_hash`] is unified across both paths:
//! it asserts that no CAS object remains that is still attributable to the
//! subject — i.e. every object under the tenant prefix either was deleted
//! (no-hold path) or carries a `cas_retention` row (hold path). Zero
//! un-accounted objects ⇒ [`CANONICAL_EMPTY_TENANT_HASH`]; otherwise a
//! remaining-count fingerprint (mirrors the `adapter_r2_cas` re-LIST pattern).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use corelink_privacy_erasure_worker::error::ErasureBackendError;
use corelink_privacy_erasure_worker::event::{BackendErasureOutcome, BackendKind};
use corelink_tenant_path::derive_prefix;

use super::adapter_r2_cas::R2CasEraseAdapter;
use super::d1util::{col_str, d1_query_blocking, load_tdk};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;
use crate::storage::region_map::CAS_REGIONS;
use crate::storage::StorageEnv;

/// Default single CAS bucket; overridable via `R2_CAS_BUCKET` (non-prod).
/// Mirrors [`super::adapter_r2_cas`].
const DEFAULT_CAS_BUCKET: &str = "corelink-cas-prod";

/// Governance-mode legal-hold-aware CAS erase adapter.
pub(super) struct R2CasLegalHoldEraseAdapter {
    d1: Arc<D1HttpClient>,
    cas_bucket: String,
}

impl std::fmt::Debug for R2CasLegalHoldEraseAdapter {
    // Redact the inner client (holds the CF API token).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("R2CasLegalHoldEraseAdapter")
            .field("d1", &"[D1HttpClient]")
            .field("cas_bucket", &self.cas_bucket)
            .finish()
    }
}

impl R2CasLegalHoldEraseAdapter {
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

/// Canonical `cas_retention` INSERT (idempotent under a replayed hold). Pinned
/// as a constant so a test guards the column list + `'governance'` mode +
/// `CAST(... AS INTEGER)` against accidental drift.
const RETENTION_INSERT_SQL: &str = "INSERT OR IGNORE INTO cas_retention \
     (tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms) \
     VALUES (?1, ?2, ?3, NULL, 'governance', CAST(?4 AS INTEGER))";

/// Governance retention must never treat an S3 Compliance archive row as an
/// R2 legal-hold acknowledgement. The mode fence preserves the independent
/// deletion and retention semantics of the two planes.
const GOVERNANCE_RETENTION_LOOKUP_SQL: &str =
    "SELECT object_key FROM cas_retention WHERE tenant_id = ?1 AND mode = 'governance'";

#[cfg(test)]
fn governance_retained_keys(rows: &[(&str, &str)]) -> HashSet<String> {
    rows.iter()
        .filter(|(mode, _key)| *mode == "governance")
        .map(|(_mode, key)| (*key).to_owned())
        .collect()
}

/// Count CAS objects still ATTRIBUTABLE to the subject: present under the prefix
/// AND lacking a `cas_retention` row. Pure verification accounting — unified
/// across both erase paths:
/// - no-hold path deletes the objects (empty `objects`) ⇒ 0;
/// - hold path re-indexes every present object into `retained` ⇒ 0;
/// - a leftover object with no retention row ⇒ nonzero (verification mismatch).
fn unaccounted_count(objects: &[(String, String)], retained: &HashSet<String>) -> u64 {
    objects
        .iter()
        .filter(|(_region, key)| !retained.contains(key))
        .count() as u64
}

/// Current wall-clock as Unix epoch ms (saturating; the epoch is always after
/// `UNIX_EPOCH`).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// LIST every CAS object under each region prefix for a tenant WITHOUT
/// deleting. Returns `(region, full_key)` pairs. Same async→sync bridge
/// envelope as [`super::d1util::d1_query_blocking`].
fn list_cas_objects(
    cas_bucket: &str,
    tenant_prefix: &str,
) -> Result<Vec<(String, String)>, ErasureBackendError> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let env = StorageEnv::from_env().ok_or_else(|| {
                ErasureBackendError::Transport(
                    "StorageEnv unavailable for R2 CAS legal-hold list".to_owned(),
                )
            })?;
            let client = R2S3Client::new(&env, cas_bucket.to_owned())
                .await
                .map_err(ErasureBackendError::Transport)?;
            let mut out: Vec<(String, String)> = Vec::new();
            for region in CAS_REGIONS {
                let prefix = cas_list_prefix(region, tenant_prefix);
                let keys = client
                    .list_objects_v2(&prefix)
                    .await
                    .map_err(ErasureBackendError::Transport)?;
                for key in keys {
                    out.push(((*region).to_owned(), key));
                }
            }
            Ok(out)
        })
    })
}

impl BackendErasureAdapter for R2CasLegalHoldEraseAdapter {
    fn kind(&self) -> BackendKind {
        BackendKind::R2CasLegalHold
    }

    fn erase(
        &self,
        tenant_id: Uuid,
        _subject_id: Uuid,
        _erasure_salt: &[u8; 32],
        legal_hold: bool,
    ) -> Result<BackendErasureOutcome, ErasureBackendError> {
        // No active hold → this is an ordinary CAS erase. Delegate to the
        // effective CAS adapter (LIST+DELETE across CAS_REGIONS + D1 cleanup).
        if !legal_hold {
            return R2CasEraseAdapter::new(Arc::clone(&self.d1)).erase(
                tenant_id,
                _subject_id,
                _erasure_salt,
                false,
            );
        }

        // Active Governance-mode hold: preserve the frozen bytes, sever the
        // subject→object PII linkage by re-indexing each held object under the
        // subject-free `cas_retention` table so a later drain can complete the
        // physical erasure once the hold is released.
        //
        // No TDK ⇒ fail CLOSED: we cannot derive the tenant prefix, so we
        // cannot enumerate the held objects and must not claim to have
        // pseudonymized them.
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive CAS tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, tenant_id).to_string();

        let objects = list_cas_objects(&self.cas_bucket, &prefix)?;

        let tid = tenant_id.to_string();
        let ts = now_ms();
        let mut redacted: u64 = 0;
        for (region, key) in &objects {
            // INSERT OR IGNORE: a replayed erasure under a still-active hold is
            // an idempotent no-op (PRIMARY KEY collision is expected). Integer
            // columns CAST to INTEGER (the D1 REST binder ships numbers as REAL;
            // see the d1-migrations-ledger note). `retain_until_ms` is left NULL
            // — the hold end is admin-driven, not a timer, at launch.
            d1_query_blocking(
                &self.d1,
                RETENTION_INSERT_SQL,
                vec![json!(tid), json!(region), json!(key), json!(ts)],
            )
            .map_err(ErasureBackendError::Transport)?;
            redacted = redacted.saturating_add(1);
        }

        Ok(BackendErasureOutcome::Pseudonymized {
            records_redacted: redacted,
        })
    }

    fn verification_hash(&self, ctx: VerificationContext) -> Result<[u8; 32], ErasureBackendError> {
        let tdk = load_tdk().ok_or_else(|| {
            ErasureBackendError::Transport(
                "R2_TDK_HEX unavailable — cannot derive CAS tenant prefix".to_owned(),
            )
        })?;
        let prefix = derive_prefix(&tdk, ctx.tenant_id).to_string();

        // Every CAS object still present under the tenant prefix.
        let objects = list_cas_objects(&self.cas_bucket, &prefix)?;

        // Objects this tenant is holding under governance retention.
        let tid = ctx.tenant_id.to_string();
        let rows = d1_query_blocking(&self.d1, GOVERNANCE_RETENTION_LOOKUP_SQL, vec![json!(tid)])
            .map_err(ErasureBackendError::Transport)?;
        let retained: HashSet<String> = rows
            .iter()
            .filter_map(|r| col_str(r, "object_key"))
            .collect();

        // An object is still "attributable" only if it is present AND has no
        // retention row. Under the no-hold path all objects were deleted (list
        // empty); under the hold path all present objects were retained. Both
        // paths therefore yield zero un-accounted objects on success.
        let unaccounted = unaccounted_count(&objects, &retained);

        if unaccounted == 0 {
            Ok(CANONICAL_EMPTY_TENANT_HASH)
        } else {
            let mut h = Sha256::new();
            h.update(b"corelink/v1/r2-cas-legalhold-unaccounted:");
            h.update(unaccounted.to_le_bytes());
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
    fn kind_is_r2_cas_legalhold() {
        // Wire mnemonic is the PERSISTED form (D1 CHECK migration 0022 +
        // cloudevents enum); only the Rust variant identifier was de-stubbed.
        assert_eq!(
            BackendKind::R2CasLegalHold.as_str(),
            "r2_cas_legalhold_pseudo"
        );
    }

    #[test]
    fn kind_is_pseudonymized_not_effective() {
        // The legal-hold CAS backend is a pseudonymized backend: on the hold
        // path it retains + pseudonymizes rather than physically deleting.
        assert!(BackendKind::R2CasLegalHold.is_pseudonymized());
        assert!(!BackendKind::R2CasLegalHold.is_effective());
    }

    #[test]
    fn list_prefix_layout_matches_blob_key() {
        let key = R2S3Client::blob_key(
            "iad",
            "abcdef1234567890",
            "deadbeef",
            corelink_handler_cas::DigestAlgo::Blake3,
        );
        let prefix = cas_list_prefix("iad", "abcdef1234567890");
        assert!(key.starts_with(&prefix), "key={key} prefix={prefix}");
    }

    fn objs(keys: &[&str]) -> Vec<(String, String)> {
        keys.iter()
            .map(|k| ("iad".to_owned(), (*k).to_owned()))
            .collect()
    }

    #[test]
    fn hold_path_all_objects_retained_verifies_clean() {
        // Under an active hold the bytes are NOT deleted — every present object
        // is re-indexed into `cas_retention`. Verification must then see zero
        // un-accounted (still-attributable) objects ⇒ clean sentinel.
        let objects = objs(&["iad/pfx/aaa", "iad/pfx/bbb", "iad/pfx/ccc"]);
        let retained: HashSet<String> = objects.iter().map(|(_r, k)| k.clone()).collect();
        assert_eq!(unaccounted_count(&objects, &retained), 0);
    }

    #[test]
    fn no_hold_path_deleted_objects_verifies_clean() {
        // The no-hold path DELETEs the bytes: the post-erase LIST is empty ⇒
        // zero un-accounted objects ⇒ clean sentinel.
        let objects: Vec<(String, String)> = Vec::new();
        let retained: HashSet<String> = HashSet::new();
        assert_eq!(unaccounted_count(&objects, &retained), 0);
    }

    #[test]
    fn leftover_object_without_retention_row_fails_verification() {
        // A CAS object still present with NO retention row is still attributable
        // to the subject — verification MUST flag it (nonzero ⇒ mismatch).
        let objects = objs(&["iad/pfx/aaa", "iad/pfx/leak"]);
        let mut retained: HashSet<String> = HashSet::new();
        retained.insert("iad/pfx/aaa".to_owned());
        assert_eq!(unaccounted_count(&objects, &retained), 1);
    }

    #[test]
    fn retention_insert_sql_shape_pinned() {
        // Guards the persisted retention contract: correct table, subject-free
        // column list, governance mode, and INTEGER coercion of the ms column.
        assert!(RETENTION_INSERT_SQL.contains("INTO cas_retention"));
        assert!(RETENTION_INSERT_SQL.contains("'governance'"));
        assert!(RETENTION_INSERT_SQL.contains("CAST(?4 AS INTEGER)"));
        // The retention row carries NO subject_id (severs the PII linkage).
        assert!(!RETENTION_INSERT_SQL.contains("subject_id"));
    }

    #[test]
    fn governance_lookup_excludes_compliance_metadata_rows() {
        assert!(GOVERNANCE_RETENTION_LOOKUP_SQL.contains("mode = 'governance'"));
        assert!(!GOVERNANCE_RETENTION_LOOKUP_SQL.contains("'compliance'"));
        let retained = governance_retained_keys(&[
            ("governance", "iad/pfx/held-r2"),
            ("compliance", "audit/tenant/s3-version"),
        ]);
        assert_eq!(retained, HashSet::from(["iad/pfx/held-r2".to_owned()]));
    }
}
