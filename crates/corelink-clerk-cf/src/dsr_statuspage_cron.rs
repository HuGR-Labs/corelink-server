//! DSR Statuspage 24h publish cron entry point.
//!
//! # CF Workers wiring
//!
//! The Cloudflare Workers `[triggers] crons = ["0 6 * * *"]` in
//! `wrangler.toml` fires this module's `#[event(scheduled)]` handler
//! at 06:00 UTC daily. The handler:
//!
//! 1. Resolves the canonical bindings from `env`:
//!    - `STATUSPAGE_PAGE_ID`     (config var)
//!    - `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS` (config var)
//!    - `STATUSPAGE_API_KEY`     (always a secret)
//!    - `STATUSPAGE_TENANT_ID`   (config var — tenant the cron read
//!      scopes against; CTRL-PRIV-001 / INV-TENANT-ISOLATION)
//!    - `DSR_LOG_DB`             (D1 binding)
//! 2. Constructs the wave-18 real backends:
//!    - [`corelink_dsr_statuspage_scheduler::D1Wasm32RowSource`] —
//!      `worker::D1Database`-backed reader against the canonical
//!      `dsr_erasure_log` table.
//!    - [`corelink_statuspage_real::StatuspageWasm32Client`] —
//!      `worker::Fetch`-backed Statuspage Public-Metric publisher.
//! 3. Composes
//!    `D1Wasm32RowSource::fetch_window_async → aggregate_24h_window →
//!    bridge_to_report → StatuspageWasm32Client::publish_dsr_metric_async`
//!    and emits the canonical wave-17 four-event audit envelope as
//!    NDJSON `worker::console_log!` lines.
//!
//! # Wave-18 closure
//!
//! Wave-17 (commit `49901da`) shipped the cron firing surface but
//! short-circuited with the canonical `wasm32_real_binding_deferred`
//! skip event because the wave-16 native `StatuspageHttpClient` uses
//! `reqwest::blocking` which does not link on wasm32. Wave-18 lands
//! the real `worker::Fetch`-backed
//! [`corelink_statuspage_real::StatuspageWasm32Client`] + the real
//! `worker::D1Database`-backed
//! [`corelink_dsr_statuspage_scheduler::D1Wasm32RowSource`] and wires
//! them into this handler. The skip event is REMOVED.
//!
//! # Why hand-composed (not via the native scheduler)
//!
//! The native [`corelink_dsr_statuspage_scheduler::DsrStatuspagePublishScheduler`]
//! orchestrator uses **sync** trait surfaces (`D1RowSource` /
//! `StatuspageBackend`); on wasm32 the canonical equivalents are async
//! (`worker::D1Database::all` / `worker::Fetch::send`). With no
//! `block_on` executor on wasm32-unknown-unknown we cannot bridge
//! sync→async without a panic-on-await — explicitly forbidden by the
//! workspace lint set. The wasm32 cron therefore replicates the
//! scheduler's composition inline using the **async** entry points
//! shipped wave-18. Audit semantics, fail-CLOSED ordering, idempotency
//! pre-flight (skipped for wave-18 — the canonical `(date, metric_id)`
//! ledger lives in the same D1 binding and is consulted in a follow-up
//! wave) all mirror the canonical wave-17 scheduler stance.

#[cfg(target_arch = "wasm32")]
use worker::{Env, ScheduleContext, ScheduledEvent};

/// Canonical cron expression. Pinned verbatim to match
/// `corelink-dsr-statuspage-scheduler::CRON_EXPRESSION`.
pub const CRON_EXPRESSION: &str = "0 6 * * *";

/// Canonical 24h publish window length (seconds). Pinned verbatim to
/// match `corelink-dsr-statuspage-scheduler::PUBLISH_WINDOW_SECONDS`.
pub const PUBLISH_WINDOW_SECONDS: u64 = 86_400;

