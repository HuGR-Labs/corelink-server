//! Canonical tenant-offboarding state machine: states + per-state
//! capability flags + transition triggers + transition validity table.
//!
//! ## Canonical state machine
//!
//! ```text
//!   ACTIVE
//!     │
//!     │ CustomerInitiated (T+0, anti-fraud verified)
//!     v
//!   CANCEL_REQUESTED ───────── CustomerReverted ──┐
//!     │                                            │
//!     │ TimerExpired (T+1 day)                     │
//!     v                                            │
//!   GRACE_PERIOD ──────────── CustomerReverted ────┤
//!     │                                            │
//!     │ TimerExpired (T+30 days)                   │
//!     v                                            │
//!   READ_ONLY ──────────────── CustomerReverted ───┘
//!     │
//!     │ TimerExpired (T+45 days)
//!     v
//!   SUSPENDED
//!     │
//!     │ TimerExpired (T+90 days) +
//!     │ AdminCommitErasure (dry-run preview confirmed)
//!     v
//!   ERASED  (terminal)
//! ```
//!
//! Plus `OpsForced` allowed on any non-terminal state for the
//! operator force-advance path (auditable, runbook-gated).

use serde::{Deserialize, Serialize};

/// Canonical 6-arm tenant-offboarding state taxonomy.
///
/// `#[non_exhaustive]` reserves additive growth (e.g. a future
/// `LEGAL_HOLD` arm for litigation-pause semantics).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum TenantOffboardingState {
    /// Normal operating tenant. All capabilities allowed; no
    /// offboarding ticket exists.
    Active,
    /// `CANCEL_REQUESTED` — tenant has been marked for offboarding at
    /// T+0 (customer click + anti-fraud verified by support agent).
    /// Read/write/admin still allowed (we do not strand customers
    /// before they've been told the offboarding started). Export
    /// allowed.
    CancelRequested,
    /// `GRACE_PERIOD` — T+1 to T+30. Read + write + admin still
    /// allowed (customer may still operate the tenant; this is a
    /// soft phase). Export allowed; restore (revert to ACTIVE)
    /// allowed.
    GracePeriod,
    /// `READ_ONLY` — T+30 to T+45. Read + export allowed; writes
    /// rejected (HTTP 409 with `offboarding_read_only` body code);
    /// admin still allowed for runbook ops; restore still allowed.
    ReadOnly,
    /// `SUSPENDED` — T+45 to T+90. No read / no write / no export.
    /// Admin allowed only for runbook ops + force-advance. Restore
    /// requires Ops force (no longer self-service).
    Suspended,
    /// `ERASED` — T+90+. Terminal. Cryptographic erasure executed
    /// (BYOK CMK destroyed when applicable) + R2/D1/KV tombstone-
    /// and-purge committed. No reads / writes / admin / export /
    /// restore. The audit chain entry remains as the forensic
    /// anchor; the tenant_id itself is reserved (never re-issued).
    Erased,
}

impl TenantOffboardingState {
    /// Canonical lower-snake-case mnemonic. Pinned for D1 CHECK
    /// constraints + dashboard widget grouping.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::CancelRequested => "cancel_requested",
            Self::GracePeriod => "grace_period",
            Self::ReadOnly => "read_only",
            Self::Suspended => "suspended",
            Self::Erased => "erased",
        }
    }

    /// Whether this state is terminal (no further transitions out).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Erased)
    }

    /// Canonical per-state capability flags. Production-side
    /// middleware reads this surface to gate writes / reads /
    /// admin / export / restore at the request-handler boundary.
    #[must_use]
    pub const fn caps(self) -> TenantOffboardingCaps {
        match self {
            Self::Active => TenantOffboardingCaps {
                read_allowed: true,
                write_allowed: true,
                admin_allowed: true,
                export_allowed: true,
                restore_allowed: false, // nothing to restore from
            },
            Self::CancelRequested | Self::GracePeriod => TenantOffboardingCaps {
                read_allowed: true,
                write_allowed: true,
                admin_allowed: true,
                export_allowed: true,
                restore_allowed: true,
            },
            Self::ReadOnly => TenantOffboardingCaps {
                read_allowed: true,
                write_allowed: false,
                admin_allowed: true,
                export_allowed: true,
                restore_allowed: true,
            },
            Self::Suspended => TenantOffboardingCaps {
                read_allowed: false,
                write_allowed: false,
                admin_allowed: true, // ops force-advance / force-revert only
                export_allowed: false,
                restore_allowed: false, // ops force only
            },
            Self::Erased => TenantOffboardingCaps {
                read_allowed: false,
                write_allowed: false,
                admin_allowed: false,
                export_allowed: false,
                restore_allowed: false,
            },
        }
    }
}

