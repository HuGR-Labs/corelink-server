---
name: own-corelink-tier-selection
description: Maintain source-scoped ownership records for CoreLink tier taxonomy, local gates, and in-memory fixtures without inferring external effects.
metadata:
  evidence-set: w011-tier-selection-source-static-20260920
  package: corelink-tier-selection
  manifest: crates/corelink-tier-selection/Cargo.toml
  profile: S
  evidence_mode: SOURCE
  source-commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
---

# Own `corelink-tier-selection`

Use this guide only for `crates/corelink-tier-selection/Cargo.toml` and the
named local Rust declarations at the pinned source baseline. SOURCE means
checked-in text. It establishes neither a caller, an external effect, an
executed path, nor an environment result. The verified OKF route is used only
as routing context and is neither copied nor revalidated here.

[Scope](#s01) · [Taxonomy](#s02) · [Gates](#s03) · [Fakes](#s04) · [Relations](#s05) · [Evidence](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Fix the source scope

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A request names this package or its ownership records | Record the pinned baseline, manifest, root, and affected local module | `Cargo.toml`; `src/lib.rs`; changed paths | Tree identity, package identity, or source scope is ambiguous |

<a id="s02"></a>
## S02 — Classify the tier taxonomy

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A tier variant, canonical list, string, or route predicate changes | Compare `TierKind`, `canonical_tiers`, `canonical_runner_tiers`, and their named helpers | `src/tier.rs` | A conclusion needs a consumer contract or observed behavior |

<a id="s03"></a>
## S03 — Classify local gate and identifier contracts

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A gate, identifier, receipt, state, or error declaration changes | Trace its public signature and exact local branch or data shape | `src/{dpa,tenant,ledger,error}.rs` | The claim requires durable state, a handler, or a caller decision |

<a id="s04"></a>
## S04 — Preserve fake boundaries

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A local sink, gate, client-shaped fake, or adversarial fixture changes | Name its trait, mutex-backed field, and inspection or failure helper separately | `src/{dpa,audit,stripe}.rs` | A fake is presented as an external system or a persistence result |

<a id="s05"></a>
## S05 — Record atomic relations

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A declared source relation changes | Write one directed arrow, its trigger, evidence path, and non-inference boundary | [B01–B06](../../../docs/ownership/crates/corelink-tier-selection/BLAST_RADIUS.md#b01) | Full reverse-graph, compatibility, or execution evidence is requested |

<a id="s06"></a>
## S06 — Apply evidence and OKF routing

Class each statement as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE is limited to
manifest and local text; DOCUMENTARY is limited to structural checks. The
verified OKF route remains canonical routing only. Do not recreate it here or
promote it to proof about this package.

<a id="s07"></a>
## S07 — Hand off a bounded packet

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| The four ownership artifacts are ready | Report the baseline, four paths, R/B/M identifiers, five axioms, five unknowns, four profile-S check results, and whitespace-diff result | [M06](../../../docs/ownership/crates/corelink-tier-selection/MAINTENANCE.md#m06) | A documentary outcome is described as execution, compatibility, or independent-review proof |

Success is bounded, falsifiable source documentation. Completeness requires
S01–S07, [R01–R08](../../../docs/ownership/crates/corelink-tier-selection/REFERENCE.md#r01),
[B01–B06](../../../docs/ownership/crates/corelink-tier-selection/BLAST_RADIUS.md#b01),
and [M01–M06](../../../docs/ownership/crates/corelink-tier-selection/MAINTENANCE.md#m01).
Quality requires atomic relations, explicit modes, five source axioms, five
unknowns, and no unsupported external-effect claim.
