---
schema: corelink-ownership/1.1
document: reference
package: corelink-runner-aggregate
manifest: crates/corelink-runner-aggregate/Cargo.toml
source_commit: cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6
profile: S
state: author_validated
evidence_set: runner-aggregate-main-readback-20260923
---

# corelink-runner-aggregate — ownership reference

This source reference is pinned to `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`. It describes the local Rust package contract only. Manifest descriptions and source comments mention a credentialed workflow, D1 writes, and billing phases; this packet does not establish that those paths are wired or executed. Verified OKF remains canonical and is routed through the [wave plan](../../WAVE_010_PLAN.md).

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and role

| Field | Source-defined value |
|---|---|
| Package / manifest | `corelink-runner-aggregate` / `crates/corelink-runner-aggregate/Cargo.toml` |
| Library | `src/lib.rs` |
| Binary | `runner-aggregate-run`, `src/bin/runner-aggregate-run.rs` |
| Tests | Unit tests in `src/lib.rs`; integration tests in `tests/runner_aggregate_run_bin.rs` |
| Role | Pure transformation from staged runner events and prior state into counter, chain-head, and shadow-ledger values |
| Evidence | `SOURCE` at `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`; no execution claim |

The manifest inherits workspace version, edition, Rust version, license, publish policy, and lint configuration. It declares three first-party dependencies plus six external crates; declaration means build input, not runtime reachability.

<a id="r02"></a>
## R02 — Boundaries and ownership

This package owns the local input validation, global batch idempotency-key deduplication, grouping, per-region chain-builder calls, counter/head output mapping, and per-tenant cumulative shadow calculation. It consumes event vocabulary and hash-chain contracts from `corelink-billing-emit` and `corelink-billing-aggregator`, and conversion/pricing helpers from `corelink-runner-overage`.

The package does not own those imported contracts. No inspected Rust implementation here reads staging, persists the returned rows, coordinates a watermark, obtains tenant terms, schedules a run, calls a runner, charges money, contacts Stripe, or deploys the binary. Those effects and their authorities are unknown at this source boundary.

<a id="r03"></a>
## R03 — Implementation map

| Path | Responsibility |
|---|---|
| `Cargo.toml` | Package identity, dependencies, inherited workspace policy, and one explicit bin target |
| `src/lib.rs` | Public data/error types, terms validation, aggregation, chain mapping, shadow calculation, unit tests |
| `src/bin/runner-aggregate-run.rs` | Stdin, JSON parse, library call, pretty JSON stdout, stderr reporting, exit code |
| `tests/runner_aggregate_run_bin.rs` | Source-defined subprocess contract cases for valid input, malformed JSON, and empty staged input; not executed evidence |

The public entry point is `aggregate_runner_usage(&RunnerAggregateInput)`. Internal helpers parse 32-byte hex, validate immutable terms bindings, and calculate checked cumulative shadow charge. The function walks `BTreeMap` keys for stable region/tenant ordering. It deduplicates idempotency keys globally within one supplied batch before group accumulation.

<a id="r04"></a>
## R04 — Public contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006).

<a id="api-001"></a>
### API-001 — Aggregation function and artifact version

