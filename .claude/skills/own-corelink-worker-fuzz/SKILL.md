---
name: own-corelink-worker-fuzz
description: >-
  Use when changing the corelink-worker-fuzz Cargo package, either fuzz target,
  its direct dependencies, or its documented workflow boundary. Route oracle,
  impact, and bounded local validation; do not use for production R2,
  credentials, or corelink-worker API ownership.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-worker-fuzz"
  manifest: "crates/corelink-worker/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "worker-fuzz-source-20260921"
---

# Ownership — corelink-worker-fuzz

[Trigger](#s01) · [Authority](#s02) · [Read](#s03) · [Decisions](#s04) ·
[Flow](#s05) · [Stop](#s06) · [Output](#s07).

<a id="s01"></a>
## S01 — Trigger

| Use this skill when | Do not use this skill when |
|---|---|
| Editing `crates/corelink-worker/fuzz/Cargo.toml`, either target source, a package dependency, or the package's fuzz oracle | Changing `corelink-worker` public APIs, key policy, a Cloudflare R2 binding, deployment, credentials, or production data |
| Reviewing a package change that affects `r2_path` or `r2_put_get_roundtrip` | Editing only a generic workflow/shared fuzz campaign; route to its verified workflow owner |
| Updating this package's ownership artifacts against an exact source pin | Assuming the `r2_*` names prove remote R2 execution, CI success, or runtime reachability |

<a id="s02"></a>
## S02 — Territory and authority

**Implementation:** the independent `corelink-worker-fuzz` package has two source-declared fuzz bins. Named implementation owner is not verified.

**Harness contracts:** the package owns only its test oracles. `corelink-worker` owns the adapter contract; `corelink-hash` and `corelink-tenant-path` own their respective value/derivation contracts. See [reference](../../../docs/ownership/crates/corelink-worker-fuzz/REFERENCE.md#r02).

**Composition:** each harness composes the worker writer/reader with `InMemoryR2`. The CI workflows configure fuzz invocation; they do not prove it ran. Production composition is not established.

**Operation and review:** `.github/CODEOWNERS` has a default `@gmhelmold` review route, but its header says it is not a control and the sole owner is usually the author. It does not identify an independent reviewer, contract owner, workflow operator, or escalation route. Obtain actual assignments from the repository's authorized process; do not invent a person or team. This skill does not authorize external or production operations.

<a id="s03"></a>
## S03 — Read routing

| Question | Read directly |
|---|---|
| What does each binary do? | [Targets and oracles](../../../docs/ownership/crates/corelink-worker-fuzz/REFERENCE.md#r03) · [Harness contracts](../../../docs/ownership/crates/corelink-worker-fuzz/REFERENCE.md#r04) |
| What dependencies/workflows are affected? | [Direct relations](../../../docs/ownership/crates/corelink-worker-fuzz/BLAST_RADIUS.md#b03) · [Propagation](../../../docs/ownership/crates/corelink-worker-fuzz/BLAST_RADIUS.md#b04) |
| How do I validate or recover? | [Procedures](../../../docs/ownership/crates/corelink-worker-fuzz/MAINTENANCE.md#m02) · [Recovery](../../../docs/ownership/crates/corelink-worker-fuzz/MAINTENANCE.md#m05) |
| Which shared concepts apply? | [CAS/AC core](../../../docs/knowledge/crates/cas-ac-core.md) · [R2 CAS bucket](../../../docs/knowledge/storage/r2-cas-bucket.md) · [Tenant prefix](../../../docs/knowledge/adr/adr-0043-hmac-tenant-prefix-algorithm.md) |

Load only records related to the target or contract being changed. Keep implementation, contract, composition, operator, and review authority separate.

<a id="s04"></a>
## S04 — Decisions and invariants

| Condition | Action and evidence | Stop when |
|---|---|---|
| Fuzz target or oracle changes | Inspect the exact input layout, early returns, assertions, and fake backend in the target source; update `API`/`INV` and `REL` records | A new backend, secret source, or external effect lacks an owner and contract |
| Worker/hash/tenant-path API changes | Route the public contract to its source owner; document this package's affected oracle separately | Reachability, data compatibility, or contract ownership is unknown |
| Dependency/feature/workspace changes | Reconcile the package manifest, standalone workspace, lockfile, and affected workflow selection | A Cargo-resolved graph is needed but no authorized validation is available |
| Workflow timing/runner/cache changes | Review the active PR smoke, the dormant per-crate `fuzz-nightly` declaration, and the active root `nightly.yml` matrix; label configuration separately from run evidence | Operator, resource limit, or recovery path is unverified |

Preserve the five ownership invariants: do not disguise functional changes as documentation; this skill grants no external or production authority; never claim production behavior from source or fuzz intent; do not omit material relations to fit a size limit; never self-approve. Also keep the campaign boundaries explicit: Cargo package name is authoritative; target declaration is not execution; fuzz placement is not parent implementation ownership; source reachability is not runtime observation; fuzz input and unsafe boundary claims are package-specific.

<a id="s05"></a>
## S05 — Workflow

1. Confirm the manifest, exact source pin, integration baseline, and authorized change scope. 2. Read only the applicable target, its direct contracts, and affected `REL` cards. 3. Decide whether the task is static documentation, harness code, dependency, or workflow work; choose the matching [procedure](../../../docs/ownership/crates/corelink-worker-fuzz/MAINTENANCE.md#m02). 4. Before any authorized local fuzz run, isolate corpus/artifact output and set a recorded input and time budget; roundtrip body allocation occurs before the 5 MiB

check. 5. Record source, host/toolchain, command, result, crash inputs, and the gap between fake behavior and any external backend. 6. Update the four package artifacts affected by evidence, preserve source pin provenance, and submit final hashes for independent cold review.

<a id="s06"></a>
## S06 — Stop conditions

Stop if the source pin drifts, the target changes from `InMemoryR2` to an external adapter, an input/resource limit is unspecified, a finding contains unreviewed data, or a required operator/contract owner/reviewer cannot be verified. Route the question through the authorized project process; this evidence does not identify a person or escalation channel.

Do not equate declared targets or workflow YAML with execution. Do not use or inspect credentials, provider accounts, production, databases, or remote storage as part of package ownership work.

<a id="s07"></a>
## S07 — Evidence and handoff

Return the goal and source pin; baseline; target/API/INV/REL/PROC records touched; exact commands and results actually observed; whether behavior was only inspected, built, executed, deployed, or runtime-observed; resource and recovery state; unresolved owners/findings; four independent review states. Mark every unrun procedure as not executed. Never report a fuzz run, production R2 path, or review approval based on intent or source declaration.

[Back to trigger](#s01)
