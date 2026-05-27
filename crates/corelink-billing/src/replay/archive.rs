//! R2 NDJSON archive read trait + in-memory fake.
//!
//! Production wiring at WI-S10-007 binds this to the canonical R2
//! Object Lock 7y archive at
//! `usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson` per the
//! WI-S10-001 emit surface; the trait surface here pins the canonical
//! 3-layer reconstruction signature so the orchestrator's deterministic
//! re-derivation arm exercises the same fail-CLOSED envelope discipline
//! against the in-memory fake.
//!
//! ## Why `trait + fake` here, real R2 NDJSON reader at WI-S10-007
//!
//! S-10 lands without Cloudflare R2 staging wired into CI (no remote +
//! Cloudflare Workers + R2 staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! deterministic re-derivation of the canonical 3 layer totals from the
//! raw event sequence; tenant isolation (the archive is keyed by
//! `(tenant_id, billing_period)`; cross-tenant reads are structurally
//! impossible); per-(tenant, billing_period) byte-identical
//! reconstruction across multiple invocations.
//!
//! The live `R2NdjsonReplayArchive` Cloudflare Worker binding (R2
//! `GET` + NDJSON line-by-line parse + canonical `UsageEvent` re-
//! deserialization + per-`UsageEventKind` SUM + canonical aggregator
//! re-application + Stripe usage-record idempotency-key re-derivation)
//! lands alongside WI-S10-007 (PRR ship gate).

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use super::error::ReplayIdempotencyError;
use super::event::ReconstructedLayers;

/// Replay archive read trait. Production wiring composes the canonical
/// R2 NDJSON archive at `usage/{tenant_id}/{billing_period}/*.usage.ndjson`
/// at WI-S10-007 PRR ship gate per the `trait-abstraction-defer`
/// charter pattern.
pub trait ReplayArchive: Send + Sync + core::fmt::Debug {
    /// Read + deterministically re-aggregate the 3-layer totals for
    /// the canonical `(tenant, billing_period)` scope.
    ///
    /// ## Determinism
    ///
    /// Per WI-S10-006 §1 invariant 7 (forensic determinism rationale):
    /// the canonical reconstruction MUST be byte-identical across
    /// invocations against the same `(tenant, billing_period)` scope.
    /// A non-deterministic implementation (e.g. a parallel R2 fetch
    /// that observes events in different orders + a sum reduction
    /// that depends on visit order — even though `u128 + u128` is
    /// associative + commutative, the canonical aggregator surface
    /// has typed event ordering rules: `(time_ms, idem_key)` lex per
    /// WI-S10-002 `deterministic_event_order`).
    ///
    /// # Errors
    ///
    /// Returns [`ReplayIdempotencyError::Backend`] on archive read
    /// failure (the orchestrator surfaces this as
    /// [`super::error::ReplayError::Idempotency`] per the canonical
    /// fail-CLOSED envelope discipline).
    fn read_layers(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<ReconstructedLayers, ReplayIdempotencyError>;
}

/// In-memory replay archive backed by `BTreeMap<(tenant_id,
/// billing_period), ReconstructedLayers>`. Tests pre-seed the map +
/// the orchestrator reads from it deterministically.
#[derive(Clone, Debug, Default)]
pub struct InMemoryReplayArchive {
    inner: std::sync::Arc<Mutex<BTreeMap<(Uuid, String), ReconstructedLayers>>>,
}

impl InMemoryReplayArchive {
    /// Construct a fresh archive.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-seed a `(tenant, billing_period)` row with the canonical
    /// reconstructed-layers snapshot. Used by tests to set up the
    /// archive contents BEFORE the orchestrator reads. Returns the
    /// prior layers if the row was previously populated.
    pub fn seed(
        &self,
        tenant_id: Uuid,
        billing_period: impl Into<String>,
        layers: ReconstructedLayers,
    ) -> Option<ReconstructedLayers> {
        match self.inner.lock() {
            Ok(mut g) => g.insert((tenant_id, billing_period.into()), layers),
            Err(p) => p.into_inner().insert((tenant_id, billing_period.into()), layers),
        }
    }

