---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-worker-fuzz
manifest: crates/corelink-worker/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: worker-fuzz-source-20260921
---

# corelink-worker-fuzz — blast radius

The package owns two fuzz harness binaries. They call `corelink-worker` storage code using a fresh `InMemoryR2` fake per input; the names and CI commands do not prove a Cloudflare R2 path, credential use, or successful fuzz execution. The map is a static source review at the pinned commit; Cargo resolution, runtime tracing, and CI results were not observed.

[Scope](#b01) · [Method](#b02) · [Direct relations](#b03) ·
[Propagation](#b04) · [Change impact](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and priority risks

The key risks are changing the key-shape oracle, weakening the same-tenant or cross-tenant oracle, increasing adversarial-input resource use, changing dependency APIs, or altering CI invocation and runner behavior. Neither fuzz target constructs a production binding. `r2_put_get_roundtrip` allocates its body before checking the writer's 5 MiB limit.

**Source pin:** `1177dad2ca2a9f21c29b5a118aa7944b77147798`; local Git inspection only. No path diff against the integration base was found for the root manifest, package, adapter/key sources, or workflows.

**Configured builds:** independent workspace and two host bins; PR smoke active, per-crate nightly dormant, root matrix active at 3,600 seconds. No feature resolution or build was inspected. **Runtime:** unknown. **Approval:** pending.

<a id="b02"></a>
## B02 — Census and method

| Population | Sources / selection | Found and documented | Limit |
|---|---|---|---|
| Package identity and targets | Root `Cargo.toml`, fuzz manifest, tracked package tree at pin | 1 package, 2 explicit bins, 1 own workspace, 1 local lockfile | No Cargo metadata or resolved feature graph |
| Direct dependencies | Fuzz manifest `[dependencies]` | 8 declarations: 3 path crates and 5 registry crates | Inverse dependents and resolution not queried |
| Harness sources | Both files under `fuzz_targets/` | 2 full target files read; both use `InMemoryR2` | No execution, coverage, crash corpus, or runtime reachability claim |
| Out-of-Cargo literal census | Pinned Git tree; needles `corelink-worker-fuzz`, `crates/corelink-worker/fuzz`, `r2_path`, `r2_put_get_roundtrip` | 14 matching tracked files classified below | Generated/dynamic links and runtime not discoverable by literal search |

**Operational workflow matches:** `.github/workflows/corelink-worker.yml` has a pull-request path and `fuzz-smoke` job running both bins for 60 seconds; its `fuzz-nightly` job is declared with 3,600-second commands but the `on.schedule` block is commented. `.github/workflows/nightly.yml` has the effective schedule and includes both bins in its active 3,600-second matrix. These are configured jobs, not execution records.

**Other matches:** root `Cargo.toml` confirms workspace exclusion. The three sealed work items, two Cargo metadata audits, one proptest audit, and one pentest evidence inventory are design/inventory text, not package invokers.

`crates/corelink-worker/README.md` and `crates/corelink-reapi/tests/prop_cas.rs` discuss parent property tests, not consumers of these binaries. `reports/b326-loc-cap-baseline.txt` is a path inventory and `reports/perf/b152-actions-2026-09-06.json` is historical workflow reporting; neither was accepted as execution proof for this source pin.

No direct SQL, TypeScript, schema, or application-route consumer matched those package/target/path needles. This bounded source search is not proof that generated or dynamically selected code cannot invoke a target.

| Identity in this document | Exact manifest / configuration | Authority status |
|---|---|---|
| `worker-fuzz` | `crates/corelink-worker/fuzz/Cargo.toml` | Package identity from Cargo manifest |
| `worker` | `crates/corelink-worker/Cargo.toml` | Parent API source; named owner unverified |
| `hash` | `crates/corelink-hash/Cargo.toml` | Digest and verified-body source; named owner unverified |
| `tenant-path` | `crates/tenant-path/Cargo.toml` | Tenant derivation source; named owner unverified |
| `worker-ci` / `nightly-ci` | `.github/workflows/corelink-worker.yml` / `.github/workflows/nightly.yml` | Workflow configuration; operator/reviewer unverified |

<a id="b03"></a>
## B03 — Direct relations

`repo:1232040291:boundary:worker-fuzz-<suffix>` is the shared-key form; local `REL-nnn` values are anchors only. Dependency direction is consumer → provider. Data flow and change/failure impact are stated separately on every card.

| ID | Boundary / type | Dependency direction | Data direction | Impact direction |
|---|---|---|---|---|
| [REL-001](#rel-001) | Key-shape path / runtime-call | fuzz → worker | input → writer → local fake key | worker key/write API → `r2_path` oracle |
| [REL-002](#rel-002) | Digest types / dependency | fuzz → hash | body → digest/verified body | hash contract → both targets |
| [REL-003](#rel-003) | Tenant derivation / dependency | fuzz → tenant-path | TDK + UUID → prefix via worker context | derivation contract → both targets |
| [REL-004](#rel-004) | Fuzz engine / dependency | fuzz → libfuzzer-sys | engine → target input; finding → runner | macro/runner change → target operation |
| [REL-005](#rel-005) | Byte buffers / dependency | fuzz → bytes | body slice → owned `Bytes` | bytes API → both targets |
| [REL-006](#rel-006) | UUID parser / dependency | fuzz → uuid | 16 bytes → tenant UUID | UUID API → both targets |
| [REL-007](#rel-007) | Secret zeroization wrapper / dependency | fuzz → zeroize | TDK array → `Zeroizing` → derivation | wrapper/API change → both targets |
| [REL-008](#rel-008) | Local executor / dependency | fuzz → futures-executor | future → `block_on` result | executor change → `r2_path` and roundtrip |
| [REL-009](#rel-009) | Workspace exclusion / build | root workspace excludes fuzz; fuzz declares own workspace | not applicable | root/workspace resolver → standalone target selection |
| [REL-010](#rel-010) | Worker workflow PR smoke / test | workflow → fuzz bins | generated input → target; failure → job | PR job failure → workflow result |
| [REL-011](#rel-011) | Worker workflow dormant nightly declaration / test | workflow → fuzz bins | generated input → target only if schedule is restored; failure → job | no active per-crate trigger; declaration → future runner load/artifacts |
| [REL-012](#rel-012) | Root nightly active matrix / test | workflow → fuzz bins | generated input → target; failure → job | active schedule/dispatch matrix → runner load/artifacts |
| [REL-013](#rel-013) | Roundtrip path / runtime-call | fuzz → worker | body → writer/fake → reader → target | worker read/write API → roundtrip oracle |

<a id="rel-001"></a>
### REL-001 — `r2_path` key construction path
**Identity:** `repo:1232040291:boundary:worker-fuzz-worker-key-001`.
**Dependency / data / impact:** fuzz→worker; input→`R2Writer::put`→fake key snapshot; worker write/key change→`r2_path` oracle.
**Surface / activation:** `Region`, `TenantCtx`, `R2Writer`, `InMemoryR2::keys_snapshot`; `r2_path` only.
**Contract:** assertions validate the first key if present; PUT error or empty key returns early.

**State / effects:** fresh in-memory `HashMap` for this input; no remote call.
**Failure / containment:** key assertion failure is a fuzz finding; fake disposal contains state.
**Validation:** inspect `r2_path` and worker adapter/key source; no fuzz run.
**Coordination / evidence:** worker API owner unverified; [reference S03, S05–S06](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Digest and verified body
**Identity:** `repo:1232040291:boundary:worker-fuzz-hash-001`.
**Dependency / data / impact:** fuzz→hash; bytes→`Digest`/`VerifiedBody`; type/API change→both target expectations.
**Surface / activation:** `Digest::compute`, `VerifiedBody::new`; both binaries.
**Contract:** each target computes the claimed digest from its own body before constructing the verified value.

**State / effects:** local value construction; no storage owned by hash is implied.
**Failure / containment:** constructor/API change can prevent target build or alter oracle setup.
**Validation:** source inspection only; no mismatch case or build was run.
**Coordination / evidence:** hash contract owner not identified; [source paths](REFERENCE.md#r08), `corelink-hash/src/lib.rs`. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Tenant derivation
**Identity:** `repo:1232040291:boundary:worker-fuzz-tenant-path-001`.
**Dependency / data / impact:** fuzz→tenant-path; fuzz TDK/UUID→worker `TenantCtx`→derived prefix; derivation change→both oracles.
**Surface / activation:** `TenantDerivationKey::from_bytes`; target creates context for each input.
**Contract:** the harness supplies arbitrary key/UUID bytes; tenant-path owns derivation semantics.

**State / effects:** TDK input is wrapped in `Zeroizing`; no real credential source is read.
**Failure / containment:** changed derivation can change key shape or isolation result in the fake.
**Validation:** inspect sources and local oracle; no fuzz run or reachability trace.
**Coordination / evidence:** tenant-path owner not identified; [S03–S05](REFERENCE.md#r08), `crates/tenant-path`. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — LibFuzzer entry
**Identity:** `repo:1232040291:boundary:worker-fuzz-libfuzzer-001`.
**Dependency / data / impact:** fuzz→libfuzzer-sys; engine bytes→`fuzz_target!` closure; engine/macro change→both fuzz bins.
**Surface / activation:** manifest version `0.4`; both target entry macros.
**Contract:** arbitrary byte slices drive assertions or early returns; panic/assertion is the finding boundary.

**State / effects:** runner may write corpus/artifact state; this review wrote none.
**Failure / containment:** runner failure is limited to the configured job/local run.
**Validation:** manifest/source read; no runner was invoked.
**Coordination / evidence:** resolved version and operator unknown; fuzz manifest and target sources at pinned commit. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Byte buffer dependency
**Identity:** `repo:1232040291:boundary:worker-fuzz-bytes-001`.
**Dependency / data / impact:** fuzz→bytes; target body slice→`Bytes`; API change→both target construction and equality checks.
**Surface / activation:** manifest version `1`; both targets.
**Contract:** buffers own or share body bytes as provided by `Bytes`; exact implementation resolution not queried.

**State / effects:** `r2_put_get_roundtrip` copies arbitrary body bytes before the size check.
**Failure / containment:** input size can exhaust local runner memory; no external data write exists here.
**Validation:** source inspection only; allocation/resource limits were not exercised.
**Coordination / evidence:** registry resolution unknown; target sources `S03–S04`. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — UUID decoding
**Identity:** `repo:1232040291:boundary:worker-fuzz-uuid-001`.
**Dependency / data / impact:** fuzz→uuid; 16-byte segments→`Uuid`; API behavior→tenant contexts/oracle.
**Surface / activation:** manifest version `1`; one UUID in `r2_path`, two in roundtrip.
**Contract:** bytes are passed to `Uuid::from_bytes`; no parsing or invalid-length branch follows the minimum-length guard.
**State / effects:** identifiers exist only for one input.
**Failure / containment:** dependency/API change is local to target construction.
**Validation:** source read; no property, build, or fuzz run.
**Coordination / evidence:** resolved version unknown; `S03–S04`. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Roundtrip read/write path
**Identity:** `repo:1232040291:boundary:worker-fuzz-worker-roundtrip-001`.

**Dependency / data / impact:** fuzz→worker; body→writer/fake→reader→comparison; read/write contract change→`API-002`.
**Surface / activation:** `R2Writer::put`, `R2Reader::get`, shared `InMemoryR2`; `r2_put_get_roundtrip` only.
**Contract:** checks fresh/duplicate writes, same-tenant equality, and cross-tenant NotFound.
**State / effects:** one fake per input; writes remain in process memory.
**Failure / containment:** assertion/expect/panic is local to the fuzz input; no live object is addressed.
**Validation:** inspect target and fake adapter; no execution.
**Coordination / evidence:** worker API owner unverified; [reference S04–S05](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — TDK zeroization wrapper
**Identity:** `repo:1232040291:boundary:worker-fuzz-zeroize-001`.
**Dependency / data / impact:** fuzz→zeroize; arbitrary 32-byte array→`Zeroizing`→key constructor; API change→both contexts.
**Surface / activation:** version `1.8` with `derive`; both targets.
**Contract:** source wraps input TDK arrays; this does not establish secret provenance.
**State / effects:** wrapper lifetime follows each input; no external secret is loaded.
**Failure / containment:** type or feature change can block the target build.
**Validation:** manifest/source inspection; no memory-clearing claim was measured.
**Coordination / evidence:** resolved features unknown; target sources `S03–S04`. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Synchronous local executor
**Identity:** `repo:1232040291:boundary:worker-fuzz-futures-executor-001`.
**Dependency / data / impact:** fuzz→futures-executor; writer future→`block_on` result; executor/API change→both target outcomes.
**Surface / activation:** version `0.3`; both targets call `futures_executor::block_on`.
**Contract:** the harness waits synchronously for its local writer/reader futures.
**State / effects:** no async service runtime is instantiated by this use.
**Failure / containment:** panic/error affects the current input/run only.
**Validation:** source read; no executor or target was run.
**Coordination / evidence:** resolved version unknown; `S03–S04`. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Separate workspace selection
**Identity:** `repo:1232040291:boundary:worker-fuzz-workspace-001`.
**Dependency / data / impact:** root workspace excludes fuzz; fuzz manifest owns its workspace; selection change→Cargo scope and fuzz jobs.
**Surface / activation:** root `exclude` entry and nested `[workspace]` at pinned source.
**Contract:** this is an independent package path, not a member inferred from folder name.

**State / effects:** standalone lockfile is present; Cargo did not resolve it here.
**Failure / containment:** workspace edits may include/exclude targets unexpectedly.
**Validation:** static manifest comparison; no Cargo command.
**Coordination / evidence:** root build policy; [source S01–S02](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Pull-request smoke configuration
**Identity:** `repo:1232040291:boundary:worker-fuzz-pr-smoke-001`.
**Dependency / data / impact:** worker workflow→both bins; engine input→fake-backed assertions; finding→PR job status.
**Surface / activation:** `fuzz-smoke`, pull-request event, 60 seconds per bin, self-hosted Mac runner.
**Contract:** workflow declares `cargo fuzz run` for each target from the parent crate directory.

**State / effects:** runner builds and fuzzes; no successful run was observed here.
**Failure / containment:** CI result belongs to workflow; runner and artifact details require run evidence.
**Validation:** YAML read only; no GitHub/provider access.
**Coordination / evidence:** workflow operator/reviewer unknown; [worker workflow](../../../../.github/workflows/corelink-worker.yml), `S07`. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Per-crate dormant nightly declaration
**Identity:** `repo:1232040291:boundary:worker-fuzz-worker-nightly-001`.
**Dependency / data / impact:** worker workflow→both bins; engine input→fake-backed assertions; job→runner load/artifact state.
**Surface / activation:** `fuzz-nightly` declares 3,600-second commands behind `if: github.event_name == 'schedule'`; its `on.schedule` block is commented at `corelink-worker.yml:22-32`, so no active per-crate trigger exists.
**Contract:** dormant workflow declaration, not an active invocation or completion record.

**State / effects:** no per-crate scheduled execution is active; a restored trigger could consume runner resources and emit findings.
**Failure / containment:** do not count this as a second active schedule; corpus and artifacts require run-specific evidence.
**Validation:** workflow source read only; no GitHub operation.
**Coordination / evidence:** operator/reviewer unknown; `S07`. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Root nightly matrix configuration
**Identity:** `repo:1232040291:boundary:worker-fuzz-root-nightly-001`.
**Dependency / data / impact:** nightly workflow→both target entries; generated input→target; failure→matrix job.
**Surface / activation:** active `on.schedule` and `workflow_dispatch`; `fuzz-matrix` includes `corelink-worker::{r2_path,r2_put_get_roundtrip}` for 3,600-second runs.
**Contract:** root matrix is the effective scheduled invocation surface at this pin; the per-crate schedule is commented.

**State / effects:** scheduled runner use and failure artifacts are configured, not observed.
**Validation:** workflow matrix read only; no job was triggered or inspected.
**Coordination / evidence:** workflow operator/reviewer unknown; [nightly workflow](../../../../.github/workflows/nightly.yml), `S08`. [Relation index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation

| Destination | Witness path | Condition | Causal effect | Containment / validation |
|---|---|---|---|---|
| Worker key constructor and fake | `REL-001` + `REL-002` + `REL-003` | Target receives at least 49 bytes | Tenant prefix and digest form an in-memory key checked by `r2_path` | Assertion stops at target; no remote storage path observed |
| Worker write/read methods | `REL-013` + `REL-002` + `REL-005` + `REL-006` + `REL-008` | Roundtrip input has at least 65 bytes | Body is wrapped, written twice, read back, then compared in the fake | Target oracle only; the writer size check follows body allocation |
| Fuzz job status | `REL-004` + `REL-010` / `REL-011` / `REL-012` | Active PR trigger or root nightly trigger plus job run; REL-011 is dormant | Fuzzer findings or resource failures can fail an active workflow job | Workflow source proves declaration/trigger shape, not execution; inspect a run before claiming result |
| Production R2 binding | No witness REL in this package | Would require a different composition/binding path | No causal edge from the inspected fake-backed harness to live R2 was established | Explicitly unknown; do not infer from names, worker dependency, or workflow |

Every edge above is a source-level call/configuration path. It does not prove runtime reachability, deployment, or observations beyond the selected source revision.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Affected records | Consumer / state effect | Validation to select | Coordination / recovery |
|---|---|---|---|---|
| Key grammar, tenant prefix, digest form | `API-001`, `INV-001`, `REL-001–003` | Harness oracle and parent adapter contract | `r2_path` bounded run plus source comparison | Contract owners are unverified; preserve old finding inputs and review key compatibility |
| Write, duplicate, size, or read semantics | `API-002`, `INV-002`, `REL-013`, `REL-002–008` | Fake-backed roundtrip and adversarial memory use | `r2_put_get_roundtrip` with explicit input bound; test the oversize branch only with an approved memory budget | Do not treat fake behavior as real binding compatibility |
| Dependency, feature, or workspace change | `REL-001–009` | Target selection/build can change | Reconcile manifest, lock, parent features, and inverse consumers before a future Cargo gate | Workspace/operator owner unverified; retain a restorable lockfile diff |
| Workflow trigger, duration, runner, or cache change | `REL-010–012` | PR smoke is active; root nightly is active; per-crate nightly is dormant unless re-armed | Review both workflow declarations and preserve one intentional schedule; run evidence requires authorized CI execution | Do not re-arm the per-crate cron while root matrix is active without an explicit duplication decision; owner is unverified |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---|---|
| Package targets | 2 | 2 | 0 | Cargo-expanded targets/features were not resolved |
| Direct dependencies | 8 | 8 | 0 | Resolved versions, optional closure, and inverse graph |
| Atomic relations | 13 | 13 | 0 | Independent relation-owner reconciliation |
| Out-of-Cargo literal matches | 14 files | 14 classified in B02 | Historical/report and parent-test references are not target consumers | Generated names, dynamic dispatch, run history, operator identities |
| Runtime / production storage | 0 established edges | 0 | No fake-to-production binding edge was evidenced | Deployment, credentials, actual R2 invocation, runtime observations |

**Explicit exclusions:** parent property-test mentions are not consumers of these fuzz binaries; sealed work-item text is design context; report files are not accepted as source-pinned runtime evidence. The source package itself only shows `InMemoryR2` in the two fuzz targets.

**Not observed:** no `cargo metadata`, build, test, fuzz run, network, GitHub/provider access, deployment, production, database, or storage operation. The literal census and source tree cannot certify completeness of dynamic/runtime wiring. Four cold reviews and any verified owner or escalation route remain unassigned.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
