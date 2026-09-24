---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-tenant-isolation
manifest: tests/e2e-tenant-isolation/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-tenant-isolation-static-1177dad2c
---

# e2e-tenant-isolation — blast radius

Static package and consumer relationships only. The harness uses real tenant-path, audit, and BYOK APIs behind local fakes; it is not a production composition root and has no observed runtime reachability.

[Scope](#b01) · [Method](#b02) · [Atomic relations](#b03) · [Propagation](#b04) ·
[Change map](#b05) · [Coverage and unknowns](#b06).

<a id="b01"></a>
## B01 — Scope

This map covers the pinned package manifest, ten Rust sources, and literal external references in the pinned repository. It distinguishes declared Cargo edges, source calls, static verification scripts, and generic workspace workflows. No resolved graph, workflow result, provider call, deployment, or production consequence is claimed.

<a id="b02"></a>
## B02 — Census method and populations

| Population | Static finding at 1177dad | Boundary |
|---|---|---|
| Cargo identity | Workspace member; one library and one explicit adversarial target; autotests=false | Manifest declaration, not Cargo-resolved selection |
| Direct dependencies | Three first-party normal edges; six other normal dependencies; tokio dev dependency | No resolved versions/features or inverse Cargo graph |
| Local source | Eight library Rust files; two test Rust files; 25 named scenarios | Source census, not an execution result |
| Exact outside-package literals | 12 tracked paths match e2e-tenant-isolation; 10 match tests/e2e-tenant-isolation; 4 match e2e_tenant_isolation | Exact text census, not proof of complete dynamic/generated references |
| Verifier scripts | Five scripts reference test files or import markers | REL-007/008 remain unexecuted source-marker mismatches; REL-009 was run and is explicitly `BROKEN` at B297, with no Rust result implied |
| Workflow source / endpoint | No package-specific workflow or deployed endpoint names this package; `cas_foundation.yml` and `workspace-lint.yml` are generic workspace gates whose commands include declared members/targets | A workflow command is not an observed run, target selection, endpoint, or runtime result |
| Runtime and providers | Not observed | Unknown; no reachability or provider claim |

The 12 outside-package literal paths are: root Cargo.toml, Cargo.lock, .sbom/cyclonedx-rust.json, BACKLOG.md, the five scripts named in REL-005–009, reports/audits/2026-08-07-external-hard-audit.md, reports/b326-loc-cap-baseline.txt, and specs/_audits/sealed/2026-05-15-ga-readiness-consolidation-wave-13-17.md.

These are workspace/build metadata, static checks, backlog, or documentation—not additional Rust consumers. Generic INV-TENANT-ISOLATION hits and generic type names are excluded because they do not identify this package.

Static workflow anchors: `cas_foundation.yml` has only `workflow_dispatch` active in its `on` block (the schedule is commented); job `workspace-build-test` declares `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo doc --no-deps --workspace` at lines 157–169.

`workspace-lint.yml` declares a main push lane on Rust/manifest/lock/toolchain changes, a `pull_request` self-test only when `.github/workflows/workspace-lint.yml` itself changes, and `workflow_dispatch`; job `clippy-workspace` runs `cargo clippy --workspace --all-targets --message-format=short -- -D warnings` at lines 57–73. Neither file names this package, its `adversarial` endpoint, or a deployed service endpoint. Inclusion follows workspace/target declaration only if the workflow selects it; trigger and results remain unobserved. The pull-request self-test is not coverage for a package source change.

<a id="b03"></a>
## B03 — Atomic relations

| ID | Boundary | Dependency direction | Data direction | Impact direction |
|---|---|---|---|---|
| [REL-001](#rel-001) | Tenant prefix | harness → tenant-path | fixture IDs/TDK → derivation → prefix | tenant-path API → harness compile/assertions |
| [REL-002](#rel-002) | Audit capture | harness → corelink-audit | fake deny → event/emitter/capture | audit API → fake and assertions |
| [REL-003](#rel-003) | BYOK AAD | harness → corelink-byok | test context/envelope → encrypt/decrypt | BYOK API → adversarial target |
| [REL-004](#rel-004) | Package library / target | adversarial → e2e library | fixture/fake values → scenario assertions | library source → target assertions |
| [REL-005](#rel-005) | B126 source verifier | verifier → test sources | source text → split/test markers | source reshape → verifier predicate |
| [REL-006](#rel-006) | B270–281 verifier | verifier → fake source files | imports/markers → textual contract | source import change → verifier predicate |
| [REL-007](#rel-007) | B291–293 verifier | verifier → adversarial_tail | source text → required/forbidden import | either source/script change → mismatch risk |
| [REL-008](#rel-008) | B294–296 verifier | verifier → adversarial_tail | source text → required/forbidden import | either source/script change → mismatch risk |
| [REL-009](#rel-009) | B297–312 verifier | verifier → adversarial.rs | parsed imports → named check | source/import change → verifier predicate |
| [REL-010](#rel-010) | CAS Foundation CI | workflow → workspace members/targets | Cargo command selects workspace targets | target change → future workflow result |
| [REL-011](#rel-011) | workspace lint | workflow → workspace members/targets | Cargo command selects workspace targets | target/source change → future lint result |

<a id="rel-001"></a>
### REL-001 — Tenant-prefix derivation

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-tenant-path-001.\
**Dependency / data / impact:** harness→tenant-path; UUID plus shared fixture TDK→derive_prefix→TenantPrefix; upstream type/derivation changes→possible harness compile/assertion change.\
**Surface / activation:** normal manifest dependency; tenants.rs calls derive_prefix and stores use TenantPrefix.\

**State / failure / boundary:** test fixture only; prefix/API mismatch is local. No request routing or production prefix use is shown.\
**Validation / coordination:** scenarios S01–S03, S14, S22 are declared; not run. Coordinate with tenant-path owner. Cargo.toml lines 18–20; tenants.rs; fakes/stores.rs.\ [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Audit event and capture

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-audit-001.\
**Dependency / data / impact:** harness→corelink-audit; fake deny→AuthEvent→InMemoryEmitter, then AuditAttempt capture; audit API changes→local compile/assertions.\
**Surface / activation:** normal manifest edge; AuditCapture::record_deny in fake source.\

**State / failure / boundary:** emitter and attempts are process-local; emit error stops before append. Durable D1/outbox behavior is not owned here.\
**Validation / coordination:** scenario assertions are source only; not run. Coordinate with corelink-audit owner. Cargo.toml lines 18–20; fakes/foundation.rs lines 124–178, 280–305.\ [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — BYOK envelope AAD

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-byok-001.\
**Dependency / data / impact:** harness→corelink-byok; test tenant/blob context→EnvelopeEncryptor encrypt/decrypt; BYOK API changes→target compile/assertions.\

**Surface / activation:** normal manifest edge used in S04/S05; StubKms is local and returns byte vectors.\

**State / failure / boundary:** wrong AAD yields AadMismatch in source assertions; no remote KMS, credential, or provider selection is shown.\
**Validation / coordination:** S04/S05 source only; S04 has no audit assertion, S05 records after decrypt returns. Coordinate with corelink-byok owner. Cargo.toml lines 20–21; adversarial.rs lines 49–91, 229–299.\ [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Library exports to adversarial target

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-library-target-001.\
**Dependency / data / impact:** adversarial target→e2e_tenant_isolation library; fixture/fake values→scenario calls and assertions; library export change→target compile or assertion impact.\

**Surface / activation:** explicit test target imports public harness types; adversarial.rs includes adversarial_tail as a module.\

**State / failure / boundary:** all state is test harness state; declaration/import does not prove Cargo selection or CI execution.\
**Validation / coordination:** one declared target; no run. Package and target owners are the same source unit. Manifest lines 14–15, 31–33; lib.rs lines 78–87; adversarial.rs lines 41–46, 802.\ [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — B126 source-shape verifier

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-verifier-b126-001.\
**Dependency / data / impact:** verify_b126_t3_refactor.py→adversarial.rs and adversarial_tail.rs; it reads source text and declares split/test markers; source layout change→possible verifier failure.\
**Surface / activation:** static Python verifier entry; line 77–84 names paths/scenario markers.\

**State / failure / boundary:** reads repository files; no production state. It is not a Cargo edge and was not run.\
**Validation / coordination:** compare the specified markers to pinned source before relying on it; route its own maintenance to its verified owner. scripts/verify_b126_t3_refactor.py.\ [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — B270–281 fake-source verifier

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-verifier-b270-001.\
**Dependency / data / impact:** verify_b270_b281_bundle_repairs.py→foundation.rs, extended.rs, stores.rs; it checks literal imports/markers; source import change→possible verifier result change.\
**Surface / activation:** CONTRACTS entries at lines 67, 99–100.\

**State / failure / boundary:** static source strings only; no fake execution or provider effect. Script was not run.\
**Validation / coordination:** recheck each literal and its purpose before changing either side. Owner of script/check is not identified here. scripts/verify_b270_b281_bundle_repairs.py.\ [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — B291–293 import verifier

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-verifier-b291-001.\
**Dependency / data / impact:** verify_b291_b293_bundle_residuals.py→adversarial_tail.rs; requires use e2e_tenant_isolation::* and forbids use super::*; source/script difference→check outcome.\
**Surface / activation:** literal contract at verifier line 9.\


**State / failure / boundary:** pinned tail line 15 currently has use super::*; the expectation conflicts with this snapshot and may be stale or inverted. It was not executed; status unresolved.\
**Validation / coordination:** stop before relying on or rewriting either side; ask repository lead to identify the verifier owner. No outcome inferred.\ [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — B294–296 import verifier

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-verifier-b294-001.\
**Dependency / data / impact:** verify_b294_b296_bundle_residuals.py→adversarial_tail.rs; requires use e2e_tenant_isolation::* and forbids use super::*; source/script difference→check outcome.\
**Surface / activation:** literal contract at verifier line 7.\


**State / failure / boundary:** pinned tail line 15 currently has use super::*; the expectation conflicts with this snapshot and may be stale or inverted. It was not executed; status unresolved.\
**Validation / coordination:** stop before relying on or rewriting either side; ask repository lead to identify the verifier owner. No outcome inferred.\ [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — B297–312 adversarial-import verifier

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-verifier-b297-001.\
**Dependency / data / impact:** verify_b297_b312_bundle_residuals.py→adversarial.rs; parses e2e_tenant_isolation imports; changed namespace/import→possible verifier result.\
**Surface / activation:** ADVERSARIAL path and import check at lines 21, 194–204.\

**State / failure / boundary:** static parser inputs; no target run or runtime assertion. The read-only verifier run is `BROKEN` at B297 because its expected import set contradicts the pinned source.\
**Validation / coordination:** retain the exact output below; inspect source and verifier together, then route to the script owner.\
```text
B-297..B-312 BROKEN: B-297 imports: expected exact 12-symbol set, got ['AuditCapture', 'AuditChain', 'AuditQueryEngine', 'CasStore', 'CmkRotationLedger', 'ConstantTimeAuthProbe', 'DenyKind', 'DsrIntake', 'HierarchicalQuotaStore', 'IdempotencyStore', 'KvReplicatedPatStore', 'MultipartBroker', 'PatRevokeLedger', 'PatStore', 'QuotaStore', 'RateLimiter', 'RegionRouter', 'StripeWebhookLedger', 'TenantCtx']
```
[Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — CAS Foundation workspace workflow

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-cas-foundation-ci-001.\
**Dependency / data / impact:** cas_foundation.yml→workspace members/targets; workspace Cargo commands select declared targets; source/target change→possible future result.\
**Surface / activation:** only `workflow_dispatch` is active; schedule and pull_request are absent/commented. Lines 160–169 declare clippy, test, and `cargo doc --no-deps --workspace`, selecting workspace documentation targets without `--all-targets`.\


**State / failure / boundary:** package target is eligible by workspace membership; no workflow was dispatched and no result is claimed.\
**Validation / coordination:** static config; workflow owner unknown. Cargo.toml line 312; workflow file.\
**Endpoint boundary:** generic workspace job; no package or production endpoint named/observed.\ [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — workspace lint workflow

**Identity:** repo:1232040291:boundary:e2e-tenant-isolation-workspace-lint-ci-001.\
**Dependency / data / impact:** workspace-lint.yml→workspace Rust targets; cargo clippy --workspace --all-targets reads package/test targets; source change→possible future lint result.\
**Surface / activation:** push to main on Rust/manifest/lock/toolchain paths, `pull_request` only when `.github/workflows/workspace-lint.yml` changes, and workflow_dispatch; lines 27–43, 72–73. The pull-request lane is a workflow self-test, not a package-source lane.\


**State / failure / boundary:** lint declaration only; no dispatch, check run, or result observed. Does not execute test assertions.\
**Validation / coordination:** static config only; owner/actual selection remain unknown. Root workspace membership and explicit target; workflow file.\
**Endpoint boundary:** generic workspace lint job only; no package-specific test endpoint or production route is named or observed.\ [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagation and composition

An upstream change to tenant-path, audit, or BYOK can alter a declared API the harness imports and may prevent the target from compiling or change a source predicate. A local fake/assertion change changes the harness's evidence surface; it does not alter a live tenant store.

The explicit adversarial target is a member of the root workspace and is named by generic all-target workflow commands, but no CI run was observed. No edge from this harness to a running service, provider, durable database, or customer storage is established.

The five verifier scripts read Rust source as text. Two assert an import form opposite to the pinned tail module; this source/configuration mismatch is unresolved. The B297–312 verifier was executed read-only and is `BROKEN` because its expected 12-symbol set disagrees with the pinned 19-symbol import.

A source or script change could alter these outcomes; none is a Cargo or runtime result. Backlog, report, SBOM, lockfile, and spec references are inventory/evidence records; they do not execute the harness.

<a id="b05"></a>
## B05 — Change to impact to static evidence

| Change | Affected contracts/relations | Static re-read and limit |
|---|---|---|
| Rename package, library, or test target | INV-005; REL-004, REL-010/011 | Manifest, root membership, exact imports, and workflow selectors; no Cargo resolution |
| Change tenant fixture/prefix use | API-001; INV-001; REL-001 | tenants.rs, store keying, affected S01–03/S14/S22 source; no runtime reachability |
| Change fake rejection or capture order | API-003; INV-002/004; REL-002 | foundation/stores and each matching assertion; no durable audit claim |
| Change BYOK scenario/context | INV-003; REL-003 | S04/S05 call and caller-boundary audit behavior; no KMS operation |
| Change adversarial scenario or tail module | INV-005; REL-004/005/007/008/009 | all scenario IDs, inclusion/imports, verifier literals; preserve the unresolved mismatch until owner review |
| Change test assertion without fake implementation | Scenario evidence only | Identify which predicate becomes undetected; do not describe it as production behavior |

<a id="b06"></a>
## B06 — Coverage and unknowns

### Cross-package relation reconciliation

The three first-party edges now use the stable identities below, mirrored by
their provider-package BLAST records. The identities bind only the stated
source-level API boundary and ownership split; they do not certify Cargo
resolution, execution, production composition, or runtime reachability.

| Local relation | Qualified endpoints and shared facts | Ownership split / runtime limit |
|---|---|---|
| REL-001 | `repo:1232040291:boundary:e2e-tenant-isolation-tenant-path-001`; `tests/e2e-tenant-isolation` → `corelink-tenant-path::{derive_prefix,TenantDerivationKey,TenantPrefix}`; fixture UUID/TDK inputs → `TenantPrefix` | API/algorithm owner: `corelink-tenant-path`; harness owns fixed fixtures/assertions; mirrored at `corelink-tenant-path` REL-010. Test source only; no service composition/runtime endpoint observed. |
| REL-002 | `repo:1232040291:boundary:e2e-tenant-isolation-audit-001`; `tests/e2e-tenant-isolation` → `corelink-audit::{AuthEvent,Emitter,InMemoryEmitter}`; fake deny → event/emitter → process-local capture | API/event owner: `corelink-audit`; harness owns fake capture/assertions; mirrored at `corelink-audit` B06. No durable sink, service composition, or runtime endpoint observed. |
| REL-003 | `repo:1232040291:boundary:e2e-tenant-isolation-byok-001`; `tests/e2e-tenant-isolation` → `corelink-byok::EnvelopeEncryptor` and related envelope/AAD types; test context/envelope → local encrypt/decrypt assertions | API/algorithm owner: `corelink-byok`; harness owns `StubKms` and assertions; mirrored at `corelink-byok` B05. No provider selection, service composition, or runtime endpoint observed. |

The corresponding peer records carry the same identity, producer/consumer,
contract owner, source boundary, and limits. Static shared identity is
reconciled; runtime reachability remains UNKNOWN and is not inferred from these
manifest edges or a re-export.

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Direct first-party Cargo edges | 3 | 3 | 0 | 0 declarations; resolution unknown |
| Package targets | 2 | 2 | 0 | 0 declarations; selection/run unknown |
| Local Rust source files | 10 | 10 | 0 | 0 in package tree |
| Exact outside-package literal paths | 12 | 12 | 0 | Dynamic/generated/semantic references not covered |
| Static verifier scripts | 5 | 5 | 0 | Execution/owner status unknown |
| Generic workspace CI workflows | 2 | 2 | 0 | Actual dispatch, commit inclusion, and results unknown |

The literal census is reproducible by searching the pinned tree for e2e-tenant-isolation, tests/e2e-tenant-isolation, and e2e_tenant_isolation, excluding the package directory. It does not prove no dynamic or generated consumer exists.

Exact identifier searches found no external Rust import of this crate; generic names such as TenantCtx or CasStore produce unrelated matches and are not promoted to relations. No resolved reverse graph or runtime census was performed.

Critical unknowns: (1) selected target/features and resolved dependency graph; (2) actual CI runs/results; (3) production auth/container composition; (4) durable D1/outbox, R2/KV, and provider behavior; (5) semantic correctness of verifier scripts, particularly REL-007/008. No self-approval is recorded.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01).
