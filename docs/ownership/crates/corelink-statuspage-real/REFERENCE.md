---
schema: corelink-ownership/1.1
document: reference
package: corelink-statuspage-real
manifest: crates/corelink-statuspage-real/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w010-statuspage-real-source-static-20260920
---

# corelink-statuspage-real — ownership reference

SOURCE-only reference for target-selected external-adapter declarations and
local fakes. It does not claim a Statuspage API request, credential use,
network result, deployment, scheduler execution, or runtime reachability.

[Identity](#r01) · [Targets](#r02) · [Backend](#r03) · [Report](#r04) · [Axioms](#r05) · [Adapters](#r06) · [Fakes](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| The manifest package is `corelink-statuspage-real`; the crate root declares the listed local modules and re-exports the package surface. | Renaming the package, removing a module declaration, or changing its matching `pub use` in `Cargo.toml` or `src/lib.rs` falsifies this inventory. |

<a id="r02"></a>
## R02 — Target-selection invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| The manifest declares `reqwest` only for `cfg(not(target_arch = "wasm32"))` and declares `worker`, `wasm-bindgen`, `wasm-bindgen-futures`, and `js-sys` only for `cfg(target_arch = "wasm32")`; `src/lib.rs` gates `http` and `wasm32_backend` with the corresponding predicates. | Changing either manifest target block or either crate-root `cfg` falsifies the declared split. This does not prove target selection, linking, or execution. |

<a id="r03"></a>
## R03 — Backend contract invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `StatuspageBackend` requires `publish_dsr_metric(&DsrCompletionReport, u64) -> Result<PublishOutcome, StatuspageClientError>` and has source implementations for the native client and `InMemoryStatuspageBackend`; the wasm client exposes a separate async method instead of this trait implementation. | Editing the trait signature, implementation blocks, or wasm public method in `src/{backend,http,memory,wasm32_backend}.rs` falsifies the contract. It does not prove a caller invokes either implementation. |

<a id="r04"></a>
## R04 — Report and bridge invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `DsrCompletionReport::new` accepts only a source-visible 86,400-second window and p95 at most 7,200; `to_metric_body` constructs a local JSON value from the window end and p95; `bridge_to_report` forwards the named fields from `DsrCompletionStats`. | Altering either validation branch, JSON keys/fields, or bridge field mapping in `src/{report,dsr_bridge}.rs` falsifies this record. It does not prove aggregation input, serialization, or external receipt. |

<a id="r05"></a>
## R05 — Five source axioms

| ID | Falsifiable SOURCE axiom | Static falsifier / limit |
|---|---|---|
| AX-SP-01 | Native `http` and wasm `wasm32_backend` are exposed by mutually opposed crate-root target predicates. | Change a `cfg` or target dependency block; no selected-target or build claim follows. |
| AX-SP-02 | A report constructor rejects a non-24-hour window and p95 above 7,200 before returning a report. | Remove/change the two branches in `src/report.rs`; no statement about actual DSR data follows. |
| AX-SP-03 | The native client and in-memory fake call their local audit sink before their corresponding caller-visible success or rate-limit result; an audit error is represented by `AuditFailed`. | Reorder/remove the local `emit` calls or error conversion; this is not proof of an audit service. |
| AX-SP-04 | `StatuspageRateLimiter` keys local state by `(page_id, metric_id)` and records only an `Allow` decision. | Change the map key or insert/deny control flow in `src/rate_limit.rs`; no quota observation follows. |
| AX-SP-05 | `RetryPolicy::decide` classifies 2xx as success, 401/403 as auth give-up, and 429/5xx or absent status as retryable only while `attempt < max_retries`. | Change the match arms or retry bound in `src/retry.rs`; no remote response behavior follows. |

<a id="r06"></a>
## R06 — External-adapter declaration invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| The native client source constructs a URL from `base_url`, page ID, and metric ID; its request-building text names a POST, an `Authorization` value derived from the held key, and `DsrCompletionReport::to_metric_body`. The wasm client contains an analogous source-level request construction behind wasm32. | Changing these local expressions in `src/{http,wasm32_backend}.rs` falsifies the declaration. Endpoint availability, authorization, request transmission, response semantics, and delivery are UNKNOWN. |

<a id="r07"></a>
## R07 — Fake and local-observer invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `InMemoryStatuspageBackend` holds a mutex-protected `Vec<RecordedPublish>` and `InMemoryStatuspageAuditSink` holds a mutex-protected `Vec<StatuspageAuditEvent>`; both expose snapshot/count helpers. | Removing a named type, vector, mutex field, or helper in `src/{memory,audit}.rs` falsifies the fake inventory. It does not demonstrate an external adapter, persistent audit record, concurrency property, or test result. |

<a id="r08"></a>
## R08 — Evidence limit and five unknowns

Evidence mode is SOURCE: the package manifest and named local Rust source at
the pinned commit. DOCUMENTARY evidence may establish only structural-document
checks. The verified canonical OKF route is [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md), used for routing only and neither copied nor revalidated here.

1. Selected target, feature resolution, compilation, and linking are UNKNOWN.
2. Caller graph, compatibility, scheduler invocation, and reachability are UNKNOWN.
3. Credentials, authorization, request transmission, remote API behavior, and response receipt are UNKNOWN.
4. Audit persistence, metric publication, quota observation, retries, and timing in an environment are UNKNOWN.
5. Deployment, configuration/bindings, production data, monitoring, and independent-review outcome are UNKNOWN.

Claiming any unknown without separately selected evidence falsifies this record's
boundary.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-statuspage-real/SKILL.md#s01)
