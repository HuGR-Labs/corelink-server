---
id: "DD-PAGERDUTY-2026-05-15"
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
tags: ["soc2", "cc9.2", "vendor-dd", "important", "pagerduty", "incident", "oncall", "gap-14"]
---

# Vendor DD — PagerDuty, Inc.

> **doc_status:** ACTIVE · Register row: 9 · Category: **Important** · Residual risk: **2.5** (Inherent 10 × CEF 0.25).
>
> PagerDuty is CoreLink's incident-management and on-call alerting layer for both **production alerts** and **synthetic-drill routes** (chaos drills, tabletop exercises, BCP/DR rehearsals). It does not hold customer-tenant data; payloads are alert metadata + run-log references.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | PagerDuty, Inc. (Delaware) — NYSE: PD |
| HQ | 600 Townsend Street, Suite 200, San Francisco, CA 94103, USA |
| EEA / UK entity | PagerDuty Limited (London) — for EEA/UK data-processing |
| Contract effective | 2026-04-23 (Professional plan; click-through) |
| Account contacts | (recorded in `legal/vendor-contacts.md`); PagerDuty account ID + integration keys in `secrets-checklist.md` row 19 |
| Primary integration | Events API v2 (per `secrets-checklist.md` row 19) — write-only routing keys per service |

## 2. Service scope

- **PagerDuty Incident Response** — on-call schedules, rotations, escalation policies, override management
- **Events API v2** — alert ingestion from CoreLink Workers (`apps/server` alert routing layer) + Grafana Cloud alerting webhook + synthetic drill harness
- **Notifications** — push, SMS, voice, email to responders (per-responder contact preferences)
- **Incident analytics** — MTTR / MTTA dashboards (used by R7 reliability review)
- **Status page integration** (not used — CoreLink uses Atlassian Statuspage directly, register row 18)
- **Runbook automation** (not used — CoreLink runbooks live in `specs/_runbooks/`)

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Alert payload (service name, severity, dedup key, error summary, runbook URL) | CoreLink → PagerDuty (Events API v2) | PagerDuty-managed (AES-256) | TLS 1.3 | PagerDuty |
| Responder contact info (name, email, phone, push device) | CoreLink team → PagerDuty | PagerDuty-managed | TLS 1.3 | PagerDuty |
| On-call schedule + rotation data | CoreLink team → PagerDuty | PagerDuty-managed | TLS 1.3 | PagerDuty |
| Incident timeline + responder notes | CoreLink team ↔ PagerDuty | PagerDuty-managed | TLS 1.3 | PagerDuty |
| Routing key (per service) | PagerDuty → CoreLink (one-time at integration setup) | `cf-wrangler` secret store | TLS 1.3 | CoreLink |

