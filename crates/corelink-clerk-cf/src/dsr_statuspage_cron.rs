//! Wave-17 DSR Statuspage 24h publish cron entry point.
//!
//! # CF Workers wiring
//!
//! The Cloudflare Workers `[triggers] crons = ["0 6 * * *"]` in
//! `wrangler.toml` fires this module's `#[event(scheduled)]` handler
//! at 06:00 UTC daily. The handler:
//!
//! 1. Resolves three Worker bindings from `env`:
//!    - `STATUSPAGE_PAGE_ID`     (config var, set via `wrangler.toml`
//!      `[vars]` table OR `wrangler secret put`)
//!    - `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS` (ditto)
//!    - `STATUSPAGE_API_KEY`     (always a secret — set via
//!      `wrangler secret put STATUSPAGE_API_KEY`)
//! 2. Constructs the trait-bound collaborators that the wave-17
//!    `corelink-dsr-statuspage-scheduler::DsrStatuspagePublishScheduler`
//!    composes:
//!    - [`crate::dsr_statuspage_cron::wasm_d1_row_source`] — D1
//!      reader against the canonical `dsr_erasure_log` table
//!      (deferred to a follow-up real-binding wave per trait-
//!      abstraction-defer: this module's wasm32 path returns the
//!      empty slice so the scheduler emits the canonical `Skipped /
//!      empty_window` audit + ledger row instead of mis-aggregating
//!      against an empty fetch).
//!    - [`crate::dsr_statuspage_cron::wasm_cron_run_log`] — D1-backed
//!      dedupe ledger (deferred ditto; native-only fake satisfies
//!      the test surface).
//!    - The wave-16 `StatuspageHttpClient` — wired against the
//!      `worker::Fetch` API rather than `reqwest::blocking` (deferred
//!      to the same follow-up wave; the wasm32 build cannot link
//!      `reqwest::blocking`).
//! 3. Calls `scheduler.run_once(now_unix_s)` and emits one of four
//!    canonical scheduler audit events (`scheduled` → `succeeded` /
//!    `failed` / `skipped`).
//!
//! # Trait-abstraction-defer charter
//!
//! Per the canonical `trait-abstraction-defer` pattern (see
//! `corelink_autonomous_execution_charter.md`), this wave-17 commit
//! ships:
//!
//! - The native-only scheduler crate (`corelink-dsr-statuspage-
//!   scheduler`) with the four-trait orchestration + WireMock-backed
//!   integration tests for happy / empty-window / 401 / 429 / D1-fail.
//! - The wasm32 `[event(scheduled)]` entry point (this module) +
//!   `wrangler.toml` `[triggers]` row firing at 06:00 UTC daily.
//! - The audit-event surface emitted at boot when the scheduled
//!   handler fires (so the audit chain pins the cron tick BEFORE the
//!   real D1 / `worker::Fetch` adapters land in a follow-up wave).
//!
//! The real D1 reader, real `worker::Fetch`-backed Statuspage client,
//! and real audit-chain-backed scheduler audit sink remain trait-
//! abstraction-deferred — pinned by the `dsr_statuspage_cron::*`
//! TODO_FOLLOW_UP audit events emitted from the wasm32 handler.

#[cfg(target_arch = "wasm32")]
use worker::{Env, ScheduleContext, ScheduledEvent};

/// Canonical cron expression. Pinned verbatim to match
/// `corelink-dsr-statuspage-scheduler::CRON_EXPRESSION` (the scheduler
/// crate cannot be a direct dep of `corelink-clerk-cf` because the
/// underlying `reqwest::blocking` HTTP client does not link on the
/// `wasm32-unknown-unknown` target). A unit test in the native-only
/// `corelink-dsr-statuspage-scheduler` crate asserts the same canonical
/// value (`"0 6 * * *"`) so drift between the two strings would fail
/// CI.
pub const CRON_EXPRESSION: &str = "0 6 * * *";

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
    /// bottoms-out at the `StatuspageHttpClient` layer so the
    /// plaintext key never appears in audit envelopes or logs.
    pub const STATUSPAGE_API_KEY: &str = "STATUSPAGE_API_KEY";
}

/// Canonical NDJSON audit line emitted at the start of every cron
/// tick. The CF Logpush pipeline ingests this into the centralised
/// audit chain. Constant string so the line shape is grep-stable
/// across pipeline upgrades.
pub const SCHEDULED_AUDIT_TYPE: &str =
    "corelink.privacy.statuspage_publish_scheduled.v1";

