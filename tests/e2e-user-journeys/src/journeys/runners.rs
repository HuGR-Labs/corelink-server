//! # Runners entitlement journeys (gap-map M8).
//!
//! STUB (F-B scaffold). Owner WP: **W2**. Must prove the Runners concurrency
//! entitlement is ENFORCED end-to-end: a Runners-tier purchase seeds a
//! `runners_entitlement` row, the fabric ADMITS a runner under the concurrency
//! cap, and REJECTS over it (no free unlimited concurrency = revenue leak; no
//! false-reject = angry customer). The introspect journeys only prove the
//! introspect endpoint answers — not that admit/over-cap actually gate.
//!
//! Contract: `cfg.token(TokenKind::Runner)` + `cfg.runner_tenant`
//! (`CORELINK_E2E_PAT_RUNNER` / `CORELINK_E2E_RUNNER_TENANT`). Assert the seeded
//! cap value and the admit/reject boundary, not just a 200.

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

/// Run the Runners entitlement journeys. STUB → one gated row until W2 fills it.
pub fn run(_cfg: &Config, _client: &Client) -> Vec<JourneyResult> {
    vec![JourneyResult::gated(
        "Runners: admit-under-cap → reject-over-cap + entitlement seed — M8",
        "TODO(W2): assert runners_entitlement seeded + concurrency admit/over-cap boundary",
    )]
}
