---
type: "ADR"
title: "ADR-S11-008 — Mandatory Sub-Processor Notifications for ALL 5 Canonical Plans"
description: "Why CoreLink reversed its tier-gated sub-processor notice design and now sends mandatory 30-day advance notices to all 5 plans under a legal_obligation basis that cannot be opted out of."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md"
checkpoint_sha: "d24ff6f3093497a7f2a63aa232ef181733423c19"
provenance: "AUTHORED"
tags: ["adr", "s11", "sub-processor", "gdpr", "lgpd", "legal-obligation"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-008 — Mandatory Sub-Processor Notifications for ALL 5 Canonical Plans

Sub-processor transparency under GDPR Art. 28.2 and LGPD Art. 39 is a regulatory right, not an email marketing campaign — so it cannot be tier-gated. This ADR (v2.0.0) **reverses** the original v1.0 design that auto-subscribed only `team`+ plans, and establishes that all five canonical plans receive mandatory 30-day advance notice on sub-processor changes, carried under a `legal_obligation` basis that consent-revoke cannot touch.

# Context

WI-S11-005 implements sub-processor transparency per GDPR Art. 28.2 + LGPD Art. 39. The original ADR v1.0 tier-gated notices — `team`+ auto-subscribed, `free`/`solo` opt-in — on an LGPD Art. 6 minimização (anti-spam) rationale (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:23-33`).

# Decision

Reversed: **all 5 canonical plans as this ADR named them** (`free`, `solo`, `team`, `business`, `enterprise`) receive mandatory notices. ⚠️ **Taxonomy note (2026-06-28):** this "5 canonical plans" list is the S11-era taxonomy, NOT the sold ladder. ADR-S19-001 later REMOVED `team` from the visible/sold taxonomy (retained only additively in the persisted CHECK domain for back-compat) and added `starter`/`pro`/`max`; `business` named here NEVER SHIPPED. The live sold ladder is the 6-tier `{Free,Solo,Starter,Pro,Max,Enterprise}` (see `launch/tier-model`). The ADR's legal conclusion is unchanged — sub-processor notices are mandatory for EVERY plan including `free` regardless of the tier names — but read the specific 5 slugs here as historical, not the current taxonomy. The `sub_processor_notifications` purpose carries a `legal_obligation` basis (the canonical purpose taxonomy) and is therefore NOT opt-out-able via consent-revoke — only `marketing_email` is (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:35-44`). The rationale: GDPR Art. 28.2 and LGPD Art. 39 carve out no tier exception; every plan — including `free` — enters a DPA with HuGR, so the Art. 28.2 right to object applies to all; and LGPD Art. 6 minimização governs data *collection*, not legal notification obligations, so it does not override Art. 39 (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:46-66`). v1.0 had conflated `marketing_email` (opt-out, legitimate interest) with `sub_processor_notifications` (legal obligation).

# Consequences

All 5 plans get 30-day advance notice emails on sub-processor changes; the `sub_processor_notifications` purpose's `legal_obligation` basis means `POST /v1/privacy/dsr/consent_revoke` cannot opt out of it; the broadcast log seeds all subscribed tenants; and any future re-introduction of tier-gating requires Privacy Officer + Compliance + Legal sign-off (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:76-82`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:23-33` — Context: WI-S11-005 + the superseded v1.0 tier-gating proposal.
2. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:35-44` — Decision: all-5-plans + the legal_obligation (non-opt-out) basis.
3. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:46-66` — Rationale: Art. 28.2 / Art. 39 no-tier-carve-out + minimização does not override Art. 39.
4. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:76-82` — Consequences: notice delivery, non-opt-out, broadcast log, sign-off gate.
5. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:68-74` — Correction to v1.0: v1.0 had conflated `marketing_email` (legitimate interest) with `sub_processor_notifications` (legal obligation).
