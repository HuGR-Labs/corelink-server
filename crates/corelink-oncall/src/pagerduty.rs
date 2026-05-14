//! PagerDuty Schedule + Events API client trait + fakes.
//!
//! ## Production wiring (deferred to PRR ship gate)
//!
//! - Schedule API HTTPS PUT (`/schedules/{id}` envelope: layers /
//!   restrictions / users) — applied via `worker::send_future`
//!   fire-and-forget per Lote 10.7bis R5 P0-3 (NEVER `tokio::spawn`).
//!   Bound to `${{ secrets.PAGERDUTY_API_KEY }}` (REST API token v2).
//! - Events API v2 HTTPS POST (`/v2/enqueue` envelope: `routing_key`
//!   plus `event_action` / `dedup_key` / `payload`) for the automatic
//!   handoff event when a HARD threshold breach fires.
//! - Webhook receiver: PagerDuty `incident.acknowledged` /
//!   `incident.resolved` webhooks update the ledger MTTA / MTTR
//!   counters.
//!
//! ## Why trait + fake here
//!
//! WI-S17-005 lands without remote + Cloudflare Workers + PagerDuty
//! staging account secrets (HARD inflection points per autonomous
//! execution charter). The fake covers the algorithmic invariants
//! that a production wiring bug would expose: per-engineer rotation
//! roster isolation; HARD-threshold-triggered handoff returns a
//! distinct `assignment_id`; failure modes surface as typed errors
//! per `OncallPagerDutyError`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::engineer::EngineerId;
use crate::error::OncallPagerDutyError;
use crate::tier::Tier;

/// Canonical PagerDuty Events API v2 `event_action` values per
/// <https://developer.pagerduty.com/docs/events-api-v2/>.
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
    /// Canonical wire string.
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

/// Canonical 3-element list of [`PagerDutyEventAction`] for
/// surface-stability regression tests.
#[must_use]
pub const fn canonical_pagerduty_actions() -> &'static [PagerDutyEventAction; 3] {
    &[
        PagerDutyEventAction::Trigger,
        PagerDutyEventAction::Acknowledge,
        PagerDutyEventAction::Resolve,
    ]
}

/// Canonical PagerDuty schedule key — one per tier per WI §6.1.1.
/// The `#[non_exhaustive]` marker reserves additive growth for
/// follow-on WIs (e.g. APAC GA pós-S-20 lights up a region-scoped
/// tier-1 schedule).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PagerDutyScheduleKey {
    /// `corelink-oncall-tier-1` — primary responder schedule.
    Tier1,
    /// `corelink-oncall-tier-2` — escalation schedule.
    Tier2,
    /// `corelink-oncall-tier-3` — architect / security schedule.
    Tier3,
}

impl PagerDutyScheduleKey {
    /// Canonical schedule name per WI §1.
    #[must_use]
    pub const fn schedule_name(self) -> &'static str {
        match self {
            Self::Tier1 => "corelink-oncall-tier-1",
            Self::Tier2 => "corelink-oncall-tier-2",
            Self::Tier3 => "corelink-oncall-tier-3",
        }
    }

    /// Convert from a [`Tier`].
    #[must_use]
    pub const fn from_tier(tier: Tier) -> Self {
        match tier {
            Tier::Tier1 => Self::Tier1,
            Tier::Tier2 => Self::Tier2,
            Tier::Tier3 => Self::Tier3,
        }
    }
}

impl core::fmt::Display for PagerDutyScheduleKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.schedule_name())
    }
}

/// A schedule assignment returned by the PagerDuty client when a
/// handoff is requested. The `assignment_id` mirrors the
/// PagerDuty API `incident.assignments[].assignee.id` field.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PagerDutyAssignment {
    /// PagerDuty assignment id.
    pub assignment_id: String,
    /// Target schedule.
    pub schedule: PagerDutyScheduleKey,
    /// Engineer the assignment was issued to.
    pub engineer: EngineerId,
    /// Effective start (ms since epoch).
    pub start_ms: u64,
}

