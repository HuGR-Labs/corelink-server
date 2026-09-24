---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-replication-failover
manifest: tests/e2e-replication-failover/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-replication-failover-static-20260921
---

# e2e-replication-failover — source relations

These are source-level relationships, not resolved dependency or runtime traces. The package owns its fixture and assertions; the coordinator and replica-worker retain their implementation and contract ownership.

[B01](#b01) · [B02](#b02) · [B03](#b03) · [B04](#b04) · [B05](#b05) · [B06](#b06).

<a id="b01"></a>
## B01 — Scope and highest-risk boundary

The harness links scenario source to the in-memory coordinator. The most material change paths are coordinator decision/audit contracts, the worker-owned `Region` re-export, and local fixture assertions. Nothing here proves that tests were selected or run, that an audit bus or replica worker was reached, or that a failover drill occurred. REL keys use the existing repository namespace `repo:1232040291` documented in [ownership records](../corelink-hash/BLAST_RADIUS.md); peer-side key reconciliation remains pending.

<a id="b02"></a>
## B02 — Inventory and search method

| Population | Source / method | Observed | Limit |
|---|---|---|---|
| Workspace and package declarations | `git grep -n -I -e 'e2e-replication-failover' -e 'e2e_replication_failover' -- 'Cargo.toml' '**/Cargo.toml'`; inspect root/package manifests | 2 matching manifest paths (root member + package identity); package has 2 targets and 3 normal dependency declarations | Cargo metadata/resolution not run |
| Package sources and inverse source references | `git grep -n -I -e 'E2EFixture' -e 'e2e_replication_failover' -- ':!docs/ownership/**'`; inspect the 2 package Rust files | One local integration consumer (`tests/scenarios.rs`); no direct worker/`thiserror` import in package source | Not a complete Cargo reverse graph; lockfile names are not consumers |
| Inverse manifests / aliases | `git grep -n -I -e 'e2e-replication-failover' -e 'e2e_replication_failover' -- 'Cargo.toml' '**/Cargo.toml'` plus later `cargo tree --workspace --locked --offline --invert e2e-replication-failover` | Static count is 2 manifest paths; resolved inverse count is `UNKNOWN` until the cargo command is authorized/executed | A literal search cannot prove selected target/feature reach |
| Non-Cargo paths | `git grep -n -I -e 'e2e-replication-failover' -e 'e2e_replication_failover' -- '.github/**' 'scripts/**' 'Makefile*' 'justfile'` | Historical/spec references exist; no package-specific workflow selector was found; workspace test scripts are broad | Dynamic/generated/external links and actual CI runs remain unknown |
| CI/build/deploy | Inspect `.github/workflows`, `scripts/ci-bounded-workspace-tests.sh`, release/deploy manifests; search exact package and workspace commands | Broad workspace test commands are visible; no package-specific build artifact or deploy target was established | Workflow dispatch/branch/target selection and deployment reach are unknown |
| Runtime/data selection | Static source and configuration only | In-memory collaborators and explicit test inputs visible | No SQL, HTTP, provider, CI run, deployment, or production trace established |

**REL index:** [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018).

<a id="b03"></a>
## B03 — Atomic direct relations

| ID | Type / local direction | Surface and activation | Contract owner |
|---|---|---|---|
| [REL-001](#rel-001) | dependency: harness → coordinator | Normal manifest dependency; package selected | Coordinator |
| [REL-002](#rel-002) | dependency: harness → worker | Normal manifest dependency; package selected | Worker |
| [REL-003](#rel-003) | data: fixture ↔ coordinator | Injected in-memory heartbeat registry | Coordinator |
| [REL-004](#rel-004) | data: coordinator → fixture | Injected in-memory audit sink; scenario 1 reads records | Coordinator |
| [REL-005](#rel-005) | test: setup → coordinator | `init_topology` registers canonical roles | Coordinator |
| [REL-006](#rel-006) | test: scenario 2a → coordinator | Duplicate-primary registration | Coordinator |
| [REL-007](#rel-007) | test: scenario 1 → coordinator | Stale-primary evaluation | Coordinator |
| [REL-008](#rel-008) | test: scenario 1 → coordinator | Promotion and audit ordering | Coordinator |
| [REL-009](#rel-009) | test: scenario 1 → coordinator | Primary/standby write routing | Coordinator |
| [REL-010](#rel-010) | test: scenario 1 → coordinator | Cooldown and failback | Coordinator |
| [REL-011](#rel-011) | test: scenario 2b → coordinator | Promotion with wrong demotion target | Coordinator |
| [REL-012](#rel-012) | test: scenario 3 → coordinator | R2 lag breach and replacement primary | Coordinator |
| [REL-013](#rel-013) | test: status scenario → coordinator | Healthy then stale status snapshot | Coordinator |
| [REL-014](#rel-014) | test: no-eligible scenario → coordinator | No replica meets lag bound | Coordinator |
| [REL-015](#rel-015) | reexport: worker → coordinator → scenario | `Region` public type identity and import | Worker |
| [REL-016](#rel-016) | test: package library → integration target | `E2EFixture` public local API | This package |
| [REL-017](#rel-017) | data: fixture → heartbeat registry | Explicit test timestamps and lag values | Coordinator |
| [REL-018](#rel-018) | dependency: manifest → `thiserror` | Declared workspace dependency with no observed local import | `thiserror` upstream |

Every card separates dependency, data-flow, and impact directions. REL IDs are local navigation anchors; qualified keys use the repository namespace above. The lead must reconcile peer records before closing these relations.

<a id="rel-001"></a>
### REL-001 — Declared coordinator dependency

**Identity / type:** `repo:1232040291:boundary:e2e-rf-coordinator-dependency-001`; `dependency`.
**Endpoints:** package manifest → `corelink-replication-coordinator` manifest.
**Dependency:** harness → coordinator, normal path declaration.
**Data flow:** not-applicable at manifest layer.
**Impact:** coordinator API changes → harness compatibility.
**Surface / activation:** manifest line 17; package/target selection is unresolved.
**Effect / failure:** declaration only; resolution and failure reach are unknown.
**Validation / coordination:** source inspection, no Cargo; coordinator package owns its contracts. ([manifest](../../../../tests/e2e-replication-failover/Cargo.toml#L16-L19)). [Relation index](#b03)


<a id="rel-018"></a>
### REL-018 — Declared `thiserror` dependency

**Identity / type:** `repo:1232040291:boundary:e2e-rf-thiserror-dependency-001`; `dependency`.
**Endpoints:** package manifest → workspace `thiserror` package.
**Dependency:** harness → `thiserror`, normal workspace declaration.
**Data flow:** none observed in the two package Rust files.

**Impact:** resolution/version or lint policy changes may affect package selection; no local API consumer was found.
**Surface / activation:** `Cargo.toml:20`; activation requires Cargo resolution/target selection.
**Effect / failure:** declared edge may resolve or fail independently of visible source use; do not infer an error contract.
**Validation / coordination:** static manifest/source census only; use the dependency owner for version questions. ([manifest](../../../../tests/e2e-replication-failover/Cargo.toml#L16-L21)). [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — Declared worker dependency

**Identity / type:** `repo:1232040291:boundary:e2e-rf-worker-dependency-001`; `dependency`.
**Endpoints:** package manifest → `corelink-replica-worker` manifest.
**Dependency:** harness → worker, normal path declaration.
**Data flow:** not-applicable at manifest layer.
**Impact:** selected worker changes may affect harness resolution.
**Surface / activation:** manifest line 18; package/target selection is unresolved.
**Effect / failure:** no direct worker import appears in the two package Rust files.
**Validation / coordination:** source search only, no Cargo; worker owns its contracts. ([manifest](../../../../tests/e2e-replication-failover/Cargo.toml#L16-L19), [package import](../../../../tests/e2e-replication-failover/src/lib.rs#L18-L24)). [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — Injected in-memory heartbeat registry

**Identity / type:** `repo:1232040291:boundary:e2e-rf-heartbeat-injection-001`; `data`.
**Endpoints:** fixture → coordinator constructor → injected registry.
**Dependency:** harness → coordinator.
**Data flow:** bidirectional; fixture records, coordinator reads latest values.
**Impact:** coordinator trait changes → fixture setup and scenarios.
**Surface / activation:** `new`, `record`, and `Arc<dyn HeartbeatRegistry>` at fixture creation.
**Effect / failure:** poisoned memory lock propagates `CoordinatorError`; no worker emission.
**Validation / coordination:** source only; coordinator owns the trait. ([composition](../../../../tests/e2e-replication-failover/src/lib.rs#L43-L57), [trait](../../../../crates/corelink-replication-coordinator/src/heartbeat.rs#L74-L96)). [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — In-memory audit capture

**Identity / type:** `repo:1232040291:boundary:e2e-rf-audit-injection-001`; `data`.
**Endpoints:** coordinator → injected sink; scenario 1 → captured records.
**Dependency:** harness → coordinator.
**Data flow:** coordinator emits; scenario reads the sink snapshot.
**Impact:** audit trait/event changes → fixture and audit assertions.
**Surface / activation:** `emit`/`records` during promotion and failback.
**Effect / failure:** sink lock poison can fail emit; no external bus/delivery is shown.
**Validation / coordination:** source assertions only; no failing sink here; coordinator owns the contract. ([fixture](../../../../tests/e2e-replication-failover/src/lib.rs#L47-L57), [sink](../../../../crates/corelink-replication-coordinator/src/audit.rs#L67-L115), [assertions](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L76-L90)). [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — Canonical topology setup

**Identity / type:** `repo:1232040291:boundary:e2e-rf-topology-fixture-001`; `test`.
**Endpoints:** `init_topology` helper → coordinator `register`.
**Dependency:** harness → coordinator.
**Data flow:** primary/replica roles → `register`; `Result` returns to fixture.
**Impact:** registration semantics → fixture setup.
**Surface / activation:** registers one primary and remaining `Region::ALL` entries per scenario setup.
**Effect / failure:** first registration error stops the helper; split-brain policy stays upstream.
**Validation / coordination:** static source; coordinator owns registration. ([helper](../../../../tests/e2e-replication-failover/src/lib.rs#L60-L78)). [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — Duplicate-primary rejection

**Identity / type:** `repo:1232040291:boundary:e2e-rf-duplicate-primary-001`; `test`.
**Endpoints:** scenario 2a → coordinator `register`.
**Dependency:** harness → coordinator.
**Data flow:** second primary registration → error returned to scenario.
**Impact:** rejection contract → scenario 2a predicate.
**Surface / activation:** two registrations in `scenario_2a_split_brain_at_registration_rejected`.
**Effect / failure:** wrong variant or changed role count falsifies the source assertion.
**Containment / validation:** local in-memory map; source only, not run. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L139-L162)).
**Coordination:** coordinator package owns the contract. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — Stale-primary evaluation

**Identity / type:** `repo:1232040291:boundary:e2e-rf-lifecycle-evaluate-001`; `test`.
**Endpoints:** scenario 1 → coordinator `evaluate`.
**Dependency:** harness → coordinator.
**Data flow:** explicit `Region`/millisecond inputs → `PromotionDecision`.
**Impact:** freshness/eligibility changes → scenario decision assertion.
**Surface / activation:** `evaluate(Region::Wnam, T0 + 90_000)`.
**Effect / failure:** unexpected decision or candidate fails the assertion.
**Validation:** source only, not run; coordinator owns semantics. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L98-L108)). [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — Promotion and audit ordering

**Identity / type:** `repo:1232040291:boundary:e2e-rf-lifecycle-promote-001`; `test`.
**Endpoints:** scenario 1 → coordinator `promote` → audit sink.
**Dependency:** harness → coordinator.
**Data flow:** old/new regions and time → role/audit result.
**Impact:** promotion or event ordering changes → event and role assertions.
**Surface / activation:** upstream `promote(Wnam, Enam, T0 + 90_001)` emits audit events before role insertion; the harness reads the sink only after the call.

**Effect / failure:** upstream order is a coordinator invariant; wrong events or role transition falsifies harness predicates, but no failing sink or mutation boundary is exercised.
**Validation:** upstream source fact plus post-operation scenario assertions; coordinator owns audit/roles. ([implementation](../../../../crates/corelink-replication-coordinator/src/coordinator.rs#L417-L465), [scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L110-L137)). [Relation index](#b03)


<a id="rel-009"></a>
### REL-009 — Primary and standby write routing

**Identity / type:** `repo:1232040291:boundary:e2e-rf-lifecycle-write-001`; `test`.
**Endpoints:** scenario 1 → coordinator `route_write`.
**Dependency:** harness → coordinator.
**Data flow:** target region/time → `Result` returned to scenario.
**Impact:** role-based write admission changes → routing assertions.
**Surface / activation:** ENAM succeeds; WNAM `HotStandby` returns an internal error.
**Effect / failure:** writes accepted/refused contrary to source predicate.
**Validation:** fixture-local source only; no storage write is reached. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L123-L131)). [Relation index](#b03)


<a id="rel-010"></a>
### REL-010 — Cooldown and failback

**Identity / type:** `repo:1232040291:boundary:e2e-rf-lifecycle-failback-001`; `test`.
**Endpoints:** scenario 1 → coordinator `failback` → audit sink/role map.
**Dependency:** harness → coordinator.
**Data flow:** WNAM and explicit boundary times → errors, events, final roles.
**Impact:** cooldown arithmetic or failback mutation changes → selected predicates.
**Surface / activation:** one call before and one at `86_400` seconds after promotion.
**Effect / failure:** early success or wrong final role/event sequence falsifies source.
**Validation:** source only; no operator/runbook path. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L108-L137)). [Relation index](#b03)


<a id="rel-011"></a>
### REL-011 — Wrong-demotion split-brain rejection

**Identity / type:** `repo:1232040291:boundary:e2e-rf-split-brain-promote-001`; `test`.
**Endpoints:** scenario 2b → coordinator `promote`.
**Dependency:** harness → coordinator.
**Data flow:** supplied regions/time → call; rejection and role map → scenario.
**Impact:** promotion validation changes → scenario 2b.
**Surface / activation:** wrong demotion target while WNAM remains primary.
**Effect / failure:** expected `SplitBrainRejected` and unchanged-primary predicate may differ.
**Containment / validation:** local map; source only, not run. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L164-L196)).
**Coordination:** coordinator package owns the contract. [Relation index](#b03)


<a id="rel-012"></a>
### REL-012 — Lag-breach write refusal and promotion

**Identity / type:** `repo:1232040291:boundary:e2e-rf-lag-scenario-001`; `test`.
**Endpoints:** scenario 3 → coordinator methods.
**Dependency:** harness → coordinator.
**Data flow:** explicit R2 lag/time → calls; errors/decision/map → assertions.
**Impact:** SLO or decision changes → scenario 3.
**Surface / activation:** `route_write`, `evaluate`, `promote`, then write to new primary.
**Effect / failure:** wrong refusal, candidate, or primary count falsifies source predicate.
**Containment / validation:** fixture-local; source unexecuted. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L198-L235)).
**Coordination:** coordinator package owns these contracts. [Relation index](#b03)


<a id="rel-013"></a>
### REL-013 — Status snapshot

**Identity / type:** `repo:1232040291:boundary:e2e-rf-status-scenario-001`; `test`.
**Endpoints:** status scenario → coordinator `replication_status`.
**Dependency:** harness → coordinator.
**Data flow:** explicit `now_ms` → status snapshot.
**Impact:** status shape/health rules → scenario assertions.
**Surface / activation:** healthy and stale timestamps in the status scenario.
**Effect / failure:** wrong health, role count, freshness, or region falsifies predicates.
**Containment / validation:** in-memory snapshot, no `/health` request; source only. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L237-L269)).
**Coordination:** coordinator package owns the contract. [Relation index](#b03)


<a id="rel-014"></a>
### REL-014 — No-eligible result

**Identity / type:** `repo:1232040291:boundary:e2e-rf-no-eligible-scenario-001`; `test`.
**Endpoints:** no-eligible scenario → coordinator `evaluate` and `promote`.
**Dependency:** harness → coordinator.
**Data flow:** per-region D1 lag → calls; decision/errors/map → scenario.
**Impact:** eligibility changes → no-eligible predicates.
**Surface / activation:** all regions receive over-bound lag in the final scenario.
**Effect / failure:** unexpected promotion or changed primary falsifies assertions.
**Containment / validation:** source does not call a runbook; source only, not run. ([scenario](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L271-L302)).
**Coordination:** coordinator owns the contract; operator is unknown. [Relation index](#b03)


<a id="rel-015"></a>
### REL-015 — Worker-owned `Region` re-export

**Identity / type:** `repo:1232040291:boundary:e2e-rf-region-reexport-001`; `reexport`.
**Endpoints:** worker `Region` → coordinator re-export → scenario import.
**Dependency:** harness → coordinator; coordinator → worker by its manifest.
**Data flow:** not-applicable (type identity).
**Impact:** worker type change → coordinator export → harness compatibility.
**Surface / activation:** coordinator `lib.rs:145-147`; compile selection unknown.
**Effect / failure:** source import/type mismatch; no runtime behavior follows.
**Validation / coordination:** static source; worker owns the type. ([definition](../../../../crates/corelink-replica-worker/src/region.rs#L34-L50), [re-export](../../../../crates/corelink-replication-coordinator/src/lib.rs#L145-L147), [import](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L21-L26)). [Relation index](#b03)


<a id="rel-016"></a>
### REL-016 — Local library to integration target

**Identity / type:** `repo:1232040291:boundary:e2e-rf-library-scenarios-target-001`; `test`.
**Endpoints:** `scenarios` target → this package's library target.
**Dependency:** integration target → local library target.
**Data flow:** fixture values/method calls → scenario; results/errors → assertions.
**Impact:** `E2EFixture` API changes → integration target source.
**Surface / activation:** `use e2e_replication_failover::E2EFixture` in scenario source.
**Effect / failure:** signature change breaks source/build if selected.
**Validation / coordination:** source only; package human maintainer is unknown. ([target](../../../../tests/e2e-replication-failover/Cargo.toml#L13-L25), [import](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L21-L34)). [Relation index](#b03)


<a id="rel-017"></a>
### REL-017 — Explicit heartbeat test inputs

**Identity / type:** `repo:1232040291:boundary:e2e-rf-heartbeat-test-inputs-001`; `data`.
**Endpoints:** fixture/scenarios → coordinator-provided registry contract.
**Dependency:** test source → coordinator.
**Data flow:** `record(Heartbeat)` receives synthetic region/time/lag; coordinator reads registry.
**Impact:** heartbeat/freshness changes → affected scenario predicates.
**Surface / activation:** helper and explicit healthy/stale/lag inputs.
**Effect / failure:** registry error propagates; freshness/SLO semantics remain upstream.
**Validation / coordination:** source only; coordinator owns registry. ([inputs](../../../../tests/e2e-replication-failover/tests/scenarios.rs#L58-L64), [registry](../../../../crates/corelink-replication-coordinator/src/heartbeat.rs#L74-L96)). [Relation index](#b03)


<a id="b04"></a>
## B04 — Material propagation and reachability limits

Static source path: `tests/scenarios.rs` → local `E2EFixture` → coordinator-owned in-memory orchestrator → coordinator-owned heartbeat/audit traits. `Region` originates in replica-worker, is re-exported by coordinator, then imported by the scenario. The package has three declared normal dependencies; `thiserror` has no observed local import and is classified as a manifest/resolution edge only.

At the pinned tree, the direct manifest census found two matching Cargo manifests (root member declaration plus package manifest), while the inverse package consumer count remains unresolved until Cargo tree/metadata is run. Non-Cargo search found historical specs/audit references but no package-specific workflow selector. Reverse Cargo edges, resolved feature/target reach, generated callers, CI selection, and production composition are unknown; no absence claim follows from literal search.

| Destination | Witness | Effect if selected / changed | Limit |
|---|---|---|---|
| Coordinator | REL-001, REL-003–REL-014, REL-017 | Contract/behavior changes can alter source compatibility or scenario expectations | No resolved graph or test execution |
| Replica-worker `Region` | REL-002, REL-015 | Type edit may flow through coordinator re-export into scenario source | No worker behavior is invoked by this harness source |
| `thiserror` resolution | REL-018 | Dependency version/feature changes may affect package resolution | No local source consumer or resolved graph established |
| Local scenario target | REL-016 | Fixture API changes affect the declared test consumer | Other Cargo consumers unknown |
| CI/build/deploy | Non-Cargo literal search; workspace scripts | A broad workspace lane could select the package if configured | No package-specific selector, artifact or deploy edge established |
| Operators, external systems | No verified source edge | No causal path confirmed in supplied static scope | Selection/operator/dynamic links remain unknown |

<a id="b05"></a>
## B05 — Change, impact, validation, recovery

| Change | REL/API/INV | Local impact | Required validation / coordination |
|---|---|---|---|
| Fixture API or setup | REL-003, REL-005, REL-016; API-001–003 | Scenario compilation/input predicates may change | Profile checks for docs; future selected package checks by lead; coordinator contract owner for signature changes |
| Coordinator decision, role, heartbeat, or audit contract | REL-003–REL-014, REL-017; INV-001–005 | Expected decisions, errors, roles, or captured events may change | Coordinate with coordinator owner; do not claim runtime from this harness |
| Worker `Region` contract | REL-002, REL-015 | Coordinator re-export and scenario type compatibility may change | Coordinate with worker owner; verify complete consumer graph separately |
| Manifest/workspace/CI selection | REL-001–002, REL-016, REL-018 | Package/target reach may change | Lead must run authorized metadata/tree/CI inventory gates outside this source-static authorship |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Inspected/documented | Excluded or unknown |
|---|---|---|
| Package manifest declarations | `git grep` exact-name census: 2 matching manifest paths; package fields, inherited fields, library, test, 3 normal deps, empty dev deps | Resolved versions/features and selected targets |
| First-party relations | Coordinator and worker declarations; fixture, API, re-export, target source paths; 18 atomic REL cards after splitting scenario 1 | Peer-side key reconciliation and full inverse Cargo graph |
| Inverse package consumers | Static Cargo.toml search returned no package consumer manifest beyond root membership + self; exact `cargo tree --workspace --locked --offline --invert e2e-replication-failover` is specified but not run | Resolved inverse packages, aliases, target-specific and feature-gated consumers |
| Non-Cargo paths | Literal source/config/workflow/spec search; historical audit references recorded; broad workspace CI commands recorded | Dynamic/generated/external consumers, CI execution and operator assignment |
| Build/deploy | `.github`, scripts and release/deploy path search for package name and workspace commands | Workflow dispatch, artifact production, deployment and environment reach |
| Runtime/data/provider | In-memory fixture inputs and captured records | Durable state, bus delivery, replica copying, staging/production drill |

Cross-package closure is explicitly pending. The proposed shared fingerprints
`repo:1232040291:boundary:e2e-rf-coordinator-dependency-001`,
`repo:1232040291:boundary:e2e-rf-worker-dependency-001`, and
`repo:1232040291:boundary:e2e-rf-region-reexport-001` are recorded here with
qualified endpoints.

The peer package records at their own source pins do not yet carry reconciled
reciprocal cards. Until the lead compares endpoint, contract owner, activation
and failure facts on both sides, these relations remain
`PENDING_PEER_RECONCILIATION`, not closed coverage.

Historical claims are not treated as executed evidence for this pin. The exact cold review and reviewer identity are outstanding. No escalation owner or runtime operator is inferred.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01).