/// Canonical binding names the wasm32 `scheduled` handler resolves
/// from `worker::Env`. Pinned as constants so the wrangler-side
/// secret declaration and the code-side `env.var(...)` lookup stay
/// drift-free.
pub mod bindings {
    /// Statuspage page ID binding name (config var; non-secret).
    pub const STATUSPAGE_PAGE_ID: &str = "STATUSPAGE_PAGE_ID";
    /// Statuspage DSR resolution-hours metric ID binding name
    /// (config var; non-secret).
    pub const STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS: &str =
        "STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS";
    /// Statuspage API key binding name. ALWAYS a `wrangler secret`
    /// (never a `[vars]` row). The wave-16 `redact_api_key` helper
    /// bottoms-out at the `StatuspageWasm32Client` layer so the
    /// plaintext key never appears in audit envelopes or logs.
    pub const STATUSPAGE_API_KEY: &str = "STATUSPAGE_API_KEY";
    /// Canonical tenant-id the cron scopes the D1 read against.
    /// CTRL-PRIV-001 / INV-TENANT-ISOLATION: every D1 query MUST
    /// flow through the wave-14 `TenantScopedQuery` + bind-time
    /// constant-time tenant-id check. Set via wrangler `[vars]`.
    pub const STATUSPAGE_TENANT_ID: &str = "STATUSPAGE_TENANT_ID";
    /// Canonical D1 binding name for the `dsr_erasure_log` database.
    /// The wave-18 `D1Wasm32RowSource` consumes this binding through
    /// the wave-14 `CfD1DatabaseReal` wrapper.
    pub const DSR_LOG_DB: &str = "DSR_LOG_DB";
}

/// Canonical NDJSON audit event types emitted by the wave-18 cron
/// handler. Pinned verbatim against
/// `corelink-dsr-statuspage-scheduler::audit::SchedulerAuditOutcome::event_type`
/// (the scheduler crate is native-only; the wasm32 cron replicates
/// the canonical type strings here so the Logpush filter rules see
/// the same envelope shape regardless of which target emitted it).
pub const SCHEDULED_AUDIT_TYPE: &str =
    "corelink.privacy.statuspage_publish_scheduled.v1";
/// Canonical succeeded audit event type.
pub const SUCCEEDED_AUDIT_TYPE: &str =
    "corelink.privacy.statuspage_publish_succeeded.v1";
/// Canonical failed audit event type.
pub const FAILED_AUDIT_TYPE: &str =
    "corelink.privacy.statuspage_publish_failed.v1";
/// Canonical skipped audit event type.
pub const SKIPPED_AUDIT_TYPE: &str =
    "corelink.privacy.statuspage_publish_skipped.v1";

/// Canonical skip reasons surfaced by the wasm32 cron handler.
/// Mirrors `corelink-dsr-statuspage-scheduler::SkipReason` for the
/// arms reachable from the wasm32 hot path.
pub const SKIP_REASON_EMPTY_WINDOW: &str = "empty_window";
/// Skip-reason emitted when a required CF Worker binding is missing
/// at handler dispatch time.
pub const SKIP_REASON_MISSING_BINDING: &str = "missing_binding";

