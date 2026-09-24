---
name: own-corelink-adapters-cloud
description: >-
  Own the canonical cloud-adapter import facade in corelink-adapters-cloud.
  Use for public re-export-path changes; do not use for provider implementation,
  target selection, build, migration, or runtime operation.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-adapters-cloud"
  manifest: "crates/corelink-adapters-cloud/Cargo.toml"
  source-commit: "1c99a5b8ce1db7b622db7811512432a5d070bbbe"
  evidence-set: "w005-cloud-source-static-20260920"
---

# Ownership — corelink-adapters-cloud

Candidate ownership for the source-defined canonical import facade. It is not approval of a provider integration, target outcome, build, migration, deployment, or runtime operation.

[Trigger](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) · [Flow](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Use this skill when | Route elsewhere when |
|---|---|
| Changing the public `cf`, `clerk`, `stripe`, `statuspage`, or `slack` facade path | Changing an upstream implementation or its public contract |
| Reviewing a re-export addition, removal, rename, or documentation claim | Selecting a target, compiling, migrating consumers, or operating a provider |
| Classifying static facade compatibility risk | Establishing a reverse-consumer inventory or runtime reachability |

<a id="s02"></a>
## S02 — Territory and authority

This package owns its five local facade modules and root module declarations: `src/{lib,cf,clerk,stripe,statuspage,slack}.rs`. It owns the canonical paths `corelink_adapters_cloud::{cf,clerk,stripe,statuspage,slack}`.

The implementation owners are the direct re-export targets: `corelink-cf-bindings`, `corelink-clerk-cf`, `corelink-stripe-real`, `corelink-statuspage-real`, and `corelink-slack-real`. They retain their source implementation and contract authority. Target selection belongs to the relevant build/workspace and consumer boundary, not this facade. This skill grants no authority for Cloudflare, HTTPS providers, credentials, builds, migration, deployment, publication, or runtime operation.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| Which canonical path maps to which implementation package? | [R03–R04](../../../docs/ownership/crates/corelink-adapters-cloud/REFERENCE.md#r03) |
| What relation and compatibility impact applies? | [B02–B05](../../../docs/ownership/crates/corelink-adapters-cloud/BLAST_RADIUS.md#b02) |
| What source/static procedure is safe? | [M02–M05](../../../docs/ownership/crates/corelink-adapters-cloud/MAINTENANCE.md#m02) |
| What remains unresolved? | [R08](../../../docs/ownership/crates/corelink-adapters-cloud/REFERENCE.md#r08) and [B06](../../../docs/ownership/crates/corelink-adapters-cloud/BLAST_RADIUS.md#b06) |

<a id="s04"></a>
## S04 — Decision form

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A facade module gains, loses, or renames an upstream re-export | Trace API-001 and the matching REL record | Local `pub use` path and upstream manifest identity | Upstream public contract or all consumers are unresolved |
| A root module name changes | Treat the canonical import path as compatibility-sensitive | `src/lib.rs` declaration and API-001 | Migration or reverse-compatibility proof is requested |
| An implementation behavior is requested | Route to its implementation package | Direct dependency/re-export boundary | This facade has no implementation evidence for it |
| A target, provider, or runtime claim is requested | Obtain the appropriate owner and evidence class | Explicit external evidence, if separately authorized | Only SOURCE evidence is available here |

<a id="s05"></a>
## S05 — Work flow

1. Confirm the manifest, fixed source baseline, and five local facade modules.
2. Map each canonical module to its direct `pub use` target.
3. Separate the public path owner from the implementation owner and from target selection.
4. Record additions, removals, and renames as source compatibility risk until consumers are independently resolved.
5. Run only authorized documentary checks and retain their exit status.
6. Request independent review for changed artifact bytes and unresolved compatibility.

<a id="s06"></a>
## S06 — Stop conditions

Stop when the request requires a wasm build, target result, Cloudflare action, HTTPS/provider call, credential, migration, deployment, publication, or runtime assertion. Stop when a re-export is used to infer that an upstream type exists on a selected target or that a consumer imports it. A manifest dependency and `pub use` statement establish source wiring only.

<a id="s07"></a>
## S07 — Handoff and definition of done

Report baseline, exact changed paths, affected canonical paths, implementation-owner boundary, source/static evidence, commands and exit status, skipped checks, and explicit unknowns. Success is a bounded facade change with each public path mapped to its direct implementation package. Completeness requires all four ownership artifacts, structural validation, and a scope-only diff. The author’s checks are not cold review and do not certify target selection, build, provider operation, migration, or runtime behavior.

[Back to start](#s01)
