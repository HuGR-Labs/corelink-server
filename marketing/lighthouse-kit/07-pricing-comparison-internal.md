---
id: "LIGHTHOUSE-KIT-07-PRICING-COMPARISON-INTERNAL"
type: "internal"
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
tags: ["lighthouse", "internal", "pricing", "comparison", "value-sizing", "not-for-customer"]
visibility: "INTERNAL_ONLY"
---

# 07 — Pricing Comparison (INTERNAL ONLY — NOT FOR CUSTOMERS)

> **DO NOT SHARE WITH CUSTOMERS.** This document sizes the value of the 6-month-free offer per lighthouse slot, for internal finance / founder visibility. Customer-facing pricing is delivered separately via Sales after the lighthouse window.
> **Currency convention:** all monetary amounts shown as `$X` placeholders — actual figures live in the Finance model, not this repo.
> **Purpose:** capture the internal logic for the lighthouse offer's economic shape so a future revision can be defended against a "why are we giving so much away?" question.

---

## 1. Why this exists

The lighthouse program offers each customer 6 months free on their tier plus dedicated engineering hours. To defend this offer internally — and to negotiate consistently with future lighthouse cohorts — we need a clear-eyed view of:

1. What that customer would have paid at list price for 6 months.
2. The cost-to-serve we eat (engineer hours + infra).
3. The intangible value (case study + reference call + product feedback) that justifies the gap.

This document records that math at the structural level, with `$X` placeholders. The numeric model lives in `_archive/finance/lighthouse-economics.xlsx` (not in the spec repo).

---

## 2. Per-slot value sizing

### 2.1 LH-FORGE (Team tier; HuGR Forge customer-zero)

| Line item | 6-month value | Notes |
|---|---|---|
| Foregone list-price revenue (Team tier × est. usage) | `$X` | Internal customer; "list price" is an accounting transfer, not a cash item |
| Dedicated CS engineer time (est. 30h over 60d) | `$X` | Burdened hourly rate × hours |
| Engineer S-20 lead time (est. 20h over 60d) | `$X` | Burdened hourly rate × hours |
| Infra cost-to-serve (compute + R2 storage + KMS calls) | `$X` | Standard cost-to-serve model |
| **Total cost** | `$X` | Sum of above |
| Intangible value captured | "customer-zero" credibility | n/a |

**Notes:** Forge is an internal customer; the "free" framing is accounting only. The real cost is the engineering time, which we would have spent regardless of the lighthouse program.

### 2.2 LH-OSS-01 (Team tier; external OSS)

| Line item | 6-month value | Notes |
|---|---|---|
| Foregone list-price revenue (Team tier × est. usage) | `$X` | Est. usage based on candidate's contributor count + monthly CI minutes (from `specs/_lighthouse/recruitment-shortlist.md` candidate profile) |
| Dedicated CS engineer time (est. 30h over 60d) | `$X` | Burdened hourly rate × hours |
| Engineer S-20 lead time (est. 15h over 60d) | `$X` | OSS integrations tend to be simpler than Enterprise |
| Marketing case-study production (est. 20h) | `$X` | Writer + designer + review cycles |
| Infra cost-to-serve | `$X` | Standard cost-to-serve model |
| **Total cost** | `$X` | Sum of above |
| Intangible value captured | OSS community validation; public testimonial; ecosystem signal | n/a |

**Notes:** the case study is the primary deliverable here. Without a case study, this slot fails its purpose even if the SLA is signed.

### 2.3 LH-ENT-BYOK-01 (Enterprise BYOK)

| Line item | 6-month value | Notes |
|---|---|---|
| Foregone list-price revenue (Enterprise BYOK tier × est. usage) | `$X` | Internal Sales pricing band; customer's est. usage from scoping call |
| Founder time (est. 25h over 60d) | `$X` | Founder rate; outreach + DPA review + ongoing exec sponsorship |
| Dedicated CS engineer time (est. 50h over 60d) | `$X` | Enterprise needs more handholding |
| Engineer S-20 lead time (est. 40h over 60d) | `$X` | BYOK setup + chaos drills + DPA technical addenda |
| Privacy Officer time (est. 15h) | `$X` | Schrems II TIA + DPA amendment review |
| Legal Counsel time (est. 20h) | `$X` | DPA negotiation + attestation review + case-study sanitization |
| Infra cost-to-serve | `$X` | Enterprise tier carries higher infra cost (BYOK KMS calls, dedicated regions, higher-priority queues) |
| **Total cost** | `$X` | Sum of above |
| Intangible value captured | Enterprise validation; sanitized reference architecture; sales-pipeline accelerator; sub-processor + SOC 2 Type 1 + pentest narrative anchor | n/a |

