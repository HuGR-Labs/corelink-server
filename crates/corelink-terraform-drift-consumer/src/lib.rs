//! `corelink-terraform-drift-consumer` — Terraform drift detection pipeline
//! (WI-S13-004).
//!
//! # What this crate ships
//!
//! Pure-logic skeleton for the drift consumer Worker that sits between
//! GitHub Actions cron and the D1 `terraform_drift_findings` audit table.
//!
//! ## Components
//!
//! 1. [`event`] — [`DriftPlanEvent`] (input from GH Actions webhook);
//!    [`DriftFinding`] (D1 row); [`DriftSeverity`]; [`DriftStatus`];
//!    [`RemediationDecision`]; Prometheus metric label types.
//!
//! 2. [`classifier`] — [`DriftClassifier`] trait +
//!    [`DefaultDriftClassifier`] (maps `diff_count` → severity;
//!    validates region; validates exit code).
//!
//! 3. [`store`] — [`DriftFindingStore`] trait +
//!    [`InMemoryDriftFindingStore`] (append-only; INV-AUDIT-APPEND-ONLY
//!    enforced: no DELETE; status updates append new fields only).
//!
//! 4. [`audit`] — [`DriftAuditSink`] trait + [`InMemoryDriftAuditSink`];
//!    CloudEvent types `corelink.admin.terraform_drift.detected` +
//!    `corelink.admin.terraform_drift.remediated`. Fail-CLOSED per
//!    INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
//!
//! 5. [`metrics`] — [`DriftMetrics`] struct; 4 canonical metric names
//!    per WI-S13-004 §6.1.5.
//!
//! 6. [`consumer`] — [`DriftConsumer`] orchestrator: receive plan event →
//!    classify severity → audit emit (before state mutation) → store
//!    INSERT → metrics increment.
//!
//! 7. [`error`] — [`DriftConsumerError`] `#[non_exhaustive]` taxonomy.
//!
//! # Security properties
//!
//! - **Auto-apply FORBIDDEN**: no `terraform apply` call anywhere in this
//!   crate. This is a hardened invariant (WI-S13-004 §7 anti-scope).
//! - **OIDC credentials**: crate is agnostic to credentials; production
//!   wiring uses OIDC-bound tokens only.
//! - **Append-only audit**: `DriftFindingStore` has no DELETE surface.
//!
//! # Production wiring (deferred)
//!
//! - Cloudflare Worker binding to D1 `terraform_drift_findings` table.
//! - GitHub Actions webhook → Worker HTTP endpoint.
//! - Prometheus metrics emission via CF Workers Analytics Engine.
//! - OTel trace spans per `admin.terraform_drift.{cron_run,alert_post,remediate}`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod audit;
pub mod classifier;
pub mod consumer;
pub mod error;
pub mod event;
pub mod metrics;
pub mod store;

pub use audit::{DriftAuditEventType, DriftAuditRecord, DriftAuditSink, InMemoryDriftAuditSink};
pub use classifier::{DefaultDriftClassifier, DriftClassifier};
pub use consumer::DriftConsumer;
pub use error::DriftConsumerError;
pub use event::{
    DriftFinding, DriftPlanEvent, DriftSeverity, DriftStatus, RemediationDecision, REGIONS,
};
pub use metrics::{DriftMetricOutcome, DriftMetrics};
pub use store::{DriftFindingStore, InMemoryDriftFindingStore};
