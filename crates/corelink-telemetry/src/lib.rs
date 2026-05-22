//! `corelink-telemetry` — canonical observability surface for the
//! CoreLink Rust workspace.
//!
//! Wave-33 Stage 0 sub-step 3 lands this crate as the **single import
//! target** Stage 1 streams will use for every telemetry primitive:
//!
//! ```text
//! use corelink_telemetry::tracing::*;          // structured tracing fields
//! use corelink_telemetry::logpush::*;          // CF Logpush sink
//! use corelink_telemetry::otel::*;             // OTLP export
//! use corelink_telemetry::canary::*;           // canary rollout signals
//! use corelink_telemetry::synthetic_pager::*;  // synthetic pager drill
//! use corelink_telemetry::slo::*;              // SLO emit + Sli observer
//! use corelink_telemetry::lighthouse::*;       // lighthouse customer tracker
//! ```
//!
//! ## Stage 0 absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 sub-step
//! 3, this crate "absorbs" 7 existing telemetry crates. Following the
//! same Option-A pattern established by sub-step 2 (`corelink-crypto`),
//! this Stage 0 commit lands corelink-telemetry as a re-export
//! aggregator rather than a physical source move:
//!
//! - `corelink-tracing` → [`tracing`]
//! - `corelink-logpush` → [`logpush`]
//! - `corelink-otel-export` → [`otel`]
//! - `corelink-canary` → [`canary`]
//! - `corelink-synthetic-pager` → [`synthetic_pager`]
//! - `corelink-slo` → [`slo`]
//! - `corelink-lighthouse-tracker` → [`lighthouse`]
//!
//! Why aggregator rather than physical move:
//!
//! - The 7 absorbed crates are consumed pervasively across CoreLink
//!   (≥20 dependent crates). Physical absorption would require an
//!   atomic update of every consumer's `use` statements; that is the
//!   work Stage 1 Stream C (infra+ops) owns per the wave-33 reorg
//!   spec §6 Stage 1.
//! - The 7 crates retain their own tests + benches + examples that
//!   reference internal-crate types. Physical absorption would
//!   require rewriting those harnesses to use `corelink_telemetry::*`
//!   paths — pure churn with no behavioural payoff at Stage 0.
//!
//! Stage 1 Stream C will execute the physical move atomically with
//! consumer migration so the workspace doesn't carry a transient
//! "two-path" state across multi-day work.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 7 absorbed crates remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken. Stage 1 streams MAY adopt the new canonical
//! paths incrementally without coordination cost.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod canary;
pub mod lighthouse;
pub mod logpush;
pub mod otel;
pub mod slo;
pub mod synthetic_pager;
pub mod tracing;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Compile-only smoke check: each canonical re-export submodule
    //! resolves. The actual type-level assertions live in the
    //! absorbed crates' own test suites (preserved unchanged in
    //! Stage 0 Option-A aggregator pattern).
    //!
    //! A single test references all 7 submodule paths via
    //! `module_path!()` constants — this forces the compiler to
    //! resolve every path while side-stepping the unused-import lint
    //! that more idiomatic `use crate::*` checks would trigger.

    // Compile-time path references. If any aggregator submodule
    // fails to compile (e.g. an underlying crate gets renamed or
    // deleted) these resolve-paths would surface as a `cargo test`
    // build error. Functions are unused at runtime — their value is
    // the type-check on each path.
    #[allow(dead_code)]
    fn _resolve_tracing() -> &'static str {
        crate::tracing::module_path_marker()
    }
    #[allow(dead_code)]
    fn _resolve_logpush() -> &'static str {
        crate::logpush::module_path_marker()
    }
    #[allow(dead_code)]
    fn _resolve_otel() -> &'static str {
        crate::otel::module_path_marker()
    }
    #[allow(dead_code)]
    fn _resolve_canary() -> &'static str {
        crate::canary::module_path_marker()
    }
    #[allow(dead_code)]
    fn _resolve_synthetic_pager() -> &'static str {
        crate::synthetic_pager::module_path_marker()
    }
    #[allow(dead_code)]
    fn _resolve_slo() -> &'static str {
        crate::slo::module_path_marker()
    }
    #[allow(dead_code)]
    fn _resolve_lighthouse() -> &'static str {
        crate::lighthouse::module_path_marker()
    }

    #[test]
    fn all_seven_submodules_publish_a_marker() {
        // Each aggregator submodule publishes a tiny `module_path_marker`
        // const so this smoke test has something concrete to assert on
        // beyond compile resolution.
        assert_eq!(crate::tracing::module_path_marker(), "corelink_telemetry::tracing");
        assert_eq!(crate::logpush::module_path_marker(), "corelink_telemetry::logpush");
        assert_eq!(crate::otel::module_path_marker(), "corelink_telemetry::otel");
        assert_eq!(crate::canary::module_path_marker(), "corelink_telemetry::canary");
        assert_eq!(
            crate::synthetic_pager::module_path_marker(),
            "corelink_telemetry::synthetic_pager"
        );
        assert_eq!(crate::slo::module_path_marker(), "corelink_telemetry::slo");
        assert_eq!(
            crate::lighthouse::module_path_marker(),
            "corelink_telemetry::lighthouse"
        );
    }
}
