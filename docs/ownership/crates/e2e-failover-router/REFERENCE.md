---
schema: corelink-ownership/1.1
document: reference
package: e2e-failover-router
manifest: tests/e2e-failover-router/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: e2e-failover-router-static-20260921
---

# e2e-failover-router source reference

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08)

<a id="r01"></a>
## R01 — Identity and purpose

At source commit `cb94e251c0f17382565bf863f517945cbb2a84d6`, `[package].name` is `e2e-failover-router`; the package is `publish = false` and declares a library plus one integration target named `scenarios`. The manifest description calls it a DR-16 harness and lists five scenario arms. That is declared intent, not execution evidence. Sources: [manifest](../../../../tests/e2e-failover-router/Cargo.toml#L1-L27), [library root](../../../../tests/e2e-failover-router/src/lib.rs#L1-L6).

<a id="r02"></a>
## R02 — Ownership boundary

| Surface | Owner | Boundary |
|---|---|---|
| Logical clock and write-lease ledger | This package | Test-local state and helper API only ([lib.rs](../../../../tests/e2e-failover-router/src/lib.rs#L48-L210)) |
| Probe, route decision, router errors | `corelink-failover-router` | Upstream static API; harness composes its in-memory types ([router.rs](../../../../crates/corelink-failover-router/src/router.rs#L66-L136)) |
| Region graph and replica audit taxonomy | `corelink-replica-worker` | Owner remains upstream despite router re-exports ([router lib.rs](../../../../crates/corelink-failover-router/src/lib.rs#L69-L87)) |
| Drill, staging, and production operation | Unknown | No operation or runtime result is established here |

<a id="r03"></a>
## R03 — Targets, modules, and direct dependencies

| Item | Static declaration or role |
|---|---|
| Library | `src/lib.rs`: `LogicalClock`, `LeaseEntry`, `WriteLeaseLedger`, `HarnessError`; forbids unsafe code ([source](../../../../tests/e2e-failover-router/src/lib.rs#L42-L52), [types](../../../../tests/e2e-failover-router/src/lib.rs#L92-L210)) |
| Integration target | `tests/scenarios.rs`: fixture, constants, five named scenario tests, graph check, ledger check, error check ([manifest](../../../../tests/e2e-failover-router/Cargo.toml#L21-L27), [test declarations](../../../../tests/e2e-failover-router/tests/scenarios.rs#L35-L55), [tests](../../../../tests/e2e-failover-router/tests/scenarios.rs#L61-L405)) |
| First-party dependencies | Manifest declares `corelink-failover-router` and `corelink-replica-worker`; test source imports `corelink_failover_router`, while `Region` and `ResidencyGraph` are re-exported there. No direct `corelink_replica_worker` import appears in these two package source files ([manifest](../../../../tests/e2e-failover-router/Cargo.toml#L13-L19), [re-export](../../../../crates/corelink-failover-router/src/lib.rs#L69-L87)). |
| Other dependency | `thiserror` comes from the workspace; resolved versions/features are not established by this manifest ([manifest](../../../../tests/e2e-failover-router/Cargo.toml#L13-L19)). |

The package manifest has no feature section. The workspace lint table is inherited; its effective settings and the resolved build graph are outside this reference.

**API and invariant index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003).

<a id="api-001"></a>
### API-001 — Logical clock
**Symbols:** `LogicalClock::{new,now_ms,advance_ms}`. **Input/output:** initial/current `u64` milliseconds; `advance_ms` uses saturating addition. **Failure:** poisoned mutex maps to `HarnessError::ClockPoisoned`. **Effect/limit:** test-local state, not wall clock or elapsed-time evidence. **Evidence:** `src/lib.rs`; impact [REL-003](BLAST_RADIUS.md#rel-003). [Index](#r06)

<a id="api-002"></a>
### API-002 — Write-lease ledger
**Symbols:** `WriteLeaseLedger::{bootstrap,handover,holder_at,snapshot,distinct_holder_count}` and `LeaseEntry`. **Input/output:** caller-supplied region/holder/timestamp rows; lookup returns the last insertion-order entry with timestamp `<= t`. **Failure:** poisoned mutex maps to `HarnessError::LedgerPoisoned`. **Effect/limit:** local vector only; no ordering validation, router integration, or distributed lease guarantee. **Evidence:** `src/lib.rs`; impact [REL-004](BLAST_RADIUS.md#rel-004). [Index](#r06)

<a id="api-003"></a>
### API-003 — In-memory router fixture
**Symbol:** scenario-local `fresh_router`. **Inputs:** fixed test fixture/probe/sink; **output:** `InMemoryFailoverRouter` behind the upstream API. **Failure:** router methods return upstream `FailoverError`; tests unwrap selected results. **Effect/limit:** fresh in-memory collaborators for each fixture call. **Evidence:** `tests/scenarios.rs`; impact [REL-001](BLAST_RADIUS.md#rel-001). [Index](#r06)

<a id="inv-001"></a>
### INV-001 — Clock advances monotonically with saturation
**Predicate:** `advance_ms(delta)` stores `saturating_add(delta)` and returns the resulting value. **Falsifier:** ordinary wrapping/addition or a different returned value. **Verification:** source and helper assertions; execution unknown. **State:** SOURCE. Related API/edge: [API-001](#api-001), [REL-003](BLAST_RADIUS.md#rel-003). [Index](#r06)

<a id="inv-002"></a>
### INV-002 — Ledger lookup is insertion-order local state
**Predicate:** `holder_at(t)` selects the last inserted row whose timestamp is at most `t`; it does not validate handover order or consult router state. **Falsifier:** sorted-time selection, validation, or router coupling. **Verification:** source and scenario assertions; execution unknown. **State:** SOURCE. Related API/edge: [API-002](#api-002), [REL-004](BLAST_RADIUS.md#rel-004). [Index](#r06)

<a id="inv-003"></a>
### INV-003 — Resolution audit is scenario-authored
**Predicate:** scenario 4 constructs and emits its resolved record directly into the local sink. **Falsifier:** the scenario obtains that record from the router instead. **Verification:** scenario source; no router emission/execution claim. **State:** SOURCE. Related API/edge: [API-003](#api-003), [REL-005](BLAST_RADIUS.md#rel-005). [Index](#r06)

<a id="r04"></a>
## R04 — Local contracts

| Symbol | Falsifiable source contract | Source |
|---|---|---|
| `LogicalClock::{new,now_ms,advance_ms}` | Stores a `u64` behind `Mutex`; reads return its current value; advance uses `saturating_add`; mutex poison maps to `ClockPoisoned`. | [clock](../../../../tests/e2e-failover-router/src/lib.rs#L48-L89) |
| `WriteLeaseLedger::{bootstrap,handover,holder_at,snapshot,distinct_holder_count}` | Bootstrap inserts one row; handover appends without ordering/holder validation; `holder_at(t)` returns the last insertion-order row whose timestamp is `<= t`; snapshot clones rows. | [ledger](../../../../tests/e2e-failover-router/src/lib.rs#L92-L195) |
| `HarnessError` | Non-exhaustive enum with clock-poison, ledger-poison, and invariant text variants. | [errors](../../../../tests/e2e-failover-router/src/lib.rs#L197-L210) |
| `fresh_router` | Builds per-call in-memory probe and sink behind trait-object `Arc`s, then constructs `InMemoryFailoverRouter`. | [fixture](../../../../tests/e2e-failover-router/tests/scenarios.rs#L41-L55) |

<a id="r05"></a>
## R05 — Test predicates and limits

| Test source predicate | Failure witness in source | Limit |
|---|---|---|
| Healthy WNAM decision is primary, writes allowed, overhead is within the router constant, and no detected record exists. | Any compared field, overhead, or event assertion differs. | Assertion source only; no measured SLO or run is reported. ([scenario 1](../../../../tests/e2e-failover-router/tests/scenarios.rs#L61-L94)) |
| Injected WEUR degradation returns SAM replica/read-only decision and a detected record with the expected pair and trigger text. | Route, mode, pair, count, or detail assertion differs. | This test does not compare mutation time with audit time; the ordering comment is not a separate assertion. ([scenario 2](../../../../tests/e2e-failover-router/tests/scenarios.rs#L100-L142)) |
| Scenario 3 manually records WNAM→ENAM, calls a degraded route, then compares blocked primary mode and ledger holder over 120 half-second steps. | Either comparison or the two-holder count differs. | Router state and test ledger are separate objects; the ledger is not a router lease integration. ([scenario 3](../../../../tests/e2e-failover-router/tests/scenarios.rs#L148-L207)) |
| Scenario 4 checks recovery routes to WNAM, manually appends failback, emits a resolved record directly, then compares detected/resolved order and timestamps. | Route, ledger snapshot, audit order, or timestamp comparison differs. | This harness emits `failover.resolved`; it does not establish that the router emits it. ([scenario 4](../../../../tests/e2e-failover-router/tests/scenarios.rs#L213-L293)) |
| Scenario 5 compares a custom 5xx/latency/failure state with a later injected-degraded case; the graph test asserts acyclicity and pinned sibling pairs. | Compared mode, region, failover flag, repeat route, or graph assertion differs. | Values and assertions are source only; no per-object routing or runtime behavior is established. ([scenario 5 and graph](../../../../tests/e2e-failover-router/tests/scenarios.rs#L295-L378)) |

The two final helpers assert lookup at selected ledger times and an error display substring; neither is an external failover check ([helpers](../../../../tests/e2e-failover-router/tests/scenarios.rs#L380-L405)).

<a id="r06"></a>
## R06 — Configuration and composition

The package declares no features or service configuration. Its source uses a fixed `T0_MS`, a test tenant string, and in-memory router/probe/audit types. The manifest declares direct paths to both first-party crates, but a declaration does not prove resolved dependency selection or that a target is selected by CI. Sources: [manifest](../../../../tests/e2e-failover-router/Cargo.toml#L13-L27), [fixtures](../../../../tests/e2e-failover-router/tests/scenarios.rs#L35-L55).

<a id="r07"></a>
## R07 — Failure, observability, and composition boundaries

Local mutex poisoning has explicit `HarnessError` variants. Router `route_read` exposes `FailoverError`, while the scenario target unwraps several calls under integration-test lint allowances. The upstream in-memory router source selects a sibling and calls the audit sink before constructing its degraded decision; that is upstream code, not a result of running this package ([router implementation](../../../../crates/corelink-failover-router/src/router.rs#L174-L246), [test allowances](../../../../tests/e2e-failover-router/tests/scenarios.rs#L13-L19)).

The harness stores audit records in a local in-memory sink and asserts selected
record order/fields. No metric, log, alert, external audit delivery, or runtime
observation is defined by the package artifacts inspected here. Scenario-local
assertion failures are test failures only if a target is actually executed;
CI selection and execution remain unknown. See
[REL-005](BLAST_RADIUS.md#b03) for the local audit boundary.

The source comments point to `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` as scenario context. That reference does not show that a drill script, staging operation, audit transport, replica worker, or production route executed ([scenario header](../../../../tests/e2e-failover-router/tests/scenarios.rs#L1-L11)).

<a id="r08"></a>
## R08 — Evidence status and unknowns

Evidence class: source-static at the pinned commit. The five limits in [the ownership skill](../../../../.claude/skills/own-e2e-failover-router/SKILL.md#s02) apply here: declarations are not resolved selection; signatures/re-exports do not transfer behavior ownership; imports do not prove invocation; fixtures/tests do not prove execution; and OKF is a routing destination only. The verified [OKF profile](../../../internal/okf-wiki/01-okf-corelink-profile.contract.md) is not copied or revalidated.

**Contract crosswalk:** API-001/INV-001 → [REL-003](BLAST_RADIUS.md#rel-003); API-002/INV-002 → [REL-004](BLAST_RADIUS.md#rel-004); API-003/INV-003 → [REL-001](BLAST_RADIUS.md#rel-001) and [REL-005](BLAST_RADIUS.md#rel-005). The upstream graph/re-export contract is in [REL-002](BLAST_RADIUS.md#rel-002); its defining types remain owned by `corelink-replica-worker`.

Unknown: whether Cargo compiled or ran this target; which targets CI selects and which features/dependency versions resolve; and whether any staging or production failover drill, provider, audit delivery, or deployment occurred. A structural document check cannot resolve these unknowns or approve the content.

[Skill](../../../../.claude/skills/own-e2e-failover-router/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
