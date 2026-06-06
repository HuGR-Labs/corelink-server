//! Host-side test fake for [`MetaRevocationSink`] modeling the Neon
//! `pat` table + `audit_outbox` INSERT-only log behind a single
//! `Mutex`. Includes the `TestClock` trait + `MonotonicTestClock`
//! default implementation + `TestAuditRow` projection.
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1).

use core::fmt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use tokio::sync::Mutex;

use corelink_pat::{PatId, PrincipalId as PatPrincipalId, TenantId as PatTenantId};

use super::traits::MetaRevocationSink;
use super::types::{
    MassRevokeId, MassRevokeRow, MetaMassRevokeOutcome, MetaRevokeOutcome, RevocationError,
    RevocationReason, SessionCacheKey,
};

// ---------------------------------------------------------------------------
// In-memory MetaRevocationSink fake (Neon SoT model)
// ---------------------------------------------------------------------------

/// Host-side test fake for [`MetaRevocationSink`]. Models the Neon
/// `pat` table + the `audit_outbox` INSERT-only log behind a single
/// `Mutex`. The combined invariant is the load-bearing test surface:
/// `audit_outbox` row count MUST equal the count of
/// `MetaRevokeOutcome::Revoked` returns (no duplicates on retry per
/// INV-AUTH-REVOCATION-IDEMPOTENT).
pub struct InMemoryMetaRevocationSink {
    inner: Mutex<InMemoryMetaState>,
    clock: Arc<dyn TestClock>,
}

#[derive(Default)]
struct InMemoryMetaState {
    /// `pat` rows the sink knows about. The orchestrator-side tests
    /// pre-seed via [`InMemoryMetaRevocationSink::seed_pat`].
    pat_rows: HashMap<PatId, FakePatRow>,
    /// audit_outbox INSERT log. Each row is one
    /// `auth.token.revoked` event.
    audit_outbox: Vec<FakeAuditRow>,
}

#[derive(Clone, Debug)]
struct FakePatRow {
    tenant_id: PatTenantId,
    principal_id: PatPrincipalId,
    revoked_at: Option<SystemTime>,
    /// Canonical session-cache key for this row's plaintext;
    /// pre-seeded by tests so Phase 2 mass-revoke fan-out has the
    /// canonical handle.
    session_cache_key: SessionCacheKey,
}

#[derive(Clone, Debug)]
struct FakeAuditRow {
    /// `pat_id` of the revoked row.
    pub pat_id: PatId,
    /// `tenant_id` of the revoked row.
    pub tenant_id: PatTenantId,
    /// Authoritative `revoked_at`.
    pub revoked_at: SystemTime,
    /// Reason tag.
    pub reason: RevocationReason,
    /// Origin: single-revoke `None`, mass-revoke `Some(parent_id)`.
    pub mass_revoke_id: Option<MassRevokeId>,
}

/// Test-only clock surface. `SystemTime`-monotonic by default; the
/// in-memory sink uses it to stamp `revoked_at` deterministically.
pub trait TestClock: Send + Sync {
    /// Return a monotonically-non-decreasing instant.
    fn now(&self) -> SystemTime;
}

impl fmt::Debug for dyn TestClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TestClock")
    }
}

/// Default monotonic clock for [`InMemoryMetaRevocationSink`]. Seeds
/// from a fixed UNIX epoch and increments by 1 ms per call so two
/// back-to-back revokes deterministically receive distinct
/// timestamps (necessary for property tests asserting timestamp
/// uniqueness across N calls).
#[derive(Debug)]
pub struct MonotonicTestClock {
    next_micros: std::sync::atomic::AtomicU64,
}

impl MonotonicTestClock {
    /// Construct a fresh clock seeded at `1_700_000_000_000_000` µs
    /// past UNIX epoch (a fixed reference instant).
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_micros: std::sync::atomic::AtomicU64::new(1_700_000_000_000_000),
        }
    }
}

impl Default for MonotonicTestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TestClock for MonotonicTestClock {
    fn now(&self) -> SystemTime {
        let micros = self
            .next_micros
            .fetch_add(1_000, std::sync::atomic::Ordering::AcqRel);
        SystemTime::UNIX_EPOCH + Duration::from_micros(micros)
    }
}

impl fmt::Debug for InMemoryMetaRevocationSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryMetaRevocationSink")
            .finish_non_exhaustive()
    }
}

