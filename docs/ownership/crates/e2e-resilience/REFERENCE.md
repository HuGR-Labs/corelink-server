---
schema: corelink-ownership/1.1
document: reference
package: e2e-resilience
manifest: tests/e2e-resilience/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-e2e-resilience-source-1177dad2
---

# e2e-resilience — source ownership reference

Static inventory at the pinned source commit. The manifest description states intent; the package source shows a local harness and scenario assertions, not execution or production wiring.

[Identity](#r01) · [Ownership](#r02) · [Source map](#r03) · [Contracts](#r04) ·
[State and axioms](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and purpose

| Field | SOURCE evidence |
|---|---|
| Cargo identity | `[package].name = e2e-resilience`; `tests/e2e-resilience/Cargo.toml:1-3` |
| Workspace membership | `Cargo.toml:295`; source pin `1177dad2ca2a9f21c29b5a118aa7944b77147798` |
| Targets | Library target path `src/lib.rs` (`Cargo.toml:13-14`); explicit `[[test]]` target `name = "scenarios"`, path `tests/scenarios.rs` (`Cargo.toml:24-26`) |
| First-party declarations | `corelink-ratelimit` and `corelink-rate-headers`; manifest `:16-20` |
| Features and publication | No package `[features]`; `publish = false`, manifest `:8-22` |
| Evidence class | Source-static; no Cargo selection, test result, or runtime was inspected |

The local code supplies deterministic helpers and six integration-test functions. “End-to-end” in the manifest is a package description, not proof of HTTP, network, production middleware, or deployment execution.

<a id="r02"></a>
## R02 — Ownership boundary

| Surface | Implementation owner | Contract owner | Operation / escalation |
|---|---|---|---|
| Clock, response shape, local queue, fixtures, assertions | `e2e-resilience` source | This package's local source contract | No separate operational owner identified |
| Token bucket, rate-limit audit and metrics traits | `corelink-ratelimit` | `corelink-ratelimit` | Not established by this harness |
| Circuit state, audit, metrics, retry constant | `corelink-rate-headers` | `corelink-rate-headers` | Not established by this harness |
| Production Tower composition and live request handling | Not assigned by this package's source | Not assigned here | UNKNOWN; stop and locate the actual composition root |
| Repository review routing | Not an implementation owner | `.github/CODEOWNERS` default `@gmhelmold` requests review; it is not a merge control (`.github/CODEOWNERS:3-27,45-50`) | Does not define incident escalation |

The test target uses upstream **in-memory** limiter and breaker implementations, sinks, and metrics. `BackPressureQueue` and `AlwaysFailingCircuitAuditSink` are local test models. The local queue does not own or prove the production Tower `ConcurrencyLimit` layer described by comments (`src/lib.rs:181-190`; `tests/scenarios.rs:388-401`). No contract ownership transfers across a manifest edge.

<a id="r03"></a>
## R03 — Source and target map

| Path / symbols | Responsibility and nature | Evidence |
|---|---|---|
| `src/lib.rs` | Owned local `LogicalClock`, `HttpStatus`, `ResilienceResponse`, `BackPressureQueue`, `BackPressureRejectRecord`, `ResilienceError`; forbids unsafe code | `:51-55,57-356` |
| `tests/scenarios.rs` | Explicit `scenarios` test target; local fixtures, a failing circuit sink, and six `#[test]` functions | Manifest `:24-26`; source `:51-139,145-475` |
| `corelink-ratelimit` imports | In-memory limiter, audit/metrics sinks, key, config, outcomes, rate-limit events | `tests/scenarios.rs:37-40,60-72,145-216` |
| `corelink-rate-headers` imports | In-memory breaker, circuit contracts, audit/metrics, and circuit retry constant | `tests/scenarios.rs:28-35,81-139,222-342,388-471` |
| `uuid` / `thiserror` | UUID value construction is test-only; `thiserror::Error` derives the local error implementation | Manifest `:19-20`; `src/lib.rs:55,343-355`; `tests/scenarios.rs:26,77-79` |

The package has no declared binary, build script, package feature, or dev-dependency. It inherits version, edition, Rust version, license, and lints from the workspace (`Cargo.toml:4-22`); effective values and target resolution were not queried.

<a id="r04"></a>
## R04 — Local and composed contracts

**Index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Logical clock

**Exact symbols:** `LogicalClock::new(t0_ms: u64) -> Self`, `LogicalClock::now_ms(&self) -> Result<u64, ResilienceError>`, and `LogicalClock::advance_ms(&self, delta_ms: u64) -> Result<u64, ResilienceError>`. Input unit is Unix-epoch milliseconds; precondition is an owned clock, output is the stored/advanced `u64`, and `ClockPoisoned` is the lock error. `advance_ms` uses saturating addition. This local source clock has no distributed-time guarantee. Evidence: `src/lib.rs:57-107`; implementation `yes`, source-callsite wired `yes`, selected-target/build inclusion `unknown`, runtime-verified `unknown`; compatibility is local Rust API. Links: [INV-001](#inv-001), [FLOW-001](#flow-001), [REL-005](BLAST_RADIUS.md#rel-005), [REL-002](BLAST_RADIUS.md#rel-002), [REL-011](BLAST_RADIUS.md#rel-011).

[Contract index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Response status model

**Enum:** `#[non_exhaustive] HttpStatus::{Ok, TooManyRequests, ServiceUnavailable}`. **Struct/fields:** `ResilienceResponse { status: HttpStatus, retry_after_secs: Option<u64> }`. **Signatures:** `HttpStatus::as_u16(self)->u16`; fully qualified `ResilienceResponse::ok()->Self`, `ResilienceResponse::too_many_requests(u64)->Self`, `ResilienceResponse::service_unavailable(u64)->Self`. **State:** `as_u16` impl yes/wired no callsite found; `ok` yes/yes (`try_admit`); `too_many_requests` yes/no callsite found; `service_unavailable` yes/yes (`try_admit`, circuit helper); runtime unknown. **Precondition:** valid enum/`u64`; **post:** Ok=200/None, TooManyRequests=429/Some(x), ServiceUnavailable=503/Some(x). Pure/no errors; local Rust/status compatibility, not wire. Evidence `src/lib.rs:110-179`; links [INV-002](#inv-002), [REL-005](BLAST_RADIUS.md#rel-005), [REL-011](BLAST_RADIUS.md#rel-011).

Divergence: upstream `CircuitDecision::Reject` documents 429 (`crates/corelink-rate-headers/src/circuit.rs:360-400`); local helper returns 503 (`tests/scenarios.rs:131-138`). Intentional mapping versus defect is unresolved; not canonical HTTP behavior.

[Contract index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Back-pressure queue

**Symbols/signatures:** `with_capacity(usize)->Self`; `depth(&self)->Result<usize,ResilienceError>`; `audit_snapshot(&self)->Result<Vec<BackPressureRejectRecord>,ResilienceError>`; `arm_audit_failure(&self)->Result<(),ResilienceError>`; `try_admit(&self,u64,u64,u64)->Result<ResilienceResponse,ResilienceError>`; `drain_one(&self)->Result<(),ResilienceError>`.

**Fields/effects:** `BackPressureRejectRecord` has `request_id:u64`, `now_ms:u64`, `retry_after_secs:u64`, `event_type:&'static str`. Units are milliseconds and seconds. Capacity is inclusive: below it depth increments; at it a reject is recorded or armed `AuditFailed` returns. `depth` returns depth; `audit_snapshot` returns chronological records; `arm_audit_failure` is one-shot; `drain_one` is a zero-depth no-op. Mutex poison returns `BackPressurePoisoned`; state is depth/records/fail flag. Compatibility: local Rust API/record shape; no production Tower equivalence. Evidence `src/lib.rs:181-340`; implementation yes, source-callsite wired yes, selected-target/build inclusion and runtime unknown. Links: [INV-002](#inv-002), [FLOW-002](#flow-002), [REL-005](BLAST_RADIUS.md#rel-005).

[Contract index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Scenario composition

**Exact scenario symbols:** private helpers `canonical_tenant`, `fresh_rate_limiter`, `fresh_breaker`, `five_xx`, `ok_obs`, and `circuit_reject_to_response`; exact `#[test]` functions `rate_limit_per_tenant_burst_over_quota_rejects_excess_with_429_and_audit`, `token_bucket_refills_after_one_logical_second_and_admits_next_request`, `circuit_breaker_opens_on_five_consecutive_5xx_then_rejects_fast`, `circuit_breaker_half_open_probe_success_closes_failure_reopens`, `back_pressure_rejects_when_queue_depth_exceeds_threshold_with_503_and_retry_after`, and `audit_fail_closed_at_circuit_layer_blocks_state_change_before_rejection` (`tests/scenarios.rs:77,146-482`). Preconditions are the declared `scenarios` target and resolved upstream APIs; output is assertion coverage, not a production result. Evidence: `tests/scenarios.rs:60-139,145-475`; implementation/wiring `yes/yes`, execution `unknown`; links: [FLOW-003](#flow-003), [REL-002](BLAST_RADIUS.md#rel-002), [REL-011](BLAST_RADIUS.md#rel-011).

The private `AlwaysFailingCircuitAuditSink` implements `CircuitAuditSink::emit(&self, CircuitAuditRecord) -> Result<(), CircuitAuditSinkError>` and always returns `Err(CircuitAuditSinkError::Store("scenario 6: failing sink".to_string()))` (`tests/scenarios.rs:393-401`). It is a local fixture, not the upstream `FailingCircuitAuditSink`; each upstream crate retains its API/implementation contract. No scenario execution is established.

[Contract index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State, flow, invariants, and five axioms

| State | Owner and lifetime | Read/write evidence |
|---|---|---|
| Logical milliseconds | `LogicalClock`; local `Mutex<u64>` for object lifetime | `src/lib.rs:57-107` |
| Queue depth, reject records, fail flag | `BackPressureQueue`; local mutexes and vector, no persistence | `src/lib.rs:191-196,204-209,219-253,275-306` |
| Rate bucket, breaker, audit and metrics | Upstream in-memory implementations created per scenario; provider objects, not durable stores | `tests/scenarios.rs:60-105,225-257,411-471` |

**State flow:** scenario creates a fixed clock and in-memory fixture → sends timestamps/observations or requests → inspects decision/state/audit snapshot → discards local fixture. The source has no database, storage, network client, or customer-data call in this path.

<a id="flow-001"></a>
### FLOW-001 — Logical time into provider decisions

`LogicalClock` creates a fixed `u64` instant; scenario helpers pass `now_ms` into `try_acquire`, `record_observation`, and queue admission; assertions then inspect decisions/snapshots before the fixture is dropped (`src/lib.rs:61-107`; `tests/scenarios.rs:146-216,223-348`). This is a source flow only: implementation and source-callsite wiring are `yes`; selected-target/build inclusion and runtime verification are `unknown`.

[State index](#r05)

<a id="flow-002"></a>
[↩](#r01)
### FLOW-002 — Local back-pressure rejection

`BackPressureQueue::try_admit` reads depth, either increments admission or checks the failure flag, appends `BackPressureRejectRecord`, and returns `ResilienceResponse`; `drain_one` reduces local depth (`src/lib.rs:191-326`; `tests/scenarios.rs:349-390`). No Tower or external queue call is established; implementation and source-callsite wiring are `yes`; selected-target/build inclusion and runtime verification are `unknown`.

[State index](#r05)

<a id="flow-003"></a>
[↩](#r01)
### FLOW-003 — Composed rate-limit/circuit scenarios

`scenarios` constructs upstream in-memory sinks/implementations, invokes `try_acquire`, `record_observation`, `check`, and `record_probe_outcome`, then asserts state/audit/metrics-adjacent outputs (`tests/scenarios.rs:60-139,145-342,388-475`). The fixture-to-upstream source edge is implemented and statically wired; target selection, execution, and production reachability remain `unknown`.

[State index](#r05)

**Invariant index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003).

<a id="inv-001"></a>
[↩](#r01)
### INV-001 — Clock advance saturates

Predicate: for `old <= u64::MAX`, `advance_ms(delta)` returns `min(old + delta, u64::MAX)` and a poisoned mutex returns `ClockPoisoned`. Enforcement: `LogicalClock::advance_ms` (`src/lib.rs:90-105`). Violation: replacing `saturating_add` or changing the poison mapping. Verification: source review plus the `token_bucket_refills_after_one_logical_second_and_admits_next_request` call site; execution state is `UNKNOWN` in this evidence set. Current state: implemented `yes`, source-callsite wired `yes`, selected-target/build wired `unknown`, runtime-verified `unknown`. Scope is harness clock only, not monotonic OS time. [R05](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Local queue audits before rejection response

Predicate: when `depth >= capacity`, `try_admit` checks `audit_should_fail` before appending a record or returning 503; armed failure returns `AuditFailed` and leaves the audit vector unchanged. Enforcement: `BackPressureQueue::try_admit` (`src/lib.rs:257-306`). Violation: response/record emitted before the check, or failure mutates the record vector. Verification: four admits, fifth rejection, snapshot, drain/re-admit, and failure sub-case (`tests/scenarios.rs:348-390,473-482`); execution state is `UNKNOWN`. Current state: implemented `yes`, source-callsite wired `yes`, selected-target/build wired `unknown`, runtime-verified `unknown`; no production queue wiring follows. [R05](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Circuit trip assertion aborts on failing audit

Predicate: with private `AlwaysFailingCircuitAuditSink`, the fifth 5xx observation returns `Err(CircuitError::Audit)` through the upstream error surface and the breaker remains `Closed` with `trips_count == 0`. Enforcement: local `emit` returns `CircuitAuditSinkError::Store("scenario 6: failing sink")`; upstream `record_observation` and local assertions (`tests/scenarios.rs:388-471`). Violation: state opens or trip count advances despite audit failure.

Verification: `audit_fail_closed_at_circuit_layer_blocks_state_change_before_rejection`; execution state `UNKNOWN`. Current state: local assertion implemented/wired `yes`; upstream behavior and runtime verification `unknown`. The breaker implementation is owned by `corelink-rate-headers`; this is not an executed conformance result. [R05](#r05) · [REL-011](BLAST_RADIUS.md#rel-011)

**Five evidence axioms:** (1) manifest/source prove declarations and visible source only; (2) signatures/re-exports do not prove behavior beyond their implementation; (3) imports/traits do not prove invocation or a complete graph; (4) fakes/assertions do not prove execution or provider behavior; (5) the wave plan and verified OKF profile route questions but do not duplicate or redefine policy. These limits apply to all three INV records.
[↩](#r01)

<a id="r06"></a>
## R06 — Configuration, targets, and features

| Item | Declared value | Limit |
|---|---|---|
| Package targets | `src/lib.rs`; explicit `[[test]] scenarios` at `tests/scenarios.rs` | Declaration is not selection or run |
| Features | No package `[features]` section | Do not invent or pass `--all-features` as package policy |
| Environment | Source fixtures use constants and arguments; no package env lookup appears in these files | No claim about external runner configuration |
| Workspace fields | Version, edition, Rust version, license, lint table inherited | Effective values not resolved |
| Network/storage | The package source uses local models and has no I/O client import | Does not prove transitive dependencies or any deployment state |

<a id="r07"></a>
## R07 — Errors and observability

| Signal | Source meaning | Limit |
|---|---|---|
| `ClockPoisoned` | Clock mutex lock failed | No injected poison scenario shown |
| `BackPressurePoisoned` | Local queue mutex lock failed | No external queue behavior |
| `AuditFailed` | Armed local queue audit path aborts before response | Does not identify a production audit sink |
| `RateLimitDecision` / `CircuitDecision` | Upstream in-memory decision arms consumed by assertions | API/behavior owner remains upstream |
| Test panic/assertion | Scenario source marks a predicate mismatch | No test result is observed by this document |

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set `w015-e2e-resilience-source-1177dad2` covers the pinned manifest, root workspace declaration, library, scenario source, direct-manifest inverse search, workflow command declarations, `.github/CODEOWNERS`, and outside-Cargo package-name matches. Here `wired` means a source callsite; selected target/build inclusion is separate and remains `unknown`. The five evidence axioms above remain in force. No Cargo, Rust, test, fuzz, network, provider, GitHub, deployment, database, storage, or production command was run.

| Record | Evidence anchor | Implemented | Wired | Runtime-verified | State/limit |
|---|---|---|---|---|---|
| API-001 / INV-001 / FLOW-001 | `src/lib.rs:57-107`; `tests/scenarios.rs:195-216` | yes | source-callsite yes; selected-target unknown | unknown | Source/static only; no execution proof |
| API-002 / FLOW-002 | `src/lib.rs:110-179`; `tests/scenarios.rs:349-390` | yes | yes | unknown | Local response model only |
| API-003 / INV-002 / FLOW-002 | `src/lib.rs:181-340`; `tests/scenarios.rs:348-390,473-482` | yes | source-callsite yes; selected-target unknown | unknown | No production Tower equivalence |
| API-004 / INV-003 / FLOW-003 | `tests/scenarios.rs:223-342,388-475` | yes | yes | unknown | Upstream behavior and target execution unresolved |

Unknowns: (1) Cargo-resolved dependency versions/features and selected targets; (2) whether any local or CI invocation ran and its result; (3) actual production Tower composition and the owner of that composition root; (4) live audit/metrics delivery, storage, and provider effects; (5) deployed/runtime behavior and an operational escalation route.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
