//! # Journey modules — one file per surface, each fillable by one agent.
//!
//! Each module exposes `pub fn run(cfg: &Config, client: &Client) ->
//! Vec<JourneyResult>`. The runner concatenates them all via [`all`].
//!
//! Modules that are still stubs return a single `Gated("TODO: …")` row so the
//! skeleton compiles and runs today; the fleet replaces the stub `run()` with
//! real black-box journeys (using ONLY the harness URL builders + helpers).
//!
//! IMPLEMENTED at harness-freeze (the 7 migrated journeys, URLs corrected):
//!   - `identity`  — onboarding/ping + auth-rejection
//!   - `cas`       — cache miss→hit + tenant isolation (adversarial)
//!   - `bazel`     — Bazel CLI round-trip (CLI-gated)
//!   - `audit`     — audit export + chain re-derive
//!   - `quota`     — quota hard-cap (slow-gated)
//!
//! STUBS awaiting the fleet:
//!   - `ac`, `turbo`, `adapters`, `dashboard`, `pat_lifecycle`, `billing`,
//!     `dsr`, `introspect`

use reqwest::blocking::Client;

use crate::harness::{Config, JourneyResult};

pub mod abuse;
pub mod ac;
pub mod adapters;
pub mod audit;
pub mod bazel;
pub mod billing;
pub mod cas;
pub mod concurrency;
pub mod dashboard;
pub mod dsr;
pub mod edge;
pub mod identity;
pub mod introspect;
pub mod oci;
pub mod oci_public_isolation;
pub mod pat_lifecycle;
pub mod quota;
pub mod runner_purchase;
pub mod runners;
pub mod security;
pub mod shared_cache;
pub mod team;
pub mod turbo;

/// Run every journey module and concatenate the results, in a stable order.
pub fn all(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    let mut out = Vec::new();
    // Pre-warm the scale-to-zero OCI host ONCE, before any module — the
    // `adapters` module's OCI journeys run before `oci`, so a warmup local to
    // `oci::run` is too late for them. A cold OCI container otherwise eats a
    // >30s cold-start on the first hit and flakes the gate RED (see warm_oci).
    let _ = oci::warm_oci(cfg, client);
    out.extend(identity::run(cfg, client));
    out.extend(cas::run(cfg, client));
    out.extend(concurrency::run(cfg, client));
    out.extend(ac::run(cfg, client));
    out.extend(bazel::run(cfg, client));
    out.extend(turbo::run(cfg, client));
    out.extend(adapters::run(cfg, client));
    out.extend(oci::run(cfg, client));
    out.extend(oci_public_isolation::run(cfg, client));
    out.extend(dashboard::run(cfg, client));
    out.extend(pat_lifecycle::run(cfg, client));
    out.extend(billing::run(cfg, client));
    out.extend(quota::run(cfg, client));
    out.extend(dsr::run(cfg, client));
    out.extend(introspect::run(cfg, client));
    out.extend(audit::run(cfg, client));
    out.extend(security::run(cfg, client));
    out.extend(edge::run(cfg, client));
    out.extend(abuse::run(cfg, client));
    out.extend(shared_cache::run(cfg, client));
    out.extend(runners::run(cfg, client));
    out.extend(runner_purchase::run(cfg, client));
    out.extend(team::run(cfg, client));
    out
}
