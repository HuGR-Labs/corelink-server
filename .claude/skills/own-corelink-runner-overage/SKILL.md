---
name: own-corelink-runner-overage
description: >-
  Own the source-defined, integer-only runner-overage arithmetic API and tier
  mapping. Use for static contract work only; do not use it to assert runner,
  billing, Stripe, credential, or deployment operation.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-runner-overage
  manifest: crates/corelink-runner-overage/Cargo.toml
  source-commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
  evidence-set: w010-runner-overage-source-static-20260920
---

# Ownership — corelink-runner-overage

Candidate, SOURCE-only ownership. This guide describes a manifest and one Rust
module. It does not establish that a runner is invoked, a meter is emitted, a
charge occurs, a Stripe request is sent, a credential exists, or a deployment
is reachable.

[Entry](#s01) · [Boundary](#s02) · [Read](#s03) · [Decide](#s04) · [Flow](#s05) · [Stop](#s06) · [Record](#s07)

<a id="s01"></a>
## S01 — Entry

| Condition | Action | Stop when |
|---|---|---|
| A change names tier, SKU, allowance, overage, decimal rendering, or millicents | Trace the named public item in [R03](../../../docs/ownership/crates/corelink-runner-overage/REFERENCE.md#r03) | The request requires an external outcome |
| A change names rate, rounding, or integer width | Trace [R04](../../../docs/ownership/crates/corelink-runner-overage/REFERENCE.md#r04) and [B03](../../../docs/ownership/crates/corelink-runner-overage/BLAST_RADIUS.md#b03) | Authority for a non-source policy value is required |

<a id="s02"></a>
## S02 — Authority and boundary

Implementation territory is `crates/corelink-runner-overage/Cargo.toml` and
`src/lib.rs`. This package owns its constants, four public `RunnerTier`
methods, and four public free helpers. It owns neither a caller's aggregate input nor an external price,
meter, ledger, provider, credential, or runtime. The verified OKF routing
reference is `docs/internal/okf-wiki/concept-manifest.yaml`; consult it only to
keep this leaf separate from a wider narrative, never as new evidence or an
operational claim.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| Which symbols and types are public? | [R03](../../../docs/ownership/crates/corelink-runner-overage/REFERENCE.md#r03) |
| Which arithmetic predicates constrain a change? | [R04](../../../docs/ownership/crates/corelink-runner-overage/REFERENCE.md#r04) |
| Which static relation changes with a symbol? | [B02–B05](../../../docs/ownership/crates/corelink-runner-overage/BLAST_RADIUS.md#b02) |
| Which evidence is permitted? | [M01–M04](../../../docs/ownership/crates/corelink-runner-overage/MAINTENANCE.md#m01) |

<a id="s04"></a>
## S04 — Decision form

| Condition | Required action | Evidence | Stop if |
|---|---|---|---|
| Tier/SKU/allowance changes | Preserve the bidirectional mapping and unit conversion review | R03, R04, B02 | An external entitlement owner must decide |
| Decimal or charge helper changes | State integer operands, floor point, and saturation behavior | R04, B03 | Currency policy or external acceptance is asserted |
| Public signature/constant change | Record static compatibility risk and unknown reverse consumers | B05, M05 | Consumer inventory is needed for approval |

<a id="s05"></a>
## S05 — Source-only flow

1. Confirm the pinned manifest, `src/lib.rs`, and intended baseline.
2. Identify the exact public symbol and write a falsifiable source predicate.
3. Trace its local unit conversion, saturation, formatting, or mapping relation.
4. Record SOURCE evidence and explicit unknowns; do not promote comments to runtime fact.
5. Run only documentary structural checks after documentation edits.

<a id="s06"></a>
## S06 — Stop conditions

Stop for runner execution, usage ingestion, external meter or provider action,
price configuration, credentials, ledger mutation, network, Cargo compilation,
tests, deployment, publishing, or production evidence. Stop as well when a
claim depends on a reverse consumer, policy authority, or external rounding
rule not established by this package source.

<a id="s07"></a>
## S07 — Record and done gate

Record baseline, paths inspected and changed, affected R/B/M IDs, source
predicates, documentary command results, and unknowns. Success is falsifiable
static documentation; completeness is S01–S07 plus the three companion
documents; quality is the absence of runtime inference. A checker pass is not
review, approval, or evidence of any external operation.

[Back to entry](#s01)
