//! Real D1 effective erase adapter (`BackendKind::D1`), WI-S11-008 Wave 1
//! increment 2. Hard-deletes a tenant's subject PII from the D1
//! `corelink-config-prod` database.
//!
//! The erase-set + the erase-vs-retain classification are OWNER-RATIFIED
//! in ADR-S11-013 (2026-06-11). Every table/column below was cold-verified
//! against its `migrations/d1/*.sql` `CREATE TABLE`. Load-bearing
//! subtleties, all honored here:
//!
//! - **Ordering.** Child rows first, then the root `tenant` row LAST (so a
//!   FK-enforced delete never blocks); `signup_attempts` is deleted BEFORE
//!   `signup_orchestration` because it joins through
//!   `signup_orchestration.idempotency_key`.
//! - **No cross-tenant break.** `adapter_cache_map` / `adapter_npm_meta` /
//!   `adapter_pip_index` are keyed by `namespace`, which is either a tenant
//!   UUID or the synthetic `'_public'` (shared public-registry content,
//!   `INV-TENANT-ISOLATION`). We delete `WHERE namespace = <tenant_uuid>`;
//!   a tenant UUID can never equal `'_public'`, so shared content is safe.
//! - **No phantom tables.** `tenant_primary_region` (0028) and
//!   `byok_tenant_status` (0031) are ALTER COLUMNS on `tenant`, not tables;
//!   deleting the `tenant` row covers them.
//! - **SQL safety.** Table + column names are compile-time constants
//!   (never request input); only the tenant id is a bound `?1` parameter.

use std::sync::Arc;

use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use corelink_privacy_erasure_worker::error::ErasureBackendError;
use corelink_privacy_erasure_worker::event::{BackendErasureOutcome, BackendKind};

use super::d1util::{d1_query_blocking, scalar_count};
use crate::storage::d1_http::D1HttpClient;

/// Erase-set tables keyed directly by a `tenant_id` column (incl.
/// tenant-leftmost composite PKs, where `WHERE tenant_id = ?` is exact).
/// `tenant` itself is handled separately (deleted LAST).
const TENANT_ID_TABLES: &[&str] = &[
    "tier_selections",
    "tenant_billing",
    "pilot_signups",
    "pat",
    "usage_counter",
    "tenant_storage_state",
    "tenant_offboarding_state",
    "usage_event_staging",
    "dpa_acceptance_pending",
    "tenant_config",
    "hot_blobs",
    "quota_reservations",
    "quota_cas_attempts",
    "quota_fsm_state",
    "ratelimit_buckets",
    "byok_envelope",
    "adapter_oci_kv",
];

/// Erase-set tables keyed by a `namespace` column. The bound value is the
/// tenant UUID, which can never equal the shared `'_public'` namespace —
/// so shared public-registry content is never touched.
const NAMESPACE_TABLES: &[&str] = &["adapter_cache_map", "adapter_npm_meta", "adapter_pip_index"];

/// Tables that MUST NEVER appear in the erase-set (retain-set per
/// ADR-S11-013: the erasure record itself, fiscal 5y, audit WORM 7y).
/// Used by the guard test to catch an accidental erase-set addition.
#[cfg(test)]
const RETAIN_SET: &[&str] = &[
    "dsr_erasure_log",
    "dsr_requested",
    "dpa_acceptances",
    "erasure_attestation",
    "erasure_attestations",
    "export_audit_log",
    "stripe_customers",
    "stripe_subscriptions",
    "stripe_invoices",
    "stripe_disputes",
    "stripe_refunds",
    "audit_outbox",
];

/// Real D1 effective erase adapter.
pub(super) struct D1EraseAdapter {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1EraseAdapter {
    // Never surface the inner client's Debug — it holds the CF API token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1EraseAdapter")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl D1EraseAdapter {
    /// Construct over a shared [`D1HttpClient`].
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// `SELECT COUNT(*)` for `<table> WHERE <col> = ?1`. `table`/`col` are
    /// compile-time constants (injection-safe); `val` is the bound param.
    fn count(&self, table: &str, col: &str, val: &str) -> Result<u64, ErasureBackendError> {
        let sql = format!("SELECT COUNT(*) AS n FROM {table} WHERE {col} = ?1");
        let rows = d1_query_blocking(&self.d1, &sql, vec![json!(val)])
            .map_err(ErasureBackendError::Transport)?;
        Ok(scalar_count(&rows, "n"))
    }

    /// Count then (only if non-zero) hard-delete `<table> WHERE <col> = ?1`.
    /// Returns the number of rows that were present (= deleted).
    fn count_then_delete(
        &self,
        table: &str,
        col: &str,
        val: &str,
    ) -> Result<u64, ErasureBackendError> {
        let n = self.count(table, col, val)?;
        if n > 0 {
            let sql = format!("DELETE FROM {table} WHERE {col} = ?1");
            d1_query_blocking(&self.d1, &sql, vec![json!(val)])
                .map_err(ErasureBackendError::Transport)?;
        }
        Ok(n)
    }

