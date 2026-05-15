---
id: "ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15"
type: "compliance_statement_of_applicability"
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
  - "ISO27001-CROSSWALK-2026-05-15"
  - "ISO27001-GAP-ANALYSIS-2026-05-15"
  - "ISO27001-ROADMAP-2026-05-15"
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
tags:
  - "iso27001"
  - "iso27001-2022"
  - "statement-of-applicability"
  - "soa"
  - "annex-a"
  - "stage-1-required"
  - "r-prep"
---

# ISO/IEC 27001:2022 — Statement of Applicability (CoreLink)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** mandatory Stage 1 audit artifact per ISO/IEC 27001:2022 Clause 6.1.3 d). Lists **all 93 Annex A controls**, declares applicability (Yes / No-with-justification), implementation status, and the evidence vehicle. Companion mapping in `ISO27001-CROSSWALK-2026-05-15.md` §2-§5; gap closure plans in `ISO27001-GAP-ANALYSIS.md`; roadmap to Q1-2027 certification in `ISO27001-ROADMAP.md`.

## 1. Header

| Field | Value |
|---|---|
| Framework | **ISO/IEC 27001:2022** (with AMD1:2024 climate-change clause; Annex A unchanged) |
| Annex A control set | **93 controls** across 4 themes (Organizational 37 · People 8 · Physical 14 · Technological 34) |
| ISMS scope | Cloudflare Workers + Pages + R2 + D1 + DO + KV + Clerk + Stripe; BYOK via AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault; sub-processors per `legal/sub-processors.md`. Corporate IT / personal devices excluded. |
| SoA version | **1.0.0** (baseline replacing the 46-row CSV at `specs/_audits/iso27001-soa.csv`; full 93-row format for Stage 1 audit submission) |
| Snapshot commit | `30bd735` (`origin/main` HEAD at SoA freeze) |
| Cut date | **2026-05-15** |
| Target Stage 1 audit | Q4-2026 |
| Target Stage 2 audit | Q1-2027 |
| Audit firm | Schellman (incumbent SOC 2; accredited for ISO 27001 by ANAB) · A-LIGN secondary |
| Approver | Gustavo Schneiter (Owner + Compliance Officer interim) |
| Next review | 2026-08-15 (quarterly cadence per `ISO27001-INTERNAL-AUDIT-PROGRAM.md` §3) |

### 1.1 Legend

- **Applicability:** `Yes` (in-scope; control applies) · `No` (excluded — justify per Clause 6.1.3 d 2)).
- **Implementation status:** `I` Implemented (auditor-grade evidence today) · `P` Partial (design exists; gap closure in flight) · `G` Gap (no control yet; ISO-unique work tracked in gap analysis) · `inherited` (control delivered by sub-processor per their SOC 2 Type II).
- **Closure vehicle:** SOC 2 GAP-XX (from `SOC2-GAP-ANALYSIS.md`) or GAP-ISO-XX (from `ISO27001-GAP-ANALYSIS.md`). Empty when status = `I` or `inherited`.
- **Evidence pointer:** canonical artifact path. All evidence sourced from existing compliance corpus — no inventions in this SoA per the DEBT-012 charter.

---

## 2. Theme A.5 — Organizational controls (37)

