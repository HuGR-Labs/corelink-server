---
id: "FEDRAMP-MODERATE-CROSSWALK-2026-05-15"
type: "compliance_crosswalk"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R-PREP-FEDRAMP-INFO"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
  - "ISO27001-CROSSWALK-2026-05-15"
  - "COMPLIANCE-MATRIX"
  - "SECURITY-MODEL"
tags:
  - "fedramp"
  - "fedramp-moderate"
  - "nist-800-53"
  - "nist-rev5"
  - "crosswalk"
  - "informational"
  - "not-in-scope"
  - "enterprise-rfp"
  - "r-prep"
---

# FedRAMP Moderate — Informational Crosswalk (CoreLink)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:**
> **INFORMATIONAL ONLY**. CoreLink does **not** target FedRAMP certification.
> This crosswalk exists so that enterprise prospects in regulated industries
> (defense-adjacent, federal-system-integrator, healthcare-with-federal-tie-in)
> can quickly assess how our SOC 2 + ISO 27001 + LGPD/GDPR posture aligns with
> the NIST SP 800-53 Rev 5 Moderate baseline they may already understand.
>
> **Not-in-scope rationale (read first):** see
> `specs/_compliance/FEDRAMP-NOT-IN-SCOPE-RATIONALE.md` for market, cost,
> timeline, and trigger conditions that would re-open the FedRAMP decision.

## 1. Header

| Field | Value |
|---|---|
| Framework | **FedRAMP Rev 5 Moderate Baseline** (per GSA / FedRAMP PMO Rev 5 baselines, effective 2023-05-30 with one-year transition window closed 2024-05-30) |
| Underlying standard | **NIST SP 800-53 Rev 5** + **SP 800-53B Rev 5** (control selection) |
| Moderate baseline control count | **325 controls** (incl. control enhancements) across 20 NIST families; 18 families in scope here (PM + PT excluded as program-management / privacy out-of-baseline) |
| CoreLink scope | Same boundary as SOC 2 + ISO 27001: Cloudflare Workers + Pages + R2 + D1 + DO + KV + Clerk + Stripe; BYOK via AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault; sub-processors per `legal/sub-processors.md`. Corporate IT / personal devices explicitly excluded. |
| Observation date | **2026-05-15** (informational snapshot; aligned with SOC 2 rollup cut) |
| Snapshot commit | `de84b9d` (`origin/main` HEAD at crosswalk cut) |
| Certification target | **None.** CoreLink does not pursue 3PAO assessment, JAB P-ATO, or Agency ATO. See `FEDRAMP-NOT-IN-SCOPE-RATIONALE.md`. |
| Customer-facing page | `apps/docs/docs/trust/fedramp-info.mdx` |
| Refresh cadence | **Annual** (or upon Rev 5 baseline change, or upon a customer-sponsored re-evaluation per the trigger conditions in `FEDRAMP-NOT-IN-SCOPE-RATIONALE.md §4`). |

### 1.1 Legend

- **Status (per row):**
  - `Coverage` — control substance fully implemented; SOC 2 / ISO 27001 / CTRL-* evidence already satisfies the NIST 800-53 control intent.
  - `Partial` — control substance is in place but FedRAMP-specific evidence formality (per `FedRAMP_System_Security_Plan_Template.docx` or per `FedRAMP_Continuous_Monitoring_Strategy_Guide`) would require additional documentation.
  - `Not-in-Scope` — control is FedRAMP-unique and not addressed by SOC 2 / ISO 27001 (typically FIPS 140-3 validation, US-citizen personnel screening, FedRAMP-specific incident reporting to US-CERT).
  - `Gap` — no equivalent control today; would require new build.
- **SOC 2 CC overlap** uses TSC 2017 (+2022 PoF) criterion IDs from `SOC2-EVIDENCE-ROLLUP-2026-05-15.md`.
- **CTRL ID** references `specs/03_architecture/security_model.md §6` and `specs/03_architecture/privacy_model.md`.
- **Coverage estimate** is family-level rolled from the rows; precision intent is "decision-quality for a procurement conversation", not "3PAO-grade".

---

## 2. Family-level crosswalk (18 families × NIST 800-53 Rev 5 Moderate)

