//! Collusion-rotation 3-cycle defense (NIST SP 800-53 AC-2(7)).
//!
//! # Algorithm (Lote 10.13 canonical strengthening)
//!
//! Query `admin_op_log` for the **last 2 distinct** `approver_user_id`
//! values for destructive ops in the tenant 24h window:
//!
//! ```sql
//! SELECT DISTINCT approver_user_id
//! FROM admin_op_log
//! WHERE tenant_id = $tenant
//!   AND op_type IN ('ConfigRollback','RetentionPolicyReduce',
//!                   'FeatureFlagDisable','SecretRotationStart','TenantTombstone')
//!   AND outcome = 'approved'
//!   AND created_at_ms > $now_ms - 86_400_000
//! ORDER BY MAX(created_at_ms) DESC
//! LIMIT 2
//! ```
//!
//! If `proposed_approver` is in that set → `CollusionRotation` (403).
//!
//! This forces 3 distinct approvers in any rolling window of 3 destructive
//! ops — equivalent to detecting A→B/B→A/A→B at the 3rd op rather than
//! the 4th (the prior LIMIT 3 + count-distinct-< 3 oracle).
//!
//! # In-memory simulation
//!
//! The production implementation queries D1 via Worker binding
//! (WI-S13-002 §6.2). Here we ship an `InMemoryCollusionStore` that
//! faithfully exercises the algorithm for CI + property tests.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::error::DualApprovalError;
use crate::types::AdminOpType;

/// Maximum history window in milliseconds (24 hours).
const WINDOW_MS: u64 = 86_400_000;

/// Number of distinct recent approvers tracked per (tenant, 24h window).
const RECENT_LIMIT: usize = 2;

/// A single log entry used by the in-memory store.
#[derive(Debug, Clone)]
struct LogEntry {
    approver_user_id: Uuid,
    created_at_ms: u64,
}

/// Per-tenant deque of recent approved destructive-op entries (for simulation).
type TenantLog = VecDeque<LogEntry>;

/// In-memory collusion-rotation store (production: D1 `admin_op_log` query).
///
/// Thread-safe via `Arc<Mutex<>>` (F-001 closure).
#[derive(Debug, Clone)]
pub struct InMemoryCollusionStore {
    // tenant_id → list of approved destructive ops in chronological order
    inner: Arc<Mutex<HashMap<Uuid, TenantLog>>>,
}

impl InMemoryCollusionStore {
    /// Construct a fresh empty store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return the last ≤ `RECENT_LIMIT` distinct approver UUIDs for
    /// destructive ops in the tenant 24h window, ordered by recency
    /// (most-recent first).
    ///
    /// This mirrors the canonical D1 oracle:
    /// `SELECT DISTINCT approver_user_id … ORDER BY MAX(ts) DESC LIMIT 2`.
    pub fn recent_approvers(
        &self,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<Vec<Uuid>, DualApprovalError> {
        let guard = self
            .inner
            .lock()
            .map_err(|e| DualApprovalError::Internal(e.to_string()))?;

        let log = match guard.get(&tenant_id) {
            None => return Ok(Vec::new()),
            Some(l) => l,
        };

        let window_start = now_ms.saturating_sub(WINDOW_MS);

        // Walk from newest to oldest; collect distinct approvers up to RECENT_LIMIT.
        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut result: Vec<Uuid> = Vec::new();
        for entry in log.iter().rev() {
            if entry.created_at_ms < window_start {
                break;
            }
            if seen.insert(entry.approver_user_id) {
                result.push(entry.approver_user_id);
                if result.len() >= RECENT_LIMIT {
                    break;
                }
            }
        }
        Ok(result)
    }

    /// Check whether the proposed approver is in the recent-approver set.
    ///
    /// Returns `Ok(())` if no violation; `Err(CollusionRotation)` if found.
    pub fn check_collusion(
        &self,
        tenant_id: Uuid,
        proposed_approver: Uuid,
        now_ms: u64,
    ) -> Result<(), DualApprovalError> {
        let recent = self.recent_approvers(tenant_id, now_ms)?;
        if recent.contains(&proposed_approver) {
            Err(DualApprovalError::CollusionRotation {
                recent_approver_uuids: recent,
            })
        } else {
            Ok(())
        }
    }

    /// Record a newly-approved destructive op (called after a successful
    /// dual-approval verify; production: included in D1 atomic batch).
    pub fn record_approval(
        &self,
        tenant_id: Uuid,
        approver_user_id: Uuid,
        op_type: &AdminOpType,
        created_at_ms: u64,
    ) -> Result<(), DualApprovalError> {
        if !op_type.is_destructive() {
            return Ok(());
        }
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| DualApprovalError::Internal(e.to_string()))?;
        let log = guard.entry(tenant_id).or_insert_with(VecDeque::new);
        log.push_back(LogEntry {
            approver_user_id,
            created_at_ms,
        });
        // Trim old entries beyond window (keep bounded memory).
        let window_start = created_at_ms.saturating_sub(WINDOW_MS);
        while log.front().is_some_and(|e| e.created_at_ms < window_start) {
            log.pop_front();
        }
        Ok(())
    }
}