| ID | Title | Applicability | Status | Justification / Implementation | Closure vehicle | Evidence pointer |
|---|---|---|---|---|---|---|
| A.5.1 | Policies for information security | Yes | I | 13-role sign-off in `specs/_governance/`; quarterly legal review template; 60+ runbooks under VCS. | — | `specs/_governance/` · `legal/quarterly-legal-review-template.md` |
| A.5.2 | Information security roles and responsibilities | Yes | I | WI frontmatter (`owner` / `final_approver` / `reviewers`) + Clerk role assignments + RACI in DPO matrix. | — | `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md` |
| A.5.3 | Segregation of duties | Yes | I | INV-ADMIN-DUAL-APPROVAL + branch protection + sprint contracts §5.1; CTRL-AUTH-010 + PAT-DUAL-APPROVAL-001. | — | `specs/03_architecture/security_model.md §6` |
| A.5.4 | Management responsibilities | Yes | P | Advisor-pool framework + Owner attestation; awaiting advisor pool sign-off batch. | SOC2 GAP-04 | `specs/_governance/` |
| A.5.5 | Contact with authorities | Yes | I | ANPD / CNIL / ICO contact register + RB-BREACH-NOTIF. | — | `legal/breach-notification/` · `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` |
| A.5.6 | Contact with special interest groups | Yes | I | CISA feeds + GitHub Security Advisories + OWASP membership (Security Lead). | — | `specs/03_architecture/security_model.md` |
| A.5.7 | Threat intelligence | Yes | I | `cargo-audit` daily CI + Dependency-Track + STRIDE/LINDDUN matrix. | — | `specs/_audits/matrix-stride-ctrl.csv` |
| A.5.8 | Information security in project management | Yes | I | Sprint contracts §15 risk row + ADR process + preflight reviews. | — | `specs/00_meta/sprint_contracts/` |
| A.5.9 | Inventory of information and other associated assets | Yes | P | Asset inventory exists across `data_model.md` + Drata + `legal/sub-processors.md`; consolidating into a single signed register. | GAP-ISO-01 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-01` |
| A.5.10 | Acceptable use of information and other associated assets | Yes | P | AUP content embedded in code-of-conduct + DPA + contractor templates; formal standalone AUP issuance scheduled. | GAP-ISO-02 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-02` |
| A.5.11 | Return of assets | Yes | I | Sub-processor offboarding clauses; no HuGR-owned hardware in scope. | — | `legal/sub-processors-templates/` |
| A.5.12 | Classification of information | Yes | I | 3-tier classification (Public / Internal / Confidential-Tenant-Data) in `privacy_model.md §2`; CTRL-PRIV-001/003. | — | `specs/03_architecture/privacy_model.md §2` |
| A.5.13 | Labelling of information | Yes | I | `purpose_tag` schema on every audit event (CloudEvents). | — | `specs/03_architecture/privacy_model.md` |
| A.5.14 | Information transfer | Yes | I | TLS 1.3 enforced; envelope encryption per `compliance/byok-fips-matrix.md`; DPA §transfer clauses. | — | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` |
| A.5.15 | Access control | Yes | P | Clerk SSO + MFA + per-tenant DO; BYOK FIPS attestation per provider in progress. | SOC2 GAP-02 | `specs/03_architecture/auth_model.md` |
| A.5.16 | Identity management | Yes | I | Clerk identity provisioning + WI-S19-001 onboarding flow. | — | `specs/03_architecture/auth_model.md` |
| A.5.17 | Authentication information | Yes | I | PAT signed (HMAC-SHA256) + no-secret-in-log lint + WebAuthn UV=1. | — | `specs/03_architecture/security_model.md §6` |
| A.5.18 | Access rights | Yes | P | Clerk RBAC + tenant erasure pipeline; quarterly access-review automation pending. | SOC2 GAP-01 | `specs/_compliance/SOC2-GAP-ANALYSIS.md GAP-01` |
| A.5.19 | Information security in supplier relationships | Yes | I | 19-vendor register + 11 vendor DD files + `VENDOR-RISK-METHODOLOGY.md`. SOC2 GAP-14 closed 2026-05-15. | — | `specs/_compliance/VENDOR-RISK-REGISTER.md` |
| A.5.20 | Addressing information security within supplier agreements | Yes | P | DPA template + sub-processor termination clauses; per-contract security-clause checklist pending. | GAP-ISO-03 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-03` |
| A.5.21 | Managing information security in the ICT supply chain | Yes | I | SLSA L3 + Cosign + Rekor + SBOM CycloneDX + Dependency-Track + license allowlist + reproducible build. | — | `specs/03_architecture/security_model.md §6` |
| A.5.22 | Monitoring, review and change management of supplier services | Yes | P | Quarterly vendor review runbook + sub-processor SOC 2 refresh; cadence calendarization in flight. | SOC2 GAP-09/21/32 | `specs/05_quality/runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md` |
| A.5.23 | Information security for use of cloud services | Yes | I | Cloudflare WAF + Access + mTLS edge-to-origin + per-tenant DO + R2 bucket policy + egress allowlist. | — | `specs/03_architecture/security_model.md §6` |
| A.5.24 | Information security incident management planning and preparation | Yes | P | IR tabletop playbook + 6 scenarios + 2026 schedule; first execution Q3-2026. | SOC2 GAP-03 | `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` |
| A.5.25 | Assessment and decision on information security events | Yes | I | PagerDuty SEV matrix + RB-* triage + STRIDE/LINDDUN risk scoring. | — | `specs/_audits/matrix-stride-ctrl.csv` |
| A.5.26 | Response to information security incidents | Yes | P | RB-BREACH-NOTIF + RB-CONSENT-TAMPERING + RB-DATA-RESIDENCY-LEAK + RB-DSR-ERASURE-INCOMPLETE + RB-BYOK-REVOKE; postmortem template harmonization pending. | SOC2 GAP-12 | `specs/05_quality/runbooks/` |
| A.5.27 | Learning from information security incidents | Yes | P | `specs/_postmortems/` format + adversarial summaries; template harmonization pending. | SOC2 GAP-12 | `specs/_postmortems/` |
| A.5.28 | Collection of evidence | Yes | I | R2 Object Lock Governance Mode + Merkle audit chain + 7y retention + INV-AUDIT-APPEND-ONLY. | — | `specs/03_architecture/security_model.md §6 CTRL-AUDIT-001..005` |
| A.5.29 | Information security during disruption | Yes | P | RB-DR-DRILL + ACTIVE-FAILOVER + COLD-RESTORE + 14-drill cadence; first cycle pending. | SOC2 GAP-13/15 | `specs/_compliance/BCP-DR-DRILL-CADENCE.md` |
| A.5.30 | ICT readiness for business continuity | Yes | P | SLO catalog + multi-region D1+DO+R2 + chaos region-outage + RTO 15min / RPO 5min; formal BIA pending. | GAP-ISO-04 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-04` |
| A.5.31 | Legal, statutory, regulatory and contractual requirements | Yes | I | Quarterly legal review template + LGPD-FULL-AUDIT + GDPR mapping + DPA + sub-processors. | — | `legal/quarterly-legal-review-template.md` |
| A.5.32 | Intellectual property rights | Yes | I | `cargo-deny` license allowlist (MIT / Apache-2.0 / BSD / ISC / MPL-2.0; deny GPL / AGPL / SSPL) + INV-SUPPLY-LICENSE-ALLOWLIST. | — | `specs/03_architecture/security_model.md §6 CTRL-SUPPLY-006` |
| A.5.33 | Protection of records | Yes | I | R2 Object Lock Governance Mode + 7y retention lifecycle + quarterly recovery test. | — | `specs/03_architecture/security_model.md §6 CTRL-AUDIT-001/005` |
| A.5.34 | Privacy and protection of PII | Yes | I | `privacy_model.md` + LGPD-FULL-AUDIT + LGPD-ROPA + RB-DSR-LGPD-FULL + INV-CONSENT-PROOF-VERIFIABLE. SOC2 GAP-22 closed 2026-05-15. PIMS extension (ISO 27701) tracked as GAP-ISO-08 informational deferred to Q3-2027 post-cert. | — | `specs/03_architecture/privacy_model.md` |
| A.5.35 | Independent review of information security | Yes | P | External pentest annual (Schellman / Bishop Fox); SOC 2 Type I T+6m. Cadence calendarization pending. | SOC2 GAP-20 | `specs/_pentest/SOW-S20-EXTERNAL-PENTEST.md` |
| A.5.36 | Compliance with policies, rules and standards for information security | Yes | P | Drata continuous compliance + 96.4% green sustained + weekly compliance review; review cadence pending. | SOC2 GAP-08 | `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` |
| A.5.37 | Documented operating procedures | Yes | I | 60+ `RB-*.md` runbooks under VCS. | — | `specs/05_quality/runbooks/` |

**Theme A.5 totals:** 37 / 37 applicable · 23 I · 14 P · 0 G · 0 N/A.

---

## 3. Theme A.6 — People controls (8)

| ID | Title | Applicability | Status | Justification / Implementation | Closure vehicle | Evidence pointer |
|---|---|---|---|---|---|---|
| A.6.1 | Screening | Yes | P | Owner-attested for solo-founder org; advisor pool CV review template (CISA / CIPP-E / OSCP). | SOC2 GAP-05 | `legal/legal-externo-engagement-contract.md` |
| A.6.2 | Terms and conditions of employment | Yes | I | Owner-signed code-of-conduct + contributor NDAs in `legal/contractor-templates/`. | — | `legal/contractor-templates/` |
| A.6.3 | Information security awareness, education and training | Yes | P | Skill matrix + advisor onboarding training; tracking automation pending. | SOC2 GAP-30 | `templates/` |
| A.6.4 | Disciplinary process | Yes | I | Embedded in code-of-conduct + DPA §confidentiality breach clauses. | — | `legal/dpa/v1.0.0` |
| A.6.5 | Responsibilities after termination or change of employment | Yes | P | Clerk de-provisioning + INV-DATA-ERASURE-COMPLETE on offboarding; signed offboarding checklist pending. | GAP-ISO-05 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-05` |
| A.6.6 | Confidentiality or non-disclosure agreements | Yes | I | DPA template §confidentiality + contractor NDAs + sub-processor templates. | — | `legal/dpa/v1.0.0` |
| A.6.7 | Remote working | Yes | I | Cloudflare Access for admin plane + WebAuthn + session-bind-to-UA+IP. | — | `specs/03_architecture/security_model.md §6 CTRL-NET-001 + CTRL-AUTH-010` |
| A.6.8 | Information security event reporting | Yes | I | RB-BREACH-NOTIF intake + PagerDuty 24/7 + `security@corelink.dev` triage. | — | `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` |