Exact symbol: `aggregate_runner_usage(&RunnerAggregateInput) -> Result<RunnerAggregateOutput, RunnerAggregateError>`; supported `RUNNER_AGGREGATE_ARTIFACT_CONTRACT_VERSION` is `3`, and `RUNNER_VCPU_SECONDS_PRODUCT` is `"runner_vcpu_seconds"`. The function rejects any other input version. Preconditions include valid supplied terms for every staged/prior tenant. It returns values only; no I/O or clock read occurs. Failures include unsupported version, malformed event/head, invalid prior/terms, chain break, and checked arithmetic overflow. [Index](#r04)

<a id="api-002"></a>
### API-002 — Input records and immutable terms

Exact symbols: `RunnerAggregateInput`, `StagedRunnerEvent`, `PriorChainHead`, `PriorTenantConsumption`, `TenantPeriodTerms`, and `TenantPeriodTerms::expected_snapshot_digest_hex`. Input carries `billing_period`, `period_start_ms`, `period_end_ms`, `now_ms`, prior consumption by tenant, staged rows, required tenant-period terms, and optional region heads (serde-defaulted). Terms bind tenant, period, allowance, whole cents per vCPU-hour, nonempty snapshot ref, and canonical lower-case 64-hex BLAKE3 digest. Prior consumption must match product, period, and the exact terms ref/digest. Units are milliseconds, vCPU-seconds, cents, and `u128` cumulative quantities. Missing/mismatched terms fail closed. [Index](#r04)

<a id="api-003"></a>
### API-003 — Staged-event to counter values

Exact symbols: `StagedRunnerEvent` and `CounterRow`. Event fields are tenant UUID, region string, `qty_vcpu_seconds: u128`, 64-hex `idem_key_hex`, and `time_ms`. Keys must decode to 32 bytes; duplicate decoded keys are skipped globally within the call. Remaining values group by region then tenant; checked quantity/count accumulation rejects overflow. Each row carries group coordinates, period, quantity, count, supplied `now_ms`, and previous/own chain digest hex. Event `time_ms` does not supply the aggregation timestamp. [Index](#r04)

<a id="api-004"></a>
### API-004 — Chain values and output

Exact symbols: `PriorChainHead`, `ChainHeadUpdate`, `CounterRow`, `RunnerAggregateOutput`. Each visited region starts a new builder or resumes the supplied head/sequence. Each aggregate uses fixed UUIDv5 namespace and `(tenant,region,period)` name; pre-append head maps to `prev_hash_hex`, append result to `own_digest_hex`. One post-append update per visited region returns head, next sequence, and period. Empty input emits empty vectors. Persistence and concurrency are not established. [Index](#r04)

<a id="api-005"></a>
### API-005 — Shadow ledger values

Exact symbols: `ShadowLine`, `SkipNote`, and output fields `shadow_ledger`, `skipped`, `total_shadow_millicents`. For each tenant with grouped usage, checked batch usage is added to prior consumption; snapshot allowance applies across regions. The emitted millicent charge is cumulative charge after minus before; cents are display conversion, and vCPU-hours use an imported exact-decimal helper. Valid terms are required, not silently skipped. `skipped` is empty in this implementation. This value does not charge. [Index](#r04)

<a id="api-006"></a>
### API-006 — Errors and binary process contract

Exact symbols: non-exhaustive `RunnerAggregateError` variants `UnsupportedArtifactContractVersion`, `MalformedEvent`, `MalformedChainHead`, `ChainBreak`, `InvalidPriorConsumption`, `InvalidTenantPeriodTerms`, and `ArithmeticOverflow`; binary `main`. The binary reads all stdin, parses JSON, calls the function, pretty-serializes output, writes one JSON line to stdout, writes diagnostics and success summary to stderr, and returns failure for read/parse/aggregate/serialize/write errors. Callers must allow future error variants. Integration-test source names valid, malformed-JSON, and empty-input cases; results have not been executed in this evidence set. [Index](#r04)
[Back to start](#r01)

<a id="r05"></a>
## R05 — Falsifiable invariants

| ID | Predicate → enforcement point | Violation condition → verification | State |
|---|---|---|---|
| INV-001 | Version equals 3 at function entry. | Another version succeeds → guard; named rejection test, not executed. | Source-confirmed |
| INV-002 | Terms bind tenant, period, nonempty ref, and canonical digest; every staged/prior tenant has them. | A mismatch reaches output → validator and missing-terms test, not executed. | Source-confirmed |
| INV-003 | A decoded idempotency key counts once per invocation before grouping. | A duplicate changes output → key set and named duplicate test, not executed. | Source-confirmed |
| INV-004 | Region/tenant order is deterministic; material quantity/count/total arithmetic is checked. | Overflow wraps or partial output succeeds → checked branches and named tests, not executed. | Source-confirmed |
| INV-005 | Supplied valid head/sequence seeds each region builder; row hashes are pre-head/append result; update reflects post-append state. | Mapping differs → call mapping and deterministic/resume tests, not executed. | Source-confirmed |
| INV-006 | Charge delta equals `F(prior + batch) - F(prior)` under the same snapshot and tenant-wide allowance. | Delta or regional partition result differs → source formula and allowance/partition tests, not executed. | Source-confirmed |

“Source-confirmed” means the predicate and enforcement are visible in the pinned code; it does not certify tests or external persistence.

<a id="r06"></a>
## R06 — Configuration, targets, and features

There are no package feature declarations or target-specific dependencies in this manifest. It declares library source by Cargo convention and one `[[bin]]`: name `runner-aggregate-run`, path `src/bin/runner-aggregate-run.rs`. Direct dependencies: `thiserror`, `serde`, `serde_json`, `hex`, `blake3`, `uuid` with `v5` and `serde`, plus `corelink-billing-emit`, `corelink-billing-aggregator`, and `corelink-runner-overage`. All use workspace dependency definitions. Exact selected features/resolved graph are not recorded here; no target build inclusion or deployment is claimed.

<a id="r07"></a>
## R07 — Failures and observability

Library failures include unsupported contract version; malformed staged key; malformed supplied chain head; imported append rejection wrapped as `ChainBreak`; invalid prior product/period/terms binding; invalid terms identity/period/ref/digest; and checked arithmetic overflow. The bin source emits contextual errors to stderr and exits failure for read, parse, aggregate, serialization, and stdout-write errors. On source success it emits pretty JSON to stdout and a shadow-only count/amount summary to stderr.

There is no inspected retry, log backend, durable rollback, alert, external trace, runner telemetry, or operator recovery implementation. Process-output behavior is source-defined and integration-test-covered in source only; no observed output is asserted.

<a id="r08"></a>
## R08 — Verification, evidence, and unknowns

The evidence pin is `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`. Inspected files are the manifest, library, bin source, and integration-test source. Evidence class is `SOURCE`; no Rust tests, binary invocation, feature resolution, runtime, deployment, or consumer census is certified.

Success means identity, contracts, invariants, and errors match those files. Completeness means R01–R08 link targets, APIs, test-source examples, direct declarations, local relations, and exclusions. Quality separates static facts from operation. Done requires the matching structural check, whitespace check, and independent cold review; none implies runtime proof.

Unknowns include reverse consumers and aliases; selected features and shipped target graph; test execution; workflow reachability; staging completeness and extraction; terms source/authority; D1 schema/read/write/atomicity; watermark coordination; concurrent chain updates; actual runner operation; billing/Stripe behavior; telemetry; deployment; and operational rollback.

[Back to start](#r01)
