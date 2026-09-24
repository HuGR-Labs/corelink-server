---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-byok-fuzz
manifest: crates/corelink-byok/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w016-byok-fuzz-source-1177dad2
---

# corelink-byok-fuzz — blast radius

This is a bounded static map of one independent fuzz manifest, two target
sources, and exact source references. It does not establish target execution,
provider use, persisted data, CI activity, or runtime reachability.

[Scope](#b01) · [Method](#b02) · [Relations](#b03) · [Propagation](#b04) ·
[Change impact](#b05) · [Coverage and unknowns](#b06).

<a id="b01"></a>
## B01 — Scope and boundary

The package declares two envelope-focused binaries and one local path dependency
on `corelink-byok`. Its sources parse envelope-shaped values and exercise a
stub-backed envelope path. Changes can affect target build compatibility or
assertions; no production edge is established by this map.

**Build selection:** UNKNOWN; no Cargo resolution, feature selection, or target
build was performed. The standalone workspace declaration does not prove the
targets ran.

<a id="b02"></a>
## B02 — Inventory and outside-Cargo census

| Population | Source method at pin | Discovered / documented / excluded | Limit |
|---|---|---:|---|
| Declared targets | Manifest `[[bin]]` entries and target source paths | 2 / 2 / 0 | Not execution evidence |
| Direct dependency keys | Manifest `[dependencies]` keys | 6 / 6 / 0 | Resolved versions/features UNKNOWN |
| Tracked package-tree files | Git tree under `crates/corelink-byok/fuzz/` | 3 / 3 / 0 | No untracked checkout files or runtime corpus inferred |
| Exact fuzz callers | Pinned exact-name/target search | 2 / 2 / 0 | Static references only; not a complete call graph |

The tracked package tree contains the manifest and two Rust target files; no
tracked corpus file was found there. Exact target references occur in
`scripts/fuzz-all.sh` and `.github/workflows/fuzz-nightly.yml`. The workflow
source is dispatch-only with its schedule commented as parked; this does not
show whether anyone dispatched it. `scripts/fuzz-all.sh` lists both targets
among a broader set and contains a `cargo +nightly fuzz run` loop; it was not run.

The exact string `envelope_roundtrip` also matches the parent crate's Criterion
bench and performance workflow/scripts/reports. Those references are excluded
from fuzz-target caller counts: they name the parent benchmark. Likewise,
historical audit prose is not an active invocation.

No broader Cargo inverse graph or TypeScript/SQL/schema/config consumer census
was performed. No workflow run or external repository census was performed.
Unknown remains explicit.

<a id="b03"></a>
## B03 — Direct relation index

| ID | Type / direction | Surface | Activation / contract owner |
|---|---|---|---|
| [REL-001](#rel-001) | dependency; fuzz → parent | `corelink_byok` APIs | Target use; parent owns API |
| [REL-002](#rel-002) | dependency; fuzz → external | `libfuzzer_sys` | Macro/target declaration; publisher/version unresolved |
| [REL-003](#rel-003) | dependency; fuzz → external | `serde_json` | JSON parse/serialize calls |
| [REL-004](#rel-004) | dependency; fuzz → external | `arbitrary` | Declared `derive`; use not found in target files |
| [REL-005](#rel-005) | dependency; fuzz → external | `tokio` | `rt` runtime call; feature selection unresolved |
| [REL-006](#rel-006) | dependency; fuzz → external | `async-trait` | Stub trait implementation attribute |
| [REL-007](#rel-007) | source invocation | `scripts/fuzz-all.sh` | Script loop; execution unknown |
| [REL-008](#rel-008) | build/run workflow | `.github/workflows/fuzz-nightly.yml` | Dispatch-only source configuration; run unknown |

<a id="rel-001"></a>
### REL-001 — Fuzz targets call parent envelope API

**Identity:** proposed `repo:1232040291:boundary:byok-fuzz-envelope-api-001`; peer reconciliation pending.
**Dependency direction:** `corelink-byok-fuzz → corelink-byok` (path declaration).
**Data direction:** harness → parent API, then returned values → harness assertions.
**Impact direction:** parent API changes can affect target compatibility; target-only changes do not establish a parent implementation change.

**Surface:** `WrappedDek`, `EncryptedBlob`, `EnvelopeEncryptor`, `DekCache`, and `KmsProvider`; see both target sources.
**Activation / state:** selected fuzz target only; local input and `StubKms` state.
**Failure / containment:** parse/error branches may return; assertions may panic. Containment ends at target; no runtime effect observed.
**Validation / coordination:** source review; coordinate API decisions with [parent reference](../corelink-byok/REFERENCE.md#r04). Its current blast-radius page has no matching shared key. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — libFuzzer macro dependency

**Identity:** `repo:1232040291:boundary:byok-fuzz-libfuzzer-001`.
**Dependency direction:** fuzz package → `libfuzzer-sys`; **data direction:** fuzzer input → target closure; **impact direction:** dependency/macro changes can alter target build or input dispatch.
**Surface:** `fuzz_target!` in both binaries; declared version `0.4`.
**Activation:** target build/run, not established here. **Owner:** external package owner UNKNOWN.
**Failure / containment:** target build or dispatch behavior is unknown; no result observed. **Validation:** manifest/source inspection only. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — JSON serde dependency

**Identity:** `repo:1232040291:boundary:byok-fuzz-serde-json-001`.
**Dependency direction:** fuzz package → `serde_json`; **data direction:** bytes → parsed values → serialized bytes; **impact direction:** serde behavior changes can alter parse/roundtrip assertions.
**Surface:** `from_slice` and `to_vec` for `WrappedDek` and `EncryptedBlob`; version key `1`.
**Activation:** successful parse branches only. **Owner/version resolution:** UNKNOWN.
**Failure / containment:** parse errors skip a branch; failed `expect` panics if reached. No run observed.
**Validation:** source inspection; parent type compatibility remains separate. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — `arbitrary` derive dependency

**Identity:** `repo:1232040291:boundary:byok-fuzz-arbitrary-001`.
**Dependency direction:** fuzz package → `arbitrary`; **data direction:** n/a; **impact direction:** declaration/feature changes may affect resolution or build.
**Surface:** direct key `arbitrary` v1 with `derive`; no use found in inspected targets.
**Activation / owner:** selection and publisher unresolved.
**Failure / limit:** resolved effect UNKNOWN; no claim beyond inspected files.
**Validation:** manifest and target-source inspection; no Cargo resolution. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Tokio runtime dependency

**Identity:** `repo:1232040291:boundary:byok-fuzz-tokio-001`.
**Dependency direction:** fuzz package → `tokio`; **data direction:** not applicable as a separate payload edge; **impact direction:** runtime API/feature changes may affect the target path.
**Surface:** `tokio::runtime::Builder::new_current_thread`, `.enable_time()`, and `.build()`; declared features `macros, rt`.
**Activation:** `envelope_roundtrip` target path only; feature selection UNKNOWN.
**Failure / containment:** runtime construction error returns early. **Validation:** static source inspection; no runtime created in this task. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — async-trait macro dependency

**Identity:** `repo:1232040291:boundary:byok-fuzz-async-trait-001`.
**Dependency direction:** fuzz package → `async-trait`; **data direction:** not applicable; **impact direction:** macro/signature changes may affect the local `KmsProvider` implementation at build time.
**Surface:** `#[async_trait] impl KmsProvider for StubKms`; declared version `0.1`.
**Activation / owner:** build selection and external owner/version UNKNOWN.
**Failure / containment:** compile-time compatibility only is in scope; no build result observed.
**Validation:** manifest and source inspection. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — `scripts/fuzz-all.sh` target entries

**Identity:** `repo:1232040291:boundary:byok-fuzz-all-script-001`.
**Dependency direction:** not Cargo; **data direction:** target name/duration → `cargo fuzz`; **impact direction:** edits change attempted commands.
**Surface:** both target entries; script enters the parent crate path and loops over all pairs.
**Activation:** explicit invocation; operator/authorization UNKNOWN.
**Failure / limit:** source exits nonzero if any target fails; no invocation observed.
**Validation:** pinned source only; no script/CI run. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Nightly fuzz workflow matrix

**Identity:** `repo:1232040291:boundary:byok-fuzz-nightly-workflow-001`.
**Dependency direction:** not Cargo; **data direction:** matrix target/duration → `cargo fuzz`; **impact direction:** edits change runner, command, retention, or activation.
**Surface:** two BYOK entries, templated parent working directory, run command, artifact path.
**Activation:** `workflow_dispatch`; schedule is commented/parked. Run and operator UNKNOWN.
**Failure / limit:** source describes artifact upload on failure; no run/artifact observed.
**Validation:** pinned workflow source only; no successful lane inferred. [Relation index](#b03)

<a id="b04"></a>
## B04 — Potential propagation paths

| Destination | REL path | Condition | Potential effect | Containment / evidence |
|---|---|---|---|---|
| `corelink-byok` target call surface | REL-007/008 → REL-001 | A listed target is actually invoked and reaches a call | API/build incompatibility may fail the target; no parent production propagation shown | Static script/workflow and target source; run status UNKNOWN |
| `corelink-byok` persisted envelope users | REL-001 | A parent wire representation changes | Compatibility could affect data outside this fuzz package | Persistence/composition owner and N/N-1 behavior UNKNOWN; stop for that claim |
| Fuzz workflow artifact path | REL-008 | A workflow run fails and uploads files | Artifact may be retained under workflow policy | No run or artifact observed; retention text is not evidence of an artifact |

There is no demonstrated path from these target sources to a live provider,
customer input, external storage, or deployment. Reachability beyond the listed
source references is not established.

<a id="b05"></a>
## B05 — Change, impact, and validation

| Change | API / INV / REL | Potential consumer effect | Validation / coordination |
|---|---|---|---|
| Target assertion or input layout | API-001/002, INV-001/002, REL-001 | Changes local fuzz oracle or accepted byte layout | Static source review; parent contract review if its symbols/shape changed |
| Direct dependency key or feature | REL-002–006 | Resolution/build compatibility may change | Resolve only under separate authorization; current resolution UNKNOWN |
| Parent serialized envelope shape | API-001/002, INV-001/002, REL-001 | New roundtrip assertions do not prove old-data compatibility | Ask the verified parent/data owner; data owner currently UNKNOWN |
| Fuzz runner list or activation | REL-007/008 | A future invocation may select these target names | Review workflow/script scope and authorization; do not infer a run |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Manifest targets | 2 | 2 | 0 | 0 at declaration level |
| Direct dependency keys | 6 | 6 | 0 | Resolved graph/features |
| Tracked package files | 3 | 3 | 0 | Untracked files and runtime corpus |
| Exact static fuzz callers | 2 | 2 | 0 | Other indirect callers and execution |
| Similar `envelope_roundtrip` benchmark hits | 0 fuzz callers | 0 | 1 population class | Parent benchmark references are not fuzz calls |

**Excluded matches:** parent `corelink-byok` Criterion benchmark and performance
workflow/scripts/reports that share the name `envelope_roundtrip`; historical
audit prose. They are not target invocations.

**Material unknowns:** completeness of Cargo inverses and outside-Cargo census,
selected targets/features, resolved versions, owner of workflow operation,
corpus/resource bounds, actual fuzz outcomes, parent data compatibility, runtime,
deployment, and cold review. Equal counts do not prove complete discovery.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to scope](#b01)