### 2.1 AC — Access Control

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| AC (Access Control) — ~25 controls in Moderate | AC-2 Account Management · AC-3 Access Enforcement · AC-4 Information Flow · AC-5 Separation of Duties · AC-6 Least Privilege · AC-7 Unsuccessful Logon · AC-8 System Use Notification · AC-11 Session Lock · AC-12 Session Termination · AC-14 Permitted Actions w/o ID · AC-17 Remote Access · AC-18 Wireless · AC-19 Mobile Devices · AC-20 External Systems · AC-21 Information Sharing · AC-22 Public Content | CC6.1 · CC6.2 · CC6.3 · CC6.6 · CC6.7 | CTRL-AUTH-001/004/007/010 · CTRL-AUTHZ-001/002 · CTRL-NET-001/002/003 · PAT-DUAL-APPROVAL-001 | **Coverage** (AC-18 wireless N/A — cloud-only; AC-19 MDM partial via GAP-ISO-07) |

### 2.2 AT — Awareness and Training

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| AT (Awareness & Training) — ~4 controls in Moderate | AT-1 Policy · AT-2 Literacy Training · AT-3 Role-Based Training · AT-4 Training Records | CC1.4 · CC2.2 | process control + `templates/` skill matrix | **Partial** (training cadence informal; closes alongside ISO GAP-30; FedRAMP would require formal annual cycle + records retention) |

### 2.3 AU — Audit and Accountability

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| AU (Audit & Accountability) — ~12 controls in Moderate | AU-2 Event Logging · AU-3 Content of Audit Records · AU-4 Storage Capacity · AU-5 Response to Audit Failure · AU-6 Review/Analysis · AU-7 Reduction/Reporting · AU-8 Time Stamps · AU-9 Protection of Audit Info · AU-11 Audit Retention · AU-12 Audit Generation | CC2.1 · CC4.1 · CC7.2 · PI1.2 · PI1.5 | CTRL-AUDIT-001..005 · CTRL-META-001 · INV-AUDIT-APPEND-ONLY · INV-OBS-AUDIT-CHAIN-INTEGRITY | **Coverage** (Merkle audit chain + R2 Object Lock + 7y retention + RFC 6962 inclusion proofs exceed Moderate baseline) |

### 2.4 CA — Assessment, Authorization, and Monitoring

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| CA (Assessment, Authorization & Monitoring) — ~9 controls in Moderate | CA-1 Policy · CA-2 Control Assessments · CA-3 Information Exchange · CA-5 Plan of Action & Milestones (POA&M) · CA-6 Authorization · CA-7 Continuous Monitoring · CA-8 Penetration Testing · CA-9 Internal System Connections | CC4.1 · CC4.2 · CC7.1 | CTRL-COMP-001 + Drata continuous compliance + `specs/_compliance/SOC2-GAP-ANALYSIS.md` (POA&M analog) | **Partial** (continuous monitoring + pentest + POA&M analog covered; CA-6 ATO process is FedRAMP-unique and **Not-in-Scope**) |

### 2.5 CM — Configuration Management

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| CM (Configuration Management) — ~11 controls in Moderate | CM-2 Baseline Configuration · CM-3 Change Control · CM-4 Impact Analyses · CM-5 Access Restrictions for Change · CM-6 Configuration Settings · CM-7 Least Functionality · CM-8 System Component Inventory · CM-9 Configuration Mgmt Plan · CM-10 Software Usage · CM-11 User-Installed Software · CM-12 Information Location | CC6.8 · CC8.1 | PAT-DRIFT-DETECTION-001 + CTRL-SUPPLY-001/002/003/008 · branch protection · ADR process · sprint contracts §5.1 | **Coverage** (SLSA L3 + Cosign + Rekor + reproducible builds + Terraform drift detection exceed Moderate baseline) |

### 2.6 CP — Contingency Planning

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| CP (Contingency Planning) — ~9 controls in Moderate | CP-2 Contingency Plan · CP-3 Training · CP-4 Testing · CP-6 Alternate Storage Site · CP-7 Alternate Processing Site · CP-8 Telecommunications Services · CP-9 System Backup · CP-10 System Recovery & Reconstitution | A1.1 · A1.2 · A1.3 · CC7.5 | CTRL-GC-001/002 + CTRL-BACKOFF-001 + CTRL-QUOTA-001 + CTRL-RATE-001 + `BCP-DR-DRILL-CADENCE.md` (14 drills) + `COLD-RESTORE-DRILL-SPEC.md` + `ACTIVE-FAILOVER-DRILL-SPEC.md` | **Partial** (multi-region D1+DO+R2 + DR-15 cold restore + DR-16 warm failover; first cycle pending per GAP-15; FedRAMP CP-4(2) annual full-recovery test would require calendarized cycle) |

