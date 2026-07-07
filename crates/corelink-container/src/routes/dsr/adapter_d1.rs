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
//! - **D1 does NOT enforce FOREIGN KEYs (CAA-360 #21).** D1/SQLite ships with
//!   `PRAGMA foreign_keys = OFF` and D1 does not expose a reliable per-connection
//!   way to turn it on, so tenant-keyed tables (`tenant_quota`, `cas_tombstone`,
//!   …) intentionally omit FK declarations — they would be inert. Referential
//!   integrity is therefore an APPLICATION invariant, maintained two ways:
//!   (1) on erase, child rows are deleted BEFORE the parent `tenant` row (the
//!   ordering above), so no orphan is ever left pointing at a deleted tenant;
//!   (2) on insert, the writing path only creates a `tenant_quota` /
//!   `cas_tombstone` row for a tenant that already exists (provisioned first).
//!   Adding `FOREIGN KEY` clauses to the migrations would NOT change runtime
//!   behavior on D1 and is deliberately not done (see migrations 0066/0067).

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
///
/// `remaining_rows()` scans this whole slice, so any table added here is
/// automatically covered by the post-erase verification sweep (no separate
/// edit needed) — the property the **CF-1 fix** relies on.
///
/// `pub(super)` so the DSR ACCESS / PORTABILITY gather pipeline
/// ([`super::access`]) single-sources this exact erase-set — a future table
/// added here is then covered by BOTH erase AND access (no parallel list to
/// drift).
pub(super) const TENANT_ID_TABLES: &[&str] = &[
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
    "tenant_byok_config",
    "tenant_byok_secret",
    "adapter_oci_kv",
    // CAA-360 #4: tenant-linked NPS/CSAT PII (recipient_hash); `tenant_id`-keyed
    // (migration 0046). Not a legal-retention category, so it IS erased on a DSR
    // (GDPR Art.17) — distinct from the retained fiscal `stripe_*` records.
    "survey_responses",
    // CAA-360 #11: per-tenant spend ledger (`tenant_id` PK, migration 0066).
    // Operational quota state, not a fiscal invoice record → erased on a DSR.
    "tenant_quota",
    // ── CF-1 (2026-06-28): tenant-keyed tables added AFTER the 2026-06-11
    // ADR-S11-013 ratification that were never back-added to the erase-set.
    // Each is operational tenant state / tenant PII with no legal-retention
    // basis → ERASE per ADR-S11-013's classification policy. ───────────────
    // Seat PII: `tenant_id` + raw Clerk `user_id` + `email_hash` (migr. 0074).
    // The worst gap — a team-tenant's member roster survived "VerifiedComplete".
    "team_member",
    // CAS deletion tombstones `(tenant_id, digest)` (migr. 0067). Operational
    // 410-Gone state; meaningless once the tenant is gone → ERASE.
    "cas_tombstone",
    // Pilot enrolment rows keyed by `tenant_id` (migr. 0065). Subject PII.
    "pilot_tenants",
    // Runners add-on entitlement, `tenant_id` PK (migr. 0070/0072). Operational.
    "runners_entitlement",
    // Per-tenant monthly request counters (migr. 0071). Operational usage state.
    "monthly_request_counts",
    // Advisory tier-selection lock, `tenant_id` PK (migr. 0039). Operational.
    "tier_selection_locks",
    // Stripe Checkout *session* state, `tenant_id`-keyed (migr. 0039/0062).
    // Transient pre-purchase intent — NOT the fiscal record (the retained
    // invoice/customer/subscription rows are the 5y fiscal artifact). ERASE.
    // [owner edge: defensible as billing-adjacent; see FLAGGED note below.]
    "stripe_checkout_sessions",
    // Runner subscription↔tenant billing mirror, `tenant_id`-indexed (migr. 0087).
    // The runner analog of `tenant_billing`: an OPERATIONAL subscription-state
    // mirror (plan/status), NOT the fiscal record (the retained Stripe
    // customer/invoice/subscription rows are the 5y fiscal artifact). Same class
    // as `tenant_billing` / `stripe_checkout_sessions` above → ERASE per
    // ADR-S11-013 (`DELETE ... WHERE tenant_id = ?`; the tenant_id index covers it).
    "runner_billing",
    // GC run + candidate state, `tenant_id`-keyed (migr. 0006/0007). Operational
    // storage-GC bookkeeping over the tenant's own blobs → ERASE.
    "gc_run",
    "gc_candidates",
    // Region-migration request + progress, `tenant_id`-keyed (migr. 0023/0027).
    // Operational residency-move state, no retention basis → ERASE.
    "region_migration_request",
    "region_migration_progress",
    // Clerk org → isolated tenant identity map (migr. 0083). PRIMARY identity is
    // `clerk_org_id`, but it CARRIES `tenant_id` — classify by tenant_id: the
    // org→tenant mapping is operational identity state with no retention basis
    // and is removed when that tenant is erased (GDPR Art.17). ERASE.
    "tenant_org_map",
    // GitHub App installation → isolated tenant identity map (migr. 0084).
    // PRIMARY identity is `installation_id`, but it CARRIES `tenant_id` —
    // classify by tenant_id: the installation→tenant mapping is operational
    // identity state with no retention basis and is removed when that tenant is
    // erased (GDPR Art.17). ERASE.
    "tenant_gh_installation_map",
    // Per-tenant runner repo allowlist, tenant-leftmost composite PK
    // `(tenant_id, repo_full_name)` (migr. 0085). Operational entitlement state
    // with no retention basis → ERASE.
    "runner_repo_allowlist",
    // Per-tenant, per-day usage rollup for the dashboard ROI surface
    // `(tenant_id, day)` (migr. 0089). Display telemetry — the tenant's own
    // operational usage state, no retention basis → ERASE.
    "usage_daily",
];

