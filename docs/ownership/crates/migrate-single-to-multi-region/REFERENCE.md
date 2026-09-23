---
schema: corelink-ownership/1.1
document: reference
package: migrate-single-to-multi-region
manifest: apps/migrate-single-to-multi-region/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-migrate-single-to-multi-region-static-20260921
---

# migrate-single-to-multi-region — ownership reference

This is a SOURCE-only account of one declared binary and its local control flow. The manifest description advertises tenant data movement; the inspected source instead contains hard-coded fixtures, an in-memory audit sink, and rollback instructions printed for an operator. No migration mode was invoked and no provider behavior is established.

[Identity](#r01) · [Ownership boundary](#r02) · [Source map](#r03) ·
[Contracts](#r04) · [Invariants](#r05) · [Configuration](#r06) ·
[Failures](#r07) · [Evidence limits](#r08).

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-migrate-single-to-multi-region/SKILL.md#s01) · [Wave 015 route](../../WAVE_015_PLAN.md).

<a id="r01"></a>
## R01 — Identity

| Field | Source-defined value |
|---|---|
| Package / manifest | migrate-single-to-multi-region / [Cargo.toml](../../../../apps/migrate-single-to-multi-region/Cargo.toml) |
| Source inspected | [src/main.rs](../../../../apps/migrate-single-to-multi-region/src/main.rs); resolver context is [Cargo.lock](../../../../Cargo.lock) |
| Target | One binary, migrate-single-to-multi-region, at src/main.rs; no other target file is present in the package tree. |
| First-party normal dependency | corelink-region; the only first-party dependency declared by this manifest. |
| Other normal dependencies | anyhow, serde, serde_json, uuid with v7 and serde features, tokio. These are declarations, not resolved-build evidence. |
| Development dependencies | proptest and tokio-test. No package-local test source or explicit test target was found. |
| Package features / target conditions | No package [features] table or target-specific dependency block is declared. The actual resolved feature graph and supported compiler targets are unknown. |
| Lockfile context | The source pin has a `migrate-single-to-multi-region` lock entry; integration baseline `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540` differs from source pin only by removal of `regex` from the unrelated `corelink-ops` lock entry. The migration package lock entry is unchanged; no lock resolution was run. |
| Inherited values | Version, edition, rust-version, license, publish, and workspace lints are inherited; effective values were not resolved here. |

The package is a CLI-shaped application source with local reporting and mode text. It is not evidence of a working tenant migration service, production binary, or deployed operator tool.

<a id="r02"></a>
## R02 — Ownership boundary

| Surface | Implementation owner | Contract owner | Composition / runtime / review |
|---|---|---|---|
| Args, main, mode functions, report formatting, rollback text | This package, in src/main.rs | Private source shape | No invocation or runtime owner observed. |
| Region, MigrationReport, TenantMigrationResult, audit record and sink interfaces | Imported by this package; implementation is outside this package | corelink-region; see its [reference](../corelink-region/REFERENCE.md#r01) | A Rust import does not establish composition or execution. |
| D1, R2, Terraform, tenant data and operator workflow | Not implemented by the inspected main.rs | External contracts and operational owner are not identified here | RB-region contains procedural text but does not prove authorization, provider state, or a verified escalation route. |
| Independent review | Not owned by the author | Reviewer remains unassigned in this artifact | Cold review of final bytes is pending. |

The root [workspace manifest](../../../../Cargo.toml) includes this package. Search found no other Cargo manifest declaring a dependency on it. The canonical-concept route remains the [Wave 015 plan](../../WAVE_015_PLAN.md); its ADR seed is indexed in the [OKF concept manifest](../../../internal/okf-wiki/concept-manifest.yaml). The [knowledge ADR](../../../knowledge/adr/adr-s14-001-multi-region-terraform-module.md) is not revalidated here and does not override source evidence.

<a id="r03"></a>
## R03 — Source map

| Path / symbol | Static responsibility | Boundary |
|---|---|---|
| Cargo.toml | Package, one binary, normal and development dependency declarations, inherited settings. | Manifest text only. |
| Cargo.lock | Pinned resolver snapshot for the workspace, including this package's declared dependency entry. | Lockfile text only; no resolution or build was run. |
| Args and Args::parse | Defaults, recognized flags, target-region parsing, optional tenant filter, and unknown-flag behavior. | Local parser; no argv instance was observed. |
| main | Async entrypoint and branch precedence: rollback, then execute, otherwise dry-run. | Source branch only. |
| run_dry_run | Builds a report from three hard-coded tenant/count tuples, estimates duration, and prints text plus JSON. | Local fixture; no D1 query or R2 inventory. |
| run_execute | Builds a report from two hard-coded tuples and an InMemoryRegionAuditSink; constructs a literal hash-shaped string and marks local results verified. | In-memory/source values; no D1/R2 operation or computed hash. |
| run_rollback | Prints Terraform, D1 PITR, and R2 restore directions and says each step is manual. | Instructions only; no provider call or state restore. |

The file contains no local module tree, test module, network client, database client, storage adapter, environment-variable read, or external audit transport. This inventory is limited to the package manifest and its sole source file.

<a id="r04"></a>
## R04 — Source-visible contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005).

<a id="api-001"></a>
### API-001 — Argument parser result

**Symbols:** `Args::parse() -> Result<Args, anyhow::Error>`; input is process argv. **Preconditions:** any argv, including unknown flags or missing values. **Postconditions:** defaults are dry-run/`wnam`; recognized flags mutate local fields; unknown flags are ignored. **Errors:** invalid `Region::from_str`; no argv result was observed. **Effects:** no external I/O. **Compatibility:** flag names/defaults are caller surface. **Links:** INV-001/002; REL-001/002/009/010. **Evidence:** `src/main.rs:40-93`, SOURCE. [Index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Mode dispatch precedence

**Symbols:** `main()`, `run_rollback`, `run_execute`, `run_dry_run`; input is parsed `Args`. **Precondition:** `Args::parse` succeeds. **Postcondition:** rollback wins, otherwise execute, otherwise dry-run. **Errors:** selected function errors propagate; no invocation occurred. **Effects:** branch-local stdout/report/sink only. **Compatibility:** precedence and ignored `dry_run` field shape caller behavior. **Links:** INV-001; REL-009/011/012/013. **Evidence:** `src/main.rs:95-107`, SOURCE. [Index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Dry-run report construction

**Symbols:** `run_dry_run(&Args) -> anyhow::Result<()>`; input is target and optional filter. **Precondition:** selected dry-run branch. **Postcondition:** three fixtures are added; filter uses unanchored `contains(trim_end_matches('*'))`, so empty/`*` matches all and nonmatches become `SkippedFiltered`; duration includes every tuple. **Errors:** serialization can propagate; clock errors are suppressed to `0` by `unwrap_or(0)`. **Effects:** local report and stdout/JSON only. **Compatibility:** filter syntax and output fields are caller-visible. **Links:** INV-003; REL-004/011/024. **Evidence:** `src/main.rs:110-195`, SOURCE; no D1 inventory/output observed. [Index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Execute report and audit seam

**Symbols:** `run_execute(&Args) -> anyhow::Result<()>`; input is target plus optional filter. **Precondition:** selected execute branch. **Postcondition:** two fixtures are reported as `Migrate`; audit records use `hash_<fixture-id>`, `hash_verified=true`, and an in-memory sink. The filter is copied into `MigrationReport::new_execute` metadata but is not applied to fixture selection. **Errors:** sink emit can propagate; clock errors are suppressed to `0` by `unwrap_or(0)`. **Effects:** process-local report/sink/stdout only. **Compatibility:** report/audit fields and order matter. **Links:** INV-004; REL-012/024/025. **Evidence:** `src/main.rs:198-283`, SOURCE; no D1/R2/hash computation/external sink/execution observed. [Index](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Rollback output contract

**Symbols:** `run_rollback(&Args) -> anyhow::Result<()>`; input is parsed `Args`. **Precondition:** rollback selected by `main`. **Postcondition:** prints manual Terraform, D1 PITR, and R2 restore directions, then returns `Ok(())`. **Errors/effects:** no provider call or state effect is present; stdout only. **Compatibility:** text and rollback flag are prospective operator surface. **Links:** INV-005; REL-013/020/023. **Evidence:** `src/main.rs:285-304`, SOURCE; runbook is documentary and no rollback ran. [Index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Five falsifiable source invariants

| ID | Predicate | Enforcement point | Violation | Test/verification | Current state |
|---|---|---|---|---|---|
| INV-001 | Parser defaults to dry-run/`wnam`; dispatch selects rollback before execute. | `main.rs:43-47,99-105` assignments and `if` chain. | Default, assignment, or precedence changes. | SOURCE comparison of parser and dispatch; no argv test executed. | SOURCE holds; runtime unknown. |
| INV-002 | An unmatched argv arm performs no error or state mutation. | `main.rs:51-79`, wildcard `_ => {}`. | Unknown flag returns an error or changes a field. | SOURCE inspection; no parser invocation. | SOURCE holds; runtime unknown. |
| INV-003 | Dry-run visits three fixed tuples and computes each estimate as `rows + blobs * 10`. | `main.rs:130-164`, tuple loop and sum. | Query/source or formula changes, or tuple count differs. | SOURCE inspection and arithmetic trace; no execution. | SOURCE holds for pinned fixture; D1 behavior unknown. |
| INV-004 | Execute visits two fixed tuples, emits to `InMemoryRegionAuditSink`, and uses literal `hash_` plus ID with `hash_verified=true`. | `main.rs:211-253`, sink construction and result fields. | Provider sink, computed hash, filter selection, or fixture mapping replaces this shape. | SOURCE inspection; no mode invocation. | SOURCE holds; external execution unknown. |
| INV-005 | Rollback only prints manual Terraform/D1/R2 directions and returns `Ok(())`. | `main.rs:285-304`, print statements and return. | Provider/state call or non-`Ok` return is added. | SOURCE inspection against runbook; no rollback. | SOURCE holds; recovery outcome unknown. |

**State and flow:** argv → `Args::parse` → `main` precedence → one local mode → report/sink/stdout. State is process-local `Args`, report, and in-memory audit vector; no package-local durable state is shown. The flow is static SOURCE only and does not prove compile-time reachability, provider wiring, idempotence, migration, audit delivery, hash correctness, or recovery.

<a id="r06"></a>
## R06 — Configuration and targets

The manifest declares one binary and no package feature table. Args::parse reads process arguments; no environment variables are read. The target string defaults to wnam and is passed to corelink_region::region::Region::from_str. The imported source declares six Region variants, but region availability in any environment is unknown. Dependency feature declarations are uuid v7 and serde; they are not package-level selectable features. No Cargo resolution or target build was performed.

<a id="r07"></a>
## R07 — Local failures and contradiction handling

An invalid target region propagates from Region::from_str through Args::parse and main. Serialization and audit-sink errors are mapped to anyhow errors in the local source. SystemTime errors are suppressed to timestamp `0` by `unwrap_or(0)`. Unknown flags are ignored. The execute failure-report branch checks MigrationReport::has_failures, but the inspected fixed tuple path only adds Migrate results; the audit error propagates before that result is added. No failure was induced or observed.

The manifest description and surrounding runbook advertise D1/R2 movement, hash verification, idempotence, and rollback. Those are intended or documentary claims, not implementation evidence in this source snapshot. Record the contradiction; do not select the stronger claim without separate code and runtime evidence.

<a id="r08"></a>
## R08 — Evidence, axioms, and unknowns

Evidence class is SOURCE: root Cargo.toml, the package Cargo.toml, Cargo.lock, apps/migrate-single-to-multi-region/src/main.rs, and the static outside-Cargo paths named in [B02](BLAST_RADIUS.md#b02), all anchored to 1177dad2ca2a9f21c29b5a118aa7944b77147798. The integration baseline is separate from this source pin.

The assigned integration baseline ab7137cd178f0e6cb282f3e944f9f7b58d0f5540 was rechecked against the pin for root Cargo.toml, the package manifest, and src/main.rs; no content drift was found in those paths. Cargo.lock does drift: one `regex` entry was removed from the unrelated `corelink-ops` package dependency list, while this package's lock entry is unchanged. This is a lockfile-baseline fact, not proof of a fresh dependency resolution.

Five ownership axioms remain binding: Cargo package.name is authoritative; a test declaration is not a test execution; dependency/import is not runtime reachability; fake/in-memory behavior is not provider behavior; advertised migration modes are not permission to invoke them.

| Material claim | Implemented | Wired | Runtime verified | Evidence / scope |
|---|---|---|---|---|
| CLI parser, three mode branches, local reports | yes | unknown | unknown | `src/main.rs` SOURCE only |
| Imported region/report/audit contracts | no — implementation is owned by `corelink-region`; this package only imports/uses them | unknown | unknown | `corelink-region` R04/R05/R07 and local SOURCE imports; no selected graph |
| D1/R2 tenant migration and computed hash | no implementation in inspected body | unknown | unknown | comments/manifest only; no provider calls |
| Durable audit emission and rollback | no implementation in inspected body | unknown | unknown | in-memory sink/manual text only |
| Shipped image/binary reachability | unknown | unknown | unknown | Docker recipe omits binary; no target build/artifact |

Unknown: resolved dependency graph and features, compiler target support, test execution, binary invocation, caller identity, live tenant list, row/blob counts, data movement, transaction semantics, computed hash, audit persistence/delivery, provider resources, rollback outcome, compatibility of previously moved data, runtime/deployment reachability, actual operation owner, and independent cold review. Continue to [relations](BLAST_RADIUS.md#b01) and [procedures](MAINTENANCE.md#m01). Quality means no source comment, fixture, manifest description, or runbook command is promoted to an observed outcome.

[Back to start](#r01)