**Data classification:** alert payloads include **service names, error type/summary, and runbook URLs** — no customer-tenant data, no customer PII, no credentials. Alert summaries are intentionally scrubbed of customer identifiers (CoreLink alert middleware in `apps/server/lib/alerts/redact.ts` strips tenant IDs from messages before posting to PagerDuty). Responder PII (team members) is the larger PII surface — handled per CoreLink employee privacy policy.

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 2 Type II | Attested | PagerDuty Trust portal (https://www.pagerduty.com/security/) — Drata vendor module pulls annually |
| ISO 27001 / 27017 / 27018 | Certified | PagerDuty Trust portal |
| GDPR | Processor; SCCs Module 2; EU data-residency option via EU region | PagerDuty DPA |
| LGPD | Processor; data hosted in US/EU (no BR region) — minimal PII scope reduces residency concern | PagerDuty DPA |
| CCPA | Processor | PagerDuty DPA |
| HIPAA | BAA available; not signed (no PHI) | — |
| FedRAMP | Not authorized at commercial tier (Federal SKU separate) | N/A for CoreLink |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| PagerDuty Master Subscription Agreement | 2026-04-23 (click-through) | Professional plan |
| PagerDuty DPA | 2026-04-23 (unilateral acceptance) | SCCs Module 2 |
| Standard SLA | 99.9% Events API ingestion + notification SLA | Per PagerDuty SLA addendum |
| BAA | Not signed | No PHI in scope |

## 6. Control mapping

| CoreLink CTRL | Dependency on PagerDuty |
|---|---|
| CTRL-OBS-005 | Primary alert notification channel for SEV-1/SEV-2 incidents |
| CTRL-IR-002 | On-call rotation + escalation policy (NIST 800-61 alignment per `specs/_compliance/IR-PLAYBOOK.md`) |
| CTRL-IR-003 | Tabletop exercise synthetic-alert routing (per `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`) |
| CTRL-IR-004 | Incident timeline evidence (used as cross-reference in post-mortems) |
| CTRL-BCP-DR-009 | BCP/DR drill alert routing (chaos drills page on-call through PagerDuty) |

## 7. Risk assessment narrative

**Inherent risk (10 = 2 × 5):** Impact 2 (PagerDuty down does not break customer surface; degrades CoreLink's ability to notice + respond to incidents; mitigated by Grafana Cloud secondary alerting path + SMS fallback); Likelihood 5 (PagerDuty has had **recurrent incidents** in the last 12 months, including the 2024-08 Events API ingestion degradation and 2025 notification-delivery delays — vendor maturity is high but incident frequency is non-trivial).

**Residual risk (2.5):** CEF **0.25** — three of four conditions met: (a) SOC 2 Type II current, (b) compensating controls (alert payload redaction, Grafana Cloud secondary alerting, on-call SMS fallback via Twilio direct, manual escalation playbook), (c) tested failover (Grafana Cloud → Slack `#alerts-fallback` channel exercised in 2026-04 chaos drill — see `specs/_audits/2026-04-region-outage-chaos.md`); (d) data shared is alert metadata + responder PII — limited customer-impact scope.

**Top risks monitored:**

1. PagerDuty Events API ingestion outage → CoreLink alerts not paged. Mitigation: Grafana Cloud parallel alert path → Slack `#alerts-fallback` + Twilio-direct SMS to primary on-call; chaos-drill-tested.
2. Notification delivery delay (push/SMS/voice) → on-call missed alert. Mitigation: multi-channel notification policy (push → SMS → voice → email at 2-min intervals); escalation to secondary on-call after 5 min.
3. Responder PII leak via PagerDuty compromise → social-engineering surface against CoreLink team. Mitigation: responder profile data minimization (work email + work phone only; no personal contacts); SSO via Google Workspace with MFA mandatory.
4. Routing key exfiltration → attacker can inject false alerts. Mitigation: alert-payload signature verification on PagerDuty side (HMAC over routing key + dedup key); CI scanning for committed routing keys.

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| PagerDuty Support (Professional plan) | PagerDuty Console → Support + support@pagerduty.com | 4h business hours; 1h for SEV-1 |
| Customer Success Manager | (assigned at contract; recorded in `legal/vendor-contacts.md`) | 1 business day |
| Security / abuse | security@pagerduty.com | 24h |
| Privacy / DPA | privacy@pagerduty.com | 5 business days |
| Breach notification | security@pagerduty.com + privacy@pagerduty.com → notify security@hugr.dev | Per DPA (without undue delay; target 72h) |

## 9. Termination / exit plan

If PagerDuty terminates CoreLink, raises prices, or sustained outage forces migration:

1. **Day 0..3:** export on-call schedules + escalation policies + incident history via PagerDuty API; mirror responder contact preferences to a flat YAML.
2. **Day 3..14:** stand up alternate (candidate primary: Opsgenie [Atlassian] for similar feature set; secondary: Grafana OnCall integrated with existing Grafana Cloud account; tertiary: incident.io for IR-workflow-first product). Re-import schedules + policies via the alternate's import tooling.
3. **Day 14..21:** swap Events API integration keys in `apps/server` via Wrangler env var; deprecate PagerDuty routing keys; cut DNS for any PagerDuty-hosted webhooks.
4. **Day 21..30:** decommission PagerDuty account; submit data-retention/deletion request; confirm purge complete.

**RTO:** 14 days for primary on-call routing; full historical analytics migration takes 30 days. **RPO:** alert payloads are not authoritative state — none lost on cutover (alerts are emitted continuously).

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 2.5 | Initial DD; closes GAP-14 Important-tier batch. CEF 0.25 — strong attestations + tested Grafana failover path. |

Next bi-annual review: **2026-11-15**.