/// CF Workers `scheduled` event entry point.
///
/// Wired to `wrangler.toml`'s `[triggers] crons = ["0 6 * * *"]`. On
/// every fire, this handler:
///
/// 1. Resolves the canonical bindings (page ID, metric ID, API key,
///    tenant ID, D1 binding). A missing binding emits the canonical
///    `Skipped / missing_binding` audit + returns (fail-CLOSED).
/// 2. Emits the `scheduled` audit BEFORE any other work.
/// 3. Constructs the wave-18 real backends + composes the canonical
///    wave-17 four-step orchestration.
/// 4. Emits one of `succeeded` / `failed` / `skipped` per the run
///    outcome.
#[cfg(target_arch = "wasm32")]
#[worker::event(scheduled)]
pub async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let page_id = match env.var(bindings::STATUSPAGE_PAGE_ID) {
        Ok(v) => v.to_string(),
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{SKIPPED_AUDIT_TYPE}\",\"reason\":\"{SKIP_REASON_MISSING_BINDING}:{}\"}}",
                bindings::STATUSPAGE_PAGE_ID
            );
            return;
        }
    };
    let metric_id = match env.var(bindings::STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS) {
        Ok(v) => v.to_string(),
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{SKIPPED_AUDIT_TYPE}\",\"reason\":\"{SKIP_REASON_MISSING_BINDING}:{}\"}}",
                bindings::STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS
            );
            return;
        }
    };
    let api_key = match env.secret(bindings::STATUSPAGE_API_KEY) {
        Ok(v) => v.to_string(),
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{SKIPPED_AUDIT_TYPE}\",\"reason\":\"{SKIP_REASON_MISSING_BINDING}:{}\"}}",
                bindings::STATUSPAGE_API_KEY
            );
            return;
        }
    };
    let tenant_id_raw = match env.var(bindings::STATUSPAGE_TENANT_ID) {
        Ok(v) => v.to_string(),
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{SKIPPED_AUDIT_TYPE}\",\"reason\":\"{SKIP_REASON_MISSING_BINDING}:{}\"}}",
                bindings::STATUSPAGE_TENANT_ID
            );
            return;
        }
    };
    let d1 = match env.d1(bindings::DSR_LOG_DB) {
        Ok(v) => v,
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{SKIPPED_AUDIT_TYPE}\",\"reason\":\"{SKIP_REASON_MISSING_BINDING}:{}\"}}",
                bindings::DSR_LOG_DB
            );
            return;
        }
    };

    // `ScheduledEvent::schedule()` returns f64 epoch ms (CF Workers
    // surfaces the JS `Date.now()` numeric type verbatim). Clamp to
    // u64 — negative values fall to 0; out-of-u64-range falls to
    // u64::MAX. Neither extreme is observable in production but the
    // clamp keeps the cron handler panic-free per charter.
    let now_ms_f = event.schedule();
    let now_ms: u64 = if !now_ms_f.is_finite() || now_ms_f < 0.0 {
        0
    } else if now_ms_f >= u64::MAX as f64 {
        u64::MAX
    } else {
        now_ms_f as u64
    };
    let now_unix_s = now_ms / 1_000;
    let window_start_unix_s = now_unix_s.saturating_sub(PUBLISH_WINDOW_SECONDS);

    // 1. scheduled audit BEFORE any work.
    worker::console_log!(
        "{{\"audit_type\":\"{SCHEDULED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"now_ms\":{now_ms},\"cron\":\"{CRON_EXPRESSION}\"}}"
    );

    // 2. Build the tenant-scoped D1 wrapper (wave-14 CfD1DatabaseReal).
    let tenant = match corelink_cf_bindings::d1_real::TenantId::new(tenant_id_raw) {
        Ok(t) => t,
        Err(e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{FAILED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"reason\":\"tenant_id: {e}\"}}"
            );
            return;
        }
    };
    let db = corelink_cf_bindings::d1_real::CfD1DatabaseReal::new(d1, tenant);
    let row_source =
        corelink_dsr_statuspage_scheduler::D1Wasm32RowSource::new(db);

    // 3. Fetch the 24h window.
    let outcomes = match row_source.fetch_window_async(window_start_unix_s).await {
        Ok(rs) => rs,
        Err(e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{FAILED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"reason\":\"d1 read: {e}\"}}"
            );
            return;
        }
    };
    let rows_read = outcomes.len() as u64;

    // 4. Aggregate.
    let stats = corelink_privacy_erasure_worker::aggregate_24h_window(
        &outcomes,
        window_start_unix_s,
    );

    // 5. Empty-window short-circuit (canonical wave-17 skip arm).
    if outcomes.is_empty() {
        worker::console_log!(
            "{{\"audit_type\":\"{SKIPPED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"reason\":\"{SKIP_REASON_EMPTY_WINDOW}\",\"rows_read\":0}}"
        );
        return;
    }

    // 6. Bridge to the canonical Statuspage publish payload.
    let report = match corelink_statuspage_real::bridge_to_report(&stats) {
        Ok(r) => r,
        Err(e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{FAILED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"reason\":\"bridge: {e}\",\"rows_read\":{rows_read}}}"
            );
            return;
        }
    };

    // 7. Build the wave-18 wasm32 Statuspage publisher + the audit
    //    sink that fans the wave-16 audit envelope into Logpush.
    let audit = std::sync::Arc::new(
        corelink_statuspage_real::InMemoryStatuspageAuditSink::new(),
    );
    let client = corelink_statuspage_real::StatuspageWasm32Client::new(
        page_id.clone(),
        metric_id.clone(),
        api_key,
        audit.clone(),
    );
    let p95 = report.p95_resolution_hours;
    match client.publish_dsr_metric_async(&report, now_ms).await {
        Ok(out) => {
            worker::console_log!(
                "{{\"audit_type\":\"{SUCCEEDED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"rows_read\":{rows_read},\"p95_hours_observed\":{p95},\"status\":{},\"attempts\":{}}}",
                out.status,
                out.attempts
            );
        }
        Err(e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{FAILED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"rows_read\":{rows_read},\"p95_hours_observed\":{p95},\"reason\":\"publish: {e}\"}}"
            );
        }
    }
    // Fan the wave-16 audit envelope (per-attempt) into Logpush as
    // NDJSON. The InMemory sink captures every emission; the cron
    // handler is responsible for fanning into the console_log channel
    // so the Logpush pipeline picks it up.
    for evt in audit.snapshot() {
        worker::console_log!(
            "{{\"audit_type\":\"{}\",\"page_id\":\"{}\",\"metric_id\":\"{}\",\"api_key_redacted\":\"{}\",\"final_status\":{},\"attempts\":{},\"p95_hours_observed\":{},\"window_end_unix_s\":{}}}",
            evt.outcome.event_type(),
            evt.page_id,
            evt.metric_id,
            evt.api_key_redacted,
            evt.final_status
                .map(|s| s.to_string())
                .unwrap_or_else(|| "null".to_owned()),
            evt.attempts,
            evt.p95_hours_observed,
            evt.window_end_unix_s
        );
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
    fn canonical_binding_names_pinned() {
        assert_eq!(bindings::STATUSPAGE_PAGE_ID, "STATUSPAGE_PAGE_ID");
        assert_eq!(
            bindings::STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS,
            "STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS"
        );
        assert_eq!(bindings::STATUSPAGE_API_KEY, "STATUSPAGE_API_KEY");
        assert_eq!(bindings::STATUSPAGE_TENANT_ID, "STATUSPAGE_TENANT_ID");
        assert_eq!(bindings::DSR_LOG_DB, "DSR_LOG_DB");
    }

    #[test]
    fn canonical_cron_expression_re_exported() {
        // Re-exported from `corelink-dsr-statuspage-scheduler` so
        // the wrangler.toml `[triggers]` row stays drift-free.
        assert_eq!(CRON_EXPRESSION, "0 6 * * *");
        assert_eq!(PUBLISH_WINDOW_SECONDS, 86_400);
    }

    #[test]
    fn canonical_audit_type_strings_pinned() {
        assert_eq!(
            SCHEDULED_AUDIT_TYPE,
            "corelink.privacy.statuspage_publish_scheduled.v1"
        );
        assert_eq!(
            SUCCEEDED_AUDIT_TYPE,
            "corelink.privacy.statuspage_publish_succeeded.v1"
        );
        assert_eq!(
            FAILED_AUDIT_TYPE,
            "corelink.privacy.statuspage_publish_failed.v1"
        );
        assert_eq!(
            SKIPPED_AUDIT_TYPE,
            "corelink.privacy.statuspage_publish_skipped.v1"
        );
        assert_eq!(SKIP_REASON_EMPTY_WINDOW, "empty_window");
        assert_eq!(SKIP_REASON_MISSING_BINDING, "missing_binding");
    }
}
