//! Host-side in-memory simulator of the WI-S04-002 `ac_meta` schema.
//!
//! The simulator is **not** a SQL parser — it is a hand-coded fake of
//! the `ac_meta` table that enforces every load-bearing invariant the
//! migration relies on:
//!
//! - `ac_meta` PRIMARY KEY uniqueness on `(tenant_id, action_digest)`.
//! - Every inline `CHECK` constraint:
//!   - `chk_ac_action_digest_len`: `length(action_digest) = 64`.
//!   - `chk_ac_result_hash_len`: `length(result_hash) = 64`.
//!   - `chk_ac_blob_refs_size`: `length(blob_refs) <= 10240`.
//!   - `chk_ac_blob_refs_count`: `0 <= blob_refs_count <= 4096`.
//!   - `chk_ac_result_size`: `0 <= result_size_bytes <= 1048576`.
//!   - `chk_ac_region`: `region IN ('sam','iad','lhr','nrt','syd')`.
//!   - `chk_ac_sig_alg`: `sig_alg = 'hkdf-sha256'`.
//!   - `chk_ac_lifecycle`: `last_hit_at >= created_at` and
//!     `expires_at IS NULL OR expires_at >= created_at`.
//!   - `chk_ac_tenant_prefix_len`: `length(tenant_prefix) = 16`.
//!   - `chk_ac_path_key_id_positive`, `chk_ac_sig_key_id_positive`.
//! - `INSERT … ON CONFLICT (tenant_id, action_digest) DO UPDATE`
//!   semantic that pins `INV-AC-IDEMPOTENT` +
//!   `INV-AC-RESULT-HASH-IMMUTABLE`.
//! - Tenant-isolation envelope (`SELECT … WHERE tenant_id = $ctx`):
//!   modelled by [`AcSchema::list_for_tenant`] which returns only the
//!   rows whose `tenant_id` matches the active context.
//!
//! Pure side-effect-free Rust — no async, no I/O. Property tests in
//! `tests/prop_ac_schema.rs` drive 10 000 iterations against this
//! surface.

use std::collections::BTreeMap;

use thiserror::Error;
use uuid::Uuid;

use crate::region::AcRegion;

/// Canonical hex digest length (BLAKE3-256 / SHA-256 hex).
pub const DIGEST_HEX_LEN: usize = 64;
/// Canonical materialized tenant_prefix length (HMAC truncation).
pub const TENANT_PREFIX_LEN: usize = 16;
/// Canonical max blob_refs JSON byte length.
pub const BLOB_REFS_SIZE_MAX: usize = 10_240;
/// Canonical max blob_refs_count denormalized count.
pub const BLOB_REFS_COUNT_MAX: i64 = 4_096;
/// Canonical max result_size_bytes envelope size.
pub const RESULT_SIZE_BYTES_MAX: i64 = 1_048_576;

/// All structured failure modes the simulator can report. Names mirror
/// the SQLite check / unique violation semantic (the PK violation for
/// the `(tenant_id, action_digest)` table is reported via the same
/// channel as a `UniqueViolation` on the PK column tuple).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SimError {
    /// Attempted to insert a row whose PK / UNIQUE column already
    /// exists. Mirrors SQLite `UNIQUE constraint failed`.
    #[error("unique_violation: {0}")]
    UniqueViolation(&'static str),

    /// CHECK constraint failed. Mirrors SQLite
    /// `CHECK constraint failed: <name>`.
    #[error("check_violation: {0}")]
    CheckViolation(&'static str),
}

/// Mirror of the schema `sig_alg` whitelisted enum (currently single
/// value; CHECK enforces). New values require a new ADR + new
/// migration per ADR-0036 Rule 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SigAlg {
    /// HKDF-SHA256 (canonical S-04 v1).
    HkdfSha256,
}

impl SigAlg {
    /// Canonical SQL literal value as it appears in the schema column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HkdfSha256 => "hkdf-sha256",
        }
    }
}

