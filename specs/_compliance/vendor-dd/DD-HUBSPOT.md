---
id: "DD-HUBSPOT-2026-05-15"
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
tags: ["soc2", "cc9.2", "vendor-dd", "important", "hubspot", "crm", "gap-14"]
---

# Vendor DD — HubSpot, Inc.

> **doc_status:** ACTIVE · Register row: 11 · Category: **Important** · Residual risk: **3.6** (Inherent 9 × CEF 0.40).
>
> HubSpot is CoreLink's CRM for **enterprise-inquiry intake** (the `corelink.humangr.com/enterprise` form posts directly to HubSpot via a private app token). It holds **sales prospect contact data only** — no customer tenant data, no billing data, no source code.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | HubSpot, Inc. (Delaware) — Nasdaq: HUBS |
| HQ | 25 First Street, 2nd Floor, Cambridge, MA 02141, USA |
| EEA acquiring | HubSpot Ireland Limited (Dublin) — EEA data-processing |
| Contract effective | 2026-04-23 (Sales Hub Starter; click-through) |
| Account contacts | (recorded in `legal/vendor-contacts.md`); HubSpot portal ID in `secrets-checklist.md` row 24 |
| Primary integration | Private app token (per `secrets-checklist.md` row 24) — write-only contact + deal creation from the enterprise-inquiry form |

## 2. Service scope

- **HubSpot CRM (Sales Hub Starter)** — contacts, companies, deals, pipelines
- **Forms API** — `corelink.humangr.com/enterprise` inquiry form submission → HubSpot contact + deal creation
- **Workflows** — internal sales-cycle automation (assignment, follow-up cadence)
- **Email tracking + sequences** — outbound sales emails to prospects who opted in
- **Meetings tool** — calendar-booking links for AE → prospect calls

CoreLink does **not** consume: HubSpot Marketing Hub (no marketing-automation send paths), HubSpot Service Hub, HubSpot CMS Hub, HubSpot Operations Hub, HubSpot Payments — these are explicitly out of scope.

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Prospect contact (name, email, company, role, country, inquiry message) | CoreLink → HubSpot (via Forms API) | HubSpot-managed (AES-256) | TLS 1.3 | HubSpot |
| Deal metadata (pipeline stage, estimated ARR, owner) | CoreLink ↔ HubSpot | HubSpot-managed | TLS 1.3 | HubSpot |
| Email engagement events (opens, clicks) | HubSpot tracking pixel → HubSpot | HubSpot-managed | TLS 1.3 | HubSpot |
| Private app token | CoreLink-side only | `cf-wrangler` secret store | TLS 1.3 | CoreLink |

**Data classification:** all data flowing to HubSpot is **sales-prospect PII** explicitly volunteered via the enterprise-inquiry form. No customer-tenant data, no billing data, no source code, no operational telemetry transits HubSpot. The form includes GDPR-compliant consent capture (`apps/web/components/EnterpriseInquiryForm.tsx`) and the consent record is mirrored in HubSpot via the `legal_basis` property.

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 2 Type II | Attested | HubSpot Trust Center (Drata vendor module pulls annually) |
| ISO 27001 / 27018 | Certified | HubSpot Trust Center |
| ISO 27701 (PIMS) | Certified | HubSpot Trust Center |
| GDPR | Processor; SCCs Module 2; EU data-residency option | HubSpot DPA |
| LGPD | Processor; data hosted in US/EU (no BR region) — explicit consent path covers cross-border transfer | HubSpot DPA |
| CCPA | Processor; "Do Not Sell" honored | HubSpot DPA |
| PCI-DSS | N/A (HubSpot Payments not used by CoreLink) | — |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| HubSpot Customer Terms of Service | 2026-04-23 (click-through) | Sales Hub Starter tier |
| HubSpot DPA | 2026-04-23 (unilateral acceptance) | https://legal.hubspot.com/dpa ; SCCs Module 2 |
| Standard SLA | 99.95% uptime commitment | Per HubSpot SLA addendum |
| BAA | N/A | No PHI in scope |

## 6. Control mapping

