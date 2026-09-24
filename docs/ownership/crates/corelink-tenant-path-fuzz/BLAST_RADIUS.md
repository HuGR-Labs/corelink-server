---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-tenant-path-fuzz
manifest: crates/tenant-path/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w016-tenant-path-fuzz-source-20260921
---

# corelink-tenant-path-fuzz — blast radius

SOURCE-static relation map. A manifest edge, test harness, or workflow step does not establish a completed fuzz run, production data flow, or tenant isolation.

[Scope](#b01) · [Method](#b02) · [Relations](#b03) · [Propagation](#b04) · [Change impact](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Scope

This document covers package `corelink-tenant-path-fuzz`, manifest `crates/tenant-path/fuzz/Cargo.toml`, and its two binaries. It does not transfer ownership of the parent algorithm, worker port, storage consumers, CI runner, or operational environment. Source pin is `1177dad2ca2a9f21c29b5a118aa7944b77147798`; integration baseline is `8cdc02828132b9b6f03a3b57117b8140325f6762`.

The parent package's typed `derive_prefix` boundary in [B02](../corelink-tenant-path/BLAST_RADIUS.md#b02) shares identity `repo:1232040291:boundary:tenant-path-derive-prefix-001` with this package. Fingerprint: `corelink-tenant-path::derive_prefix(&TenantDerivationKey, Uuid) -> TenantPrefix`; HMAC-SHA256, URL-safe no-pad base64, first 16 characters. Dependency: fuzz → parent. Data: fuzz bytes → typed key/UUID → prefix. Impact: parent contract changes alter the harness result; slicing/assertion changes alter the property. Surfaces: `crates/tenant-path/src/prefix.rs::derive_prefix`; `crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs`; `crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs`. Parent owns API/algorithm; fuzz owns slicing/assertions. Source-only; no durable effect/runtime observation.

<a id="b02"></a>
## B02 — Method and population

The parent BLAST is pinned to `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. The scoped `crates/tenant-path` diff between that pin and this fuzz pin is metadata-only: the Cargo `repository` URL changes from `HuGR-Labs/corelink-server` to `HuGR-dev/corelink-server`; no parent algorithm or fuzz-target source changes. Re-check this source drift before approval; no runtime equivalence is claimed.

Static inspection covered the root and standalone manifests, both target files, parent API source, tracked fuzz files, direct manifest declarations of `corelink-tenant-path`, fuzz target selectors, GitHub workflow configuration, and selected worker/vector references. No Cargo metadata/resolution, fuzz command, CI query, test, or external-system access was performed.

The inventory identifies two declared bins and four direct dependencies. Literal manifest matches found six consumer manifests (five crates and the e2e test package), plus the independent worker-fuzz manifest; the root manifest provides the workspace dependency alias. These are not a complete resolved inverse graph. The outside-Cargo scan found the worker TypeScript port, parity tests/vectors, and workflow selectors. Dynamic and generated relationships remain unknown.

<a id="b03"></a>
## B03 — Atomic direct relations

| ID | Boundary | Dependency direction | Peer / evidence |
|---|---|---|---|
| [REL-001](#rel-001) | Derivation call | fuzz → tenant-path | parent package |
| [REL-002](#rel-002) | Fuzzer callback | fuzz → libfuzzer-sys | external crate |
| [REL-003](#rel-003) | UUID construction | fuzz → uuid | external crate |
| [REL-004](#rel-004) | TDK wrapper | fuzz → zeroize | external crate |
| [REL-005](#rel-005) | PR smoke selector | workflow → fuzz target | `derive_prefix` |
| [REL-006](#rel-006) | Dormant tenant-path lane | workflow → fuzz target | `derive_prefix` |
| [REL-007](#rel-007) | Active nightly matrix | workflow → fuzz target | `derive_prefix` |
| [REL-008](#rel-008) | Expansion workflow matrix | workflow → fuzz target | `derive_prefix_extended` |
| [REL-009](#rel-009) | Aggregate target list | script → fuzz target | `scripts/fuzz-all.sh` |

Each record has separate dependency, data, and impact directions. Workflow entries prove configured selection only. No relation below is runtime-observed.

<a id="rel-001"></a>
### REL-001 — Parent derivation API
**Type:** API/data boundary. **Key:** `repo:1232040291:boundary:tenant-path-derive-prefix-001`.
**Dependency:** fuzz package → `corelink-tenant-path`.
**Data:** raw harness bytes → TDK/UUID → `crates/tenant-path/src/prefix.rs::derive_prefix` → prefix.
**Impact:** parent API/representation changes can alter harness behavior; target slicing or assertions change the explored property. **Activation:** either fuzz target passes its input guard and calls the parent function.

**Contract:** typed key/UUID input returns `TenantPrefix`; target assertions are narrower.
**State/effects:** synthetic bytes, in-memory call, no durable effect.
**Failure/propagation:** API mismatch or assertion failure stops the harness; no service claim.
**Boundary:** target entrypoint to parent API; no storage or authorization.

**Coordination/evidence:** parent [`B02`](../corelink-tenant-path/BLAST_RADIUS.md#b02); canonical contract owner is `corelink-tenant-path`; source/M04 only. No fuzz run or runtime behavior is claimed.
[B03](#b03)

<a id="rel-002"></a>
### REL-002 — libFuzzer callback
**Type:** macro/runtime boundary. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-libfuzzer-001`.
**Dependency:** fuzz package → `libfuzzer-sys`. **Data:** fuzzer byte slice → target closure.
**Impact:** macro/runtime incompatibility can prevent harness build or execution. **Activation:** target binary selection invokes the callback; source wiring is not a run.

**Contract:** callback receives arbitrary bytes; macro supplies the target entrypoint.
**State/effects:** process-local fuzzer input; no package data persistence.
**Failure/propagation:** dependency incompatibility prevents build/run; no production propagation.
**Boundary:** harness callback to external fuzz runtime; resolver state UNKNOWN.

**Coordination/evidence:** maintain with manifest and target source; no resolved version or run evidence.
[B03](#b03)

<a id="rel-003"></a>
### REL-003 — UUID byte construction
**Type:** typed-data conversion. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-uuid-001`.
**Dependency:** fuzz package → `uuid` with feature `v7`.
**Data:** 16 arbitrary input bytes → `Uuid::from_bytes` → parent API.
**Impact:** representation or offsets change the generated cases; feature does not prove v7-only inputs. **Activation:** accepted target input reaches the UUID slice.

**Contract:** raw 16 bytes construct a `Uuid` without version validation.
**State/effects:** UUID value exists only for the derivation call.
**Failure/propagation:** changed construction alters coverage and compatibility assumptions.
**Boundary:** byte slice to parent typed input; no tenant parser or storage path.

**Coordination/evidence:** target source and manifest; UUID version census and execution UNKNOWN.
[B03](#b03)

<a id="rel-004"></a>
### REL-004 — Zeroize wrapper
**Type:** secret-wrapper boundary. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-zeroize-001`.
**Dependency:** fuzz package → `zeroize`. **Data:** TDK array copy → `Zeroizing` → key constructor.
**Impact:** wrapper/API changes can affect construction; erasure of all copies is unproven. **Activation:** accepted input builds TDK A or TDK B from synthetic bytes.

**Contract:** a 32-byte wrapper is accepted by `TenantDerivationKey::from_bytes`.
**State/effects:** key value is held during local derivation; no real secret is allowed.
**Failure/propagation:** wrapper/API mismatch blocks target construction; no key-custody claim.
**Boundary:** synthetic TDK bytes to parent constructor; memory behavior unobserved.

**Coordination/evidence:** target and parent constructor source; no memory/runtime evidence.
[B03](#b03)

<a id="rel-005"></a>
### REL-005 — Pull-request smoke
**Type:** workflow configuration. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-pr-smoke-001`.
**Dependency:** workflow → `derive_prefix` target. **Data:** CI-runner inputs → harness.
**Impact:** selected run could provide bounded harness evidence; it cannot certify other targets or production. **Activation:** pull-request path filter selects the job; no run is claimed.

**Contract:** configured `derive_prefix` smoke invocation with a 60-second limit.
**State/effects:** runner process and ignored fuzz artifacts; no durable product effect.
**Failure/propagation:** failed job reports CI failure; absent run leaves status UNKNOWN.
**Boundary:** PR path filter to fuzz job; no GitHub result queried.

**Coordination/evidence:** [`tenant-path.yml`](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/tenant-path.yml); source only.
[B03](#b03)

<a id="rel-006"></a>
### REL-006 — Dormant tenant-path lane
**Type:** workflow configuration. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-crate-nightly-001`.
**Dependency:** workflow → `derive_prefix` target. **Data:** scheduled-run inputs → harness.
**Impact:** a completed run would add bounded evidence, not universal coverage. **Activation:** dormant job configuration is present; no active schedule or run is inferred.

**Contract:** dormant job names `derive_prefix` with a 3600-second limit.
**State/effects:** configured runner process; no product data effect.
**Failure/propagation:** no active schedule means no inferred run; future failure is job-scoped.
**Boundary:** workflow trigger/job to target; old cron is commented out.

**Coordination/evidence:** [`tenant-path.yml`](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/tenant-path.yml); no run queried.
[B03](#b03)

<a id="rel-007"></a>
### REL-007 — Active nightly base target
**Type:** scheduled workflow configuration. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-nightly-matrix-001`.
**Dependency:** scheduled workflow matrix → `derive_prefix`. **Data:** matrix runner → base harness.
**Impact:** a completed run is bounded evidence; YAML does not prove a result. **Activation:** nightly trigger selects the matrix entry; no CI result was queried.

**Contract:** active nightly matrix selects `tenant-path::derive_prefix` for 3600 seconds.
**State/effects:** scheduled runner and fuzzer artifacts; no product data effect.
**Failure/propagation:** job failure affects nightly status; no result means runtime UNKNOWN.
**Boundary:** nightly trigger/matrix to target; no CI query performed.

**Coordination/evidence:** [nightly source](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/nightly.yml); source only.
[B03](#b03)

<a id="rel-008"></a>
### REL-008 — Expansion workflow matrix
**Type:** dispatch workflow configuration. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-extended-dispatch-001`.
**Dependency:** workflow matrix → `derive_prefix_extended`. **Data:** selected matrix runner → harness.
**Impact:** dispatch can provide bounded evidence; parked schedule means no scheduled result is inferred. **Activation:** manual dispatch/matrix selection is configured; no run was queried.

**Contract:** matrix selects `derive_prefix_extended` with a declared 1800-second duration.
**State/effects:** isolated runner/fuzzer artifacts; no product data effect.
**Failure/propagation:** job failure is matrix-scoped; no run leaves execution UNKNOWN.
**Boundary:** workflow dispatch/matrix to extended target; no CI result queried.

**Coordination/evidence:** [`fuzz-nightly.yml`](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/fuzz-nightly.yml); source only.
[B03](#b03)

<a id="rel-009"></a>
### REL-009 — Aggregate target selector
**Type:** shell selector configuration. **Key:** `repo:1232040291:boundary:tenant-path-fuzz-all-script-001`.
**Dependency:** `scripts/fuzz-all.sh` selector → both target names. **Data:** selector entries → script loop.
**Impact:** removing a name changes selection if invoked; no caller or run is established. **Activation:** script invocation is required; source listing alone does not activate either target.

**Contract:** target list contains both package target names and a duration variable.
**State/effects:** shell loop selects processes; artifacts are external to Git.
**Failure/propagation:** nonzero target exits accumulate and fail the script.
**Boundary:** static selector to shell invocation; invocation and environment UNKNOWN.

**Coordination/evidence:** [`fuzz-all.sh`](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/scripts/fuzz-all.sh); list source only.
[B03](#b03)

<a id="b04"></a>
## B04 — Propagation beyond the package

The fuzzer's direct implementation peer is `corelink-tenant-path`. Static manifest census found these additional parent consumers:

| Declared consumer | Manifest | Evidence limit |
|---|---|---|
| `corelink-container` | `crates/corelink-container/Cargo.toml` | Declared edge; call sites not resolved here |
| `corelink-r2-multipart` | `crates/corelink-r2-multipart/Cargo.toml` | Declared edge; call sites not resolved here |
| `corelink-auth` | `crates/corelink-auth/Cargo.toml` | Also has a source re-export; no downstream reachability claim |
| `corelink-reapi` | `crates/corelink-reapi/Cargo.toml` | Declared edge; call sites not resolved here |
| `corelink-worker` | `crates/corelink-worker/Cargo.toml` | Declared edge; call sites not resolved here |
| e2e tenant-isolation tests | `tests/e2e-tenant-isolation/Cargo.toml` | Test package declaration, not a production caller |
| `corelink-worker-fuzz` | `crates/corelink-worker/fuzz/Cargo.toml` | Separate fuzz package declaration |

The root `Cargo.toml` provides a workspace dependency alias; this package has its own direct path edge. The table is static and not Cargo-resolved or feature-complete.

Outside Cargo, `worker/src/lib/edge_find_missing.ts` and `edge_public_read.ts` contain TypeScript prefix derivation, with worker tests and `worker/tests/vectors/tenant_prefix_vectors.json`; `crates/tenant-path/tests/edge_parity_vectors.rs` reads those vectors. These are shared-algorithm compatibility surfaces, not calls made by either fuzz target. The fuzzer does not parse the vector JSON, invoke TypeScript, or validate storage consumers.

<a id="b05"></a>
## B05 — Change, impact, and validation

| Change | Impact path | Review evidence |
|---|---|---|
| Fuzz input offsets or minimum length | Corpus interpretation → target coverage | M04 exact byte-slice matrix; M05 reproducer compatibility |
| Assertion/oracle change | Harness finding sensitivity → claimed property | Compare executable predicate with comments and parent API |
| Parent derivation or output change | Fuzzer contract → Rust consumers and separate worker parity implementation | Coordinate parent API, direct declared consumers, vectors, and recovery owner |
| Direct dependency or target declaration | Standalone fuzz build/selection | Manifest diff and resolved selection from an authorized later procedure |
| Workflow trigger, duration, or runner change | Configured selection and resource profile | Workflow source plus actual run evidence; do not infer a green run |

<a id="b06"></a>
## B06 — Coverage and unknowns

The manifest and source establish package identity, two targets, four direct dependencies, and their visible calls. Workflow files establish configured selectors. Unknowns include a complete resolver graph, every transitive or generated consumer, actual corpus state, toolchain/dependency resolution for a run, CI outcomes, minimized findings, worker parity execution, key provenance, production storage effects, and runtime reachability.

No named owner or operator is verified. The `derive_prefix` peer boundary is reconciled by the shared identity above; all other graph, corpus, execution, deployment, and runtime questions remain unknown. Treat all listed facts as SOURCE evidence, not execution or deployment evidence.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-corelink-tenant-path-fuzz/SKILL.md#s01)
