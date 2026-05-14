---
id: "ADR-S11-009"
title: "3 jurisdictional templates coverage rationale — BR (ANPD/LGPD) + EU (Irish DPC/GDPR) + US-CA (California AG/CCPA); UK ICO + other state AGs + India DPDPA + China PIPL deferred post-GA"
type: "adr"
doc_status: "SEALED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - { role: "privacy_lead", name: "DPO interim (Gustavo until hire)" }
  - { role: "compliance", name: "Compliance Officer (sign-off pending pre-GA hire)" }
supersedes: null
superseded_by: null
tags: ["adr", "s-11", "privacy", "breach-notification", "lgpd", "gdpr", "ccpa", "jurisdictional-coverage"]
review_schedule: "Quarterly — if enterprise customer materializes in UK/India/China/other US states, escalate immediately"
---

# ADR-S11-009 — 3 Jurisdictional Templates Coverage Rationale

## Context

WI-S11-006 requires pre-drafted breach notification templates for regulatory dispatch.
A breach notification template takes 4–8h to draft from scratch under legal counsel review.
During an actual incident, this drafting time directly eats into the 72h regulatory SLA
(GDPR Art. 33 + LGPD Art. 48 + CCPA §1798.82). Pre-drafted templates eliminate this bottleneck.

**Design question**: How many jurisdictions to cover pre-GA?

CoreLink's target markets are:
1. **Brazil** — primary HuGR Labs headquarters jurisdiction; all customers are at minimum subject to LGPD.
2. **European Union** — target enterprise market; GDPR applies to any EU data subjects regardless of controller location.
3. **United States (California)** — largest US SaaS market; CCPA §1798.82 applies to businesses that process California residents' personal data.

Additional jurisdictions with active privacy laws:
- **UK**: UK GDPR (post-Brexit) + ICO as supervisory authority. UK ICO notification requirements mirror GDPR Art. 33 (72h deadline).
- **Other US states**: Virginia (CDPA), Colorado (CPA), Connecticut (CTDPA), Texas (TDPSA), etc. — each has notification requirements.
- **India**: Digital Personal Data Protection Act (DPDPA) 2023 — rules pending; notification to Data Protection Board required.
- **China**: Personal Information Protection Law (PIPL) 2021 — notification to Cyberspace Administration of China (CAC).
- **Canada**: PIPEDA / Quebec Law 25 — OPC notification.

## Decision

**Cover 3 jurisdictions** (BR + EU + US-CA) in pre-drafted templates for pre-GA.
**Defer** UK ICO, other US state AGs, India DPDPA, China PIPL, Canada PIPEDA to post-GA enterprise expansion (S-19+).

## Rationale

### Why BR + EU + US-CA are sufficient pre-GA

1. **BR (LGPD + ANPD)**: HuGR Labs is incorporated in Brazil. ALL customers are subject to LGPD regardless of location — a Brazilian controller must notify ANPD for any breach affecting Brazilian titulares. This is non-optional.

2. **EU (GDPR + Irish DPC)**: The EU market represents the highest regulatory risk (GDPR fines up to 4% global revenue). Irish DPC as lead supervisory authority (LSA) per GDPR Art. 56 covers multi-country EU breaches with a single notification. Pre-drafting the Irish DPC template covers the entire EU in one template.

3. **US-CA (CCPA + California AG)**: California has the most comprehensive US breach notification law and the largest US SaaS market. California AG notification ≥ 500 residents (§1798.82(f)). Other US state laws (Virginia, Colorado, etc.) have similar structures — Legal can adapt the CA template with minimal effort when needed.

### Why UK ICO is deferred

- UK GDPR is structurally identical to EU GDPR.
- The Irish DPC template covers ~90% of the content.
- UK ICO requires a separate notification only when HuGR has a UK main establishment OR when UK residents are specifically affected.
- Pre-GA customer base is unlikely to include material UK-resident-only accounts; EU (including Irish DPC) template suffices.
- Deferral risk: if a UK enterprise prospect materializes, escalate immediately — UK ICO template can be derived from GDPR template in ~2h.

### Why other US state AGs are deferred

- CCPA §1798.82 notification structure is the US de-facto standard (California as privacy leader).
- Virginia CDPA §59.1-578, Colorado CPA, etc. have 72h–30d notification requirements with similar content.
- The California AG template, lightly adapted, covers other states.
- No current enterprise customer in those states at pre-GA stage.

### Why India DPDPA and China PIPL are deferred

- DPDPA rules are pending (2023 law; implementing rules as of early 2026 not yet finalized).
- PIPL requires in-China data residency and appointed data protection officer — full compliance is a separate S-19+ enterprise initiative.
- Pre-GA customer base does not include India/China enterprise accounts.

## Quarterly Review Trigger

This decision MUST be revisited quarterly in:
1. Enterprise sales pipeline includes prospect in UK / India / China / other US states.
2. ANPD/DPC/AG issues new guidance that materially changes notification requirements.
3. A real incident reveals coverage gaps (e.g., affected subjects in uncovered jurisdictions).

**Action on trigger**: Privacy Officer + Legal assess → ADR update → template drafted within 30d.

## Consequences

**Positive**:
- Pre-drafted templates for 90%+ of pre-GA customers.
- Legal review burden bounded to 3 templates (not 6–8).
- Time-to-decision SLO ≤ 4h achievable with 3 pre-drafted templates.

**Negative/risks**:
- If a breach affects UK residents, Irish DPC notification + ad-hoc UK ICO notification required. UK ICO template must be drafted under time pressure.
- If a breach affects US residents in states other than California, state-by-state compliance may require additional notifications.
- Mitigation: legal retainer with international privacy law firm pre-GA (compliance_matrix.md §9 GAP-01).

## Decision Owner Sign-offs

| Role | Status |
|---|---|
| Owner (Gustavo Schneiter) | ACCEPTED |
| DPO interim | ACCEPTED (interim Gustavo) |
| Compliance Officer | PENDING (pre-GA hire per compliance_matrix.md §9 GAP-01) |
| Legal externo | PENDING (3 Legal Review PDFs per template; EVT-044) |
