//! Edge policy orchestrator — the pure-logic core of the per-IP edge
//! enforcement layer (camada 2 of the 4-layer rate-limit bulkhead
//! PAT-RATE-LIMIT-001).
//!
//! ## Decision pipeline (per request)
//!
//! For each pre-auth request `(tenant_id, ip, now_ms)`:
//!
//! 1. **CIDR blocklist scan** under per-instance Mutex: longest-prefix-
//!    match against the per-tenant prefix index. If a match exists,
//!    return [`EdgeDecision::DenyBlocklisted`] with the matched
//!    prefix.
//! 2. **Default action**: if no blocklist entry matched, fall back to
//!    [`EdgeConfig::default_action`] (canonical = Allow; lockdown
//!    mode toggles to Deny).
//! 3. **Audit emit** of the per-decision record (`Allowed` /
//!    `DeniedBlocklist` / `DeniedAbuse`). Audit failure surfaces as
//!    [`EdgeError::Audit`] (handler maps to 503).
//! 4. **Metrics emit** (`decision_total{result=…}`).
//!
//! For each admin mutation `(tenant_id, cidr, admin_id, now_ms)`:
//!
//! 1. **Overlap detection** — admin add of `192.0.2.0/24` over an
//!    existing `192.0.2.0/16` (or vice versa) is REJECTED with
//!    [`EdgeError::CidrOverlap`] per WI §6.1.7 (admin intent
//!    ambiguous; admin MUST remove the broader entry first).
//! 2. **Capacity ceiling** — the in-memory blocklist enforces the
//!    same hard ceiling as the production CF List (WI §6.1.6;
//!    canonical 10000 max items).
//! 3. **Audit emit BEFORE state mutation** (fail-closed envelope per
//!    Lote 10.6bis pattern + S-07 sprint-close P1-1 fix): emit the
//!    canonical `BlocklistAdded` / `BlocklistRemoved` record. Audit
//!    failure ROLLS BACK the operation (returns `Err(Audit)`;
//!    production wiring rolls back the D1 batch).
//! 4. **Commit state** to per-instance `HashMap`.
//! 5. **Metrics emit** (`cidr_blocklist_added_total{reason}` /
//!    `cidr_blocklist_removed_total` + `cidr_blocklist_size{family}`
//!    gauge re-sampled).
//!
//! ## F-001 closure
//!
//! The orchestrator state is held in a per-instance
//! `Arc<Mutex<HashMap<Uuid, TenantBlocklist>>>` (NOT a process-global
//! `static LazyLock<Mutex<>>`). Tests instantiate fresh orchestrators
//! per case so the harness cannot accidentally leak state across
//! cases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::audit::{
    EdgeAuditRecord, EdgeAuditSink, EdgeEventType,
};
use crate::cidr::{longest_match, overlaps, Cidr, CidrFamily, IpAddr};
use crate::config::{EdgeConfig, EdgeDefaultAction};
use crate::error::EdgeError;
use crate::metrics::{
    EdgeMetricsObserver, EdgeResultLabel,
};

/// Wall-clock source the orchestrator consults for the `now_ms`
/// audit + metrics timestamp. Production wiring consults a CF Worker
/// `Date::now()` shim; tests substitute a deterministic stub.
pub trait EdgeClock: Send + Sync + core::fmt::Debug {
    /// Current wall-clock instant (Unix epoch milliseconds).
    fn now_ms(&self) -> u64;
}

/// `std::time::SystemTime`-backed clock; saturates on backward time
/// travel.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemEdgeClock;