impl Default for InMemoryCollusionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn empty_store_no_collusion() {
        let store = InMemoryCollusionStore::new();
        let tenant = Uuid::now_v7();
        let approver = Uuid::now_v7();
        store.check_collusion(tenant, approver, 1_000_000).unwrap();
    }

    #[test]
    fn single_prior_no_collusion_for_new_approver() {
        let store = InMemoryCollusionStore::new();
        let tenant = Uuid::now_v7();
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        store
            .record_approval(tenant, a, &AdminOpType::ConfigRollback, 1_000)
            .unwrap();
        store.check_collusion(tenant, b, 10_000).unwrap();
    }

    #[test]
    fn collusion_a_b_a_detected_on_third_op() {
        // A approves B's op, B approves A's op; now A tries to approve → rejected.
        let store = InMemoryCollusionStore::new();
        let tenant = Uuid::now_v7();
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();

        // Op 1: caller=B, approver=A → record A as approver
        store
            .record_approval(tenant, a, &AdminOpType::TenantTombstone, 1_000)
            .unwrap();
        // Op 2: caller=A, approver=B → record B as approver
        store
            .record_approval(tenant, b, &AdminOpType::TenantTombstone, 2_000)
            .unwrap();

        // Op 3: caller=B, proposes approver=A → should REJECT (A ∈ {A, B})
        let err = store.check_collusion(tenant, a, 5_000).unwrap_err();
        assert!(matches!(err, DualApprovalError::CollusionRotation { .. }));
    }

    #[test]
    fn three_distinct_approvers_passes() {
        // A→B, B→C, then D tries with C as approver — C ∈ {B, C}, so rejected.
        // But if E is approver (not in {B, C}), passes.
        let store = InMemoryCollusionStore::new();
        let tenant = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();
        let e = Uuid::now_v7();

        store
            .record_approval(tenant, b, &AdminOpType::ConfigRollback, 1_000)
            .unwrap();
        store
            .record_approval(tenant, c, &AdminOpType::ConfigRollback, 2_000)
            .unwrap();

        // Recent = [c, b] (most recent first). E ∉ {c, b} → ok.
        store.check_collusion(tenant, e, 5_000).unwrap();
    }

    #[test]
    fn expired_entries_not_counted() {
        let store = InMemoryCollusionStore::new();
        let tenant = Uuid::now_v7();
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();

        // Record A as approver 25h ago (outside 24h window).
        let now_ms = 100_000_000u64;
        let old_ts = now_ms - (25 * 3_600_000); // 25h ago
        store
            .record_approval(tenant, a, &AdminOpType::ConfigRollback, old_ts)
            .unwrap();

        // B tries to approve at now; A is expired → recent = [] → ok.
        store.check_collusion(tenant, b, now_ms).unwrap();
    }
}
