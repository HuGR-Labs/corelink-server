---
name: own-e2e-dsr
description: Maintain source-scoped ownership records for the e2e-dsr harness without claiming DSR, erasure, identity, storage, provider, or runtime effects.
metadata:
  evidence-set: e2e-dsr-source-cb94e251c
  package: e2e-dsr
  manifest: tests/e2e-dsr/Cargo.toml
  profile: S
  evidence_mode: SOURCE
  source-commit: cb94e251c0f17382565bf863f517945cbb2a84d6
---

# Own `e2e-dsr`

Use this guide for the declared `e2e-dsr` package and its local harness source at the pinned revision. Manifest descriptions, test names, fakes, comments, and assertions are source claims; none establish that a target ran or that DSR, identity, erasure, signing, R2, or backend behavior occurred.

[Scope](#s01) · [Identity](#s02) · [Composition](#s03) · [Axioms](#s04) · [Relations](#s05) · [Evidence](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Task concerns this harness or its ownership records | Read the pinned manifest, `src/{lib,helpers,policy,r2}.rs`, and affected declared target | [R03](../../../docs/ownership/crates/e2e-dsr/REFERENCE.md#r03) | It asks for a real subject request, erasure, receipt, storage, provider, or runtime result |
| Task changes source rather than these records | Route implementation to the owning code workflow; record only observed source contract here | [B03](../../../docs/ownership/crates/e2e-dsr/BLAST_RADIUS.md#b03) | Source edit is being made under this documentation assignment |

<a id="s02"></a>
## S02 — Package identity decision

Take identity from `[package].name = "e2e-dsr"`, manifest path, one declared library, and twelve `[[test]]` entries. Report declarations, not resolved selection or execution. Keep `proptest` as a dev dependency; the manifest does not prove a 500-case run. See [R01](../../../docs/ownership/crates/e2e-dsr/REFERENCE.md#r01) and [R06](../../../docs/ownership/crates/e2e-dsr/REFERENCE.md#r06).

<a id="s03"></a>
## S03 — Harness composition decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| DSR endpoint or receipt fixture changes | Trace helper wiring and the exact assertion source | `tests/e2e-dsr/src/helpers.rs:126-175`; [API-001](../../../docs/ownership/crates/e2e-dsr/REFERENCE.md#api-001) | A claim depends on production JWT, MFA, API, or identity behavior |
| Erasure worker, policy, or R2 stub changes | Keep each local adapter/ledger edge separate; record manual calls and mutations | `helpers.rs:143-175`; `tests/e2e-dsr/src/{policy,r2}.rs`; [B03](../../../docs/ownership/crates/e2e-dsr/BLAST_RADIUS.md#b03) | The claim assumes automatic queue wiring or external backend/storage effects |

<a id="s04"></a>
## S04 — Five source axioms decision

Preserve the five falsifiable source axioms in [R05](../../../docs/ownership/crates/e2e-dsr/REFERENCE.md#r05). They cover fixture identity, fixed-clock assembly, the synthetic MFA token, ordered-subsequence audit matching, and local R2 expiry/key lookup. Axiom changes require the affected source and textual falsifier; none proves execution.

<a id="s05"></a>
## S05 — Atomic-relation decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Direct dependency, local module, or documentary consumer changes | Update one arrow at a time, naming producer, consumer, surface, activation, and failure boundary | [B03](../../../docs/ownership/crates/e2e-dsr/BLAST_RADIUS.md#b03) | A complete runtime/reverse graph or provider effect is inferred from a manifest edge |

<a id="s06"></a>
## S06 — Evidence and OKF decision

Label claims `SOURCE`, `DOCUMENTARY`, or `UNKNOWN`. The verified canonical OKF route is the [SRE operations hub](../../../docs/knowledge/ops/sre-operations-hub.md); route there only. Do not copy, revalidate, or redefine its policy, and do not treat it as evidence for this package.

<a id="s07"></a>
## S07 — Handoff decision

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Four ownership artifacts are ready | Report baseline, exact paths, affected R/B/M IDs, four structural checker results, whitespace diff, and five unresolved unknowns | [M06](../../../docs/ownership/crates/e2e-dsr/MAINTENANCE.md#m06) | A structural check is called a build, test, runtime, external API, provider, deployment, or independent-review result |

Completion requires S01–S07, [R01–R08](../../../docs/ownership/crates/e2e-dsr/REFERENCE.md#r01), [B01–B06](../../../docs/ownership/crates/e2e-dsr/BLAST_RADIUS.md#b01), and [M01–M06](../../../docs/ownership/crates/e2e-dsr/MAINTENANCE.md#m01). Keep five unknowns explicit and the scope source-only.
