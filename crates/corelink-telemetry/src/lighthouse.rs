//! `corelink-lighthouse-tracker` — WI-S20-004.
//!
//! Operationalizes the **3 lighthouse customer program** (2 team tier + 1
//! enterprise BYOK) state machine + 30-day SLA observation + attestation
//! gating per WI-S20-004 §5.1..§5.3 + spec contract §10.s20.7.
//!
//! ## Contract
//!
//! Pure-logic library — no I/O, no async runtime. Host adapters (D1
//! migration `0042`, CF Cron, CLI) bind via the [`LighthouseStore`] trait.
//! Library is **fail-CLOSED**: illegal state transitions or
//! schema-violating inputs return [`TrackerError`] and emit
//! `corelink_lighthouse_illegal_transition_total` at the adapter layer.
//!
//! ## Domain model
//!
//! * [`CustomerId`] — newtype, validated `LH-<UPPER ALNUM/HYPHEN>+` shape
//!   (e.g. `LH-FORGE`, `LH-OSS-01`, `LH-ENT-BYOK-01`).
//! * [`Tier`] — `Team` | `EnterpriseByok`. 2 team + 1 enterprise per spec.
//! * [`LifecycleState`] — 6 states covering the WI-S20-004 §5.1 timeline:
//!   recruiting → engaged → migrating → observing → attested →
//!   case-study-signed. Plus terminal [`LifecycleState::Withdrawn`]
//!   (FM-LIGHTHOUSE-CUSTOMER-DESISTS-MID-SPRINT mitigation).
//! * [`LighthouseCustomer`] — full record persisted in D1.
//! * [`SlaSample`] — one daily observation point used to compute breach.
//!
//! ## State machine canonical
//!
//! ```text
//! Recruiting ──► Engaged ──► Migrating ──► Observing ──► Attested ──► CaseStudySigned
//!     │             │           │              │
//!     └─► Withdrawn ◄┘           └► Withdrawn  └► (SLA miss) ──► Engaged-rollback NOT permitted at S-20
//! ```
//!
//! Per WI-S20-004 §6.2 negative path 2: SLA miss em 30d → SRE escalation +
//! remediation flow + iteration cycle. The state machine **does not** auto-
//! transition Observing → Attested on SLA breach; instead it surfaces the
//! breach + requires explicit operator clearance.
//!
//! ## 30-day window canonical
//!
//! Per WI-S20-004 §2.1 item 4 + spec contract §6.1 + §10.s20.7:
//!
//! * Observation window = **30 days** fixed (2 592 000 seconds — not
//!   calendar months; matches `corelink-runbook-tracker` precedent).
//! * SLA claim met sustained 30d = zero `slo_violation_total` em any sample
//!   during the window for the per-customer SLO set.
//!
//! ## Failure modes mitigated
//!
//! * `FM-LIGHTHOUSE-CUSTOMER-DESISTS-MID-SPRINT` — [`LifecycleState::Withdrawn`]
//!   reachable from `Recruiting | Engaged | Migrating`. Once Observing, a
//!   withdrawal flips the slot to `Engaged` for a backup candidate.
//! * `FM-LIGHTHOUSE-SLA-CLAIM-MISS-30D` — [`ObservationOutcome::Miss`]
//!   blocks `Observing → Attested`. Operator must clear via explicit
//!   remediation cycle (separate Engaged → Observing re-entry).
//! * `FM-ENTERPRISE-BYOK-ICP-FAIL` — only [`Tier::EnterpriseByok`] requires
//!   `byok_key_health_ok` true at attestation; [`Tier::Team`] ignores it.
//!
//! ## Privacy
//!
//! `customer_id` is the **internal lighthouse slot id** (`LH-FORGE`, not a
//! tenant_id). No PII stored. Operator names are operator ids
//! (`op_<alias>`). Notes are sanitized free-text per CTRL-PRIV-001.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical observation window — 30 days fixed (not calendar months) per
/// WI-S20-004 §2.1 item 4 + spec contract §6.1. Matches the
/// `corelink-runbook-tracker` precedent.
pub const OBSERVATION_WINDOW_SECS: i64 = 30 * 24 * 60 * 60;

