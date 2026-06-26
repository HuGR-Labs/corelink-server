---
type: "ADR"
title: "ADR-S11-007 — Privacy Notice Material vs Minor Change Criteria"
description: "The typed semver criteria that decide whether a privacy-notice change is material (force re-consent) or minor (silent), making the Privacy Officer's judgment consistent and auditable."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "privacy-notice", "consent", "semver"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-007 — Privacy Notice Material vs Minor Change Criteria

A "material change" to a privacy notice legally triggers force re-consent, but "material" is a judgment call that drifts between reviewers and over time, creating audit-defensibility gaps. This ADR codifies the call as a **typed semver convention** — a major bump means material (re-consent), a minor bump means clarification (silent) — so the Privacy Officer applies a written table instead of ad-hoc judgment, and CI can enforce a valid bump.

# Context

EDPB (WP29 WP260rev01), LGPD/ANPD, and CCPA/CPPA require that material privacy-notice changes force re-consent from affected data subjects. Codifying the criteria under CTRL-PRIV-CONSENT-005 replaces ad-hoc judgment — which causes inconsistent classification and audit gaps — with a typed table that a CI hook (`validate_privacy_notice.py`) enforces (`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:23-29`).

# Decision

A change MUST be **major** (material → force re-consent via `stale_consent_check`) when it meets any of M-1..M-7: new data category, new sub-processor, new purpose, extended retention, extended DSR SLA, new cross-border transfer region, or changed legal basis for an existing purpose (`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:33-45`). A change MAY be **minor** (silent) only when none of those apply and it is purely clarification, typo/grammar, DPO-contact update, an additional non-primary locale, or formatting (m-1..m-5) (`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:47-60`). Borderline changes default to **minor** (defensible posture, mitigated by Privacy Officer dual-approval + quarterly audit), escalating to Legal for a written opinion when undeterminable; every change references the matched section in its PR (`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:62-68`).

# Consequences

The CI hook `validate_privacy_notice.py` takes this ADR as its normative reference; every notice-change PR must cite an M-/m-/borderline section; major bumps require Privacy Officer dual-approval; and all minor classifications get a quarterly audit review to catch misclassifications (`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:82-87`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:23-29` — Context: ad-hoc judgment problem + the semver-convention solution.
2. `specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:33-45` — the M-1..M-7 major-bump criteria.
3. `specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:47-60` — the m-1..m-5 minor-bump criteria.
4. `specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:62-68` — Borderline default-minor + escalation procedure.
5. `specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md:82-87` — Consequences: CI hook, PR reference, dual-approval, quarterly audit.
