---
name: own-corelink-cas
description: Review source-grounded ownership changes to the corelink-cas hybrid CAS facade without transferring ownership through re-exports.
metadata:
  evidence-set: cas-static-graph-20260920
  source-commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
  manifest: crates/corelink-cas/Cargo.toml
  package: corelink-cas
  scope: corelink-cas
  evidence: static-source-only
  profile: H
---

# Own corelink-cas

Use this skill for `crates/corelink-cas`: a hybrid CAS facade with six in-tree
implementations, four dependency-owned CAS re-export modules, and two
worker-owned re-export surfaces. It records source/static evidence only. It
does not establish consumer migration, selected targets, R2/worker/storage
operation, deployment, or independent review.

[Baseline](#s01) · [Classification](#s02) · [Local modules](#s03) · [Facade](#s04) · [Composition](#s05) · [Impact](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the source baseline

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A CAS change or diff arrives | Record revision, manifest, `src/lib.rs`, and every changed module before assigning ownership | `crates/corelink-cas/Cargo.toml`; `src/lib.rs`; changed-path inventory | Baseline, package identity, or path set is unavailable or differs |

<a id="s02"></a>
## S02 — Classify the public path before editing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A `corelink_cas::*` path changes | Classify it as local implementation, dependency re-export, or worker re-export; preserve the defining owner separately from the canonical path | `src/lib.rs`; route module; manifest dependency | A re-export is treated as transferred implementation ownership or as proof of consumer migration |

<a id="s03"></a>
## S03 — Guard locally implemented module families

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `chunker`, `dedup`, `edge`, `lru_tracker`, `manifest`, or `multipart_schema` changes | Trace its top-level module, public re-exports, direct submodules, fixtures, and relevant integration tests; retain its local contract boundary | `src/{chunker,dedup,edge,lru_tracker,manifest,multipart_schema}.rs` and their in-tree trees; `tests/` | The requested conclusion requires live D1/R2/Worker behavior, an external provider, or a historical-data compatibility decision |

<a id="s04"></a>
## S04 — Preserve façade ownership

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `eviction`, `r2_multipart`, `meta`, or `handler` changes | Trace its forwarding file and named dependency; change the provider contract with that provider’s owner, not by assuming this facade implements it | `src/{eviction,r2_multipart,meta,handler}.rs`; `Cargo.toml` | A glob or module re-export is used to certify implementation, compatibility, operation, or migration |

<a id="s05"></a>
## S05 — Escalate composition and runtime boundaries

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `r2_storage`, `cache`, storage selection, or request handling is implicated | Keep `corelink-worker` as the visible implementation owner for forwarded worker paths; identify the composition root and remote operator needed for the requested result | `src/lib.rs`; `Cargo.toml`; worker source/contract supplied by its owner | Target selection, R2 reachability, credentials, storage mutation, worker execution, or deployment evidence is required |

<a id="s06"></a>
## S06 — Bound static impact analysis

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public path, manifest edge, feature, error, schema, or invariant changes | Review the canonical path, defining module/provider, tests/examples, and any concrete static consumer edge separately recorded | `src/lib.rs`; route module; manifest; `tests/`; `examples/` | A complete reverse graph, feature resolution, selected target, or runtime caller is needed but lacks direct evidence |

<a id="s07"></a>
## S07 — Hand off with falsifiable limits

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static ownership work is ready | Report baseline, local/facade/worker classification, changed symbols, checks actually run, and unknowns | [Reference](../../../docs/ownership/crates/corelink-cas/REFERENCE.md#r01); [Blast radius](../../../docs/ownership/crates/corelink-cas/BLAST_RADIUS.md#b01); [Maintenance](../../../docs/ownership/crates/corelink-cas/MAINTENANCE.md#m01) | Do not call document checks a Cargo build, test result, runtime proof, consumer migration, or cold review |