/// Materialised `ac_meta` row (mirror of the SQL row layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcMetaRow {
    /// PK component 1 — UUIDv7 minted app-side (canonical text form).
    pub tenant_id: Uuid,
    /// PK component 2 — REAPI v2 `Digest.hash` of the canonical Action
    /// proto (64 hex characters).
    pub action_digest: String,
    /// Materialised tenant prefix BLOB(16) — HMAC truncation per
    /// ADR-0035 H-3.
    pub tenant_prefix: [u8; TENANT_PREFIX_LEN],
    /// Path-derivation TDK version (forward-compat S-14).
    pub path_key_id: i64,
    /// BLAKE3-256 of canonical merkle_root (per ADR-0037; 64 hex).
    pub result_hash: String,
    /// JSON array of digests (output_files + output_dirs).
    pub blob_refs: String,
    /// Denormalised count for cheap COUNT(*).
    pub blob_refs_count: i64,
    /// Envelope payload size (cost/quota observability).
    pub result_size_bytes: i64,
    /// Lifecycle timestamps (unix ms).
    pub created_at: i64,
    /// Lifecycle timestamps (unix ms).
    pub last_hit_at: i64,
    /// Optional TTL (unix ms; NULL = no expiry pre-S-07).
    pub expires_at: Option<i64>,
    /// Sig key id (WI-S04-004 binding; >= 1; 0 reserved sentinel).
    pub sig_key_id: i64,
    /// Whitelisted sig algorithm.
    pub sig_alg: SigAlg,
    /// Region scope (R2 envelope location).
    pub region: AcRegion,
    /// Source PAT id (audit cross-check; NULL pre-PAT migration).
    pub created_by_pat_id: Option<String>,
    /// Request id correlation (NULL pre-PAT migration).
    pub created_by_request_id: Option<String>,
}

/// Argument bundle for [`AcSchema::upsert`].
#[derive(Clone, Debug)]
pub struct AcUpsertRequest {
    /// PK component 1.
    pub tenant_id: Uuid,
    /// PK component 2 — 64-char hex digest.
    pub action_digest: String,
    /// Materialised tenant prefix (handler computes via
    /// `corelink_tenant_path::derive_prefix(tdk, tenant_id)` once at
    /// INSERT time per ADR-0035 H-3).
    pub tenant_prefix: [u8; TENANT_PREFIX_LEN],
    /// Path-derivation TDK version.
    pub path_key_id: i64,
    /// BLAKE3 of canonical merkle_root; 64 hex.
    pub result_hash: String,
    /// JSON array TEXT of blob_refs.
    pub blob_refs: String,
    /// Denormalised count.
    pub blob_refs_count: i64,
    /// Envelope payload size.
    pub result_size_bytes: i64,
    /// Sig key id (>= 1).
    pub sig_key_id: i64,
    /// Algorithm.
    pub sig_alg: SigAlg,
    /// Region.
    pub region: AcRegion,
    /// Unix epoch ms — handler clock (sets created_at on INSERT,
    /// last_hit_at always).
    pub now_ms: i64,
    /// Optional TTL — `Some(delta_ms)` ⇒ `expires_at = now_ms + delta`;
    /// `None` ⇒ NULL on first INSERT, leave unchanged on idempotent.
    pub ttl_ms: Option<i64>,
    /// Source PAT id (audit cross-check).
    pub created_by_pat_id: Option<String>,
    /// Request id correlation.
    pub created_by_request_id: Option<String>,
}

/// Outcome of [`AcSchema::upsert`].
///
/// Mirrors the production `INSERT … ON CONFLICT (tenant_id,
/// action_digest) DO UPDATE` semantic + handler enforcement of
/// `INV-AC-RESULT-HASH-IMMUTABLE`:
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AcUpsertOutcome {
    /// First-time INSERT (no prior `(tenant_id, action_digest)` row).
    Inserted,
    /// Row pre-existed with the **same** `result_hash`; refreshed
    /// `last_hit_at` (+ optional `expires_at`). Every immutable column
    /// (`result_hash`, `created_at`, `tenant_prefix`, `region`,
    /// `path_key_id`, `sig_key_id`, `sig_alg`, `blob_refs*`,
    /// `result_size_bytes`, `created_by_*`) is preserved.
    IdempotentRefresh,
    /// Row pre-existed with a **different** `result_hash`. The upsert
    /// is rejected; existing row is preserved per
    /// `INV-AC-RESULT-HASH-IMMUTABLE`. Handler maps to 409
    /// `COR_AC_RESULT_HASH_MISMATCH`.
    ResultHashMismatch,
}

/// In-memory simulator for `ac_meta`. Key is `(tenant_id,
/// action_digest)`; only entry to a row is via this PK shape, so a
/// Tenant B query for a Tenant A row returns `Ok(None)`.
#[derive(Debug, Default)]
pub struct AcSchema {
    rows: BTreeMap<(Uuid, String), AcMetaRow>,
}