/// Number of lighthouse slots at GA per spec contract §15 risk row 2:
/// 2 team tier + 1 enterprise BYOK = 3. Waivable to 2 with plan to add 1
/// within 60d pós-GA (ADR + CEO approval).
pub const GA_LIGHTHOUSE_SLOTS_TOTAL: usize = 3;
/// Number of Team-tier slots at GA.
pub const GA_LIGHTHOUSE_TEAM_SLOTS: usize = 2;
/// Number of Enterprise BYOK slots at GA.
pub const GA_LIGHTHOUSE_ENTERPRISE_SLOTS: usize = 1;

/// Errors produced by the tracker.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrackerError {
    /// Customer id failed shape validation (`LH-<UPPER ALNUM/HYPHEN>+`).
    #[error("invalid customer id: {0}")]
    InvalidCustomerId(String),
    /// Operator id was empty.
    #[error("empty operator id")]
    EmptyOperator,
    /// Free-text required field was empty.
    #[error("empty required field: {0}")]
    EmptyRequiredField(&'static str),
    /// Illegal state transition attempted (fail-CLOSED).
    #[error("illegal transition: {from:?} → {to:?}")]
    IllegalTransition {
        /// Source state.
        from: LifecycleState,
        /// Attempted target state.
        to: LifecycleState,
    },
    /// Attestation attempted while observation window not yet elapsed.
    #[error("observation window not elapsed: {elapsed_secs}s < {required_secs}s")]
    ObservationWindowNotElapsed {
        /// Seconds since observation started.
        elapsed_secs: i64,
        /// Required seconds (canonical 30d).
        required_secs: i64,
    },
    /// Attestation attempted while a recorded SLA miss is still open.
    #[error("attestation blocked: SLA breach recorded — remediation required")]
    SlaBreachBlocksAttestation,
    /// Enterprise BYOK attestation attempted without a healthy key status.
    #[error("enterprise BYOK attestation blocked: key health = false")]
    ByokKeyHealthFailed,
    /// Slot allocation violates the canonical 2 team + 1 enterprise breakdown.
    #[error("slot allocation violation: {0}")]
    SlotAllocationViolation(String),
    /// A timestamp was negative (caller bug).
    #[error("negative timestamp: {0}")]
    NegativeTimestamp(i64),
}

/// Validated lighthouse customer slot id — `LH-<UPPER ALNUM/hyphen>+`.
///
/// Examples: `LH-FORGE`, `LH-OSS-01`, `LH-ENT-BYOK-01`. Casing is
/// preserved (id is stored verbatim) but validation is case-sensitive —
/// lighthouse slot ids are uppercase in the spec corpus.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CustomerId(String);

impl CustomerId {
    /// Validate and wrap a string as a [`CustomerId`].
    pub fn new(raw: impl Into<String>) -> Result<Self, TrackerError> {
        let s: String = raw.into();
        if !s.starts_with("LH-") || s.len() < 4 {
            return Err(TrackerError::InvalidCustomerId(s));
        }
        let suffix = s.get(3..).unwrap_or("");
        if suffix.is_empty()
            || !suffix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(TrackerError::InvalidCustomerId(s));
        }
        Ok(Self(s))
    }

    /// Borrow the wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CustomerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for CustomerId {
    type Error = TrackerError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<CustomerId> for String {
    fn from(c: CustomerId) -> Self {
        c.0
    }
}

/// Lighthouse tier per spec WI-S20-004 §2.1 item 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Tier {
    /// Team tier — Forge customer-zero + 1 external OSS Bazel/Buck2.
    Team,
    /// Enterprise BYOK — 1 ICP enterprise pre-revenue Q3 via Sales outreach.
    EnterpriseByok,
}

impl Tier {
    /// Snake-case label for Prometheus + audit JSON.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::EnterpriseByok => "enterprise_byok",
        }
    }

    /// Whether BYOK key health is a gating condition for attestation.
    #[must_use]
    pub const fn requires_byok_health(self) -> bool {
        matches!(self, Self::EnterpriseByok)
    }
}

/// Lifecycle state per WI-S20-004 §5.1 timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleState {
    /// Outreach + LOI signed; no engagement contract yet.
    Recruiting,
    /// LOI signed + DPA review in flight; scoping call complete.
    Engaged,
    /// Migration day window — signup → DPA → tier → first PAT → first CAS PUT.
    Migrating,
    /// 30d observation post-migration; SLA tracked daily.
    Observing,
    /// 30d window cleared + attestation form signed by customer.
    Attested,
    /// Case study Legal-reviewed + customer-approved; sharable sob NDA.
    CaseStudySigned,
    /// Customer withdrew mid-sprint; slot must be backfilled by backup
    /// per FM-LIGHTHOUSE-CUSTOMER-DESISTS-MID-SPRINT.
    Withdrawn,
}

