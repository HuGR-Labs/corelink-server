---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-client-verify-fuzz
manifest: crates/corelink-client-verify/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: client-verify-fuzz-source-static-20260921
---

# corelink-client-verify-fuzz — blast radius

This map describes declared/static boundaries at the pinned source. It does not say either fuzz target ran, was reached, or found coverage. The direct implementation boundary is `corelink-client-verify`; CI workflow entries are configuration, not run evidence.

[Scope](#b01) · [Method](#b02) · [Relations](#b03) ·
[Propagation](#b04) · [Changes](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and priority

Highest-risk source changes are the FFI pointer/length oracle, the parent FFI contract it mirrors, target names/features, and CI command selection. The package has no persistent production state or SDK composition role established by source. Evidence stops at source declarations and calls; no resolver, build, CI readback, fuzz run, runtime observation, or deployment was examined.

**Build selection described by source:** two independent fuzz bins; path dependency enables `stream,ffi`. The workflow has an active PR-only `fuzz-smoke` job with 60-second invocations and a separately declared `fuzz-nightly` job with 3,600-second commands.

The nightly `on.schedule` trigger is commented at lines 23-33, so it is dormant. **Not observed:** any selected artifact, event or run result.

<a id="b02"></a>
## B02 — Inventory and method

| Population | Sources / method | Finding | Limit |
|---|---|---|---|
| Cargo package | Pinned root `Cargo.toml`, fuzz manifest and lock; static `git show` | Own `[workspace]`; root excludes fuzz path; package-local lock tracked | No Cargo metadata or resolved graph |
| Targets and calls | Two tracked `fuzz_targets/*.rs` files | `verify_sync`, `verify_ffi` | No compile, invocation, corpus, reachability or coverage |
| Inverse Cargo references | Exact `git grep` of `corelink-client-verify-fuzz` in tracked `*Cargo.toml` and `*Cargo.lock` | Own fuzz manifest and lock only | Literal census, not dynamic resolution |
| Outside-package exact-term files | Exact terms `corelink-client-verify/fuzz`, `corelink-client-verify-fuzz`, `verify_sync`, `verify_ffi`; tracked source at pin | 10 files outside the fuzz subtree, enumerated/classified in B06 | Parent benchmark substring and historical documents are not fuzz-binary call edges |
| Workflow / operation | `.github/workflows/corelink-client-verify.yml` source | PR path filter, active PR job, and dormant nightly job declaration | No GitHub access or run history; `on.schedule` is commented at lines 23-33 |
| Runtime / external state | No runtime, provider, SDK installation, corpus, DB or storage operation | UNKNOWN | No absence claim beyond inspected tracked package tree |

Source IDs and immutable links are listed in [R08](REFERENCE.md#r08). The complete tracked package subtree is four files: manifest, lockfile, and the two binaries. The parent package API consumers are not automatically consumers of this fuzz package; bug triage may require their owners, but no such path is implied here.

<a id="b03"></a>
## B03 — Direct relations

| ID | Type / direction | Surface | Activation | Contract owner |
|---|---|---|---|---|
| [REL-001](#rel-001) | build-deploy; outer workspace → excluded path | Root exclusion + local `[workspace]` | Workspace discovery | Repository workspace policy UNKNOWN |
| [REL-002](#rel-002) | dependency; fuzz package → parent library | Local path dep, `stream,ffi` | Selected fuzz manifest | `corelink-client-verify` source |
| [REL-003](#rel-003) | dependency; fuzz package → `libfuzzer-sys` | Direct `0.4`; both macros | Target build selection | External crate contract; resolution UNKNOWN |
| [REL-004](#rel-004) | dependency; fuzz package → `hex` | Direct `0.4`; no import in either target | Manifest resolution | External crate contract; direct use UNKNOWN |
| [REL-005](#rel-005) | dependency/source-call; fuzz → parent | Sync API in `verify_sync` | Callback source | Parent Rust API |
| [REL-006](#rel-006) | ffi; fuzz → parent | Unsafe C-ABI calls in `verify_ffi` | Callback source | Parent FFI API |
| [REL-007](#rel-007) | build-deploy; package path → CI workflow | Pull-request path filter | Matching PR event | Workflow contract; run UNKNOWN |
| [REL-008](#rel-008) | build-deploy; PR job → `verify_sync` | `cargo fuzz run`, 60 seconds | `pull_request` | Workflow configuration |
| [REL-009](#rel-009) | build-deploy; PR job → `verify_ffi` | `cargo fuzz run`, 60 seconds | `pull_request` | Workflow configuration |
| [REL-010](#rel-010) | build-deploy; dormant nightly job declaration → `verify_sync` | `cargo fuzz run`, 3,600 seconds | no active `schedule` trigger; job guard requires `schedule` | Workflow configuration |
| [REL-011](#rel-011) | build-deploy; dormant nightly job declaration → `verify_ffi` | `cargo fuzz run`, 3,600 seconds | no active `schedule` trigger; job guard requires `schedule` | Workflow configuration |

The PR job is source-wired under `pull_request`; the nightly job’s names and durations are dormant source configuration only. Do not promote either to execution records.

<a id="rel-001"></a>
### REL-001 — Independent workspace boundary

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-workspace-001`; peer root manifest.
**Dependency:** not applicable; this is workspace selection, not a package edge.
**Data:** not applicable.
**Impact:** root exclusion or local workspace changes may alter package discovery.
**Surface / activation:** root `Cargo.toml:384-394`; fuzz `Cargo.toml:1`; Cargo discovery only.
**Contract:** standalone package boundary declared; no metadata result.

**State / failure:** build configuration; wrong membership can change workspace selection.
**Containment:** no service/runtime edge shown. **Validation:** source diff only.
**Coordination / sources:** workspace owner UNKNOWN; S01, S02 at the pinned commit. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — Path dependency and feature selection

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-parent-dep-001`; peer parent manifest.
**Dependency:** fuzz package → `corelink-client-verify`.
**Data:** not applicable to the manifest edge; callback data is REL-005/006.
**Impact:** path or feature edits can change available dependency surfaces.

**Surface / activation:** `path = ".."`, `features = ["stream", "ffi"]`; selected fuzz manifest.
**Contract:** parent defaults empty; these two opt-in features are declared.

**State / failure:** compile graph only; resolution and compilation unknown.
**Containment:** no wrapper or production path inferred. **Validation:** source inspection.
**Coordination / sources:** parent API owner; S01, S05. [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — libFuzzer macro dependency

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-libfuzzer-001`; peer registry crate.
**Dependency:** fuzz package → `libfuzzer-sys` declared `0.4`.
**Data:** callback input semantics are not independently resolved here.
**Impact:** declaration or macro integration changes may affect selected target builds.
**Surface / activation:** `fuzz_target!` imports in both target files.
**Contract:** manifest key and macro references only; version resolution unknown.

**State / failure:** no package-owned persistent state established.
**Containment:** vendor implementation not inspected. **Validation:** SOURCE only.
**Coordination / sources:** dependency owner UNKNOWN; S01, S03, S04. [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — Declared hex dependency

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-hex-001`; peer registry crate.
**Dependency:** fuzz package → `hex` declared `0.4`.
**Data:** no direct flow identified in the two target files.
**Impact:** removing/changing the declaration affects manifest inputs; source use is unshown.
**Surface / activation:** `[dependencies]`; neither target imports `hex`.
**Contract:** declaration only; no selected build or resolution evidence.

**State / failure:** no runtime effect established.
**Containment:** no transitive-use claim. **Validation:** manifest + both targets inspected.
**Coordination / sources:** package manifest owner UNKNOWN; S01, S03, S04. [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — Sync verification oracle

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-sync-api-001`; peer parent library.
**Dependency:** fuzz package → parent Rust API.
**Data:** input bytes → claim/body; parent result → local assertions.
**Impact:** parent API/result changes may alter compile or oracle outcome; harness edits alter cases/assertions.

**Surface / activation:** `default_on`, `Digest::{from_hex,compute,verify_constant_time,to_hex}`, `verify`.
**Contract:** source asserts match success and mismatch error/fields.

**State / failure:** callback-local values; assertion panic is a possible fuzz finding.
**Containment:** oracle reuses parent `Digest`; not an independent BLAKE3 check.
**Validation:** source predicate only; no target run. **Coordination:** parent owner UNKNOWN; S03, S06. [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — FFI verifier and pointer oracle

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-ffi-001`; peer parent FFI module.
**Dependency:** fuzz package → parent `ffi` symbols.
**Data:** controls/bytes → owned buffers and raw arguments; return code → local assertions.
**Impact:** FFI contract changes may alter code tags or assertions; harness edits alter generated cases only.

**Surface / activation:** two constructors, `corelink_verifier_verify`, `corelink_verifier_free`.
**Contract:** null, length, codec, disabled, match/mismatch precedence asserted in source.

**State / failure:** scratch buffers live through the call; handle freed in callback source.
**Containment:** fuzz manifest allows unsafe; library denies it except local FFI module allowance.
**Validation:** source inspection; no unsafe execution. **Coordination:** parent FFI owner UNKNOWN; S01, S04, S06, S07. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — Workflow path filter

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-pr-filter-001`; peer CI workflow.
**Dependency:** not applicable; a workflow path filter is not Cargo dependency.
**Data:** not applicable.
**Impact:** a matching package-path change can make the PR workflow eligible.
**Surface / activation:** workflow `pull_request.paths: crates/corelink-client-verify/**`.
**Contract:** path match declaration only; event/run status unknown.

**State / failure:** CI selection; no repository-side data effect established.
**Containment:** no trigger result or job output inspected. **Validation:** workflow source.
**Coordination / sources:** workflow owner UNKNOWN; S08. [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — PR sync smoke invocation

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-pr-sync-001`; peer CI workflow.
**Dependency:** configured PR job → `verify_sync` target.
**Data:** fuzzer bytes → callback; process result → job if invoked.
**Impact:** target/job changes may affect PR gate outcome; no production path follows.
**Surface / activation:** `cargo fuzz run verify_sync -- -max_total_time=60`; `pull_request`.
**Contract:** workflow command and duration, not a result.

**State / failure:** runner/build/corpus effects unknown; no run readback.

**Containment:** no production path established. **Validation:** S07 only; not invoked here.
**Coordination / sources:** workflow operator identity UNKNOWN; S08. [Relation index](#b03)


<a id="rel-009"></a>
### REL-009 — PR FFI smoke invocation

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-pr-ffi-001`; peer CI workflow.
**Dependency:** configured PR job → `verify_ffi` target.
**Data:** fuzzer bytes → unsafe callback; process result → job if invoked.
**Impact:** target/job changes may affect PR gate outcome; no production path follows.
**Surface / activation:** `cargo fuzz run verify_ffi -- -max_total_time=60`; `pull_request`.
**Contract:** workflow command and duration, not a result.

**State / failure:** runner/build/corpus effects unknown; no run readback.

**Containment:** no production path established. **Validation:** S07 only; not invoked here.
**Coordination / sources:** workflow operator identity UNKNOWN; S08. [Relation index](#b03)


<a id="rel-010"></a>
### REL-010 — Scheduled sync fuzz invocation

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-nightly-sync-001`.
**Dependency:** declared `fuzz-nightly` → `verify_sync`; no active trigger.
**Data:** bytes → callback; result → job if invoked.
**Impact:** changes matter only if the trigger is restored; no production path.
**Surface / activation:** workflow `:393-395`; job guard `if: schedule`; `on.schedule` commented `:23-33`.
**Contract:** dormant job declaration and duration, not a result.

**State / failure:** runner/build/corpus effects inactive; no run readback. **Containment / validation:** no active schedule history; workflow SOURCE.
**Coordination / sources:** workflow operator identity UNKNOWN; S08. [Relation index](#b03)


<a id="rel-011"></a>
### REL-011 — Scheduled FFI fuzz invocation

**Identity:** `repo:1232040291:boundary:client-verify-fuzz-nightly-ffi-001`.
**Dependency:** declared `fuzz-nightly` → `verify_ffi`; no active trigger.
**Data:** bytes → unsafe callback; result → job if invoked.
**Impact:** changes matter only if the trigger is restored; no production path.
**Surface / activation:** workflow `:396-398`; job guard `if: schedule`; `on.schedule` commented `:23-33`.
**Contract:** dormant job declaration and duration, not a result.

**State / failure:** runner/build/corpus effects inactive; no run readback. **Containment / validation:** no active schedule history; workflow SOURCE.
**Coordination / sources:** workflow operator identity UNKNOWN; S08. [Relation index](#b03)


<a id="b04"></a>
## B04 — Propagation and containment

| Destination | Path | Condition | Causal effect | Containment / validation |
|---|---|---|---|---|
| Parent verifier API | REL-002 → REL-005 or REL-006 | A target source or parent API change | Compile/oracle outcome may differ if selected and invoked | Workflow config is static; review parent contract; no runtime consequence established |
| Pull-request gate | REL-007 → REL-008/009 | Matching PR path and workflow execution | Target/job process status may affect the job | No event, run, or result readback |
| Dormant nightly declarations | REL-010/011 | Schedule trigger restored and event emitted | Target/job process status may affect a future job | Trigger is commented; no schedule history or result readback |
| SDKs or service | No path from this fuzz package found | Would require separate fix/composition work in parent consumers | No direct data or code path from harness to SDK/runtime | Do not infer that a fuzz target changed or protects consumer behavior |

The parent library has separate consumers, but this package only declares and sources a local parent dependency. A discovered failure could motivate a separately owned fix; no automatic propagation into Python, Go, JS/WASM, CLI, service, or production is established.

<a id="b05"></a>
## B05 — Change, impact, validation

| Change | Relations / invariants | Likely bounded effect | Validation / coordination |
|---|---|---|---|
| Rename a bin or alter flags | REL-001/007–011; INV-002 | Workflow command may no longer name a declared target | Compare manifest and workflow source; owner UNKNOWN |
| Change dependency features | REL-002; INV-004/005 | Available library surfaces/lint boundary may change | Inspect parent feature/lint declarations; no build claim |
| Change sync oracle | REL-005; INV-003 | Assertion semantics or shared-hash assumption changes | Compare target with parent API source; independent review needed |
| Change FFI oracle or `unsafe` use | REL-006; INV-005 | Pointer lifetime, input precedence, or expected tags may drift | Review target and `src/ffi.rs`; stop for missing FFI owner |
| Change CI paths/commands | REL-007–011 | Eligibility or configured target/duration changes | Workflow source review; GitHub run remains unknown |
| Rely on a pentest/STRIDE claim | B06 contradiction | Historical documents name Merkle-proof fuzzing absent from these targets | Resolve with the documentation/security owner; identity/route UNKNOWN |

<a id="b06"></a>
## B06 — Coverage, exclusions, and unknowns

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Manifest direct deps | 3 | 3 | 0 | resolution/transitives |
| Declared bins | 2 | 2 | 0 | selected builds and execution |
| Outside-package exact-term files | 10 | 10 | 0 | dynamic/generated references |
| Runtime/CI results | 0 | 0 | 0 | all results and reachability |

**Outside-package census:** ten tracked files matched the exact search terms. The root `Cargo.toml` carries the workspace exclusion (REL-001), and `.github/workflows/corelink-client-verify.yml` carries the path filter and commands (REL-007–011).

Other literal matches are `benches/verify_bench.rs` (`bench_verify_sync`, a parent Criterion benchmark substring, not this fuzz bin); `reports/b326-loc-cap-baseline.txt` (path inventory); `reports/perf/b152-actions-2026-09-06.json` (job-name snapshot with unverified provenance/results); two 2026-05-27 Cargo audit documents (manifest history); and `WI-S02-003` (intent plus historical success claim). These are not direct package call edges.

The pentest evidence package and STRIDE report call this path Merkle/RFC-6962 proof fuzzing and/or “fuzz-tested”. That conflicts with pinned target code, which exercises client digest verification and C-ABI error handling, with no Merkle proof call. Exclude that historical claim as current source evidence and reconcile it with its owner before reuse; owner and route are UNKNOWN.

**Differences from independent census:** no additional exact-term source file was found. Historical inventory/audit/security prose is excluded as current call or runtime evidence. **Not observed:** resolved inverse Cargo graph, generated/alias paths, CI readback, input corpus, fuzz reachability/coverage, crash artifacts, SDK behavior, external owners, runner state, credentials, provider, storage, deployment, or production. Matching static counts do not prove completeness.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