impl SystemEdgeClock {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl EdgeClock for SystemEdgeClock {
    fn now_ms(&self) -> u64 {
        match std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
        {
            Ok(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
            Err(_) => 0,
        }
    }
}

/// The matched CIDR prefix surfaced on the deny-blocklist arm. Carries
/// the full canonical [`Cidr`] (so the handler can render the appeal
/// URL message + log the entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchedPrefix {
    /// Canonical CIDR that matched the request IP.
    pub cidr: Cidr,
}

/// Per-request decision rendered by [`EdgePolicy::evaluate`]. The
/// 3-arm taxonomy mirrors the canonical [`EdgeResultLabel`] +
/// [`EdgeEventType`] vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeDecision {
    /// Request admitted — IP not on the blocklist + no abuse signal.
    Allow,
    /// Request denied — IP fell under a blocklist entry (longest-prefix
    /// match resolved to `matched_prefix`).
    DenyBlocklisted {
        /// The most-specific entry that matched.
        matched_prefix: MatchedPrefix,
    },
    /// Request denied — runtime abuse signal independent of the
    /// blocklist (e.g. WI-S08-004 sustained-abuse cascade). Reserved
    /// for future composition; the canonical fail-closed taxonomy
    /// already pins this arm so callers can pattern-match
    /// exhaustively today.
    DenyAbuse {
        /// Free-form taxonomy reason — opaque to THIS crate (WI-S08-004
        /// drives the canonical literal set).
        reason: &'static str,
    },
}

impl EdgeDecision {
    /// Whether THIS decision admits the request.
    #[must_use]
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Whether THIS decision rejects the request.
    #[must_use]
    pub const fn is_deny(&self) -> bool {
        matches!(self, Self::DenyBlocklisted { .. } | Self::DenyAbuse { .. })
    }

    /// Result label corresponding to THIS arm (for metric emit).
    #[must_use]
    pub const fn result_label(&self) -> EdgeResultLabel {
        match self {
            Self::Allow => EdgeResultLabel::Allowed,
            Self::DenyBlocklisted { .. } => EdgeResultLabel::DeniedBlocklist,
            Self::DenyAbuse { .. } => EdgeResultLabel::DeniedAbuse,
        }
    }

    /// Canonical audit event type corresponding to THIS arm.
    #[must_use]
    pub const fn audit_event_type(&self) -> EdgeEventType {
        match self {
            Self::Allow => EdgeEventType::Allowed,
            Self::DenyBlocklisted { .. } => EdgeEventType::DeniedBlocklist,
            Self::DenyAbuse { .. } => EdgeEventType::DeniedAbuse,
        }
    }
}

/// Trait surfaced by every CIDR blocklist backend (production CF List
/// replica + D1 source-of-truth / in-memory fake). Mutations are
/// audit-fail-closed: the audit record is emitted BEFORE the state
/// changes; emit failure aborts the mutation.
pub trait CidrBlocklist: Send + Sync + core::fmt::Debug {
    /// Resolve the longest-prefix-match entry covering `ip` for the
    /// given tenant. Returns `Ok(None)` when no entry matches.
    ///
    /// # Errors
    ///
    /// Surface as [`EdgeError`] on backend failure (mutex poisoning /
    /// D1 unavailable / etc.).
    fn is_blocklisted(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
    ) -> Result<Option<MatchedPrefix>, EdgeError>;

    /// Add a CIDR to the per-tenant blocklist. The fail-closed
    /// envelope MUST be enforced by the implementation:
    ///
    /// 1. Validate (overlap / capacity).
    /// 2. Emit audit (record carries the CIDR + admin_id + reason).
    /// 3. Mutate state (only AFTER the audit lands).
    /// 4. Emit metrics.
    ///
    /// `reason` is a free-form canonical literal (`Sustained4xx` /
    /// `DDoSPattern` / `ManualAdmin` / `Compliance` per WI §6.1
    /// `BlockReason` enum); validated at the admin endpoint
    /// boundary, NOT here.
    ///
    /// # Errors
    ///
    /// - [`EdgeError::CidrOverlap`] if `cidr` overlaps an existing
    ///   entry.
    /// - [`EdgeError::CapacityReached`] when the blocklist is at the
    ///   `max_size` ceiling.
    /// - [`EdgeError::Audit`] when the audit emit fails.
    /// - [`EdgeError::Metrics`] when the metrics emit fails (note:
    ///   the state mutation already landed; production wiring may
    ///   downgrade to log-and-continue at the boundary).
    /// - [`EdgeError::Backend`] on mutex poisoning.
    fn add_prefix(
        &self,
        tenant_id: Uuid,
        cidr: Cidr,
        reason: &str,
        admin_id: Uuid,
        request_id: &str,
        now_ms: u64,
    ) -> Result<(), EdgeError>;