impl core::fmt::Display for TenantOffboardingState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-state capability flags. Production middleware uses these to
/// reject requests at the handler boundary (HTTP 409 with the
/// canonical body code per `error_taxonomy.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantOffboardingCaps {
    /// Whether the tenant can serve read requests.
    pub read_allowed: bool,
    /// Whether the tenant can serve write requests.
    pub write_allowed: bool,
    /// Whether the tenant can serve admin requests (RBAC role
    /// `admin`; includes tenant settings, audit reads, runbook ops).
    pub admin_allowed: bool,
    /// Whether the tenant can run a CLI / API export of its data.
    pub export_allowed: bool,
    /// Whether a [`TransitionTrigger::CustomerReverted`] is allowed
    /// from this state without operator escalation.
    pub restore_allowed: bool,
}

/// Canonical 5-arm transition trigger taxonomy.
///
/// Every state transition must be triggered by exactly one of these
/// canonical events. `#[non_exhaustive]` reserves additive growth.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum TransitionTrigger {
    /// Customer clicks "Cancel my tenant" + the support agent
    /// verifies anti-fraud (someone authorized to sign on the
    /// account; canonical T-0 check). Advances
    /// ACTIVE → CANCEL_REQUESTED.
    CustomerInitiated,
    /// Customer changes their mind (self-service) during the
    /// canonical revert window (T+0..T+45). Advances any of
    /// CANCEL_REQUESTED / GRACE_PERIOD / READ_ONLY back to ACTIVE.
    CustomerReverted,
    /// The canonical daily-cron tick observes that the per-state
    /// timer (T+30 / T+45 / T+90) has been reached. Advances
    /// CANCEL_REQUESTED → GRACE_PERIOD / GRACE_PERIOD → READ_ONLY /
    /// READ_ONLY → SUSPENDED.
    TimerExpired,
    /// Operator force-advance (auditable; runbook-gated; required
    /// because some lifecycle events are time-driven but the
    /// operator may need to skip ahead for legal / regulatory /
    /// support reasons). Allowed on any non-terminal source state.
    OpsForced,
    /// The canonical commit-final-erasure trigger. Requires a
    /// dry-run preview to have been generated AND confirmed by
    /// operator; advances SUSPENDED → ERASED with BYOK CMK destroy
    /// + R2/D1/KV cleanup. Distinct from `OpsForced` because
    /// the final erasure is irreversible and demands a stronger
    /// authorization envelope at the runbook layer.
    AdminCommitErasure,
}

impl TransitionTrigger {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CustomerInitiated => "customer_initiated",
            Self::CustomerReverted => "customer_reverted",
            Self::TimerExpired => "timer_expired",
            Self::OpsForced => "ops_forced",
            Self::AdminCommitErasure => "admin_commit_erasure",
        }
    }
}

