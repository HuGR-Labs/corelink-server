//! Daily cron pass — scans pending-re-acceptance tenants, sends
//! reminders within the 7-day window, and degrades tenants past
//! `grace_expires_at`.
//!
//! Production wiring (Cloudflare Cron Trigger calling into the CF
//! Worker) substitutes a `Utc::now()` clock for the `now` argument.

use std::sync::Arc;

use super::broadcast::{BroadcastEnvelope, BroadcastKind, BroadcastSink};
use super::error::DpaVersioningError;
use super::schema::{GraceCheckReceipt, UnixSeconds, REMINDER_WINDOW_SECONDS};
use super::store::{DpaStore, InMemoryDpaStore};

/// Daily cron primitive. Stateless w.r.t. the store — every pass
/// recomputes the working set from `list_pending_tenants`.
#[derive(Debug)]
pub struct GraceExpirationCron {
    store: Arc<InMemoryDpaStore>,
    broadcast: Arc<dyn BroadcastSink>,
}

impl GraceExpirationCron {
    /// Construct a cron over an in-memory store + broadcast sink.
    #[must_use]
    pub fn new(store: Arc<InMemoryDpaStore>, broadcast: Arc<dyn BroadcastSink>) -> Self {
        Self { store, broadcast }
    }

    /// Run a single pass at logical clock `now`.
    ///
    /// Each pending tenant falls into exactly one bucket:
    /// - `grace_expires_at - now <= 0` ⇒ **degrade** (emit
    ///   [`BroadcastKind::DegradeNotice`]; the read-only gate uses the
    ///   already-set `grace_expires_at` to enforce, so no store mutation
    ///   is required here — the tenant is already `is_grace_expired`).
    /// - `grace_expires_at - now <= REMINDER_WINDOW_SECONDS` ⇒
    ///   **remind** (emit [`BroadcastKind::GraceReminder`]).
    /// - otherwise ⇒ **still_in_grace** (no broadcast).
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::Store`] on store failure and
    /// [`DpaVersioningError::Broadcast`] on broadcast sink failure.
    /// Broadcast errors abort the pass at the first failure so the
    /// caller can re-run after recovery — the cron is idempotent on
    /// re-run (same tenant gets the same envelope kind).
    pub fn run(&self, now: UnixSeconds) -> Result<GraceCheckReceipt, DpaVersioningError> {
        let pending = self.store.list_pending_tenants()?;
        let latest = self.store.latest_version()?;

        let mut newly_degraded = 0u64;
        let mut reminded = 0u64;
        let mut still_in_grace = 0u64;

        for tenant in pending {
            let Some(deadline) = tenant.grace_expires_at else {
                // Pending without a deadline — defensive: treat as
                // still_in_grace and skip broadcast.
                still_in_grace = still_in_grace.saturating_add(1);
                continue;
            };
            let remaining = deadline.saturating_sub(now);
            let version = latest
                .as_ref()
                .map_or(tenant.current_dpa_version, |l| l.version);

            if remaining <= 0 {
                self.broadcast.send(BroadcastEnvelope {
                    tenant_id: tenant.tenant_id,
                    version,
                    kind: BroadcastKind::DegradeNotice,
                })?;
                newly_degraded = newly_degraded.saturating_add(1);
            } else if remaining <= REMINDER_WINDOW_SECONDS {
                self.broadcast.send(BroadcastEnvelope {
                    tenant_id: tenant.tenant_id,
                    version,
                    kind: BroadcastKind::GraceReminder,
                })?;
                reminded = reminded.saturating_add(1);
            } else {
                still_in_grace = still_in_grace.saturating_add(1);
            }
        }

        Ok(GraceCheckReceipt {
            newly_degraded,
            reminded,
            still_in_grace,
        })
    }
}
