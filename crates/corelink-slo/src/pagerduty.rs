//! PagerDuty Events API v2 dispatcher trait + InMemory test sink.
//!
//! ## Events API v2 dedup-key idempotency
//!
//! Per PagerDuty Events API v2 §`dedup_key`: when an Event is sent
//! with `event_action: "trigger"` carrying a `dedup_key` already
//! associated with an open incident, the event is collapsed (no new
//! incident opened; no extra page dispatched). The orchestrator
//! ([`crate::alert::MultiBurnRateAlert`]) constructs the dedup key
//! canonically as `{sli_slug}:{window_slug}:{tenant_id}` so the same
//! sustained burn at the same SLI × window × tenant collapses to a
//! single ongoing incident — pinned by
//! `prop_pagerduty_dispatch_idempotent_dedup_key`.
//!
//! ## 3 services per environment
//!
//! Per sprint contract §5.5 R-S09-14 canonical (Lote 10.9bis P0-F
//! corrected from prior 5): staging / prod-us / prod-eu. Each service
//! has its own `routing_key` (PagerDuty Integration Key) the
//! production wiring binds via Worker secret; the InMemory dispatcher
//! routes by [`PagerDutyServiceKey`] for fixture isolation in tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::SloPagerDutyDispatchError;

/// Canonical PagerDuty Events API v2 `event_action` values per
/// `https://developer.pagerduty.com/docs/events-api-v2/`. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (e.g. `change` deferred to S-19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PagerDutyEventAction {
    /// Open a new incident OR collapse to an existing one with the
    /// same `dedup_key`.
    Trigger,
    /// Mark an incident acknowledged (suppresses re-paging on
    /// escalation timer).
    Acknowledge,
    /// Resolve an incident (clears the open state).
    Resolve,
}

impl PagerDutyEventAction {
    /// Canonical wire string per Events API v2.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::Acknowledge => "acknowledge",
            Self::Resolve => "resolve",
        }
    }
}

impl core::fmt::Display for PagerDutyEventAction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element list of Events API v2 `event_action` values
/// used for surface-stability regression tests.
#[must_use]
pub const fn canonical_pagerduty_actions() -> &'static [PagerDutyEventAction; 3] {
    &[
        PagerDutyEventAction::Trigger,
        PagerDutyEventAction::Acknowledge,
        PagerDutyEventAction::Resolve,
    ]
}

/// Canonical PagerDuty service routing key (one per environment per
/// sprint contract §5.5 R-S09-14 canonical; Lote 10.9bis P0-F
/// corrected from prior 5). The `#[non_exhaustive]` marker reserves
/// additive growth for follow-on WIs (e.g. S-14 region residency may
/// add prod-apac forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PagerDutyServiceKey {
    /// `corelink-staging` — staging environment service.
    Staging,
    /// `corelink-prod-us` — production US-East-1 region service.
    ProdUs,
    /// `corelink-prod-eu` — production EU-West-1 region service.
    ProdEu,
}

impl PagerDutyServiceKey {
    /// Canonical service name per sprint contract §5.5 R-S09-14.
    #[must_use]
    pub const fn service_name(self) -> &'static str {
        match self {
            Self::Staging => "corelink-staging",
            Self::ProdUs => "corelink-prod-us",
            Self::ProdEu => "corelink-prod-eu",
        }
    }
}

impl core::fmt::Display for PagerDutyServiceKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.service_name())
    }
}

/// PagerDuty Events API v2 envelope (subset; production wiring
/// serializes to the canonical JSON `routing_key` + `event_action` +
/// `dedup_key` + `payload` shape via a hand-rolled emitter; the
/// in-memory orchestrator captures the typed shape for property
/// testing). The CloudEvents-aligned `corelink-audit-chain::AuditEvent`
/// produces an audit row in parallel to the dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagerDutyEvent {
    /// Target service (production wiring resolves to the bound
    /// Integration Key via Worker secret).
    pub service: PagerDutyServiceKey,
    /// Canonical Events API v2 `event_action` value.
    pub event_action: PagerDutyEventAction,
    /// Dedup key (`{sli_slug}:{window_slug}:{tenant_id}`); same key =
    /// same incident on the PagerDuty side.
    pub dedup_key: String,
    /// Severity slug (`sev0` / `sev1`); sourced from
    /// `AlertDecision::severity_label()`.
    pub severity: &'static str,
    /// Human-readable summary annotation (≤ 1024 chars per Events
    /// API v2; truncated at the boundary).
    pub summary: String,
    /// Source attribution for the dispatch (e.g. `slo-orchestrator`).
    pub source: &'static str,
    /// Runbook URL (deep link to `docs/runbooks/RB-SLO-*.md`).
    pub runbook_url: String,
}

