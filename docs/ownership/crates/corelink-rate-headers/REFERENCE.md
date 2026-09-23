---
schema: corelink-ownership/1.1
document: reference
package: corelink-rate-headers
manifest: crates/corelink-rate-headers/Cargo.toml
source_commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
profile: S
state: author_validated
evidence_set: rate-headers-source-static-20260920
---

# corelink-rate-headers — ownership reference

S-profile SOURCE reference for the rate-header and circuit/fake contracts. It
does not establish emitted HTTP headers, a selected provider, D1 activity,
route reachability, test execution, runtime state, or deployment.

[Identity](#r01) · [Boundaries](#r02) · [Implementation](#r03) · [Contracts](#r04) ·
[State and invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Identity and role

| Atomic predicate | Static falsifier / evidence |
|---|---|
| The manifest names package `corelink-rate-headers` and declares `thiserror`, `uuid`, and `proptest` as visible dependency/test declarations. | Renaming/removing the entries in `Cargo.toml` falsifies the source inventory. |

Package role: pure-logic library for typed rate-limit headers and an in-memory circuit model. Manifest declares one lib, two explicit test targets (`prop_rate_headers`, `migration_canonical_0014`), a `uuid/v7` feature, and no bin/build target. `MIGRATION_0014_GLOBAL_CIRCUIT_STATE` embeds `migrations/d1/0014_global_circuit_state.sql`; embedding is not applying it. Source: `Cargo.toml`, `src/lib.rs:220-230`.

The current manifest metadata names repository `https://github.com/HuGR-Labs/corelink-server`; this is Cargo package metadata and does not establish a release or deployment destination. The pinned source commit predates this metadata value; the current manifest was read separately.

<a id="r02"></a>
## R02 — Boundaries and ownership

This package owns the pure header values and in-memory circuit model. HTTP emission, concrete audit/metrics providers, persistence, and D1 migration application belong to consumers or operators. The embedded SQL is a contract input, not evidence of applied schema.

| Atomic predicate | Static falsifier / evidence |
|---|---|
| `src/lib.rs` declares `audit`, `circuit`, `error`, `headers`, and `metrics`, then re-exports their named public surface. | Removing a module declaration or matching `pub use` falsifies this root-route claim. |

<a id="r03"></a>
## R03 — Implementation map

`lib.rs` routes public modules through `audit`, `circuit`, `error`, `headers`, and `metrics`. `headers.rs` implements value construction and rendering; `circuit.rs` implements state and signal evaluation; `audit.rs` and `metrics.rs` define observer seams and fakes. The migration SQL is embedded by `lib.rs`.

| Atomic predicate | Static falsifier / evidence |
|---|---|
| `XRateLimitTypeKind` has source arms mapping to `tenant_quota`, `per_ip`, `per_pat`, `over_quota`, and `global_circuit_open`; `canonical_kind_list` lists five values. | Editing an arm literal, adding/removing a list member, or removing the enum falsifies it in `src/headers.rs`. |

Header bounds are recorded as [INV-001](#inv-001).

<a id="r04"></a>
## R04 — Public contracts

API index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011).

| ID | Exact symbols, inputs, outputs and effects | Error/compatibility, invariants, relation and evidence |
|---|---|---|
| <a id="api-001"></a>API-001 Header builder | `RateLimitHeaderBuilder::build(u64,u64,u64,Vec<RateLimitPolicy>,u64,XRateLimitTypeKind)->RateLimitHeaders`; `build_with_vendor(u64,u64,u64,Vec<RateLimitPolicy>,u64,XRateLimitTypeKind,String,String)->RateLimitHeaders`. Ordered inputs: limit, remaining, reset seconds, policies, retry seconds, kind, then tier/reset UTC. | Pure value constructors, no error/I/O. Remaining and retry clamp to limit and 30-day ceiling; vendor defaults are empty in `build`. Changing field order or units breaks callers. [INV-001](#inv-001); [REL-001](BLAST_RADIUS.md#rel-001); `src/headers.rs:325-396`. [API index](#r04). |
| <a id="api-002"></a>API-002 Global header | `RateLimitHeaderBuilder::for_global_circuit_open(Vec<RateLimitPolicy>)->RateLimitHeaders`; `RateLimitPolicy::new(u64,u64)->RateLimitPolicy`; `RateLimitPolicy::render(&self)->String`. | Global constructor fixes limit/remaining to zero and retry/reset to 60 seconds. Policy inputs are limit and window seconds; rendering has no I/O/error. R04/R05; [REL-001](BLAST_RADIUS.md#rel-001); `src/headers.rs:136-158,399-412`. [API index](#r04). |
| <a id="api-003"></a>API-003 Header rendering/body | `RateLimitHeaders::{render_rate_limit(&self),render_rate_limit_policy(&self),render_retry_after(&self)}->String`; `RateLimitErrorBody::from_headers(&RateLimitHeaders,String,String)->Self`; `render_json(&self)->String`. | Produces local strings/JSON; body mirrors header fields plus message/request ID. No HTTP emission or `Result`. Field names and units are compatibility surfaces. R03/R04; [REL-001](BLAST_RADIUS.md#rel-001); `src/headers.rs:235-274,472-519`. [API index](#r04). |
| <a id="api-004"></a>API-004 Circuit decision | `GlobalCircuitBreaker::check(&self,observation_id:u64,now_ms:u64)->Result<CircuitDecision,CircuitError>`; `record_observation(&self,HealthObservation,now_ms:u64)->Result<CircuitState,CircuitError>`; `record_probe_outcome(&self,success:bool)->Result<(),CircuitError>`. | Decision may reject and emit audit; observations/probes mutate in-memory state. Audit or metrics errors can fail closed. Milliseconds and monotonic observation ID are material. [INV-002](#inv-002), [INV-003](#inv-003); [REL-002](BLAST_RADIUS.md#rel-002); `src/circuit.rs:384-439`. [API index](#r04). |
| <a id="api-005"></a>API-005 Circuit administration | `GlobalCircuitBreaker::manual_override(&self,ManualOverrideTarget,String,String,u64)->Result<CircuitState,CircuitError>`; `snapshot(&self)->Result<CircuitStateSnapshot,CircuitError>`. Ordered override inputs are target, admin ID, reason, now milliseconds. | Override changes state and emits audit/metrics; snapshot reads state. Admin authorization happens before this trait call, not within it. R05; [REL-002](BLAST_RADIUS.md#rel-002); `src/circuit.rs:447-461`. [API index](#r04). |
| <a id="api-006"></a>API-006 Embedded schema | `MIGRATION_0014_GLOBAL_CIRCUIT_STATE: &str`; `rate_headers_schema_version()->u32` returns 14. | Constant embeds migration SQL; neither symbol applies D1 schema or returns an error. Textual migration test is declared, not executed here. [REL-008](BLAST_RADIUS.md#rel-008); `src/lib.rs:229,270-272`. [API index](#r04). |
| <a id="api-007"></a>API-007 Signal helpers | `allow_halfopen_request(observation_id:u64,sample_pct:u8)->bool`; `evaluate_signals(&VecDeque<HealthObservation>,&CircuitThresholds,now_ms:u64)->SignalEvaluation`. | Pure local calculations. The sampler caps input at 100, then admits residues where `observation_id % 10 < min(sample_pct,100) / 10`: effective rate is `10 × floor(min(sample_pct,100)/10)%` across each 10-ID block (9→0%, 19→10%, 99→90%). Evaluation uses a five-minute window and returns flags/rates, not a state transition. [INV-002](#inv-002), [INV-003](#inv-003); [REL-002](BLAST_RADIUS.md#rel-002); `src/circuit.rs:237-317`. [API index](#r04). |
| <a id="api-008"></a>API-008 SLI classification | `XRateLimitTypeKind::counts_against_sli(self,is_manual_override:bool)->bool`. | Pure predicate: `TenantQuota` is true; `GlobalCircuitOpen` is true unless `is_manual_override`; `PerIp`, `PerPat`, and `OverQuota` are false. Caller must set the flag only for a `TripReason::ManualOverride`; the breaker returns that flag on `Reject`. Wrong flag changes the availability SLI classification. [REL-001](BLAST_RADIUS.md#rel-001), [REL-002](BLAST_RADIUS.md#rel-002); `src/headers.rs:79-119`; `src/circuit.rs:619-665`. [API index](#r04). |
| <a id="api-009"></a>API-009 Breaker constructors | `InMemoryGlobalCircuitBreaker::<A,M>::with_defaults(audit:Arc<A>,metrics:Arc<M>,region:impl Into<String>)->Self`; `new(audit:Arc<A>,metrics:Arc<M>,region:impl Into<String>,thresholds:CircuitThresholds)->Self`, where `A:CircuitAuditSink`, `M:CircuitMetricsObserver`. | No `Result` or I/O. `with_defaults` passes `CircuitThresholds::canonical()` (5xx 0.5, p99 1,000,000 µs, DO error 0.3, minimum 100 observations). Both start a per-instance mutex-protected `Closed` state, empty observation buffer, no last evaluation, and zero counters. [REL-002](BLAST_RADIUS.md#rel-002); `src/circuit.rs:174-207,502-577`. [API index](#r04). |
| <a id="api-010"></a>API-010 Audit observer | `CircuitAuditSink::emit(&self,record:CircuitAuditRecord)->Result<(),CircuitAuditSinkError>`; bound `Send+Sync+Debug`. | Persists/emits one record through the selected sink; `Store` is the backend failure. In-memory sink appends to a shared buffer; failing sink returns `Store`. Breaker maps audit error to `CircuitError::Audit` before transition assignment or a `Reject` result, though an observation can already have entered its buffer. No concrete durable provider is selected here. [INV-004](#inv-004); [REL-002](BLAST_RADIUS.md#rel-002); `src/audit.rs:144-225`; `src/circuit.rs:579-752`. [API index](#r04). |
| <a id="api-011"></a>API-011 Metrics observer | `CircuitMetricsObserver` (`Send+Sync+Debug`) has seven `&self` methods, all returning `Result<(),CircuitMetricsObserverError>`: `record_within_quota(kind_label:&'static str,region:&str)`, `record_over_quota(kind_label:&'static str,region:&str)`, `record_trip(region:&str,reason_label:&'static str)`, `record_recovery(region:&str)`, `record_single_signal_alarm(region:&str,signal_label:&'static str)`, `record_manual_override(region:&str,target_state_label:&'static str)`, and `record_rfc9331_compliance(kind_label:&'static str)`. | Each reports one named counter through the selected observer; `Backend(String)` is its failure. The breaker discards metrics results at its current call sites, so observer failure does not block local decisions/transitions. The in-memory and failing observers are test seams, not evidence of a production provider. [INV-004](#inv-004); [REL-002](BLAST_RADIUS.md#rel-002); `src/metrics.rs:82-169`; `src/circuit.rs:619-870`. [API index](#r04). |

<a id="r05"></a>
## R05 — State, flows, and invariants

The in-memory circuit owns `Closed`, `Open`, and `HalfOpen` state. `as_label` maps them to `closed`, `open`, `half_open`; `as_gauge` maps them to 0, 2, 1 respectively. Observation IDs enter `check`'s deterministic half-open sampler; caller-supplied `now_ms` drives audit timestamps, window cutoff, and dwell comparisons. The embedded D1 migration does not make these local transitions durable.

| Current state / call | Source predicate and next state | Effects and failure order |
|---|---|---|
| `Closed` / `record_observation` | Five-minute evaluation needs at least `min_observations` (canonical 100). At least two of 5xx rate > 0.5, maximum observed p99 > 1,000,000 µs, and latest DO error rate > 0.3 gives `Open { tripped_at_ms: now_ms, MultiSignalCombined }`; otherwise stays Closed. | One breached signal records a best-effort alarm. On trip, audit `Tripped` must succeed, then trip count and best-effort metric change, then state/last evaluation are assigned. |
| `Open` / `record_observation` | `now_ms - tripped_at_ms >= 120,000`, sufficient data, and all signals strictly below 90% of their trip thresholds gives `HalfOpen { since_ms: now_ms, reason, probe_success:0, probe_failure:0 }`; `ManualOverride` reason blocks this automatic recovery. Otherwise stays Open. | Audit `HalfOpenProbe` precedes state assignment. The code tests the current evaluation after time since trip; it does not separately track a continuous 120-second healthy interval. |
| `HalfOpen` / `record_observation` | Any one signal above its trip threshold, with sufficient data, immediately returns to Open. Otherwise, after `now_ms - since_ms >= 120,000`, all signals strictly below 50% thresholds, and probe success ratio >= 0.9, becomes Closed. Zero probes count as ratio 1.0. Otherwise stays HalfOpen. | Re-trip audit precedes trip count/metric/state; recovery audit precedes recovery count/metric/state. For a one-signal re-trip, the carried prior reason is reused. |
| Any / `manual_override` | Nonempty `admin_id` and target Open force `Open { ManualOverride }`; target Closed forces Closed, counting a recovery only from Open/HalfOpen. | Empty ID returns `AdminAuthFailed`; then audit `ManualOverride` must succeed before lock/state change. Metrics error is ignored. This method does not authenticate the supplied ID. |
| Any / `check` or `record_probe_outcome` | `check` allows Closed, rejects Open, and permits exactly the sampled HalfOpen IDs; `record_probe_outcome` saturating-increments success/failure only in HalfOpen. | A reject emits `RequestRejected` audit before returning; failure returns `CircuitError::Audit`, while the within-quota metric error is ignored. Probe recording has no audit/metric call. |

`CircuitInner` is held in a per-instance `Arc<Mutex<_>>`, starting empty, capped at 3,000 observations. `record_observation` pushes before evaluation, evicts one front element above the cap, then removes front entries older than `now_ms - 300,000`; `evaluate_signals` filters by timestamp again. A poisoned mutex returns `CircuitError::Backend`.

The observation buffer, counters, and state are process-local and disappear with the instance. Audit failure can leave the newly pushed/pruned observation in memory even though the state assignment is skipped; metrics failures are ignored. Falsifier: a changed branch, constant, lock scope, or observer call order in `src/circuit.rs:208-317,491-870` changes this table.

Invariant index: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

<a id="inv-001"></a>
### INV-001 — Header bounds

Predicate: built `remaining <= limit` and retry-after is at most `RETRY_AFTER_HARD_CEILING_SECS`. Enforcement: `RateLimitHeaderBuilder::build_with_vendor` clamps both before constructing fields (`src/headers.rs:369-396`). Violation: removing either clamp permits out-of-bound output. Verification: inspect both assignments and the declared `prop_rate_limit_remaining_consistent_with_bucket_state` / `prop_retry_after_clamped_to_30_days` cases in `tests/prop_rate_headers.rs`. Current state: SOURCE_CONFIRMED; tests declared, not run for this record.
[Back to invariant index](#r05)

<a id="inv-002"></a>
### INV-002 — Multi-signal trip

Predicate: with enough observations in the five-minute window, one breached signal yields no trip reason and two or three yield `MultiSignalCombined`. Enforcement: `evaluate_signals` counts Boolean breaches and sets `trip_reason` only at `signals_tripped >= 2` (`src/circuit.rs:253-317`); the in-memory breaker handles the single-signal alarm. Violation: a one-signal trip or a two-signal non-trip. Verification: inspect the branch and declared `prop_multi_signal_trigger_no_single_signal_trip` / `prop_multi_signal_combined_trips` in `tests/prop_rate_headers.rs`. Current state: SOURCE_CONFIRMED; tests not run here.
[Back to invariant index](#r05)

<a id="inv-003"></a>
### INV-003 — Half-open quantization

Predicate: for every contiguous ten observation IDs, exactly `floor(min(sample_pct,100)/10)` pass; canonical `HALFOPEN_SAMPLE_PCT=10` passes one. Enforcement: `allow_halfopen_request` uses modulo 10 and integer division (`src/circuit.rs:237-241`). Violation: an admission count differing from that formula, including an assumed 99% admission at input 99. Verification: inspect the expression; declared `prop_halfopen_sample_deterministic` and `prop_halfopen_sample_density_exactly_10_pct` cover canonical 10%, while noncanonical inputs require a separate check. Current state: SOURCE_CONFIRMED; tests not run here.
[Back to invariant index](#r05)

<a id="inv-004"></a>
### INV-004 — Observer seams

Predicate: audit and metrics each expose a trait, an in-memory implementation and a failing implementation; the circuit exposes its trait and in-memory model. Enforcement: declarations in `src/{audit,metrics,circuit}.rs`; consumers inject the observer types. Violation: removing or renaming a listed type breaks this source inventory. Verification: inspect those declarations and the declared `prop_metrics_record_per_reject` / `prop_audit_emit_per_request_rejected` cases. Current state: SOURCE_CONFIRMED inventory only; no provider selection or test execution established.
[Back to invariant index](#r05)

<a id="r06"></a>
## R06 — Configuration, targets, and features

The manifest declares `uuid/v7`, the library, and the two explicit test targets named in R01. There is no bin/build target or feature-selected provider in this package record. `RateLimitHeaderBuilder` takes limit, remaining, reset/retry seconds, tier and reset UTC at the call site; their runtime origin remains with the consumer.

<a id="r07"></a>
## R07 — Failures and observability

The observer inventory and its verification boundary are [INV-004](#inv-004).

Failures: `CircuitError` includes audit, metrics and admin-auth branches; builder/render APIs return value/string rather than `Result`. `FailingCircuitAuditSink` and `FailingCircuitMetrics` are source seams only. Their presence does not prove a selected provider or emitted production signal.

<a id="r08"></a>
## R08 — Verification and evidence

| Atomic predicate | Static falsifier / evidence |
|---|---|
| This record is SOURCE-only: manifest, root/modules, declared tests, and bounded references are the evidence set. | Claiming executed Cargo/test, HTTP emission, provider/D1 action, runtime, deployment, or review without new evidence falsifies its stated boundary. |

Root `uuid/v7` declaration is not resolution. Test targets are declared in the package manifest; neither target was run for this record. Migration 0014 is a static included artifact checked by the text-oriented migration test; actual D1 schema/apply state is UNKNOWN.

The verified canonical OKF route is [storage quota header](../../../knowledge/tenancy/storage-quota-header.md); it is routed only and not revalidated or copied here. Unknown: resolved graph/features, consumer compatibility, compilation, tests, HTTP response behavior, provider/D1 activity, runtime, deployment, and cold-review outcome.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