    /// Remove a CIDR from the per-tenant blocklist. Idempotent: the
    /// remove of a non-existent entry is a no-op (no audit emit).
    ///
    /// # Errors
    ///
    /// - [`EdgeError::Audit`] when the audit emit fails (the entry
    ///   was found but the emit fired BEFORE the remove; the entry
    ///   stays put).
    /// - [`EdgeError::Metrics`] when the metrics emit fails.
    /// - [`EdgeError::Backend`] on mutex poisoning.
    fn remove_prefix(
        &self,
        tenant_id: Uuid,
        cidr: Cidr,
        admin_id: Uuid,
        request_id: &str,
        now_ms: u64,
    ) -> Result<bool, EdgeError>;

    /// Snapshot the per-tenant entries (for diagnostic / cold-start
    /// reload tests). Order is unspecified.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Backend`] on mutex poisoning.
    fn snapshot(&self, tenant_id: Uuid) -> Result<Vec<Cidr>, EdgeError>;

    /// Cardinality of the per-tenant blocklist (for capacity
    /// assertions).
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Backend`] on mutex poisoning.
    fn len(&self, tenant_id: Uuid) -> Result<usize, EdgeError>;

    /// Whether the per-tenant blocklist is empty.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Backend`] on mutex poisoning.
    fn is_empty(&self, tenant_id: Uuid) -> Result<bool, EdgeError> {
        Ok(self.len(tenant_id)? == 0)
    }
}

/// Trait surfaced by every edge enforcement backend (production CF
/// Ruleset Engine + reconcile DO / in-memory fake). The orchestrator
/// composes [`CidrBlocklist`] + audit + metrics + clock into a single
/// per-request decision call.
pub trait EdgePolicy: Send + Sync + core::fmt::Debug {
    /// Render the per-request edge decision. Audit + metrics emit
    /// internally; caller maps [`EdgeDecision`] to the canonical
    /// response shape (`Allow` → continue; `DenyBlocklisted` →
    /// 429 + `X-Rate-Limit-Type: per_ip`; `DenyAbuse` → 429 +
    /// `X-Rate-Limit-Type: abuse`).
    ///
    /// # Errors
    ///
    /// Surface as [`EdgeError`] on backend / audit / metrics failure.
    fn evaluate(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
        request_id: &str,
    ) -> Result<EdgeDecision, EdgeError>;

    /// Force the orchestrator to evaluate the abuse arm (for
    /// composition with the WI-S08-004 sustained-abuse detector).
    /// Production wiring consults the abuse score per request; this
    /// surface lets tests inject a deny-abuse decision deterministically.
    ///
    /// # Errors
    ///
    /// Surface as [`EdgeError`] on backend / audit / metrics failure.
    fn evaluate_with_abuse_signal(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
        request_id: &str,
        abuse_reason: Option<&'static str>,
    ) -> Result<EdgeDecision, EdgeError>;
}

#[derive(Debug, Default)]
struct TenantBlocklist {
    entries: Vec<Cidr>,
}

impl TenantBlocklist {
    fn count_family(&self, family: CidrFamily) -> u64 {
        u64::try_from(self.entries.iter().filter(|c| c.family() == family).count())
            .unwrap_or(u64::MAX)
    }
}