/// Erase-set tables keyed by a `namespace` column. The bound value is the
/// tenant UUID, which can never equal the shared `'_public'` namespace —
/// so shared public-registry content is never touched.
///
/// `pub(super)` — single-sourced by the DSR ACCESS gather ([`super::access`]).
pub(super) const NAMESPACE_TABLES: &[&str] =
    &["adapter_cache_map", "adapter_npm_meta", "adapter_pip_index"];

/// Tables that MUST NEVER appear in the erase-set (retain-set per
/// ADR-S11-013: a lawful retention basis — the erasure record itself,
/// fiscal 5y, audit/legal 7y WORM). Used by the guard test to catch an
/// accidental erase-set addition AND by the completeness gate to confirm
/// every tenant-keyed table is consciously classified.
///
/// **CF-1 (2026-06-28): PROMOTED out of `#[cfg(test)]`** so it exists at
/// runtime — the completeness gate / runtime drift assertion can consult it,
/// not just the test build.
pub(super) const RETAIN_SET: &[&str] = &[
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
    // ── CF-1: tenant-keyed tables with a lawful retention basis (ADR-S11-013
    // RETAIN policy: fiscal / billing-reconciliation / audit-evidence). ─────
    "billing_replay_audit",          // billing replay audit trail (migr. 0021)
    "stripe_idempotency_keys",       // fiscal idempotency ledger (migr. 0018)
    "billing_reconciliation_drift",  // billing reconciliation evidence (migr. 0019)
    "stripe_submission_state",       // billing submission state (migr. 0019)
    "customer_audit_events",         // per-tenant audit evidence (migr. 0077)
    "audit_chain_head",              // audit-chain seal head — integrity (migr. 0078)
    // Legal-hold control record (migr. 0076): the durable signal that gates
    // erasure itself. A row = "destructive erasure refused"; it is a
    // legal-process / audit anchor (`placed_at`), `reason` is operator-internal
    // and explicitly NOT DSR-disclosable per the migration → RETAIN.
    "tenant_legal_hold",
    // abuse_score_history (migr. 0013): OWNER-DECIDED RETAIN — fraud/abuse
    // prevention is a legitimate interest under GDPR Art.17(3)(... ) / Art.6(1)(f);
    // retaining behavioural abuse scores survives an Art.17 erasure as an
    // anti-abuse safeguard. This is an owner/legal call (ADR-S11-013 FLAGGED
    // "abuse_scores"); revisit if the legal basis changes.
    "abuse_score_history",
    // session_exchange_throttle (migr. 0068): keyed by `clerk_sub` — an opaque
    // SHA-256-derived principal id (NOT raw Clerk id, NOT email — already
    // pseudonymous) — with NO `tenant_id` column, so the tenant-scoped D1
    // adapter cannot target it by `WHERE tenant_id = ?`. It is also an
    // ephemeral fixed-window throttle counter (self-expiring). RETAIN-by-
    // construction. ⚠ FLAGGED for owner: if per-principal erasure is later
    // required, it needs a principal-keyed delete path (out of this adapter's
    // tenant-scoped contract).
    "session_exchange_throttle",
];

