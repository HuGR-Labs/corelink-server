---
id: "DD-DRATA-2026-05-15"
type: "vendor_due_diligence"
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
inherits_from: ["VENDOR-RISK-REGISTER-2026-05-15", "VENDOR-RISK-METHODOLOGY-2026-05-15"]
tags: ["soc2", "cc9.2", "vendor-dd", "critical", "drata", "compliance-platform", "gap-14"]
---

# Vendor DD — Drata, Inc.

> **doc_status:** ACTIVE · Register row: 7 · Category: **Critical** · Residual risk: **4.8** (Inherent 12 × CEF 0.40).
>
> Drata is the SOC 2 / ISO 27001 evidence-collection platform. Drata is **the auditor's read-side** for ~95% of CoreLink controls. Drata outage doesn't break the customer surface but does break audit posture.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | Drata, Inc. |
| HQ | San Diego, CA, USA |
| Plan | (per `H-10` in `ROADMAP-TO-GA.md` — subscription tier tracked in vendor contacts) |
| Contract effective | 2026-04-23 |
| Account contacts | (recorded in `legal/vendor-contacts.md`) |
| Companion doc | `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` (six EvidenceStream variants × TSC mapping) |

## 2. Service scope

- Continuous evidence collection (auto-pull from Cloudflare, AWS, GitHub, Clerk, Stripe, PagerDuty, Slack, etc.)
- Control framework templates (SOC 2 TSC 2017+2022, ISO 27001:2022, GDPR, HIPAA)
- Auditor portal (read-only access for SOC 2 audit firm under `H-10`)
- Vendor-risk module (this register's auto-import target)
- Employee-attestation tracker (security training, policy acknowledgment)
- Risk-register module (open-action tracker)
- Policy template library

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Vendor configuration metadata (read-only OAuth tokens to integrations) | CoreLink → Drata | Drata-managed | TLS 1.3 | Drata |
| Audit-log aggregates from integrations | CoreLink → Drata | Drata-managed | TLS 1.3 | Drata |
| Employee directory (names, emails, role) | CoreLink → Drata via SSO | Drata-managed | TLS 1.3 | Drata |
| Policy documents (markdown) | CoreLink → Drata | Drata-managed | TLS 1.3 | Drata |
| Customer PII | (never) | — | — | — |

Drata processes **internal-only** data: about CoreLink's own employees, configurations, and compliance posture. No customer data flows to Drata.

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 2 Type II | Attested (Drata audits itself, of course) | [Drata Trust Center](https://drata.com/trust) |
| ISO 27001 | Certified | Drata Trust Center |
| GDPR | Processor; SCCs Module 2 | Drata DPA |
| LGPD | Processor | Drata DPA |
| HIPAA | Not applicable to CoreLink usage | — |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| Drata MSA | 2026-04-23 | Annual subscription |
| DPA | 2026-04-23 (unilateral acceptance) | SCCs Module 2 |
| SLA | 99.9% monthly uptime (standard plan) | — |
| BAA | Not signed | No PHI |

## 6. Control mapping

| CoreLink CTRL | Dependency on Drata |
|---|---|
| CC9.2 (vendor risk) | Drata vendor module is the **canonical store** for the register entries here |
| CC1.2 / CC1.3 (org governance) | Employee-attestation tracker holds policy-acknowledgment evidence |
| CC2.2 (communication) | Policy template library + auditor-portal read access |
| CC4.1 (monitoring activities) | Continuous evidence collection auto-pulls log integrity proofs |
| CC7.x (system monitoring) | Drata cross-checks alerting + log retention against control settings |
| Every TSC criterion auto-mapped in `DRATA-INTEGRATION-COVERAGE.md` (~95% auto coverage) | |

## 7. Risk assessment narrative

**Inherent risk (12 = 3 × 4):** Impact 3 (Drata down ≠ CoreLink customer-facing outage; only the audit-evidence pipeline stalls; manual evidence collection is documented in `RB-DRATA-SYNC-FAILURE.md` as fallback); Likelihood 4 (mid-stage SaaS; less battle-tested than AWS/Cloudflare).

**Residual risk (4.8):** CEF 0.40 because (a) Drata has current SOC 2 Type II + DPA, but (b) there is **no second-vendor failover** — Drata is single-vendor by design and switching would mean rebuilding integrations across all 19 vendors. The compensating control is the documented **manual-evidence fallback** in `DRATA-INTEGRATION-COVERAGE.md`: every Drata auto-evidence stream has a manual-upload counterpart so a Drata outage degrades audit speed, not audit completeness.

**Top risks monitored:**

1. Drata account compromise → attacker reads everything we know about ourselves (internal-only data, but still material). Mitigation: SSO + MFA mandatory; quarterly access review of who has Drata admin rights.
2. Drata integration credential drift → evidence pulled is stale; auditor sees yesterday's state. Mitigation: `RB-DRATA-SYNC-FAILURE.md` runbook + weekly automated drift check.
3. Drata acquired or product-direction shifts away from SOC 2 → forced platform migration. Mitigation: manual-evidence fallback is the actual fallback; new platform onboarding is a quarter-long project.
4. Drata reports misleading evidence (e.g., incorrect TSC coverage claim). Mitigation: `DRATA-INTEGRATION-COVERAGE.md` is **CoreLink-owned** — auditor reads CoreLink's authoritative mapping, not Drata's UI.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| Drata customer success | dedicated CSM (per `legal/vendor-contacts.md`) + support@drata.com | 1 business day |
| Drata support — P1 | support@drata.com `Subject: URGENT` | 4h business hours |
| Security | security@drata.com | 24h |
| Privacy / DPA | privacy@drata.com | 5 business days |
| Breach notification | privacy@drata.com → security@hugr.dev | Per DPA |

## 9. Termination / exit plan

If Drata exits or CoreLink terminates the subscription:

1. **Day 0..7:** snapshot all Drata data: vendor entries, policy library, employee directory, evidence archive. Drata supports a full data export via UI.
2. **Day 7..30:** convert manual-evidence fallback (already documented per integration) to the **primary** evidence collection path until replacement platform is selected.
3. **Day 30..90:** evaluate replacement (Vanta, Secureframe, Hyperproof, Thoropass) → onboard → re-integrate.
4. **Audit continuity:** SOC 2 audit firm uses snapshot exports plus manual evidence; auditor walkthrough script (`AUDITOR-WALKTHROUGH-SCRIPT.md`) is **vendor-neutral** by design and survives a Drata exit.

**RTO:** 0 days for audit continuity (manual fallback exists); ≤ 90 days for full replacement platform productivity.

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 4.8 | Initial DD; closes GAP-14. Highest residual of the Critical six because no failover, but inherent is lowest because Drata outage does not break customer surface. |

Next quarterly review: **2026-08-15**.
