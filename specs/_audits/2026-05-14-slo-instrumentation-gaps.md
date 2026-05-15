---
id: "AUDIT-2026-05-14-SLO-INSTRUMENTATION"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-05-14"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "slo", "observability", "instrumentation", "gap-analysis"]
---

# SLO Instrumentation Gap Audit — 2026-05-14

> **Purpose:** cross-reference every SLO declared in
> `specs/03_architecture/slo_catalog.md` against the actual emission
> surface in code (`crates/corelink-slo`, `crates/corelink-tracing`,
> `crates/corelink-analytics`, `apps/server`). Identify SLOs that ship
> on paper but would NOT page in production today.
>
> **Scope:** GA Phase 1 (Remote Cache) + admin/DR/oncall SLOs declared
> for S-13 / S-17. Phase 2 (`execute-action`) explicitly out-of-scope.

## 1. Methodology

1. Parse `slo_catalog.md §4.1–§4.21` for every `**SLO-*** ID + the
   metric-name reference in its SLI row.
2. `grep -R` each metric base name across `apps/server/src/`,
   `crates/corelink-slo/src/`, `crates/corelink-tracing/src/`,
   `crates/corelink-analytics/src/`.
3. Classify each SLO row across four axes:
   - **declared in spec?** — present in §4.x (yes / no).
   - **emitted in code?** — at least one production code path
     references the canonical metric name AND a corresponding
     observer/emitter sink trait is called outside `#[cfg(test)]`
     (yes / no / partial = name reserved in enum but no emit-site).
   - **prometheus histogram bucketing?** — for latency / freshness
     SLOs, whether a `_bucket{le=…}` histogram form exists in the
     metric registry (yes / no / N/A for counters/gauges).
   - **alert wired?** — whether an `Sli` enum variant exists in
     `corelink-slo::definition::Sli` so multi-burn-rate alert path
     in `corelink-slo::alert` can evaluate this SLO at runtime
     (yes / no / partial).

## 2. Canonical taxonomies (baseline state, pre-fix)

- `crates/corelink-analytics::canonical::RedMetricKind` — 15 variants
  (9 RED + 6 USE), pinned by the canonical-15 cardinality budget
  discipline.
- `crates/corelink-slo::definition::Sli` — 7 variants (`AvailCasGet`,
  `AvailCasPut`, `AvailAcLookup`, `AvailAuth`, `LatencyCasGetP99`,
  `DedupRatio`, `RateLimitWithinQuota`).

## 3. Coverage matrix

| SLO ID | Catalog § | Metric (canonical) | Declared | Emitted | Histogram | Alert wired |
|---|---|---|---|---|---|---|
| SLO-AVAIL-CP | 4.1 | `corelink_cp_requests_total` | yes | **no** | N/A (counter) | **no** |
| SLO-AVAIL-CAS-GET | 4.2 | `corelink_cas_get_requests_total` | yes | **no** | N/A (counter) | yes (`Sli::AvailCasGet`) |
| SLO-AVAIL-CAS-PUT | 4.3 | `corelink_cas_put_requests_total` | yes | partial (enum only) | N/A | yes (`Sli::AvailCasPut`) |
| SLO-AVAIL-AC | 4.4 | `corelink_ac_requests_total` | yes | partial (close cognate `corelink_ac_lookup_requests_total` in `RedMetricKind`) | N/A | yes (`Sli::AvailAcLookup`) |
| SLO-AVAIL-EXEC | 4.5 | `corelink_exec_completed_total` | yes | **no** (Phase 2; deferred) | N/A | **no** (Phase 2) |
| SLO-LAT-CAS-GET | 4.6 | `corelink_cas_get_duration_seconds` | yes | **no** | **no** (not in `RedMetricKind`) | yes (`Sli::LatencyCasGetP99`) |
| SLO-LAT-CAS-PUT | 4.7 | `corelink_cas_put_duration_seconds` | yes | partial (enum only; declared `is_histogram=true` in `RedMetricKind::CasPutDurationSeconds` but no emit-site) | **partial** | **no** |
| SLO-LAT-AC-HIT | 4.8 | `corelink_ac_get_duration_seconds` | yes | **no** | **no** | **no** |
| SLO-DEDUP-RATIO | 4.8.1 | `corelink_dedup_bytes_saved_total` / `corelink_dedup_bytes_uploaded_total` | yes | partial (cognate `corelink_dedup_ratio` gauge in `RedMetricKind`) | N/A | yes (`Sli::DedupRatio`) |
| SLO-CORRECT-CAS | 4.9 | `corelink_cas_client_verify_total` | yes | **no** | N/A | **no** |
| SLO-CORRECT-ISO | 4.10 | `corelink_isolation_assertion_total` | yes | **no** | N/A | **no** |
| SLO-FRESH-BILLING | 4.11 | `corelink_billing_event_age_seconds` | yes | partial (cognate `corelink_billing_events_emitted_total` counter; **no `_age_seconds` histogram**) | **no** | **no** |
| SLO-FRESH-DSR-ERASURE | 4.12 | `corelink_dsr_resolution_hours` | yes | partial (cognate `corelink_privacy_dsr_active_total` gauge) | **no** | **no** |
| SLO-DEPLOY-SAFE | 4.13 | (internal, deploy / rollback ratio) | yes | **no** | N/A | **no** |
| SLO-ADMIN-CONFIG-PROPAGATION | 4.14 | `corelink_admin_config_propagation_ms` | yes | **no** | **no** | **no** |
| SLO-ADMIN-DUAL-APPROVAL-LATENCY | 4.15 | (admin dual-approval handling time) | yes | **no** | **no** | **no** |
| SLO-ADMIN-ROTATION-OVERLAP | 4.16 | (per-asset-class overlap ratio) | yes | **no** | N/A | **no** |
| SLO-ADMIN-ROLLBACK-RECOVERY | 4.17 | `corelink_admin_rollback_recovery_ms` | yes | **no** | **no** | **no** |
| SLO-RTO-REGION-FAILOVER | 4.18 | `dr_drill_runs.rto_seconds` | yes | **no** (DR drill measurement, S-17) | N/A | **no** |
| SLO-RPO-REGION | 4.19 | `dr_drill_runs.rpo_seconds` | yes | **no** (S-17) | N/A | **no** |
| SLO-ONCALL-MTTA-SEV1 | 4.20 | `oncall_pages.acknowledged_at − triggered_at` | yes | **no** (PagerDuty-side; S-17) | N/A | **no** |
| SLO-ONCALL-MTTR-SEV1 | 4.21 | `oncall_pages.resolved_at − triggered_at` | yes | **no** (PagerDuty-side; S-17) | N/A | **no** |

