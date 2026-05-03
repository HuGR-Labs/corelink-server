//! `corelink-canary` — synthetic canary 3-region probe orchestrator
//! (WI-S09-007 — S-09 ship gate, HIGH_RISK lane).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the synthetic canary loop the production CF Workers
//! cron-trigger will satisfy (real `worker::Fetch` HTTP client + real
//! R2 PUT + GET + AC lookup + BLAKE3 digest verify + Mimir / Loki /
//! Tempo / Grafana dashboard health probes), plus an in-memory
//! orchestrator that exercises every load-bearing invariant the
//! production wiring relies on. Property tests pinned at 10k iter
//! against the orchestrator cover the canonical assertion ladder
//! (`cas_put_p99_ms ≤ 100` / `cas_get_p99_ms ≤ 50` /
//! `ac_lookup_p99_ms ≤ 30`), the BLAKE3 digest match correctness
//! invariant, the cron drift detection (> 90s = SEV-3 alert source),
//! the synthetic tenant SLI exclusion (Lote 10.8bis P1-NEW-3
//! inheritance from WI-S08-005 ManualOverride pattern), and the
//! audit-emit-BEFORE-mutation fail-CLOSED envelope on every emit arm
//! per Lote 10.6bis pattern + S-07 P1-1 fix.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`region`] module ships [`CanaryRegion`] `#[non_exhaustive]`
//!    3-canonical (`Enam` / `Weur` / `Apac` per `data_model.md §2.1`
//!    R2 region hints; Lote 10.9bis P1 R4 P1-10 corrected from IATA
//!    colocodes). Each canary region maps to a parent
//!    [`corelink_analytics::Region`] for cross-crate label cartesian
//!    consistency without label drift.
//! 2. The [`assertion`] module ships [`CanaryAssertion`]
//!    `#[non_exhaustive]` 4-canonical (`CasPutP99` / `CasGetP99` /
//!    `AcLookupP99` / `DigestMatch`) + canonical SLO ceilings
//!    (`cas_put_p99_ms ≤ 100` / `cas_get_p99_ms ≤ 50` /
//!    `ac_lookup_p99_ms ≤ 30`).
//! 3. The [`result`] module ships [`CanaryLoopResult`] +
//!    [`CanaryDecision`] `#[non_exhaustive]` 3-canonical (`Pass` /
//!    `Degraded` / `FailedRegion`) + [`ObservabilityHealthReport`]
//!    + [`HealthComponent`] `#[non_exhaustive]` 4-canonical (Mimir /
//!    Loki / Tempo / Dashboard) per WI §1 invariant 3.
//! 4. The [`probe`] module ships [`CanaryProbe`] trait +
//!    [`InMemoryCanaryProbe`] (per-instance `Arc<Mutex<>>` per-region
//!    ledger; F-001 closure; audit-emit-BEFORE-mutation fail-CLOSED
//!    envelope on every emit arm) + [`FailingCanaryProbe`]
//!    adversarial fixture.
//! 5. The [`audit`] module ships [`CanaryAuditEventType`]
//!    `#[non_exhaustive]` 5-canonical taxonomy
//!    (`corelink.canary.{loop_executed, assertion_failed, digest_mismatch, observability_unhealthy, dispatch_lag_exceeded}`)
//!    + [`CanaryAuditRecord`] + [`CanaryAuditSink`] trait +
//!    [`InMemoryCanaryAuditSink`] + [`FailingCanaryAuditSink`].
//! 6. The [`error`] module ships [`CanaryError`] `#[non_exhaustive]`
//!    taxonomy (`AssertionFailed` / `DigestMismatch` /
//!    `ObservabilityUnhealthy` / `DispatchLagExceeded` / `Audit` /
//!    `Internal`).
//! 7. The [`config`] module ships canonical constants pinned at the
//!    type system layer for surface-stability tests:
//!    `CRON_INTERVAL_SECS = 60` (sprint contract §6 DoD); 72h-loop
//!    target `12_960` (3 regions × 4_320 loops/72h/region; Lote
//!    10.9bis P0-A corrected); `DISPATCH_LAG_SEV3_MS = 90_000`;
//!    canonical synthetic canary tenant_id literal
//!    `00000000-0000-0000-0000-canary00000`.
//!
//! # Why the canary is `trait + fake` here, real CF Workers cron in S-20
//!
//! S-09 lands without Cloudflare Worker secrets bound to a staging
//! account (no remote + Cloudflare Workers + R2 staging are HARD
//! inflection points per `corelink_autonomous_execution_charter.md`).
//! The fake covers the algorithmic invariants that a production wiring
//! bug would expose: per-region ledger isolation; canonical assertion
//! ladder boundaries (`cas_put ≤ 100ms` / `cas_get ≤ 50ms` /
//! `ac_lookup ≤ 30ms`); BLAKE3 digest match correctness; cron drift
//! detection (> 90s = SEV-3); audit-of-canary fail-CLOSED envelope on
//! every decision arm; synthetic tenant SLI exclusion (Lote 10.8bis
//! P1-NEW-3 inheritance). The live CF Workers cron-trigger 24/7
//! sustained 72h sem gap (12_960 loops/72h target per Lote 10.9bis
//! P0-A corrected) + `worker::Fetch` HTTP client + real R2 PUT/GET /
//! AC lookup paths + Mimir/Loki/Tempo/Grafana health probe HTTPS
//! endpoints run alongside S-20 (GA gate; staging account provisioned).
//!
//! # Cripto-driven invariants enforced
//!
//! - **Canary assertion ladder boundary discipline** (HIGH; WI §1
//!   invariant 2): for any (region, observed latency) tuple, the
//!   canonical decision matches the ladder ceilings exactly. Pinned
//!   by `prop_canary_assertion_ladder` (10k iter PR gate).
//! - **BLAKE3 digest match correctness** (CRITICAL; WI §1 invariant 2
//!   #3): canary CAS PUT writes blob with digest D; CAS GET reads
//!   blob with digest D' = D byte-for-byte; any mismatch emits SEV-1
//!   `corelink.canary.digest_mismatch`. Pinned by
//!   `prop_canary_digest_correctness`.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (HIGH; lift from S-07
//!   P1-1 fix plus Lote 10.6bis pattern): audit-of-canary emit BEFORE
//!   state mutation on every decision arm. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): synthetic canary tenant isolated from real
//!   tenant traffic; canary loops never inflate real-tenant SLI
//!   denominators. Pinned by `prop_synthetic_tenant_excluded_from_sli`
//!   (Lote 10.8bis P1-NEW-3 inheritance from WI-S08-005
//!   ManualOverride exclusion pattern).
//! - **Cron drift detection** (informational invariant; WI §1
//!   invariant 9): expected interval 60s; observed interval > 90s
//!   sustained = SEV-3 alert source. Pinned by
//!   `prop_dispatch_lag_threshold_canonical`.
//! - **Three-region canonical** (informational invariant; sprint
//!   contract §6 DoD ship gate criterion): canary deployed em 3
//!   regions exactly (Enam + Weur + Apac); single-region canary
//!   insufficient for multi-region production confidence. Pinned by
//!   `prop_three_region_decision_correctness`.
//!
//! # Production wiring (deferred to S-20)
//!
//! - CF Workers scheduled trigger via `crons = ["*/1 * * * *"]` cron
//!   expression per region (3 regions independent CF Worker
//!   deployments per WI §6.1.2); canary HTTP client via
//!   `worker::Fetch::send` per Lote 10.7bis R5 P0-3 (NEVER
//!   `tokio::spawn`).
//! - Real R2 PUT (CAS write) + GET (CAS read) + AC lookup paths via
//!   the corelink-worker `cas` + `ac` modules.
//! - BLAKE3 digest verify via `blake3::hash` on the read blob bytes;
//!   compared byte-for-byte against the written digest.
//! - Mimir / Loki / Tempo / Grafana health probe HTTPS endpoints
//!   (Grafana Cloud `/api/health` per stack tenant config).
//! - 72h sustained validation = ship gate criterion; canary loops
//!   counted per region (target 4_320 loops/72h/region; total
//!   12_960/72h per Lote 10.9bis P0-A corrected).
//! - RB-FM-153 (Grafana Cloud outage) runbook dry-run + RB-OBS-
//!   CARDINALITY-001 (cardinality explosion) runbook dry-run executed
//!   em staging per sprint contract §6 DoD EVT-017.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    reason = "module-level docs use deep nested numbered/bulleted lists; \
              clippy's auto-detection is over-aggressive on the canonical \
              3-region canary prose"
)]

pub mod assertion;
pub mod audit;
pub mod config;
pub mod error;
pub mod probe;
pub mod region;
pub mod result;

pub use assertion::{
    canonical_canary_assertions, AssertionCeilings, CanaryAssertion, CanaryLatenciesMs,
};
pub use audit::{
    canonical_canary_audit_event_strings, CanaryAuditEmitError, CanaryAuditEventType,
    CanaryAuditRecord, CanaryAuditSink, FailingCanaryAuditSink, InMemoryCanaryAuditSink,
};
pub use config::{
    CRON_INTERVAL_SECS, DISPATCH_LAG_SEV3_MS, SUSTAINED_72H_LOOP_TARGET, SYNTHETIC_CANARY_TENANT_ID,
};
pub use error::CanaryError;
pub use probe::{
    CanaryProbe, FailingCanaryProbe, InMemoryCanaryProbe, RegionProbeStats,
};
pub use region::{canonical_canary_regions, CanaryRegion};
pub use result::{
    canonical_canary_decisions, canonical_health_components, CanaryDecision, CanaryLoopResult,
    HealthComponent, ObservabilityHealthReport,
};

/// Crate canonical schema version constant.
#[must_use]
pub const fn canary_schema_version() -> u32 {
    1
}
