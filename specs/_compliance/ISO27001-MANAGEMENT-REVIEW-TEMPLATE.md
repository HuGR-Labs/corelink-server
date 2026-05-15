---
id: "ISO27001-MANAGEMENT-REVIEW-TEMPLATE-2026-05-15"
type: "compliance_management_review_template"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-DEBT-012-ISO27001-GAP-CLOSURE"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "ISO27001-INTERNAL-AUDIT-PROGRAM-2026-05-15"
  - "ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15"
  - "ISO27001-ROADMAP-2026-05-15"
tags:
  - "iso27001"
  - "iso27001-2022"
  - "clause-9-3"
  - "management-review"
  - "isms"
  - "r-prep"
---

# ISO/IEC 27001:2022 — Management Review Template (Clause 9.3)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** template for the management review meeting required by ISO/IEC 27001:2022 Clause 9.3. Each review instance copies this template and is preserved as `specs/_compliance/management-reviews/MGMT-REVIEW-<YYYY-MM-DD>.md` so the certification body has a historical chain. Companion: internal audit programme at `ISO27001-INTERNAL-AUDIT-PROGRAM.md`.

## 1. Header

| Field | Value |
|---|---|
| Standard reference | **ISO/IEC 27001:2022 Clause 9.3** (Management review) — Clause 9.3.1 General · 9.3.2 Inputs · 9.3.3 Outputs |
| Cadence | **Quarterly** management reviews + **annual** comprehensive review (aligned with audit programme cadence per `ISO27001-INTERNAL-AUDIT-PROGRAM.md §3`) |
| Chair | Owner (Gustavo Schneiter) |
| Secretary | Compliance Officer (Gustavo Schneiter interim) |
| Required attendees | Owner · Compliance Officer · SRE Lead · Privacy Officer · Security Lead (+ Internal Auditor for annual review) |
| Optional attendees | Advisor pool member (CISA / CIPP-E) for annual review · Legal for supplier / contractual items |
| Meeting duration | Quarterly: 90 min · Annual: half-day |
| Records retention | Minutes + decisions + action register: 7 years in R2 Object Lock Governance Mode (per CTRL-AUDIT-005) |
| First review | T+4m (per `ISO27001-ROADMAP.md` M3) |

---

## 2. Meeting metadata (copy + fill per instance)

| Field | Value (fill in) |
|---|---|
| Review instance ID | MGMT-REVIEW-YYYY-MM-DD |
| Date | YYYY-MM-DD |
| Type | Quarterly / Annual |
| Location | Virtual (Google Meet / Zoom — link captured) |
| Chair | Gustavo Schneiter (Owner) |
| Secretary | Compliance Officer |
| Attendees present | (names + roles) |
| Attendees absent | (names + reason) |
| Period under review | (e.g., 2026-Q3: 2026-07-01 .. 2026-09-30) |
| Prior review reference | (e.g., MGMT-REVIEW-2026-08-15 for the next one to reference back) |

---

## 3. Agenda (Clause 9.3.2 — required inputs)

The agenda **must** cover every required input listed in Clause 9.3.2. The template lists each topic + the evidence vehicle.

### 3.1 Status of actions from previous management reviews (Clause 9.3.2 a)

Walk the action register from the prior management review.

| Action ID | Owner | Target date | Status | Notes |
|---|---|---|---|---|
| MR-YYYY-Q#-### | — | — | open / in-flight / closed / overdue | — |
| (one row per open or recently-closed action) | | | | |

Decision required: confirm closures · re-target overdue actions · escalate to corrective action if pattern emerges.

### 3.2 Changes in external and internal issues relevant to the ISMS (Clause 9.3.2 b)

| Category | Change | Impact on ISMS | Action |
|---|---|---|---|
| Regulatory | (e.g., LGPD ANPD enforcement update; new EU AI Act provisions; CPRA amendments) | (low / med / high) | (open new risk-treatment ticket / update SoA / no action) |
| Technology | (e.g., Cloudflare Workers runtime upgrade; D1 replication GA; new BYOK provider) | — | — |
| Threat landscape | (per `cargo-audit` / CISA feeds / GitHub Security Advisories review) | — | — |
| Organizational | (e.g., headcount change crossing 10-employee threshold → formal MDM trigger per GAP-ISO-07) | — | — |
| Customer / contractual | (new RFP commitments; new sub-processor demand; new Tier-1 customer adding GDPR DPA) | — | — |