/// PagerDuty dispatcher trait. Production wiring composes:
///
/// - `PagerDutyEventsApiV2HttpsDispatcher` — HTTPS POST to
///   `https://events.pagerduty.com/v2/enqueue` with the bound
///   `routing_key` (per-service Integration Key from Worker secret);
///   202 Accepted on success.
/// - `TwilioFallbackDispatcher` — secondary fallback when PagerDuty
///   API outage (sprint contract §6.1 chaos scenario 4).
pub trait PagerDutyDispatcher: Send + Sync + core::fmt::Debug {
    /// Dispatch `event` to PagerDuty. Returns `Ok(())` on accepted
    /// dispatch. Caller maps a non-`Ok` return to alert fail-OPEN
    /// (alert-of-alert SEV-2 source `corelink_alerts_dispatched_total`
    /// counter increment with `dispatch="fail"` label per WI §6.1.10).
    ///
    /// # Errors
    ///
    /// Returns [`SloPagerDutyDispatchError::Transport`] on transport
    /// failure (HTTPS POST rejected / network timeout / 5xx).
    fn dispatch(&self, event: PagerDutyEvent) -> Result<(), SloPagerDutyDispatchError>;
}

/// In-memory PagerDuty dispatcher honoring Events API v2 dedup-key
/// idempotency. Cloning shares the underlying buffers.
#[derive(Clone, Default, Debug)]
pub struct InMemoryPagerDutyDispatcher {
    events: Arc<Mutex<Vec<PagerDutyEvent>>>,
    open_incidents: Arc<Mutex<HashMap<String, PagerDutyEvent>>>,
}

