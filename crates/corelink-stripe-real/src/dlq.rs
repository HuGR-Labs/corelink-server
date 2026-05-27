//! Dead-letter quarantine (DLQ) surface for Stripe webhook events
//! that exhausted handler retry.
//!
//! # Why this module exists
//!
//! Closes the P0 gaps surfaced in
//! `specs/_audits/sealed/2026-05-15-webhook-retry-dlq.md`:
//!
//! - **GAP-P0-1**: Stripe-side retries top out after 3 days. A
//!   persistently-failing handler (tier-ledger bug, transient backend
//!   that does not recover within the Stripe window, etc.) means the
//!   event is **silently dropped** with NO operator-actionable copy on
//!   the CoreLink side. This module ships the canonical DLQ data
//!   shape ([`WebhookDlqRow`]) + in-memory trait fake the production
//!   D1 binding wires against ([`WebhookDlqStore`]).
//!
//! - **GAP-P0-2**: No alerting surface for DLQ depth / oldest-row age.
//!   This module ships the canonical Prometheus metric-name constants
//!   ([`DLQ_DEPTH_GAUGE`], [`DLQ_OLDEST_AGE_SECONDS_GAUGE`],
//!   [`DLQ_QUARANTINED_TOTAL`], [`DLQ_REPLAYED_TOTAL`]) +
//!   recommended alert thresholds ([`DLQ_WARN_DEPTH`],
//!   [`DLQ_PAGE_OLDEST_AGE_SECONDS`]).
//!
//! - **GAP-P0-3**: Replay surface forward-compatible with dual-approval.
//!   The DLQ row carries `replay_request_id` + `replayed_by` slots so
//!   the admin-API surface (S-R3.x deferred) can write a dual-approval
//!   audit trail without a schema migration.
//!
//! # Production wiring (deferred to admin API sprint)
//!
//! The canonical D1 binding lives in `migrations/d1/0045_stripe_webhook_dlq.sql`
//! (additive). The production [`WebhookDlqStore`] impl runs in
//! `apps/server` against D1 via the canonical
//! `INSERT INTO stripe_webhook_events_dlq …` SQL surface; this module
//! ships the trait + in-memory fake the unit + property tests exercise.
//!
//! # Invariants
//!
//! - **DLQ_TTL_ENFORCED**: rows with `expires_at_ms <= now_ms` MUST be
//!   pruned. Pinned by the property test in
//!   `crates/corelink-stripe-real/tests/prop_dlq.rs`.
//! - **DLQ_IDEMPOTENT_ON_EVENT_ID**: re-quarantining the same
//!   `event_id` (after a failed replay attempt) does NOT insert a
//!   second row — instead the existing row's `attempt_count` is
//!   incremented + `last_seen_at_ms` / `last_error` are updated.
//!   The `dlq_row_id` stays stable so the audit trail does not split.
//! - **NEVER LOG RAW BODY OR SECRETS**: `Debug` impl on
//!   [`WebhookDlqRow`] redacts `raw_body_hex` to `<redacted len=N>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// =========================================================================
// Canonical Prometheus metric names + alert thresholds (GAP-P0-2 closure).
// =========================================================================

/// Gauge metric: current DLQ depth (count of un-replayed rows with
/// `expires_at_ms > now_ms`). Target = 0.
pub const DLQ_DEPTH_GAUGE: &str = "corelink_stripe_webhook_dlq_depth";

/// Gauge metric: age in seconds of the oldest un-replayed DLQ row
/// (now_ms - first_seen_at_ms). Target = 0.
pub const DLQ_OLDEST_AGE_SECONDS_GAUGE: &str =
    "corelink_stripe_webhook_dlq_oldest_age_seconds";

/// Counter metric: total events quarantined into the DLQ since
/// process boot. Labels: `event_type`.
pub const DLQ_QUARANTINED_TOTAL: &str =
    "corelink_stripe_webhook_dlq_quarantined_total";

/// Counter metric: total replay attempts. Labels:
/// `outcome ∈ {succeeded,failed,abandoned}`.
pub const DLQ_REPLAYED_TOTAL: &str = "corelink_stripe_webhook_dlq_replayed_total";

/// Counter metric: total DLQ rows pruned (TTL expiry). Labels: none.
pub const DLQ_PRUNED_TOTAL: &str = "corelink_stripe_webhook_dlq_pruned_total";