### 3.3 Performance of the ISMS — feedback on the information security performance (Clause 9.3.2 c)

Including trends in:

**3.3.1 Non-conformities and corrective actions**

| Source | Open count | Closed in period | Overdue | Trend vs prior period |
|---|---:|---:|---:|---|
| Internal audits (this period) | — | — | — | — |
| External pentest (last cycle) | — | — | — | — |
| Drata continuous compliance | — | — | — | — |
| SOC 2 audit findings | — | — | — | — |
| Customer security reviews | — | — | — | — |

**3.3.2 Monitoring and measurement results (Clause 9.1)**

| Metric | Target | Actual (period) | Status |
|---|---|---|---|
| Drata continuous-compliance green % | ≥ 95% | — | — |
| SLO catalog adherence | per `specs/03_architecture/observability_model.md` SLO catalog | — | — |
| Audit-chain integrity (INV-OBS-AUDIT-CHAIN-INTEGRITY) | 100% verified daily | — | — |
| Admin MFA freshness (INV-ADMIN-MFA-FRESHNESS) | 100% admin operations within freshness window | — | — |
| BYOK kill-switch drill RTO | ≤ 4 hours | — | — |
| DR-16 active-failover drill RTO | ≤ 15 minutes | — | — |
| DSR erasure SLA (CTRL-PRIV-014) | ≤ 30 days (GDPR / LGPD) | — | — |
| Vulnerability management SLA per severity | per GAP-29 once closed | — | — |
| Mean-time-to-detect (MTTD) for SEV-1 | ≤ 15 minutes | — | — |
| Mean-time-to-respond (MTTR) for SEV-1 | ≤ 1 hour | — | — |

**3.3.3 Audit results**

- Internal audit reports issued in period (per `ISO27001-INTERNAL-AUDIT-PROGRAM.md §3`).
- External audit results: SOC 2 (annual) · pentest (annual) · surveillance audit (post-cert).

**3.3.4 Fulfilment of information security objectives**

| Objective (per `specs/_governance/`) | Target | Status |
|---|---|---|
| ISO 27001 Stage 1 audit-ready by Q4-2026 | per `ISO27001-ROADMAP.md` M4 | — |
| SOC 2 Type I report issued Q1-2027 | per `SOC2-ROADMAP.md` | — |
| Zero data residency leaks YTD | 0 | — |
| Zero unauthorized admin-plane changes YTD | 0 (enforced by PAT-DUAL-APPROVAL-001 + INV-ADMIN-DUAL-APPROVAL) | — |
| Zero unsigned deploys YTD | 0 (enforced by CTRL-SUPPLY-002 + Cosign verification) | — |

### 3.4 Feedback from interested parties (Clause 9.3.2 d)

| Interested party | Channel | Feedback in period | Action |
|---|---|---|---|
| Tier-1 customers | RFP / SIG / CAIQ responses + security reviews | — | — |
| Regulators | ANPD / CNIL / ICO interactions (if any) | — | — |
| Sub-processors | SOC 2 refresh notifications + change notices | — | — |
| Advisor pool | quarterly review + ad-hoc memo | — | — |
| Internal team | retros + incident postmortems | — | — |

### 3.5 Results of risk assessment and status of risk treatment plan (Clause 9.3.2 e)

| Risk treatment item | Risk ID | Status | Residual risk | Decision |
|---|---|---|---|---|
| (per `specs/_audits/matrix-stride-ctrl.csv` STRIDE/LINDDUN matrix) | — | open / in-flight / closed | low / med / high | accept / mitigate further / transfer |

### 3.6 Opportunities for continual improvement (Clause 9.3.2 f)

- Process improvements surfaced by internal audits.
- Automation candidates (manual evidence → Drata auto-collection).
- Tooling upgrades (e.g., MDM formalization post-GAP-ISO-07; Vault auth-modes hardening).
- Skills / training gaps surfaced by adversarial summaries.

---

## 4. Outputs (Clause 9.3.3)

Per Clause 9.3.3, management review outputs **must** include decisions on:

