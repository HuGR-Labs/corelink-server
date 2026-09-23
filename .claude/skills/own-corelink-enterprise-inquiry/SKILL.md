---
name: own-corelink-enterprise-inquiry
description: Route source-static ownership changes for the enterprise inquiry Rust package.
metadata:
  evidence-set: w012-enterprise-inquiry-source-static-20260920
  source-commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
  schema: "corelink-ownership/1.1"
  package: corelink-enterprise-inquiry
  manifest: crates/corelink-enterprise-inquiry/Cargo.toml
  profile: S
---

# Ownership — corelink-enterprise-inquiry

This guide owns only the manifest and local Rust declarations. It records no external operation, persisted state, provider effect, deployment, or execution result.

[Baseline](#s01) · [Surface](#s02) · [Ledger](#s03) · [Adapters](#s04) · [Unknowns](#s05) · [Checks](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Fix the source baseline

**Condition:** an ownership or contract change is proposed. **Action:** record `3feae2baed63061354533ffdfe2d94acfd24aa9a` and inspect the manifest plus affected `src/` text. **Evidence:** [R01](../../../docs/ownership/crates/corelink-enterprise-inquiry/REFERENCE.md#r01). **Stop:** a different revision or package path needs a new scoped review.

<a id="s02"></a>
## S02 — Preserve the public surface

**Condition:** an exported type, trait, constant, or module changes. **Action:** reconcile its exact Rust shape, non-exhaustive marker where present, and falsifier in R02–R04; include the known `corelink-ops` re-export and `corelink-slack-real` adapter implementation in source-impact routing. **Evidence:** [R02](../../../docs/ownership/crates/corelink-enterprise-inquiry/REFERENCE.md#r02), [R08](../../../docs/ownership/crates/corelink-enterprise-inquiry/REFERENCE.md#r08). **Stop:** compatibility beyond these checked-in edges remains unknown without separate evidence.

<a id="s03"></a>
## S03 — Change ledger rules atomically

**Condition:** `submit_inquiry`, outbox status, audit ordering, idempotency, escalation, or SLA text changes. **Action:** retain an atomic source predicate and its B relation. **Evidence:** [R05](../../../docs/ownership/crates/corelink-enterprise-inquiry/REFERENCE.md#r05), [B02](../../../docs/ownership/crates/corelink-enterprise-inquiry/BLAST_RADIUS.md#b02). **Stop:** do not turn source control flow into a durability or delivery claim.

<a id="s04"></a>
## S04 — Keep ports and fakes distinct

**Condition:** Slack, CRM, mail, audit, encryption, or HubSpot text changes. **Action:** identify the trait, local implementation, and source-only boundary separately. **Evidence:** [R03](../../../docs/ownership/crates/corelink-enterprise-inquiry/REFERENCE.md#r03), [B03](../../../docs/ownership/crates/corelink-enterprise-inquiry/BLAST_RADIUS.md#b03). **Stop:** transport, credential, provider, or data-effect claims require independent evidence.

<a id="s05"></a>
## S05 — Route canonical policy only

**Condition:** policy or operational interpretation is needed. **Action:** route to the verified [OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md); do not copy or revalidate it here. **Evidence:** [R08](../../../docs/ownership/crates/corelink-enterprise-inquiry/REFERENCE.md#r08). **Stop:** the package source is not canonical policy evidence.

<a id="s06"></a>
## S06 — Perform documentary checks

**Condition:** only these four ownership artifacts changed. **Action:** run the supplied profile-S checker once for each artifact and the baseline whitespace diff. **Evidence:** [M05](../../../docs/ownership/crates/corelink-enterprise-inquiry/MAINTENANCE.md#m05). **Stop:** repair only these four paths on a structural failure.

<a id="s07"></a>
## S07 — Hand off bounded evidence

**Condition:** static documentation is ready. **Action:** report baseline, paths, R/B/M IDs, five axioms, unknowns, four checker verdicts, and diff result. **Evidence:** [M06](../../../docs/ownership/crates/corelink-enterprise-inquiry/MAINTENANCE.md#m06). **Stop:** documentary checks are not a build, test, independent review, or operational result.