/// Tenant-keyed tables whose erasure is owned by ANOTHER canonical backend
/// (ADR-S11-013 backend mapping), so they are intentionally NOT in this D1
/// adapter's erase-set — but MUST still be consciously accounted for by the
/// completeness gate (they are not "unclassified").
///
/// - `blob_meta` → `R2Cas` (refcount-aware: decrement, delete object only at 0).
/// - `ac_meta`   → `R2Ac` (D1-driven per-region action-cache delete).
/// - `chunks` / `manifest_chunks` / `multipart_sessions` → the multipart/chunk
///   CAS path is the in-memory sim with **zero prod write sites** (ADR-S11-013
///   §"R2-erasure design"), so no durable rows exist; ownership sits with the
///   CAS plane if/when it ships.
const CAS_PLANE_OWNED: &[&str] = &[
    "blob_meta",
    "ac_meta",
    "chunks",
    "manifest_chunks",
    "multipart_sessions",
];

/// Erase-set tables handled by bespoke logic (not the simple
/// `WHERE <col> = ?1` loop): `signup_attempts` joins through
/// `signup_orchestration.idempotency_key`; `tenant` is deleted LAST.
pub(super) const SPECIAL_ERASE_TABLES: &[&str] =
    &["signup_orchestration", "signup_attempts", "tenant"];

/// **Every** live, tenant-scoped D1 table (keyed by `tenant_id`, `namespace`,
/// or an opaque principal id), derived from `migrations/d1/*.sql`. Transient
/// table-rebuild artifacts (`*_new`, immediately `DROP`+`RENAME`'d away by
/// migr. 0062/0064) are excluded.
///
/// **CF-1 invariant:** every entry here MUST be classified into EXACTLY ONE of
/// {erase-set, namespace-set, retain-set, CAS-plane-owned, special} — enforced
/// fail-closed by [`unclassified_tenant_keyed_tables`] (runtime assert + test)
/// AND cross-checked against the migrations on disk by the
/// `every_migrated_tenant_keyed_table_is_classified` test. A FUTURE tenant-keyed
/// migration therefore cannot silently escape erasure classification.
const ALL_TENANT_KEYED_TABLES: &[&str] = &[
    // erase-set (tenant_id)
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
    "tenant_byok_config",
    "tenant_byok_secret",
    "adapter_oci_kv",
    "survey_responses",
    "tenant_quota",
    "team_member",
    "cas_tombstone",
    "pilot_tenants",
    "runners_entitlement",
    "monthly_request_counts",
    "tier_selection_locks",
    "stripe_checkout_sessions",
    "runner_billing",
    "gc_run",
    "gc_candidates",
    "region_migration_request",
    "region_migration_progress",
    "tenant_org_map",
    "tenant_gh_installation_map",
    "runner_repo_allowlist",
    "usage_daily",
    // erase-set (namespace)
    "adapter_cache_map",
    "adapter_npm_meta",
    "adapter_pip_index",
    // retain-set
    "dsr_erasure_log",
    "dsr_requested",
    "dpa_acceptances",
    "erasure_attestations",
    "audit_outbox",
    "stripe_customers",
    "stripe_subscriptions",
    "stripe_invoices",
    "stripe_disputes",
    "stripe_refunds",
    "billing_replay_audit",
    "stripe_idempotency_keys",
    "billing_reconciliation_drift",
    "stripe_submission_state",
    "customer_audit_events",
    "audit_chain_head",
    "tenant_legal_hold",
    "abuse_score_history",
    "session_exchange_throttle",
    // CAS-plane-owned
    "blob_meta",
    "ac_meta",
    "chunks",
    "manifest_chunks",
    "multipart_sessions",
    // special
    "signup_orchestration",
    "signup_attempts",
    "tenant",
];

/// The five mutually-exclusive classification buckets every tenant-keyed table
/// must land in exactly once.
const CLASSIFICATION_SETS: &[(&str, &[&str])] = &[
    ("erase:tenant_id", TENANT_ID_TABLES),
    ("erase:namespace", NAMESPACE_TABLES),
    ("retain", RETAIN_SET),
    ("cas-plane-owned", CAS_PLANE_OWNED),
    ("special", SPECIAL_ERASE_TABLES),
];

/// Number of classification buckets a table appears in (must be exactly 1).
fn classification_count(table: &str) -> usize {
    CLASSIFICATION_SETS
        .iter()
        .filter(|(_, set)| set.contains(&table))
        .count()
}