### 4.1 Opportunities for continual improvement — decided

| Improvement | Decision | Owner | Target date | Action ID |
|---|---|---|---|---|
| (item from §3.6) | approved / deferred / rejected | — | — | MR-YYYY-Q#-### |

### 4.2 Need for changes to the ISMS — decided

| ISMS change | Decision | Owner | Target date | Action ID |
|---|---|---|---|---|
| (e.g., SoA update — new control applicable / change of applicability) | approved / deferred / rejected | — | — | — |
| (e.g., scope change — new sub-processor) | — | — | — | — |
| (e.g., policy revision — AUP version bump) | — | — | — | — |

### 4.3 Resource needs (Clause 9.3.3)

| Resource | Justification | Decision | Owner | Target |
|---|---|---|---|---|
| (e.g., formal MDM tool license once headcount > 10) | GAP-ISO-07 trigger | approve / defer | — | — |
| (e.g., dedicated internal auditor role when org > 25) | Independence requirement strengthens at scale | approve / defer | — | — |

---

## 5. Action register (carry forward to next review)

| Action ID | Description | Owner | Target date | Status |
|---|---|---|---|---|
| MR-YYYY-Q#-001 | — | — | — | open |
| MR-YYYY-Q#-002 | — | — | — | open |

Actions are tracked weekly by Compliance Officer + reviewed at next management review per §3.1.

---

## 6. Sign-off

| Role | Name | Signature (e-sign) | Date |
|---|---|---|---|
| Chair (Owner) | Gustavo Schneiter | — | YYYY-MM-DD |
| Secretary (Compliance Officer) | — | — | YYYY-MM-DD |

> Sign-off captured via Drata document attestation OR via GPG-signed commit of the rendered minutes file `specs/_compliance/management-reviews/MGMT-REVIEW-<YYYY-MM-DD>.md`.

---

## 7. First-cycle execution plan (feeds Stage 1 audit Q4-2026)

| Review | Date target | Type | Inputs available |
|---|---|---|---|
| MGMT-REVIEW-2026-Q3 | T+4m (≈ 2026-09-15) | Quarterly | First 2 monthly process audits + first quarterly deep audit |
| MGMT-REVIEW-2026-Q4 | T+7m (≈ 2026-12-15) | Quarterly | 5 monthly audits + Q3 quarterly audit + Stage 1 audit findings (if available) |
| MGMT-REVIEW-2027-Q1 (Annual) | T+10m (≈ 2027-03-15) | Annual | Full 12-month internal audit cycle + Stage 1 corrective-action closure + Stage 2 audit findings |
| MGMT-REVIEW-2027-Q2 | T+13m (≈ 2027-06-15) | Quarterly | Post-certification surveillance prep |

---

## 8. Cross-references

- **Internal audit programme (Clause 9.2):** `specs/_compliance/ISO27001-INTERNAL-AUDIT-PROGRAM.md`
- **SoA (93 Annex A rows + 3 exclusions):** `specs/_compliance/ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md`
- **Roadmap to Q1-2027 cert (M3 + M5 + M6 align with this template):** `specs/_compliance/ISO27001-ROADMAP.md`
- **Gap analysis (7 ISO-unique + 1 informational PIMS):** `specs/_compliance/ISO27001-GAP-ANALYSIS.md`
- **Crosswalk (control-by-control evidence pointers):** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **SOC 2 evidence rollup (shared metrics):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **Risk matrix (STRIDE/LINDDUN):** `specs/_audits/matrix-stride-ctrl.csv`
- **SLO catalog (Clause 9.1 measurement):** `specs/03_architecture/observability_model.md`

---

## 9. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 DEBT-012 closure agent) | Initial management review template satisfying ISO/IEC 27001:2022 Clause 9.3 (inputs Clause 9.3.2 a–f + outputs Clause 9.3.3). Quarterly + annual cadence; aligned with internal audit programme `ISO27001-INTERNAL-AUDIT-PROGRAM.md`. First review T+4m (Q3-2026) feeds Stage 1 audit Q4-2026 per `ISO27001-ROADMAP.md` M3. |

---

**Fim ISO27001-MANAGEMENT-REVIEW-TEMPLATE.**