| CoreLink CTRL | Dependency on HubSpot |
|---|---|
| CTRL-COMM-003 | Enterprise-inquiry intake pathway |
| CTRL-PRIV-022 | Prospect consent capture + DSR-erasure path (DSR-erasure for a HubSpot contact = delete via HubSpot API on customer request) |
| CTRL-COMPL-009 | Marketing/sales PII handling (limited scope: sales prospects only, no customer-tenant data) |

## 7. Risk assessment narrative

**Inherent risk (9 = 3 × 3):** Impact 3 (HubSpot compromise = exposure of CoreLink's sales pipeline + prospect contact list; reputationally bad but **does not touch any customer-tenant data**; one-customer-cohort impact ceiling is the entire sales prospect set, which is in the low thousands at GA scale); Likelihood 3 (HubSpot had a 2022 customer-data exposure incident affecting a small number of accounts — public, disclosed; otherwise mature posture).

**Residual risk (3.6):** CEF **0.40** — two of four conditions met: (a) SOC 2 Type II current, (b) compensating controls (DSR-erasure path documented, prospect-only data scope, no customer-tenant data); (c) **no tested failover** — if HubSpot is down the enterprise-inquiry form fails open (queued to D1 `inquiry_dlq` table, replayed when HubSpot recovers) but a multi-week HubSpot outage would force migration to an alternate CRM (candidates: Pipedrive, Salesforce, Close); (d) data shared is PII — does not meet "no plaintext customer data" bar.

**Top risks monitored:**

1. HubSpot account compromise via stolen credentials → sales pipeline exfiltration. Mitigation: SSO via Google Workspace, MFA mandatory, private app token scoped narrowly to `contacts.write` + `deals.write` only, token rotation quarterly.
2. Private app token leak (e.g., committed to git) → write-only token means **no read exfil path** but spam injection possible. Mitigation: token-scanning CI (`scripts/scan-secrets.sh`); allowlist on HubSpot side restricts inbound IP to CoreLink Workers egress range.
3. HubSpot DSR-erasure SLA breach → CoreLink-side DSR cannot complete on time. Mitigation: DSR runbook `specs/_runbooks/RB-DSR-FULFILLMENT.md` includes HubSpot-erasure step; 14-day buffer built into customer-facing 30-day GDPR/LGPD SLA.
4. Cross-border transfer concern (BR prospect → US/EU HubSpot region). Mitigation: explicit consent captured at form submission; SCCs Module 2 in DPA; LGPD legitimate-interest documented for sales contact.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| HubSpot Support (Starter tier) | HubSpot in-app chat + support@hubspot.com | 24h business hours |
| Account executive | (assigned at contract; recorded in `legal/vendor-contacts.md`) | 1 business day |
| Security / abuse | security@hubspot.com | 24h |
| Privacy / DPA | privacy@hubspot.com | 5 business days |
| Breach notification | privacy@hubspot.com + security@hubspot.com → notify security@hugr.dev | Per DPA Section "Personal Data Breach" (without undue delay) |

## 9. Termination / exit plan

If HubSpot terminates CoreLink or pricing/feature drift forces migration:

1. **Day 0..7:** export all contacts + companies + deals + engagement history via HubSpot's CRM Export tool (CSV + JSON via API).
2. **Day 7..30:** stand up alternate CRM (candidate primary: Pipedrive for similar pricing/feature fit; secondary: HubSpot Free if downgrade is acceptable; tertiary: Salesforce if enterprise scale demands it). Re-create pipelines + workflows + email sequences.
3. **Day 30..45:** swap enterprise-inquiry form Forms API endpoint via Wrangler env var; deprecate HubSpot private app token; deactivate HubSpot meeting links + reroute calendar booking.
4. **Day 45..60:** HubSpot data retention/deletion request submitted (30-day grace per DPA); confirm purge complete.

**RTO:** 30 days (worst case) for full CRM migration. **RPO:** form-submission DLQ handles up to 24h gap; longer requires manual replay.

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 3.6 | Initial DD; closes GAP-14 Important-tier batch. CEF 0.40 — narrow scope (sales-prospect-only PII) but no tested failover. |

Next bi-annual review: **2026-11-15**.
