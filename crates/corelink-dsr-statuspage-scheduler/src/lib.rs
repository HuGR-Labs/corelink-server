//! `corelink-dsr-statuspage-scheduler` — wave-17 DSR Statuspage 24h
//! publish scheduler.
//!
//! # Purpose
//!
//! Wave-16 (commit `47e3442` per `specs/_audits/2026-05-15-dsr-worker-
//! production.md` §5) shipped the publish-path layers
//! (`corelink-statuspage-real` trait + transport + aggregator + bridge
//! + `corelink-privacy-erasure-worker::statuspage_publish` pure
//! aggregator). The actual SCHEDULER that fires the publish job once
//! per 24h, reads `STATUSPAGE_*` from Worker bindings, fetches a 24h
//! window of [`corelink_privacy_erasure_worker::VerificationOutcome`]
//! rows from the D1 `dsr_erasure_log` table, and composes
//! `aggregate_24h_window → bridge_to_report → publish_dsr_metric` was
//! deferred to WI-S11-008 PRR ship gate per the `trait-abstraction-
//! defer` charter. This crate ships that scheduler.
//!
//! # Wire surface
//!
//! - [`DsrStatuspagePublishScheduler`] — the composed entry point.
//!   `run_once(now_unix_s)` is the single canonical action fired by
//!   the CF Worker cron-trigger `0 6 * * *` (06:00 UTC daily).
//! - [`D1RowSource`] — trait surface for reading the last-24h slice of
//!   [`corelink_privacy_erasure_worker::VerificationOutcome`] rows
//!   from the canonical D1 `dsr_erasure_log` table. In-memory fake
//!   provided as [`InMemoryD1RowSource`].
//! - [`CronRunLog`] — trait surface for the
//!   `(date_yyyymmdd, metric_id)` idempotency dedupe table backing the
//!   D1 `cron_run_log` table. In-memory fake provided as
//!   [`InMemoryCronRunLog`].
//! - [`SchedulerAuditSink`] — trait surface for the canonical
//!   wave-17 four-event audit envelope:
//!   - `corelink.privacy.statuspage_publish_scheduled.v1` (start)
//!   - `corelink.privacy.statuspage_publish_succeeded.v1` (2xx)
//!   - `corelink.privacy.statuspage_publish_failed.v1` (fail-CLOSED)
//!   - `corelink.privacy.statuspage_publish_skipped.v1`
//!     (empty-window OR same-day-dedupe).
//!   In-memory fake provided as [`InMemorySchedulerAuditSink`].
//!
//! # Charter alignment
//!
//! - `#![forbid(unsafe_code)]`, no `unwrap` / `expect` / `panic` in
//!   `src/`, no `tokio` in `src/` (orchestration is sync-blocking; the
//!   wave-16 `StatuspageHttpClient` is `reqwest::blocking`).
//! - All public enums + structs are `#[non_exhaustive]`.
//! - Audit emit is fail-CLOSED on every path that mutates externally-
//!   visible state (`scheduled` BEFORE publish dispatch; `succeeded` /
//!   `failed` AFTER outcome resolution but BEFORE caller return).
//! - The Statuspage API key never appears plaintext in audit envelopes
//!   or logs — the wave-16 `redact_api_key` helper bottoms-out at the
//!   `StatuspageHttpClient` layer (this crate never sees the plaintext
//!   key in its audit envelope).
//! - Composition is **idempotent**: if the cron fires twice in the
//!   same UTC day (`YYYYMMDD` truncation of `now_unix_s`), the second
//!   call short-circuits before publish dispatch and emits the
//!   `skipped` audit with `SkipReason::AlreadyPublishedToday`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
// Doc comments use the canonical `+ continuation` / sub-item style
// (mirrors the wider CoreLink doc tradition where module headers list
// canonical taxonomies). Clippy's `doc_lazy_continuation` flags any
// continuation as a list item; the explicit allow keeps the existing
// doc style stable across the crate.
#![allow(clippy::doc_lazy_continuation)]

pub mod audit;
pub mod cron_log;
pub mod row_source;
pub mod scheduler;
// Wave-18 wasm32 real D1 row source. The native build also compiles
// the module (so the canonical SQL constant + the tenant-scope
// validator smoke test run on host CI), but the `D1Wasm32RowSource`
// struct itself is gated `#[cfg(target_arch = "wasm32")]` (it carries
// a `CfD1DatabaseReal` field whose inner `worker::D1Database` only
// exists on wasm32).
pub mod wasm32_row_source;

pub use audit::{
    InMemorySchedulerAuditSink, SchedulerAuditError, SchedulerAuditEvent, SchedulerAuditOutcome,
    SchedulerAuditSink, SkipReason,
};
pub use cron_log::{CronRunLog, CronRunLogError, InMemoryCronRunLog};
pub use row_source::{D1RowSource, D1RowSourceError, InMemoryD1RowSource};
pub use scheduler::{
    DsrStatuspagePublishScheduler, RunOutcome, SchedulerError, CRON_EXPRESSION,
    PUBLISH_WINDOW_SECONDS,
};
pub use wasm32_row_source::CRON_OUTCOME_QUERY;
#[cfg(target_arch = "wasm32")]
pub use wasm32_row_source::D1Wasm32RowSource;