impl InMemoryPagerDutyDispatcher {
    /// Construct a fresh dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the full audit-trail of dispatch attempts (every
    /// `dispatch` call is recorded, including duplicates collapsed by
    /// dedup-key).
    #[must_use]
    pub fn snapshot_attempts(&self) -> Vec<PagerDutyEvent> {
        match self.events.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Snapshot the currently-open incident set (dedup-key →
    /// PagerDutyEvent map; mirrors PagerDuty incident state).
    #[must_use]
    pub fn snapshot_open_incidents(&self) -> Vec<(String, PagerDutyEvent)> {
        match self.open_incidents.lock() {
            Ok(g) => {
                let mut v: Vec<_> = g.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v
            }
            Err(p) => {
                let g = p.into_inner();
                let mut v: Vec<_> = g.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v
            }
        }
    }

    /// Number of dispatch attempts (including dedup collapses).
    #[must_use]
    pub fn attempt_count(&self) -> usize {
        match self.events.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Number of currently-open incidents (post-dedup).
    #[must_use]
    pub fn open_incident_count(&self) -> usize {
        match self.open_incidents.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }
}

impl PagerDutyDispatcher for InMemoryPagerDutyDispatcher {
    fn dispatch(&self, event: PagerDutyEvent) -> Result<(), SloPagerDutyDispatchError> {
        // Audit-trail-style: every attempt recorded (including those
        // collapsed by dedup_key).
        {
            let mut attempts = self.events.lock().map_err(|_| {
                SloPagerDutyDispatchError::Transport(
                    "pagerduty events buffer mutex poisoned".to_string(),
                )
            })?;
            attempts.push(event.clone());
        }
        let mut open = self.open_incidents.lock().map_err(|_| {
            SloPagerDutyDispatchError::Transport(
                "pagerduty open incidents mutex poisoned".to_string(),
            )
        })?;
        match event.event_action {
            PagerDutyEventAction::Trigger => {
                // Honors PagerDuty Events API v2 dedup_key contract:
                // a Trigger with a key that already maps to an open
                // incident is collapsed (no new incident).
                open.entry(event.dedup_key.clone()).or_insert(event);
            }
            PagerDutyEventAction::Acknowledge => {
                // Acknowledge does NOT remove the incident — only
                // suppresses re-paging on escalation timer.
            }
            PagerDutyEventAction::Resolve => {
                open.remove(&event.dedup_key);
            }
        }
        Ok(())
    }
}

/// Always-failing PagerDuty dispatcher for adversarial tests of the
/// fail-OPEN envelope (alert path increments
/// `corelink_alerts_dispatched_total{dispatch="fail"}` SEV-2 counter
/// when production wiring detects transport failure).
#[derive(Debug, Default)]
pub struct FailingPagerDutyDispatcher;

impl FailingPagerDutyDispatcher {
    /// Construct a fresh always-failing dispatcher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl PagerDutyDispatcher for FailingPagerDutyDispatcher {
    fn dispatch(&self, _event: PagerDutyEvent) -> Result<(), SloPagerDutyDispatchError> {
        Err(SloPagerDutyDispatchError::Transport(
            "induced PagerDuty dispatch failure (test fixture)".to_string(),
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

    fn ev(dedup: &str, action: PagerDutyEventAction) -> PagerDutyEvent {
        PagerDutyEvent {
            service: PagerDutyServiceKey::ProdUs,
            event_action: action,
            dedup_key: dedup.to_string(),
            severity: "sev0",
            summary: "synthetic".to_string(),
            source: "slo-orchestrator",
            runbook_url: "https://corelink.io/runbooks/RB-SLO.md".to_string(),
        }
    }

    #[test]
    fn pagerduty_event_action_canonical_strings_pinned() {
        assert_eq!(PagerDutyEventAction::Trigger.as_str(), "trigger");
        assert_eq!(PagerDutyEventAction::Acknowledge.as_str(), "acknowledge");
        assert_eq!(PagerDutyEventAction::Resolve.as_str(), "resolve");
    }

    #[test]
    fn canonical_actions_unique() {
        let v = canonical_pagerduty_actions();
        assert_eq!(v.len(), 3);
        let mut set = std::collections::HashSet::new();
        for a in v {
            assert!(set.insert(a.as_str()));
        }
    }

    #[test]
    fn canonical_3_services_pinned() {
        assert_eq!(
            PagerDutyServiceKey::Staging.service_name(),
            "corelink-staging"
        );
        assert_eq!(
            PagerDutyServiceKey::ProdUs.service_name(),
            "corelink-prod-us"
        );
        assert_eq!(
            PagerDutyServiceKey::ProdEu.service_name(),
            "corelink-prod-eu"
        );
    }

    #[test]
    fn dispatch_trigger_opens_incident() {
        let d = InMemoryPagerDutyDispatcher::new();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        assert_eq!(d.attempt_count(), 1);
        assert_eq!(d.open_incident_count(), 1);
    }

    #[test]
    fn dispatch_trigger_dedup_collapses_to_single_incident() {
        let d = InMemoryPagerDutyDispatcher::new();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        assert_eq!(d.attempt_count(), 3);
        assert_eq!(d.open_incident_count(), 1);
    }

    #[test]
    fn dispatch_resolve_closes_incident() {
        let d = InMemoryPagerDutyDispatcher::new();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        d.dispatch(ev("k1", PagerDutyEventAction::Resolve)).unwrap();
        assert_eq!(d.open_incident_count(), 0);
        assert_eq!(d.attempt_count(), 2);
    }

    #[test]
    fn dispatch_acknowledge_does_not_resolve() {
        let d = InMemoryPagerDutyDispatcher::new();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        d.dispatch(ev("k1", PagerDutyEventAction::Acknowledge))
            .unwrap();
        assert_eq!(d.open_incident_count(), 1);
    }

    #[test]
    fn dispatch_distinct_keys_open_distinct_incidents() {
        let d = InMemoryPagerDutyDispatcher::new();
        d.dispatch(ev("k1", PagerDutyEventAction::Trigger)).unwrap();
        d.dispatch(ev("k2", PagerDutyEventAction::Trigger)).unwrap();
        assert_eq!(d.open_incident_count(), 2);
    }

    #[test]
    fn snapshot_open_incidents_sorted_by_key() {
        let d = InMemoryPagerDutyDispatcher::new();
        d.dispatch(ev("zebra", PagerDutyEventAction::Trigger))
            .unwrap();
        d.dispatch(ev("alpha", PagerDutyEventAction::Trigger))
            .unwrap();
        d.dispatch(ev("mango", PagerDutyEventAction::Trigger))
            .unwrap();
        let v = d.snapshot_open_incidents();
        let keys: Vec<&str> = v.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["alpha", "mango", "zebra"]);
    }

    #[test]
    fn failing_dispatcher_returns_transport_error() {
        let d = FailingPagerDutyDispatcher::new();
        let err = d
            .dispatch(ev("k1", PagerDutyEventAction::Trigger))
            .unwrap_err();
        assert!(matches!(err, SloPagerDutyDispatchError::Transport(_)));
    }

    #[test]
    fn cloned_dispatcher_shares_buffer() {
        let d1 = InMemoryPagerDutyDispatcher::new();
        let d2 = d1.clone();
        d1.dispatch(ev("k1", PagerDutyEventAction::Trigger))
            .unwrap();
        assert_eq!(d2.attempt_count(), 1);
        assert_eq!(d2.open_incident_count(), 1);
    }
}