impl AcSchema {
    /// Construct an empty schema instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply the canonical PK + CHECK constraint validation against
    /// `request`; returns `Ok(())` if every constraint is satisfied.
    fn validate_check_constraints(req: &AcUpsertRequest) -> Result<(), SimError> {
        if req.action_digest.len() != DIGEST_HEX_LEN {
            return Err(SimError::CheckViolation("chk_ac_action_digest_len"));
        }
        if !is_lower_hex(&req.action_digest) {
            return Err(SimError::CheckViolation(
                "chk_ac_action_digest_len: non-hex characters",
            ));
        }
        if req.result_hash.len() != DIGEST_HEX_LEN {
            return Err(SimError::CheckViolation("chk_ac_result_hash_len"));
        }
        if !is_lower_hex(&req.result_hash) {
            return Err(SimError::CheckViolation(
                "chk_ac_result_hash_len: non-hex characters",
            ));
        }
        if req.blob_refs.len() > BLOB_REFS_SIZE_MAX {
            return Err(SimError::CheckViolation("chk_ac_blob_refs_size"));
        }
        if !(0..=BLOB_REFS_COUNT_MAX).contains(&req.blob_refs_count) {
            return Err(SimError::CheckViolation("chk_ac_blob_refs_count"));
        }
        if !(0..=RESULT_SIZE_BYTES_MAX).contains(&req.result_size_bytes) {
            return Err(SimError::CheckViolation("chk_ac_result_size"));
        }
        // Region is `AcRegion` enum so `chk_ac_region` is type-system enforced.
        // Tenant prefix is `[u8; 16]` so `chk_ac_tenant_prefix_len` is type-system enforced.
        if req.path_key_id < 1 {
            return Err(SimError::CheckViolation("chk_ac_path_key_id_positive"));
        }
        if req.sig_key_id < 1 {
            return Err(SimError::CheckViolation("chk_ac_sig_key_id_positive"));
        }
        // sig_alg is `SigAlg` enum so `chk_ac_sig_alg` is type-system enforced.

        let expires_after_created = req
            .ttl_ms
            .map(|delta| delta >= 0)
            .unwrap_or(true);
        if !expires_after_created {
            return Err(SimError::CheckViolation("chk_ac_lifecycle"));
        }
        Ok(())
    }

    /// Upsert: INSERT new row OR refresh `last_hit_at` (+ optional
    /// `expires_at`) on idempotent re-update. On `result_hash`
    /// mismatch the existing row is preserved per
    /// `INV-AC-RESULT-HASH-IMMUTABLE`.
    pub fn upsert(&mut self, req: AcUpsertRequest) -> Result<AcUpsertOutcome, SimError> {
        Self::validate_check_constraints(&req)?;

        let key = (req.tenant_id, req.action_digest.clone());
        match self.rows.get_mut(&key) {
            None => {
                // Lifecycle invariant on first INSERT: last_hit_at = created_at.
                let row = AcMetaRow {
                    tenant_id: req.tenant_id,
                    action_digest: req.action_digest.clone(),
                    tenant_prefix: req.tenant_prefix,
                    path_key_id: req.path_key_id,
                    result_hash: req.result_hash,
                    blob_refs: req.blob_refs,
                    blob_refs_count: req.blob_refs_count,
                    result_size_bytes: req.result_size_bytes,
                    created_at: req.now_ms,
                    last_hit_at: req.now_ms,
                    expires_at: req.ttl_ms.map(|delta| req.now_ms + delta),
                    sig_key_id: req.sig_key_id,
                    sig_alg: req.sig_alg,
                    region: req.region,
                    created_by_pat_id: req.created_by_pat_id,
                    created_by_request_id: req.created_by_request_id,
                };
                // chk_ac_lifecycle (post-write self-consistency).
                if row.last_hit_at < row.created_at {
                    return Err(SimError::CheckViolation("chk_ac_lifecycle"));
                }
                if let Some(exp) = row.expires_at {
                    if exp < row.created_at {
                        return Err(SimError::CheckViolation("chk_ac_lifecycle"));
                    }
                }
                self.rows.insert(key, row);
                Ok(AcUpsertOutcome::Inserted)
            }
            Some(existing) => {
                if existing.result_hash != req.result_hash {
                    return Ok(AcUpsertOutcome::ResultHashMismatch);
                }
                // Idempotent path — refresh last_hit_at + optional
                // expires_at. INV-AC-TTL-REFRESH-MONOTONIC: clamp to
                // max(prev, now) to absorb minor NTP skew.
                let new_last_hit = existing.last_hit_at.max(req.now_ms);
                if new_last_hit < existing.created_at {
                    return Err(SimError::CheckViolation("chk_ac_lifecycle"));
                }
                existing.last_hit_at = new_last_hit;
                if let Some(delta) = req.ttl_ms {
                    let new_exp = req.now_ms + delta;
                    if new_exp < existing.created_at {
                        return Err(SimError::CheckViolation("chk_ac_lifecycle"));
                    }
                    existing.expires_at = Some(new_exp);
                }
                // Every other immutable column is preserved (per
                // INV-AC-RESULT-HASH-IMMUTABLE + handler discipline).
                Ok(AcUpsertOutcome::IdempotentRefresh)
            }
        }
    }