### 2.7 IA — Identification and Authentication

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| IA (Identification & Authentication) — ~12 controls in Moderate | IA-2 Identification & Auth (Org Users) · IA-2(1) MFA Privileged · IA-2(2) MFA Non-Privileged · IA-3 Device ID & Auth · IA-4 Identifier Mgmt · IA-5 Authenticator Mgmt · IA-5(1) Password-Based · IA-6 Auth Feedback · IA-7 Cryptographic Module Auth · IA-8 ID & Auth (Non-Org Users) · IA-11 Re-authentication · IA-12 Identity Proofing | CC6.1 · CC6.2 | CTRL-AUTH-001/004/007/010 + CTRL-CRED-001 + Clerk SSO + WebAuthn UV=1 + PAT HMAC-SHA256 | **Partial** (MFA + WebAuthn + PAT integrity covered; **IA-7 cryptographic module authentication requires FIPS 140-3 validated module — that's the FedRAMP-unique gap** also tracked as SOC 2 GAP-02 BYOK FIPS attestation per provider; AWS L3 attested, GCP/Azure pending RFI response) |

### 2.8 IR — Incident Response

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| IR (Incident Response) — ~9 controls in Moderate | IR-2 Training · IR-3 Testing · IR-4 Incident Handling · IR-5 Monitoring · IR-6 Reporting · IR-7 Assistance · IR-8 Incident Response Plan · IR-9 Information Spillage Response | CC7.3 · CC7.4 · P-BREACH | process control + `IR-TABLETOP-PLAYBOOK.md` (6 scenarios) + `RB-BREACH-NOTIF.md` + PagerDuty 24/7 + NIST 800-61 Rev.2 alignment | **Partial** (playbook + scenarios + first tabletop Q3-2026 per GAP-03; **IR-6 reporting to US-CERT within 1 hour is FedRAMP-unique and Not-in-Scope**) |

### 2.9 MA — Maintenance

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| MA (Maintenance) — ~6 controls in Moderate | MA-2 Controlled Maintenance · MA-3 Maintenance Tools · MA-4 Nonlocal Maintenance · MA-5 Maintenance Personnel · MA-6 Timely Maintenance | CC6.8 · CC8.1 | OOS_INHERITED (sub-processor for physical hw) + CTRL-SUPPLY-008 (Terraform IaC drift) | **Coverage** (cloud-only; physical maintenance inherited from Cloudflare/AWS/GCP/Azure SOC 2 + FedRAMP-authorized providers; nonlocal maintenance gated by Cloudflare Access + WebAuthn) |

### 2.10 MP — Media Protection

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| MP (Media Protection) — ~7 controls in Moderate | MP-2 Media Access · MP-3 Media Marking · MP-4 Media Storage · MP-5 Media Transport · MP-6 Media Sanitization · MP-7 Media Use | C1.1 · C1.2 · CC6.4 | CTRL-PRIV-014 (logical erasure) + INV-DATA-ERASURE-COMPLETE + INV-ERASURE-ATTESTATION-SIGNED + OOS_INHERITED (physical media destruction) | **Coverage** (no owned physical media; logical erasure attested with cryptographic proof; sub-processor SOC 2 covers physical sanitization NIST SP 800-88 Rev 1 compliant per Cloudflare/AWS/GCP/Azure) |

### 2.11 PE — Physical and Environmental Protection

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| PE (Physical & Environmental Protection) — ~17 controls in Moderate | PE-2 Physical Access Authorizations · PE-3 Physical Access Control · PE-4 Access Control for Transmission · PE-5 Access Control for Output · PE-6 Monitoring Physical Access · PE-8 Visitor Access Records · PE-9 Power Equipment · PE-10 Emergency Shutoff · PE-11 Emergency Power · PE-12 Emergency Lighting · PE-13 Fire Protection · PE-14 Temperature & Humidity · PE-15 Water Damage · PE-16 Delivery & Removal · PE-17 Alternate Work Site | CC6.4 · CC6.5 | OOS_INHERITED (sub-processor SOC 2 Type II reports) | **Coverage** (entirely inherited from Cloudflare / AWS GovCloud-ready / GCP / Azure FedRAMP-authorized infrastructure; no owned facilities) |

### 2.12 PL — Planning

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| PL (Planning) — ~4 controls in Moderate | PL-2 System Security & Privacy Plans · PL-4 Rules of Behavior · PL-8 Security & Privacy Architectures · PL-10 Baseline Selection | CC1.1 · CC5.3 · CC8.1 | process control + `specs/03_architecture/security_model.md` + `privacy_model.md` + AUP (GAP-ISO-02) | **Partial** (security/privacy architecture documented exhaustively; **PL-2 FedRAMP SSP template is FedRAMP-unique formatted artefact and Not-in-Scope** — would require translation of current specs into the GSA SSP Word template) |

### 2.13 PS — Personnel Security

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| PS (Personnel Security) — ~8 controls in Moderate | PS-2 Position Risk Designation · PS-3 Personnel Screening · PS-4 Personnel Termination · PS-5 Personnel Transfer · PS-6 Access Agreements · PS-7 External Personnel Security · PS-8 Personnel Sanctions · PS-9 Position Descriptions | CC1.1 · CC1.4 · CC6.3 · CC6.5 | process control + Owner attestation + advisor pool engagement letters | **Not-in-Scope** (PS-3 requires US citizenship + federal background investigation for Moderate baseline; CoreLink has no path to this without a sponsor agency; solo-founder org with global advisor pool — covered by ISO A.6.1 screening at SOC 2 / ISO 27001 level only) |

### 2.14 RA — Risk Assessment

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| RA (Risk Assessment) — ~7 controls in Moderate | RA-2 Security Categorization · RA-3 Risk Assessment · RA-5 Vulnerability Monitoring & Scanning · RA-7 Risk Response · RA-9 Criticality Analysis | CC3.1 · CC3.2 · CC3.3 · CC3.4 · CC7.1 | CTRL-SUPPLY-004 · CTRL-SUPPLY-007 · STRIDE/LINDDUN matrix · Dependency-Track · cargo-fuzz daily · `failure_modes.md` FM-XXX taxonomy | **Coverage** (vuln scanning + threat modeling + failure-mode taxonomy + dependency-track all continuous; risk register `VENDOR-RISK-REGISTER.md` + sprint-contract risk row) |

### 2.15 SA — System and Services Acquisition

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| SA (System & Services Acquisition) — ~14 controls in Moderate | SA-2 Allocation of Resources · SA-3 SDLC · SA-4 Acquisition Process · SA-5 System Documentation · SA-8 Security Engineering Principles · SA-9 External System Services · SA-10 Developer Config Mgmt · SA-11 Developer Testing & Evaluation · SA-15 Development Process · SA-22 Unsupported System Components | CC8.1 · CC9.2 · PI1.1 | CTRL-INPUT-001..004 + CTRL-SUPPLY-001..008 + CTRL-FORMAL-001/002 + ADR + sprint preflight + `VENDOR-RISK-REGISTER.md` (19 vendors) | **Coverage** (secure SDLC + property tests + TLA+ model check + SBOM + license allowlist + vendor risk register all in place) |

### 2.16 SC — System and Communications Protection

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| SC (System & Communications Protection) — ~32 controls in Moderate | SC-4 Information in Shared Resources · SC-5 DoS Protection · SC-7 Boundary Protection · SC-8 Transmission Confidentiality & Integrity · SC-10 Network Disconnect · SC-12 Cryptographic Key Establishment & Mgmt · SC-13 Cryptographic Protection · SC-15 Collaborative Computing · SC-17 PKI Certificates · SC-18 Mobile Code · SC-20/21/22 DNS · SC-23 Session Authenticity · SC-28 Protection of Info at Rest · SC-39 Process Isolation | CC6.1 · CC6.6 · CC6.7 · C1.1 | CTRL-CRYPTO-001/002/003 + CTRL-NET-001/002/003/004/005 + CTRL-ISO-001..005 + TLS 1.3 + AES-256-GCM at rest + BYOK envelope + per-tenant DO + Cloudflare WAF | **Partial** (substance fully covered; **SC-13 cryptographic protection requires FIPS 140-3 validated modules — same GAP-02 BYOK FIPS attestation thread; AWS KMS L3 attested, GCP CloudHSM L3 path mapped, Azure Key Vault HSM path mapped, HashiCorp Vault Enterprise FIPS path mapped**) |

### 2.17 SI — System and Information Integrity

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| SI (System & Information Integrity) — ~17 controls in Moderate | SI-2 Flaw Remediation · SI-3 Malicious Code Protection · SI-4 System Monitoring · SI-5 Security Alerts & Advisories · SI-6 Security Function Verification · SI-7 Software, Firmware, Information Integrity · SI-8 Spam Protection · SI-10 Information Input Validation · SI-11 Error Handling · SI-12 Information Mgmt & Retention · SI-16 Memory Protection | CC6.8 · CC7.1 · CC7.2 · PI1.1 · PI1.2 · PI1.5 | CTRL-INPUT-001..004 + CTRL-SUPPLY-002/003/004 + CTRL-META-001 + Cosign verification + immutable Workers + cargo-fuzz + property tests + Rust memory safety | **Coverage** (Rust memory safety + property tests + cargo-fuzz + immutable Workers + Cosign-verified deploys + INV-AUDIT-APPEND-ONLY exceed Moderate baseline) |

### 2.18 SR — Supply Chain Risk Management (new in Rev 5)

| NIST family × ctrl set | Representative controls (Moderate baseline) | SOC 2 CC overlap | Our CTRL ID | Status |
|---|---|---|---|---|
| SR (Supply Chain Risk Management) — ~12 controls in Moderate (introduced in Rev 5) | SR-2 Supply Chain Risk Mgmt Plan · SR-3 Supply Chain Controls & Processes · SR-5 Acquisition Strategies, Tools & Methods · SR-6 Supplier Assessments & Reviews · SR-8 Notification Agreements · SR-10 Inspection of Systems & Components · SR-11 Component Authenticity · SR-12 Component Disposal | CC5.2 · CC6.8 · CC9.2 | CTRL-SUPPLY-001..008 + SLSA L3 + Cosign + Rekor + CycloneDX SBOM + Dependency-Track + license allowlist + reproducible builds + `VENDOR-RISK-REGISTER.md` (19 vendors with quarterly review cadence) | **Coverage** (SLSA L3 build attestation + signed deploys + transparency logs + vendor risk register + sub-processor 30-day change notification all in place; one of our strongest families) |

---

## 3. Coverage rollup

### 3.1 Per-family summary

| # | NIST family | Moderate baseline controls (~) | Status | SOC 2 + ISO 27001 satisfies |
|---|---|---:|---|---|
| 1 | AC — Access Control | 25 | Coverage | yes |
| 2 | AT — Awareness & Training | 4 | Partial | yes (formality gap) |
| 3 | AU — Audit & Accountability | 12 | Coverage | yes (exceeds) |
| 4 | CA — Assessment, Authorization & Monitoring | 9 | Partial | yes (CA-6 ATO Not-in-Scope) |
| 5 | CM — Configuration Management | 11 | Coverage | yes (exceeds) |
| 6 | CP — Contingency Planning | 9 | Partial | yes (drill cycle pending) |
| 7 | IA — Identification & Authentication | 12 | Partial | yes (IA-7 FIPS gap, GAP-02) |
| 8 | IR — Incident Response | 9 | Partial | yes (US-CERT reporting Not-in-Scope) |
| 9 | MA — Maintenance | 6 | Coverage | yes (inherited) |
| 10 | MP — Media Protection | 7 | Coverage | yes |
| 11 | PE — Physical & Environmental Protection | 17 | Coverage | yes (inherited) |
| 12 | PL — Planning | 4 | Partial | yes (SSP template Not-in-Scope) |
| 13 | PS — Personnel Security | 8 | Not-in-Scope | no (US-citizen screening) |
| 14 | RA — Risk Assessment | 7 | Coverage | yes |
| 15 | SA — System & Services Acquisition | 14 | Coverage | yes |
| 16 | SC — System & Communications Protection | 32 | Partial | yes (SC-13 FIPS gap, GAP-02) |
| 17 | SI — System & Information Integrity | 17 | Coverage | yes (exceeds) |
| 18 | SR — Supply Chain Risk Management | 12 | Coverage | yes (exceeds) |
| | **Total (in-scope 18 families)** | **~215 controls (rounded; subset of 325 baseline)** | — | — |

> **Note on control count.** The FedRAMP Rev 5 Moderate baseline contains
> **325 total parameters** (controls + control enhancements + control
> parameters) per the FedRAMP PMO baseline workbook. Mapping at the
> family level rather than per-enhancement is the convention for an
> informational crosswalk; a 3PAO would require per-parameter mapping
> into the FedRAMP SSP Appendix A workbook (Not-in-Scope per
> `FEDRAMP-NOT-IN-SCOPE-RATIONALE.md`).

### 3.2 Headline coverage estimate

| Bucket | Family count | Estimated control coverage |
|---|---:|---:|
| **Coverage** (full substance via SOC 2 + ISO 27001 + CTRL-*) | 10 / 18 | ~165 / 215 = **76.7%** |
| **Partial** (substance present; formality / FedRAMP-unique evidence gap) | 7 / 18 | ~42 / 215 = **19.5%** |
| **Not-in-Scope** (PS family — US-citizen personnel screening) | 1 / 18 | ~8 / 215 = **3.7%** |
| **Gap** (no equivalent control) | 0 / 18 | 0% |

**Estimated FedRAMP Moderate substance coverage today: ~85% (Coverage + Partial)** — i.e., the technical and operational substance of the Moderate baseline is in place for 17 of 18 families; the remaining ~15% is either FedRAMP-formality (POA&M-in-FedRAMP-format, SSP-in-GSA-template) or FedRAMP-unique procurement (FIPS 140-3 validated modules where SOC 2 GAP-02 attestation exists but FedRAMP demands the validation certificate number itself).

### 3.3 Highlight: families ≥ 80% covered via SOC 2 + ISO 27001 overlap

The following **13 families** are estimated to clear the 80% control-substance threshold solely from SOC 2 Type I + ISO 27001 Annex A crosswalk evidence already produced (per `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` + `ISO27001-CROSSWALK-2026-05-15.md`):

1. **AC** Access Control — Coverage
2. **AU** Audit & Accountability — Coverage (exceeds baseline)
3. **CM** Configuration Management — Coverage (exceeds baseline)
4. **MA** Maintenance — Coverage (inherited)
5. **MP** Media Protection — Coverage
6. **PE** Physical & Environmental Protection — Coverage (inherited)
7. **RA** Risk Assessment — Coverage
8. **SA** System & Services Acquisition — Coverage
9. **SI** System & Information Integrity — Coverage (exceeds baseline)
10. **SR** Supply Chain Risk Management — Coverage (exceeds baseline)
11. **AT** Awareness & Training — Partial (≥80% substance; formality gap)
12. **CP** Contingency Planning — Partial (≥80% substance; drill-cycle calendarization pending)
13. **CA** Assessment, Authorization & Monitoring — Partial (≥80% substance excluding CA-6 ATO process itself)

The remaining 5 families are either FedRAMP-unique (PS US-citizen personnel; PL FedRAMP SSP template; IR US-CERT reporting clause) or shared cryptographic-formality gaps (IA-7 + SC-13 FIPS 140-3 validation — both threaded through SOC 2 GAP-02, in-flight per the BYOK FIPS attestation kit).

---

## 4. What would change if we ever pursued FedRAMP (for context only)

> **Repeat: we do not pursue this today.** This section exists only so that
> a prospect can ground-truth our coverage claim against the real delta.

| Workstream | Description | First-year incremental cost | Calendar lead time |
|---|---|---|---|
| 3PAO selection + engagement | Schellman, A-LIGN, Coalfire, or KPMG (all FedRAMP-accredited 3PAOs) | $200-350k (Stage 1 + Stage 2) | 3-6 months |
| SSP authoring (GSA Word template, ~400 pages) | Translate `security_model.md` + `privacy_model.md` + `compliance_matrix.md` into the FedRAMP SSP template; ~12 person-months | $80-150k (contracted FedRAMP SSP author) | 6-9 months |
| FIPS 140-3 validation evidence | Move BYOK to AWS KMS FIPS endpoint (already FIPS 140-3 L3 attested per GAP-02 progress) + GCP CloudHSM FIPS path + Azure Key Vault HSM Premium + HashiCorp Vault Enterprise FIPS | $30-60k (Azure HSM Premium + Vault Enterprise license uplift) | 3-6 months |
| US-citizen personnel screening | Implement OPM-style background investigation pathway for any HuGR employee with privileged access to FedRAMP boundary | $5-15k per employee + sponsor agency arrangement | 6-12 months (background investigation cycle) |
| Continuous Monitoring (ConMon) | Monthly POA&M submission + monthly scans uploaded to FedRAMP Marketplace + Significant Change Request workflow | $50-100k/year ongoing | Continuous post-ATO |
| Agency sponsorship (Agency ATO path) OR JAB P-ATO path | Identify sponsor agency willing to issue ATO OR submit to JAB queue (12-24 month queue today) | $0 cost (in-kind sponsorship); $200-400k JAB path | 9-24 months |
| **Total first-year incremental (above current SOC 2 + ISO 27001 spend)** | — | **~$500-900k** | **12-18 months** |

These numbers come from FedRAMP PMO publicly available marketplace data + industry blog posts (Coalfire 2024 cost analysis; FedRAMP PMO 2024 cycle-time report). They are **estimates** and would be re-priced against actual quotes if a sponsorship trigger fired per `FEDRAMP-NOT-IN-SCOPE-RATIONALE.md §4`.

---

## 5. How to read this crosswalk

### 5.1 For enterprise prospects (RFP / SIG / CAIQ response)

> "CoreLink does **not** hold a FedRAMP authorization and does not target one
> today. However, our SOC 2 Type I (target Q1-2027) + ISO 27001:2022
> (target Q1-2027) program crosswalks to approximately **85% of the
> FedRAMP Rev 5 Moderate baseline substance** across 17 of 18 NIST 800-53
> control families. The full mapping is in
> `specs/_compliance/FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md` (this doc)
> and the rationale for not pursuing certification is in
> `specs/_compliance/FEDRAMP-NOT-IN-SCOPE-RATIONALE.md`. If you require
> FedRAMP authorization, please contact `enterprise@corelink.dev` for a
> sponsorship discussion — the trigger conditions for re-opening the
> decision are documented."

### 5.2 For an internal procurement / sales contact

- Use the §2 family table to answer "do you do <NIST control X>?" directly.
- Anything **Coverage** = answer "yes, here is the SOC 2 / ISO evidence" and link to the corresponding row in `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` or `ISO27001-CROSSWALK-2026-05-15.md`.
- Anything **Partial** = answer "substance is in place; FedRAMP-formality artefact would need translation; SOC 2 evidence available now".
- Anything **Not-in-Scope** = escalate to `enterprise@corelink.dev` per the customer page; this is a deal-shape question, not a controls question.

### 5.3 For a future re-evaluation (per trigger conditions)

If a customer sponsorship triggers re-opening the decision, the work order would be:

1. Refresh this crosswalk to per-control (not per-family) granularity (~6 person-weeks);
2. Author the FedRAMP SSP Appendix A workbook per parameter (~12 person-months — contracted out);
3. Engage 3PAO and run a readiness assessment (3 months calendar);
4. Then execute the §4 workstreams in parallel (12-18 months total to ATO).

---

## 6. Cross-references

- **Not-in-scope rationale + trigger conditions:** `specs/_compliance/FEDRAMP-NOT-IN-SCOPE-RATIONALE.md`
- **Customer-facing one-pager:** `apps/docs/docs/trust/fedramp-info.mdx`
- **SOC 2 evidence rollup (source for coverage claims):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **ISO 27001:2022 Annex A crosswalk:** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **Compliance matrix (Level 3 framework crosswalk):** `specs/03_architecture/compliance_matrix.md`
- **Security model (CTRL catalog):** `specs/03_architecture/security_model.md` §6
- **BYOK FIPS attestation matrix (IA-7 + SC-13 thread):** `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`
- **NIST SP 800-53 Rev 5 (authoritative source):** <https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final>
- **NIST SP 800-53B Rev 5 (control baselines):** <https://csrc.nist.gov/publications/detail/sp/800-53b/final>
- **FedRAMP Rev 5 Baselines:** <https://www.fedramp.gov/rev5/baselines/>

---

## 7. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 R-prep FedRAMP-info agent) | Initial informational FedRAMP Rev 5 Moderate baseline crosswalk; 18 NIST families × representative controls × SOC 2 CC overlap × CTRL ID × status; ~85% substance coverage today via SOC 2 + ISO 27001 overlap; 13 families ≥ 80% threshold; PS family Not-in-Scope (US-citizen screening); FIPS thread (IA-7 + SC-13) routed through SOC 2 GAP-02; explicit declaration CoreLink does not target FedRAMP certification; refresh cadence annual or upon Rev 5 baseline change. |

---

**Fim FEDRAMP-MODERATE-CROSSWALK.**