    /// Number of seeded rows.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the archive is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ReplayArchive for InMemoryReplayArchive {
    fn read_layers(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<ReconstructedLayers, ReplayIdempotencyError> {
        let guard = self.inner.lock().map_err(|_| {
            ReplayIdempotencyError::Backend(
                "billing-replay archive mutex poisoned (read_layers)".to_string(),
            )
        })?;
        // The canonical absent-row policy is "treat as zero-totals":
        // re-running replay against a billing_period that never had
        // any events is a legitimate operation (the auditor wants to
        // verify zero-billed periods are truly zero). Returning a
        // structured error here would force the caller to special-case
        // the empty path; instead the canonical aggregator returns the
        // zero snapshot.
        Ok(guard
            .get(&(tenant_id, billing_period.to_string()))
            .copied()
            .unwrap_or_else(ReconstructedLayers::zero))
    }
}

/// Always-failing archive for adversarial tests of the fail-CLOSED
/// envelope.
#[derive(Debug, Default)]
pub struct FailingReplayArchive;

impl FailingReplayArchive {
    /// Construct a fresh always-failing archive.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ReplayArchive for FailingReplayArchive {
    fn read_layers(
        &self,
        _tenant_id: Uuid,
        _billing_period: &str,
    ) -> Result<ReconstructedLayers, ReplayIdempotencyError> {
        Err(ReplayIdempotencyError::Backend(
            "induced billing-replay archive failure (test fixture)".to_string(),
        ))
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

    #[test]
    fn read_returns_seeded_layers() {
        let a = InMemoryReplayArchive::new();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(100, 100, 100);
        a.seed(t, "2026-05", layers);
        let read = a.read_layers(t, "2026-05").unwrap();
        assert_eq!(read, layers);
    }

    #[test]
    fn read_returns_zero_for_unseeded() {
        let a = InMemoryReplayArchive::new();
        let t = Uuid::now_v7();
        let read = a.read_layers(t, "2026-05").unwrap();
        assert_eq!(read, ReconstructedLayers::zero());
    }

    #[test]
    fn deterministic_re_read_returns_same_value() {
        let a = InMemoryReplayArchive::new();
        let t = Uuid::now_v7();
        a.seed(t, "2026-05", ReconstructedLayers::new(42, 42, 42));
        let r1 = a.read_layers(t, "2026-05").unwrap();
        let r2 = a.read_layers(t, "2026-05").unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn tenant_isolation_pins_per_tenant_reads() {
        let a = InMemoryReplayArchive::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        a.seed(t1, "2026-05", ReconstructedLayers::new(100, 100, 100));
        a.seed(t2, "2026-05", ReconstructedLayers::new(200, 200, 200));
        let r1 = a.read_layers(t1, "2026-05").unwrap();
        let r2 = a.read_layers(t2, "2026-05").unwrap();
        assert_ne!(r1, r2);
        assert_eq!(r1.layer1_emit_total_qty, 100);
        assert_eq!(r2.layer1_emit_total_qty, 200);
    }

    #[test]
    fn cloned_archive_shares_storage() {
        let a1 = InMemoryReplayArchive::new();
        let a2 = a1.clone();
        let t = Uuid::now_v7();
        a1.seed(t, "2026-05", ReconstructedLayers::new(1, 1, 1));
        assert_eq!(a2.len(), 1);
    }

    #[test]
    fn failing_archive_returns_backend_error() {
        let a = FailingReplayArchive::new();
        let err = a.read_layers(Uuid::now_v7(), "2026-05").unwrap_err();
        assert!(matches!(err, ReplayIdempotencyError::Backend(_)));
    }

    #[test]
    fn empty_archive_reports_empty() {
        let a = InMemoryReplayArchive::new();
        assert!(a.is_empty());
        assert_eq!(a.len(), 0);
    }

    #[test]
    fn re_seed_returns_prior_layers() {
        let a = InMemoryReplayArchive::new();
        let t = Uuid::now_v7();
        let prior = a.seed(t, "2026-05", ReconstructedLayers::new(1, 1, 1));
        assert!(prior.is_none());
        let prior = a.seed(t, "2026-05", ReconstructedLayers::new(2, 2, 2));
        assert_eq!(prior, Some(ReconstructedLayers::new(1, 1, 1)));
    }
}