**Totals (baseline, pre-fix):** 22 SLOs declared. Emission column:
6 partial (cognate-only) + 16 no = **0 / 22 fully wired**. Alert
column: 5 / 22 wired in `Sli` enum (counting partial as wired).

## 4. Gap classification

### 4.1 P0 — customer-facing GA hot path (must fix this audit)

| SLO ID | Why P0 | Closure |
|---|---|---|
| SLO-AVAIL-CP | Control-plane availability is the AUTH + admin door; outage = 100% customer impact. Currently no `Sli` enum variant ⇒ multi-burn-rate alert path cannot evaluate. | Add `Sli::AvailControlPlane`. |
| SLO-LAT-CAS-PUT | Latency SLO for CAS PUT (p99 < 1s team / 600ms enterprise) — histogram defined in `RedMetricKind::CasPutDurationSeconds` but `Sli` taxonomy lacks the latency variant. | Add `Sli::LatencyCasPutP99`. |
| SLO-LAT-AC-HIT | AC hit latency p99 < 150ms — drives every cache-hit user experience. No `Sli` variant ⇒ no alert. | Add `Sli::LatencyAcHitP99`. |
| SLO-CORRECT-CAS | Correctness SLO 100% (zero-budget). Any miss = SEV-1. No `Sli` variant ⇒ alert path can't classify. | Add `Sli::CorrectnessCas`. |
| SLO-CORRECT-ISO | Tenant isolation correctness 100% (zero-budget). INV-TENANT-ISO already CRITICAL in invariant registry; the SLO has no run-time alert binding. | Add `Sli::CorrectnessTenantIsolation`. |

### 4.2 P1 — admin/DR/oncall (sprints S-13, S-17)

Tracked but deferred to their owning sprints. Includes
`SLO-ADMIN-*` (S-13), `SLO-RTO-REGION-FAILOVER` / `SLO-RPO-REGION`
(S-17), `SLO-ONCALL-MTTA-SEV1` / `SLO-ONCALL-MTTR-SEV1` (S-17), and
`SLO-DEPLOY-SAFE` (rollout subsystem).

### 4.3 P2 — Phase 2 / Remote Execution

`SLO-AVAIL-EXEC` is explicitly `(Fase 2)` per the catalog phase-boundary
banner. Out-of-scope for GA instrumentation.

## 5. Closures shipped in this PR (Goal B — 5 P0 closures)

- `Sli::AvailControlPlane` — slug `SLI-AVAIL-CP`; prometheus base
  `corelink_cp_requests`.
- `Sli::LatencyCasPutP99` — slug `SLI-LATENCY-CAS-PUT-P99`; prometheus
  base `corelink_cas_put_latency`.
- `Sli::LatencyAcHitP99` — slug `SLI-LATENCY-AC-HIT-P99`; prometheus
  base `corelink_ac_get_latency`.
