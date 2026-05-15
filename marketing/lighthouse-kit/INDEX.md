---
id: "LIGHTHOUSE-KIT-INDEX"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "marketing", "kit", "onboarding", "index", "r5p"]
---

# CoreLink Lighthouse Customer Onboarding Kit — INDEX

> **Purpose:** Day-1 outreach kit for the 3-slot Lighthouse Program (2 Team + 1 Enterprise BYOK).
> **Audience:** Customer Success + Founder (outreach owners). Several assets are customer-facing once instantiated; `07-pricing-comparison-internal.md` is **INTERNAL ONLY**.
> **Canonical spec:** `specs/_lighthouse/lighthouse-customer-program.md`.

---

## 1. Asset table

| # | File | Use | Audience |
|---|---|---|---|
| 01 | `01-outreach-email.md` | Cold email template (2 variants) | Customer-facing |
| 02 | `02-intro-deck.md` | 12-slide intro deck outline | Customer-facing |
| 03 | `03-integration-timeline.md` | D+0..D+60 engagement schedule | Customer-facing |
| 04 | `04-weekly-checkin-agenda.md` | 30-min weekly sync template | Customer-facing |
| 05 | `05-sla-attestation-instructions.md` | How to sign the 30d SLA attestation form | Customer-facing |
| 06 | `06-case-study-interview-script.md` | 60-min interview agenda for case study | Customer-facing |
| 07 | `07-pricing-comparison-internal.md` | Lighthouse-vs-paid-tier value sizing | **INTERNAL ONLY** |
| -- | `INDEX.md` | This file | Internal navigation |

---

## 2. Acquisition flow

```
                            ┌─────────────────────────┐
                            │  D-60 Shortlist (§spec) │
                            └────────────┬────────────┘
                                         │
                                         ▼
                         ┌──────────────────────────────┐
                         │  01-outreach-email.md sent   │
                         │  (variant: OSS / Enterprise) │
                         └────────────┬─────────────────┘
                                      │ reply / book call
                                      ▼
                         ┌──────────────────────────────┐
                         │  Intro call (30 min)         │
                         │  walks 02-intro-deck.md      │
                         └────────────┬─────────────────┘
                                      │ verbal interest
                                      ▼
                         ┌──────────────────────────────┐
                         │  NDA + LOI signed (Legal)    │
                         │  state: Recruiting→Engaged   │
                         └────────────┬─────────────────┘
                                      │
                                      ▼
                         ┌──────────────────────────────┐
                         │  03-integration-timeline.md  │
                         │  shared as schedule contract │
                         └────────────┬─────────────────┘
                                      │ D+0
                                      ▼
                ┌──────────────────────────────────────────┐
                │  Migration (D+1..D+10)                   │
                │  state: Engaged→Migrating→Observing      │
                └──────────────┬───────────────────────────┘
                               │ D+10
                               ▼
                ┌──────────────────────────────────────────┐
                │  30d observation (D+10..D+40)            │
                │  weekly cadence: 04-weekly-checkin       │
                └──────────────┬───────────────────────────┘
                               │ D+40
                               ▼
                ┌──────────────────────────────────────────┐
                │  05-sla-attestation-instructions.md      │
                │  customer signs, CoreLink countersigns   │
                │  state: Observing→Attested               │
                └──────────────┬───────────────────────────┘
                               │ D+45
                               ▼
                ┌──────────────────────────────────────────┐
                │  06-case-study-interview-script.md       │
                │  60-min interview → case study published │
                │  state: Attested→CaseStudySigned         │
                └──────────────────────────────────────────┘
```

Total elapsed: **~60 days** from intro call to published case study.

---

## 3. Slot → variant matrix

| Slot id | Tier | Email variant | Deck emphasis | Case study slot |
|---|---|---|---|---|
| `LH-FORGE` | Team | n/a (internal) | n/a | `team-tier-1-forge.md` |
| `LH-OSS-01` | Team | A (OSS Bazel) | OSS-friendly, public testimonial | `team-tier-2-oss.md` |
| `LH-ENT-BYOK-01` | Enterprise BYOK | B (CISO) | BYOK + Schrems II + audit chain | `enterprise-byok.md` |

---

## 4. Quality gates referenced

- `python3 scripts/validate_specs.py` — must not regress (marketing layer; spec corpus untouched).
- Markdownlint clean — all files render in GitHub + Pitch.com / Slides exporters.
- No real customer names — only synthetic candidate placeholders from `specs/_lighthouse/recruitment-shortlist.md`.

---

## 5. Cross-references

- `specs/_lighthouse/lighthouse-customer-program.md` — program framework.
- `specs/_lighthouse/recruitment-shortlist.md` — candidate list.
- `specs/_lighthouse/sla-attestation-template.md` — form to be signed.
- `specs/_lighthouse/case-study-templates/` — narrative skeletons.
- `specs/04_sprints/S20/work_items/WI-S20-004-*.md` — work item canonical.

---

**Fim LIGHTHOUSE-KIT-INDEX.**
