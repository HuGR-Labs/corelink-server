---
id: "VENDOR-RISK-METHODOLOGY-2026-05-15"
type: "compliance_methodology"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R5-3-GAP-14-VENDOR-RISK"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["SOC2-EVIDENCE-ROLLUP-2026-05-15"]
tags: ["soc2", "cc9.2", "vendor-risk", "methodology", "nist-aligned", "gap-14"]
---

# Vendor Risk Scoring Methodology

> **doc_status:** ACTIVE · **audit_status:** ACTIVE · **scope:** All CoreLink third-party SaaS and infrastructure vendors (≥ 15 entries in `VENDOR-RISK-REGISTER.md`).
>
> **Primary SOC 2 hook:** CC9.2 (Vendor and Business-Partner Risk Management).
> **Cross-framework alignment:** NIST SP 800-30 Rev. 1 (Risk Assessment), NIST SP 800-161r1 (C-SCRM), ISO 27036 (Supplier Relationships), GDPR Art. 28 (sub-processor due diligence).

---

## 1. Purpose and scope

This document defines **how CoreLink scores, categorises, and re-assesses third-party vendor risk**. It is the canonical reference cited from:

- `VENDOR-RISK-REGISTER.md` (per-vendor risk scores)
- `specs/_compliance/vendor-dd/*.md` (per-Critical-vendor due-diligence files)
- `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md` (90-day cycle)
- `specs/03_architecture/compliance_matrix.md` §CC9.2

Auditors should treat this doc as the **CoreLink-side definition of "vendor risk"** for SOC 2 Type I / Type II walkthroughs.

---

## 2. Vendor categorisation

Every vendor is placed in exactly one of three categories. Category drives review cadence, owner, and the threshold above which a residual-risk score escalates to the Risk Committee.

| Category | Definition | Examples | Cadence | Default owner |
|---|---|---|---|---|
| **Critical** | Vendor outage or compromise breaks CoreLink core data plane, control plane, billing, or auth. RTO ≤ 4h. Loss of vendor would trigger an SLA breach within one business day. | Cloudflare, AWS KMS, GCP KMS, Azure Key Vault, Stripe, Clerk, Drata | **Quarterly** (≥ 4×/yr) | VP-Sec |
| **Important** | Vendor outage degrades a customer-visible feature but does not break the data plane. RTO 4h..72h. | PagerDuty, Slack, HubSpot, GitHub Enterprise, Anthropic, OpenAI, Datadog, Grafana Cloud, Neon | **Bi-annual** (≥ 2×/yr) | Eng-Lead |
| **Standard** | Internal-facing tooling, dev infra, optional integrations. RTO > 72h tolerable. | SendGrid, Twilio, Atlassian Statuspage, Cookiebot, Sigstore, Dependency-Track (self-hosted), Linear, Notion | **Annual** (1×/yr) | Eng-Lead |

**Category override:** A vendor may be promoted (e.g., Standard → Important) if a data-flow review identifies a new PII pathway, or demoted if a control change makes the data flow non-material. Overrides require Risk-Committee approval and are logged in the register's "category history" column.

---

## 3. Inherent risk scoring

**Inherent risk = Impact × Likelihood**, both on a 1..5 scale, yielding a 1..25 inherent score.

### 3.1 Impact (1..5)

What happens if this vendor is compromised, breached, or has a sustained outage?

| Score | Definition | Example trigger |
|---|---|---|
| 5 | Catastrophic — multi-tenant data exfiltration, regulatory reportable breach, GA-blocker | Cloudflare account compromise; AWS KMS key material leak |
| 4 | Severe — control-plane data loss, BYOK envelope corruption, multi-customer downtime | Drata roll-up incorrect; GCP KMS region down |
| 3 | Major — feature-level outage, one-customer-cohort impact | PagerDuty alert delivery delay; HubSpot CRM data inconsistency |
| 2 | Moderate — internal-only impact, audit-trail gap recoverable from backups | Slack #alerts channel down; Datadog ingest lag |
| 1 | Minor — cosmetic, no customer-visible effect | Statuspage UI lag; Cookiebot dashboard outage |

### 3.2 Likelihood (1..5)

Probability of a meaningful security or availability event in the next 12 months, conditioned on **vendor maturity + threat intel + recent incident history**.

| Score | Definition | Anchor |
|---|---|---|
| 5 | Recurrent — vendor had ≥ 2 SEV-1 incidents in last 12 months | (none in our current pool) |
| 4 | Frequent — known unpatched CVE class; recent CISA advisory naming vendor | (case-by-case) |
| 3 | Plausible — vendor had ≥ 1 public incident in last 12 months | (e.g., post-incident review pending) |
| 2 | Unlikely — mature vendor, SOC 2 Type II, no recent material incidents | Cloudflare, Stripe, AWS, GCP, Azure |
| 1 | Remote — no recent incidents + multiple attestations + customer-side controls | Sigstore (open source, transparency log) |

### 3.3 Inherent risk bands

