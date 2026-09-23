---
schema: corelink-ownership/1.1
document: blast_radius
package: migrate-single-to-multi-region
manifest: apps/migrate-single-to-multi-region/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-migrate-single-to-multi-region-static-20260921
---

# migrate-single-to-multi-region — blast radius

This is a static map of one binary, its declared dependencies, source-local flows, and discovered non-Cargo references. Dependency, data-flow, and impact directions are separate. None proves a build, invocation, tenant data movement, provider access, audit delivery, or rollback.

[Scope](#b01) · [Census](#b02) · [Direct relations](#b03) ·
[Propagation](#b04) · [Change impact](#b05) · [Coverage](#b06).

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-migrate-single-to-multi-region/SKILL.md#s01).

<a id="b01"></a>
## B01 — Scope and reading rules

Cargo package.name is authoritative; declared tests are not test execution; dependencies/imports are not runtime reachability; local fakes are not provider behavior; advertised migration modes are not authorization. Read each direction independently. A source print containing Terraform, D1, or R2 text is not a provider call. Unknowns in B06 override names, comments, and design intent.

<a id="b02"></a>
## B02 — Census and method

| Population | Static search / source | Findings documented | Limit |
|---|---|---|---|
| Package and workspace | Root Cargo.toml, package Cargo.toml, package tree at pinned source | One member, one bin, one main.rs; corelink-region plus five normal and two dev dependency declarations | Declaration is not resolved graph. |
| Lockfile snapshot | Cargo.lock package entry at pinned source and integration baseline | This package's locked entry is unchanged; baseline removes `regex` only from unrelated `corelink-ops` | No target-specific resolution or build was run. |
| Cargo reverse edges | Literal package name search in worktree Cargo.toml files | No other manifest declares a dependency on this package | Does not discover dynamic process calls or untracked repositories. |
| Source imports and local effects | Package main.rs, imported corelink-region source contracts | Arg parsing, three mode bodies, report values, in-memory audit, stdout | No runtime trace, provider binding, or executed behavior. |
| Non-Cargo invocation and data paths | Exact package/binary names across worktree text; runbook, specs, Dockerfile, workflows, scripts, infra, tests, crates, tools, and generated SBOM | 19 outside-Cargo literal file hits; RB-region command text, Docker COPY, SBOM component, ADR/spec, and ownership references. No exact-name match in workflow files, scripts, infra, tests, crates, or tools. | Literal search cannot find generated, dynamic, external-repository, or differently named invocations. |
| Build and packaging | Dockerfile plus broad workflow/script target text | Docker copies source into build context; explicit build targets and runtime copies omit this binary. Generic workspace CI/SBOM commands may select it. | Workflow definitions are not runs or artifacts. |

The 19 outside-Cargo literal hits group into: the RB-region runbook; S14 ADR, work item, and sprint notes; the knowledge ADR and OKF concept manifest; Dockerfile; generated `.sbom/cyclonedx-rust.json`; ownership [CARGO_CENSUS](../../CARGO_CENSUS.md), WAVE_015_PLAN, and ROLLOUT; seven historical audit reports; and the LOC baseline report. Six implementation-relevant paths map to REL-014 and REL-018–023, with REL-020/023 sharing RB-region; the other 13 are reference/history/LOC text, not binary call sites. The generated `docs/okf-wiki-site/index.html` was excluded as derived output.

Cross-package reconciliation uses canonical keys `repo:1232040291:boundary:migrate-single-to-multi-region-region-parse`, `repo:1232040291:boundary:migrate-single-to-multi-region-report`, and `repo:1232040291:boundary:migrate-single-to-multi-region-audit`. Their peer anchors are `corelink-region#R04` (region), `corelink-region#R07` (migration), and `corelink-region#R05` (audit); the peer blast document currently expresses these as B01–B06 prose rather than matching REL IDs. Shared facts are limited to symbol names, source paths, and local in-memory semantics; peer ownership remains with corelink-region and no runtime consumer is inferred.

The specific operation/design sources are specs/05_runbooks/RB-region.md; specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md; specs/04_sprints/S14/work_items/WI-S14-001-r2-d1-do-provisioning-4-regions-terraform.md; and specs/04_sprints/S14/sprint.md. The old snake-case script spelling appears in specification text; no scripts/migrate_single_to_multi_region.rs source file was found.

Reference-only exclusions are specs/04_sprints/S14/sprint.md, docs/internal/okf-wiki/concept-manifest.yaml, docs/ownership/CARGO_CENSUS.md, docs/ownership/WAVE_015_PLAN.md, docs/ownership/ROLLOUT.md, reports/b326-loc-cap-baseline.txt, and generated docs/okf-wiki-site/index.html; they respectively summarize the task, route a concept, inventory ownership, count source lines, or are derived output. The seven history reports are specs/_audits/2026-05-27-w32-phaseE-container-audit-seal.md, specs/_audits/2026-05-27-cargo-toml-followups-seal.md, specs/_audits/2026-05-27-cargo-toml-audit-post-w36.md, specs/_audits/sealed/2026-05-15-replication-audit.md, specs/_audits/sealed/2026-05-26-w32-phaseE-apply.md, specs/_audits/sealed/2026-05-22-w33-stage2-a-worker-moves.md, and specs/_audits/sealed/2026-05-26-w33-stage2-b-container.md; these record prior design/build discussions, not current calls or outcomes.

Build/composition references found by the broader static target review are Dockerfile, .github/workflows/codeql.yml, .github/workflows/cas_foundation.yml, .github/workflows/sbom-consolidated.yml, and scripts/sbom-aggregate.sh. The three workflows and script do not contain the package name; their workspace-wide target declarations are a separate potential selection. The old snake-case spelling is only in specification text; no scripts/migrate_single_to_multi_region.rs source file was found.

No Cargo graph resolution or external runtime census was performed. Static workspace-wide workflow commands are recorded as potential selection only. The bounded census does not establish complete callers.

<a id="b03"></a>
## B03 — Atomic direct relations

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018) · [REL-019](#rel-019) · [REL-020](#rel-020) · [REL-021](#rel-021) · [REL-022](#rel-022) · [REL-023](#rel-023) · [REL-024](#rel-024) · [REL-025](#rel-025) · [REL-027](#rel-027) · [REL-028](#rel-028).

<a id="rel-001"></a>
### REL-001 — Region parser contract

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-region-parse`; producer `apps/migrate-single-to-multi-region/src/main.rs#Args::parse`; consumer `corelink-region#Region::from_str`; peer `corelink-region#R04`.

**Directions:** consumer→provider; caller→binary region value; vocabulary change→source and CLI callers.

**Surface / activation:** `--target-region` in `Args::parse` at startup.

**Contract / state:** `Region::from_str` returns `Region` or parse error; value is stored in local `Args`; no provider state.

**Failure / limit:** invalid value reaches anyhow; region availability/runtime selection unknown.

**Validation / coordination:** compare parser use with corelink-region; coordinate vocabulary changes there. [Index](#b03)

<a id="rel-002"></a>
### REL-002 — anyhow error boundary

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-anyhow`; producer `apps/migrate-single-to-multi-region/src/main.rs#main,Args::parse`; consumer `anyhow::Error` and `anyhow!`.

**Directions:** dependency consumer→provider; data bidirectional for locally constructed error values; impact error API change→source compatibility, local propagation change→main result handling.

**Surface / activation:** normal dependency; Args::parse result, anyhow::Error return types, anyhow! construction, and propagated errors.

**Contract / state:** local error wrapping only; no persistent state.

**Failure / limit:** invalid region, serialization, or audit error may return Err in source; no such failure was induced.

**Validation / coordination:** inspect signatures and each ? path; external crate implementation is outside ownership. [Index](#b03)

<a id="rel-003"></a>
### REL-003 — serde declaration

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-serde`; producer `apps/migrate-single-to-multi-region/Cargo.toml#[dependencies]`; consumer `serde` resolver/dependency edge.

**Directions:** dependency consumer→provider; data not-applicable for a direct source call; impact dependency removal/version change→possible target resolution or trait compatibility.

**Surface / activation:** normal dependency declaration; main.rs has no direct serde import or call.

**Contract / state:** manifest declaration only; the imported report type may implement serialization outside this package.

**Failure / limit:** direct target need is unconfirmed; do not infer use from serde_json or upstream derives.

**Validation / coordination:** inspect manifest and package source; resolve only under a separately authorized Cargo review. [Index](#b03)

<a id="rel-004"></a>
### REL-004 — serde_json report formatting

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-serde-json`; producer `apps/migrate-single-to-multi-region/src/main.rs#run_dry_run`; consumer `serde_json::to_string_pretty`.

**Directions:** dependency consumer→provider; data bidirectional between local MigrationReport and the formatter; impact serialization API/type change→source compatibility and output shape.

**Surface / activation:** normal dependency; run_dry_run calls to_string_pretty and maps its error to anyhow.

**Contract / state:** a local report is formatted as a String before println; no external file or audit store is used.

**Failure / limit:** serialization can return Err in source; no output or serialized tenant data was observed.

**Validation / coordination:** inspect the call and report type owner; formatting does not verify the migration. [Index](#b03)

<a id="rel-005"></a>
### REL-005 — uuid declaration

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-uuid`; producer `apps/migrate-single-to-multi-region/Cargo.toml#uuid`; consumer `uuid` resolver/dependency edge.

**Directions:** dependency consumer→provider; data not-applicable for a direct local UUID call; impact feature/version change→possible package resolution or upstream type compatibility.

**Surface / activation:** normal dependency declares v7 and serde; main.rs contains no direct uuid import or call.

**Contract / state:** the MigrationReport run_id is supplied by corelink-region constructors; that does not show this package uses its direct uuid declaration.

**Failure / limit:** direct use is unobserved; no UUID generation was separately inspected in this package.

**Validation / coordination:** review manifest and source, then resolve the target only with separate authorization. [Index](#b03)

<a id="rel-006"></a>
### REL-006 — Tokio entrypoint

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-tokio`; producer `apps/migrate-single-to-multi-region/src/main.rs#main`; consumer `#[tokio::main]`.

**Directions:** dependency consumer→provider; data not-applicable at the macro boundary; impact macro/runtime contract change→entrypoint compatibility.

**Surface / activation:** normal dependency and #[tokio::main] on main.

**Contract / state:** source declares an async main transformed through Tokio; no scheduler, network client, or runtime invocation was observed.

**Failure / limit:** build/runtime behavior is unknown; async syntax is not evidence of a live task.

**Validation / coordination:** inspect attribute and manifest; no runtime claim follows from the macro. [Index](#b03)

<a id="rel-007"></a>
### REL-007 — proptest development declaration

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-proptest`; producer `apps/migrate-single-to-multi-region/Cargo.toml#[dev-dependencies]`; consumer `proptest` resolver/dependency edge.

**Directions:** dependency consumer→provider in development scope only; data not-applicable; impact change→possible test-target dependency resolution.

**Surface / activation:** dev-dependency; no package test source or proptest use was found.

**Contract / state:** not a production edge and not evidence a property test exists or ran.

**Failure / limit:** whether a generated/untracked test consumer exists is unknown.

**Validation / coordination:** re-census package tree before changing the declaration; do not run tests in this task. [Index](#b03)

<a id="rel-008"></a>
### REL-008 — tokio-test development declaration

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-tokio-test`; producer `apps/migrate-single-to-multi-region/Cargo.toml#[dev-dependencies]`; consumer `tokio-test` resolver/dependency edge.

**Directions:** dependency consumer→provider in development scope only; data not-applicable; impact change→possible test-target dependency resolution.

**Surface / activation:** dev-dependency; no package test source or tokio-test use was found.

**Contract / state:** not a production edge and not evidence async test coverage exists or ran.

**Failure / limit:** test-target resolution and future test use are unknown.

**Validation / coordination:** re-census package tree before changing the declaration; do not run tests in this task. [Index](#b03)

<a id="rel-009"></a>
### REL-009 — Mode and filter arguments

**Identity / type:** runtime-call; key `repo:1232040291:boundary:migrate-single-to-multi-region-cli-modes`; producer unidentified process argv; consumer `apps/migrate-single-to-multi-region/src/main.rs#Args::parse`.

**Directions:** dependency not-applicable; data caller→binary through argv; impact parser/flag change→documented callers and runbook.

**Surface / activation:** process startup; --dry-run, --execute, --rollback, and --tenant-id-filter.

**Contract / state:** flags mutate local booleans/filter; unknown flags are ignored and mode conflicts are not rejected.

**Failure / limit:** rollback has dispatch priority; caller identity/invocation is unknown and no mode was called.

**Validation / coordination:** compare parser and runbook text; do not invoke a mode. [Index](#b03)

<a id="rel-010"></a>
### REL-010 — Target-region argument

**Identity / type:** config; key `repo:1232040291:boundary:migrate-single-to-multi-region-target-region`; producer unidentified process argv; consumer `apps/migrate-single-to-multi-region/src/main.rs#Args::parse → crates/corelink-region/src/region.rs#Region::from_str`.

**Directions:** dependency binary→corelink-region; data caller→binary then returned Region→local code; impact accepted-value change→callers and report labels.

**Surface / activation:** --target-region; default string wnam; parsed during startup.

**Contract / state:** imported parser accepts its source-declared region spellings and returns an error for an unknown string.

**Failure / limit:** a value parse error propagates; deployed region availability is unknown.

**Validation / coordination:** compare exact parser and Region source; no target was supplied or tested. [Index](#b03)

<a id="rel-011"></a>
### REL-011 — Dry-run report to stdout

**Identity / type:** data; key `repo:1232040291:boundary:migrate-single-to-multi-region-dry-run-output`; producer `apps/migrate-single-to-multi-region/src/main.rs#run_dry_run`; consumer unidentified stdout reader.

**Directions:** dependency not-applicable; data binary→stdout; impact field/count/format change→text consumers.

**Surface / activation:** main default branch when rollback and execute are false.

**Contract / state:** three hard-coded tuples feed MigrationReport and pretty JSON; duration uses rows plus blobs times ten.

**Failure / limit:** serialization or time handling is local; no D1 query, R2 count, actual report, or consumer was observed.

**Validation / coordination:** inspect tuple, filter, arithmetic, and print mapping. [Index](#b03)

<a id="rel-012"></a>
### REL-012 — Execute branch to audit sink

**Identity / type:** data; key `repo:1232040291:boundary:migrate-single-to-multi-region-execute-audit`; producer `apps/migrate-single-to-multi-region/src/main.rs#run_execute`; consumer `crates/corelink-region/src/audit.rs#InMemoryRegionAuditSink`.

**Directions:** dependency binary→corelink-region; data binary→sink; impact sink/record contract change→source compatibility and local error propagation.

**Surface / activation:** execute branch; two hard-coded tenant tuples.

**Contract / state:** literal hash_ plus ID is passed to an audit constructor and emitted to an in-memory sink; report construction is a separate REL-024 surface.

**Failure / limit:** emit Err aborts locally; no D1/R2 mutation, computed hash, durable audit, or real tenant was observed.

**Validation / coordination:** inspect emit order and local values; coordinate interface edits with corelink-region. [Index](#b03)

<a id="rel-013"></a>
### REL-013 — Rollback source output to operator

**Identity / type:** data; key `repo:1232040291:boundary:migrate-single-to-multi-region-rollback-output`; producer `apps/migrate-single-to-multi-region/src/main.rs#run_rollback`; consumer unidentified stdout reader.

**Directions:** dependency not-applicable; data binary→stdout; impact output/mode change→prospective operator instructions.

**Surface / activation:** the selected rollback branch in apps/migrate-single-to-multi-region/src/main.rs.

**Contract / state:** source prints manual Terraform/D1 PITR/R2 restore directions and returns Ok; no persistent state is handled by this branch.

**Failure / limit:** no restore or post-restore verification occurs in this source; operator, authorization, and actual state are unknown.

**Validation / coordination:** compare printed strings and local return; no rollback command is permitted here. [Index](#b03)

<a id="rel-014"></a>
### REL-014 — Docker package-context input

**Identity / type:** build-deploy; key `repo:1232040291:boundary:migrate-single-to-multi-region-docker-context`; producer `apps/migrate-single-to-multi-region/{Cargo.toml,src/main.rs}`; consumer `Dockerfile#builder COPY`.

**Directions:** dependency builder→package source; data package files→builder context; impact source/manifest edit→builder cache input.

**Surface / activation:** Dockerfile copies apps/migrate-single-to-multi-region and workspace manifests; its explicit release targets are corelink-server and gc binaries.

**Contract / state:** Dockerfile copies this package directory and workspace manifests into the builder context; binary selection is separated into REL-027.

**Failure / limit:** Dockerfile text is not a build; no image or shipped migration binary was inspected.

**Validation / coordination:** compare package COPY paths with the manifest; no container command was run. [Index](#b03)

<a id="rel-015"></a>
### REL-015 — CodeQL workspace target selection

**Identity / type:** build-deploy; key `repo:1232040291:boundary:migrate-single-to-multi-region-codeql-selection`; producer `.github/workflows/codeql.yml#Rust matrix`; consumer Cargo workspace build step.

**Directions:** dependency workflow→workspace packages; data not-applicable; impact package change→possible workspace compile/analysis.

**Surface / activation:** .github/workflows/codeql.yml has a Rust matrix step declaring cargo build --workspace --all-targets --locked.

**Contract / state:** the root workspace includes this package; the workflow has no package-specific invocation.

**Failure / limit:** definition does not prove workflow trigger, package compilation, analysis, or binary execution.

**Validation / coordination:** static compare workflow and workspace members; GitHub was not contacted. [Index](#b03)

<a id="rel-016"></a>
### REL-016 — Workspace build-and-test workflow

**Identity / type:** test; key `repo:1232040291:boundary:migrate-single-to-multi-region-cas-foundation-selection`; producer `.github/workflows/cas_foundation.yml#workspace steps`; consumer Cargo workspace targets.

**Directions:** dependency workflow→workspace packages; data not-applicable; impact package change→possible compile/test selection.

**Surface / activation:** .github/workflows/cas_foundation.yml defines workspace all-target clippy, test, and doc commands.

**Contract / state:** this package is a workspace member; its source declares no test target/file in the inspected tree.

**Failure / limit:** no workflow run, package test, or output was observed.

**Validation / coordination:** inspect workflow target text and source inventory; do not trigger GitHub or Cargo. [Index](#b03)

<a id="rel-017"></a>
### REL-017 — Workspace SBOM aggregation

**Identity / type:** runtime-call; key `repo:1232040291:boundary:migrate-single-to-multi-region-sbom-workflow`; producer `.github/workflows/sbom-consolidated.yml#SBOM step`; consumer `scripts/sbom-aggregate.sh`.

**Directions:** dependency workflow→aggregation script; data SBOM_REF workflow input→script environment; impact workflow/input change→possible aggregation selection.

**Surface / activation:** .github/workflows/sbom-consolidated.yml calls scripts/sbom-aggregate.sh for workspace SBOM work.

**Contract / state:** workflow text only; no script invocation, SBOM artifact, or package row was observed.

**Failure / limit:** trigger and downstream outcome are unknown.

**Validation / coordination:** inspect the separate script relation below; no workflow was run. [Index](#b03)

<a id="rel-018"></a>
### REL-018 — S14 architecture decision text

**Identity / type:** external-contract; key `repo:1232040291:boundary:migrate-single-to-multi-region-s14-adr`; producer `specs/03_architecture/adrs/ADR-S14-001-multi-region-terraform-module.md`; consumer implementation reader of `apps/migrate-single-to-multi-region/src/main.rs`.

**Directions:** dependency not-applicable; data design text→reader; impact requirement or wording change→source/docs reconciliation.

**Surface / activation:** Decision 4 describes a migration binary with dry-run/execute/rollback and audit behavior.

**Contract / state:** design intent only; R04 identifies a material gap from the inspected implementation.

**Failure / limit:** ADR prose is not implementation, test, provider, or runtime proof; the contradiction remains explicit.

**Validation / coordination:** route concepts through the wave plan; do not copy or revalidate policy. [Index](#b03)

<a id="rel-019"></a>
### REL-019 — S14 work-item source expectation

**Identity / type:** external-contract; key `repo:1232040291:boundary:migrate-single-to-multi-region-s14-work-item`; producer `specs/04_sprints/S14/work_items/WI-S14-001-r2-d1-do-provisioning-4-regions-terraform.md`; consumer implementer of `apps/migrate-single-to-multi-region/src/main.rs`.

**Directions:** dependency not-applicable; data work-item text→reader; impact requirement/source-path change→design reconciliation.

**Surface / activation:** work item describes a migration script under scripts/migrate_single_to_multi_region.rs and contains command/type examples.

**Contract / state:** specification intent, not the current package source path or evidence that the sample was built.

**Failure / limit:** the named scripts source is absent from the inspected worktree; tenant/provider behavior is unknown.

**Validation / coordination:** compare work-item expectations with the current manifest/source and preserve the mismatch. [Index](#b03)

<a id="rel-020"></a>
### REL-020 — RB-region command procedure

**Identity / type:** external-contract; key `repo:1232040291:boundary:migrate-single-to-multi-region-runbook-cli`; producer `specs/05_runbooks/RB-region.md#sections-4-5`; consumer prospective operator of `migrate-single-to-multi-region`.

**Directions:** dependency not-applicable; data runbook text→reader/caller; impact flag or target change→runbook compatibility.

**Surface / activation:** sections 4–5 show dry-run, execute, and rollback command text for the declared binary.

**Contract / state:** procedure text only; the runbook is not an invocation, approval, or current-owner proof.

**Failure / limit:** source mode behavior differs from some advertised effects; no command was run.

**Validation / coordination:** reconcile exact flags against API-001/002; stop before any mode or provider action. [Index](#b03)

<a id="rel-021"></a>
### REL-021 — Derived knowledge ADR claims

**Identity / type:** external-contract; key `repo:1232040291:boundary:migrate-single-to-multi-region-knowledge-adr`; producer `docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md`; consumer knowledge reader comparing `apps/migrate-single-to-multi-region/src/main.rs`.

**Directions:** dependency not-applicable; data narrative→reader; impact claim/source change→knowledge reconciliation.

**Surface / activation:** the narrative calls the binary built and tested and describes audit-before-mutation behavior.

**Contract / state:** derived documentation only; those claims are not supported as runtime/build facts by this static inspection.

**Failure / limit:** no build/test, audit persistence, region availability, or live provisioned state was observed.

**Validation / coordination:** retain the discrepancy; this ownership record does not revise the knowledge artifact. [Index](#b03)

<a id="rel-022"></a>
### REL-022 — SBOM script and workspace metadata

**Identity / type:** build-deploy; key `repo:1232040291:boundary:migrate-single-to-multi-region-sbom-script`; producer `scripts/sbom-aggregate.sh`; consumer Cargo CycloneDX, `.sbom/cyclonedx-rust.json`, and `target/sbom/corelink-workspace.cdx.json`.

**Directions:** dependency script→Cargo CycloneDX; data workspace manifests/BOM files→local aggregation output; impact package manifest change→possible SBOM component/dependency entry.

**Surface / activation:** script runs cargo cyclonedx from workspace root and gathers bom.json files, then writes target/sbom/corelink-workspace.cdx.json.

**Contract / state:** script text and generated SBOM component are static evidence; output path and whole-workspace intent are not an execution result.

**Failure / limit:** tool availability, invocation, generated package component, and publication are unknown.

**Validation / coordination:** inspect script and workflow statically; no Cargo, SBOM, or publication command was run. [Index](#b03)

<a id="rel-023"></a>
### REL-023 — RB-region manual restore directions

**Identity / type:** external-contract; key `repo:1232040291:boundary:migrate-single-to-multi-region-runbook-recovery`; producer `specs/05_runbooks/RB-region.md#section-5`; consumer prospective recovery operator for Terraform/D1/R2.

**Directions:** dependency not-applicable; data recovery text→reader; impact wording/state assumptions→recovery plan.

**Surface / activation:** section 5 documents manual Terraform-state, D1 PITR, and R2 version restore steps.

**Contract / state:** operator guidance only; actual provider state, snapshot, permissions, and recovery authority are not established.

**Failure / limit:** the package source does not perform or verify those actions; Git revert cannot establish data recovery.

**Validation / coordination:** use only a separately verified operation owner and approved recovery plan; neither is identified here. [Index](#b03)

<a id="rel-024"></a>
### REL-024 — Migration report/result contract

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-report`; producer `apps/migrate-single-to-multi-region/src/main.rs#run_dry_run,run_execute`; consumer `crates/corelink-region/src/migration.rs#MigrationReport,TenantMigrationResult,MigrationDecision`; peer `corelink-region#R07`.

**Directions:** dependency binary→corelink-region; data local tuples→report and report→stdout; impact report taxonomy/field change→source construction and output review.

**Surface / activation:** report constructors and `add_tenant_result` in the two local mode functions.

**Contract / state:** source constructs in-process report/result values; no D1/R2 inventory or durable report was observed.

**Failure / limit:** serialization or imported constructor/field changes can fail locally; no report was executed or observed.

**Validation / coordination:** compare exact migration source symbols with the corelink-region reference; coordinate type changes with that package owner. [Index](#b03)

<a id="rel-025"></a>
### REL-025 — Audit record/sink contract

**Identity / type:** dependency; key `repo:1232040291:boundary:migrate-single-to-multi-region-audit`; producer `apps/migrate-single-to-multi-region/src/main.rs#run_execute`; consumer `crates/corelink-region/src/{event.rs,audit.rs}#RegionAuditRecord,InMemoryRegionAuditSink`; peer `corelink-region#R05`.

**Directions:** dependency binary→corelink-region; data record→local sink; impact contract change→source/error propagation.

**Surface / activation:** execute branch constructs and emits the record before adding a result.

**Contract / state:** sink is in-memory; value uses literal `hash_`; no durable delivery observed.

**Failure / limit:** emit failure maps to anyhow; no external audit or state mutation follows.

**Validation / coordination:** inspect order and peer contract; coordinate edits with corelink-region. [Index](#b03)

<a id="rel-027"></a>
### REL-027 — Docker binary selection

**Identity / type:** build-deploy; key `repo:1232040291:boundary:migrate-single-to-multi-region-docker-binary-selection`; producer `Dockerfile#builder RUN/runtime COPY`; consumer image runtime filesystem.

**Directions:** dependency build recipe→binary selection; data compiled binaries→runtime image; impact target/name change→image contents and prospective operator availability.

**Surface / activation:** Dockerfile names corelink-server and gc binaries for build/runtime stages and omits this binary.

**Contract / state:** omission is a static recipe fact, not proof of an image or deployment.

**Failure / limit:** no container build, tag, digest, or shipped artifact was observed.

**Validation / coordination:** compare build and runtime binary lists; use `built-not-wired` for any shipped-target conclusion. [Index](#b03)

<a id="rel-028"></a>
### REL-028 — Cargo lockfile snapshot

**Identity / type:** build; key `repo:1232040291:boundary:migrate-single-to-multi-region-lockfile`; producer `Cargo.lock#package[migrate-single-to-multi-region]`; consumer Cargo resolver/build event for `apps/migrate-single-to-multi-region/Cargo.toml`.

**Directions:** dependency lockfile→selected package graph; data locked package/version/dependency entries→build input; impact lockfile drift→resolution and reproducibility review.

**Surface / activation:** locked commands or other consumers that read the committed workspace lockfile.

**Contract / state:** the migration package entry is unchanged between source pin `1177dad2` and integration baseline `ab7137cd`; the baseline removes `regex` only from unrelated `corelink-ops`.

**Failure / limit:** no lock resolution, target-specific graph, or build was run; the snapshot does not prove reachability.

**Validation / coordination:** compare the exact lockfile package entry and record any changed package separately; do not call the unrelated drift a migration change. [Index](#b03)

<a id="b04"></a>
## B04 — Propagation paths and limits

| Destination | Static path | Condition | Causal effect | Containment / evidence |
|---|---|---|---|---|
| Local caller interface | REL-009/010 → Args::parse → main branch | If a caller supplies flags | Parser or precedence changes can select different local function. | No caller or process execution observed. |
| Local report and output | REL-004/011/024/025 → report constructors/results or sink → stdout | Selected source branch | API, fixture, serialization, or audit-contract changes alter local report/output shape. | Hard-coded data and in-memory sink bound the inspected body. |
| Prospective operator | REL-013/020/023 → source output and runbook text | If an operator follows external text | Changed flags or instructions can diverge. | No command, authorization, provider state, or restore result observed. |
| Workspace builder / CI | REL-014/015/016/017/022/027/028 → context, binary selection, workspace selection, or metadata | When defined workflow/build is triggered | Source, binary-list, lockfile, or manifest changes may affect compile/test/image/SBOM inputs. | Definition-only evidence; no target graph, build, CI, artifact, or test result. |
| S14 design reader | REL-018/019/021 → ADR/work-item/knowledge text | When implementation is compared with design | Stated migration intent and local implementation can disagree. | Record contradiction; do not infer missing implementation or actual operation. |

No resolved feature/target graph was collected, so transitive Cargo consumers and their source call paths are unknown. The single declared first-party edge terminates at corelink-region for this artifact; this map does not recursively claim its providers or consumers.

<a id="b05"></a>
## B05 — Change to impact and validation

| Change | Relations / contracts | Potential effect | Documentary validation |
|---|---|---|---|
| Package, binary name/path, dependency, feature declaration | REL-001–008, REL-014–017, REL-022, REL-028 | Workspace, caller, target, lockfile, and SBOM selection may change. | Reconcile manifest, lockfile package entry, and source inventory; use only the documented future package-scoped matrix in M04. |
| Flag parsing, mode precedence, target validation | REL-001/009/010/019/020; API-001/002 | Caller selection and runbook wording may diverge. | Trace each parser assignment through main; no mode invocation. |
| Dry-run or report fields/arithmetic | REL-004/011/024; API-003 | Local report shape or stdout consumers may change; provider inventory remains unproven. | Reconcile report fields, tuple/filter/formula and printed mappings. |
| Execute, audit, or verification logic | REL-012/024/025; API-004 | Imported report/audit contract and local result text may change; external persistence/atomicity claims need separate evidence. | Inspect call order and exact sink; do not migrate or contact a provider. |
| Rollback wording or recovery requirements | REL-013/023; API-005 | Printed and written instructions may diverge from source behavior. | Reconcile source text with RB-region and mark actual recovery owner unknown. |
| S14 design / knowledge claims | REL-018/019/021 | Requirements, sample source path, or derived claims can conflict with implementation. | Compare each source document separately; keep contradictions explicit. |
| Build, CI, or SBOM configuration | REL-014–017, REL-022, REL-027–028 | Potential target, image, lockfile, or metadata selection may change. | Static workflow/Dockerfile/script/lockfile review only; no CI, Docker, Cargo, or SBOM execution. |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Normal direct dependency declarations | 6 | 6 | 0 | 0 declaration records; resolution remains unknown |
| Development dependency declarations | 2 | 2 | 0 | 0 declaration records; test use/execution remains unknown |
| Cargo.lock package entry | 1 | 1 | 0 | Exact migration entry mapped to REL-028; resolver/build event remains unknown |
| Direct reverse Cargo declarations | 0 | 0 | 0 | Dynamic/non-workspace consumers remain unknown |
| Local source target files | 1 | 1 | 0 | Runtime reachability and invocation remain unknown |
| Named outside-Cargo literal references | 19 file hits | 6 files mapped to RELs, including `.sbom/cyclonedx-rust.json` via REL-022 | 13 enumerated reference/history/LOC files | Other repositories and dynamic references remain unknown |
| Broad workspace workflow/SBOM definitions | 4 files: 3 workflows and 1 aggregation script | 4 mapped to REL-015–017/022 | 0 | Trigger, package selection result, and artifact remain unknown |

The counts describe this bounded literal/static search, not population completeness. Broad workspace workflows are listed separately from package-specific references and are not claimed to have run.

Unknowns include compiler/resolved target graph, external callers, trigger history, data/provider state, actual owner/escalation, migration behavior, and cold review. Success requires atomic producer/consumer records, separate directions, and explicit limits. See [R08](REFERENCE.md#r08) and [M06](MAINTENANCE.md#m06).

[Back to start](#b01)
