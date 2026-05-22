//! OTLP export primitives — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-otel-export`
//! (enterprise observability fan-out: Datadog, OTel Collector, Grafana
//! Cloud). The actual implementation lives in
//! `crates/corelink-otel-export/` (Stage 0 sub-step 3 Option-A
//! aggregator pattern; see crate-level rustdoc).

pub use corelink_otel_export::*;

/// Module-path marker used by the crate-level smoke tests.
#[must_use]
pub const fn module_path_marker() -> &'static str {
    "corelink_telemetry::otel"
}
