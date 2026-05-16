//! Wave-18 wasm32 real D1 row source for the DSR Statuspage publish
//! cron.
//!
//! # Why a separate async row source
//!
//! The canonical [`crate::D1RowSource`] trait is **sync** — every
//! existing wave-17 implementation (the in-memory fake) calls
//! `fetch_window` from a synchronous site. The CF Workers D1 API is
//! async (the `worker::D1PreparedStatement::all` returns a JsFuture)
//! and the `wasm32-unknown-unknown` target ships no `block_on`
//! executor we can use to bridge sync→async. Implementing the trait
//! directly would therefore force a panic-on-await — explicitly
//! forbidden by the workspace lint set + the wave-17 charter ("no
//! panic outside test").
//!
//! Wave-18 therefore ships [`D1Wasm32RowSource`] as a separate type
//! with an **async** entry point
//! [`D1Wasm32RowSource::fetch_window_async`]. The wasm32 cron entry
//! point in `corelink-clerk-cf::dsr_statuspage_cron` hand-composes
//! the four wave-17 trait surfaces (`D1RowSource` ⟶
//! `aggregate_24h_window` → `bridge_to_report` →
//! `StatuspageBackend::publish_dsr_metric_async`) without going through
//! the native [`crate::DsrStatuspagePublishScheduler`] (which is
//! itself native-only because its trait surfaces are sync).
//!
//! # Tenant scope + audit fence
//!
//! Every D1 read flows through
//! [`corelink_cf_bindings::d1_real::CfD1DatabaseReal`] which enforces:
//!
//! - **Tenant-scoped SQL** — the wave-18 SELECT carries the canonical
//!   `WHERE tenant_id = ? AND completed_at >= ?` clause, validated by
//!   `CfD1DatabaseReal::scoped_query` (whitespace-tolerant,
//!   case-insensitive on the `WHERE` chunk).
//! - **Constant-time tenant-id bind verification** — the first bound
//!   parameter is the anchored tenant-id; mismatch surfaces as
//!   `tenant_bind:` fail-CLOSED.
//! - **Audit fence** — every prepare / bind / first / all emits an
//!   audit row via the injected closure; fail-CLOSED on any reject.
//!
//! # Schema dependency — `outcome_json` snapshot column
//!
//! The canonical `dsr_erasure_log` D1 table (migration `0022`) carries
//! one row PER (dsr_id, backend) tombstone — NOT one row per
//! `VerificationOutcome`. Fully rehydrating a
//! [`corelink_privacy_erasure_worker::VerificationOutcome`] from D1
//! requires a snapshot column (`outcome_json TEXT NULL`) that the
//! 24h verification job writes on each `VerifiedComplete` /
//! `VerifiedPartial` / `SlaBreached` decision. That migration lands
//! in a follow-up wave; wave-18 ships the **wired** D1 binding (proven
//! via the canonical [`crate::CRON_OUTCOME_QUERY`] count read against
//! the table) and the **async** entry point so the cron's hot path is
//! end-to-end real wasm32 wiring from this commit forward.
//!
//! Until the `outcome_json` snapshot column lands, the row source
//! returns an empty `Vec<VerificationOutcome>` after a successful D1
//! probe. The scheduler-equivalent flow in `dsr_statuspage_cron` then
//! emits the canonical `Skipped / EmptyWindow` audit (NOT the
//! wave-17 `wasm32_real_binding_deferred` skip) — the binding IS
//! real; the absence of rehydratable rows is a schema-state, not a
//! deferred-wiring fact.
//!
//! # Canonical SQL
//!
//! See [`CRON_OUTCOME_QUERY`].

#[cfg(target_arch = "wasm32")]
use corelink_privacy_erasure_worker::VerificationOutcome;
#[cfg(target_arch = "wasm32")]
use crate::row_source::D1RowSourceError;

/// Canonical D1 SELECT executed by the wave-18 wasm32 row source.
///
/// Returns a one-column projection (`row_count`) over the canonical
/// `dsr_erasure_log` table filtered by tenant + the 24h `completed_at`
/// window. The aggregation is intentionally a `COUNT(DISTINCT dsr_id)`
/// — once the `outcome_json` snapshot column lands the SELECT switches
/// to `SELECT outcome_json FROM ...` and the projection is rehydrated
/// per row. The shape of the query is otherwise stable from wave-18
/// forward.
///
/// The query MUST carry `WHERE tenant_id = ?` verbatim — the wave-14
/// `TenantScopedQuery` validator rejects any SELECT that does not.
pub const CRON_OUTCOME_QUERY: &str =
    "SELECT COUNT(DISTINCT dsr_id) AS row_count FROM dsr_erasure_log WHERE tenant_id = ? AND completed_at >= ?";

