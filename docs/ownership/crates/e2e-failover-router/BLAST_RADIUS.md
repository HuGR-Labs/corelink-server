---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-failover-router
manifest: tests/e2e-failover-router/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: e2e-failover-router-static-20260921
---

# e2e-failover-router — blast radius

Source-level relationships only. This package is a test harness; its declarations and assertions do not prove that CI selected or executed a target, or that a failover drill ran.

[B01 Scope](#b01) · [B02 Method](#b02) · [B03 Direct relationships](#b03) · [B04 Transitive propagation](#b04) · [B05 Change validation](#b05) · [B06 Coverage](#b06).

<a id="b01"></a>
## B01 — Scope

The scoped producer is package `e2e-failover-router` at `tests/e2e-failover-router/Cargo.toml`, comprising its library and `scenarios` integration-test target. Review authority follows implementation ownership: this package owns its logical clock, lease ledger, and assertions; `corelink-failover-router` owns router/probe APIs; `corelink-replica-worker` owns region graph and replica audit types; an authorized operations owner owns staging/production drills. Sources: manifest, `src/lib.rs`, `tests/scenarios.rs`, and the two upstream crate roots.

<a id="b02"></a>
## B02 — Method and populations

**Inventory:** inspected the manifest targets and direct dependencies, plus the library and integration-test sources cited below. The manifest declares router, replica-worker, and `thiserror`; tests import router types and receive `Region`/`ResidencyGraph` through router re-exports.

**Resolved graph:** not inspected for a selected target/feature set. **Semantic source:** traced fixture construction, helper calls, assertions, and upstream re-export definitions. Reverse search is bounded to these named sources; CI selection, additional consumers, and external users are unknown. No test was executed for this review.

| Population | Method at pinned source | Found/documented | Excluded or unknown |
|---|---|---|---|
| Manifest and package targets | Read manifest target/dependency/features declarations | Library, `scenarios` integration target, two first-party dependencies and `thiserror` | Resolved versions, selected target/features, and build graph unknown |
| Package source | Read `src/lib.rs` and all scenario source; trace named public helpers into each use | Clock, ledger, fixture, five scenarios, graph and error helpers | No test execution or measurement |
| Direct reverse consumers | Search package name/manifest path and inspect named package manifests | No reverse consumer established in the bounded source set | Exhaustive workspace/external callers unknown |
| Upstream peers | Read router manifest/root/router/audit and replica region source | Router API and replica-owned types documented in REL-001/002 | Bilateral relation fingerprint/review is `NOT_RECONCILED` |
| CI and operations | Search named workflow/config sources for manifest, target, and scenario selectors | No selector/execution evidence used here | CI selection, staging/production drills and operation owners unknown |

<a id="b03"></a>
## B03 — Direct relationships

<a id="rel-001"></a><a id="rel-002"></a><a id="rel-003"></a><a id="rel-004"></a><a id="rel-005"></a>

**REL-001 — Harness fixture to in-memory router.** Identity: `repo:1232040291:boundary:e2e-failover-router-fixture-router`. Producer: `fresh_router()`; consumer: five scenario arms invoking `route_read`/`write_mode`.

**Surface/activation:** fixture returns `InMemoryFailoverRouter` with injected in-memory probe and audit sink; a test calls it.

**Contract/effect:** constructor and trait-object composition follow upstream source.

**Failure propagation:** a fixture/API change can break scenario compilation/assertions; it says nothing about a live probe or route.

**Validation/coordination:** compare fixture and router constructor/error paths; coordinate `corelink-failover-router`.

**Peer reconciliation:** router-side paired record/fingerprint is not established (`NOT_RECONCILED`).

**Evidence:** `tests/scenarios.rs` lines 41–120; router `src/router.rs` lines 136–164.

**REL-002 — Replica types through router re-export.** Identity: `repo:1232040291:boundary:e2e-failover-router-replica-region-reexport`. Producer: `corelink-replica-worker::{Region,ResidencyGraph}`; consumer: router crate root re-export, then this package's imports/assertions.

**Surface/activation:** upstream source export plus test target imports.

**Contract/effect:** preserves type identity and graph lookup API; no replica-copy effect follows.

**Failure propagation:** rename/type change can break both re-export and tests.

**Validation/coordination:** inspect replica definitions, router `src/lib.rs`, and test import/graph assertions; coordinate both owners.

**Peer reconciliation:** neither upstream ownership record nor a shared fingerprint is reconciled (`NOT_RECONCILED`).

**Evidence:** replica `src/region.rs` lines 16–45, 182–236; router root lines 69–87; scenario lines 23–27, 366–378.

**REL-003 — Logical clock to scenario timestamps.** Identity: `repo:1232040291:boundary:e2e-failover-router-logical-clock`. Producer: local `LogicalClock::{now_ms,advance_ms}`; consumer: scenario method inputs and timestamp assertions. **Surface/activation:** mutex-backed logical `u64`, called directly by tests. **Contract/effect:** saturating increment and poison-to-error mapping; not wall time. **Failure propagation:** clock signature or boundary changes alter local assertions. **Validation/coordination:** inspect helper and each changed call/assertion; coordinate this package only. **Peer reconciliation:** local relation; no external peer applies. **Evidence:** `src/lib.rs` lines 48–89; scenario lines 35–39, 61–84.

**REL-004 — Lease ledger to scenario assertions.** Identity: `repo:1232040291:boundary:e2e-failover-router-local-lease-ledger`. Producer: local `WriteLeaseLedger`; consumer: holder/count/snapshot assertions.

**Surface/activation:** tests bootstrap and append caller-supplied entries, then query the same ledger.

**Contract/effect:** insertion-order history with lookup by `timestamp <= t`; it is separate from router state.

**Failure propagation:** changed query/order behavior changes scenario outcomes; no distributed lease or exclusivity guarantee follows.

**Validation/coordination:** compare methods and all uses; coordinate package-local contract.

**Peer reconciliation:** local relation; it is explicitly not the router lease interface.

**Evidence:** `src/lib.rs` lines 92–195; scenario lines 148–207 and 213–247.

**REL-005 — Scenario-local audit fixture.** Identity: `repo:1232040291:boundary:e2e-failover-router-scenario-audit-fixture`. Producer: scenario 4 manually constructs a resolved record and emits to in-memory sink; consumer: local record-order/time assertions.

**Surface/activation:** direct test statements.

**Contract/effect:** records stay in local sink.

**Failure propagation:** event shape/sink changes alter the assertion; router-originated resolution and transport are not tested by this edge.

**Validation/coordination:** compare scenario emission and sink API; coordinate replica/router owners for upstream changes.

**Peer reconciliation:** replica/router audit record peers are not reconciled (`NOT_RECONCILED`).

**Evidence:** scenario lines 249–293; router `src/audit.rs` lines 1–14.

<a id="b04"></a>
## B04 — Transitive propagation

Router API changes propagate from the upstream router surface into the fixture and scenario calls (REL-001). Replica-owned `Region`/`ResidencyGraph` flow through a separate router re-export into scenario imports and graph assertions (REL-002). These are distinct source/compile relationships; neither relation proves runtime routing or replica behavior. The local clock, ledger, and manually emitted audit assertions remain package-local (REL-003, REL-004, REL-005).

The local ledger is not wired into router lease state. Direct audit-fixture emission is not a router-produced event. No additional transitive runtime edge is evidenced.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Impact boundary | Source validation and coordination |
|---|---|---|
| Manifest target/dependency/feature | May alter REL-001/002 compilation or declared test population | Compare manifest, target sources and bounded CI selector search; coordinate package/integration owner; graph/execution remain unknown |
| Router/probe signature or route semantics | Changes REL-001 fixture/call/assertion surface | Trace changed method through fixture and scenario calls; coordinate router owner; source checks only unless separately authorized execution evidence exists |
| Replica type, graph, or router re-export | Changes REL-002 type identity/import/assertions | Compare replica definition, router re-export, exact scenario imports/assertions; reconcile peer; coordinate both upstream owners |
| Logical clock contract | Changes API-001/INV-001 and REL-003 timestamps | Compare mutex/error/saturating-add source and every call/assertion; remain local-clock only |
| Ledger query/order contract | Changes API-002/INV-002 and REL-004 holder assertions | Compare insert/query code and scenario uses; preserve no-router-integration boundary |
| Scenario-authored audit record | Changes API-003/INV-003 and REL-005 local record assertion | Compare direct construction/emission and sink assertions; keep router-origin and transport unknown |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Covered by this record | Remaining limit |
|---|---|---|
| Declared targets/dependencies | Library, `scenarios`, two first-party crates, and `thiserror` | Resolved versions/features and selected CI jobs unknown |
| Package APIs/assertions | Clock, local ledger, fixture, five scenarios, graph/error helpers | No target execution or measured SLO |
| Upstream interfaces | Router API, probe/audit interfaces, region graph re-export | No bilateral peer reconciliation; no live probe, distributed lease, replica copy or router-origin resolution proof |
| External operations | Not included as observed population | Drill, route traffic, audit delivery/durability, telemetry, deployment and production remain unknown |
| Reverse consumers | Bounded named-source search | External/generated callers are unknown |

A source search or re-export does not resolve these unknowns. Coordinate implementation changes with the defining crate; route operational proof through the authorized operations owner and canonical OKF process.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-e2e-failover-router/SKILL.md#s01)
