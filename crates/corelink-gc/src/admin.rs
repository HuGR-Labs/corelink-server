//! Manual admin trigger stub for `POST /v1/admin/gc/trigger`.
//!
//! WI-S06-001 §6.1.6 — staging stub only; production wiring lands
//! alongside the S-13 admin plane (which provides the rate limit +
//! audit emit + metric counter wiring).
//!
//! Canonical envelope:
//!
//! - **403** when the supplied PAT lacks the `gc:trigger` scope.
//! - **501** when the environment is not staging (`prod` returns
//!   501 until S-13 admin plane lands).
//! - **200** otherwise — the trigger acquires a fresh gc_run row
//!   per the canonical scheduler path.

use uuid::Uuid;

use crate::audit::{GcAuditRecord, GcAuditSink, GcEventType};
use crate::error::GcError;
use crate::region::GcRegion;
use crate::run::{GcPhase, GcStatus, RunId};

/// Canonical admin trigger outcome envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdminTriggerOutcome {
    /// 200 — trigger admitted; worker spawned with the supplied
    /// `run_id`.
    Admitted {
        /// The newly-minted run identifier.
        run_id: RunId,
    },
    /// 403 — PAT lacked the `gc:trigger` scope.
    Forbidden {
        /// PAT id hex prefix (no raw secret).
        pat_id_hex: String,
    },
    /// 501 — environment is not staging.
    NotImplemented {
        /// Environment label that rejected the trigger.
        environment: String,
    },
}

/// Drive the canonical staging-stub admin trigger.
///
/// # Parameters
///
/// - `pat_has_gc_trigger_scope` — caller-validated boolean (S-03 PAT
///   middleware decodes the `gc:trigger` bit; S-13 admin plane
///   re-validates).
/// - `pat_id_hex` — short hex prefix of the PAT id (audit forensics;
///   no raw secret).
/// - `environment` — current deployment environment (`"staging"` /
///   `"prod"` / `"dev"`).
/// - `tenant_id` — target tenant.
/// - `region` — target region.
/// - `audit` — audit sink for the canonical
///   `corelink.gc.run_started` (or `corelink.gc.run_aborted` for the
///   403 / 501 paths).
/// - `now_ms` — wall-clock instant.
/// - `run_id_seed` — caller-supplied seed for the minted run id
///   (production: UUIDv7 minted from the request id).
///
/// # Errors
///
/// Surfaces as [`GcError::UnauthorizedTrigger`] /
/// [`GcError::NotImplementedInEnvironment`] when the caller wants to
/// promote the structured envelope into the canonical error
/// taxonomy. The default envelope is returned via [`AdminTriggerOutcome`].
#[allow(
    clippy::too_many_arguments,
    reason = "canonical admin_trigger envelope per WI-S06-001 §6.1.6 — every parameter is a load-bearing input from the S-13 admin plane wiring; refactoring into a struct would obscure the canonical 403/200/501 contract"
)]
pub fn admin_trigger<A: GcAuditSink>(
    pat_has_gc_trigger_scope: bool,
    pat_id_hex: &str,
    environment: &str,
    tenant_id: Uuid,
    region: GcRegion,
    audit: &A,
    now_ms: u64,
    run_id_seed: u128,
) -> Result<AdminTriggerOutcome, GcError> {
    // 403 path — PAT scope check.
    if !pat_has_gc_trigger_scope {
        // Audit emit so the S-09 audit chain captures the denial.
        audit.emit(GcAuditRecord {
            event_type: GcEventType::RunAborted,
            run_id: RunId(Uuid::from_u128(run_id_seed)),
            tenant_id,
            region,
            status: GcStatus::Aborted,
            from_phase: Some(GcPhase::Idle),
            to_phase: None,
            created_by_request_id: pat_id_hex.to_owned(),
            reason: "admin_trigger_unauthorized",
            now_ms,
        })?;
        return Ok(AdminTriggerOutcome::Forbidden {
            pat_id_hex: pat_id_hex.to_owned(),
        });
    }

    // 501 path — non-staging env.
    let is_staging = matches!(environment, "staging" | "dev");
    if !is_staging {
        return Ok(AdminTriggerOutcome::NotImplemented {
            environment: environment.to_owned(),
        });
    }

    // 200 path — admit. Production wiring delegates to the canonical
    // scheduler entry point; the stub returns the minted run_id
    // directly.
    let rid = RunId(Uuid::from_u128(run_id_seed));
    audit.emit(GcAuditRecord {
        event_type: GcEventType::RunStarted,
        run_id: rid,
        tenant_id,
        region,
        status: GcStatus::Running,
        from_phase: Some(GcPhase::Idle),
        to_phase: None,
        created_by_request_id: pat_id_hex.to_owned(),
        reason: "admin_trigger_admitted",
        now_ms,
    })?;
    Ok(AdminTriggerOutcome::Admitted { run_id: rid })
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
    use crate::audit::InMemoryGcAuditSink;

    #[test]
    fn unauthorized_trigger_returns_forbidden() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            false,
            "pat_aaaa",
            "staging",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::Forbidden { .. }));
        let snap = audit.snapshot_of(GcEventType::RunAborted);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].reason, "admin_trigger_unauthorized");
    }

    #[test]
    fn prod_returns_not_implemented() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            true,
            "pat_aaaa",
            "prod",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::NotImplemented { .. }));
        // Audit emit for the 200 path is suppressed; the 403 path is
        // also not entered → audit sink remains empty.
        assert!(audit.is_empty());
    }

    #[test]
    fn staging_admits() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            true,
            "pat_aaaa",
            "staging",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::Admitted { .. }));
        let snap = audit.snapshot_of(GcEventType::RunStarted);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].reason, "admin_trigger_admitted");
    }

    #[test]
    fn dev_admits_too() {
        let audit = InMemoryGcAuditSink::new();
        let out = admin_trigger(
            true,
            "pat_aaaa",
            "dev",
            Uuid::nil(),
            GcRegion::Sam,
            &audit,
            123,
            0xaaaa,
        )
        .unwrap();
        assert!(matches!(out, AdminTriggerOutcome::Admitted { .. }));
    }
}
