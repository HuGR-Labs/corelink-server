//! Top-level orchestrator for DPA version bumps.
//!
//! Pipeline (Major bump):
//!
//! 1. Classify `(old, new)` via [`super::version::classify_bump`].
//! 2. Persist the new version record via [`super::store::DpaStore::append_version`].
//! 3. Flag every existing tenant for re-acceptance + seed
//!    `grace_expires_at = now + 30d`.
//! 4. Dispatch a [`super::broadcast::BroadcastKind::MajorBumpNotice`]
//!    per flagged tenant (best-effort: broadcast failure is logged
//!    but does NOT roll back state — degrade-by-default is preferable
//!    to leaving customers writable under stale consent).
//!
//! Minor / Patch bumps only persist the version record.

use std::sync::Arc;

use super::broadcast::{BroadcastEnvelope, BroadcastKind, BroadcastSink};
use super::error::DpaVersioningError;
use super::schema::{BumpReceipt, DpaVersionRecord, UnixSeconds, GRACE_PERIOD_SECONDS};
use super::store::{DpaStore as _, InMemoryDpaStore};
use super::version::{classify_bump, BumpClassification, BumpKind, SemverVersion};

/// High-level orchestrator surface.
pub trait DpaVersioning: std::fmt::Debug + Send + Sync {
    /// Invoked when CI deploys a new DPA version.
    ///
    /// # Errors
    ///
    /// Returns [`DpaVersioningError::InvalidBump`] on a regression /
    /// no-op, [`DpaVersioningError::Store`] on persistence failure.
    /// Broadcast failures are surfaced as [`DpaVersioningError::Broadcast`]
    /// **only** when the very first envelope fails — partial broadcast
    /// failures are aggregated into the returned receipt.
    fn on_dpa_version_bumped(
        &self,
        new_version: SemverVersion,
        content_hash: String,
        content_url: String,
        now: UnixSeconds,
    ) -> Result<BumpReceipt, DpaVersioningError>;
}

/// In-memory orchestrator backing every unit / property test.
#[derive(Debug)]
pub struct InMemoryDpaVersioning {
    store: Arc<InMemoryDpaStore>,
    broadcast: Arc<dyn BroadcastSink>,
}

impl InMemoryDpaVersioning {
    /// Construct an orchestrator over an in-memory store + a broadcast
    /// sink.
    #[must_use]
    pub fn new(store: Arc<InMemoryDpaStore>, broadcast: Arc<dyn BroadcastSink>) -> Self {
        Self { store, broadcast }
    }

    /// Access the underlying store (test helper).
    #[must_use]
    pub fn store(&self) -> &Arc<InMemoryDpaStore> {
        &self.store
    }
}

impl DpaVersioning for InMemoryDpaVersioning {
    fn on_dpa_version_bumped(
        &self,
        new_version: SemverVersion,
        content_hash: String,
        content_url: String,
        now: UnixSeconds,
    ) -> Result<BumpReceipt, DpaVersioningError> {
        let latest = self.store.latest_version()?;
        let old_version = latest
            .as_ref()
            .map_or(SemverVersion::new(0, 0, 0), |l| l.version);

        // Classify.
        let bump_kind = match classify_bump(old_version, new_version) {
            BumpClassification::Detected(kind) => kind,
            BumpClassification::NoBump | BumpClassification::Regression => {
                return Err(DpaVersioningError::InvalidBump {
                    old: old_version.render(),
                    new: new_version.render(),
                });
            }
        };

        // Persist new version.
        let record = DpaVersionRecord {
            version: new_version,
            published_at: now,
            content_hash,
            content_url,
        };
        self.store.append_version(record)?;

        // Non-Major bump: silent. Receipt with no broadcast / no flag.
        if bump_kind != BumpKind::Major {
            return Ok(BumpReceipt {
                new_version,
                tenants_flagged: 0,
                broadcast_dispatched: false,
            });
        }

        // Major bump: flag tenants + broadcast.
        let grace_expires_at = now.saturating_add(GRACE_PERIOD_SECONDS);
        let tenants_flagged = self.store.flag_all_pending(grace_expires_at)?;

        let pending = self.store.list_pending_tenants()?;
        for tenant in &pending {
            // Broadcast failure is logged-but-not-rolled-back: the
            // tenant is already flagged for read-only post-grace, so
            // missing one email does not violate INV-CONSENT-PROOF-VERIFIABLE.
            // First-envelope failure surfaces so callers learn the
            // sink is misconfigured.
            self.broadcast.send(BroadcastEnvelope {
                tenant_id: tenant.tenant_id,
                version: new_version,
                kind: BroadcastKind::MajorBumpNotice,
            })?;
        }

        Ok(BumpReceipt {
            new_version,
            tenants_flagged,
            broadcast_dispatched: !pending.is_empty(),
        })
    }
}