**Theme A.6 totals:** 8 / 8 applicable · 5 I · 3 P · 0 G · 0 N/A.

---

## 4. Theme A.7 — Physical controls (14)

> **Scope note:** CoreLink runs entirely on managed cloud infrastructure (Cloudflare + AWS + GCP + Azure). HuGR owns **no** data center, server room, or office facility. Most A.7 controls are inherited from sub-processor SOC 2 Type II reports per `legal/sub-processors.md`. Per ISO 27001:2022 Clause 6.1.3 d) 2), inheritance is documented per row; the two `No` exclusions (A.7.3 + A.7.6) are justified by absence of owned facilities.

| ID | Title | Applicability | Status | Justification / Implementation | Closure vehicle | Evidence pointer |
|---|---|---|---|---|---|---|
| A.7.1 | Physical security perimeters | Yes | inherited | Cloudflare / AWS / GCP / Azure SOC 2 Type II reports. | — | `legal/sub-processors.md` |
| A.7.2 | Physical entry | Yes | inherited | Sub-processor SOC 2 reports. | — | `legal/sub-processors.md` |
| A.7.3 | Securing offices, rooms and facilities | **No** | — | **Excluded:** remote-first organization; HuGR owns no offices, rooms, or facilities. All compute / storage in managed cloud. Justification per Clause 6.1.3 d) 2). | — | `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md §4 scope note` |
| A.7.4 | Physical security monitoring | Yes | inherited | Sub-processor SOC 2 reports. | — | `legal/sub-processors.md` |
| A.7.5 | Protecting against physical and environmental threats | Yes | inherited | Multi-region D1+DO+R2 replication + sub-processor environmental controls. | — | `legal/sub-processors.md` |
| A.7.6 | Working in secure areas | **No** | — | **Excluded:** no owned facilities; remote-first org. Justification per Clause 6.1.3 d) 2). | — | `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md §4 scope note` |
| A.7.7 | Clear desk and clear screen | Yes | P | Workstation auto-lock + informal practice; standalone policy pending. | GAP-ISO-06 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-06` |
| A.7.8 | Equipment siting and protection | Yes | inherited | Sub-processor SOC 2 reports. | — | `legal/sub-processors.md` |
| A.7.9 | Security of assets off-premises | Yes | P | Workstation FDE (FileVault / BitLocker) verified at onboarding + Cloudflare Access posture-check; MDM-lite documentation pending. | GAP-ISO-07 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-07` |
| A.7.10 | Storage media | Yes | I | Sub-processor media destruction + INV-DATA-ERASURE-COMPLETE for logical disposal. | — | `legal/sub-processors.md` |
| A.7.11 | Supporting utilities | Yes | inherited | Sub-processor SOC 2 reports (power / cooling / network redundancy). | — | `legal/sub-processors.md` |
| A.7.12 | Cabling security | Yes | inherited | Sub-processor SOC 2 reports. | — | `legal/sub-processors.md` |
| A.7.13 | Equipment maintenance | Yes | inherited | Sub-processor SOC 2 reports. | — | `legal/sub-processors.md` |
| A.7.14 | Secure disposal or re-use of equipment | Yes | I | Sub-processor media destruction + INV-ERASURE-ATTESTATION-SIGNED for logical disposal. | — | `specs/03_architecture/security_model.md §6 CTRL-PRIV-014` |

