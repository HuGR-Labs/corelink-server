---
id: "DD-SLACK-2026-05-15"
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
tags: ["soc2", "cc9.2", "vendor-dd", "important", "slack", "salesforce", "comms", "gap-14"]
---

# Vendor DD — Slack Technologies, LLC (Salesforce)

> **doc_status:** ACTIVE · Register row: 10 · Category: **Important** · Residual risk: **3.6** (Inherent 9 × CEF 0.40).
>
> Slack is CoreLink's internal communications hub and the **secondary alert channel** for production, customer-escalation channels, on-call handoff, breach-notification kickoff, and lighthouse-customer shared channels. Owned by Salesforce since 2021.

---

## 1. Vendor profile

| Field | Value |
|---|---|
| Legal entity | Slack Technologies, LLC (Delaware) — wholly-owned subsidiary of Salesforce, Inc. (NYSE: CRM) since 2021-07-21 |
| HQ | 415 Mission Street, 3rd Floor, San Francisco, CA 94105, USA |
| EEA / UK entity | Slack Technologies Limited (Dublin) — for EEA/UK data-processing |
| Plan | Slack Business+ (Enterprise Grid evaluation deferred until ≥ 100 seats) |
| Contract effective | 2026-04-23 (Business+ plan; click-through) |
| Account contacts | (recorded in `legal/vendor-contacts.md`); workspace ID + bot token + webhook URLs in `secrets-checklist.md` rows 20-22 |
| Primary integrations | (a) Incoming webhooks per channel (`#alerts`, `#alerts-fallback`, `#enterprise-inquiries`, `#oncall-handoff`, `#lighthouse-<customer>`, `#breach-notification`); (b) Bot token for chat-ops + DSR-ticket notifications |

## 2. Service scope

- **Slack channels + DMs** — internal team communication
- **Incoming webhooks** — alert / event / inquiry notification payload delivery
- **Bot user (CoreLinkBot)** — chat-ops surface (deploy approvals, DSR-ticket assignments, drift alerts)
- **Connect (shared channels)** — lighthouse-customer escalation channels (`#lighthouse-<customer>`)
- **Huddles** — voice/video for incident response war-rooms
- **Workflow Builder** — internal on-call handoff automation
- **Audit logs API** — Drata pulls workspace audit events for SOC 2 evidence (`specs/_compliance/DRATA-INTEGRATION-COVERAGE.md`)

CoreLink does **not** consume: Slack AI, Slack Lists, Slack Canvas as a knowledge base of record, Salesforce Data Cloud integration — these are explicitly out of scope.

## 3. Data sharing

| Data category | Direction | Encryption at rest | Encryption in transit | Key holder |
|---|---|---|---|---|
| Alert notification payloads (service, severity, error type, runbook URL) | CoreLink → Slack (incoming webhooks) | Slack-managed (AES-256) | TLS 1.3 | Slack |
| Enterprise-inquiry notifications (prospect name + company + summary) | CoreLink → Slack `#enterprise-inquiries` | Slack-managed | TLS 1.3 | Slack |
| Internal team conversation history | CoreLink team ↔ Slack | Slack-managed (Enterprise Key Management available on Enterprise Grid only — **not used at Business+ tier**) | TLS 1.3 | Slack |
| Shared-channel content (with lighthouse customers) | CoreLink ↔ Customer-via-Slack-Connect | Slack-managed | TLS 1.3 | Slack |
| Audit log events | Slack → Drata (read-only API pull) | Slack-managed → Drata-managed | TLS 1.3 | Slack / Drata |
| Bot token + webhook URLs | Slack → CoreLink (one-time at integration setup) | `cf-wrangler` secret store | TLS 1.3 | CoreLink |

**Data classification:** notification payloads contain **service names, error summaries, prospect names** — no customer-tenant data and no customer PII (alert middleware in `apps/server/lib/alerts/redact.ts` strips tenant identifiers before Slack delivery). Internal-team conversations may contain operational discussion + post-mortem drafts, but **customer-tenant data must not be pasted into Slack** per `specs/_compliance/COMPLIANCE-SOPS.md` §"Data Handling — Slack Channels" (handbook acknowledgement tracked in Drata).

## 4. Regulatory and attestation scope

| Framework | Status | Evidence path |
|---|---|---|
| SOC 2 Type II | Attested | Salesforce Compliance + Slack Trust portal (Drata vendor module pulls annually) |
| ISO 27001 / 27017 / 27018 | Certified | Salesforce Compliance |
| FedRAMP Moderate | Authorized (Slack for Government — separate tenant; CoreLink uses commercial) | N/A for CoreLink commercial workspace |
| GDPR | Processor; SCCs Module 2; EU data-residency option (EKM + EDR on Enterprise Grid only) | Slack DPA |
| LGPD | Processor; data hosted in US/EU (no BR region) — minimal customer-PII scope | Slack DPA |
| CCPA | Processor | Slack DPA |
| HIPAA | BAA available (Enterprise Grid only); not applicable to CoreLink Business+ — no PHI in scope | — |
| PCI-DSS | N/A (no cardholder data) | — |

## 5. Contractual posture

| Document | Signed | Notes |
|---|---|---|
| Slack Customer Terms of Service / Master Subscription Agreement | 2026-04-23 (click-through) | Business+ plan |
| Slack DPA | 2026-04-23 (unilateral acceptance) | SCCs Module 2; https://slack.com/terms-of-service/data-processing-addendum |
| Standard SLA | 99.99% uptime commitment for Business+ | Per Slack SLA addendum |
| BAA | N/A at Business+ tier | No PHI in scope; upgrade to Enterprise Grid required if BAA ever needed |

