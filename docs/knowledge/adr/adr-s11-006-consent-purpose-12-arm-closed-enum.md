---
type: "ADR"
title: "ADR-S11-006 — 12-Arm Closed ConsentPurpose Enum Discipline"
description: "Why CoreLink enforces purpose-limitation at the type level with a closed 12-variant ConsentPurpose enum and a compile-time-fixed legal-basis mapping."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md"
  - "crates/corelink-privacy/src/consent/schema.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "consent", "purpose-limitation", "privacy"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-006 — 12-Arm Closed ConsentPurpose Enum Discipline

GDPR Art. 5(1)(b) and LGPD Art. 6 II demand purpose limitation: every consent is tied to a specific, explicit, legitimate purpose. This ADR settles *how* CoreLink enforces that specificity — at the Rust type level — choosing a **closed** 12-variant enum over a free-form string or an open `#[non_exhaustive]` enum, so the compiler itself guarantees no code path can use an unregistered purpose.

# Context

The implementation question is how to enforce purpose specificity at the type level. A `ConsentPurpose(String)` admits arbitrary purposes and is vulnerable to scope creep; an open `#[non_exhaustive]` enum permits future additions but does not force every purpose used in code to be pre-registered (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:23-35`).

# Decision

Twelve canonical purposes in a **closed** enum (no `#[non_exhaustive]`), grouped as core-service (4), product-analytics (3), optional (3), and anomaly/abuse (2). The `LegalBasis` mapping per purpose is fixed at compile time via a `const legal_basis_for(purpose)` — no runtime swap — so a purpose cannot silently change basis (e.g. `MarketingEmail` flipping to `LegitimateInterest` when consent is revoked). Adding a purpose requires a spec change, this ADR's update, and a DPIA refresh (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:37-70`). The closed enum yields pattern-match exhaustiveness; "12 is a cap, not a target," aligned with WP29 Op. 03/2013 granularity guidance (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:72-84`).

# Consequences

Positive: regulator-defensible, a compile-time guarantee that no path uses an unregistered purpose, and a tractable per-purpose DPIA (12 sections). Negative: requesting a new purpose carries change-request overhead, mitigated by quarterly batch reviews. Forbidden: `#[non_exhaustive]` on `ConsentPurpose`, runtime swap of a purpose's `LegalBasis`, and purpose strings in DB/API (`specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:86-97`).

# Status vs shipped code

The shipped enum **contradicts this ADR's central decision**: `ConsentPurpose` carries
`#[non_exhaustive]` (`crates/corelink-privacy/src/consent/schema.rs:99`), which the ADR explicitly
lists as a *forbidden* pattern ("closed enum, no `#[non_exhaustive]`"). The compile-time
exhaustiveness guarantee the ADR relies on therefore does **not** hold for downstream crates. The
fixed-at-compile-time legal-basis mapping IS shipped as a `const`-style `match`
(`crates/corelink-privacy/src/consent/schema.rs:138-149`), but it groups the 12 variants **by legal
basis** — Contract(2) / LegalObligation(1) / LegitimateInterest(2) / Consent(7) — not the ADR's
core-service(4) / product-analytics(3) / optional(3) / anomaly-abuse(2) **category** grouping; the two
groupings are orthogonal, so the "4/3/3/2" framing does not match the shipped basis partition. The
no-runtime-swap and pre-registered-purpose intent holds; the `#[non_exhaustive]` "FORBIDDEN" claim is
the one the code breaks.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:23-35` — Context: the open-enum vs string vs closed-enum framing.
2. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:37-70` — Decision: the 12-variant closed enum + fixed `const` legal-basis mapping.
3. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:72-84` — Rationale: closed-enum exhaustiveness, fixed mapping, "12 is a cap."
4. `specs/03_architecture/adrs/ADR-S11-006-consent-purpose-12-arm-closed-enum.md:86-97` — Consequences and the three forbidden patterns.