/// PagerDuty client trait — every backend (live Schedule + Events
/// API, in-memory fake, adversarial failing fake) satisfies this.
pub trait PagerDutyClient: core::fmt::Debug + Send + Sync {
    /// Return the engineer currently assigned to `schedule` at
    /// `as_of_ms` (None when the schedule is empty / the timestamp
    /// is outside the rostered window).
    fn current_oncall(
        &self,
        schedule: PagerDutyScheduleKey,
        as_of_ms: u64,
    ) -> Result<Option<EngineerId>, OncallPagerDutyError>;

    /// Request an automatic handoff to the backup engineer for
    /// `schedule` at `as_of_ms`. The client mutates the upstream
    /// schedule (or in the fake: the in-memory roster) and returns
    /// the new assignment record.
    fn handoff_to_backup(
        &self,
        schedule: PagerDutyScheduleKey,
        as_of_ms: u64,
    ) -> Result<PagerDutyAssignment, OncallPagerDutyError>;
}

/// In-memory PagerDuty client fake — keeps a per-schedule ordered
/// rotation roster + a pointer to the current oncall slot.
#[derive(Clone, Debug, Default)]
pub struct InMemoryPagerDutyClient {
    state: Arc<Mutex<InMemoryPdState>>,
}

#[derive(Debug, Default)]
struct InMemoryPdState {
    rosters: HashMap<PagerDutyScheduleKey, Vec<EngineerId>>,
    current_idx: HashMap<PagerDutyScheduleKey, usize>,
    next_assignment_id: u64,
}

impl InMemoryPagerDutyClient {
    /// Construct an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the roster for a schedule. The first engineer in the
    /// roster is treated as the active oncall; subsequent entries
    /// are the backup rotation order.
    pub fn seed_roster(
        &self,
        schedule: PagerDutyScheduleKey,
        roster: Vec<EngineerId>,
    ) -> Result<(), OncallPagerDutyError> {
        let mut g = self.lock_state()?;
        g.rosters.insert(schedule, roster);
        g.current_idx.insert(schedule, 0);
        Ok(())
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, InMemoryPdState>, OncallPagerDutyError> {
        self.state
            .lock()
            .map_err(|e| OncallPagerDutyError::Transport(format!("mutex poisoned: {e}")))
    }
}

impl PagerDutyClient for InMemoryPagerDutyClient {
    fn current_oncall(
        &self,
        schedule: PagerDutyScheduleKey,
        _as_of_ms: u64,
    ) -> Result<Option<EngineerId>, OncallPagerDutyError> {
        let g = self.lock_state()?;
        let roster = match g.rosters.get(&schedule) {
            Some(r) => r,
            None => return Ok(None),
        };
        let idx = *g.current_idx.get(&schedule).unwrap_or(&0);
        Ok(roster.get(idx).cloned())
    }

