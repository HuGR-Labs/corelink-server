---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-resilience
manifest: tests/e2e-resilience/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-e2e-resilience-source-1177dad2
---

# e2e-resilience — blast radius

Static relationship inventory at the pinned source. Dependency, data-flow, and impact directions are recorded separately; none implies test execution or production reachability.

[Scope](#b01) · [Method](#b02) · [Relations](#b03) ·
[Propagation](#b04) · [Change map](#b05) · [Unknowns](#b06).

<a id="b01"></a>
## B01 — Scope

This map covers the package manifest, library source, one explicit `scenarios` target, direct package declarations, workspace inverse declarations, visible CI selectors, and literal package-name references outside Cargo. It does not claim a resolved graph, runtime callers, successful CI, or production equivalence.

**Build selection inspected:** package `e2e-resilience`; library and explicit test `scenarios`; no package features declared (`Cargo.toml:13-26`). The package is a workspace member (`Cargo.toml:295`).

<a id="b02"></a>
## B02 — Method and populations

| Population | Evidence and result | Limit |
|---|---|---|
| Manifest and targets | `tests/e2e-resilience/Cargo.toml:1-26`; root `Cargo.toml:295` | Declarations only |
| Outgoing direct declarations | `corelink-ratelimit` and `corelink-rate-headers` are path dependencies; `uuid` and `thiserror` are workspace dependencies; manifest `:16-20` | Declared normal dependencies; resolved activation remains UNKNOWN; Cargo.lock records `e2e-resilience` 0.1.0 with the two first-party edges plus `uuid`/`thiserror` (`Cargo.lock:3082-3090`) but does not prove target selection |
| Reverse package references | No manifest declares `e2e-resilience`; billing and container each declare `corelink-ratelimit`; billing declares `corelink-rate-headers` | Three declared inverse edges, split atomically in REL-008/009/010; no resolved inverse graph |
| Source calls | Six test functions import and call upstream in-memory APIs; `tests/scenarios.rs:28-45,145-475` | Source calls do not prove selected or executed target |
| CI and outside Cargo | `cas_foundation.yml:157-169` (fmt/clippy/test/doc), `rustfmt.yml:70,96-105` (fmt), `workspace-lint.yml:58-73` (clippy); ownership checker is a local campaign command; literal name hits also in SBOM, LOC report, and two audit documents | No workflow/result or package-specific selector was verified |

Search was a literal source census at `1177dad2ca2a9f21c29b5a118aa7944b77147798`. The audit-document mentions (`specs/_audits/sealed/2026-05-15-ratelimit-ux-audit.md:197`; `2026-05-16-chaos-campaign-harness.md:14,128`) are documentary references, not runtime or Cargo consumers. SBOM and LOC entries are inventories, not callers. Stable identities use repository `1232040291`, qualified package endpoints, and the keys listed in B03; they are source-census identities, not resolved-graph proof.

**Census commands/evidence:** from the pinned tree, enumerate manifests with `rg --files -g 'Cargo.toml'`; inspect declarations, lock entries, and inverse consumers with `rg -n 'corelink-ratelimit|corelink-rate-headers|e2e-resilience' --glob 'Cargo.toml' --glob 'Cargo.lock'`; inspect source call sites with `rg -n 'corelink_ratelimit|corelink_rate_headers' tests crates --glob '*.rs'`; inspect workflow selectors with `rg -n 'cargo (fmt|clippy|test|doc)|check_docs' .github/workflows`; and inspect outside-Cargo literals with `rg -n 'e2e-resilience' --glob '!docs/ownership/**'`. These commands were source discovery only; no Cargo resolution or execution was performed.

<a id="b03"></a>
## B03 — Atomic direct and selection relations

**Index:** [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) ·
[REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) ·
[REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) ·
[REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) ·
[REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015).

<a id="rel-001"></a>
### REL-001 — Token-bucket manifest dependency

**Key:** `repo:1232040291:boundary:e2e-resilience-manifest->corelink-ratelimit-normal`. **Dependency arrow:** `tests/e2e-resilience/Cargo.toml[dependencies].corelink-ratelimit` → `crates/corelink-ratelimit[Cargo.toml package]`. **Class:** normal path dependency; source `Cargo.toml:16-17`; root workspace alias `Cargo.toml:578`; pinned `Cargo.lock:2166-2174` records `corelink-ratelimit` 0.1.0. **Activation:** package target resolution, not proven here. **Impact:** resolution or package/API change can block compilation. **Failure boundary:** declaration/lock resolution only; lock presence does not prove selected target/runtime effect. **Contract owner:** `corelink-ratelimit`; resolved activation/features UNKNOWN.

**Validation:** inspect manifest/lockfile and affected target; do not infer resolution. **Coordination:** `corelink-ratelimit` owner plus this package owner. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — Token-bucket scenario call surface

**Shared fingerprint:** `repo:1232040291:boundary:e2e-resilience-scenarios->corelink-ratelimit-api-v1`. **Call arrow:** `tests/e2e-resilience/tests/scenarios.rs[rate-limit tests]` → `corelink_ratelimit::{BucketKey,InMemoryRateLimitAuditSink,InMemoryRateLimitMetrics,InMemoryTokenBucketRateLimiter,RateLimitConfig,RateLimitDecision,RateLimitEventType,RateLimiter,TokenBucketState}`. **Surface:** `canonical`, `new`, `seed_bucket`, `try_acquire`, `snapshot_of`; `:37-40,60-72,145-216`. **Data:** tenant/key/tokens/time → decision/audit. **Impact:** signature/decision changes can break compile/assertions. **Boundary:** source call only; target selection/execution/runtime unknown. **Contract owner:** `corelink-ratelimit`; peer relation is [corelink-ratelimit REL-007](../corelink-ratelimit/BLAST_RADIUS.md#rel-007), with the same fingerprint and reconciled source facts.

**Validation:** compare the exact imports/calls and scenario assertions; execution remains UNKNOWN. **Coordination:** `corelink-ratelimit`, billing, and container owners for provider changes. [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — UUID constructor dependency

**Key:** `repo:1232040291:boundary:e2e-resilience->uuid-normal`. **Dependency arrow:** package → workspace `uuid` (`Cargo.toml:19`). **Class:** normal workspace dependency; `Uuid::from_u128` is test-only (`tests/scenarios.rs:26,75-79`). **Impact:** constructor/type incompatibility blocks target compilation. **Boundary:** no external identity lookup or tenant service. **Contract owner:** external crate maintainer; resolved version/features UNKNOWN.

**Validation:** inspect the workspace declaration and `Uuid::from_u128` call; no resolution or execution proof. **Coordination:** workspace dependency owner and package test owner. [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — Error-derive dependency

**Key:** `repo:1232040291:boundary:e2e-resilience->thiserror-normal`. **Dependency arrow:** package → workspace `thiserror` (`Cargo.toml:20`). **Class:** normal workspace dependency; derive supplies `Error` for `ResilienceError` (`src/lib.rs:55,342-355`). **Impact:** macro/type incompatibility prevents library compilation. **Boundary:** compile-time only; no runtime service effect. **Contract owner:** external crate maintainer; resolved version/features UNKNOWN.

**Validation:** inspect derive and manifest declaration; compilation remains unexecuted. **Coordination:** workspace dependency owner and package library owner. [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — Library API to scenario target

**Key:** `repo:1232040291:boundary:e2e-resilience-scenarios->e2e-resilience-lib`. **Dependency arrow:** `scenarios` test target → package library. **Surface:** `LogicalClock`, `BackPressureQueue`, `HttpStatus`, `ResilienceResponse`, `ResilienceError`; `tests/scenarios.rs:43-45,348-475`. **Data:** local inputs → status/depth/audit/error assertions. **Activation:** explicit test target `Cargo.toml:24-26`. **Impact:** signature/result changes break compile/assertions; selection/result unknown. **Owner:** this package library.

**Validation:** inspect target imports and assertions, then run only an authorized package test. **Coordination:** e2e-resilience owner and cold reviewer. [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — Workspace convergence test job

**Key:** `repo:1232040291:boundary:workspace-test->e2e-resilience-all-targets`. **Type/direction:** test selection, `.github` workspace job → workspace members including this declared member. **Data:** not applicable; the job receives build/test results. **Impact:** package source may affect the result if the job runs; workflow changes may alter selection.

**Activation:** source exposes `workflow_dispatch`; the weekly schedule is commented out (`cas_foundation.yml:88-98`); the job declares `cargo test --workspace --all-targets` (`:134-169`). **Failure/limit:** source proves no dispatch or result. **Owner:** workflow execution owner not assigned by the package source.

**Validation:** inspect the exact workflow trigger/job and retain an authorized run URL/result; YAML alone is insufficient. **Coordination:** workflow owner and package owner. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — Workspace clippy job

**Key:** `repo:1232040291:boundary:workspace-lint->e2e-resilience-all-targets`. **Type/direction:** build selection, `.github/workflows/workspace-lint.yml` → workspace members. **Data:** not applicable; lint diagnostics return to the job. **Impact:** package source may affect diagnostics if run; workflow path/trigger changes affect selection. **Activation:** workflow dispatch or configured push paths; command is `cargo clippy --workspace --all-targets --message-format=short -- -D warnings` (`workspace-lint.yml:27-43,57-73`). **Failure/limit:** source declares activation/command, not a run or result. **Owner:** workflow execution owner not assigned here.

**Validation:** inspect the exact clippy selector and retain an authorized run result; source declaration is not execution evidence. **Coordination:** workflow owner and package owner. [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — Billing reverse consumer of rate-limit

**Key:** `repo:1232040291:boundary:corelink-billing->corelink-ratelimit-reverse-normal`. **Reverse consumer arrow:** `crates/corelink-billing/Cargo.toml[dependencies].corelink-ratelimit` → `crates/corelink-ratelimit[Cargo.toml package]`; source call sites include billing abuse scoring. **Activation:** billing package selection, not this harness. **Impact:** provider API changes may affect billing compilation and abuse-source contracts independently. **Boundary:** manifest inverse/source census only; resolved graph/runtime unknown. **Coordination:** provider and billing owners.

**Validation:** re-run inverse manifest search and inspect `crates/corelink-billing/src/abuse/scorer.rs:58,330`; do not combine this relation with the container route. [Relation index](#b03)


<a id="rel-009"></a>
### REL-009 — Container reverse consumer of rate-limit

**Key:** `repo:1232040291:boundary:corelink-container->corelink-ratelimit-reverse-normal`. **Reverse consumer arrow:** `crates/corelink-container/Cargo.toml[dependencies].corelink-ratelimit` → `crates/corelink-ratelimit[Cargo.toml package]`; source call sites include container rate-limit routes. **Activation:** container package selection, not this harness. **Impact:** provider API changes may affect container compilation and route-layer source contracts independently. **Boundary:** manifest inverse/source census only; resolved graph/runtime unknown. **Coordination:** provider and container owners.

**Validation:** re-run inverse manifest search and inspect `crates/corelink-container/src/routes/ratelimit_layer.rs:119-122,389-399`; do not combine this relation with the billing scorer. [Relation index](#b03)


<a id="rel-010"></a>
### REL-010 — Rate-header reverse consumer

**Key:** `repo:1232040291:boundary:corelink-rate-headers-reverse-consumer`. **Reverse consumer arrow:** `crates/corelink-billing/Cargo.toml[dependencies].corelink-rate-headers` → `crates/corelink-rate-headers[Cargo.toml package]`; billing source re-exports the rate-header surface. **Activation:** billing package selection, not this harness. **Impact:** circuit/header changes may affect billing independently of this harness. **Boundary:** manifest/source literal census only; resolved graph/runtime unknown. **Coordination:** `corelink-rate-headers` and billing owners.

**Validation:** inspect `crates/corelink-billing/src/rate_headers.rs:1-7` and the billing manifest; no resolved/runtime claim. **Coordination:** `corelink-rate-headers` and billing owners. [Relation index](#b03)


<a id="rel-011"></a>
### REL-011 — Circuit/header manifest dependency

**Key:** `repo:1232040291:boundary:e2e-resilience-manifest->corelink-rate-headers-normal`. **Dependency arrow:** `tests/e2e-resilience/Cargo.toml[dependencies].corelink-rate-headers` → `crates/corelink-rate-headers[Cargo.toml package]`. **Class:** normal path dependency; source `Cargo.toml:16-18`; root workspace alias `Cargo.toml:579`; pinned `Cargo.lock:2157-2164` records `corelink-rate-headers` 0.1.0. **Activation:** package target resolution, not proven here. **Impact:** resolution or API change can block compilation. **Boundary:** declaration/lock resolution only; lock presence does not prove selected target/runtime effect. **Contract owner:** `corelink-rate-headers`; resolved activation/features UNKNOWN.

**Validation:** inspect manifest/lockfile and target selection; do not infer resolution. **Coordination:** `corelink-rate-headers` owner plus this package owner. [Relation index](#b03)


<a id="rel-012"></a>
### REL-012 — Circuit/header scenario call surface

**Shared fingerprint:** `repo:1232040291:boundary:e2e-resilience-scenarios->corelink-rate-headers-api-v1`. **Arrow:** `tests/e2e-resilience/tests/scenarios.rs` → `corelink_rate_headers::{CircuitAuditRecord,CircuitAuditSink,CircuitAuditSinkError,CircuitDecision,CircuitEventType,CircuitState,CircuitThresholds,GlobalCircuitBreaker,HealthObservation,InMemoryCircuitAuditSink,InMemoryCircuitMetrics,InMemoryGlobalCircuitBreaker,ObservationStatus}` plus `HALFOPEN_DWELL_MS`, `ROLLING_WINDOW_MS`, `GLOBAL_CIRCUIT_RETRY_AFTER_SECS`. **Surface:** constructors, `record_observation`, `snapshot`, `check`, `record_probe_outcome`, `snapshot_of`; `:28-35,81-139,222-342,388-471`. **Data:** thresholds/observations/time → state/decision/audit. **Boundary:** source only; execution/runtime unknown. **Contract owner:** `corelink-rate-headers`; peer relation is [corelink-rate-headers REL-007](../corelink-rate-headers/BLAST_RADIUS.md#rel-007), with the same fingerprint and reconciled source facts.

**Validation:** compare upstream documented `Reject`=429 with local `circuit_reject_to_response()`=503; intentional mapping versus defect is unresolved. The private sink returns `CircuitAuditSinkError::Store("scenario 6: failing sink")`. **Coordination:** rate-headers, billing, and package owners; unresolved status blocks production-equivalence approval. [Relation index](#b03)


<a id="rel-013"></a>
### REL-013 — Workspace rustfmt job

**Key:** `repo:1232040291:boundary:workspace-rustfmt->e2e-resilience-source`. **Build arrow:** `.github/workflows/rustfmt.yml` → workspace source, including this member. **Surface:** `cargo fmt --all --check`, workflow lines `70,96-105`; activation is workflow trigger, not package execution. **Impact:** formatting failure can block the aggregate check; no source/runtime effect. **Boundary:** source declaration only; run/result UNKNOWN. **Validation:** inspect trigger and retain authorized run result. **Coordination:** rustfmt workflow owner and package owner. [Relation index](#b03)


<a id="rel-014"></a>
### REL-014 — Ownership-document checker

**Key:** `repo:1232040291:boundary:ownership-checker->e2e-resilience-docs`. **Validation arrow:** campaign checker → the four package ownership files. **Surface:** v1.3 `tools/check_docs.py` with `skill/reference/blast_radius/maintenance` kinds; activation is an author/reviewer command, not a CI or runtime path. **Impact:** structural failures block documentary handoff only. **Boundary:** no Cargo/runtime effect. **Validation:** run the four exact commands in MAINTENANCE PROC-001 and preserve output. **Coordination:** campaign lead and independent cold reviewer. [Relation index](#b03)


<a id="rel-015"></a>
### REL-015 — Workspace cargo-doc job

**Key:** `repo:1232040291:boundary:workspace-cargo-doc->e2e-resilience`. **Build arrow:** `.github/workflows/cas_foundation.yml` → workspace documentation targets including this declared member. **Surface:** `cargo doc --no-deps --workspace` with `RUSTDOCFLAGS=-D warnings` (`cas_foundation.yml:166-169`). **Activation:** workflow dispatch only at this source; no dispatch, target selection, or result was observed. **Impact:** package doc/link changes may alter the aggregate rustdoc result; this command does not prove test or runtime reachability. **Coordination:** workflow owner and package owner.

**Validation:** inspect the exact workflow selector and retain an authorized run URL/result; YAML and workspace membership alone are insufficient. [Relation index](#b03)


<a id="b04"></a>
## B04 — Propagation and containment

| Destination | Witness | Causal effect | Containment / validation |
|---|---|---|---|
| `corelink-billing` abuse scorer | REL-008; billing manifest and `src/abuse/scorer.rs:58,330` | `corelink-ratelimit` API/type changes can break billing compilation or alter its source contract; no runtime effect is established | Coordinate with rate-limit and billing owners; inspect this exact consumer and resolved graph before change |
| `corelink-container` rate-limit routes | REL-009; container manifest and `src/routes/ratelimit_layer.rs:119-122,389-399` | The same provider API change can independently break container compilation or route-layer source contracts | Coordinate with rate-limit and container owners; validate this exact consumer separately |
| `corelink-billing` rate-header façade | REL-010; billing manifest and `src/rate_headers.rs:1-7` | `corelink-rate-headers` changes can break the billing re-export/source contract; no runtime reachability is established | Coordinate with rate-headers and billing owners; inspect the façade and resolved graph |
| This package's rate-limit target | REL-002 | Imported decision/audit APIs can break target compilation or assertions; source calls do not prove target selection/execution | Reconcile imports, lockfile, target selection, and authorized test evidence |
| This package's circuit target | REL-012 | Circuit API/state changes can break assertions; local 503 mapping versus upstream `Reject` 429 remains an unresolved contract question | Coordinate with rate-headers; preserve the divergence until an owner resolves it |
| Workspace test/clippy jobs | REL-006/007 | A changed package may affect aggregate output only when the declared workspace command actually runs | Check exact selector, dispatch, and result through authorized CI evidence |
| Workspace rustfmt/doc jobs | REL-013/015 | Formatting or rustdoc/link changes may affect aggregate checks; neither command proves runtime behavior | Check exact selector, dispatch, and result through authorized CI evidence |
| Cargo manifest/lock resolution | REL-001/003/004/011 | Declaration or pinned lock metadata may change compilation selection; lock presence is not proof of activation | Re-read manifests and lock entries; resolve selected target/features only when authorized |
| Production rate-limit or circuit behavior | No observed package-owned call path | No propagation from a passing scenario to a live request is established | Composition root, adapters, operators, and runtime state remain UNKNOWN |

The harness scenario path ends at local in-memory provider objects and assertions. It does not call a request router, customer storage, a real audit sink, or a provider. The two audit specs describe adjacent intent; they are not causal runtime edges.

<a id="b05"></a>
## B05 — Change to impact to validation

| Change | Affected records | Expected impact | Documentary / validation action |
|---|---|---|---|
| Manifest, targets, dependencies, or lockfile | R01/R06; REL-001,003,004,005,011 | Identity, pinned dependency evidence, compile edges, and possible CI selection change | Re-read manifest/lockfile and execute the read-only procedure in MAINTENANCE PROC-003 |
| Clock, response, or local queue | API-001/002; INV-001/002; REL-005 | Scenario target may stop compiling or assertions may change | Review source and run the package target only when authorized |
| Limiter/circuit contract | REL-002/012; REL-008/009/010; dependent consumers above | Contract change can affect distinct package consumers and this target | Route to defining crate; inspect each exact inverse callsite and consumer contract |
| Workflow test selector | REL-006 | Aggregate test selection/cadence can change | Review trigger and retain authorized run evidence |
| Workflow clippy selector | REL-007 | Aggregate lint selection/result can change | Review trigger and retain authorized run evidence |
| Workflow formatting selector | REL-013 | Aggregate formatting result can change | Review rustfmt workflow and retain authorized run evidence |
| Workflow documentation selector | REL-015 | Aggregate rustdoc/link result can change | Review RUSTDOCFLAGS, workspace selector, and retain authorized run evidence |
| Ownership documentation | REL-014 and all local claims | A stale source anchor invalidates affected claims | Run MAINTENANCE PROC-001; request cold review |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Found | Documented | Excluded with reason | Unknown |
|---|---:|---:|---|---|
| Direct manifest declarations | 4 | REL-001,003,004,011 | None | Resolution/features/target activation |
| Package targets | 2 | 2 | None | Selected/executed targets |
| First-party reverse package references | 3 provider consumers; 0 manifests declare this harness | REL-008/009/010 | None; SBOM and LOC entries are inventory | Complete resolved inverse graph |
| CI/review references | 3 workflow surfaces plus local checker | REL-006/007/013/014/015 | No package-specific runtime selector | Dispatches, job/results, checker integration |
| Literal outside-Cargo package references | Two audit docs, SBOM, LOC report | B02 documented | Docs are not runtime callers; inventories are not callers | Other repositories or generated references |

**Shared-fingerprint audit:** the two public cross-package call surfaces use the stable fingerprints `repo:1232040291:boundary:e2e-resilience-scenarios->corelink-ratelimit-api-v1` and `repo:1232040291:boundary:e2e-resilience-scenarios->corelink-rate-headers-api-v1`. Their peer rows are [ratelimit REL-007](../corelink-ratelimit/BLAST_RADIUS.md#rel-007) and [rate-headers REL-007](../corelink-rate-headers/BLAST_RADIUS.md#rel-007); shared facts are reconciled at SOURCE scope. This does not claim Cargo resolution, compilation, execution, runtime, or peer approval.

Five unknown groups: (1) resolved graph/target/features; (2) CI invocation and result history; (3) production composition and ownership; (4) live audit, metrics, storage, or provider effects; (5) deployment/runtime state and operational escalation. Equal counts do not prove a complete discovery.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