    /// Read a row by canonical PK shape.
    #[must_use]
    pub fn get(&self, tenant_id: &Uuid, action_digest: &str) -> Option<&AcMetaRow> {
        self.rows.get(&(*tenant_id, action_digest.to_string()))
    }

    /// Refresh `last_hit_at` (+ optional `expires_at`) on a row.
    /// Returns `Ok(true)` when the row exists, `Ok(false)` when absent
    /// (no-op). Mirrors the `UPDATE … WHERE tenant_id = ? AND
    /// action_digest = ? RETURNING` pattern.
    pub fn refresh_on_hit(
        &mut self,
        tenant_id: &Uuid,
        action_digest: &str,
        now_ms: i64,
        ttl_extend_ms: Option<i64>,
    ) -> Result<bool, SimError> {
        let key = (*tenant_id, action_digest.to_string());
        let Some(row) = self.rows.get_mut(&key) else {
            return Ok(false);
        };
        let new_last_hit = row.last_hit_at.max(now_ms);
        if new_last_hit < row.created_at {
            return Err(SimError::CheckViolation("chk_ac_lifecycle"));
        }
        row.last_hit_at = new_last_hit;
        if let Some(delta) = ttl_extend_ms {
            let new_exp = now_ms + delta;
            if new_exp < row.created_at {
                return Err(SimError::CheckViolation("chk_ac_lifecycle"));
            }
            row.expires_at = Some(new_exp);
        }
        Ok(true)
    }

    /// Tenant-scoped enumeration. Returns rows whose `tenant_id`
    /// matches `ctx_tenant`; ignores expiry (the GET handler in
    /// WI-S04-001 already filters via `expires_at <= now_ms` post-fetch
    /// to distinguish 404 absent vs 410 expired).
    #[must_use]
    pub fn list_for_tenant(&self, ctx_tenant: &Uuid) -> Vec<&AcMetaRow> {
        self.rows
            .iter()
            .filter_map(|((t, _), row)| (t == ctx_tenant).then_some(row))
            .collect()
    }

    /// Total row count (for cardinality assertions in tests).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// True if no rows have been inserted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Re-apply the migration. Idempotent (no-op).
    ///
    /// In production, `wrangler d1 migrations apply` re-running the
    /// SQL artifact is a no-op via `CREATE TABLE IF NOT EXISTS` /
    /// `CREATE INDEX IF NOT EXISTS`. The simulator pins the same
    /// contract at the host-side level so property tests can assert
    /// "applying twice does not lose data".
    pub fn reapply_migration(&mut self) -> Result<(), SimError> {
        // Schema is the row layout; no DDL changes since the simulator
        // never had columns to add. Re-apply is a no-op; rows remain.
        Ok(())
    }
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
    use super::*;

