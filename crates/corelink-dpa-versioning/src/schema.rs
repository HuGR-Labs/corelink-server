//! Schema shapes mirroring the D1 tables added by the
//! `migrations/d1/0041_dpa_versioning.sql` additive migration.
//!
//! `dpa_versions`: append-only catalog of every DPA version ever
//! published. `tenants` adds `current_dpa_version` (the last version
//! the tenant accepted) + `dpa_grace_expires_at` (only set when the
//! tenant has a pending re-acceptance).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::version::SemverVersion;

/// Logical clock used by the in-memory orchestrator + the
/// [`crate::cron::GraceExpirationCron`]. Production wiring substitutes
/// a `Utc::now().timestamp()` source.
pub type UnixSeconds = i64;

/// Canonical grace period in seconds = 30 days.
pub const GRACE_PERIOD_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Reminder window in seconds = 7 days. Tenants with
/// `grace_expires_at - now <= REMINDER_WINDOW_SECONDS` get a daily
/// nudge.
pub const REMINDER_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// An entry in the `dpa_versions` D1 table.
///
/// `content_hash` is the SHA-256 of the canonicalised DPA text;
/// `content_url` is the immutable R2 URL (legal/dpa/v\<x\>.\<y\>.\<z\>.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpaVersionRecord {
    /// Semver triple.
    pub version: SemverVersion,
    /// Publication timestamp (UTC seconds).
    pub published_at: UnixSeconds,
    /// SHA-256 of the canonicalised DPA text body.
    pub content_hash: String,
    /// Immutable URL to the rendered DPA artefact.
    pub content_url: String,
}

/// Per-tenant DPA acceptance state mirrored on the `tenants` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantDpaState {
    /// Tenant identifier.
    pub tenant_id: Uuid,
    /// Last accepted version.
    pub current_dpa_version: SemverVersion,
    /// When set, a Major bump has been published and this tenant has
    /// not yet re-accepted; deadline at which the tenant transitions
    /// to read-only.
    pub grace_expires_at: Option<UnixSeconds>,
    /// True iff a re-acceptance is awaited (a Major bump has been
    /// broadcast and this tenant has not yet caught up).
    pub re_acceptance_pending: bool,
}

impl TenantDpaState {
    /// Construct a freshly-onboarded tenant with no pending bump.
    #[must_use]
    pub fn fresh(tenant_id: Uuid, accepted_version: SemverVersion) -> Self {
        Self {
            tenant_id,
            current_dpa_version: accepted_version,
            grace_expires_at: None,
            re_acceptance_pending: false,
        }
    }

    /// Is this tenant past its grace deadline at `now`?
    ///
    /// Returns `false` if `grace_expires_at` is `None` (no pending
    /// bump) — invariant: only tenants with a pending re-acceptance
    /// can ever be `grace_expired`.
    #[must_use]
    pub fn is_grace_expired(&self, now: UnixSeconds) -> bool {
        match self.grace_expires_at {
            Some(deadline) => self.re_acceptance_pending && now >= deadline,
            None => false,
        }
    }

    /// Days remaining in the grace period (saturating at 0).
    #[must_use]
    pub fn grace_remaining_days(&self, now: UnixSeconds) -> Option<i64> {
        let deadline = self.grace_expires_at?;
        let secs = deadline.saturating_sub(now);
        let days = secs / (24 * 60 * 60);
        Some(days.max(0))
    }
}

/// Receipt returned by a successful re-acceptance.
///
/// Mirrors the WI-S19-002 click-through receipt shape so callers can
/// reuse the same JWT carrier downstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReAcceptanceReceipt {
    /// Tenant that accepted.
    pub tenant_id: Uuid,
    /// Previous accepted version.
    pub from_version: SemverVersion,
    /// New accepted version (== latest published).
    pub to_version: SemverVersion,
    /// UTC second at which the acceptance was recorded.
    pub accepted_at: UnixSeconds,
    /// SHA-256 of the content the customer saw at acceptance time
    /// (replay-attack defence: re-derived server-side).
    pub content_hash: String,
}

/// Receipt returned by [`crate::versioning::DpaVersioning::on_dpa_version_bumped`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BumpReceipt {
    /// New version that was published.
    pub new_version: SemverVersion,
    /// Number of tenants flagged for re-acceptance (0 unless Major).
    pub tenants_flagged: u64,
    /// Whether broadcast was triggered.
    pub broadcast_dispatched: bool,
}

/// Receipt returned by the daily cron pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraceCheckReceipt {
    /// Tenants newly transitioned to read-only.
    pub newly_degraded: u64,
    /// Tenants reminded (within `REMINDER_WINDOW_SECONDS`).
    pub reminded: u64,
    /// Tenants still in grace (no action this pass).
    pub still_in_grace: u64,
}
