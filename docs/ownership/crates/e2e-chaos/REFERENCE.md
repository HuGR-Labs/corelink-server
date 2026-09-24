---
schema: corelink-ownership/1.1
document: reference
package: e2e-chaos
manifest: tests/e2e-chaos/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-chaos-static-source-20260921
---

# e2e-chaos — ownership reference

Static source only; test names/assertions do not evidence execution, runtime behavior, or production reachability. [Verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is the routing reference.

[Identity](#r01) · [Boundaries](#r02) · [Experiment mapping](#r03) · [Contracts](#r04) · [State](#r05) · [Targets](#r06) · [Failures and observability](#r07) · [Verification and evidence](#r08)

<a id="r01"></a>
## R01 — Identity and manifest boundary

The package identity is `[package].name = "e2e-chaos"` in `tests/e2e-chaos/Cargo.toml:1-8`; it is non-publishable. The library path is `tests/e2e-chaos/src/lib.rs` (`tests/e2e-chaos/Cargo.toml:13-14`). The only direct first-party normal dependency is workspace `corelink-chaos-scheduler` (`tests/e2e-chaos/Cargo.toml:16-17`); workspace `proptest` is development-only (`tests/e2e-chaos/Cargo.toml:19-20`). Twelve `[[test]]` declarations are listed at `tests/e2e-chaos/Cargo.toml:22-68`.

No `[features]` table is declared. The manifest description (`tests/e2e-chaos/Cargo.toml:3`) is intent, not selection, resolution, or execution evidence. Name, dependency, path, target, or feature changes falsify this inventory.

<a id="r02"></a>
## R02 — Library surface and ownership boundaries

The library root forbids unsafe and denies missing docs/debug (`tests/e2e-chaos/src/lib.rs:45-55`); it imports `RefCell`/`Mutex` and scheduler APIs. Package public contracts are indexed in R04 (`:68-447`); imported scheduler symbols remain scheduler-owned. Declaration/signature/enum-arm/import changes falsify this inventory.

Scheduler owns catalog, runner, types, and seed API; this package owns mappings, constructors, fakes, assertions, lock model, and targets. Cross-WI invariant comments/snapshots do not prove shared wiring (`tests/e2e-chaos/src/lib.rs:19-29`). Atomic source arrows are in [blast radius](BLAST_RADIUS.md#b03); external callers, CI selection, and operation remain unknown.

Review/assignment escalation routes through `@gmhelmold` and the ownership-wave lead; [WAVE_014_PLAN.md](../../WAVE_014_PLAN.md) assigns review, integration, and status there. Not an `e2e-chaos` operator/maintainer assignment. Package acceptance and operation remain **BLOCKED** until an accountable package/operator owner is assigned.

<a id="r03"></a>
## R03 — Experiment mapping and constructors

Scheduler's real `canonical_catalog()` has eight entries, enumerated below
(`crates/corelink-chaos-scheduler/src/catalog.rs:1-15,24-113`). Exact lookup is
at `tests/e2e-chaos/src/lib.rs:113-120`: only two e2e IDs hit; six are synthetic.

| Canonical scheduler ID | Kind / target / FM | Exact e2e-chaos hit |
|---|---|---|
| `lat-r2-get` | LatencyInjection / `r2` / FM-051 | `lat-r2-get` |
| `lat-d1-query` | LatencyInjection / `d1` / FM-150 | none |
| `lat-neon-query` | LatencyInjection / `neon` / FM-057 | none |
| `lat-kv` | LatencyInjection / `kv` / FM-152 | none |
| `fail-r2-5xx` | FailureInjection / `r2` / FM-054 | none |
| `fail-d1-timeout` | FailureInjection / `d1` / FM-202 | none |
| `res-do-storage` | ResourceExhaustion / `do` / FM-059 | none |
| `net-cross-region` | NetworkPartition / `cross-region` / FM-105 | `net-cross-region` |

Synthetic IDs are `cpu-pressure-sustained`, `mem-pressure-oomk-avoid`,
`res-r2-disk-fill`, `dns-dual-resolver-failover`, `auth-clerk-outage`, and
`kv-d1-cold-start`; their constructors are `tests/e2e-chaos/src/lib.rs:122-188`.
The two exact hits return scheduler records. Mapping, lookup, value, or arm
changes falsify this inventory; no runtime hit was observed.

<a id="r04"></a>
## R04 — Public contracts

Index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015) · [API-016](#api-016) · [API-017](#api-017)

<a id="api-001"></a>
### API-001 — `ChaosE2eDrill::experiment_id`
**Symbols:** `ChaosE2eDrill::experiment_id(self) -> &'static str`. **Input/precondition:** one of eight enum variants; none else. **Output/effect:** fixed variant-to-ID mapping; no mutation/error. **Compatibility:** the wrapper defines eight IDs; R03 separates the scheduler's eight catalog entries, the two exact lookup hits, and six local synthetic constructors. Callers may depend on strings; runtime hits and version policy are unknown. **INV/REL:** [INV-001](#inv-001), [REL-046](BLAST_RADIUS.md#rel-046), [REL-047](BLAST_RADIUS.md#rel-047). **Evidence:** `tests/e2e-chaos/src/lib.rs:89-103,461-479`; property call `tests/e2e-chaos/tests/prop_seed_replay_identical.rs:71,97-98`; tests authored, not run.
[Index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — `make_experiment`
**Symbols:** `make_experiment(drill: ChaosE2eDrill) -> ChaosExperiment`. **Input/precondition:** one drill variant; none else. **Output/effect:** clones a catalog match; on miss uses six local records or the shared network/latency fallback in R03. No `Result` or package-local mutation. **Compatibility:** returned IDs/fields feed tests; catalog is scheduler-owned. **INV/REL:** [INV-002](#inv-002), [REL-003](BLAST_RADIUS.md#rel-003), [REL-026](BLAST_RADIUS.md#rel-026), [REL-028](BLAST_RADIUS.md#rel-028)–[REL-031](BLAST_RADIUS.md#rel-031). **Evidence:** `tests/e2e-chaos/src/lib.rs:113-203,481-496`; unit source authored, not run.
[Index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — `ChaosTestTelemetry::new`
**Symbols:** `ChaosTestTelemetry::new(configured_impact_bps: u32) -> Self`. **Input/unit/precondition:** impact in basis points; no range check. **Output/effect:** empty FIFO event vector, zero `u64` counter, stored impact; local allocation only. **Errors/compatibility:** no declared error; signature/field changes affect callers; version policy unknown. **INV/REL:** [INV-004](#inv-004), [INV-005](#inv-005), [REL-007](BLAST_RADIUS.md#rel-007), [REL-008](BLAST_RADIUS.md#rel-008), [REL-018](BLAST_RADIUS.md#rel-018)–[REL-021](BLAST_RADIUS.md#rel-021), [REL-044](BLAST_RADIUS.md#rel-044), [REL-045](BLAST_RADIUS.md#rel-045). **Evidence:** `tests/e2e-chaos/src/lib.rs:217-237,289`.
[Index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — `ChaosTestTelemetry::audit_events`
**Symbols:** `ChaosTestTelemetry::audit_events(&self) -> Vec<ChaosAuditEvent>`. **Input/precondition:** receiver reference; none else. **Output/effect:** clone of current FIFO events; no write. Conflicting `RefCell` borrow can panic. **Compatibility:** returned event shape is consumed by assertions; version policy unknown. **INV/REL:** [INV-004](#inv-004), [REL-007](BLAST_RADIUS.md#rel-007), [REL-008](BLAST_RADIUS.md#rel-008), [REL-021](BLAST_RADIUS.md#rel-021), [REL-039](BLAST_RADIUS.md#rel-039), [REL-040](BLAST_RADIUS.md#rel-040). **Evidence:** `tests/e2e-chaos/src/lib.rs:239-242,248-256`; staging assertion at `tests/e2e-chaos/tests/adversarial_chaos_staging_only.rs:55-56` authored, not run.
[Index](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — `ChaosTestTelemetry::slo_violation_total`
**Symbols:** `ChaosTestTelemetry::slo_violation_total(&self) -> u64`. **Input/precondition:** receiver reference; none else. **Output/effect:** reads local counter without mutation; conflicting `RefCell` borrow can panic. **Compatibility:** counter type/semantics are source-visible; version policy unknown. **INV/REL:** [INV-005](#inv-005), [REL-007](BLAST_RADIUS.md#rel-007), [REL-021](BLAST_RADIUS.md#rel-021), [REL-041](BLAST_RADIUS.md#rel-041)–[REL-043](BLAST_RADIUS.md#rel-043). **Evidence:** `tests/e2e-chaos/src/lib.rs:244-246,248-260,527`; assertions authored, not run.
[Index](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — `run_clean_staging`
**Symbols:** `run_clean_staging(drill: ChaosE2eDrill, run_id: &str, impact_bps: u32) -> (ChaosRun, ChaosTestTelemetry)`. **Input/unit/precondition:** drill, scheduler-owned run-ID construction, impact in bps; no wrapper validation. **Output/effect:** calls `run_experiment` with Staging, SEV-1 false, error rate 100 bps; returns run and fake. No `Result`; callback path depends on scheduler. **Compatibility:** tuple and scheduler call are source-visible. **INV/REL:** [INV-005](#inv-005), [REL-003](BLAST_RADIUS.md#rel-003), [REL-007](BLAST_RADIUS.md#rel-007), [REL-008](BLAST_RADIUS.md#rel-008), [REL-015](BLAST_RADIUS.md#rel-015), [REL-018](BLAST_RADIUS.md#rel-018)–[REL-021](BLAST_RADIUS.md#rel-021), [REL-032](BLAST_RADIUS.md#rel-032)–[REL-034](BLAST_RADIUS.md#rel-034). **Evidence:** `tests/e2e-chaos/src/lib.rs:283-297`; not executed.
[Index](#r04)

<a id="api-007"></a>
[↩](#r01)
### API-007 — `assert_canonical_audit_sequence`
**Symbols:** `assert_canonical_audit_sequence(events: &[ChaosAuditEvent]) -> Result<(), String>`. **Input/precondition:** borrowed event slice; none else. **Output/postcondition:** `Ok` only for `[Aborted]` or `[Started, Completed]`; otherwise formatted `Err`; no mutation. **Compatibility:** accepted shapes/error text source-visible; version policy unknown. **INV/REL:** [INV-003](#inv-003), [REL-004](BLAST_RADIUS.md#rel-004), [REL-007](BLAST_RADIUS.md#rel-007), [REL-008](BLAST_RADIUS.md#rel-008), [REL-021](BLAST_RADIUS.md#rel-021), [REL-037](BLAST_RADIUS.md#rel-037), [REL-038](BLAST_RADIUS.md#rel-038). **Evidence:** `tests/e2e-chaos/src/lib.rs:306-314`; scenario assertions authored, not run.
[Index](#r04)

<a id="api-008"></a>
[↩](#r01)
### API-008 — `assert_rollback_within_5min`
**Symbols:** `assert_rollback_within_5min(exp: &ChaosExperiment) -> Result<(), String>`. **Input/unit/precondition:** borrowed experiment; `rollback_seconds_max` in seconds; none else. **Output/postcondition:** `Ok` at ≤300; `Err` above 300 naming ID/value; no mutation. **Compatibility:** threshold/error text source-visible; version policy unknown. **INV/REL:** [INV-002](#inv-002), [REL-035](BLAST_RADIUS.md#rel-035), [REL-036](BLAST_RADIUS.md#rel-036). **Evidence:** `tests/e2e-chaos/src/lib.rs:322-329,481-496`; unit test authored, not run.
[Index](#r04)

<a id="api-009"></a>
[↩](#r01)
### API-009 — `OpsEventKind::label`
**Symbols:** `OpsEventKind::label(self) -> &'static str`. **Input/precondition:** one known kind; enum is `#[non_exhaustive]`. **Output/effect:** Chaos=`chaos`, DrDrill=`dr_drill`, RunbookDrill=`runbook_drill`; no mutation/error. **Compatibility:** labels are source-visible; version policy unknown. **INV/REL:** [INV-007](#inv-007), [REL-048](BLAST_RADIUS.md#rel-048). **Evidence:** implementation `tests/e2e-chaos/src/lib.rs:336-369`; `adversarial_ops_exclusivity.rs:35-37` calls `held_by.label()` and asserts `dr_drill`; authored, not run.
[Index](#r04)

<a id="api-010"></a>
[↩](#r01)
### API-010 — `OpsEventLock::new`
**Symbols:** `OpsEventLock::new() -> Self`. **Input/precondition:** none. **Output/postcondition:** per-instance mutex containing `None`; no external state/effect or declared error. **Compatibility:** public constructor exposes local lock model only; version policy unknown. **INV/REL:** [INV-006](#inv-006), [REL-009](BLAST_RADIUS.md#rel-009). **Evidence:** `tests/e2e-chaos/src/lib.rs:371-386,498-512`; unit test authored, not run.
[Index](#r04)

<a id="api-011"></a>
[↩](#r01)
### API-011 — `OpsEventLock::try_acquire`
**Symbols:** `OpsEventLock::try_acquire(&self, kind: OpsEventKind) -> Result<OpsLockGuard<'_>, OpsLockError>`. **Input/precondition:** kind; none else. **Output/effect:** stores kind and returns borrowing guard if idle; otherwise `LockHeld { held_by }`, including same-kind contention. Poisoned mutex recovers inner value. **Compatibility:** non-exhaustive error enum; local only. **INV/REL:** [INV-006](#inv-006), [REL-009](BLAST_RADIUS.md#rel-009). **Evidence:** `tests/e2e-chaos/src/lib.rs:389-420`; lock tests authored, not run.
[Index](#r04)

<a id="api-012"></a>
[↩](#r01)
### API-012 — `OpsLockGuard::release`
**Symbols:** `OpsLockGuard::release(self) -> ()`. **Input/precondition:** consuming guard borrowing originating lock; none else. **Output/effect:** clears matching held kind once; `Drop` is also idempotent. No I/O or declared error. **Compatibility:** consuming signature and RAII behavior source-visible; version policy unknown. **INV/REL:** [INV-006](#inv-006), [REL-009](BLAST_RADIUS.md#rel-009). **Evidence:** `tests/e2e-chaos/src/lib.rs:423-447,498-512`; tests authored, not run.
[Index](#r04)

<a id="api-013"></a>
[↩](#r01)
### API-013 — `Telemetry::emit_audit` implementation
**Symbols:** `emit_audit(&self, run: &ChaosRun, event: ChaosAuditEvent) -> ()`. **Input/precondition:** scheduler run reference and one audit event; none else. **Output/effect:** appends event FIFO; only `Completed` with `SteadyStateBreached` saturating-increments this instance's `u64` counter. No result; conflicting `RefCell` borrow can panic. **Compatibility:** implements scheduler-owned trait; behavior local. **INV/REL:** [INV-004](#inv-004), [INV-005](#inv-005), [REL-007](BLAST_RADIUS.md#rel-007), [REL-008](BLAST_RADIUS.md#rel-008), [REL-021](BLAST_RADIUS.md#rel-021). **Evidence:** `tests/e2e-chaos/src/lib.rs:248-259`; authored tests not run.
[Index](#r04)

<a id="api-014"></a>
[↩](#r01)
### API-014 — `Telemetry::capture_state_pre` implementation
**Symbols:** `capture_state_pre(&self, run: &ChaosRun) -> u64`. **Input/precondition:** run reference, ignored; none else. **Output/effect:** returns constant `0x0000_a11c_e5ee_d000`; no state capture or mutation. **Errors/compatibility:** no declared error; trait is scheduler-owned and return value is fake-local. **INV/REL:** FLOW-001; [REL-018](BLAST_RADIUS.md#rel-018). **Evidence:** `tests/e2e-chaos/src/lib.rs:261-263`; source only, not invoked here.
[Index](#r04)

<a id="api-015"></a>
[↩](#r01)
### API-015 — `Telemetry::capture_state_post` implementation
**Symbols:** `capture_state_post(&self, run: &ChaosRun) -> u64`. **Input/precondition:** run reference, ignored; none else. **Output/effect:** returns constant `0x0000_b0bb_1b1b_0000`; no state capture or mutation. **Errors/compatibility:** no declared error; trait is scheduler-owned and return value is fake-local. **INV/REL:** FLOW-001; [REL-019](BLAST_RADIUS.md#rel-019). **Evidence:** `tests/e2e-chaos/src/lib.rs:265-267`; source only, not invoked here.
[Index](#r04)

<a id="api-016"></a>
[↩](#r01)
### API-016 — `Telemetry::measure_slo_impact` implementation
**Symbols:** `measure_slo_impact(&self, run: &ChaosRun, pre: u64, post: u64) -> u32`. **Input/unit/precondition:** run and two digest values (`u64`); all are ignored; configured impact is basis points. **Output/effect:** returns stored `configured_impact_bps`; no mutation/error. **Compatibility:** trait is scheduler-owned; local return value configures fake runner outcome. **INV/REL:** FLOW-001; [REL-020](BLAST_RADIUS.md#rel-020). **Evidence:** `tests/e2e-chaos/src/lib.rs:269-271`; source only, not invoked here.
[Index](#r04)

<a id="api-017"></a>
[↩](#r01)
### API-017 — `OpsEventLock::default`
**Symbols:** derived `Default for OpsEventLock`. **Input/precondition:** none. **Output/postcondition:** creates a fresh per-instance `Mutex<Option<OpsEventKind>>` with `None`; no external state/effect or declared error. `new()` delegates to this derived default. **Compatibility:** public trait implementation exposes the local idle-lock model; version policy unknown. **INV/REL:** [INV-006](#inv-006), [REL-009](BLAST_RADIUS.md#rel-009). **Evidence:** `tests/e2e-chaos/src/lib.rs:377-386,498-512`; unit source authored, not run.
[Index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Local state, flows, and falsifiable invariants

| State | Owner / partition | Unit; persistence / lifetime | Transition; durability / concurrency |
|---|---|---|---|
| Constructed `ChaosExperiment` | returned value; caller / drill ID | `blast_radius_bps` in bps; rollback in seconds; no persistence, value lifetime | catalog clone or local constructor; no commit/time source; value-local |
| Fake telemetry | one `ChaosTestTelemetry` / instance (no run key) | event sequence and `u64` count; in-memory until drop; impact in bps | append FIFO; saturating count on completed breach; fixed digest callbacks; no clock/timestamp; `RefCell`, no durable sink |
| Safe-mode input and result | stack-local wrapper input/output; scheduler defines `ChaosRun` type | snapshot literals: target, bool, error rate in bps; call lifetime | wrapper calls imported runner; no package persistence or compensation asserted |
| Ops lock | one `OpsEventLock` / instance (no region key) | `Option<OpsEventKind>`; memory until lock drop | idle→held→idle by acquire/release/drop; `Mutex`; poison recovers inner state; no D1/shared durability |

Flows: [FLOW-001](#flow-001) · [FLOW-002](#flow-002). Invariants: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006) · [INV-007](#inv-007).

<a id="flow-001"></a>
### FLOW-001 — Local runner wrapper
1. Accept drill, run-ID text, and impact bps.
2. Build experiment and telemetry locally.
3. Build literal Staging / no-SEV-1 / 100-bps snapshot.
4. Call imported `run_experiment`; it may return Aborted or completed outcome.
5. Return `(ChaosRun, telemetry)`. No durable write or compensation is evidenced.
[R05 index](#r05)

<a id="flow-002"></a>
[↩](#r01)
### FLOW-002 — Per-instance lock lifecycle
1. `new` starts with `None`.
2. `try_acquire` stores kind or returns current `held_by`.
3. Guard `release` or `Drop` clears only its matching kind.
4. The source models one process-local object; no region partition, shared lock, or recovery beyond poison-inner recovery is evidenced.
[R05 index](#r05)

<a id="inv-001"></a>
[↩](#r01)
### INV-001 — Drill IDs are distinct
**Predicate:** the eight enum variants map to eight distinct strings. **Enforcement:** fixed match in `experiment_id`; no dynamic validation. **Violation:** duplicate/changed mapping aliases catalog keys or test selection. **Verification:** `drill_ids_unique` source assertion (`tests/e2e-chaos/src/lib.rs:461-479`); authored, not run. **Current:** mapping present; runtime unknown.
[R05 index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Rollback cap helper is exact
**Predicate:** helper returns `Ok` iff `rollback_seconds_max <= 300` seconds. **Enforcement:** `assert_rollback_within_5min`; constructor values are not independently guaranteed on catalog hit. **Violation:** >300 yields `Err` with ID/value; caller ignoring `Err` is not ruled out. **Verification:** helper and loop test source at `tests/e2e-chaos/src/lib.rs:322-329,481-496`; authored, not run. **Current:** source branch matches predicate; catalog/runtime values unknown.
[R05 index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Audit shape helper is exact
**Predicate:** only `[Aborted]` or `[Started, Completed]` returns `Ok`. **Enforcement:** slice-pattern match. **Violation:** any other sequence returns formatted `Err`. **Verification:** source at `tests/e2e-chaos/src/lib.rs:306-314` and calls in scenario tests; authored, not run. **Current:** branch matches predicate; emitted runtime sequence unknown.
[R05 index](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Fake events preserve append order
**Predicate:** each `emit_audit` appends one event to this instance's vector; reads clone current order. **Enforcement:** `push` and `clone` in `ChaosTestTelemetry`. **Violation:** removal/reordering/alternate storage changes snapshots. **Verification:** source at `tests/e2e-chaos/src/lib.rs:239-242,248-256`; no dedicated executed check. **Current:** source implements FIFO append; runtime unknown.
[R05 index](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Fake counter saturates only on completed breach
**Predicate:** on `Completed` plus `SteadyStateBreached`, set count to `saturating_add(1)`; all other emitted events leave it unchanged. **Enforcement:** `emit_audit`. **Violation:** altered event/outcome gate changes local metric. **Verification:** unit assertion source `tests/e2e-chaos/src/lib.rs:514-528`; authored, not run. **Current:** source matches; export/persistence are absent from this fake.
[R05 index](#r05)

<a id="inv-006"></a>
[↩](#r01)
### INV-006 — Lock excludes concurrent holders per instance
**Predicate:** occupied `held_by` makes every acquisition return `LockHeld`; matching release/drop clears it. **Enforcement:** mutex-protected `Option` and guard. **Violation:** acquisition succeeds while occupied or remains held after release. **Verification:** unit source `tests/e2e-chaos/src/lib.rs:498-512`; adversarial assertions `tests/e2e-chaos/tests/adversarial_ops_exclusivity.rs:27-61`; authored, not run. **Current:** local implementation present; cross-process/region exclusivity unknown.
[R05 index](#r05)

<a id="inv-007"></a>
[↩](#r01)
### INV-007 — Event labels map exactly
**Predicate:** Chaos=`chaos`, DrDrill=`dr_drill`, RunbookDrill=`runbook_drill`. **Enforcement:** exhaustive local match for current variants. **Violation:** changed/incorrect literal changes emitted labels. **Verification:** implementation plus `held_by.label()`/`dr_drill` assertion at `tests/e2e-chaos/src/lib.rs:336-369` and `tests/e2e-chaos/tests/adversarial_ops_exclusivity.rs:35-37`; authored, not run. **Current:** source literals/assertion present; runtime unknown.
[R05 index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Declared target inventory

The manifest declares twelve targets and `path` values (`tests/e2e-chaos/Cargo.toml:22-68`):

| Target | Manifest `path` |
|---|---|
| `chaos_network_partition_recovers` | `tests/chaos_network_partition_recovers.rs` |
| `chaos_cpu_pressure_sustained` | `tests/chaos_cpu_pressure_sustained.rs` |
| `chaos_memory_pressure_oomk_avoided` | `tests/chaos_memory_pressure_oomk_avoided.rs` |
| `chaos_r2_disk_fill_quarantine` | `tests/chaos_r2_disk_fill_quarantine.rs` |
| `chaos_latency_injection_p99_bounded` | `tests/chaos_latency_injection_p99_bounded.rs` |
| `chaos_dns_failure_dual_resolver_failover` | `tests/chaos_dns_failure_dual_resolver_failover.rs` |
| `chaos_clerk_outage_grace_period` | `tests/chaos_clerk_outage_grace_period.rs` |
| `chaos_kv_d1_cold_start_below_sla` | `tests/chaos_kv_d1_cold_start_below_sla.rs` |
| `adversarial_ops_exclusivity` | `tests/adversarial_ops_exclusivity.rs` |
| `adversarial_sev1_drill_pause` | `tests/adversarial_sev1_drill_pause.rs` |
| `adversarial_chaos_staging_only` | `tests/adversarial_chaos_staging_only.rs` |
| `prop_seed_replay_identical` | `tests/prop_seed_replay_identical.rs` |

Four unit tests are separate from manifest targets (`tests/e2e-chaos/src/lib.rs:461-528`): `drill_ids_unique`; `make_experiment_rollback_charter_compliant`; `ops_lock_basic_acquire_release`; `telemetry_records_slo_violation_on_breach`. They respectively check ID distinctness, cap values, local lock, and a configured 600-bps fake-counter path; none is run evidence.

| Name / source | Effective default | Read stage | Target / condition | Effect / failure |
|---|---|---|---|
| `PROPTEST_CASES` from process environment (`tests/e2e-chaos/tests/prop_seed_replay_identical.rs:27-34,53-54`) | `200` when absent or unparsable | runtime, when the property config calls `proptest_cases()` | `prop_seed_replay_identical` property target if selected/run | parseable `u32` sets Proptest case count; absent/parse failure falls back to 200; no range check beyond `u32` parsing |

The property target declares replay-identical and distinct-seed properties (`tests/e2e-chaos/tests/prop_seed_replay_identical.rs:53-105`). Target, env, or assertion changes falsify this inventory. Selection and supplied env value are unknown.

<a id="r07"></a>
## R07 — Errors and observability

| Failure or signal | Source condition / response | Observable local result | Boundary and diagnostic limit |
|---|---|---|---|
| Catalog lookup / synthetic fallback | Static source enumerates eight scheduler entries; exact e2e lookup has two hits and six local constructors (`tests/e2e-chaos/src/lib.rs:113-203`, scheduler catalog `:24-113`) | A `ChaosExperiment` is returned; no error | This establishes mapping only; runtime hit/selection remains unknown. See [REL-003](BLAST_RADIUS.md#rel-003), [REL-026](BLAST_RADIUS.md#rel-026) |
| Non-canonical audit slice | `assert_canonical_audit_sequence` returns formatted `Err(String)` unless shape is `[Aborted]` or `[Started, Completed]` (`tests/e2e-chaos/src/lib.rs:306-314`) | Caller receives `Result`; helper does not log or mutate | Assertion source only; not run |
| Rollback cap exceeded | `rollback_seconds_max > 300` returns `Err(String)` with experiment ID and seconds (`tests/e2e-chaos/src/lib.rs:322-329`) | Caller receives local error result | Does not perform or verify rollback |
| Lock occupied / poisoned | `try_acquire` returns `LockHeld { held_by }` when `Some`; poisoned mutex recovers via `into_inner()` (`tests/e2e-chaos/src/lib.rs:389-409`) | Held kind is returned; poison is not surfaced as a separate error | One local mutex; no cross-process or D1 diagnostic |
| `RefCell` borrow conflict | `audit_events`, counter getter, or callback borrows while incompatible borrow is active | Rust borrow panic; no package error conversion | Source-level possibility; no panic observation |
| Runner safe-mode result | Imported runner creates a `ChaosRun`; abort path emits `Aborted`, non-abort path emits `Started` and calls capture/measure before `Completed` (`crates/corelink-chaos-scheduler/src/runner.rs:137-185`) | `ChaosTestTelemetry` keeps local FIFO events and local counter | Scheduler owns runner; fake is not an audit/metric sink and does not count capture calls |

Observability is source-local: `emit_audit` appends to an in-memory vector, and the `u64` counter saturates only for a `Completed` breach (`tests/e2e-chaos/src/lib.rs:248-271`). Capture methods return fixed constants and impact returns configured basis points. This package code does not establish log/tracing output, exporter delivery, persistence, provider contact, or production diagnostics. Authored test assertions can inspect the fake but are not observed results.

<a id="r08"></a>
## R08 — Verification and evidence

Evidence set `e2e-chaos-static-source-20260921` records `SOURCE` at source pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`. Tests/assertions prove authored predicates only; catalog hits are static mapping facts, not runtime observations. Entries are `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`.

| API / INV / FLOW | Source anchor and class | Authored verification / method | Limit |
|---|---|---|---|
| API-001 / INV-001 | `tests/e2e-chaos/src/lib.rs:89-103,461-479` — `SOURCE` | `drill_ids_unique` source | Runtime mapping/callers unknown |
| API-002, API-008 / INV-002 | `tests/e2e-chaos/src/lib.rs:113-203,322-329,481-496` — `SOURCE` | constructor/cap unit-test source | Static 2-hit/6-synthetic mapping known; runtime selection/results unknown |
| API-003–005, API-013–016 / INV-004–005 | `tests/e2e-chaos/src/lib.rs:217-271,514-528` — `SOURCE` | fake, callback, and breach-counter source | No real capture, export, persistence, or observed callback |
| API-006–007 / FLOW-001 / INV-003 | `tests/e2e-chaos/src/lib.rs:283-314`; adversarial targets — `SOURCE` | wrapper/helper/assertion sources | Scheduler call path unobserved at runtime |
| API-009–012, API-017 / FLOW-002 / INV-006–007 | `tests/e2e-chaos/src/lib.rs:336-447,498-512`; lock target and label assertion — `SOURCE` | label, default, and lock assertion sources | No shared lock or runtime label result |
| Declared targets/properties/config | manifest and property sources — `SOURCE` | R06 inventory and env/fallback source | Selection, cases, values, and outcomes unknown |

Validation is static source inspection; no Cargo/build/test/property/fuzz/runtime or deployment check ran. The checker cannot certify Rust behavior.

Five axioms govern this reference:

1. Manifest declarations prove declared package structure only; they do not prove resolved selection or execution.

2. Source signatures and branches prove the local source contract only; they do not prove that a caller reached them.

3. Traits and in-memory fakes prove seams and local models only; they do not prove provider, persistent-store, telemetry, or production adapters.

4. Test names, property configuration, and assertions prove authored test-source predicates only; they do not prove a passing result or runtime safety.

5. The [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is the sole designated canonical routing reference; its policy is not copied or revalidated here.

**Unknowns:** resolved graph/features, inverse consumers, CI selection, property cases/seeds, scheduler invocation, production wiring, provider reachability, rollback, delivery, persistence, deployment, and test outcomes. No contradiction becomes runtime truth; falsifiers sit in R03–R05/B03.

**Continue:** [Impact relations](BLAST_RADIUS.md#b01) · [Maintenance procedures](MAINTENANCE.md#m01).
