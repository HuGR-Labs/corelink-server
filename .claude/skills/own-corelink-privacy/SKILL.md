---
name: own-corelink-privacy
description: >-
  Govern source-grounded changes to the corelink-privacy hybrid umbrella while
  keeping absorbed modules separate from external facade implementations.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-privacy"
  manifest: "crates/corelink-privacy/Cargo.toml"
  source-commit: "8b800acd3ffb042e5f68bedbb989415a5c5b0bbe"
  evidence-set: "privacy-source-static-20260920"
  profile: "H"
---

# Ownership — corelink-privacy

This H-profile guide is limited to checked-in source and static references at
the recorded revision. It classifies import paths; it does not establish
execution, deployment, or an independent review.

[Baseline](#s01) · [Classification](#s02) · [Local modules](#s03) ·
[Facades](#s04) · [Consumers](#s05) · [Stops](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Fix the source baseline

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Starting a change or reviewing a diff | Record the revision, manifest, `src/lib.rs`, and changed module family | `crates/corelink-privacy/{Cargo.toml,src/lib.rs}` | The revision, manifest, or path set differs from the record |

<a id="s02"></a>
## S02 — Classify the public path

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A root privacy path changes | Classify it as an absorbed local module or an external facade before judging its implementation | [R02](../../../docs/ownership/crates/corelink-privacy/REFERENCE.md#r02) | A `pub use` is treated as a transfer of implementation or runtime ownership |
| A path is newly added or renamed | Trace root declaration, module file, and manifest edge | `src/lib.rs`; `Cargo.toml` | The defining package or compatibility boundary is not known |

<a id="s03"></a>
## S03 — Review absorbed local modules

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `breach`, `consent`, `notice`, `residency`, `sub_processor`, or `dpa::versioning` changes | Trace the module root, contract anchor (trait/schema/state/decision function), affected local submodules, source-stated invariant, public exports, and relevant test filename | [R03](../../../docs/ownership/crates/corelink-privacy/REFERENCE.md#r03); [B02](../../../docs/ownership/crates/corelink-privacy/BLAST_RADIUS.md#b02) | The conclusion needs a configured adapter, data store, policy decision, or execution evidence |
| A local trait or in-memory implementation changes | Keep the type contract distinct from any absent integration | [B01](../../../docs/ownership/crates/corelink-privacy/BLAST_RADIUS.md#b01) | A trait/fake is presented as an external effect |

<a id="s04"></a>
## S04 — Review external facades

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `dsr` or `dsr::statuspage` changes | Treat the module as a facade over DSR packages | [R04](../../../docs/ownership/crates/corelink-privacy/REFERENCE.md#r04) | DSR implementation ownership is inferred from the umbrella path |
| `erasure`, `pseudonymize`, or `dpa::acceptance` changes | Treat the module as a facade over its named external package | [R04](../../../docs/ownership/crates/corelink-privacy/REFERENCE.md#r04) | Worker, pseudonymizer, or DPA implementation ownership is attributed here |

<a id="s05"></a>
## S05 — Bound public-contract and consumer impact

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A module declaration, re-export, or manifest dependency changes | Trace the affected named path through B01–B05 and inspect direct static references | [B06](../../../docs/ownership/crates/corelink-privacy/BLAST_RADIUS.md#b06) | A complete reverse graph, generated consumer, or execution claim is required |
| A local public trait, type, schema, or decision function changes | Use the R03 contract row and B02 family impact; verify source-specific failure ordering before generalizing an invariant | [R03](../../../docs/ownership/crates/corelink-privacy/REFERENCE.md#r03) | Source comments, in-memory implementations, and declared tests are presented as deployed behavior or test results |
| A source comment names a policy or external component | Route to the verified OKF reference; do not reproduce or redefine it here | `docs/knowledge/crates/privacy-compliance.md` | The requested conclusion exceeds the source/static boundary |

<a id="s06"></a>
## S06 — Mandatory stops

Stop for Cargo execution, tests, network access, deployment, credentials, data access,
or any conclusion requiring external operation. Stop also if a facade is used as proof
that its defining package, worker, pseudonymizer, DPA service, or DSR flow is owned by
this package. Recover by recording the missing owner and exact source edge.

<a id="s07"></a>
## S07 — Handoff record

Report the baseline, files read, local/facade classification, changed contract
anchors and source-stated invariants, affected R/B/M identifiers, direct static
references, documentary-check results, and explicit unknowns. A structural check or `git diff --check` is not a Cargo build, test,
deployment result, execution proof, or cold review.

[Reference](../../../docs/ownership/crates/corelink-privacy/REFERENCE.md#r01) ·
[Blast radius](../../../docs/ownership/crates/corelink-privacy/BLAST_RADIUS.md#b01) ·
[Maintenance](../../../docs/ownership/crates/corelink-privacy/MAINTENANCE.md#m01)
