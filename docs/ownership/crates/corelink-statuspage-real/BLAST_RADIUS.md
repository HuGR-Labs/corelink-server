---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-statuspage-real
manifest: crates/corelink-statuspage-real/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w010-statuspage-real-source-static-20260920
---

# corelink-statuspage-real — blast radius

SOURCE-only atomic relation map. Every arrow below is a declared source
relation, not proof of Statuspage API operation, network activity, deployment,
or runtime behavior.

[Root/native](#b01) · [Root/wasm](#b02) · [Trait/native](#b03) · [Trait/fake](#b04) · [Bridge](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Crate-root → native adapter relation

**Arrow:** `src/lib.rs` `#[cfg(not(target_arch = "wasm32"))]` → `http::StatuspageHttpClient` re-export. **Mode:** SOURCE. **Evidence:** `Cargo.toml` native target dependency block; `src/lib.rs`. **Boundary:** this does not select a native target, link `reqwest`, or show an external request.

<a id="b02"></a>
## B02 — Crate-root → wasm adapter relation

**Arrow:** `src/lib.rs` `#[cfg(target_arch = "wasm32")]` → `wasm32_backend::{StatuspageWasm32Client, Wasm32BackendError}` re-export. **Mode:** SOURCE. **Evidence:** wasm target dependency block in `Cargo.toml`; `src/lib.rs`. **Boundary:** this does not select wasm32, create a Worker binding, or execute fetch.

<a id="b03"></a>
## B03 — Backend trait → native implementation relation

**Arrow:** `StatuspageBackend` → `impl StatuspageBackend for StatuspageHttpClient`. **Mode:** SOURCE. **Evidence:** `src/{backend,http}.rs`. **Boundary:** an implementation declaration is not evidence of construction, invocation, authentication, retry, or remote receipt.

<a id="b04"></a>
## B04 — Backend trait → in-memory fake relation

**Arrow:** `StatuspageBackend` → `impl StatuspageBackend for InMemoryStatuspageBackend`, with recorded local values and an injected audit trait. **Mode:** SOURCE. **Evidence:** `src/{backend,memory,audit}.rs`. **Boundary:** a local fake is not an external adapter or a persistent audit store.

<a id="b05"></a>
## B05 — Typed worker-stats → bridge-function contract

**Arrow:** `corelink_privacy_erasure_worker::DsrCompletionStats` →
`bridge_to_report(&DsrCompletionStats) -> Result<DsrCompletionReport,
DsrCompletionReportError>`. **Activation:** a caller supplies one typed
`DsrCompletionStats` value to that public function. **Failure:** the function
returns `DsrCompletionReportError` when its local report construction rejects
the forwarded window or p95 fields. **Mode:** SOURCE. **Evidence:** path
dependency in `Cargo.toml`; `src/dsr_bridge.rs`; `src/report.rs`.
**Boundary:** this single function contract does not prove aggregate generation,
caller invocation, scheduling, publication, or consumer compatibility.

<a id="b06"></a>
## B06 — Unresolved relation boundary

**Arrow:** package declarations → five unresolved domains: target/build selection; callers/schedulers; credentials/network/API; audit/metric/quota effects; deployment/runtime observation. **Mode:** UNKNOWN. **Evidence:** [R08](REFERENCE.md#r08). **Boundary:** no relation in B01–B05 may be expanded into one of these claims without separate evidence.

Known reverse Cargo consumers:

| Relation | Consumer → source relation and activation | Evidence | Unknown / limit |
|---|---|---|---|
| RC-001 | `corelink-ops` → `statuspage` publicly re-exports this crate when that facade is compiled. | `crates/corelink-ops/Cargo.toml`; `src/statuspage.rs` | No caller or external request follows from re-export. |
| RC-002 | `corelink-adapters-cloud` → `statuspage` publicly re-exports this crate when that facade is compiled. | `crates/corelink-adapters-cloud/Cargo.toml`; `src/statuspage.rs` | No target selection, client construction, or network effect is established. |
| RC-003 | `corelink-dsr-statuspage-scheduler` → native source imports the `corelink-ops::statuspage` facade; wasm32 source imports this crate directly. | `crates/corelink-dsr-statuspage-scheduler/Cargo.toml`; `src/scheduler.rs` | Requires matching target path and a scheduler call; no run or publication is proven. |
| RC-004 | `corelink-clerk-cf` → wasm32-target dependency; `dsr_statuspage_cron` constructs this crate's bridge/client on the cron path. | `crates/corelink-clerk-cf/Cargo.toml`; `src/dsr_statuspage_cron.rs` | Requires wasm32 target and cron-path invocation; credentials, fetch, deployment, and receipt are unproven. |

These source and manifest edges are not an exhaustive compiled graph; resolved
targets/features and all runtime/provider behavior remain unknown.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-statuspage-real/SKILL.md#s01)