/// Fail-closed completeness gate (the load-bearing CF-1 fix): every
/// tenant-keyed table must be classified into EXACTLY ONE bucket. Returns
/// `(table, count)` for every table that is unclassified (`0`) or ambiguously
/// multi-classified (`>1`). Empty ⇒ the classification is total and disjoint.
fn unclassified_tenant_keyed_tables() -> Vec<(&'static str, usize)> {
    ALL_TENANT_KEYED_TABLES
        .iter()
        .map(|t| (*t, classification_count(t)))
        // `t: &&str`; `classification_count` takes `&str` via deref of `*t`.
        .filter(|(_, n)| *n != 1)
        .collect()
}

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
        // CF-1 runtime drift assertion: never run an erasure (and then
        // attest VerifiedComplete) while a known tenant-keyed table is
        // unclassified. Cheap const-slice scan; fires in every debug/test
        // build so an unaccounted-for new table is caught before it can leave
        // PII behind under a green attestation. (The hard, always-on gate is
        // the `#[test]`s below, which also cross-check the migrations on disk.)
        debug_assert!(
            unclassified_tenant_keyed_tables().is_empty(),
            "CF-1: DSR erase-set classification is incomplete/ambiguous — \
             refusing to over-attest: {:?}",
            unclassified_tenant_keyed_tables()
        );
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
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests"
)]
mod tests {
    use super::*;

