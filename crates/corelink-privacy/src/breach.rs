//! `corelink-privacy-breach-emit` — Breach notification audit event emitter
//! (WI-S11-006 — S-11 Privacy Pipeline HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! The canonical audit-event emitter for the breach notification runbook
//! (RB-BREACH-NOTIF). Per WI-S11-006 §6.1 item 7, every regulatory or
//! customer notification dispatch MUST be followed by a durable
//! `dev.hugr.corelink.breach.notification_dispatched.v1` CloudEvent
//! written to the R2 `audit-<region>` bucket with Object Lock 7y
//! (INV-AUDIT-APPEND-ONLY §3.6 L116 CRITICAL).
//!
//! Specifically, the crate ships:
//!
//! 1. [`event::BreachSeverity`] `#[non_exhaustive]` 3-arm enum
//!    (Sev1 / Sev2 / Sev3) — typed severity taxonomy per RB-BREACH-NOTIF §2.
//! 2. [`event::NotificationJurisdiction`] `#[non_exhaustive]` 3-arm enum
//!    (Anpd / IrishDpc / CaliforniaAg) — typed jurisdiction taxonomy per
//!    `rb-breach-notif-decision-tree.yaml` §1.
//! 3. [`event::CustomerLocale`] `#[non_exhaustive]` 3-arm enum
//!    (PtBr / EnUs / EsMx) — mandatory 3-locale customer notification per
//!    sprint contract §10.s11.7 + LGPD Art. 9.
//! 4. [`event::BreachNotificationDispatch`] — canonical audit payload:
//!    `breach_id` (String ULID) + `severity` + `jurisdictions_notified`
//!    + `customer_notifications_sent` + `customer_locales` + `ts_ms` +
//!    `retry_attempt` + `breach_detected_at_ms`.
//! 5. [`event::EscalationPolicy`] — typed escalation policy for each
//!    severity tier: who is paged + at what timeout offset.
//! 6. [`event::escalation_policy_for`] — deterministic fn: severity →
//!    canonical escalation policy (test-pinned; property test 100k combos).
//! 7. [`audit_emit::BreachAuditSink`] trait — `emit()` returns
//!    `Result<(), BreachAuditSinkError>`.
//! 8. [`audit_emit::InMemoryBreachAuditSink`] — capture sink for tests.
//! 9. [`audit_emit::FailingBreachAuditSink`] — adversarial always-failing
//!    sink for chaos tests of the fail-CLOSED-ish envelope.
//! 10. [`metrics`] module — 4 canonical Prometheus metric names + helpers:
//!     `corelink_breach_time_to_notification_seconds`,
//!     `corelink_breach_time_to_decision_seconds`,
//!     `corelink_breach_dry_run_completion_total`,
//!     `corelink_breach_customer_notification_delivery_total`.
//! 11. [`error`] module — [`error::BreachEmitError`] + [`error::BreachAuditSinkError`]
//!     `#[non_exhaustive]` taxonomies.
//!
//! # Audit fail behavior (WI-S11-006 §9.3 DD)
//!
//! Distinct from billing fail-OPEN (Lote 10.6bis split-tier): breach audit
//! failure does NOT block or rollback the regulatory notification dispatch.
//! The 72h regulatory SLA (GDPR Art. 33 + LGPD Art. 48 + CCPA §1798.82)
//! takes priority over audit trail completeness when they conflict. On
//! audit emit failure:
//!
//! 1. Dispatch proceeds (regulatory SLA priority).
//! 2. SEV-1 alert fires to Security Lead + Compliance.
//! 3. Manual re-emit queued: same payload + `retry_attempt` incremented.
//! 4. `breach_id` is the idempotency anchor — same `breach_id` +
//!    same `severity` + same `jurisdictions_notified` = same event
//!    semantically; `retry_attempt` distinguishes emission attempts.
//!
//! This is documented-priority behavior, NOT silent-skip.
//!
//! # Audit ordering (lookup → emit → mutate per S-06/S-07 lessons)
//!
//! The `BreachAuditSink::emit()` call MUST happen BEFORE any state
//! transition (e.g., marking a `dsr_ticket` as `breach_notified`).
//! Test [`regression_audit_fail_closed_behavior`] verifies state
//! UNCHANGED when emit fails.
//!
//! # wasm32-unknown-unknown clean
//!
//! No `ring`, `jsonwebtoken`, or C toolchain dependency. `uuid` pulled
//! without the `v7` getrandom feature (caller supplies `breach_id` as
//! String ULID at incident declaration; tests use deterministic bytes).
//!
//! # F-001 closure
//!
//! Per-instance `Arc<Mutex<>>` in [`audit_emit::InMemoryBreachAuditSink`].
//! NEVER `static LazyLock<Mutex<>>`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
// Suppress doc_lazy_continuation for `+ continuation` style in doc comments.
#![allow(clippy::doc_lazy_continuation)]
// W35-P2: inherited `//!` docs from the absorbed sibling crate use
// short paths that resolved at the former crate root.
#![allow(rustdoc::broken_intra_doc_links)]

pub mod audit_emit;
pub mod error;
pub mod event;
pub mod metrics;

pub use audit_emit::{BreachAuditSink, FailingBreachAuditSink, InMemoryBreachAuditSink};
pub use error::{BreachAuditSinkError, BreachEmitError};
pub use event::{
    escalation_policy_for, BreachNotificationDispatch, BreachSeverity, CustomerLocale,
    EscalationPolicy, EscalationStep, NotificationJurisdiction, CUSTOMER_LOCALES_MANDATORY,
    JURISDICTION_COUNT, SEVERITY_COUNT,
};
pub use metrics::{
    METRIC_BREACH_CUSTOMER_NOTIFICATION_DELIVERY_TOTAL, METRIC_BREACH_DRY_RUN_COMPLETION_TOTAL,
    METRIC_BREACH_TIME_TO_DECISION_SECONDS, METRIC_BREACH_TIME_TO_NOTIFICATION_SECONDS,
};