**Notes:** Enterprise slot is by far the most expensive to serve but also the highest-value reference for the post-GA sales motion. The cost is justified only if we capture the case study + reference-call commitment.

---

## 3. Aggregate program economics

| Slot | Total cost (6mo) | Intangible value (qualitative) |
|---|---|---|
| LH-FORGE | `$X` | "We run on our own product" credibility |
| LH-OSS-01 | `$X` | OSS ecosystem validation + public testimonial |
| LH-ENT-BYOK-01 | `$X` | Enterprise reference + Sales pipeline accelerant |
| **Program total** | `$X` | **GA-gate unblock** |

The single most important framing is the last row: without 3 signed attestations, GA does not ship (per spec contract §10.s20.7). The program cost is therefore not "marketing spend" — it is "GA gate cost".

---

## 4. Decision rules for negotiating off the standard offer

| Customer ask | Default response | Approval needed if exceeded |
|---|---|---|
| Extend free window beyond 6 months | No | Founder + Finance for >+1 month |
| Adjust SLO targets in attestation | No (SLO catalog is canonical) | Engineer S-20 lead + spec contract waiver |
| Anonymize case study beyond NDA-sanitization (Enterprise) | Yes, default | n/a |
| Skip case study, keep attestation | No — case study is core deliverable | Founder for exceptional cases (1 max across cohort) |
| Add more reference calls than 2/quarter | Yes if mutual benefit | CS lead |
| Apply BYOK to a provider not in 4/4 matrix | No (out-of-scope at GA) | Engineering Council + spec contract waiver |
| Custom data residency beyond 4 regions | No (out-of-scope at GA) | Engineering Council + Roadmap committee |

---

## 5. Post-lighthouse pricing handover

At D+45 (attestation signed), Sales takes over commercial negotiation for the customer's post-free-period contract. The lighthouse engagement does NOT entitle the customer to:

- Lighthouse-pricing extension beyond 6 months.
- Discounted multi-year contracts (separate negotiation).
- SLA terms beyond standard MSA (the attestation is for the lighthouse window only; commercial SLA is the GA SLO catalog).

The Sales hand-off package consists of:
1. Signed LOI + attestation.
2. Customer Success engineer's engagement notes.
3. This document's slot section (redacted to remove `$X` figures the customer should not see — Sales recreates pricing from the live Finance model).
4. Roadmap commitment items captured during weekly check-ins.

---

## 6. Risks (internal)

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Lighthouse customer abuses 6mo free with extreme usage | M | M | Per-slot usage cap embedded in LOI (3× expected usage = flag for renegotiation) |
| Customer extracts case study + attestation, then churns | L | M | Reference-call commitment in LOI; non-honored = clawback clause |
| Cost-to-serve exceeds budget on Enterprise slot | M | M | Founder reviews monthly burn; CS hours capped at 80h over 60d (escalation if exceeded) |
| List-price reveal during outreach | M | L | No specific dollar amounts in customer-facing docs (this kit enforces) |
| Internal misalignment on which slots get which discount post-lighthouse | M | M | Sales hand-off package + this doc's §5 |

---

## 7. Audit trail

- This document lives at `marketing/lighthouse-kit/07-pricing-comparison-internal.md` and is **never** shared with customers.
- Customer-facing pricing artifacts (decks, MSAs, order forms) live elsewhere and are governed by Sales, not Marketing.
- The Finance numeric model with actual `$X` figures lives at `_archive/finance/lighthouse-economics.xlsx`. This repo only contains the structural placeholders.

---

**Fim 07-PRICING-COMPARISON-INTERNAL.** (INTERNAL ONLY)