/// Warning-level alert threshold: any non-zero DLQ depth → page
/// on-call at warning severity (= Slack, not PagerDuty).
pub const DLQ_WARN_DEPTH: u64 = 1;

/// Page-level alert threshold: oldest DLQ row > 6h →
/// PagerDuty page (something needs human attention).
pub const DLQ_PAGE_OLDEST_AGE_SECONDS: u64 = 6 * 60 * 60;

/// Default TTL applied to a freshly-quarantined DLQ row: 30 days.
/// After this point the canonical prune job removes the row.
pub const DEFAULT_DLQ_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

// =========================================================================
// Replay outcome taxonomy.
// =========================================================================

/// Canonical outcome for a DLQ replay attempt. Mirrors the SQL CHECK
/// constraint on `stripe_webhook_events_dlq.replay_outcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DlqReplayOutcome {
    /// Replay succeeded; the handler returned `Ok` on retry. The DLQ
    /// row stays for audit (`replay_outcome = 'succeeded'`) but is
    /// no longer counted in [`DLQ_DEPTH_GAUGE`].
    Succeeded,
    /// Replay failed; handler returned `Err` again. Row remains in
    /// DLQ; `attempt_count` is incremented.
    Failed,
    /// Operator decided the event is no longer applicable (e.g.
    /// stale state, customer churned, known bug already fix-forwarded).
    /// Row is marked abandoned + emitted to the audit chain;
    /// no further replay attempts will run.
    Abandoned,
}

impl DlqReplayOutcome {
    /// Canonical string form matching the SQL CHECK constraint.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }
}

// =========================================================================
// DLQ row shape (mirror of D1 `stripe_webhook_events_dlq`).
// =========================================================================

/// One row of the `stripe_webhook_events_dlq` table.
///
/// `raw_body_hex` carries the Stripe-signed body bytes (hex-encoded for
/// safe TEXT storage). The body was already HMAC-verified BEFORE this
/// row was constructed (per the route invariant) so the contents are
/// authentic Stripe payload bytes.
#[derive(Clone)]
#[non_exhaustive]
pub struct WebhookDlqRow {
    /// Stripe event id (`evt_…`).
    pub event_id: String,
    /// Per-row UUIDv7 identifier (application-generated).
    pub dlq_row_id: String,
    /// Stripe event type (e.g. `customer.subscription.created`).
    pub event_type: String,
    /// Raw webhook body bytes, hex-encoded. NEVER logged.
    pub raw_body_hex: String,
    /// Correlation id from the audit chain.
    pub correlation_id: String,
    /// Number of dispatch attempts before quarantine (≥ 1).
    pub attempt_count: u32,
    /// First time this `event_id` was quarantined (ms since epoch).
    pub first_seen_at_ms: u64,
    /// Most recent quarantine time (ms since epoch).
    pub last_seen_at_ms: u64,
    /// Redacted error string from the `processing_failed` audit arm.
    pub last_error: String,
    /// Hard expiry — pruning job removes rows past this point.
    pub expires_at_ms: u64,
    /// Replay-request slot (NULL until a replay is initiated).
    pub replay_request_id: Option<String>,
    /// Operator id that initiated the replay (NULL until replayed).
    pub replayed_by: Option<String>,
    /// Replay timestamp (NULL until replayed).
    pub replayed_at_ms: Option<u64>,
    /// Replay outcome (NULL until replayed).
    pub replay_outcome: Option<DlqReplayOutcome>,
}

impl core::fmt::Debug for WebhookDlqRow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // NEVER expose raw body in Debug output.
        let redacted_body = format!("<redacted len={}>", self.raw_body_hex.len());
        f.debug_struct("WebhookDlqRow")
            .field("event_id", &self.event_id)
            .field("dlq_row_id", &self.dlq_row_id)
            .field("event_type", &self.event_type)
            .field("raw_body_hex", &redacted_body)
            .field("correlation_id", &self.correlation_id)
            .field("attempt_count", &self.attempt_count)
            .field("first_seen_at_ms", &self.first_seen_at_ms)
            .field("last_seen_at_ms", &self.last_seen_at_ms)
            .field("last_error", &self.last_error)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("replay_request_id", &self.replay_request_id)
            .field("replayed_by", &self.replayed_by)
            .field("replayed_at_ms", &self.replayed_at_ms)
            .field("replay_outcome", &self.replay_outcome)
            .finish()
    }
}