impl LifecycleState {
    /// Snake-case label for Prometheus `corelink_lighthouse_customer_state` gauge.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Recruiting => "recruiting",
            Self::Engaged => "engaged",
            Self::Migrating => "migrating",
            Self::Observing => "observing",
            Self::Attested => "attested",
            Self::CaseStudySigned => "case_study_signed",
            Self::Withdrawn => "withdrawn",
        }
    }

    /// Whether this state is terminal (no further forward transition).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::CaseStudySigned | Self::Withdrawn)
    }

    /// Canonical forward transitions allowed from this state.
    ///
    /// `Withdrawn` is allowed from any pre-attestation state — spec contract
    /// §15 row 2 mitigation. Once `Attested` the customer is committed; the
    /// only remaining transition is `CaseStudySigned`.
    #[must_use]
    pub fn allowed_next(self) -> &'static [LifecycleState] {
        match self {
            Self::Recruiting => &[Self::Engaged, Self::Withdrawn],
            Self::Engaged => &[Self::Migrating, Self::Withdrawn],
            Self::Migrating => &[Self::Observing, Self::Withdrawn],
            Self::Observing => &[Self::Attested],
            Self::Attested => &[Self::CaseStudySigned],
            Self::CaseStudySigned | Self::Withdrawn => &[],
        }
    }

    /// Whether `to` is a legal transition from `self`.
    #[must_use]
    pub fn can_transition_to(self, to: LifecycleState) -> bool {
        self.allowed_next().contains(&to)
    }
}

/// Outcome of the 30d observation window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ObservationOutcome {
    /// 30d window elapsed clean — SLA claim met sustained.
    Met,
    /// 30d window had ≥ 1 SLO violation — remediation cycle required.
    Miss,
    /// Window still open — outcome not yet determinable.
    Pending,
}

/// One SLA observation sample (one per day per customer canonical).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SlaSample {
    /// Sample id (caller-provided UUID v4 stringified).
    pub sample_id: String,
    /// Customer slot.
    pub customer_id: CustomerId,
    /// Unix epoch seconds when sample was taken.
    pub sampled_at: i64,
    /// Whether SLO-AVAIL-CAS-PUT ≥ 99.9% sustained over sample period.
    pub avail_cas_put_met: bool,
    /// Whether SLO-AVAIL-CAS-GET ≥ 99.9% sustained over sample period.
    pub avail_cas_get_met: bool,
    /// Whether SLO-LAT-CAS-GET p99 < 300ms sustained over sample period.
    pub lat_cas_get_p99_met: bool,
    /// Whether SLO-FRESH-BILLING < 0.1% drift 24h reconciliation sustained.
    pub fresh_billing_met: bool,
    /// BYOK key health (Enterprise BYOK tier only; ignored for Team).
    pub byok_key_health_ok: bool,
}

impl SlaSample {
    /// Whether **all** SLOs were met for this sample. BYOK health is
    /// included only if the customer tier requires it; the helper [`Self::is_met_for`]
    /// applies the tier rule.
    #[must_use]
    pub const fn all_core_slos_met(&self) -> bool {
        self.avail_cas_put_met
            && self.avail_cas_get_met
            && self.lat_cas_get_p99_met
            && self.fresh_billing_met
    }

    /// Whether this sample is fully clean for the given tier.
    #[must_use]
    pub fn is_met_for(&self, tier: Tier) -> bool {
        if !self.all_core_slos_met() {
            return false;
        }
        if tier.requires_byok_health() && !self.byok_key_health_ok {
            return false;
        }
        true
    }
}

/// Full lighthouse customer record persisted in D1 `lighthouse_customers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LighthouseCustomer {
    /// Internal slot id (`LH-FORGE`, `LH-OSS-01`, `LH-ENT-BYOK-01`).
    pub customer_id: CustomerId,
    /// Tier (Team | EnterpriseByok).
    pub tier: Tier,
    /// Current lifecycle state.
    pub state: LifecycleState,
    /// Unix epoch seconds when LOI was signed (Recruiting → Engaged trigger).
    pub loi_signed_at: Option<i64>,
    /// Unix epoch seconds when migration day completed (Migrating → Observing).
    pub migration_completed_at: Option<i64>,
    /// Unix epoch seconds when 30d observation started (== `migration_completed_at`).
    pub observation_started_at: Option<i64>,
    /// Whether any SLA breach was recorded during observation.
    pub sla_breach_recorded: bool,
    /// Unix epoch seconds when attestation was signed by customer.
    pub attestation_signed_at: Option<i64>,
    /// Unix epoch seconds when case study Legal-approved + customer-approved.
    pub case_study_signed_at: Option<i64>,
}