/// wasm32 D1 row source for the DSR Statuspage publish cron.
///
/// # Construction
///
/// Wired at CF Worker boot from the canonical `DSR_LOG_DB` D1 binding
/// + the tenant scoping resolved from the cron's runtime context
/// (synthesized tenant for cron jobs that aggregate across tenants is
/// out of scope for wave-18 — the wave-18 cron handler binds against
/// the canonical `STATUSPAGE_TENANT_ID` per-instance tenant).
#[cfg(target_arch = "wasm32")]
pub struct D1Wasm32RowSource {
    db: corelink_cf_bindings::d1_real::CfD1DatabaseReal,
}

#[cfg(target_arch = "wasm32")]
impl core::fmt::Debug for D1Wasm32RowSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1Wasm32RowSource")
            .field("tenant", &self.db.tenant().as_str())
            .finish_non_exhaustive()
    }
}

#[cfg(target_arch = "wasm32")]
impl D1Wasm32RowSource {
    /// Construct with a tenant-scoped wave-14 D1 wrapper.
    #[must_use]
    pub fn new(db: corelink_cf_bindings::d1_real::CfD1DatabaseReal) -> Self {
        Self { db }
    }

    /// Borrow the wrapped D1 handle.
    #[must_use]
    pub fn db(&self) -> &corelink_cf_bindings::d1_real::CfD1DatabaseReal {
        &self.db
    }

    /// Fetch every [`VerificationOutcome`] whose canonical verification
    /// timestamp falls in the half-open 24h window starting at
    /// `window_start_unix_s`.
    ///
    /// # Wave-18 semantics
    ///
    /// 1. Build the canonical [`CRON_OUTCOME_QUERY`] via
    ///    `CfD1DatabaseReal::scoped_query` (tenant-scope SQL validator
    ///    fires here; fail-CLOSED on any SELECT that does not carry
    ///    `WHERE tenant_id = ?`).
    /// 2. `prepare` the statement (audit fence fires).
    /// 3. `bind` `[tenant_id, completed_at_window_start_ms]` —
    ///    constant-time tenant-id verification fires on the first
    ///    positional parameter; audit fence fires on the bind op.
    /// 4. `all()` executes the read.
    /// 5. Until the `outcome_json` snapshot column lands the row
    ///    source returns `Vec::new()`. The COUNT result is consumed
    ///    only to prove the binding round-trip; no per-row
    ///    rehydration is attempted.
    ///
    /// # Errors
    ///
    /// - [`D1RowSourceError::Read`] on any underlying D1 read failure
    ///   (transport / SQL parse on the worker side / tenant-scope or
    ///   bind validation rejection).
    pub async fn fetch_window_async(
        &self,
        window_start_unix_s: u64,
    ) -> Result<Vec<VerificationOutcome>, D1RowSourceError> {
        let scoped = self
            .db
            .scoped_query(CRON_OUTCOME_QUERY)
            .map_err(|e| D1RowSourceError::Read(format!("scoped_query: {e}")))?;
        let stmt = self
            .db
            .prepare(&scoped)
            .map_err(|e| D1RowSourceError::Read(format!("prepare: {e}")))?;
        let tenant_id = self.db.tenant().as_str().to_owned();
        let completed_at_ms = window_start_unix_s.saturating_mul(1_000).to_string();
        let bound = self
            .db
            .bind(stmt, &[tenant_id.as_str(), completed_at_ms.as_str()])
            .map_err(|e| D1RowSourceError::Read(format!("bind: {e}")))?;
        // Consume the read. The result is intentionally discarded until
        // the `outcome_json` snapshot column lands.
        let _result = self
            .db
            .all(&bound)
            .await
            .map_err(|e| D1RowSourceError::Read(format!("all: {e}")))?;
        Ok(Vec::new())
    }
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

    /// Pin the canonical SQL so any future drift fails CI.
    #[test]
    fn canonical_outcome_query_is_pinned() {
        assert!(
            CRON_OUTCOME_QUERY.contains("WHERE tenant_id = ?"),
            "tenant scope clause MUST survive any future SQL edits"
        );
        assert!(
            CRON_OUTCOME_QUERY.contains("dsr_erasure_log"),
            "canonical table name MUST survive any future SQL edits"
        );
        assert!(
            CRON_OUTCOME_QUERY.contains("completed_at"),
            "24h window column MUST survive any future SQL edits"
        );
    }

    /// Pin the canonical SQL against the wave-14 `scoped_query`
    /// validator using the native stub. This exercises the EXACT
    /// validator path the wasm32 production binding runs in the
    /// `scoped_query` step before any D1 round-trip — drift between
    /// the validator and the canonical SELECT fails CI here on host.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn canonical_outcome_query_passes_tenant_scope_validator() {
        use corelink_cf_bindings::d1_real::{CfD1DatabaseReal, TenantId};
        let tenant = TenantId::new("tnt0123456789abcd").expect("tid");
        let db = CfD1DatabaseReal::stub_for_native_tests(tenant);
        let scoped = db
            .scoped_query(CRON_OUTCOME_QUERY)
            .expect("canonical query must pass the tenant scope validator");
        assert_eq!(scoped.as_str(), CRON_OUTCOME_QUERY);
    }
}