impl core::fmt::Display for TransitionTrigger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical typed transition: `(from, trigger) → to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TenantOffboardingTransition {
    /// Source state.
    pub from: TenantOffboardingState,
    /// Trigger that drove the transition.
    pub trigger: TransitionTrigger,
    /// Destination state.
    pub to: TenantOffboardingState,
}

impl TenantOffboardingTransition {
    /// Resolve the canonical destination state for a
    /// `(from, trigger)` pair, or `None` if the pair is illegal.
    /// `None` SHOULD be surfaced by callers as
    /// [`crate::error::TenantOffboardingError::IllegalTransition`].
    #[must_use]
    pub const fn resolve(
        from: TenantOffboardingState,
        trigger: TransitionTrigger,
    ) -> Option<TenantOffboardingState> {
        use TenantOffboardingState as S;
        use TransitionTrigger as T;
        match (from, trigger) {
            // Customer initiates offboarding from ACTIVE.
            (S::Active, T::CustomerInitiated) => Some(S::CancelRequested),

            // Timer-driven forward progression (canonical happy path).
            (S::CancelRequested, T::TimerExpired) => Some(S::GracePeriod),
            (S::GracePeriod, T::TimerExpired) => Some(S::ReadOnly),
            (S::ReadOnly, T::TimerExpired) => Some(S::Suspended),
            (S::Suspended, T::TimerExpired) => None, // requires AdminCommitErasure

            // Customer-self-service revert (T+0..T+45 window).
            (S::CancelRequested | S::GracePeriod | S::ReadOnly, T::CustomerReverted) => {
                Some(S::Active)
            }
            (S::Suspended | S::Erased | S::Active, T::CustomerReverted) => None,

            // Ops force-advance (auditable; runbook-gated). Allowed
            // on any non-terminal source. Forward-only (skip-ahead);
            // does NOT allow backwards motion except through
            // CustomerReverted (modelled separately for clarity).
            (S::Active, T::OpsForced) => Some(S::CancelRequested),
            (S::CancelRequested, T::OpsForced) => Some(S::GracePeriod),
            (S::GracePeriod, T::OpsForced) => Some(S::ReadOnly),
            (S::ReadOnly, T::OpsForced) => Some(S::Suspended),
            (S::Suspended, T::OpsForced) => None, // explicit
            (S::Erased, T::OpsForced) => None,

            // Final erasure commit (irreversible). ONLY from
            // SUSPENDED, AND only after dry-run preview confirmation.
            (S::Suspended, T::AdminCommitErasure) => Some(S::Erased),
            (
                S::Active | S::CancelRequested | S::GracePeriod | S::ReadOnly | S::Erased,
                T::AdminCommitErasure,
            ) => None,

            // CustomerInitiated from anywhere except ACTIVE is
            // illegal (the offboarding ticket already exists).
            (
                S::CancelRequested | S::GracePeriod | S::ReadOnly | S::Suspended | S::Erased,
                T::CustomerInitiated,
            ) => None,

            // TimerExpired from ACTIVE is illegal (no timer set).
            (S::Active, T::TimerExpired) => None,
            // TimerExpired from ERASED is illegal (terminal).
            (S::Erased, T::TimerExpired) => None,
        }
    }
}

/// Canonical enumeration of every state variant (used by tests +
/// runtime taxonomy checks).
#[must_use]
pub const fn canonical_tenant_offboarding_states() -> [TenantOffboardingState; 6] {
    [
        TenantOffboardingState::Active,
        TenantOffboardingState::CancelRequested,
        TenantOffboardingState::GracePeriod,
        TenantOffboardingState::ReadOnly,
        TenantOffboardingState::Suspended,
        TenantOffboardingState::Erased,
    ]
}

/// Canonical enumeration of every trigger variant.
#[must_use]
pub const fn canonical_transition_triggers() -> [TransitionTrigger; 5] {
    [
        TransitionTrigger::CustomerInitiated,
        TransitionTrigger::CustomerReverted,
        TransitionTrigger::TimerExpired,
        TransitionTrigger::OpsForced,
        TransitionTrigger::AdminCommitErasure,
    ]
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
    fn active_has_full_caps_except_restore() {
        let c = TenantOffboardingState::Active.caps();
        assert!(c.read_allowed);
        assert!(c.write_allowed);
        assert!(c.admin_allowed);
        assert!(c.export_allowed);
        assert!(!c.restore_allowed);
    }

    #[test]
    fn read_only_blocks_writes_keeps_export() {
        let c = TenantOffboardingState::ReadOnly.caps();
        assert!(c.read_allowed);
        assert!(!c.write_allowed);
        assert!(c.export_allowed);
        assert!(c.restore_allowed);
    }

    #[test]
    fn suspended_blocks_export() {
        let c = TenantOffboardingState::Suspended.caps();
        assert!(!c.read_allowed);
        assert!(!c.export_allowed);
        assert!(c.admin_allowed); // ops force only
        assert!(!c.restore_allowed);
    }

    #[test]
    fn erased_is_terminal_zero_caps() {
        assert!(TenantOffboardingState::Erased.is_terminal());
        let c = TenantOffboardingState::Erased.caps();
        assert!(!c.read_allowed);
        assert!(!c.write_allowed);
        assert!(!c.admin_allowed);
        assert!(!c.export_allowed);
        assert!(!c.restore_allowed);
    }

    #[test]
    fn happy_path_is_canonical_timer_progression() {
        use TenantOffboardingState as S;
        use TransitionTrigger as T;
        assert_eq!(
            TenantOffboardingTransition::resolve(S::Active, T::CustomerInitiated),
            Some(S::CancelRequested)
        );
        assert_eq!(
            TenantOffboardingTransition::resolve(S::CancelRequested, T::TimerExpired),
            Some(S::GracePeriod)
        );
        assert_eq!(
            TenantOffboardingTransition::resolve(S::GracePeriod, T::TimerExpired),
            Some(S::ReadOnly)
        );
        assert_eq!(
            TenantOffboardingTransition::resolve(S::ReadOnly, T::TimerExpired),
            Some(S::Suspended)
        );
        assert_eq!(
            TenantOffboardingTransition::resolve(S::Suspended, T::AdminCommitErasure),
            Some(S::Erased)
        );
    }

    #[test]
    fn customer_revert_only_through_read_only() {
        use TenantOffboardingState as S;
        use TransitionTrigger as T;
        assert_eq!(
            TenantOffboardingTransition::resolve(S::CancelRequested, T::CustomerReverted),
            Some(S::Active)
        );
        assert_eq!(
            TenantOffboardingTransition::resolve(S::GracePeriod, T::CustomerReverted),
            Some(S::Active)
        );
        assert_eq!(
            TenantOffboardingTransition::resolve(S::ReadOnly, T::CustomerReverted),
            Some(S::Active)
        );
        // Suspended is past the canonical T+45 self-service revert
        // window; CustomerReverted is illegal.
        assert_eq!(
            TenantOffboardingTransition::resolve(S::Suspended, T::CustomerReverted),
            None
        );
    }

    #[test]
    fn timer_expired_on_suspended_does_not_advance_to_erased() {
        // Final erasure requires AdminCommitErasure (dry-run
        // confirmation). The timer alone is not sufficient — this
        // is intentional belt-and-braces against a runaway cron.
        use TenantOffboardingState as S;
        use TransitionTrigger as T;
        assert_eq!(
            TenantOffboardingTransition::resolve(S::Suspended, T::TimerExpired),
            None
        );
    }

    #[test]
    fn customer_initiated_from_non_active_is_illegal() {
        use TenantOffboardingState as S;
        use TransitionTrigger as T;
        for s in [S::CancelRequested, S::GracePeriod, S::ReadOnly, S::Suspended, S::Erased] {
            assert_eq!(
                TenantOffboardingTransition::resolve(s, T::CustomerInitiated),
                None,
                "CustomerInitiated from {s} must be illegal"
            );
        }
    }

    #[test]
    fn admin_commit_erasure_only_from_suspended() {
        use TenantOffboardingState as S;
        use TransitionTrigger as T;
        for s in [
            S::Active,
            S::CancelRequested,
            S::GracePeriod,
            S::ReadOnly,
            S::Erased,
        ] {
            assert_eq!(
                TenantOffboardingTransition::resolve(s, T::AdminCommitErasure),
                None,
                "AdminCommitErasure from {s} must be illegal"
            );
        }
    }

    #[test]
    fn canonical_state_count_is_six() {
        assert_eq!(canonical_tenant_offboarding_states().len(), 6);
    }

    #[test]
    fn canonical_trigger_count_is_five() {
        assert_eq!(canonical_transition_triggers().len(), 5);
    }

    #[test]
    fn state_str_round_trips_distinct() {
        let names: std::collections::HashSet<&str> = canonical_tenant_offboarding_states()
            .iter()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(names.len(), 6);
    }
}