| Band | Score range | Treatment |
|---|---|---|
| Low | 1..5 | Standard monitoring; annual review sufficient |
| Moderate | 6..10 | Active monitoring; bi-annual review |
| High | 11..16 | Compensating controls required; quarterly review |
| Critical | 17..25 | Compensating controls + Risk-Committee monthly check-in; mandatory **second vendor** for failover |

---

## 4. Control-effectiveness factor (CEF)

Residual risk = inherent × CEF. CEF lives in `[0.10, 1.00]`. Lower CEF = stronger CoreLink-side mitigations.

| CEF | Conditions met (all of) |
|---|---|
| 0.10 | (a) vendor has SOC 2 Type II attested ≤ 12 months old, **and** (b) CoreLink has a documented compensating control mapped in `security_model.md`, **and** (c) we have a tested failover (second vendor or in-house path), **and** (d) all data shared with vendor is encrypted at rest *and* in transit with CoreLink-held keys |
| 0.25 | (a) + (b) + (c) but data shared is encrypted only with vendor-held keys |
| 0.40 | (a) + (b) but no tested failover |
| 0.60 | (a) only — attestation but no compensating control |
| 0.80 | Vendor attests but attestation is > 12 months old, or attestation is SOC 2 Type I only |
| 1.00 | No attestation, or attestation expired, or vendor has open SEV-1 incident |

CEF is recomputed on every quarterly review. Any **upward** movement (CEF gets worse) is an automatic flag for the Risk Committee.

---

## 5. Residual risk and escalation thresholds

Residual = Inherent × CEF, rounded to one decimal.

| Residual band | Score | Action |
|---|---|---|
| Acceptable | < 4.0 | Continue as-is |
| Monitor | 4.0..7.9 | Add to quarterly-review priority list; document plan to reduce |
| Mitigate | 8.0..12.9 | Risk-Committee review within 30 days; compensating-control plan required within 60 days |
| Escalate | ≥ 13.0 | Risk-Committee review within 7 days; vendor-replacement plan required within 90 days |

**Hard rule:** No Critical-category vendor may carry a residual score ≥ 8.0 across two consecutive quarterly reviews without a written exception signed by VP-Sec + Final Approver (Gustavo).

---

## 6. Re-assessment triggers

Outside the calendar cadence (Quarterly / Bi-annual / Annual), the following events trigger an **immediate ad-hoc review** of the affected vendor:

1. **Vendor data breach** — public disclosure or private notification under the vendor's DPA. Re-review within 5 business days.
2. **Vendor regulatory change** — vendor enters/exits a framework (e.g., loses ISO 27001, gains FedRAMP). Re-review within 14 days.
3. **Contract renewal** — DPA / SLA / MSA renewal triggers a full DD refresh.
4. **Control failure detected** — CoreLink-side control mapped to the vendor produces a SEV-1 audit log. Re-review within 5 business days.
5. **Sub-processor change** — vendor announces a new sub-processor (Art. 28.2 notification). Re-review within the 30-day customer-objection window.
6. **Geopolitical / sanctions event** — vendor or sub-processor falls into a sanctioned jurisdiction. Re-review within 48 hours.
7. **Pentest finding involving vendor** — external pentest names the vendor in a Critical / High finding. Re-review within 5 business days.

All ad-hoc reviews append to the register's "review history" and produce a delta entry in `RB-VENDOR-RISK-QUARTERLY-REVIEW.md`.

---

## 7. Evidence requirements per review

For every review (calendar or ad-hoc) the following artefacts must be collected and linked from the register row:

- **Attestation** — current SOC 2 (Type II preferred), ISO 27001, FedRAMP letter, or equivalent. Drata vendor module is the source of record where available.
- **DPA** — signed, current version, link to the executed PDF (or vendor's published DPA URL where customer signature is unilateral acceptance).
- **SLA** — service-level commitments incl. availability target and credit terms.
- **BAA** — when PHI is in scope (none for CoreLink today; documented for completeness).
- **Sub-processor list** — vendor's own sub-processor disclosure where applicable.
- **Incident history** — public incident postmortems from last 12 months; vendor-issued breach notices.
- **CEF justification** — explicit checklist of which `§4` conditions are met.

---

## 8. Governance

- **Owner of methodology document:** VP-Sec (Gustavo Schneiter during pre-GA).
- **Approver of methodology changes:** Risk Committee (Final Approver: Gustavo Schneiter).
- **Reviewers:** Compliance Officer, Privacy Officer, Eng-Lead.
- **Cadence of methodology review:** Annual, or whenever an audit finding recommends a change.
- **Version history:** tracked via `version`/`updated` frontmatter and git blame.

---

## 9. Cross-references

- `specs/_compliance/VENDOR-RISK-REGISTER.md` — applied scores per vendor
- `specs/_compliance/vendor-dd/` — per-Critical-vendor due-diligence files
- `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md` — operational 90-day cycle
- `specs/03_architecture/compliance_matrix.md` §CC9.2 — TSC mapping
- `legal/sub-processors.md` — public sub-processor disclosure (subset of register)
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` GAP-14 — gap originally raised by R5-3 rollup
