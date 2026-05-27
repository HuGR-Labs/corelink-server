---
id: "LIGHTHOUSE-CUSTOMER-PROGRAM"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "ga", "s20", "customer-program", "team-tier", "enterprise-byok", "sla", "30d-observation", "governance"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-lighthouse-tracker` was absorbed into `corelink-telemetry` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md. Canonical consumer path is now `corelink_telemetry::*`.

# Lighthouse Customer Program — Framework (WI-S20-004)

> **Slots:** 2 Team tier + 1 Enterprise BYOK = **3 lighthouse customers**
> **Timeline:** D+0 recruit → D+45 case-study signed (per cohort)
> **Spec contract gate:** §10.s20.7 (GA Evidence Gate D+60) — 3/3 SLA met sustained 30d

---

## 1. Purpose

CoreLink GA cannot ship without **3 production-grade lighthouse customers** who have signed off on a 30-day SLA observation period. This document is the canonical playbook for recruiting, onboarding, observing, and attesting those customers, plus the value exchange we offer in return.

The Rust state machine (`crates/corelink-lighthouse-tracker`) + D1 migration `0042_lighthouse_customers.sql` operationalize the tracking; this document is the human-process layer.

---

## 2. Slot allocation canonical

| Slot id (internal) | Tier | Source | Notes |
|---|---|---|---|
| `LH-FORGE` | Team | HuGR org first tenant activated | Customer-zero per spec contract §5.1 — product team self-served. |
| `LH-OSS-01` | Team | OSS Bazel/Buck2 maintainer outreach | External validation of REMOTE-CACHE-PRODUCT-PROFILE; 1 LOI of 5–10 candidates. |
| `LH-ENT-BYOK-01` | Enterprise BYOK | Sales founder outreach (10–20 ICP) | DPA amendment Schrems II TIA (S-14); AWS KMS primary; BYOK kill switch ≤ 5 min chaos-drill weekly. |

Backup slots (`LH-OSS-02`, `LH-OSS-03`, `LH-ENT-BYOK-02`, `LH-ENT-BYOK-03`) are recruited in parallel and held in `Engaged` state pending a primary withdrawal (FM-LIGHTHOUSE-CUSTOMER-DESISTS-MID-SPRINT).

**Waiver path** (spec contract §19): 3 → 2 lighthouse + plan to add 1 within 60d pós-GA is the only waivable variant, and requires ADR + CEO/Founder approval. The state machine in `corelink-lighthouse-tracker` does **not** auto-apply this waiver — it must be operator-driven.

---

## 3. Engagement timeline per cohort

| Phase | Window | Owner | Exit gate |
|---|---|---|---|
| **Recruit** | D+0..D+7 | Customer Success + Sales (founder for enterprise) | LOI signed; `LH-*` slot transitions `Recruiting → Engaged` |
| **Onboard** | D+7..D+10 | Engineer S-20 lead + Customer Success | Signup + DPA + tier + first PAT + first CAS PUT complete; `Engaged → Migrating → Observing` |
| **30d observation** | D+10..D+40 | SRE on-call (daily SLA sample) + Customer Success (weekly check-in) | No SLA breach recorded; `Observing → Attested` allowed |
| **SLA attestation** | D+40..D+45 | Customer signatory (CTO or Tech Lead) + Privacy Officer (DPA) + Legal Counsel | Attestation form signed + filed in `specs/_audits/2026-XX-XX-lighthouse-customer-{forge,oss,enterprise}-attestation.md` |
| **Case study sign-off** | D+45..D+50 | Marketing co-led + Customer Success + Legal Counsel | Customer-approved; sharable sob NDA |

The three cohorts run **staggered** (Forge D+0, OSS D+5, Enterprise D+10) so SRE on-call load is bounded and Customer Success engineer attention is sequential.

---

## 4. Value exchange (what we offer the customer)

For agreeing to be a lighthouse customer + signing the attestation + appearing in a case study, each customer receives:

1. **6 months free** on the Team tier (Forge + OSS) or Enterprise tier (BYOK customer).
2. **Dedicated onboarding engineer** for the first 30 days (Customer Success engineer + Engineer S-20 lead pairing).
3. **Direct PagerDuty paging rights** during the 30d observation window (lighthouse customers are P0 priority — see `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md`).
4. **Co-authored case study** with sanitized technical architecture diagram; customer reviews + approves before publication.
5. **Roadmap influence** — quarterly product roadmap review call for the first 12 months post-attestation.

In return:
1. **Signed SLA attestation** at D+45 (see `sla-attestation-template.md`).
2. **Public testimonial** OR sanitized internal testimonial (OSS = public; Enterprise = NDA-sanitized).
3. **Case study participation** — pre-CoreLink baseline, migration experience, 30d results, future plans.
4. **Reference call participation** — up to 2 reference calls per quarter for the first 12 months (sales acceleration).

This package was calibrated against typical enterprise SaaS lighthouse exchanges (Stripe, Vercel, PlanetScale early-stage) and the Pre-S20 Tech Discovery interviews.

---

## 5. Eligibility & evaluation criteria

### 5.1 Team tier (Forge + OSS)

- Active Bazel **or** Buck2 **or** Pants build setup ≥ 6 months (rules out toy/POC users).
- Existing remote cache backend (S3 + bazel-remote-cache, Buck2 cas client, custom impl) — proves they have the problem CoreLink solves.
- Engineering team ≥ 3 (proves shared cache provides material value).
- Maintainer / engineering lead willing to spend ≤ 8h on attestation + case study work.

### 5.2 Enterprise BYOK

- Compliance-driven posture (FedRAMP-ready, EU data-residency, financial services, healthcare).
- Existing KMS adoption (AWS KMS, GCP KMS, Azure KV, HashiCorp Vault) — BYOK is a real requirement, not a checkbox.
- DPA review capacity within 3 weeks of engagement (Schrems II TIA is non-trivial).
- $50k+ annual contract size (justifies dedicated engineer + 6mo free).

### 5.3 Disqualifiers (hard NOs)

- Anyone subject to OFAC sanctions or in jurisdictions where CoreLink data residency cannot be honored.
- Competitors (BuildBuddy, EngFlow, Bazel Remote Execution providers) — IP conflict.
- Customers requiring SLA stronger than SLO catalog defaults (out of scope at GA).
- Open-source projects with < 100 GitHub stars or < 5 active contributors (insufficient ecosystem validation signal).

---

## 6. State machine + tracker bindings

The Rust state machine (`crates/corelink-lighthouse-tracker`) is the source of truth for slot state. Operator-driven transitions:

```
Recruiting ──► Engaged ──► Migrating ──► Observing ──► Attested ──► CaseStudySigned
     │            │            │              │
     └─► Withdrawn ◄────────────┘              └► (SLA miss) ──► remediation cycle