/// In-memory CIDR blocklist implementation. F-001 closure preserved
/// (per-instance `Arc<Mutex<HashMap<Uuid, TenantBlocklist>>>`).
pub struct InMemoryCidrBlocklist<A, M>
where
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
{
    audit: Arc<A>,
    metrics: Arc<M>,
    config: EdgeConfig,
    state: Arc<Mutex<HashMap<Uuid, TenantBlocklist>>>,
}

impl<A, M> core::fmt::Debug for InMemoryCidrBlocklist<A, M>
where
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryCidrBlocklist")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<A, M> InMemoryCidrBlocklist<A, M>
where
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
{
    /// Construct with the canonical default config.
    #[must_use]
    pub fn with_defaults(audit: Arc<A>, metrics: Arc<M>) -> Self {
        Self::new(audit, metrics, EdgeConfig::canonical())
    }

    /// Construct with an explicit config.
    #[must_use]
    pub fn new(audit: Arc<A>, metrics: Arc<M>, config: EdgeConfig) -> Self {
        Self {
            audit,
            metrics,
            config,
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub const fn config(&self) -> &EdgeConfig {
        &self.config
    }

    fn record_size_gauges(
        &self,
        bucket: &TenantBlocklist,
    ) -> Result<(), EdgeError> {
        self.metrics
            .observe_blocklist_size(CidrFamily::V4, bucket.count_family(CidrFamily::V4))?;
        self.metrics
            .observe_blocklist_size(CidrFamily::V6, bucket.count_family(CidrFamily::V6))?;
        Ok(())
    }
}

impl<A, M> CidrBlocklist for InMemoryCidrBlocklist<A, M>
where
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
{
    fn is_blocklisted(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
    ) -> Result<Option<MatchedPrefix>, EdgeError> {
        let guard = self
            .state
            .lock()
            .map_err(|_| EdgeError::Backend("blocklist state mutex poisoned".to_string()))?;
        let Some(bucket) = guard.get(&tenant_id) else {
            return Ok(None);
        };
        Ok(longest_match(&bucket.entries, ip).map(|c| MatchedPrefix { cidr: *c }))
    }

    fn add_prefix(
        &self,
        tenant_id: Uuid,
        cidr: Cidr,
        reason: &str,
        admin_id: Uuid,
        request_id: &str,
        now_ms: u64,
    ) -> Result<(), EdgeError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| EdgeError::Backend("blocklist state mutex poisoned".to_string()))?;
        let bucket = guard.entry(tenant_id).or_default();

        // Idempotent add: existing exact-match entry is no-op.
        if bucket.entries.contains(&cidr) {
            return Ok(());
        }

        // Capacity ceiling — enforce the same CF List 10000 cap as
        // production (WI §6.1.6).
        if bucket.entries.len() >= self.config.max_blocklist_size() {
            return Err(EdgeError::CapacityReached {
                current: bucket.entries.len(),
                max: self.config.max_blocklist_size(),
            });
        }

        // Overlap rejection (WI §6.1.7): admin intent ambiguous when
        // the new entry would either contain or be contained by an
        // existing entry.
        if let Some(existing) = bucket.entries.iter().find(|e| overlaps(e, &cidr)) {
            return Err(EdgeError::CidrOverlap {
                existing: existing.to_canonical_text(),
            });
        }

        // Audit emit BEFORE state mutation (fail-closed envelope).
        self.audit.emit(EdgeAuditRecord {
            event_type: EdgeEventType::BlocklistAdded,
            cidr: Some(cidr),
            created_by_request_id: request_id.to_string(),
            admin_id: Some(admin_id),
            now_ms,
        })?;

        // Commit.
        bucket.entries.push(cidr);

        // Metrics emit (after audit). Drop the lock before the
        // observer call so a slow metric backend doesn't extend the
        // critical section.
        let snapshot = TenantBlocklist {
            entries: bucket.entries.clone(),
        };
        drop(guard);

        self.metrics.record_blocklist_add(reason)?;
        self.record_size_gauges(&snapshot)?;
        Ok(())
    }

    fn remove_prefix(
        &self,
        tenant_id: Uuid,
        cidr: Cidr,
        admin_id: Uuid,
        request_id: &str,
        now_ms: u64,
    ) -> Result<bool, EdgeError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| EdgeError::Backend("blocklist state mutex poisoned".to_string()))?;
        let Some(bucket) = guard.get_mut(&tenant_id) else {
            return Ok(false);
        };
        let Some(idx) = bucket.entries.iter().position(|e| *e == cidr) else {
            return Ok(false);
        };

        // Audit emit BEFORE state mutation (fail-closed envelope).
        self.audit.emit(EdgeAuditRecord {
            event_type: EdgeEventType::BlocklistRemoved,
            cidr: Some(cidr),
            created_by_request_id: request_id.to_string(),
            admin_id: Some(admin_id),
            now_ms,
        })?;

        bucket.entries.remove(idx);

        let snapshot = TenantBlocklist {
            entries: bucket.entries.clone(),
        };
        drop(guard);

        self.metrics.record_blocklist_remove()?;
        self.record_size_gauges(&snapshot)?;
        Ok(true)
    }

    fn snapshot(&self, tenant_id: Uuid) -> Result<Vec<Cidr>, EdgeError> {
        let guard = self
            .state
            .lock()
            .map_err(|_| EdgeError::Backend("blocklist state mutex poisoned".to_string()))?;
        Ok(guard
            .get(&tenant_id)
            .map(|b| b.entries.clone())
            .unwrap_or_default())
    }

    fn len(&self, tenant_id: Uuid) -> Result<usize, EdgeError> {
        let guard = self
            .state
            .lock()
            .map_err(|_| EdgeError::Backend("blocklist state mutex poisoned".to_string()))?;
        Ok(guard.get(&tenant_id).map(|b| b.entries.len()).unwrap_or(0))
    }
}