impl WebhookDlqRow {
    /// Build a freshly-quarantined row with default TTL.
    #[must_use]
    pub fn new_quarantine(
        event_id: impl Into<String>,
        dlq_row_id: impl Into<String>,
        event_type: impl Into<String>,
        raw_body_hex: impl Into<String>,
        correlation_id: impl Into<String>,
        last_error: impl Into<String>,
        now_ms: u64,
    ) -> Self {
        let first_seen = now_ms;
        let expires = now_ms.saturating_add(DEFAULT_DLQ_TTL_MS);
        Self {
            event_id: event_id.into(),
            dlq_row_id: dlq_row_id.into(),
            event_type: event_type.into(),
            raw_body_hex: raw_body_hex.into(),
            correlation_id: correlation_id.into(),
            attempt_count: 1,
            first_seen_at_ms: first_seen,
            last_seen_at_ms: first_seen,
            last_error: last_error.into(),
            expires_at_ms: expires,
            replay_request_id: None,
            replayed_by: None,
            replayed_at_ms: None,
            replay_outcome: None,
        }
    }

    /// True if `now_ms >= expires_at_ms` (this row should be pruned).
    #[must_use]
    pub const fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

// =========================================================================
// DLQ store trait + in-memory fake.
// =========================================================================

/// Outcome of a `try_quarantine` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DlqQuarantineOutcome {
    /// First-time quarantine; a fresh row was inserted.
    Inserted,
    /// `event_id` was already in the DLQ; `attempt_count` +
    /// `last_seen_at_ms` + `last_error` were updated on the existing
    /// row (DLQ_IDEMPOTENT_ON_EVENT_ID invariant).
    Updated,
}

/// Errors surfaced by the [`WebhookDlqStore`] trait.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DlqError {
    /// Backend transient failure (D1 lock, network blip, etc.).
    #[error("DLQ backend transient error: {0}")]
    Backend(String),
    /// Mutex poisoned (in-memory store only).
    #[error("DLQ in-memory mutex poisoned")]
    MutexPoisoned,
}

/// Trait every DLQ backend satisfies. Production wires this to a D1
/// binding emitting the canonical `INSERT … ON CONFLICT (event_id)
/// DO UPDATE …` SQL; tests use [`InMemoryWebhookDlqStore`].
pub trait WebhookDlqStore: core::fmt::Debug + Send + Sync {
    /// Quarantine an event. Idempotent on `event_id`:
    /// - first call → inserts a new row + returns `Inserted`.
    /// - subsequent calls with same `event_id` → updates the
    ///   existing row (`attempt_count += 1`, `last_seen_at_ms`,
    ///   `last_error`) + returns `Updated`.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError`] on backend transient failure or mutex
    /// poisoning.
    fn try_quarantine(
        &self,
        row: WebhookDlqRow,
    ) -> Result<DlqQuarantineOutcome, DlqError>;

    /// Record a replay outcome on an existing row. No-op if
    /// `event_id` is not in the DLQ.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError`] on backend transient failure.
    fn record_replay(
        &self,
        event_id: &str,
        replay_request_id: &str,
        replayed_by: &str,
        replayed_at_ms: u64,
        outcome: DlqReplayOutcome,
    ) -> Result<(), DlqError>;

    /// Current depth (un-replayed, un-expired rows). Cheap snapshot
    /// for the [`DLQ_DEPTH_GAUGE`] export.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError`] on backend transient failure.
    fn depth(&self, now_ms: u64) -> Result<u64, DlqError>;

    /// Age in seconds of the oldest un-replayed, un-expired row.
    /// Returns 0 if the DLQ is empty.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError`] on backend transient failure.
    fn oldest_age_seconds(&self, now_ms: u64) -> Result<u64, DlqError>;

    /// Prune expired rows (`expires_at_ms <= now_ms`). Returns count
    /// of rows removed.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError`] on backend transient failure.
    fn prune_expired(&self, now_ms: u64) -> Result<u64, DlqError>;

    /// Look up by `event_id` for triage / replay surfaces.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError`] on backend transient failure.
    fn get(&self, event_id: &str) -> Result<Option<WebhookDlqRow>, DlqError>;
}

