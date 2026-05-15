---
id: "ISO27001-GAP-ANALYSIS-2026-05-15"
type: "compliance_gap_analysis"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R5-3-ISO27001-CROSSWALK"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "ISO27001-CROSSWALK-2026-05-15"
  - "SOC2-GAP-ANALYSIS-2026-05-14"
tags:
  - "iso27001"
  - "iso27001-2022"
  - "gap-analysis"
  - "iso-unique"
  - "r-prep"
---

# ISO/IEC 27001:2022 — Gap Analysis (ISO-Unique vs SOC 2)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** isolate the controls that are **not** already evidenced via our SOC 2 Type I readiness program. These are the gaps an enterprise prospect asking "how far are you from ISO 27001?" needs to know about. Companion: `ISO27001-CROSSWALK-2026-05-15.md` (master 93-control mapping). Roadmap: `ISO27001-ROADMAP.md`.
>
> **Scope of "ISO-unique":** a gap is ISO-unique when (a) the control exists in Annex A:2022, (b) our current SOC 2 evidence stream **does not** produce auditor-grade evidence for it, and (c) no SOC 2 GAP-XX in `SOC2-GAP-ANALYSIS.md` already plans to close it. Controls covered by SOC 2 GAPs are tracked **there**, not duplicated here.

## 1. Method