    fn handoff_to_backup(
        &self,
        schedule: PagerDutyScheduleKey,
        as_of_ms: u64,
    ) -> Result<PagerDutyAssignment, OncallPagerDutyError> {
        let mut g = self.lock_state()?;
        let roster_len = g
            .rosters
            .get(&schedule)
            .map(Vec::len)
            .ok_or_else(|| OncallPagerDutyError::NotFound(schedule.schedule_name().to_string()))?;
        if roster_len < 2 {
            return Err(OncallPagerDutyError::NotFound(format!(
                "{} has no backup engineer in roster",
                schedule.schedule_name()
            )));
        }
        let new_idx = {
            let cur = *g.current_idx.get(&schedule).unwrap_or(&0);
            (cur + 1) % roster_len
        };
        g.current_idx.insert(schedule, new_idx);
        let new_eng = g
            .rosters
            .get(&schedule)
            .and_then(|r| r.get(new_idx))
            .cloned()
            .ok_or_else(|| OncallPagerDutyError::NotFound("roster slot vacated".to_string()))?;
        let id = g.next_assignment_id;
        g.next_assignment_id = g.next_assignment_id.saturating_add(1);
        Ok(PagerDutyAssignment {
            assignment_id: format!("pd-asn-{id}"),
            schedule,
            engineer: new_eng,
            start_ms: as_of_ms,
        })
    }
}

/// Adversarial fixture that always fails (forces ledger fail-CLOSED
/// behaviour in tests).
#[derive(Clone, Debug, Default)]
pub struct FailingPagerDutyClient;

impl PagerDutyClient for FailingPagerDutyClient {
    fn current_oncall(
        &self,
        _schedule: PagerDutyScheduleKey,
        _as_of_ms: u64,
    ) -> Result<Option<EngineerId>, OncallPagerDutyError> {
        Err(OncallPagerDutyError::Transport(
            "adversarial fixture: always rejects".to_string(),
        ))
    }

    fn handoff_to_backup(
        &self,
        _schedule: PagerDutyScheduleKey,
        _as_of_ms: u64,
    ) -> Result<PagerDutyAssignment, OncallPagerDutyError> {
        Err(OncallPagerDutyError::Transport(
            "adversarial fixture: always rejects".to_string(),
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
    fn schedule_keys_canonical() {
        assert_eq!(
            PagerDutyScheduleKey::Tier1.schedule_name(),
            "corelink-oncall-tier-1"
        );
        assert_eq!(
            PagerDutyScheduleKey::from_tier(Tier::Tier2),
            PagerDutyScheduleKey::Tier2
        );
    }

    #[test]
    fn in_memory_roster_handoff_rotates_index() {
        let pd = InMemoryPagerDutyClient::new();
        let roster = vec![
            EngineerId::new("a"),
            EngineerId::new("b"),
            EngineerId::new("c"),
        ];
        pd.seed_roster(PagerDutyScheduleKey::Tier1, roster).unwrap();

        assert_eq!(
            pd.current_oncall(PagerDutyScheduleKey::Tier1, 0).unwrap(),
            Some(EngineerId::new("a"))
        );
        let asn = pd.handoff_to_backup(PagerDutyScheduleKey::Tier1, 100).unwrap();
        assert_eq!(asn.engineer, EngineerId::new("b"));
        let asn2 = pd.handoff_to_backup(PagerDutyScheduleKey::Tier1, 200).unwrap();
        assert_eq!(asn2.engineer, EngineerId::new("c"));
        // Wraps back to "a".
        let asn3 = pd.handoff_to_backup(PagerDutyScheduleKey::Tier1, 300).unwrap();
        assert_eq!(asn3.engineer, EngineerId::new("a"));
    }

    #[test]
    fn handoff_rejects_no_backup() {
        let pd = InMemoryPagerDutyClient::new();
        pd.seed_roster(PagerDutyScheduleKey::Tier1, vec![EngineerId::new("a")])
            .unwrap();
        let err = pd.handoff_to_backup(PagerDutyScheduleKey::Tier1, 0).unwrap_err();
        assert!(matches!(err, OncallPagerDutyError::NotFound(_)));
    }

    #[test]
    fn handoff_rejects_unknown_schedule() {
        let pd = InMemoryPagerDutyClient::new();
        let err = pd
            .handoff_to_backup(PagerDutyScheduleKey::Tier3, 0)
            .unwrap_err();
        assert!(matches!(err, OncallPagerDutyError::NotFound(_)));
    }

    #[test]
    fn failing_client_always_rejects() {
        let pd = FailingPagerDutyClient;
        assert!(pd.current_oncall(PagerDutyScheduleKey::Tier1, 0).is_err());
        assert!(pd
            .handoff_to_backup(PagerDutyScheduleKey::Tier1, 0)
            .is_err());
    }
}