/// In-memory edge policy orchestrator wired to a [`CidrBlocklist`] +
/// [`EdgeAuditSink`] + [`EdgeMetricsObserver`] + [`EdgeClock`].
pub struct InMemoryEdgePolicy<B, A, M, C>
where
    B: CidrBlocklist,
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
    C: EdgeClock,
{
    blocklist: Arc<B>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<C>,
    config: EdgeConfig,
}

impl<B, A, M, C> core::fmt::Debug for InMemoryEdgePolicy<B, A, M, C>
where
    B: CidrBlocklist,
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
    C: EdgeClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryEdgePolicy")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<B, A, M, C> InMemoryEdgePolicy<B, A, M, C>
where
    B: CidrBlocklist,
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
    C: EdgeClock,
{
    /// Construct with the canonical default config.
    #[must_use]
    pub fn with_defaults(
        blocklist: Arc<B>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<C>,
    ) -> Self {
        Self::new(blocklist, audit, metrics, clock, EdgeConfig::canonical())
    }

    /// Construct with an explicit config.
    #[must_use]
    pub fn new(
        blocklist: Arc<B>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<C>,
        config: EdgeConfig,
    ) -> Self {
        Self {
            blocklist,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// Snapshot the current config.
    #[must_use]
    pub const fn config(&self) -> &EdgeConfig {
        &self.config
    }

    fn dispatch(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
        request_id: &str,
        abuse_reason: Option<&'static str>,
    ) -> Result<EdgeDecision, EdgeError> {
        let now_ms = self.clock.now_ms();

        let decision = if let Some(reason) = abuse_reason {
            EdgeDecision::DenyAbuse { reason }
        } else {
            match self.blocklist.is_blocklisted(tenant_id, ip)? {
                Some(matched) => EdgeDecision::DenyBlocklisted {
                    matched_prefix: matched,
                },
                None => match self.config.default_action() {
                    EdgeDefaultAction::Allow => EdgeDecision::Allow,
                    EdgeDefaultAction::Deny => EdgeDecision::DenyAbuse {
                        reason: "lockdown_default_deny",
                    },
                },
            }
        };

        // Audit emit BEFORE returning to the caller (fail-closed
        // envelope; emit failure surfaces 503 — the audit gap MUST NOT
        // leak as a 200 to the client).
        let cidr = match decision {
            EdgeDecision::DenyBlocklisted { matched_prefix } => {
                Some(matched_prefix.cidr)
            }
            EdgeDecision::Allow | EdgeDecision::DenyAbuse { .. } => None,
        };
        self.audit.emit(EdgeAuditRecord {
            event_type: decision.audit_event_type(),
            cidr,
            created_by_request_id: request_id.to_string(),
            admin_id: None,
            now_ms,
        })?;

        // Metrics emit (after audit lands).
        self.metrics.record_decision(decision.result_label())?;

        Ok(decision)
    }
}

impl<B, A, M, C> EdgePolicy for InMemoryEdgePolicy<B, A, M, C>
where
    B: CidrBlocklist,
    A: EdgeAuditSink,
    M: EdgeMetricsObserver,
    C: EdgeClock,
{
    fn evaluate(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
        request_id: &str,
    ) -> Result<EdgeDecision, EdgeError> {
        self.dispatch(tenant_id, ip, request_id, None)
    }

    fn evaluate_with_abuse_signal(
        &self,
        tenant_id: Uuid,
        ip: IpAddr,
        request_id: &str,
        abuse_reason: Option<&'static str>,
    ) -> Result<EdgeDecision, EdgeError> {
        self.dispatch(tenant_id, ip, request_id, abuse_reason)
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
    use crate::audit::{FailingEdgeAuditSink, InMemoryEdgeAuditSink};
    use crate::metrics::{EdgeMetricKind, InMemoryEdgeMetrics};

    #[derive(Clone, Copy, Debug)]
    struct FixedClock(u64);

    impl EdgeClock for FixedClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn admin() -> Uuid {
        Uuid::from_u128(0xad)
    }

    type Bl = InMemoryCidrBlocklist<InMemoryEdgeAuditSink, InMemoryEdgeMetrics>;
    type Pol = InMemoryEdgePolicy<Bl, InMemoryEdgeAuditSink, InMemoryEdgeMetrics, FixedClock>;

    fn fresh() -> (
        Arc<Bl>,
        Arc<InMemoryEdgeAuditSink>,
        Arc<InMemoryEdgeMetrics>,
        Pol,
    ) {
        let audit = Arc::new(InMemoryEdgeAuditSink::new());
        let metrics = Arc::new(InMemoryEdgeMetrics::new());
        let bl = Arc::new(InMemoryCidrBlocklist::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
        ));
        let clock = Arc::new(FixedClock(1_000));
        let pol = InMemoryEdgePolicy::with_defaults(
            Arc::clone(&bl),
            Arc::clone(&audit),
            Arc::clone(&metrics),
            clock,
        );
        (bl, audit, metrics, pol)
    }

    #[test]
    fn empty_blocklist_admits_every_ip() {
        let (_bl, _a, metrics, pol) = fresh();
        let ip = IpAddr::parse("203.0.113.5").unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        assert!(d.is_allow());
        assert_eq!(metrics.counter_total(EdgeMetricKind::DecisionTotal), 1);
    }

    #[test]
    fn blocklisted_ip_denied_with_matched_prefix() {
        let (bl, audit, _m, pol) = fresh();
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        let ip = IpAddr::parse("203.0.113.5").unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        match d {
            EdgeDecision::DenyBlocklisted { matched_prefix } => {
                assert_eq!(matched_prefix.cidr, cidr);
            }
            _ => panic!("expected DenyBlocklisted"),
        }
        // Audit: 1 BlocklistAdded + 1 DeniedBlocklist.
        assert_eq!(
            audit.snapshot_of(EdgeEventType::BlocklistAdded).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(EdgeEventType::DeniedBlocklist).len(),
            1
        );
    }

    #[test]
    fn longest_prefix_match_wins_on_overlapping_admin_intent() {
        // Admin overlap is rejected up-front, so we exercise
        // longest-prefix via the matcher directly via two
        // non-overlapping families that converge on a common host
        // address.
        let (bl, _a, _m, pol) = fresh();
        // Add a single /24 entry; the matcher MUST resolve any
        // /24-covered IP to that exact entry.
        let net24 = Cidr::parse("203.0.113.0/24").unwrap();
        bl.add_prefix(ten_a(), net24, "ManualAdmin", admin(), "rid", 1).unwrap();
        let ip = IpAddr::parse("203.0.113.99").unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        match d {
            EdgeDecision::DenyBlocklisted { matched_prefix } => {
                assert_eq!(matched_prefix.cidr.prefix_len(), 24);
            }
            _ => panic!("expected DenyBlocklisted /24"),
        }
    }

    #[test]
    fn cidr_overlap_rejected_at_admin_boundary() {
        let (bl, _a, _m, _pol) = fresh();
        let big = Cidr::parse("192.0.0.0/16").unwrap();
        let small = Cidr::parse("192.0.2.0/24").unwrap();
        bl.add_prefix(ten_a(), big, "ManualAdmin", admin(), "rid", 1).unwrap();
        let err = bl
            .add_prefix(ten_a(), small, "ManualAdmin", admin(), "rid", 2)
            .unwrap_err();
        assert!(matches!(err, EdgeError::CidrOverlap { .. }));
    }

    #[test]
    fn add_prefix_idempotent() {
        let (bl, audit, _m, _pol) = fresh();
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 2).unwrap();
        assert_eq!(bl.len(ten_a()).unwrap(), 1);
        // Second add was no-op; only one BlocklistAdded audit emitted.
        assert_eq!(
            audit.snapshot_of(EdgeEventType::BlocklistAdded).len(),
            1
        );
    }

    #[test]
    fn remove_prefix_round_trip() {
        let (bl, audit, _m, pol) = fresh();
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        let removed = bl
            .remove_prefix(ten_a(), cidr, admin(), "rid", 2)
            .unwrap();
        assert!(removed);
        // After remove, subsequent evaluate is Allow.
        let ip = IpAddr::parse("203.0.113.5").unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        assert!(d.is_allow());
        assert_eq!(
            audit.snapshot_of(EdgeEventType::BlocklistRemoved).len(),
            1
        );
    }

    #[test]
    fn remove_nonexistent_is_idempotent_no_audit() {
        let (bl, audit, _m, _pol) = fresh();
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        let removed = bl
            .remove_prefix(ten_a(), cidr, admin(), "rid", 1)
            .unwrap();
        assert!(!removed);
        assert_eq!(
            audit.snapshot_of(EdgeEventType::BlocklistRemoved).len(),
            0
        );
    }

    #[test]
    fn tenant_a_blocklist_does_not_affect_tenant_b() {
        let (bl, _a, _m, pol) = fresh();
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        let ip = IpAddr::parse("203.0.113.5").unwrap();
        let d_a = pol.evaluate(ten_a(), ip, "rid").unwrap();
        let d_b = pol.evaluate(ten_b(), ip, "rid").unwrap();
        assert!(matches!(d_a, EdgeDecision::DenyBlocklisted { .. }));
        assert!(d_b.is_allow());
    }

    #[test]
    fn capacity_ceiling_enforced() {
        let audit = Arc::new(InMemoryEdgeAuditSink::new());
        let metrics = Arc::new(InMemoryEdgeMetrics::new());
        // Tiny config: max 2 entries.
        let cfg = EdgeConfig::with_overrides(EdgeDefaultAction::Allow, 2, 2)
            .unwrap();
        let bl = InMemoryCidrBlocklist::new(
            Arc::clone(&audit),
            Arc::clone(&metrics),
            cfg,
        );
        bl.add_prefix(
            ten_a(),
            Cidr::parse("10.0.0.0/24").unwrap(),
            "ManualAdmin",
            admin(),
            "rid",
            1,
        )
        .unwrap();
        bl.add_prefix(
            ten_a(),
            Cidr::parse("11.0.0.0/24").unwrap(),
            "ManualAdmin",
            admin(),
            "rid",
            2,
        )
        .unwrap();
        let err = bl
            .add_prefix(
                ten_a(),
                Cidr::parse("12.0.0.0/24").unwrap(),
                "ManualAdmin",
                admin(),
                "rid",
                3,
            )
            .unwrap_err();
        assert!(matches!(
            err,
            EdgeError::CapacityReached { current: 2, max: 2 }
        ));
    }

    #[test]
    fn audit_failure_aborts_admin_add_no_state_change() {
        let audit = Arc::new(FailingEdgeAuditSink::new());
        let metrics = Arc::new(InMemoryEdgeMetrics::new());
        let bl = InMemoryCidrBlocklist::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
        );
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        let err = bl
            .add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1)
            .unwrap_err();
        assert!(matches!(err, EdgeError::Audit(_)));
        // Bucket state UNCHANGED — no entries persisted.
        assert_eq!(bl.len(ten_a()).unwrap(), 0);
    }

    #[test]
    fn ipv6_match_supported() {
        let (bl, _a, _m, pol) = fresh();
        let cidr = Cidr::parse("2001:db8::/32").unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        let ip = IpAddr::parse("2001:db8:abcd::1").unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        assert!(matches!(d, EdgeDecision::DenyBlocklisted { .. }));
        // IPv4 ip with same numeric pattern should NOT match.
        let v4 = IpAddr::parse("32.1.13.184").unwrap();
        let d2 = pol.evaluate(ten_a(), v4, "rid").unwrap();
        assert!(d2.is_allow());
    }

    #[test]
    fn lockdown_mode_default_deny() {
        let audit = Arc::new(InMemoryEdgeAuditSink::new());
        let metrics = Arc::new(InMemoryEdgeMetrics::new());
        let bl = Arc::new(InMemoryCidrBlocklist::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
        ));
        let cfg = EdgeConfig::with_overrides(EdgeDefaultAction::Deny, 100, 80)
            .unwrap();
        let clock = Arc::new(FixedClock(1));
        let pol = InMemoryEdgePolicy::new(bl, audit, metrics, clock, cfg);
        let ip = IpAddr::parse("203.0.113.5").unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        assert!(matches!(d, EdgeDecision::DenyAbuse { .. }));
    }

    #[test]
    fn evaluate_with_abuse_signal_short_circuits() {
        let (_bl, _a, metrics, pol) = fresh();
        let ip = IpAddr::parse("203.0.113.5").unwrap();
        let d = pol
            .evaluate_with_abuse_signal(ten_a(), ip, "rid", Some("sustained_4xx"))
            .unwrap();
        assert!(matches!(
            d,
            EdgeDecision::DenyAbuse { reason: "sustained_4xx" }
        ));
        // decision_total{result=denied_abuse} == 1.
        let label = format!(
            "{}{{result=denied_abuse}}",
            EdgeMetricKind::DecisionTotal.as_str()
        );
        assert_eq!(metrics.counter(&label), 1);
    }

    #[test]
    fn separate_blocklist_instances_have_independent_state() {
        let (bl1, _a, _m, _p) = fresh();
        let (bl2, _a2, _m2, _p2) = fresh();
        let cidr = Cidr::parse("203.0.113.0/24").unwrap();
        bl1.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        assert_eq!(bl1.len(ten_a()).unwrap(), 1);
        assert_eq!(bl2.len(ten_a()).unwrap(), 0);
    }
}