1. Walked the 93 Annex A:2022 controls in `ISO27001-CROSSWALK-2026-05-15.md §2–§5`.
2. For each `Partial` row, asked: "does an existing SOC 2 GAP-XX in `SOC2-GAP-ANALYSIS.md` cover the residual work?" → if **yes**, the row stays on the SOC 2 register and is not duplicated here. If **no**, the gap is ISO-unique.
3. For each `Gap` row, the gap is ISO-unique by definition (SOC 2 wouldn't flag it).
4. `N/A` rows justified in the crosswalk are not gaps; SoA documents the inheritance / inapplicability.

**Result:** 7 active ISO-unique gaps (GAP-ISO-01..GAP-ISO-07) + 1 informational deferred gap (GAP-ISO-08 PIMS, only relevant if pursuing ISO 27701).

## 2. Legend

| Severity | Definition | Fix window for cert |
|---|---|---|
| **major** | Cert blocker for Stage 1 / Stage 2 audit; auditor would flag as major non-conformity | Must close before Stage 1 (Q4-2026) |
| **minor** | Auditor flag as minor non-conformity; acceptable to close before Stage 2 with documented corrective action | Stage 2 close-out (Q1-2027) |
| **informational** | Recommendation-grade; documented in SoA with justification | No hard deadline |

## 3. Active ISO-unique gaps

### GAP-ISO-01 — Formal asset register (A.5.9)

- **Annex A reference:** A.5.9 Inventory of information and other associated assets.
- **Why ISO-unique:** SOC 2 CC6.1 + asset-management points-of-focus accept an informal Drata-driven asset inventory (Cloudflare-discovered resources + GitHub repos + sub-processor list). ISO 27002:2022 §5.9 expects a **standalone asset register** enumerating (a) the asset, (b) its owner, (c) its classification per A.5.12, (d) its location, (e) acceptable use rules, (f) handling rules at end-of-life.
- **Current state:** asset inventory exists in three places — `specs/03_architecture/data_model.md`, Drata asset module, `legal/sub-processors.md` — but not consolidated into a single signed register with owner + classification per row.
- **Severity:** minor (auditor will accept consolidation as corrective action between Stage 1 and Stage 2).
- **Remediation:** create `specs/_compliance/ISO27001-ASSET-REGISTER.md` consolidating the three sources; one row per asset class (R2 bucket / D1 database / DO namespace / KV namespace / Worker / Pages / Container / sub-processor service / source repo / build artefact / audit log archive) × owner × classification × location × acceptable use × disposal rule.
- **Owner:** Compliance Officer.
- **Effort:** S (≤ 1 person-week).
- **ETA:** T+1m (post-SOC2-Type-I rollup; aligned with ISO roadmap M1).
- **Closes alongside:** none in SOC 2 register; standalone deliverable.
- **Compensating control:** Drata asset inventory module + `data_model.md` taxonomy substitute pre-register-completion.

### GAP-ISO-02 — Acceptable Use Policy formalized (A.5.10)

- **Annex A reference:** A.5.10 Acceptable use of information and other associated assets.
- **Why ISO-unique:** SOC 2 CC1.1 accepts owner-signed code-of-conduct embedded in `README.md §contributing` + DPA §confidentiality clauses. ISO 27002:2022 §5.10 expects a **standalone Acceptable Use Policy** signed by every employee + contractor at onboarding, separable from the code-of-conduct.
- **Current state:** AUP content exists implicitly in code-of-conduct + DPA + contractor templates; no single signed AUP document.
- **Severity:** minor.
- **Remediation:** issue `legal/policies/ACCEPTABLE-USE-POLICY.md` (template based on ISO 27002:2022 §5.10 implementation guidance + NIST SP 800-100); collect signatures via Drata HR module onboarding flow; one annual re-attestation cycle.
- **Owner:** Compliance Officer + Legal.
- **Effort:** S (≤ 1 person-week including legal review).
- **ETA:** T+1m.
- **Closes alongside:** GAP-04 (advisor pool sign-off — same legal-review batch).
- **Compensating control:** code-of-conduct + DPA confidentiality clauses pre-policy-issuance.

### GAP-ISO-03 — Supplier SLA contract review depth (A.5.20)

- **Annex A reference:** A.5.20 Addressing information security within supplier agreements.
- **Why ISO-unique:** SOC 2 CC9.2 evidence (DPA + sub-processor termination clauses + vendor risk register) demonstrates supplier oversight at the **aggregate** level. ISO 27002:2022 §5.20 expects a **per-contract** review checklist confirming security clauses: data classification handling, incident notification SLAs, audit rights, sub-processor change notice, BCP requirements, evidence-on-request commitments, return/destruction at termination.
- **Current state:** 19-vendor register exists (`VENDOR-RISK-REGISTER.md`); 11 vendors have DD files in `vendor-dd/`; DPA template `legal/dpa/v1.0.0` has standard clauses; but no per-contract security-clause checklist signed by Compliance Officer.
- **Severity:** minor.
- **Remediation:** issue `specs/_compliance/templates/SUPPLIER-AGREEMENT-CHECKLIST.md` (12-question checklist covering ISO 27002:2022 §5.20 PoF); run all 19 vendors through the checklist + sign-off; document residual gaps (typically: audit rights for hyperscalers — accepted compensating control = SOC 2 Type II inheritance).
- **Owner:** Compliance Officer + Legal.
- **Effort:** M (≤ 2 person-weeks across 19 vendors).
- **ETA:** T+2m.
- **Closes alongside:** GAP-21 (sub-processor change-notification automation — overlapping vendor workflow).
- **Compensating control:** DPA template covers ~70% of the §5.20 PoF; checklist closes the remaining 30%.

### GAP-ISO-04 — BCP testing depth (A.5.30 + A.8.14)

- **Annex A reference:** A.5.30 ICT readiness for business continuity + A.8.14 Redundancy of information processing facilities.
- **Why ISO-unique:** SOC 2 A1.2 + A1.3 satisfied by `BCP-DR-DRILL-CADENCE.md` (14 drills; DR-15 cold restore + DR-16 active failover + chaos region-outage + BYOK kill-switch). ISO 27031 (referenced by A.5.30) + ISO 22301 expect a **formal Business Impact Analysis (BIA)** with RTO/RPO per business process (not just per system), plus a documented BCP testing strategy across single-region / cross-region / supplier-outage / full-SEV1 scenarios.
- **Current state:** drill cadence documented + RTO/RPO defined per drill (DR-16: RTO 15min write-flip / RPO 5min); BIA per business process not formally written.
- **Severity:** minor.
- **Remediation:** issue `specs/_compliance/BUSINESS-IMPACT-ANALYSIS.md` covering 6 business processes (signup / upload / read / DSR / billing / audit-log-export) × RTO × RPO × MTPD (max tolerable period of disruption) × compensating controls during disruption; cross-reference each row to existing drill in `BCP-DR-DRILL-CADENCE.md`.
- **Owner:** SRE Lead + Compliance Officer.
- **Effort:** M (≤ 2 person-weeks).
- **ETA:** T+2m.
- **Closes alongside:** GAP-13 (DR drill cadence calendarization) + GAP-15 (cold-restore drill first cycle).
- **Compensating control:** existing 14-drill cadence covers the operational substance; BIA doc is the formalization layer.

### GAP-ISO-05 — Offboarding checklist (A.6.5)

- **Annex A reference:** A.6.5 Responsibilities after termination or change of employment.
- **Why ISO-unique:** SOC 2 CC6.3 + CC6.5 satisfied via Clerk de-provisioning + sub-processor termination clauses. ISO 27002:2022 §6.5 expects a **signed offboarding checklist** covering: returned assets, revoked access (per system), revoked credentials, NDA reaffirmed, knowledge transfer documented, exit interview (security-specific questions).
- **Current state:** offboarding handled ad-hoc (solo founder + small advisor pool); no signed checklist.
- **Severity:** minor.
- **Remediation:** issue `legal/policies/OFFBOARDING-CHECKLIST.md` (employee / contractor / advisor variants); embed in `RB-OFFBOARDING.md` runbook; tracked via Drata HR offboarding flow when first employee onboards post-GA.
- **Owner:** Compliance Officer.
- **Effort:** S (≤ 1 person-week).
- **ETA:** T+2m.
- **Closes alongside:** GAP-30 (onboarding security training tracking — same HR workflow).
- **Compensating control:** Clerk admin-plane de-provisioning is automatic; checklist is the documentation layer.

### GAP-ISO-06 — Clear-desk and clear-screen policy (A.7.7)

- **Annex A reference:** A.7.7 Clear desk and clear screen.
- **Why ISO-unique:** SOC 2 does not surface this control (remote-first orgs typically skip it). ISO 27002:2022 §7.7 expects a documented policy even for remote workers (workstation lock when away; no printed confidential materials at home unless secured; screen privacy filters in public spaces).
- **Current state:** no documented policy; informal practice only.
- **Severity:** minor (auditor will accept a thin policy + remote-workstation hygiene attestation).
- **Remediation:** issue `legal/policies/CLEAR-DESK-CLEAR-SCREEN-POLICY.md` (1-page; covers workstation auto-lock < 5min idle; screen privacy filter at conferences; secure shredding of any printed confidential material; ban on confidential-info in personal cloud accounts); attestation at onboarding.
- **Owner:** Compliance Officer.
- **Effort:** XS (≤ 0.25 person-week).
- **ETA:** T+1m.
- **Closes alongside:** GAP-ISO-02 (AUP — same policy-issuance batch).
- **Compensating control:** OS-level auto-lock + Cloudflare Access workstation-bind policy.

### GAP-ISO-07 — MDM / managed endpoint inventory (A.7.9 + A.8.1)

- **Annex A reference:** A.7.9 Security of assets off-premises + A.8.1 User end point devices.
- **Why ISO-unique:** SOC 2 CC6.1 accepts WebAuthn-bound admin sessions + Cloudflare Access workstation policy. ISO 27002:2022 §8.1 expects a **managed endpoint inventory** with remote-wipe capability, mandatory full-disk encryption (verified centrally), patch management posture (auto-update verified), and lost-device procedure.
- **Current state:** workstations encrypted via OS-native FDE (FileVault / BitLocker) verified at workstation onboarding; no central MDM (Jamf / Kandji / Intune) deployed.
- **Severity:** minor (auditor will accept manual attestation + Cloudflare Access posture-check for solo-founder org; expects formal MDM when org > 10 people).
- **Remediation:** for Q1-2027 cert with 1-3 workstations, document MDM-lite via Apple Business Manager / Google Endpoint + Cloudflare Access posture-check (FDE + OS up-to-date + screen-lock < 5min); plan formal Jamf/Kandji deployment when org headcount > 10.
- **Owner:** SRE Lead + Compliance Officer.
- **Effort:** S (≤ 1 person-week for MDM-lite documentation; L if formal MDM deployment).
- **ETA:** T+3m (MDM-lite doc for Q1-2027 cert); T+12m (formal MDM if org scales).
- **Closes alongside:** none in SOC 2 register.
- **Compensating control:** Cloudflare Access posture-check + WebAuthn-bound admin sessions + INV-ADMIN-MFA-FRESHNESS.

## 4. Informational / deferred gaps

### GAP-ISO-08 — ISO 27701 PIMS extension (A.5.34 overlap)

- **Annex A reference:** A.5.34 Privacy and protection of PII (note: ISO 27001:2022 has only this single privacy-headlined control; the full Privacy Information Management System lives in ISO 27701 as a separate extension standard).
- **Why deferred:** ISO 27001 certification **does not require** ISO 27701 PIMS controls. The substance (privacy by design, DPIA process, DSR machinery, consent receipt, breach notification) is already covered by our LGPD + GDPR + SOC 2 Privacy posture. Formally certifying to ISO 27701 is a separate engagement.
- **Severity:** informational.
- **Recommendation:** defer ISO 27701 PIMS certification to **2028** (post-SOC 2 Type II + post-ISO 27001 Stage 2); revisit if a customer RFP explicitly demands it.
- **Owner:** Privacy Officer (Gustavo interim).
- **Closes alongside:** N/A — separate cert track.

## 5. ISO-unique gap rollup

| ID | Annex A ref | Severity | Owner | Effort | ETA |
|---|---|---|---|---|---|
| GAP-ISO-01 | A.5.9 | minor | Compliance | S | T+1m |
| GAP-ISO-02 | A.5.10 | minor | Compliance + Legal | S | T+1m |
| GAP-ISO-03 | A.5.20 | minor | Compliance + Legal | M | T+2m |
| GAP-ISO-04 | A.5.30 + A.8.14 | minor | SRE + Compliance | M | T+2m |
| GAP-ISO-05 | A.6.5 | minor | Compliance | S | T+2m |
| GAP-ISO-06 | A.7.7 | minor | Compliance | XS | T+1m |
| GAP-ISO-07 | A.7.9 + A.8.1 | minor | SRE + Compliance | S | T+3m |
| GAP-ISO-08 | A.5.34 (PIMS) | informational | Privacy | — | deferred 2028 |

**Totals:** 7 active gaps (all minor) + 1 informational deferred · effort ≈ **6.25 person-weeks** total · all close before Q4-2026 Stage 1 audit window.

## 6. Cumulative effort vs SOC 2 baseline

| Track | Gaps | Person-weeks | Window |
|---|---:|---:|---|
| SOC 2 Type I (canonical 33-GAP register) | 33 | ~23.5 | D+30..T+6m |
| ISO 27001 unique additions (this doc) | 7 | ~6.25 | T+1m..T+3m |
| **Combined for SOC 2 + ISO 27001 cert pair** | **40** | **~29.75** | **D+30..T+6m** (ISO gaps slot inside the SOC 2 window) |

**Headline:** ISO 27001 adds only **~27% incremental effort** on top of SOC 2 Type I prep. The shared evidence collection (Drata 90.7% auto) covers both.

## 7. Cross-references

- **Master crosswalk:** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **Roadmap to Q1-2027 cert:** `specs/_compliance/ISO27001-ROADMAP.md`
- **SOC 2 gap register (33 GAPs):** `specs/_compliance/SOC2-GAP-ANALYSIS.md`
- **SOC 2 evidence rollup:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **ISO 27001 SoA baseline CSV:** `specs/_audits/iso27001-soa.csv`
- **Compliance matrix (Level 3):** `specs/03_architecture/compliance_matrix.md` §3

## 8. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 R-prep ISO 27001 agent) | Initial ISO-unique gap analysis isolating 7 active minor gaps + 1 informational deferred (PIMS); cumulative effort ~6.25 person-weeks on top of SOC 2 23.5; all close before Q4-2026 Stage 1 audit. |

---

**Fim ISO27001-GAP-ANALYSIS.**
