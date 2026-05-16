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
//! # Schema dependency — `outcome_json` snapshot column (wave-19 ENABLED)
//!
//! The canonical `dsr_erasure_log` D1 table (migration `0022`) carries
//! one row PER (dsr_id, backend) tombstone — NOT one row per
//! `VerificationOutcome`. Fully rehydrating a
//! [`corelink_privacy_erasure_worker::VerificationOutcome`] from D1
//! requires a snapshot column (`outcome_json TEXT NULL`) that the
//! 24h verification job writes on each `VerifiedComplete` /
//! `VerifiedPartial` / `SlaBreached` decision.
//!
//! **Wave-19 (migration `0049`):** the `outcome_json` column lands as
//! an additive `ALTER TABLE ADD COLUMN` per the wave-19 closure note.
//! [`CRON_OUTCOME_QUERY`] is rewritten to project `outcome_json` and
//! [`D1Wasm32RowSource::fetch_window_async`] parses each row via
//! `serde_json::from_str` into a [`VerificationOutcome`]. NULL rows
//! (legacy, written pre-wave-19) are skipped — they will rotate out
//! within the 24h sweep window as wave-19 forward rolls in.
//!
//! Parse failures on a non-NULL `outcome_json` value surface as
//! [`D1RowSourceError::Parse`] (fail-CLOSED per the wave-17 trait
//! contract).
//!
//! # Canonical SQL
//!
//! See [`CRON_OUTCOME_QUERY`].

use corelink_privacy_erasure_worker::VerificationOutcome;
#[cfg(target_arch = "wasm32")]
use crate::row_source::D1RowSourceError;

/// Canonical wave-19 `outcome_json` parser. Pure helper exposed at
/// public visibility so the native test harness (proptest) can pin
/// the round-trip discipline (serde derives) without requiring a
/// wasm32 toolchain. Production callers reach this through
/// [`D1Wasm32RowSource::fetch_window_async`].
///
/// # Errors
///
/// Returns the `serde_json` error message as a `String` (wrapped at
/// the call site into [`D1RowSourceError::Parse`]).
pub fn parse_outcome_json(s: &str) -> Result<VerificationOutcome, String> {
    serde_json::from_str::<VerificationOutcome>(s)
        .map_err(|e| format!("outcome_json parse: {e}"))
}

/// Canonical wave-19 `outcome_json` serializer. Symmetric to
/// [`parse_outcome_json`]; the verification job writer path
/// (`corelink-privacy-erasure-worker`) renders the column value via
/// this helper to keep the round-trip pinned in one place.
///
/// # Errors
///
/// Returns the `serde_json` error message as a `String`.
pub fn render_outcome_json(outcome: &VerificationOutcome) -> Result<String, String> {
    serde_json::to_string(outcome)
        .map_err(|e| format!("outcome_json render: {e}"))
}

/// Canonical row shape projected by [`CRON_OUTCOME_QUERY`]. One field
/// — the `outcome_json` snapshot column (migration `0049`). The SQL
/// filters out NULL rows so the field is non-Option at the typed
/// boundary; a NULL slipping through (would only happen on a future
/// SQL drift) parses as a `D1RowSourceError::Parse` fail-CLOSED at
/// the call site.
#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct OutcomeJsonRow {
    outcome_json: String,
}

/// Canonical D1 SELECT executed by the wave-19 wasm32 row source.
///
/// Returns the `outcome_json` snapshot column projection (migration
/// `0049`) over the canonical `dsr_erasure_log` table filtered by
/// tenant + the 24h `completed_at` window. NULL `outcome_json` rows
/// (legacy, pre-wave-19) are filtered out at the SQL layer — the cron
/// must only re-hydrate canonical `VerificationOutcome` snapshots.
///
/// The query MUST carry `WHERE tenant_id = ?` verbatim — the wave-14
/// `TenantScopedQuery` validator rejects any SELECT that does not.
pub const CRON_OUTCOME_QUERY: &str =
    "SELECT outcome_json FROM dsr_erasure_log WHERE tenant_id = ? AND completed_at >= ? AND outcome_json IS NOT NULL";

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
    /// # Wave-19 semantics
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
    /// 5. Each row's `outcome_json` snapshot column (migration `0049`)
    ///    is parsed via `serde_json::from_str` into a
    ///    [`VerificationOutcome`]; any parse failure surfaces as
    ///    [`D1RowSourceError::Parse`] fail-CLOSED per the wave-17
    ///    trait contract.
    ///
    /// # Errors
    ///
    /// - [`D1RowSourceError::Read`] on any underlying D1 read failure
    ///   (transport / SQL parse on the worker side / tenant-scope or
    ///   bind validation rejection).
    /// - [`D1RowSourceError::Parse`] when a row's `outcome_json`
    ///   column fails to deserialize as a canonical
    ///   [`VerificationOutcome`] (fail-CLOSED — aborts the publish).
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
        let result = self
            .db
            .all(&bound)
            .await
            .map_err(|e| D1RowSourceError::Read(format!("all: {e}")))?;
        let rows: Vec<OutcomeJsonRow> = result
            .results()
            .map_err(|e| D1RowSourceError::Read(format!("results: {e}")))?;
        let mut outcomes = Vec::with_capacity(rows.len());
        for (idx, row) in rows.into_iter().enumerate() {
            let parsed = parse_outcome_json(&row.outcome_json).map_err(|e| {
                D1RowSourceError::Parse(format!("row {idx}: {e}"))
            })?;
            outcomes.push(parsed);
        }
        Ok(outcomes)
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