    /// `signup_attempts` has no `tenant_id`; its rows join to the tenant via
    /// `idempotency_key` → `signup_orchestration`. MUST run before the
    /// `signup_orchestration` delete (else the subquery finds nothing).
    fn count_signup_attempts(&self, tid: &str) -> Result<u64, ErasureBackendError> {
        let sql = "SELECT COUNT(*) AS n FROM signup_attempts WHERE idempotency_key IN \
             (SELECT idempotency_key FROM signup_orchestration WHERE tenant_id = ?1)";
        let rows = d1_query_blocking(&self.d1, sql, vec![json!(tid)])
            .map_err(ErasureBackendError::Transport)?;
        Ok(scalar_count(&rows, "n"))
    }

    fn delete_signup_attempts(&self, tid: &str) -> Result<u64, ErasureBackendError> {
        let n = self.count_signup_attempts(tid)?;
        if n > 0 {
            let sql = "DELETE FROM signup_attempts WHERE idempotency_key IN \
                 (SELECT idempotency_key FROM signup_orchestration WHERE tenant_id = ?1)";
            d1_query_blocking(&self.d1, sql, vec![json!(tid)])
                .map_err(ErasureBackendError::Transport)?;
        }
        Ok(n)
    }

    /// Count all remaining erase-set rows for a tenant (verification sweep).
    fn remaining_rows(&self, tid: &str) -> Result<u64, ErasureBackendError> {
        let mut remaining = 0u64;
        for t in TENANT_ID_TABLES {
            remaining = remaining.saturating_add(self.count(t, "tenant_id", tid)?);
        }
        for t in NAMESPACE_TABLES {
            remaining = remaining.saturating_add(self.count(t, "namespace", tid)?);
        }
        remaining = remaining.saturating_add(self.count_signup_attempts(tid)?);
        remaining = remaining.saturating_add(self.count("signup_orchestration", "tenant_id", tid)?);
        remaining = remaining.saturating_add(self.count("tenant", "tenant_id", tid)?);
        Ok(remaining)
    }
}

impl BackendErasureAdapter for D1EraseAdapter {
    fn kind(&self) -> BackendKind {
        BackendKind::D1
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
        let mut total = 0u64;

        // Group A — tenant_id-keyed child tables.
        for t in TENANT_ID_TABLES {
            total = total.saturating_add(self.count_then_delete(t, "tenant_id", &tid)?);
        }
        // Group B — namespace-keyed (tenant UUID; never '_public').
        for t in NAMESPACE_TABLES {
            total = total.saturating_add(self.count_then_delete(t, "namespace", &tid)?);
        }
        // Group C — signup_attempts BEFORE signup_orchestration.
        total = total.saturating_add(self.delete_signup_attempts(&tid)?);
        total = total.saturating_add(self.count_then_delete("signup_orchestration", "tenant_id", &tid)?);
        // Group D — the root identity row LAST (covers the ALTER columns
        // primary_region / byok_status / clerk_user_id / email_hash /
        // stripe_customer_id on `tenant`).
        total = total.saturating_add(self.count_then_delete("tenant", "tenant_id", &tid)?);

        Ok(BackendErasureOutcome::Erased {
            records_deleted: total,
        })
    }

    fn verification_hash(
        &self,
        ctx: VerificationContext,
    ) -> Result<[u8; 32], ErasureBackendError> {
        let remaining = self.remaining_rows(&ctx.tenant_id.to_string())?;
        if remaining == 0 {
            // Canonical "no rows for tenant" sentinel (effective backend).
            Ok(CANONICAL_EMPTY_TENANT_HASH)
        } else {
            // Non-empty → a deterministic non-canonical fingerprint (Sha256,
            // a container dep; differs from the blake3-empty canonical hash),
            // so the verify sweep maps the mismatch to VerifiedPartial + SEV-1.
            let mut h = Sha256::new();
            h.update(b"corelink/v1/d1-erasure-remaining:");
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
    fn erase_set_has_no_overlap_and_no_dupes() {
        let mut all: Vec<&str> = TENANT_ID_TABLES
            .iter()
            .chain(NAMESPACE_TABLES.iter())
            .copied()
            .collect();
        all.push("signup_attempts");
        all.push("signup_orchestration");
        all.push("tenant");
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "erase-set has a duplicate table");
    }

    #[test]
    fn erase_set_never_touches_a_retain_table() {
        for t in TENANT_ID_TABLES.iter().chain(NAMESPACE_TABLES.iter()) {
            assert!(
                !RETAIN_SET.contains(t),
                "RETAIN-set table {t} must NEVER be in the D1 erase-set (ADR-S11-013)"
            );
        }
    }

    #[test]
    fn kind_is_d1() {
        // Construction needs a client; assert the const instead (kind() is
        // a pure const map). The orchestrator pins the canonical position.
        assert_eq!(BackendKind::D1.as_str(), "d1");
    }
}
