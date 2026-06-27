---
type: "ADR"
title: "ADR-S11-008 — Mandatory Sub-Processor Notifications for ALL 5 Canonical Plans"
description: "Why CoreLink reversed its tier-gated sub-processor notice design and now sends mandatory 30-day advance notices to all 5 plans under a legal_obligation basis that cannot be opted out of."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md"
  - "crates/corelink-privacy/src/sub_processor.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "sub-processors", "gdpr", "lgpd", "legal-obligation"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-008 — Mandatory Sub-Processor Notifications for ALL 5 Canonical Plans

Sub-processor transparency under GDPR Art. 28.2 and LGPD Art. 39 is a regulatory right, not an email marketing campaign — so it cannot be tier-gated. This ADR (v2.0.0) **reverses** the original v1.0 design that auto-subscribed only `team`+ plans, and establishes that all five canonical plans receive mandatory 30-day advance notice on sub-processor changes, carried under a `legal_obligation` basis that consent-revoke cannot touch.

# Context

WI-S11-005 implements sub-processor transparency per GDPR Art. 28.2 + LGPD Art. 39. The original ADR v1.0 tier-gated notices — `team`+ auto-subscribed, `free`/`solo` opt-in — on an LGPD Art. 6 minimização (anti-spam) rationale (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:23-33`).

# Decision

Reversed: **all 5 canonical plans** (`free`, `solo`, `team`, `business`, `enterprise`) receive mandatory notices. The `sub_processor_notifications` purpose carries a `legal_obligation` basis (the canonical purpose taxonomy) and is therefore NOT opt-out-able via consent-revoke — only `marketing_email` is (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:35-44`). The rationale: GDPR Art. 28.2 and LGPD Art. 39 carve out no tier exception; every plan — including `free` — enters a DPA with HuGR, so the Art. 28.2 right to object applies to all; and LGPD Art. 6 minimização governs data *collection*, not legal notification obligations, so it does not override Art. 39 (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:46-66`). v1.0 had conflated `marketing_email` (opt-out, legitimate interest) with `sub_processor_notifications` (legal obligation).

# Consequences

All 5 plans get 30-day advance notice emails on sub-processor changes; the `sub_processor_notifications` purpose's `legal_obligation` basis means `POST /v1/privacy/dsr/consent_revoke` cannot opt out of it; the broadcast log seeds all subscribed tenants; and any future re-introduction of tier-gating requires Privacy Officer + Compliance + Legal sign-off (`specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:76-82`).

# Status vs shipped code

The sub-processor system this ADR describes is **library-only and not deployed**. The transparency
primitive ships as a pure-logic skeleton in `corelink-privacy`
(`crates/corelink-privacy/src/sub_processor.rs:1-12`, per the `trait-abstraction-defer` charter), with
in-memory orchestrators exercising the invariants but **no production wiring** (the CloudEvents fan-out
to R2 audit Object Lock, the D1 broadcast log, the PagerDuty objection routing are all the deferred
"production wiring"), and **no route mounted** in the deployed container (which depends on
`corelink-privacy-erasure-worker`, NOT `corelink-privacy`). So the Consequences — the **30-day advance
notice emails**, the **broadcast log seeding all subscribed tenants**, and `POST
/v1/privacy/dsr/consent_revoke` being unable to opt out — describe the designed behavior, not a live
one; the `legal_obligation` non-opt-out semantics are proven against fakes, not running in prod.

The "5 canonical plans" list this ADR enumerates — `free`, `solo`, `team`, `business`, `enterprise` —
does **not** match CoreLink's canonical **billing** taxonomy, which is the 6 tiers
`free | solo | starter | pro | max | enterprise`. `team`/`business` are **eviction `Tier` enum** names,
not sold billing plans, so the plan list here conflates the two axes. This does not change the
decision (sub-processor notices are a `legal_obligation` sent to **all** plans, non-opt-out-able) — the
"all plans" universality is exactly what makes the specific plan enumeration immaterial — but for the
authoritative plan set defer to [ADR-S19-001](/adr/adr-s19-001-tier-taxonomy-amendment-5-to-6.md).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:23-33` — Context: WI-S11-005 + the superseded v1.0 tier-gating proposal.
2. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:35-44` — Decision: all-5-plans + the legal_obligation (non-opt-out) basis.
3. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:46-66` — Rationale: Art. 28.2 / Art. 39 no-tier-carve-out + minimização does not override Art. 39.
4. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:76-82` — Consequences: notice delivery, non-opt-out, broadcast log, sign-off gate.
5. `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md:68-74` — Correction to v1.0: v1.0 had conflated `marketing_email` (legitimate interest) with `sub_processor_notifications` (legal obligation).
6. `crates/corelink-privacy/src/sub_processor.rs:1-12` — the sub-processor primitive ships as a pure-logic skeleton in `corelink-privacy` (NOT a container dep, no route mounted): the 30-day notice emails / broadcast-log / non-opt-out consequences are designed, not deployed.