**Theme A.7 totals:** 14 controls · 12 applicable (`Yes`) · 2 excluded (`No`: A.7.3 + A.7.6) · 10 I (8 inherited + 2 native) · 2 P · 0 G.

---

## 5. Theme A.8 — Technological controls (34)

| ID | Title | Applicability | Status | Justification / Implementation | Closure vehicle | Evidence pointer |
|---|---|---|---|---|---|---|
| A.8.1 | User end point devices | Yes | P | Workstation FDE + WebAuthn-bound admin + Cloudflare Access posture; MDM-lite documentation pending. | GAP-ISO-07 | `specs/_compliance/ISO27001-GAP-ANALYSIS.md §3 GAP-ISO-07` |
| A.8.2 | Privileged access rights | Yes | I | JIT admin + MFA mandatory + INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS. | — | `specs/03_architecture/security_model.md §6 CTRL-AUTH-010` |
| A.8.3 | Information access restriction | Yes | I | Tenant-scoped DO routing + scope check by verb + explicit `tenant_id` assertion. | — | `specs/03_architecture/security_model.md §6 CTRL-AUTHZ-001/002` |
| A.8.4 | Access to source code | Yes | I | GitHub branch protection + signed commits + Cosign + Rekor + SLSA L3. | — | `specs/03_architecture/security_model.md §6 CTRL-SUPPLY-001` |
| A.8.5 | Secure authentication | Yes | I | PAT HMAC-SHA256 + nonce + replay window + WebAuthn UV=1 + Clerk SSO. | — | `specs/03_architecture/security_model.md §6 CTRL-AUTH-001/004/007/010` |
| A.8.6 | Capacity management | Yes | I | SLO catalog + per-tenant quota + token-bucket rate limiter + chaos region-outage drill. | — | `specs/03_architecture/security_model.md §6 CTRL-QUOTA-001 + CTRL-RATE-001` |
| A.8.7 | Protection against malware | Yes | P | Immutable Cloudflare Workers + Cosign verification + no dynamic loading + no untrusted deser; runtime drift detection pending. | SOC2 GAP-11 | `specs/_compliance/SOC2-GAP-ANALYSIS.md GAP-11` |
| A.8.8 | Management of technical vulnerabilities | Yes | P | Dependency-Track + `cargo-deny` + cargo-fuzz daily + INV-SUPPLY-NO-YANKED; SLA per severity formalization pending. | SOC2 GAP-29 | `specs/_compliance/SOC2-GAP-ANALYSIS.md GAP-29` |
| A.8.9 | Configuration management | Yes | I | Terraform + drift detection + reproducible build + ADR process. | — | `specs/03_architecture/security_model.md §6 PAT-DRIFT-DETECTION-001` |
| A.8.10 | Information deletion | Yes | I | INV-DATA-ERASURE-COMPLETE + INV-ERASURE-ATTESTATION-SIGNED + ADR-S11-003 erasure salt. | — | `specs/03_architecture/privacy_model.md` |
| A.8.11 | Data masking | Yes | I | redact + schema allowlist + `purpose_tag` on logs + no PII in metrics. | — | `specs/03_architecture/privacy_model.md` |
| A.8.12 | Data leakage prevention | Yes | I | Error envelope sanitizer + Container egress allowlist + no-secret-in-log lint. | — | `specs/03_architecture/security_model.md §6 CTRL-NET-004/005 + CTRL-CRED-001` |
| A.8.13 | Information backup | Yes | P | R2 Object Lock + 7y retention + multi-region replication + backup encryption; cold-restore drill first cycle pending. | SOC2 GAP-15 | `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` |
| A.8.14 | Redundancy of information processing facilities | Yes | P | Multi-region D1+DO+R2 + active-region warm failover (DR-16) + cold-restore (DR-15); BCP testing-depth formalization pending. | GAP-ISO-04 + SOC2 GAP-13/15 | `specs/_compliance/BCP-DR-DRILL-CADENCE.md` |
| A.8.15 | Logging | Yes | I | INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY + Merkle chain + Prometheus catalog. | — | `specs/03_architecture/security_model.md §6 CTRL-AUDIT-001..004` |
| A.8.16 | Monitoring activities | Yes | I | DASH-GA-READINESS + DASH-COMPLIANCE-S20 + Drata continuous + adversarial summaries. | — | `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` |
| A.8.17 | Clock synchronization | Yes | I | Cloudflare global anycast NTP + per-region drift alert (< 50 ms). | — | `specs/03_architecture/observability_model.md` |
| A.8.18 | Use of privileged utility programs | Yes | I | Admin-plane MFA freshness gate + dual-approval for destructive ops + Wrangler / CF API audit. | — | `specs/03_architecture/security_model.md §6 CTRL-AUTH-010 + PAT-DUAL-APPROVAL-001` |
| A.8.19 | Installation of software on operational systems | Yes | I | Cosign-verified deploys + no dynamic loading + immutable Workers + signed-deploy gate. | — | `specs/03_architecture/security_model.md §6 CTRL-SUPPLY-002/005` |
| A.8.20 | Networks security | Yes | P | Cloudflare WAF + Access + mTLS edge-to-origin + per-tenant DO; OWASP CRS 4.0 baseline pending. | SOC2 GAP-10 | `specs/_compliance/SOC2-GAP-ANALYSIS.md GAP-10` |
| A.8.21 | Security of network services | Yes | I | TLS 1.3 enforced + HSTS preload + CAA pin + Cloudflare-managed ruleset. | — | `specs/03_architecture/security_model.md §6 CTRL-CRYPTO-001 + CTRL-NET-001..005` |
| A.8.22 | Segregation of networks | Yes | I | Per-tenant DO namespace + R2 bucket policy + HMAC tenant prefix + AuthZ check on storage call. | — | `specs/03_architecture/security_model.md §6 CTRL-NET-003 + CTRL-ISO-001..005` |
| A.8.23 | Web filtering | Yes | I | Container egress allowlist (`*.r2.cloudflarestorage.com` + registries allowlist); admin-plane via Cloudflare Access. | — | `specs/03_architecture/security_model.md §6 CTRL-NET-005` |
| A.8.24 | Use of cryptography | Yes | P | TLS 1.3 + AES-256-GCM at rest + envelope encryption per-tenant + annual key rotation; per-provider BYOK FIPS attestation in progress. | SOC2 GAP-02 | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` |
| A.8.25 | Secure development life cycle | Yes | I | Schema validation + property tests + TLA+ 4 INV-level + ADR + sprint preflight reviews. | — | `specs/03_architecture/security_model.md §6 CTRL-INPUT-001..004 + CTRL-FORMAL-001` |
| A.8.26 | Application security requirements | Yes | I | OWASP ASVS L2/L3 per-surface map + pentest SOW + property tests. | — | `specs/_pentest/SOW-S20-EXTERNAL-PENTEST.md` |
| A.8.27 | Secure system architecture and engineering principles | Yes | I | STRIDE/LINDDUN + threat-model per WI HIGH_RISK + failure_modes.md FM-XXX taxonomy. | — | `specs/03_architecture/failure_modes.md` |
| A.8.28 | Secure coding | Yes | I | Rust + `sqlx::query!` parameterized only + `#[serde(deny_unknown_fields)]` + cargo-fuzz. | — | `specs/03_architecture/security_model.md §6 CTRL-INPUT-002/003 + CTRL-SUPPLY-005` |
| A.8.29 | Security testing in development and acceptance | Yes | I | cargo-fuzz summaries + property tests + TLA+ model check CI gate + pentest baseline. | — | `specs/_pentest/SOW-S20-EXTERNAL-PENTEST.md` |
| A.8.30 | Outsourced development | **No** | — | **Excluded:** all development is in-house; advisor pool is consultative only and is not assigned development tasks. Justification per Clause 6.1.3 d) 2). | — | `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md §5 A.8.30` |
| A.8.31 | Separation of development, test and production environments | Yes | I | Separate Cloudflare accounts (dev / staging / prod) + separate D1 + R2 + DO namespaces + branch protection on `main`. | — | `specs/03_architecture/security_model.md §6` |
| A.8.32 | Change management | Yes | I | Branch protection + PR review + signed deploy + Rekor + sprint contracts §5.1 + preflight. | — | `specs/03_architecture/security_model.md §6 PAT-DUAL-APPROVAL-001 + CTRL-SUPPLY-001` |
| A.8.33 | Test information | Yes | I | Synthetic test data only + redact + no production PII in test fixtures. | — | `specs/03_architecture/privacy_model.md` |
| A.8.34 | Protection of information systems during audit testing | Yes | I | Read-only auditor SSO + ticket-expiry 30d + meta-audit trail of auditor queries. | — | `specs/03_architecture/security_model.md §6` |