- `Sli::CorrectnessCas` — slug `SLO-CORRECT-CAS`; prometheus base
  `corelink_cas_client_verify`.
- `Sli::CorrectnessTenantIsolation` — slug `SLO-CORRECT-ISO`;
  prometheus base `corelink_isolation_assertion`.

These are additive to the `#[non_exhaustive]` `Sli` enum, do not
touch the canonical-15 `RedMetricKind` cardinality budget (which is
explicitly capped for the dashboard count discipline), and unblock
the multi-burn-rate alert evaluator in `corelink-slo::alert` for
the five SLOs above.

After this PR: **5 additional SLOs wire to the alert evaluator**
(7 → 12 alert-bound / 22 declared = 54.5%).

## 6. Closures NOT shipped (deferred with rationale)

- ~~**Real prometheus emit-site** in a Cloudflare Worker handler:
  the `apps/server` binary currently exposes only a `webhook.rs`
  surface; there are no CAS/AC/admin/billing/DSR HTTP handlers in
  `apps/server/src/` yet (those are split across separate Worker
  crates whose binaries are not in the current workspace). Adding
  emit-sites at the `Sli`-evaluator layer (the abstraction this
  audit closes) is the correct level of fix until the handler crates
  land. Per the autonomous execution charter `trait-abstraction-defer`
  pattern: ship the typed surface + canonical taxonomy here; the
  emit-site lands in the handler crate's own WI.~~ **CLOSED
  2026-05-15** by `crates/corelink-handler-cas`,
  `crates/corelink-handler-ac`, `crates/corelink-handler-admin`
  (R-prep handler-crate skeletons). Every handler entry emits one
  `SliObservation` per relevant SLI through the per-handler
  `SliObserver` trait (in-memory fake landed; real Prometheus-backed
  sink follows the same shape). One end-to-end wire-up landed at
  `apps/server::routes::cas::handle_read` demonstrating the
  cfg-gated wasm32 CF-Worker slot; AC + Admin route surfaces
  follow once their route shapes are agreed. See §6.1 below for the
  per-SLO transition.

### 6.1 SLO emit-site transitions (post handler-crate skeletons)

| SLO ID | Pre | Post handler-skeleton |
|---|---|---|
| SLO-AVAIL-CAS-GET | Sli-bound deferred | **Sli emitted at handler layer** (`corelink-handler-cas::CasReadHandler::read` → `SliObserver::observe(Sli::AvailCasGet)`) |
| SLO-AVAIL-CAS-PUT | Sli-bound deferred | **Sli emitted at handler layer** (`corelink-handler-cas::CasWriteHandler::write` → `Sli::AvailCasPut`) |
| SLO-LAT-CAS-GET (p99) | Sli-bound deferred | **Sli emitted at handler layer** (`Sli::LatencyCasGetP99` on every entry) |
| SLO-LAT-CAS-PUT (p99) | Sli-bound deferred (P0-2 closure) | **Sli emitted at handler layer** (`Sli::LatencyCasPutP99` on every entry) |
| SLO-CORRECT-CAS | Sli-bound deferred (P0-4 closure) | **Sli emitted at handler layer** (`Sli::CorrectnessCas` on every read + write entry; hash-mismatch path drives `is_error=true`) |
| SLO-AVAIL-AC | Sli-bound deferred | **Sli emitted at handler layer AND routed end-to-end** (`corelink-handler-ac::AcLookupHandler::lookup` → `Sli::AvailAcLookup`; `apps/server::routes::ac` wave-11 wire-up) |
| SLO-LAT-AC-HIT (p99) | Sli-bound deferred (P0-3 closure) | **Sli emitted at handler layer AND routed end-to-end** (`Sli::LatencyAcHitP99` on every lookup entry; `apps/server::routes::ac` wave-11 wire-up) |
| SLO-AVAIL-CP | Sli-bound deferred (P0-1 closure) | **Sli emitted at handler layer AND routed end-to-end** (`corelink-handler-admin::Admin{Read,Mutate}Handler` → `Sli::AvailControlPlane`; `apps/server::routes::admin` wave-11 wire-up; mutate path gated by dual-approval) |
| SLO-CORRECT-ISO | Sli-bound deferred (P0-5 closure) | Cross-tenant denial path emits `Sli::AvailCasGet`/`AvailCasPut` with `is_error=true` + `ReadDenied`/`WriteDenied` audit row BEFORE the rejection; standalone `Sli::CorrectnessTenantIsolation` observer integration remains pending its dedicated probe (any cross-tenant audit row already classifies; explicit emit follows). |

