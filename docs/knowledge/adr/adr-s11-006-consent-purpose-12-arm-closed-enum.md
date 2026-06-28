---
type: "ADR"
title: "ADR-S11-006 — 12-Arm Closed ConsentPurpose Enum Discipline"
description: "Why CoreLink enforces purpose-limitation at the type level with a closed 12-variant ConsentPurpose enum and a compile-time-fixed legal-basis mapping."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md"
checkpoint_sha: "0aad76e1d132cd98d35c814a5bb23008c226d08e"
provenance: "AUTHORED"
tags: ["adr", "s11", "consent", "purpose-limitation", "privacy"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-006 — 12-Arm Closed ConsentPurpose Enum Discipline

GDPR Art. 5(1)(b) and LGPD Art. 6 II demand purpose limitation: every consent is tied to a specific, explicit, legitimate purpose. This ADR settles the DESIGN-INTENT for *how* CoreLink enforces that specificity — at the Rust type level — favouring a 12-variant enum over a free-form string, so the type system constrains which purposes code may name.

> **Status vs shipped code (2026-06-28):** the spec's "**closed** enum (no `#[non_exhaustive]`) + compile-time exhaustiveness + `const legal_basis_for`" framing is DESIGN-INTENT, not the shipped shape. The live `ConsentPurpose` enum IS `#[non_exhaustive]` (consent/schema.rs:99) — so it deliberately reserves room for additive purposes — and its legal-basis mapping is a **non-`const`** method, `fn legal_basis(&self) -> LegalBasis` (consent/schema.rs:137), whose doc states the mapping "MUST NOT be changed dynamically" as a discipline rather than a `const`-enforced guarantee. Read the "closed enum / `const` mapping / `#[non_exhaustive]`-forbidden" passages below as the design rationale; the deployed enum is non_exhaustive with a non-const basis method. Adding a purpose still requires this ADR's update + a DPIA refresh.

# Context

The implementation question is how to enforce purpose specificity at the type level. A `ConsentPurpose(String)` admits arbitrary purposes and is vulnerable to scope creep; an open `#[non_exhaustive]` enum permits future additions but does not force every purpose used in code to be pre-registered (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:23-35`).

# Decision

Twelve canonical purposes in a **closed** enum (no `#[non_exhaustive]`), grouped as core-service (4), product-analytics (3), optional (3), and anomaly/abuse (2). The `LegalBasis` mapping per purpose is fixed at compile time via a `const legal_basis_for(purpose)` — no runtime swap — so a purpose cannot silently change basis (e.g. `MarketingEmail` flipping to `LegitimateInterest` when consent is revoked). Adding a purpose requires a spec change, this ADR's update, and a DPIA refresh (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:37-70`). The closed enum yields pattern-match exhaustiveness; "12 is a cap, not a target," aligned with WP29 Op. 03/2013 granularity guidance (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:72-84`).

# Consequences

Positive: regulator-defensible, a compile-time guarantee that no path uses an unregistered purpose, and a tractable per-purpose DPIA (12 sections). Negative: requesting a new purpose carries change-request overhead, mitigated by quarterly batch reviews. Forbidden: `#[non_exhaustive]` on `ConsentPurpose`, runtime swap of a purpose's `LegalBasis`, and purpose strings in DB/API (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:86-97`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:23-35` — Context: the open-enum vs string vs closed-enum framing.
2. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:37-70` — Decision: the 12-variant closed enum + fixed `const` legal-basis mapping.
3. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:72-84` — Rationale: closed-enum exhaustiveness, fixed mapping, "12 is a cap."
4. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:86-97` — Consequences and the three forbidden patterns.
