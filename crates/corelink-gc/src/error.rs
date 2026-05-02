//! Canonical [`GcError`] taxonomy surfaced by every fallible API in
//! the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 lesson — the
//! variant set grows additively across S-06 follow-on WIs (mark / sweep
//! / physical-delete / reconcile) without breaking downstream callers.

use thiserror::Error;

use crate::audit::GcAuditSinkError;
use crate::degrade::{DegradeKind, DegradeProbeError};
use crate::metrics::GcMetricsObserverError;
use crate::run::GcRunStoreError;

/// Canonical errors surfaced by [`crate::worker::GcWorker`] +
/// [`crate::scheduler::GcScheduler`] + admin trigger entry points.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GcError {
    /// Degrade-mode `gc-pause` (or stricter) is active. Worker MUST
    /// abort within ≤ 100 ms (next batch boundary) per WI §6.1.5;
    /// scheduler MUST refuse new tick admission per WI §10.s06.001.5.
    #[error("degrade mode active: {0:?}")]
    DegradeModeActive(DegradeKind),
    /// `gc_run` checkpoint table backend error (D1 INSERT / UPDATE
    /// failure or simulator constraint violation).
    #[error("gc_run store error: {0}")]
    RunStore(#[from] GcRunStoreError),
    /// Audit sink error (audit_outbox INSERT failure / SIEM webhook).
    /// WI §6.1.8 — every phase transition emits an audit record;
    /// failure to emit MUST surface to the caller (fail-closed:
    /// callers map this to 503 to preserve the `audit_emit ⇔ handler`
    /// atomicity envelope `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    #[error("audit sink error: {0}")]
    Audit(#[from] GcAuditSinkError),
    /// Metrics emit failure (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit). Caller may
    /// downgrade to log-and-continue per WI §14.s06.001.6.
    #[error("metrics observer error: {0}")]
    Metrics(#[from] GcMetricsObserverError),
    /// Degrade-mode probe backend error (config-singleton DO read
    /// failure). Caller MUST fail-closed (treat unprobeable degrade
    /// state as `gc-pause`) per WI §6.1.5 — silently proceeding could
    /// continue mark/sweep during an operator-declared incident.
    #[error("degrade probe error: {0}")]
    DegradeProbe(#[from] DegradeProbeError),
    /// Admin PAT lacks the `gc:trigger` scope. WI §6.1.6 + §8 (Gherkin
    /// "Manual trigger unauthorized").
    #[error("admin trigger unauthorized: PAT {pat_id_hex} lacks gc:trigger scope")]
    UnauthorizedTrigger {
        /// Hex prefix of the PAT id (no raw secret material; aligned
        /// with `corelink-audit::PatIdHash` redaction discipline).
        pat_id_hex: String,
    },
    /// Manual trigger requested in a non-staging environment. WI
    /// §6.1.6 — staging stub only; production wiring lands alongside
    /// the S-13 admin plane.
    #[error("manual trigger not implemented in {environment}")]
    NotImplementedInEnvironment {
        /// Environment label that rejected the trigger (e.g. `prod`).
        environment: String,
    },
    /// Phase transition violated [`crate::run::GcPhase`] monotonicity.
    /// Programmer error — every transition is checked at the
    /// [`crate::run::InMemoryGcRunStore`] seam, but a mis-wired
    /// adapter could surface this. Mapped to 5xx by handler.
    #[error("invalid phase transition: {from} -> {to}")]
    InvalidPhaseTransition {
        /// Previous phase recorded in the row.
        from: &'static str,
        /// Attempted next phase.
        to: &'static str,
    },
    /// `mark_started_at_ms` was set ONCE at Mark phase start (per
    /// `INV-GC-MARK-STARTED-AT-IMMUTABLE`); a second set attempt is a
    /// programmer error — the column MUST NOT be re-written by any
    /// subsequent UPDATE.
    #[error("mark_started_at_ms is immutable once set: existing={existing} attempted={attempted}")]
    MarkStartedAtImmutable {
        /// Existing immutable anchor.
        existing: u64,
        /// Attempted overwrite.
        attempted: u64,
    },
    /// Caller supplied a tenant batch size > [`crate::scheduler::MAX_TENANTS_PER_TICK`].
    /// Programmer wiring error.
    #[error(
        "tenants_per_tick {requested} exceeds canonical ceiling {ceiling}"
    )]
    TenantsPerTickExceeded {
        /// Caller-requested batch size.
        requested: usize,
        /// Canonical ceiling.
        ceiling: usize,
    },
    /// Scheduler config has invalid jitter (>
    /// [`crate::schedule::MAX_JITTER_MINUTES`]).
    #[error(
        "jitter_minutes {requested} exceeds canonical ceiling {ceiling}"
    )]
    JitterMinutesExceeded {
        /// Requested jitter.
        requested: u32,
        /// Canonical ceiling.
        ceiling: u32,
    },
}