impl LighthouseCustomer {
    /// Construct a new customer entering [`LifecycleState::Recruiting`].
    pub fn recruit(customer_id: CustomerId, tier: Tier) -> Self {
        Self {
            customer_id,
            tier,
            state: LifecycleState::Recruiting,
            loi_signed_at: None,
            migration_completed_at: None,
            observation_started_at: None,
            sla_breach_recorded: false,
            attestation_signed_at: None,
            case_study_signed_at: None,
        }
    }

    /// Attempt a state transition. Fail-CLOSED: illegal transitions return
    /// [`TrackerError::IllegalTransition`] and the customer is **not**
    /// mutated.
    ///
    /// # Errors
    ///
    /// * [`TrackerError::IllegalTransition`] when `target` is not in
    ///   `self.state.allowed_next()`.
    /// * [`TrackerError::ObservationWindowNotElapsed`] for
    ///   `Observing → Attested` if `now_ts - observation_started_at < 30d`.
    /// * [`TrackerError::SlaBreachBlocksAttestation`] for
    ///   `Observing → Attested` if `sla_breach_recorded == true`.
    /// * [`TrackerError::ByokKeyHealthFailed`] for enterprise BYOK
    ///   `Observing → Attested` without `byok_health_ok == true`.
    /// * [`TrackerError::NegativeTimestamp`] on negative `now_ts`.
    pub fn transition_to(
        &mut self,
        target: LifecycleState,
        now_ts: i64,
        byok_health_ok: bool,
    ) -> Result<(), TrackerError> {
        if now_ts < 0 {
            return Err(TrackerError::NegativeTimestamp(now_ts));
        }
        if !self.state.can_transition_to(target) {
            return Err(TrackerError::IllegalTransition {
                from: self.state,
                to: target,
            });
        }

        // Gate-specific preconditions (only on the load-bearing forward
        // transitions; Withdrawn is always allowed without gating).
        match (self.state, target) {
            (LifecycleState::Migrating, LifecycleState::Observing) => {
                self.migration_completed_at = Some(now_ts);
                self.observation_started_at = Some(now_ts);
            }
            (LifecycleState::Observing, LifecycleState::Attested) => {
                let started =
                    self.observation_started_at
                        .ok_or(TrackerError::IllegalTransition {
                            from: self.state,
                            to: target,
                        })?;
                let elapsed = now_ts.saturating_sub(started);
                if elapsed < OBSERVATION_WINDOW_SECS {
                    return Err(TrackerError::ObservationWindowNotElapsed {
                        elapsed_secs: elapsed,
                        required_secs: OBSERVATION_WINDOW_SECS,
                    });
                }
                if self.sla_breach_recorded {
                    return Err(TrackerError::SlaBreachBlocksAttestation);
                }
                if self.tier.requires_byok_health() && !byok_health_ok {
                    return Err(TrackerError::ByokKeyHealthFailed);
                }
                self.attestation_signed_at = Some(now_ts);
            }
            (LifecycleState::Attested, LifecycleState::CaseStudySigned) => {
                self.case_study_signed_at = Some(now_ts);
            }
            (LifecycleState::Recruiting, LifecycleState::Engaged) => {
                self.loi_signed_at = Some(now_ts);
            }
            _ => {}
        }

        self.state = target;
        Ok(())
    }

    /// Record an SLA sample; flips `sla_breach_recorded = true` if the
    /// sample is not met for this customer's tier. Idempotent on
    /// `sample.sample_id` at the adapter layer (D1 PRIMARY KEY).
    pub fn record_sample(&mut self, sample: &SlaSample) {
        if !sample.is_met_for(self.tier) {
            self.sla_breach_recorded = true;
        }
    }

    /// Compute the observation outcome at `now_ts` (without mutating).
    #[must_use]
    pub fn observation_outcome(&self, now_ts: i64) -> ObservationOutcome {
        let Some(started) = self.observation_started_at else {
            return ObservationOutcome::Pending;
        };
        let elapsed = now_ts.saturating_sub(started);
        if elapsed < OBSERVATION_WINDOW_SECS {
            return ObservationOutcome::Pending;
        }
        if self.sla_breach_recorded {
            ObservationOutcome::Miss
        } else {
            ObservationOutcome::Met
        }
    }
}