impl InMemoryMetaRevocationSink {
    /// Construct a fresh, empty sink with a monotonic clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(MonotonicTestClock::new()))
    }

    /// Construct with an arbitrary [`TestClock`].
    #[must_use]
    pub fn with_clock(clock: Arc<dyn TestClock>) -> Self {
        Self {
            inner: Mutex::new(InMemoryMetaState::default()),
            clock,
        }
    }

    /// Pre-seed a `pat` row. Tests call this to populate the table
    /// before exercising the orchestrator.
    pub async fn seed_pat(
        &self,
        pat_id: PatId,
        tenant_id: PatTenantId,
        principal_id: PatPrincipalId,
        session_cache_key: SessionCacheKey,
    ) {
        let mut guard = self.inner.lock().await;
        guard.pat_rows.insert(
            pat_id,
            FakePatRow {
                tenant_id,
                principal_id,
                revoked_at: None,
                session_cache_key,
            },
        );
    }

    /// Test-only audit row count.
    pub async fn audit_count(&self) -> usize {
        self.inner.lock().await.audit_outbox.len()
    }

    /// Test-only: count audit rows whose `(pat_id, revoked_at)`
    /// matches.
    pub async fn audit_count_for(&self, pat_id: PatId, revoked_at: SystemTime) -> usize {
        self.inner
            .lock()
            .await
            .audit_outbox
            .iter()
            .filter(|r| r.pat_id == pat_id && r.revoked_at == revoked_at)
            .count()
    }

    /// Test-only audit rows snapshot.
    pub async fn audit_rows(&self) -> Vec<TestAuditRow> {
        self.inner
            .lock()
            .await
            .audit_outbox
            .iter()
            .map(|r| TestAuditRow {
                pat_id: r.pat_id,
                tenant_id: r.tenant_id,
                revoked_at: r.revoked_at,
                reason: r.reason,
                mass_revoke_id: r.mass_revoke_id,
            })
            .collect()
    }
}

/// Public, read-only audit row projection used by tests.
#[derive(Clone, Debug)]
pub struct TestAuditRow {
    /// PAT primary key.
    pub pat_id: PatId,
    /// Tenant binding.
    pub tenant_id: PatTenantId,
    /// Authoritative timestamp.
    pub revoked_at: SystemTime,
    /// Reason tag.
    pub reason: RevocationReason,
    /// Parent mass-revoke id (single revoke = `None`).
    pub mass_revoke_id: Option<MassRevokeId>,
}

impl Default for InMemoryMetaRevocationSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetaRevocationSink for InMemoryMetaRevocationSink {
    async fn revoke(
        &self,
        pat_id: PatId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaRevokeOutcome, RevocationError> {
        let mut guard = self.inner.lock().await;
        let row = guard
            .pat_rows
            .get_mut(&pat_id)
            .ok_or(RevocationError::NotFound)?;
        if let Some(existing) = row.revoked_at {
            // Idempotent replay path. CRITICAL: do NOT insert a new
            // audit row.
            return Ok(MetaRevokeOutcome::AlreadyRevoked {
                revoked_at: existing,
            });
        }
        let revoked_at = self.clock.now();
        row.revoked_at = Some(revoked_at);
        let tenant_id = row.tenant_id;
        let _ = revoked_by;
        guard.audit_outbox.push(FakeAuditRow {
            pat_id,
            tenant_id,
            revoked_at,
            reason,
            mass_revoke_id: None,
        });
        Ok(MetaRevokeOutcome::Revoked { revoked_at })
    }

    async fn mass_revoke(
        &self,
        tenant_id: PatTenantId,
        reason: RevocationReason,
        revoked_by: PatPrincipalId,
    ) -> Result<MetaMassRevokeOutcome, RevocationError> {
        let mut guard = self.inner.lock().await;
        let revoked_at = self.clock.now();
        let _ = (reason, revoked_by);
        let mut newly_revoked = Vec::new();
        for (pat_id, row) in guard.pat_rows.iter_mut() {
            if row.tenant_id == tenant_id && row.revoked_at.is_none() {
                row.revoked_at = Some(revoked_at);
                newly_revoked.push(MassRevokeRow {
                    pat_id: *pat_id,
                    principal_id: row.principal_id,
                    token_hash_key: row.session_cache_key.clone(),
                });
            }
        }
        Ok(MetaMassRevokeOutcome {
            revoked_at,
            newly_revoked,
        })
    }

    async fn insert_outbox_batch(
        &self,
        tenant_id: PatTenantId,
        mass_revoke_id: MassRevokeId,
        revoked_at: SystemTime,
        reason: RevocationReason,
        _revoked_by: PatPrincipalId,
        chunk: &[MassRevokeRow],
    ) -> Result<(), RevocationError> {
        let mut guard = self.inner.lock().await;
        for row in chunk {
            guard.audit_outbox.push(FakeAuditRow {
                pat_id: row.pat_id,
                tenant_id,
                revoked_at,
                reason,
                mass_revoke_id: Some(mass_revoke_id),
            });
        }
        Ok(())
    }
}