**Coverage delta:** 8 SLOs transition from "Sli-bound deferred" to
"Sli emitted at handler layer". Combined with the 5 P0 closures
from §5 (which wire the `Sli` enum surface), the **multi-burn-rate
alert evaluator can now consume signal from handler-instrumented
emit sites** for the full GA-hot-path SLO set
(`SLO-AVAIL-CAS-GET/PUT`, `SLO-LAT-CAS-GET/PUT-P99`, `SLO-AVAIL-AC`,
`SLO-LAT-AC-HIT-P99`, `SLO-CORRECT-CAS`, `SLO-AVAIL-CP`).

### 6.2 Audit fail-CLOSED ordering pinned at handler layer

Each handler crate's `InMemoryFake` enforces the canonical
`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` ordering by emitting the
relevant audit row BEFORE any state mutation or response. The
proptests pin:

- `prop_audit_fail_closed_no_mutation_on_write` (CAS) — audit
  injection failure leaves storage untouched.
- `prop_audit_fail_closed_no_state_change` (Admin) — audit
  injection failure leaves the applied-mutation ledger empty.
- `prop_audit_fail_closed_no_mutation_on_update` (AC) — audit
  injection failure leaves the AC entry map untouched.

### 6.3 Remaining gaps (post handler-crate skeletons)

- **Real CF-Worker handler** wiring (`#[cfg(target_arch = "wasm32")]`
  module placeholder reserved in `corelink-handler-cas` lib.rs) —
  scheduled as `WI-S04-CF-WIRING` per the autonomous-execution
  charter `trait-abstraction-defer` rule. The wire-up shape is
  documented in the lib.rs ignore-block example.
- ~~**AC + Admin route registration** in `apps/server::routes` —
  the example end-to-end wire-up shipped only `cas-read`. AC
  lookup and Admin read/mutate routes follow the same pattern
  once their route shapes are agreed (handler crate + fakes are
  already in place).~~ **CLOSED 2026-05-15** by wave-11 wire-up
  (`apps/server::routes::ac` + `apps/server::routes::admin`).
  Both modules follow the cas-read trait-object pattern with
  `Arc<dyn ...Handler>` route state, in-memory fakes on the native
  target, and a `compile_error!` reserved slot for the wasm32
  CF-Worker impl (also tracked under `WI-S04-CF-WIRING`). Routes:
  `GET/PUT /v1/ac/{tenant}/{action_digest}`,
  `GET /v1/admin/read/{resource}`, `POST /v1/admin/mutate`. The
  composed router is built via `apps/server::routes::build()`.
- **Standalone `Sli::CorrectnessTenantIsolation` probe** — the
  cross-tenant denial path is currently classified through the
  availability SLI error flag; a dedicated observer call for the
  isolation SLI tracks as `WI-S03-ISO-PROBE`.
- The `corelink-billing-*` / `corelink-dsr-*` handlers are not in
  this skeleton lane (S-13 / S-17 admin-plane + DR scope).
- **`RedMetricKind` enum extension** for the additional metric names
  (`corelink_cp_requests_total`, `corelink_cas_get_duration_seconds`,
  `corelink_billing_event_age_seconds`, etc.) is **deliberately not
  shipped**. The canonical-15 cap is enforced by
  `prop_canonical_metric_count_pinned` + dashboard-count discipline
  + cardinality budget. Extending requires its own ADR + sprint
  contract amendment. The `Sli`-layer fix is the load-bearing
  alert-wiring closure; the metric-registry extension is its own WI.

## 7. CI gate (Goal C)

`scripts/validate_slo_instrumentation.py` enforces:

1. Every `**SLO-***` declared in `slo_catalog.md §4.x` has either:
   - An entry in the closure-list above (this audit; allowlisted as
     "Phase 2 deferred" or "S-13/S-17 deferred"), OR
   - An `Sli` variant in `crates/corelink-slo/src/definition.rs`
     whose `slug()` matches an `SLI-/SLO-` slug declared in the
     catalog row.
2. Every `Sli` variant has a corresponding `**SLO-*` declared in
   the catalog (no orphan SLIs).
3. Exits non-zero on any missing wiring; runs in CI via
   `.github/workflows/slo-instrumentation.yml`.

## 8. Follow-on work

- **WI-S13-INSTR**: close admin-plane SLO emit-sites
  (CONFIG-PROPAGATION, DUAL-APPROVAL-LATENCY, ROTATION-OVERLAP,
  ROLLBACK-RECOVERY) at the admin-API handler layer.
- **WI-S17-INSTR**: close DR + oncall SLO sources (`dr_drill_runs`
  table + PagerDuty webhook ingest).
- **ADR-NNN-RED-METRIC-EXPANSION**: ADR + sprint contract amendment
  to extend `RedMetricKind` past the canonical-15 cap, with explicit
  cardinality budget delta + dashboard count update.

---

**End of audit.** Closures land in the same PR via
`crates/corelink-slo/src/definition.rs` + `scripts/validate_slo_instrumentation.py`
+ `.github/workflows/slo-instrumentation.yml`.