**Theme A.8 totals:** 34 controls · 33 applicable (`Yes`) · 1 excluded (`No`: A.8.30) · 26 I · 7 P · 0 G.

---

## 6. SoA rollup

### 6.1 Per-theme rollup

| Theme | Total | Applicable | Excluded | I (incl. inherited) | P | G |
|---|---:|---:|---:|---:|---:|---:|
| A.5 Organizational | 37 | 37 | 0 | 23 | 14 | 0 |
| A.6 People | 8 | 8 | 0 | 5 | 3 | 0 |
| A.7 Physical | 14 | 12 | 2 (A.7.3 + A.7.6) | 10 | 2 | 0 |
| A.8 Technological | 34 | 33 | 1 (A.8.30) | 26 | 7 | 0 |
| **Total** | **93** | **90** | **3** | **64** | **26** | **0** |

### 6.2 Headline numbers (Stage 1 audit readiness)

- **Applicability decisions:** 93 / 93 controls reviewed; 90 applicable (96.8%), 3 excluded with justification per Clause 6.1.3 d) 2).
- **Implementation density:** 64 / 90 in-scope = **71.1% Implemented** (auditor-grade evidence today) · 26 / 90 = **28.9% Partial** (closure vehicle assigned).
- **Zero open Gap (G) rows:** every applicable control either has Implemented evidence or a tracked closure vehicle (SOC2 GAP-XX or GAP-ISO-XX) with owner + ETA.
- **SOC 2 overlap on closure work:** 19 of the 26 Partial rows close alongside an existing SOC 2 GAP-XX; only **7 ISO-unique gaps** require incremental work (per `ISO27001-GAP-ANALYSIS.md`).

