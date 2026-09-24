---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-tenant-path
manifest: crates/tenant-path/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: w007-tenant-path-static-source-20260920
---

# corelink-tenant-path — maintenance

These procedures are SOURCE-static and evidence-aware. They do not run or authorize Cargo, tests, benchmarks, fuzzing, key handling, tenant activity, storage access, cache operation, migration, rollback, deployment, publication, or production recovery.

[Baseline](#m01) · [Derivation](#m02) · [Types](#m03) · [Cache](#m04) · [Consumers](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix a static baseline

**Mode:** SOURCE_STATIC. **Prerequisite:** the target package is identified by `crates/tenant-path/Cargo.toml`. **Predicate:** package name and the four local source files are recorded against a fixed commit. **Action:** inspect manifest, `lib.rs`, `prefix.rs`, `cache.rs`, and `error.rs`. **Stop/recovery:** stop if baseline or package identity differs; recover by requesting the correct fixed revision. **Evidence:** manifest and source paths.

<a id="m02"></a>
## M02 — Review a derivation change

**Mode:** SOURCE_CONTRACT. **Prerequisite:** a change touches `derive_prefix`, its constants, or input/output types. **Predicate:** R03–R05 and B02 identify every local type, call order, and width claim. **Action:** compare declarations and call sequence with the fixed source. **Stop/recovery:** stop if collision probability, cryptographic assurance, keys, authorization, paths in storage, or runtime behavior must be established; recover by routing to the appropriate security, integration, or runtime evidence owner. **Evidence:** `src/prefix.rs`; R03–R05; B02.

<a id="m03"></a>
## M03 — Review a public-surface change

**Mode:** SOURCE_API. **Prerequisite:** an exported name, visibility, constructor, or formatter changed. **Predicate:** R01, R02, and R04 list the altered source-visible boundary. **Action:** trace `lib.rs` re-exports to declarations and record static direct import relations separately. **Stop/recovery:** stop if serialization, consumer compatibility, or a complete consumer census is required; recover with selected consumer source or resolved-graph evidence. **Evidence:** `src/lib.rs`; `src/prefix.rs`; B01, B04, B05.

<a id="m04"></a>
## M04 — Review a cache change

**Mode:** SOURCE_LOCAL. **Prerequisite:** `TdkVersion`, cache key, capacity, lock flow, or counters changed. **Predicate:** R06 and B03 describe the changed local control-flow relation without an execution claim. **Action:** inspect map key, direct-derive fallback, insertion, eviction, and metric declarations. **Stop/recovery:** stop if cache residency, rotation, memory, timing, concurrency, or recovery in a process must be proved; recover with separately collected execution or runtime evidence. **Evidence:** `src/cache.rs`; R06; B03.

<a id="m05"></a>
## M05 — Assess static relationships

**Mode:** SOURCE_RELATION. **Prerequisite:** a package API change may affect another package. **Predicate:** each stated relationship has a direct manifest edge, import, or re-export and its boundary is named. **Action:** inspect the selected worker and auth manifests/sources; preserve unknown consumers as unknown. **Stop/recovery:** stop on a request for full reverse dependencies, compiled features, storage behavior, or runtime reachability; recover with a resolved graph or owning integration review. **Evidence:** B04–B06.

<a id="m06"></a>
## M06 — Validate and hand off documentation

**Mode:** DOCUMENTARY. **Prerequisite:** only the assigned ownership guide and three package records changed. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; internal links resolve; profile-S structural checks and whitespace check pass. **Action:** run the supplied checker once for each kind and run `git diff --check` against the fixed baseline. **Stop/recovery:** stop on checker, link, diff, scope, or unknown-evidence failure; recover only in the four owned documents or hand off the exact finding. **Evidence:** checker JSON and diff status.

Success means claims are source-bounded, package identity is correct, and static relationships are falsifiable. Completeness means every required S/R/B/M record, procedure mode, evidence, stop, and unknown boundary is present. Quality means SOURCE facts remain separate from execution/runtime claims. DoD requires the four artifacts, successful structural and whitespace checks, scope-limited diff, and independent review; it does not claim Cargo/test/runtime or operational completion.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-tenant-path/SKILL.md#s01)