```

Each transition is fail-CLOSED — illegal attempts return `TrackerError::IllegalTransition` and emit `corelink_lighthouse_illegal_transition_total{customer_id, from_state, to_state}` for audit.

The **`Observing → Attested`** gate requires:
1. `now - observation_started_at ≥ 30 days` (canonical 2 592 000 seconds, not calendar months).
2. `sla_breach_recorded == false` over the observation window.
3. For Enterprise BYOK only: `byok_key_health_ok == true` at attestation time.

If any gate fails, the operator must run a remediation cycle (separate `Engaged → Migrating → Observing` re-entry on a fresh 30d window).

---

## 7. Observability

Per spec contract §12 + DASH-LIGHTHOUSE-CUSTOMERS panel (embedded in DASH-GA-READINESS):

| Metric | Type | Labels | Alert |
|---|---|---|---|
| `corelink_lighthouse_customer_state_gauge` | Gauge | `customer_id, tier` | none |
| `corelink_lighthouse_customer_sla_claim_met_total` | Counter | `customer_id, tier` | none |
| `corelink_lighthouse_customer_slo_violation_total` | Counter | `customer_id, slo_id, tier` | > 0 sustained 7d → page SRE on-call |
| `corelink_lighthouse_customer_observation_status_gauge` | Gauge | `customer_id, tier` (0=blocked, 1=in_progress, 2=sla_met, 3=attested) | none |
| `corelink_lighthouse_illegal_transition_total` | Counter | `customer_id, from_state, to_state` | > 0 → SRE-lead page (operator bug or audit-chain corruption) |
| `corelink_lighthouse_customer_migration_duration_days_bucket` | Histogram | `customer_id, tier` | none |

Cardinality budget: at most **3 active rows** — per INV-OBS-CARDINALITY-BUDGET canonical limited exception per sprint.md §11.

---

## 8. Audit trail

Each lifecycle transition emits an audit row (S-09 audit chain) with:

- `event_type = lighthouse_lifecycle_transition`
- `customer_id = LH-<slot>` (zero PII — internal slot id only)
- `from_state`, `to_state`
- `operator_id = op_<alias>`
- `evidence_url` — for `Migrating → Observing` this is the first-CAS-PUT receipt; for `Observing → Attested` this is the signed attestation form URL.

Append-only; chain integrity verified per CTRL-AUDIT-001..003.

---

## 9. Cross-references

- `specs/_lighthouse/recruitment-shortlist.md` — candidate companies + outreach plan.
- `specs/_lighthouse/sla-attestation-template.md` — 30-day attestation form (customer signs).
- `specs/_lighthouse/case-study-templates/` — 3 case study templates.
- `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` — incident playbook (P0).
- `crates/corelink-lighthouse-tracker/` — Rust state machine.
- `migrations/d1/0042_lighthouse_customers.sql` — D1 persistence.

---

**Fim LIGHTHOUSE-CUSTOMER-PROGRAM.**