/// In-memory `Arc<Mutex<HashMap>>` DLQ store. Mirrors the D1
/// `INSERT … ON CONFLICT DO UPDATE` semantics exactly.
#[derive(Clone, Debug, Default)]
pub struct InMemoryWebhookDlqStore {
    rows: Arc<Mutex<HashMap<String, WebhookDlqRow>>>,
}

impl InMemoryWebhookDlqStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total row count (including replayed + expired rows). For test
    /// assertions.
    ///
    /// # Errors
    ///
    /// Returns [`DlqError::MutexPoisoned`] if the inner mutex is
    /// poisoned.
    pub fn total_rows(&self) -> Result<usize, DlqError> {
        let g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        Ok(g.len())
    }
}

fn is_active(row: &WebhookDlqRow, now_ms: u64) -> bool {
    // "Active" = un-replayed, un-expired. Succeeded replays still
    // count as an audit row (they live for forensics) but they're
    // not counted toward depth / age (they no longer need attention).
    if row.is_expired(now_ms) {
        return false;
    }
    match row.replay_outcome {
        None | Some(DlqReplayOutcome::Failed) => true,
        Some(DlqReplayOutcome::Succeeded) | Some(DlqReplayOutcome::Abandoned) => false,
    }
}

impl WebhookDlqStore for InMemoryWebhookDlqStore {
    fn try_quarantine(
        &self,
        row: WebhookDlqRow,
    ) -> Result<DlqQuarantineOutcome, DlqError> {
        let mut g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        if let Some(existing) = g.get_mut(&row.event_id) {
            existing.attempt_count = existing.attempt_count.saturating_add(1);
            existing.last_seen_at_ms = row.last_seen_at_ms;
            existing.last_error = row.last_error.clone();
            Ok(DlqQuarantineOutcome::Updated)
        } else {
            g.insert(row.event_id.clone(), row);
            Ok(DlqQuarantineOutcome::Inserted)
        }
    }

    fn record_replay(
        &self,
        event_id: &str,
        replay_request_id: &str,
        replayed_by: &str,
        replayed_at_ms: u64,
        outcome: DlqReplayOutcome,
    ) -> Result<(), DlqError> {
        let mut g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        if let Some(existing) = g.get_mut(event_id) {
            existing.replay_request_id = Some(replay_request_id.to_string());
            existing.replayed_by = Some(replayed_by.to_string());
            existing.replayed_at_ms = Some(replayed_at_ms);
            existing.replay_outcome = Some(outcome);
        }
        Ok(())
    }

    fn depth(&self, now_ms: u64) -> Result<u64, DlqError> {
        let g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        let n = g.values().filter(|r| is_active(r, now_ms)).count();
        Ok(u64::try_from(n).unwrap_or(u64::MAX))
    }

    fn oldest_age_seconds(&self, now_ms: u64) -> Result<u64, DlqError> {
        let g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        let oldest_ms = g
            .values()
            .filter(|r| is_active(r, now_ms))
            .map(|r| r.first_seen_at_ms)
            .min();
        match oldest_ms {
            None => Ok(0),
            Some(t) => Ok(now_ms.saturating_sub(t) / 1_000),
        }
    }

    fn prune_expired(&self, now_ms: u64) -> Result<u64, DlqError> {
        let mut g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        let before = g.len();
        g.retain(|_, r| !r.is_expired(now_ms));
        let removed = before.saturating_sub(g.len());
        Ok(u64::try_from(removed).unwrap_or(u64::MAX))
    }