    fn fixed_tenant_a() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    fn fixed_tenant_b() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap()
    }

    fn hex64(seed: u8) -> String {
        let bytes = [seed; 32];
        hex::encode(bytes)
    }

    fn req(tid: Uuid, action_seed: u8, result_seed: u8, now_ms: i64) -> AcUpsertRequest {
        AcUpsertRequest {
            tenant_id: tid,
            action_digest: hex64(action_seed),
            tenant_prefix: [0xab; 16],
            path_key_id: 1,
            result_hash: hex64(result_seed),
            blob_refs: r#"[]"#.to_string(),
            blob_refs_count: 0,
            result_size_bytes: 64,
            sig_key_id: 1,
            sig_alg: SigAlg::HkdfSha256,
            region: AcRegion::Sam,
            now_ms,
            ttl_ms: Some(60_000),
            created_by_pat_id: None,
            created_by_request_id: None,
        }
    }

    #[test]
    fn first_upsert_inserts() {
        let mut s = AcSchema::new();
        let r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        assert_eq!(s.upsert(r).unwrap(), AcUpsertOutcome::Inserted);
        assert_eq!(s.row_count(), 1);
    }

    #[test]
    fn idempotent_refresh_preserves_immutable_columns() {
        let mut s = AcSchema::new();
        let r1 = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        s.upsert(r1.clone()).unwrap();
        let mut r2 = r1.clone();
        r2.now_ms = 5_000;
        r2.created_by_pat_id = Some("pat-other".into()); // ignored on idempotent
        assert_eq!(
            s.upsert(r2).unwrap(),
            AcUpsertOutcome::IdempotentRefresh
        );
        let row = s.get(&fixed_tenant_a(), &r1.action_digest).unwrap();
        assert_eq!(row.last_hit_at, 5_000);
        assert_eq!(row.created_at, 1_000);
        assert_eq!(row.created_by_pat_id, None); // immutable preserved
    }

    #[test]
    fn result_hash_mismatch_rejects_upsert() {
        let mut s = AcSchema::new();
        let r1 = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        s.upsert(r1.clone()).unwrap();
        let r2 = req(fixed_tenant_a(), 1, 0xbb, 2_000);
        assert_eq!(
            s.upsert(r2).unwrap(),
            AcUpsertOutcome::ResultHashMismatch
        );
        let row = s.get(&fixed_tenant_a(), &r1.action_digest).unwrap();
        assert_eq!(row.result_hash, r1.result_hash);
        assert_eq!(row.last_hit_at, 1_000); // not refreshed
    }

    #[test]
    fn cross_tenant_get_returns_none() {
        let mut s = AcSchema::new();
        let r_a = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        s.upsert(r_a.clone()).unwrap();
        let row = s.get(&fixed_tenant_b(), &r_a.action_digest);
        assert!(row.is_none());
    }

    #[test]
    fn cross_tenant_list_returns_empty() {
        let mut s = AcSchema::new();
        s.upsert(req(fixed_tenant_a(), 1, 0xaa, 1_000)).unwrap();
        s.upsert(req(fixed_tenant_a(), 2, 0xbb, 1_001)).unwrap();
        assert_eq!(s.list_for_tenant(&fixed_tenant_a()).len(), 2);
        assert_eq!(s.list_for_tenant(&fixed_tenant_b()).len(), 0);
    }

    #[test]
    fn check_action_digest_len_enforced() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.action_digest = "abcd".into(); // too short
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_action_digest_len")
        );
    }

    #[test]
    fn check_blob_refs_size_enforced() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.blob_refs = "x".repeat(BLOB_REFS_SIZE_MAX + 1);
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_blob_refs_size")
        );
    }

    #[test]
    fn check_blob_refs_count_enforced() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.blob_refs_count = BLOB_REFS_COUNT_MAX + 1;
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_blob_refs_count")
        );
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.blob_refs_count = -1;
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_blob_refs_count")
        );
    }

    #[test]
    fn check_result_size_bytes_enforced() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.result_size_bytes = RESULT_SIZE_BYTES_MAX + 1;
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_result_size")
        );
    }

    #[test]
    fn check_path_key_id_positive_enforced() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.path_key_id = 0;
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_path_key_id_positive")
        );
    }

    #[test]
    fn check_sig_key_id_positive_enforced() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.sig_key_id = 0;
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_sig_key_id_positive")
        );
    }

    #[test]
    fn refresh_on_hit_no_op_when_absent() {
        let mut s = AcSchema::new();
        let updated = s
            .refresh_on_hit(&fixed_tenant_a(), &hex64(99), 9_999, Some(60_000))
            .unwrap();
        assert!(!updated);
    }

    #[test]
    fn refresh_on_hit_clamps_monotonic() {
        let mut s = AcSchema::new();
        let r = req(fixed_tenant_a(), 1, 0xaa, 5_000);
        s.upsert(r.clone()).unwrap();
        let updated = s
            .refresh_on_hit(&fixed_tenant_a(), &r.action_digest, 1_000, None)
            .unwrap();
        assert!(updated);
        let row = s.get(&fixed_tenant_a(), &r.action_digest).unwrap();
        assert_eq!(row.last_hit_at, 5_000); // not rolled back
    }

    #[test]
    fn reapply_migration_is_idempotent() {
        let mut s = AcSchema::new();
        s.upsert(req(fixed_tenant_a(), 1, 0xaa, 1_000)).unwrap();
        s.reapply_migration().unwrap();
        s.reapply_migration().unwrap();
        assert_eq!(s.row_count(), 1);
    }

    #[test]
    fn check_lifecycle_negative_ttl_rejected() {
        let mut s = AcSchema::new();
        let mut r = req(fixed_tenant_a(), 1, 0xaa, 1_000);
        r.ttl_ms = Some(-2_000);
        assert_eq!(
            s.upsert(r).unwrap_err(),
            SimError::CheckViolation("chk_ac_lifecycle")
        );
    }
}