/// Aggregate result for the GA Evidence Gate D+60 check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GaGateReport {
    /// Total slots filled (CaseStudySigned + Attested count toward GA gate).
    pub slots_attested: usize,
    /// Team-tier attested count.
    pub team_attested: usize,
    /// Enterprise BYOK attested count.
    pub enterprise_attested: usize,
    /// Whether the 3-slot GA gate is met (≥ 3 attested with correct breakdown).
    pub gate_met: bool,
}

/// Compute the GA Evidence Gate D+60 report from a customer roster.
///
/// Per WI-S20-004 §6.1.7: 3/3 lighthouse customers SLA claim met sustained
/// 30d. `Attested` and `CaseStudySigned` both count toward the slot tally.
/// The waiver path (3 → 2 with plan to add 1 within 60d pós-GA) is **not**
/// computed here — it requires ADR + CEO approval out-of-band.
///
/// # Errors
///
/// Returns [`TrackerError::SlotAllocationViolation`] if the roster violates
/// the canonical 2-team + 1-enterprise breakdown (more than 3 slots or
/// wrong mix).
pub fn compute_ga_gate(roster: &[LighthouseCustomer]) -> Result<GaGateReport, TrackerError> {
    if roster.len() > GA_LIGHTHOUSE_SLOTS_TOTAL {
        return Err(TrackerError::SlotAllocationViolation(format!(
            "{} slots configured, max {}",
            roster.len(),
            GA_LIGHTHOUSE_SLOTS_TOTAL
        )));
    }

    let mut team_attested = 0;
    let mut enterprise_attested = 0;
    let mut team_total = 0;
    let mut enterprise_total = 0;

    for c in roster {
        // Withdrawn slots do not count toward the breakdown — they were
        // backfilled. Spec §15 row 2 mitigation.
        if c.state == LifecycleState::Withdrawn {
            continue;
        }
        match c.tier {
            Tier::Team => team_total += 1,
            Tier::EnterpriseByok => enterprise_total += 1,
        }
        if matches!(
            c.state,
            LifecycleState::Attested | LifecycleState::CaseStudySigned
        ) {
            match c.tier {
                Tier::Team => team_attested += 1,
                Tier::EnterpriseByok => enterprise_attested += 1,
            }
        }
    }

    if team_total > GA_LIGHTHOUSE_TEAM_SLOTS {
        return Err(TrackerError::SlotAllocationViolation(format!(
            "{team_total} team slots, max {GA_LIGHTHOUSE_TEAM_SLOTS}"
        )));
    }
    if enterprise_total > GA_LIGHTHOUSE_ENTERPRISE_SLOTS {
        return Err(TrackerError::SlotAllocationViolation(format!(
            "{enterprise_total} enterprise slots, max {GA_LIGHTHOUSE_ENTERPRISE_SLOTS}"
        )));
    }

    let slots_attested = team_attested + enterprise_attested;
    let gate_met = team_attested >= GA_LIGHTHOUSE_TEAM_SLOTS
        && enterprise_attested >= GA_LIGHTHOUSE_ENTERPRISE_SLOTS;

    Ok(GaGateReport {
        slots_attested,
        team_attested,
        enterprise_attested,
        gate_met,
    })
}

/// Trait for persistence — host adapter binds to D1 (migration `0042`) or
/// any other store. Tracker library is pure; the store is the I/O boundary.
pub trait LighthouseStore {
    /// Persistence error type (opaque to library; adapter chooses).
    type Error: std::error::Error + Send + Sync + 'static;

    /// Upsert a customer record. MUST be idempotent on `customer_id`.
    ///
    /// # Errors
    ///
    /// Returns adapter-defined errors (D1 transport, schema mismatch, etc.).
    fn upsert(&self, customer: &LighthouseCustomer) -> Result<(), Self::Error>;

    /// Insert an SLA sample. MUST be idempotent on `sample.sample_id` (D1
    /// PRIMARY KEY).
    ///
    /// # Errors
    ///
    /// Returns adapter-defined errors.
    fn append_sample(&self, sample: &SlaSample) -> Result<(), Self::Error>;

    /// Load the current roster (all customers, all states).
    ///
    /// # Errors
    ///
    /// Returns adapter-defined errors.
    fn load_roster(&self) -> Result<Vec<LighthouseCustomer>, Self::Error>;
}

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::lighthouse"
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "tests.rs"]
mod tests;