    #[test]
    fn erase_set_has_no_overlap_and_no_dupes() {
        // Duplicate guard (kept from the original) across the full erase-set:
        // tenant_id + namespace + the bespoke specials.
        let all: Vec<&str> = TENANT_ID_TABLES
            .iter()
            .chain(NAMESPACE_TABLES.iter())
            .chain(SPECIAL_ERASE_TABLES.iter())
            .copied()
            .collect();
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "erase-set has a duplicate table");
    }

    #[test]
    fn erase_set_never_touches_a_retain_table() {
        for t in TENANT_ID_TABLES
            .iter()
            .chain(NAMESPACE_TABLES.iter())
            .chain(SPECIAL_ERASE_TABLES.iter())
        {
            assert!(
                !RETAIN_SET.contains(t),
                "RETAIN-set table {t} must NEVER be in the D1 erase-set (ADR-S11-013)"
            );
        }
    }

    #[test]
    fn tenant_linked_pii_tables_are_in_the_erase_set() {
        // Regression: these tenant_id-keyed tables carry tenant PII / seat PII /
        // spend state and MUST be erased on a DSR (GDPR Art.17). A removal would
        // silently leave tenant data behind after an erasure request.
        // `team_member` is the CF-1 worst-case (seat roster: raw Clerk user_id +
        // email_hash) that previously survived a "VerifiedComplete" attestation.
        for t in ["survey_responses", "tenant_quota", "team_member"] {
            assert!(
                TENANT_ID_TABLES.contains(&t),
                "{t} must be in the D1 erase-set (tenant PII)"
            );
            assert!(!RETAIN_SET.contains(&t), "{t} is not a retain-set table");
        }
    }

    /// CF-1 in-code completeness gate: every table in the hand-maintained
    /// registry is classified into EXACTLY ONE bucket (no unclassified, no
    /// ambiguous double-classification). This is the runtime-checkable half of
    /// the fix (mirrors the `debug_assert!` in `erase()`).
    #[test]
    fn every_registered_tenant_keyed_table_is_classified_exactly_once() {
        let gaps = unclassified_tenant_keyed_tables();
        assert!(
            gaps.is_empty(),
            "CF-1: these tenant-keyed tables are unclassified (0) or \
             ambiguously multi-classified (>1) — they would be silently \
             skipped on erase yet attested VerifiedComplete: {gaps:?}"
        );
        // Registry itself must be dupe-free.
        let mut sorted: Vec<&str> = ALL_TENANT_KEYED_TABLES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ALL_TENANT_KEYED_TABLES.len(),
            "ALL_TENANT_KEYED_TABLES has a duplicate"
        );
    }

    /// CF-1 LOAD-BEARING drift gate: parse `migrations/d1/*.sql` on disk and
    /// assert EVERY live tenant-scoped table (keyed by tenant_id / namespace /
    /// an opaque principal id / a subject hash) is present in the registry —
    /// and therefore classified erase-or-retain by the test above. This is what
    /// makes a FUTURE tenant-keyed migration impossible to land without a
    /// conscious erase-vs-retain decision: add the table to a migration and
    /// forget the adapter, and THIS test goes red.
    #[test]
    fn every_migrated_tenant_keyed_table_is_classified() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations/d1");
        let mut found: Vec<String> = Vec::new();
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("CF-1 drift gate cannot read {dir}: {e}"));
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("sql") {
                continue;
            }
            let sql = std::fs::read_to_string(&path).unwrap();
            found.extend(extract_tenant_keyed_tables(&sql));
        }
        found.sort();
        found.dedup();
        assert!(
            !found.is_empty(),
            "CF-1 drift gate parsed ZERO tables — parser or path is broken"
        );

        let missing: Vec<&String> = found
            .iter()
            .filter(|t| !ALL_TENANT_KEYED_TABLES.contains(&t.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "CF-1: tenant-keyed table(s) exist in migrations/d1 but are NOT in \
             the DSR classification registry (a future table escaped erasure \
             classification — add to ALL_TENANT_KEYED_TABLES + classify \
             erase-vs-retain per ADR-S11-013): {missing:?}"
        );
    }

    #[test]
    fn kind_is_d1() {
        // Construction needs a client; assert the const instead (kind() is
        // a pure const map). The orchestrator pins the canonical position.
        assert_eq!(BackendKind::D1.as_str(), "d1");
    }

    /// Strip SQL line (`--`) and block (`/* */`) comments so `CREATE TABLE`
    /// inside doc-comments is not mistaken for a real DDL statement.
    fn strip_sql_comments(sql: &str) -> String {
        // block comments first (migrations are ASCII; byte scan is safe)
        let mut no_block = String::with_capacity(sql.len());
        let bytes = sql.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                // skip to closing */
                let mut j = i + 2;
                while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                    j += 1;
                }
                i = (j + 2).min(bytes.len());
                no_block.push(' ');
                continue;
            }
            no_block.push(bytes[i] as char);
            i += 1;
        }
        // then line comments
        let mut out = String::with_capacity(no_block.len());
        for line in no_block.lines() {
            let l = match line.find("--") {
                Some(k) => &line[..k],
                None => line,
            };
            out.push_str(l);
            out.push('\n');
        }
        out
    }

    /// Extract the names of `CREATE TABLE`s that have a tenant-scoping key
    /// column. Transient table-rebuild artifacts (`*_new`) are excluded — they
    /// are `DROP`+`RENAME`'d to their canonical name in the same migration.
    fn extract_tenant_keyed_tables(sql: &str) -> Vec<String> {
        const KEY_COLS: &[&str] = &[
            "tenant_id",
            "namespace",
            "clerk_sub",
            "clerk_user_id",
            "email_hash",
            "recipient_hash",
        ];
        let clean = strip_sql_comments(sql);
        let mut out = Vec::new();
        let mut rest = clean.as_str();
        while let Some(pos) = rest.find("CREATE TABLE") {
            let after = rest[pos + "CREATE TABLE".len()..].trim_start();
            // optional IF NOT EXISTS (case-insensitive)
            let after = {
                let lower = after.to_ascii_lowercase();
                if lower.starts_with("if not exists") {
                    after["if not exists".len()..].trim_start()
                } else {
                    after
                }
            };
            // table name = up to first whitespace or '('
            let name_end = after
                .find(|c: char| c.is_whitespace() || c == '(')
                .unwrap_or(after.len());
            let name = after[..name_end].trim().to_string();
            // body = from first '(' to the first "); " statement terminator.
            // tenant_id/namespace are early columns, so truncating at the first
            // ");" is sufficient for key-column detection.
            let body = match after.find('(') {
                Some(open) => {
                    let from_open = &after[open..];
                    match from_open.find(");") {
                        Some(close) => &from_open[..close],
                        None => from_open,
                    }
                }
                None => "",
            };
            if !name.is_empty()
                && !name.ends_with("_new")
                && KEY_COLS.iter().any(|k| contains_word(body, k))
            {
                out.push(name);
            }
            rest = &after[name_end..];
        }
        out
    }

    /// `body.contains(key)` but only as a whole identifier token, so e.g.
    /// `actor_email_hash` does NOT match `email_hash` and `kv_namespace_id`
    /// does NOT match `namespace` (word char = ASCII alnum or `_`).
    fn contains_word(body: &str, key: &str) -> bool {
        let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let kb = key.as_bytes();
        let bb = body.as_bytes();
        let mut i = 0usize;
        while let Some(rel) = body[i..].find(key) {
            let start = i + rel;
            let end = start + kb.len();
            let before_ok = start == 0 || !is_word(bb[start - 1] as char);
            let after_ok = end >= bb.len() || !is_word(bb[end] as char);
            if before_ok && after_ok {
                return true;
            }
            i = start + 1;
        }
        false
    }
}