### 6.3 Exclusion rationale (Clause 6.1.3 d) 2))

| Excluded control | Rationale | Sign-off |
|---|---|---|
| A.7.3 Securing offices, rooms and facilities | HuGR is a remote-first organization with no owned offices, rooms, or facilities. All compute / storage runs in managed cloud (Cloudflare + AWS + GCP + Azure). Inherited physical controls covered by sub-processor SOC 2 Type II reports per `legal/sub-processors.md`. | Gustavo Schneiter (Owner) — 2026-05-15 |
| A.7.6 Working in secure areas | Same rationale as A.7.3 — no owned secure-area facilities. Remote-working hygiene is covered by A.6.7 (Implemented) + A.7.7 (Partial via GAP-ISO-06 clear-desk policy). | Gustavo Schneiter (Owner) — 2026-05-15 |
| A.8.30 Outsourced development | All software development is performed in-house. The advisor pool (`legal/legal-externo-engagement-contract.md`) provides consultative security review only — advisors are not assigned development tasks, do not commit code, and do not have write access to source repositories (branch protection enforces this per A.8.4). Justification is re-evaluated if any development is outsourced. | Gustavo Schneiter (Owner) — 2026-05-15 |

### 6.4 Top-of-funnel risks (Stage 1 auditor likely focus areas)