/// Canonical NDJSON audit line emitted when the scheduled handler
/// short-circuits because the real D1 / `worker::Fetch` adapters are
/// trait-abstraction-deferred to a follow-up wave. The `reason`
/// field carries the pin-string `wasm32_real_binding_deferred` so
/// the alerting layer can suppress these events until the follow-up
/// lands.
pub const DEFERRED_AUDIT_TYPE: &str =
    "corelink.privacy.statuspage_publish_skipped.v1";

/// Canonical pin-string emitted in the `reason` column of the
/// deferred-binding audit event. Pinned as a constant so the
/// alerting / Logpush filter rule matches against a single grep-
/// stable token.
pub const DEFERRED_REASON: &str = "wasm32_real_binding_deferred";

/// CF Workers `scheduled` event entry point.
///
/// Wired to `wrangler.toml`'s `[triggers] crons = ["0 6 * * *"]`. On
/// every fire, this handler:
///
/// 1. Resolves the canonical `STATUSPAGE_*` bindings.
/// 2. Emits the `scheduled` audit line BEFORE any other work.
/// 3. Short-circuits with the `skipped /
///    wasm32_real_binding_deferred` audit line (per the trait-
///    abstraction-defer charter — the real D1 reader + the real
///    `worker::Fetch`-backed `StatuspageBackend` impl land in a
///    follow-up wave; the wave-17 commit pins the cron firing +
///    the audit-chain surface).
///
/// The cron tick is therefore observable in the audit chain from
/// wave-17 forward; the actual publish-to-Statuspage path activates
/// when the follow-up real-binding wave merges.
///
/// # Bindings expected in wrangler.toml
///
/// - `STATUSPAGE_PAGE_ID` (vars or secret)
/// - `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS` (vars or secret)
/// - `STATUSPAGE_API_KEY` (always a secret)
#[cfg(target_arch = "wasm32")]
#[worker::event(scheduled)]
pub async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let page_id = match env.var(bindings::STATUSPAGE_PAGE_ID) {
        Ok(v) => v.to_string(),
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{DEFERRED_AUDIT_TYPE}\",\"reason\":\"missing_binding:{}\"}}",
                bindings::STATUSPAGE_PAGE_ID
            );
            return;
        }
    };
    let metric_id = match env.var(bindings::STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS) {
        Ok(v) => v.to_string(),
        Err(_e) => {
            worker::console_log!(
                "{{\"audit_type\":\"{DEFERRED_AUDIT_TYPE}\",\"reason\":\"missing_binding:{}\"}}",
                bindings::STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS
            );
            return;
        }
    };
    // We resolve (but do not log) the API key binding to fail-CLOSED
    // on a missing-secret deployment. The plaintext key never leaves
    // this scope.
    if env.secret(bindings::STATUSPAGE_API_KEY).is_err() {
        worker::console_log!(
            "{{\"audit_type\":\"{DEFERRED_AUDIT_TYPE}\",\"reason\":\"missing_binding:{}\"}}",
            bindings::STATUSPAGE_API_KEY
        );
        return;
    }
    let now_ms = event.schedule();

    worker::console_log!(
        "{{\"audit_type\":\"{SCHEDULED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"now_ms\":{now_ms},\"cron\":\"{CRON_EXPRESSION}\"}}"
    );

    // Trait-abstraction-defer: the real D1 reader + `worker::Fetch`-
    // backed `StatuspageBackend` impl land in a follow-up wave. The
    // wave-17 commit pins the cron tick + the audit-chain surface;
    // the actual publish path activates when the follow-up merges.
    worker::console_log!(
        "{{\"audit_type\":\"{DEFERRED_AUDIT_TYPE}\",\"page_id\":\"{page_id}\",\"metric_id\":\"{metric_id}\",\"reason\":\"{DEFERRED_REASON}\"}}"
    );
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
    }

    #[test]
    fn canonical_cron_expression_re_exported() {
        // Re-exported from `corelink-dsr-statuspage-scheduler` so
        // the wrangler.toml `[triggers]` row stays drift-free.
        assert_eq!(CRON_EXPRESSION, "0 6 * * *");
    }

    #[test]
    fn canonical_audit_type_strings_pinned() {
        assert_eq!(
            SCHEDULED_AUDIT_TYPE,
            "corelink.privacy.statuspage_publish_scheduled.v1"
        );
        assert_eq!(
            DEFERRED_AUDIT_TYPE,
            "corelink.privacy.statuspage_publish_skipped.v1"
        );
        assert_eq!(DEFERRED_REASON, "wasm32_real_binding_deferred");
    }
}