    fn get(&self, event_id: &str) -> Result<Option<WebhookDlqRow>, DlqError> {
        let g = self.rows.lock().map_err(|_| DlqError::MutexPoisoned)?;
        Ok(g.get(event_id).cloned())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;

    fn fresh_row(event_id: &str, now_ms: u64) -> WebhookDlqRow {
        WebhookDlqRow::new_quarantine(
            event_id,
            format!("dlq_{event_id}"),
            "customer.subscription.created",
            "deadbeef",
            "corr_test",
            "tier ledger transient",
            now_ms,
        )
    }

    #[test]
    fn quarantine_then_update_is_idempotent_on_event_id() {
        let store = InMemoryWebhookDlqStore::new();
        let now = 1_715_000_000_000_u64;
        let out1 = store.try_quarantine(fresh_row("evt_1", now)).unwrap();
        let out2 = store.try_quarantine(fresh_row("evt_1", now + 1_000)).unwrap();
        assert_eq!(out1, DlqQuarantineOutcome::Inserted);
        assert_eq!(out2, DlqQuarantineOutcome::Updated);
        let row = store.get("evt_1").unwrap().expect("row");
        assert_eq!(row.attempt_count, 2);
        assert_eq!(row.last_seen_at_ms, now + 1_000);
        assert_eq!(store.total_rows().unwrap(), 1);
    }

    #[test]
    fn depth_excludes_replayed_succeeded() {
        let store = InMemoryWebhookDlqStore::new();
        let now = 1_715_000_000_000_u64;
        store.try_quarantine(fresh_row("evt_a", now)).unwrap();
        store.try_quarantine(fresh_row("evt_b", now)).unwrap();
        assert_eq!(store.depth(now + 1_000).unwrap(), 2);

        store
            .record_replay(
                "evt_a",
                "rep_1",
                "ops_alice",
                now + 500,
                DlqReplayOutcome::Succeeded,
            )
            .unwrap();
        assert_eq!(store.depth(now + 1_000).unwrap(), 1);
    }

    #[test]
    fn prune_expired_removes_only_past_ttl_rows() {
        let store = InMemoryWebhookDlqStore::new();
        let now = 1_715_000_000_000_u64;
        store.try_quarantine(fresh_row("evt_x", now)).unwrap();
        // Not yet expired.
        assert_eq!(store.prune_expired(now + 1_000).unwrap(), 0);
        // Past TTL.
        let past = now + DEFAULT_DLQ_TTL_MS + 1_000;
        assert_eq!(store.prune_expired(past).unwrap(), 1);
        assert_eq!(store.total_rows().unwrap(), 0);
    }

    #[test]
    fn oldest_age_seconds_reports_oldest_first_seen() {
        let store = InMemoryWebhookDlqStore::new();
        let t0 = 1_715_000_000_000_u64;
        store.try_quarantine(fresh_row("evt_old", t0)).unwrap();
        store
            .try_quarantine(fresh_row("evt_new", t0 + 60_000))
            .unwrap();
        let now = t0 + 90_000;
        // Oldest is t0, age = 90s.
        assert_eq!(store.oldest_age_seconds(now).unwrap(), 90);
    }

    #[test]
    fn oldest_age_zero_when_only_replayed_rows_remain() {
        let store = InMemoryWebhookDlqStore::new();
        let t0 = 1_715_000_000_000_u64;
        store.try_quarantine(fresh_row("evt_done", t0)).unwrap();
        store
            .record_replay(
                "evt_done",
                "rep_1",
                "ops_bob",
                t0 + 5_000,
                DlqReplayOutcome::Succeeded,
            )
            .unwrap();
        assert_eq!(store.oldest_age_seconds(t0 + 30_000).unwrap(), 0);
    }

    #[test]
    fn debug_redacts_raw_body() {
        let row = fresh_row("evt_secret", 0);
        let formatted = format!("{row:?}");
        assert!(formatted.contains("<redacted len=8>"));
        assert!(!formatted.contains("deadbeef"));
    }

    #[test]
    fn replay_outcome_str_matches_sql_check_strings() {
        assert_eq!(DlqReplayOutcome::Succeeded.as_str(), "succeeded");
        assert_eq!(DlqReplayOutcome::Failed.as_str(), "failed");
        assert_eq!(DlqReplayOutcome::Abandoned.as_str(), "abandoned");
    }

    #[test]
    fn metric_name_constants_use_canonical_prefix() {
        assert!(DLQ_DEPTH_GAUGE.starts_with("corelink_stripe_webhook_dlq_"));
        assert!(DLQ_OLDEST_AGE_SECONDS_GAUGE.starts_with("corelink_stripe_webhook_dlq_"));
        assert!(DLQ_QUARANTINED_TOTAL.starts_with("corelink_stripe_webhook_dlq_"));
        assert!(DLQ_REPLAYED_TOTAL.starts_with("corelink_stripe_webhook_dlq_"));
        assert!(DLQ_PRUNED_TOTAL.starts_with("corelink_stripe_webhook_dlq_"));
    }
}