1. **GAP-ISO-04 BCP testing depth** — formal BIA per business process (signup / upload / read / DSR / billing / audit-log-export) is the highest-visibility gap because A.5.30 + A.8.14 both depend on it. Owner: SRE Lead + Compliance Officer · ETA T+2m.
2. **GAP-ISO-03 supplier SLA contract review depth** — 19 vendors × 12-question checklist; auditor will sample 3-5 contracts. Owner: Compliance Officer + Legal · ETA T+2m.
3. **GAP-ISO-01 formal asset register** — consolidation deliverable; auditor will request the register at Stage 1 documentation review. Owner: Compliance Officer · ETA T+1m.

---

## 7. Cross-references

- **Master crosswalk:** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **Gap analysis (7 ISO-unique + 1 informational PIMS):** `specs/_compliance/ISO27001-GAP-ANALYSIS.md`
- **Roadmap to Q1-2027 cert:** `specs/_compliance/ISO27001-ROADMAP.md`
- **Internal audit program (Clause 9.2):** `specs/_compliance/ISO27001-INTERNAL-AUDIT-PROGRAM.md`
- **Management review template (Clause 9.3):** `specs/_compliance/ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md`
- **SOC 2 evidence rollup:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **SOC 2 gap register (33 GAPs):** `specs/_compliance/SOC2-GAP-ANALYSIS.md`
- **SoA baseline CSV (superseded by this 93-row doc):** `specs/_audits/iso27001-soa.csv`
- **Compliance matrix (Level 3 framework crosswalk):** `specs/03_architecture/compliance_matrix.md §3`

---

## 8. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 DEBT-012 closure agent) | Initial 93-row SoA replacing the 46-row baseline CSV at `specs/_audits/iso27001-soa.csv`. All applicability decisions sourced from `ISO27001-CROSSWALK-2026-05-15.md`; implementation status + evidence pointers sourced from existing compliance corpus (no inventions per DEBT-012 charter). 3 controls excluded (`No`) with Clause 6.1.3 d) 2) justification: A.7.3 + A.7.6 (no owned facilities) + A.8.30 (no outsourced development). 90 / 93 applicable · 64 Implemented · 26 Partial · 0 open Gap. Stage 1 audit submission-ready. |

---

**Fim ISO27001-STATEMENT-OF-APPLICABILITY.**
