---
schema: corelink-ownership/1.1
document: reference
package: e2e-replication-failover
manifest: tests/e2e-replication-failover/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-replication-failover-static-20260921
---

# e2e-replication-failover — source reference

This record is source-static at the pinned commit. The harness composes in-memory coordinator collaborators; its source and tests do not establish CI selection, test execution, audit delivery, a staging drill, or production behavior.

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08).

<a id="r01"></a>
## R01 — Identity and purpose

At source commit `1177dad2ca2a9f21c29b5a118aa7944b77147798`, the authoritative package name is `e2e-replication-failover`, the manifest sets `publish = false`, and the workspace lists this manifest. It declares a library and one explicit `scenarios` integration target. Package description prose is intent, not evidence that the advertised GA checks ran. ([manifest](../../../../tests/e2e-replication-failover/Cargo.toml#L1-L25), [workspace member](../../../../Cargo.toml#L319-L324)).

| Field | Source observation |
|---|---|
| Package / manifest | `e2e-replication-failover` / `tests/e2e-replication-failover/Cargo.toml` |
| Targets | Library at `src/lib.rs`; explicit test target `scenarios` at `tests/scenarios.rs` |
| Role | Test harness with one local in-memory fixture; no production composition shown |
| Publication | `publish = false` is explicit; inherited values are not Cargo-resolved here |
| Evidence | `SOURCE` at the pinned commit; no Cargo or test command executed for this record |

<a id="r02"></a>
## R02 — Boundaries and ownership

| Surface | Implementation owner | Contract owner | Composition / operation / review |
|---|---|---|---|
| `E2EFixture` and scenario assertions | This package | This package's local harness API | `E2EFixture::new`; production root unknown |
| Coordinator, heartbeat, audit interfaces and behavior | `corelink-replication-coordinator` | `corelink-replication-coordinator` | In-memory collaborators only here; runtime operator unknown |
| `Region` identity | `corelink-replica-worker` | `corelink-replica-worker` | Re-exported by coordinator; ownership does not move |
| Drill, staging, production, audit transport | Unknown | Unknown | No operator or escalation route verified |
| Artifact approval | Not package implementation | Independent cold reviewer | Lead/reviewer responsibility; person not supplied |

<a id="r03"></a>
## R03 — Implementation map

| Module / entries | Package responsibility | Nature | Evidence |
|---|---|---|---|
| `src/lib.rs`: `E2EFixture`, `new`, `init_topology`, `heartbeat_all_healthy` | Builds a fresh coordinator with in-memory heartbeat registry and audit sink; offers topology and healthy-heartbeat helpers | Owned fixture | [library](../../../../tests/e2e-replication-failover/src/lib.rs#L18-L97) |
| `tests/scenarios.rs`: `T0`, `primary_count`, six `#[test]` functions | Supplies timestamps, inputs, method calls, and assertions; no async runtime or wall clock | Integration-test source | [scenario target](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L21-L302) |

The inspected package tree contains these two source files. `T0` plus explicit `now_ms` arguments is not a clock abstraction or wall-clock observation. The scenario file imports coordinator symbols; it contains no direct `corelink_replica_worker` import.

<a id="r04"></a>
## R04 — Local public contracts

**Index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

The integration target also calls coordinator-owned `register`, `route_write`,
`evaluate`, `promote`, `failback`, `replication_status`, and `role_map` on
`fix.coordinator`; exact arguments and predicates are listed in [the six
flows](#flows). Their contracts remain owned upstream; source call sites and
predicates are listed in [R05](#r05) and [the relations](BLAST_RADIUS.md#b03).
`Region` is owned by replica-worker and re-exported at coordinator `lib.rs:145-147`.

<a id="api-001"></a>
### API-001 — Fixture construction

**Symbols:** `pub struct E2EFixture { pub heartbeats: Arc<InMemoryHeartbeatRegistry>, pub audit: Arc<InMemoryCoordinatorAuditSink>, pub coordinator: InMemoryReplicationCoordinator }`; `impl Default for E2EFixture { fn default() -> Self }`; `pub fn new() -> Self`. **Pre:** no inputs or external service. **Post:** fresh in-memory collaborators and zero registered regions. **Effects:** allocates `Arc`s and coordinator state. **Compatibility:** public fields and constructors are local harness API. **INV/REL:** [INV-001](#inv-001), [REL-003](BLAST_RADIUS.md#rel-003), [REL-016](BLAST_RADIUS.md#rel-016). ([implementation](../../../../tests/e2e-replication-failover/src/lib.rs#L26-L58)).

[Contract index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Four-region topology helper

**Symbol:** `pub fn init_topology(&self, primary: Region) -> Result<(), corelink_replication_coordinator::CoordinatorError>`. **Pre:** fixture coordinator is empty; `primary` is a `Region`. **Post:** `primary` is `Primary`, every other `Region::ALL` item is `Replica`, or the first error is returned. **Effects:** mutates coordinator role state through `register`. **Compatibility:** follows upstream `RegionRole`/`CoordinatorError`; no production promise. **INV/REL:** [INV-001](#inv-001), [REL-005](BLAST_RADIUS.md#rel-005), [REL-006](BLAST_RADIUS.md#rel-006). ([implementation](../../../../tests/e2e-replication-failover/src/lib.rs#L60-L79)).

[Contract index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Healthy heartbeat helper

**Symbol:** `pub fn heartbeat_all_healthy(&self, now_ms: u64) -> Result<(), corelink_replication_coordinator::CoordinatorError>`. **Pre:** fixture registry exists; `now_ms` is caller-supplied milliseconds. **Post:** one zero-lag `Heartbeat` is recorded per `Region::ALL`, or a registry error returns. **Effects:** updates only the in-memory registry; no clock or worker is sampled. **Compatibility:** depends on upstream `Heartbeat`/`LagBundle` shapes. **INV/REL:** [INV-004](#inv-004), [REL-014](BLAST_RADIUS.md#rel-014), [REL-017](BLAST_RADIUS.md#rel-017). ([implementation](../../../../tests/e2e-replication-failover/src/lib.rs#L81-L96)).

[Contract index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State, flow, and falsifiable invariants

| State | Owner / key | Lifetime and update | Evidence limit |
|---|---|---|---|
| Region roles | Coordinator in-memory map keyed by canonical region string | Fixture-local coordinator instance; calls register/promote/failback | No cross-process state established; role values are not persisted |
| Heartbeats | In-memory registry keyed by `Region` | Test supplies `u64` millisecond timestamps and `LagBundle` seconds | No worker emission, monotonic clock, or persistence established |
| Audit records | In-memory sink vector of coordinator events | Coordinator calls `emit`; tests read snapshots/records | No bus, durability, delivery, or external consumer established |
| Concurrency boundary | Upstream coordinator state mutex | Six tests use one fixture per test and no spawned tasks | Cross-process/DO singleton concurrency is unknown; source does not establish it |

**Index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001) · [six flows](#flows).

<a id="inv-001"></a>
### INV-001 — At most one primary at checked points

**Predicate:** each explicit `primary_count`/role-map assertion observes at most one `Primary`. **Enforcer owner:** coordinator `register`/`promote`; package checks selected points in the lifecycle and split-brain cases. **Falsifier:** a checked map contains two primaries. This is not an assertion at every timestamp and the test source was not run here. ([assertions](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L53-L56), [promotion](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L92-L96), [split-brain](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L159-L195)).

[State index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Audit precedes observable role mutation

**Predicate:** the upstream coordinator source emits `RegionDemoted` and `RegionPromoted` before inserting the new roles ([implementation](../../../../crates/corelink-replication-coordinator/src/coordinator.rs#L417-L465)); this harness later expects that vector order and WNAM `HotStandby`/ENAM `Primary`. **Enforcer owner:** coordinator. **Falsifier:** upstream source violating that order, or a different harness vector/role map. The harness does not observe the mutation boundary or exercise a failing sink; its own audit-before-mutation proof is therefore `UNKNOWN`. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L76-L90)).

[State index](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Promotion eligibility requires fresh, in-SLO observations

**Predicate:** source expects a stale primary or R2/D1-breaching primary to be refused/rerouted and a first fresh, in-SLO replica to be proposed. **Enforcer owner:** coordinator `evaluate`/`route_write`; fixture supplies synthetic `Heartbeat` values. **Falsifier:** an asserted decision or write result differs. No runtime heartbeat producer is involved. ([scenarios](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L98-L134), [scenarios](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L198-L235)).

[State index](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Failback waits for the configured cooldown

**Predicate:** a failback one second before `HOT_STANDBY_COOLDOWN_SECONDS` returns `CooldownNotElapsed`; the boundary call changes WNAM back to `Primary` and leaves exactly one primary. **Enforcer owner:** coordinator; package asserts selected timestamps only. **Falsifier:** early success, missing blocked event, or more than one primary. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L108-L137)).

[State index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — No eligible replica does not change the primary

**Predicate:** scenario source expects `NoEligibleReplica`, a matching refused `promote`, then the original WNAM primary in the role map. **Enforcer owner:** coordinator. **Falsifier:** a different decision/error or changed primary. The source only asserts this selected scenario path; it proves no run result. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L271-L302)).

[State index](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Primary failure and source failback scenario

1. Create an in-memory fixture and register WNAM primary plus replicas.
2. Seed explicit healthy heartbeats at `T0`; refresh replicas at `T0+89s`.
3. Call `evaluate` at `T0+90s`; the source expects Enam promotion.
4. Call `promote`; source checks demoted/promoted records and role map.
5. Try WNAM failback before the cooldown; source expects refusal.
6. Call failback at the boundary and inspect final roles and four record types.

These are source statements/assertions only, not an executed timeline. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L36-L137)).

[State index](#r05)

<a id="flows"></a>
### Six scenario flows

| Flow | Source sequence and asserted boundary | Static limit |
|---|---|---|
| `scenario_1_primary_fail_then_failback_after_24h` | WNAM primary → stale heartbeat → `evaluate` proposes ENAM → `promote` emits two audits before role flip → write routing → cooldown refusal → 24h failback and four-event sequence | Logical `T0`/`u64` inputs; not executed |
| `scenario_2a_split_brain_at_registration_rejected` | Register WNAM primary, reject ENAM primary with `SplitBrainRejected`, retain one primary | In-memory map only |
| `scenario_2b_split_brain_at_promote_rejected` | Four-region topology, stale WNAM input, wrong demotion target, reject WEUR promotion, retain WNAM primary | No concurrent caller is created |
| `scenario_3_replica_lag_slo_breach_triggers_reroute` | R2 lag breach on WNAM → `route_write` refusal → ENAM `PromoteReplica` → promotion → ENAM write succeeds | Synthetic lag and no provider/storage path |
| `scenario_status_dashboard_view` | WEUR topology + healthy heartbeats → healthy snapshot; far-future timestamp → stale primary and unhealthy snapshot | Snapshot is not an HTTP `/health` response |
| `scenario_no_eligible_replica_escalates_to_runbook` | All regions over D1 bound → `NoEligibleReplica` and refused promotion; WNAM remains primary | “Runbook” is a signal in test comments, not an invoked operator route |

Time units are explicit: scenario inputs use `u64` milliseconds (`T0 =
1_700_000_000_000`), heartbeat lag fields use seconds, and the failback
constant is `HOT_STANDBY_COOLDOWN_SECONDS = 86_400`; no wall-clock or monotonic
clock is sampled here. The source constructs no threads/tasks and cannot prove
cross-instance concurrency; the upstream coordinator's in-process mutex is an
implementation boundary, not a distributed-lock result.
[↩](#r01)

<a id="r06"></a>
## R06 — Manifest declarations and targets

| Declaration | Static value | Limit |
|---|---|---|
| Package | `name = "e2e-replication-failover"`; `publish = false` | Authoritative identity; not execution evidence |
| Workspace-inherited metadata | `version`, `edition`, `rust-version`, `license` | Effective values not resolved by Cargo here |
| Workspace lints | `[lints] workspace = true` | Effective lint selection not resolved here |
| Library / integration target | `[lib] path = "src/lib.rs"`; `[[test]] name = "scenarios"`, path `tests/scenarios.rs` | Declared target is not selected or run |
| Normal dependencies | `corelink-replication-coordinator` path; `corelink-replica-worker` path; `thiserror` workspace | Coordinator symbols are imported; worker is a declared manifest edge/re-export path; no direct source import of worker or `thiserror` found in the two package files. `thiserror` is therefore a declared normal dependency with no observed local use, not proof of an error contract or runtime edge |
| Dev dependencies / features | Empty `[dev-dependencies]`; no `[features]` declaration | No Cargo resolution performed |

The root manifest lists this package at line 323. No package-local binary/example/bench declaration was found in the manifest. At the pinned tree, literal manifest census found two matching `Cargo.toml` paths: the package manifest and the root workspace member declaration; no other manifest names this package. This is a source census, not a resolved reverse graph.

The exact repeatable search is `git grep -n -I -e 'e2e-replication-failover' -e 'e2e_replication_failover' -- 'Cargo.toml' '**/Cargo.toml'`. This explicitly covers the root manifest and nested manifests. Dependency aliases, selected features, lock resolution, CI target selection, and reverse Cargo consumers remain unknown without the documented inventory/resolution gates. ([manifest](../../../../tests/e2e-replication-failover/Cargo.toml#L1-L25), [workspace](../../../../Cargo.toml#L319-L324)).

<a id="r07"></a>
## R07 — Failure and observability boundaries

| Source condition | Test-source result | Limit |
|---|---|---|
| Coordinator `Result` errors | Fixture helpers propagate `CoordinatorError`; scenario functions use `?` and selected `.err()` matches | Coordinator owns error behavior; no run result here |
| Assertion mismatch | Rust test assertion would fail at the named predicate | Test target is declared, not executed here |
| `NoEligibleReplica` | Scenario checks the signal and unchanged primary | It does not invoke or identify a runbook/operator |
| Audit snapshot | Scenario 1 checks event count/types after operations | No failing audit sink is instantiated in this target; fail-CLOSED behavior is not proved by this package's scenario assertions |
| Runtime/provider signal | None established by fixture source | No audit bus, worker, clock, HTTP route, deployment, or production result observed |

The manifest description and source comments say transitions assert audit-before-mutation, including failure behavior. In the inspected scenario source the successful event sequence is checked after operations; a failing sink and an unchanged-state-on-audit-error predicate are absent. Keep that distinction explicit.

<a id="r08"></a>
## R08 — Verification, provenance, and unknowns

| Claim | Source / method | Result and limit |
|---|---|---|
| Package identity/targets/dependencies | Manifest and root workspace at pinned source commit | `SOURCE`; package/root paths match the integration baseline |
| Fixture and predicates | `src/lib.rs`, `tests/scenarios.rs` | `SOURCE`; no Rust/Cargo/test execution in this review |
| Historical test report | Sealed May 2026 audit §4–5 states 6/6 and lists green gates | Historical statement only; its logs/artifacts and correspondence to this pin were not verified ([audit](../../../../specs/_audits/sealed/2026-05-15-replica-coordinator-production.md)) |
| CI/build/deploy selection | Tracked workflow/script search has no package-specific selector; workspace-wide commands exist in `scripts/ci-bounded-workspace-tests.sh` | `SOURCE` only; whether this package is selected by any current CI lane, artifact or deployment is unknown |
| Current checker | Four profile-S structural checks required by M04 | Documentary only; see [maintenance](MAINTENANCE.md#m04) and author handoff |

Unknown: resolved dependency/target/feature graph and complete reverse Cargo consumer set; current CI selection; whether tests at this pin ran; external `E2EFixture` consumers; production composition root, audit delivery, drill/operator and escalation route; independent reviewer identity. Peer-side reconciliation of the REL keys is also pending. These route and review gaps keep approval blocked. The verified [OKF profile](../../../internal/okf-wiki/01-okf-corelink-profile.contract.md) remains routing context only.

[Relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-e2e-replication-failover/SKILL.md#s01).