## 6. Control mapping

| CoreLink CTRL | Dependency on Slack |
|---|---|
| CTRL-OBS-006 | Secondary alert channel (`#alerts-fallback`) — primary failover when PagerDuty down |
| CTRL-IR-005 | War-room channel + Huddle for SEV-1 incident response coordination |
| CTRL-COMM-001 | Internal team communications |
| CTRL-COMM-004 | Customer-escalation Slack Connect channels for lighthouse cohort |
| CTRL-COMM-005 | Breach-notification kickoff channel (`#breach-notification`) — first 30 minutes of IR coordination |
| CTRL-COMPL-007 | Audit-log evidence pulled by Drata for SOC 2 CC6.1 + CC7.2 |

## 7. Risk assessment narrative

**Inherent risk (9 = 3 × 3):** Impact 3 (Slack down degrades internal coordination + secondary alerting + customer Connect channels — but does not break the data plane; primary alerting still works via PagerDuty); Likelihood 3 (Slack has had ≥ 1 public incident in last 12 months — the 2024-02 global outage and 2024-09 file-upload degradation; mature but non-zero).

**Residual risk (3.6):** CEF **0.40** — two of four conditions met: (a) SOC 2 Type II current, (b) compensating controls (alert-payload redaction, no customer-tenant data per policy, audit-log integration with Drata); (c) **no tested failover** for internal comms specifically — when Slack is down team coordination falls back to email + direct phone, which is functional but slow; the **alert path** failover (PagerDuty primary, Slack secondary) is tested but the inverse (Slack primary as comms hub failover) is not; (d) data shared is internal-team PII + lighthouse-customer correspondence — does not meet "no plaintext customer data" bar (Slack Connect content includes customer-side participants).

**Top risks monitored:**

1. Slack global outage → internal coordination + Connect channels offline. Mitigation: documented email-fallback (`team@hugr.dev` distro) + voice-bridge (Google Meet); chaos-drill-tested at low severity in 2026-04.
2. Webhook URL exfiltration → attacker can post spam into alerts channel. Mitigation: webhook-URL allowlist (Slack workspace setting restricts inbound source IP), CI scanning for committed webhooks, per-channel webhook scoping (a leaked `#enterprise-inquiries` webhook cannot post to `#breach-notification`).
3. Bot token leakage → broader chat-ops surface compromise. Mitigation: token-scanning CI (`scripts/scan-secrets.sh`); bot scopes narrowly restricted to `chat:write` + `users:read` only; quarterly rotation.
4. Salesforce parent-company policy change (e.g., AI training on workspace content). Mitigation: workspace settings explicitly opt out of Slack AI training; reviewed at every Slack ToS revision (tracked in `legal/vendor-policy-watch.md`).
5. Lighthouse-customer Connect channel data scope creep (customer pastes regulated data into the shared channel). Mitigation: customer-onboarding briefing + channel-pinned data-handling reminder; Slack DLP not used at Business+ tier (Enterprise Grid feature).

## 8. Escalation contacts

| Role | Contact | SLA |
|---|---|---|
| Slack Support (Business+ plan) | Slack Help Center + feedback@slack.com | 4h business hours |
| Customer Success Manager | (assigned at contract; recorded in `legal/vendor-contacts.md`) | 1 business day |
| Security / abuse | security@slack.com (or via Slack Help Center "Report a security issue") | 24h |
| Privacy / DPA | privacy@slack.com | 5 business days |
| Breach notification | security@slack.com + privacy@slack.com → notify security@hugr.dev | Per DPA (without undue delay; Salesforce policy targets 72h) |

## 9. Termination / exit plan

If Slack/Salesforce terminates CoreLink, raises prices, or shifts policy (e.g., AI training mandate):

1. **Day 0..7:** export workspace via Slack's Standard Export (channels + messages + files) + audit logs via API; back up shared-channel content (Slack Connect export limited — preserved per-channel via manual capture).
2. **Day 7..30:** stand up alternate (candidate primary: Discord for community + Microsoft Teams for ops if forced to Microsoft stack; secondary: Mattermost self-hosted for full data-control posture). Re-create channels + restore message history where feasible (Slack export format is partially portable).
3. **Day 30..45:** swap incoming webhook URLs in `apps/server` + Grafana Cloud + PagerDuty Slack integration via Wrangler env var; deprecate bot token; notify lighthouse customers of Connect-channel cutover (joint channel re-establishment on new platform).
4. **Day 45..60:** decommission Slack workspace; submit data-retention/deletion request; confirm purge complete.

**RTO:** 30 days for full comms migration; 7 days for alert-channel redirect (the high-frequency path). **RPO:** historical Slack messages are not authoritative state for any compliance evidence (audit-log evidence is mirrored in Drata), so RPO is effectively zero for compliance purposes; informal history is best-effort.

## 10. Review history

| Review date | Reviewer | Type | Residual | Notes |
|---|---|---|---|---|
| 2026-05-15 | Gustavo Schneiter (VP-Sec) | Baseline | 3.6 | Initial DD; closes GAP-14 Important-tier batch. CEF 0.40 — strong attestations but Slack Connect customer-side participants and no tested comms-hub failover keep CEF off 0.25. |

Next bi-annual review: **2026-11-15**.
