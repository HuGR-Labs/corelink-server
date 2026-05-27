---
id: "RB-ONCALL-ESCALATION-MATRIX"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "oncall", "escalation", "matrix", "3-tier", "sev1", "sev2", "sev3", "pagerduty", "wt-r6-3", "wi-s17-005", "bcp-dr"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §1.2.

# RB-ONCALL-ESCALATION-MATRIX — 3-Tier Escalation Matrix (SEV1/2/3)

> **Parent:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md` (R-6 90-day cadence companion).
>
> **Anchors:** `crates/corelink-ops/src/oncall/` (WI-S17-005 PD schedule + fatigue; absorbed Wave 35 P2 from `corelink-oncall`), `specs/_runbooks/RB-ONCALL-POLICY.md` (rotation policy + Tier 1/2/3 names), `specs/03_architecture/failure_modes.md` §FM-202/203/258, spec contract §5.5 R-S17-12/13.
>
> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** canonical escalation behaviour for all CoreLink production + staging incidents, mapped to PagerDuty schedule slugs + comms templates.
>
> **Supersedes:** the partial escalation table inside `RB-ONCALL-POLICY.md` §5 — that doc retains rotation/fatigue concerns; this doc owns **severity × tier × SLA** + **comms templates** + **conflict tie-breakers**.

## 1. Tier definitions

| Tier | Role | Who | Auth | Replaceable |
|---|---|---|---|---|
| **L1** | Primary on-call engineer | SRE rotation per `RB-ONCALL-POLICY.md` §3 + region overlap §8 | Pages, runbook execution, restart/redeploy, feature-flag toggle, status-page yellow | Backup engineer (PD override) per RB-ONCALL-POLICY §3 |
| **L2** | Engineering manager on-call | EM rotation (weekly, monthly while team < 4 EMs); pre-GA: Gustavo (founder) is permanent L2 backstop | Customer comms approval, status-page orange/red, deploy-freeze, paging L3 | Co-founder / senior staff engineer (named in PD override) |
| **L3** | CTO / Legal on-call | CTO (Gustavo, pre-GA); Legal-on-call rotation 24/7 contracted with outside counsel | Public statement approval, regulator-notification authorisation, ToS invocation (suspend customer), CMK emergency revoke per `RB-BYOK-REVOKE.md` | None below GA scale — escalate to board chair for prolonged unreachability |

**Auto-escalation timers** (from `RB-ONCALL-POLICY.md` §5, normative source):

- `tier_1_to_tier_2_after_seconds: 300` (5 min)
- `tier_2_to_tier_3_after_seconds: 600` (10 min)
- L3 → no auto-escalation; manual page-up only (avoid wake-paging on flapping alerts).

## 2. Severity definitions

| Sev | Trigger criteria | Customer-facing? | Examples |
|---|---|---|---|
| **SEV1** | Production unavailability OR data-integrity violation OR security incident with confirmed blast radius OR cross-tenant boundary breach | YES (status page + email + customer-success outreach) | FM-101 edge outage; FM-253 cross-tenant read; FM-156 supply-chain; FM-258 insider; full-region failure |
| **SEV2** | Significant degradation but not full outage; SLO burn-rate > 14×; partial-region failure; security finding with bounded blast radius | Mostly NO (status-page yellow if customer-visible) | FM-005 DO rebalance latency spike; FM-052 R2 consistency; FM-152 Neon outage (read-only mode); FM-202 stale runbook discovered mid-incident |
| **SEV3** | Minor degradation; SLO burn-rate 1–14×; recoverable within shift; drift / hygiene findings | NO | FM-150 CF API rate-limit; FM-201 config rollback; FM-206 Terraform drift; FM-153 Grafana outage |

> **SEV0 reserved:** for catastrophic existential events (regulator subpoena, mass-data-exfil confirmed by forensics). SEV0 procedures live in `RB-POSTMORTEM-PROCESS.md` §extreme; bypass this matrix and page **all** of L1+L2+L3+board chair simultaneously. Not exercised in R-6 drills.

## 3. Severity × tier × SLA matrix

### SEV1

| Metric | L1 | L2 | L3 |
|---|---|---|---|
| Page-within | ≤ 60s of detection | ≤ 5 min (300s auto via PD) | ≤ 15 min (manual page-up by L2 OR auto-page-up after 10 min if L2 unack) |
| Ack SLA | ≤ 5 min (300s; MTTA target per `RB-ONCALL-POLICY.md` §6) | ≤ 10 min | ≤ 20 min |
| Comms cadence | Continuous Slack `#incident-live`; status update every 15 min | Status-page update every 30 min until resolved; customer-success email every 60 min | Public statement decision at T+60 min; regulator-notification go/no-go at T+4h (see §4) |
| Resolution target | MTTR < 30 min (P50 target per `RB-ONCALL-POLICY.md` §6); hard cap escalation at T+2h | n/a | n/a |

### SEV2

| Metric | L1 | L2 | L3 |
|---|---|---|---|
| Page-within | ≤ 5 min of detection | ≤ 30 min (only if L1 cannot resolve within 30 min) | Not paged unless L2 explicitly escalates |
| Ack SLA | ≤ 15 min | ≤ 30 min | n/a |
| Comms cadence | Slack `#incident-live`; status update every 30 min | Status-page yellow within 60 min IF customer-visible | n/a |
| Resolution target | MTTR < 4h; hand-off allowed at shift boundary per `RB-ONCALL-POLICY.md` §4 | n/a | n/a |

### SEV3

| Metric | L1 | L2 | L3 |
|---|---|---|---|
| Page-within | ≤ 15 min (within-business-hours); next-business-day after-hours | Not paged | Not paged |
| Ack SLA | ≤ 30 min (within-business-hours); 4 business hours after-hours | n/a | n/a |
| Comms cadence | Slack `#incident-live` opened only if duration > 2h; otherwise async ticket | n/a | n/a |
| Resolution target | MTTR < 1 business week; rolled into postmortem if pattern-repeated | n/a | n/a |

## 4. PagerDuty routing-key map

| Sev | Surface | Synthetic route | Production route | Schedule slug |
|---|---|---|---|---|
| SEV1 | `pd-sev1-prod` | `pd-sev1-synthetic` | `pd-sev1-production` | `corelink-primary-us-east` + `corelink-primary-eu-west` |
| SEV2 | `pd-sev2-prod` | `pd-sev2-synthetic` | `pd-sev2-production` | same as SEV1 (single rotation, severity-discriminated alert) |
| SEV3 | `pd-sev3-prod` | `pd-sev3-synthetic` | `pd-sev3-production` | same; SEV3 suppresses pages outside business hours via PD schedule layer |

> **Why split synthetic vs production at the routing-key layer?** Per spec contract §5.5 and `RB-SYNTHETIC-PAGE-DRILL.md` §2, synthetic-page false-positives must not contaminate production MTTA metrics; the split also enables the DR-012 drill (synthetic-vs-real identification rate ≥ 95%).

**Tier escalation policy IDs** (per PD config in `infra/pagerduty/schedule.yaml`):

| Tier path | PD policy ID slug | Auto-timer (s) |
|---|---|---|
| L1 → L2 | `tier-1-to-2-prod` | 300 |
| L2 → L3 | `tier-2-to-3-prod` | 600 |
| L3 → manual | `tier-3-manual` | ∞ (manual only) |

## 5. After-hours / weekend / holiday rules

> **Canonical source for rotation timing:** `RB-ONCALL-POLICY.md` §3 + §4 (shift boundaries, fatigue caps, follow-the-sun overlap). This section codifies severity-handling **deviations** during off-hours, not the rotation itself.

| Window | SEV1 | SEV2 | SEV3 |
|---|---|---|---|
| Weekday business hours (09:00–18:00 local on-call timezone) | Full matrix per §3 | Full matrix per §3 | Full matrix per §3 |
| Weekday after-hours (18:00–09:00) | Full matrix; L2 escalation auto-fires; L3 paged at T+15 min as normal | L1 acks; L2 paged only if not resolved within 60 min | L1 acks at next-business-day; no L2/L3 |
| Weekend | Full matrix; PD weekend overlay activates secondary engineer if primary unack > 8 min | L1 acks; L2 paged only if not resolved within 4h | Deferred to next business day; SEV3 alerts queue in PD low-priority |
| Public holiday (per `RB-ONCALL-POLICY.md` §4 holiday list) | Full matrix; CTO MUST be reachable (no opt-out for L3 during pre-GA) | Same as weekend | Deferred to next business day |
| Pre-GA `holiday-freeze` (Dec 22 – Jan 2) | Full matrix; founder (Gustavo) is permanent L1+L2+L3 fallback; **no deploys** | L1 acks; resolution may slip to post-freeze | Deferred |

**Backup activation lead time:** per `RB-ONCALL-POLICY.md` §11 (PTO/leave), backup MUST be activated ≥ 14 days in advance; ad-hoc weekend swap allowed within `corelink-oncall::rotation::SwapWindow` rules.

## 6. Conflict resolution + incident-commander tie-breakers

The **incident commander (IC)** is L1 by default, transitioning to L2 when L2 joins, and to L3 only if L3 explicitly takes the role (CTO discretion). Conflicts:

| Conflict | Tie-breaker |
|---|---|
| L1 + L2 disagree on customer-comms wording | L2 has final say (auth per §1); L1 documents disagreement in incident timeline |
| L1 + L2 disagree on whether to declare SEV1 | Default UP — declare SEV1; L2 can downgrade after 30 min if data does not support; downgrade event logged in audit |
| L2 + L3 disagree on public statement | L3 has final say; L2 documents in timeline; postmortem MUST review |
| Two L2s on call (rare; handoff window) | Outgoing L2 retains IC role until L1 ack-handoff completes per `RB-ONCALL-POLICY.md` §4 |
| L3 unreachable for SEV1 > 30 min | Co-founder / senior staff engineer named in PD override assumes L3 authority; logged in incident audit + 5-Why postmortem mandatory per `RB-ONCALL-POLICY.md` §10 |
| L1 fatigue score > 70 (`DASH-ONCALL-FATIGUE`) at page time | L2 auto-paged in parallel; handoff to backup engineer triggered per `RB-ONCALL-POLICY.md` §3 |
| Privacy + legal disagree on regulator notification (GDPR Art. 33 timer) | Legal-on-call has final say; privacy lead documents dissent in DPIA delta |
| Customer success + comms disagree on customer-email cadence | L2 IC decides; comms-as-of-record retained in `specs/_audits/` |

## 7. Comms templates (canonical)

> All templates live in `specs/_communications/` after R-2 wiring; this section is the auditable canonical reference + frozen text for the R-6 cadence. Substitute `{{...}}` placeholders at send time.

### 7.1 Status page — SEV1 initial (within 5 min of declaration)

```
[Investigating] CoreLink — service degradation
We're investigating reports of {{symptom}} affecting {{scope}}. Our team is engaged.
Updates every 15 minutes. Next update: {{next_update_utc}} UTC.
```

### 7.2 Status page — SEV1 mitigated

```
[Identified] CoreLink — root cause identified
We've identified the root cause as {{root_cause}}. Mitigation is in progress; expected
recovery within {{eta_minutes}} minutes. We'll update when fully restored.
```

### 7.3 Status page — SEV1 resolved

```
[Resolved] CoreLink — service fully restored
The incident is fully resolved as of {{resolved_utc}} UTC. Total duration: {{duration}}.
A full postmortem will be published within 5 business days at status.corelink.humangr.com/postmortems.
```

### 7.4 Customer email — SEV1 (sent by L2 after status-page identified phase)

```
Subject: CoreLink incident update — {{incident_id}}

We're writing to update you on an ongoing incident affecting {{scope}} that started at
{{started_utc}} UTC. Current status: {{status}}. We expect recovery by approximately
{{eta_utc}} UTC.

Customer impact for your tenant: {{tenant_impact_summary}}.
What we're doing: {{mitigation_summary}}.
What you can do: {{customer_action}}.

We'll send a final update when the incident is fully resolved, followed by a public
postmortem within 5 business days. Reach us at incidents@humangr.com for any
immediate concerns.

— CoreLink Engineering
```

### 7.5 Internal Slack — SEV1 declaration (auto-posted by PD webhook)

```
:rotating_light: SEV1 declared — {{incident_id}}
IC: {{l1_engineer}} (L1)
Symptom: {{symptom}}
Detection: {{detection_source}} at {{detected_utc}}
Status page: {{status_page_url}}
Slack channel: #incident-{{incident_id}}
PD: {{pd_incident_url}}
Runbook: {{runbook_url}}
Next status update: {{next_update_utc}}
```

### 7.6 Internal Slack — SEV2 declaration

```
:warning: SEV2 declared — {{incident_id}}
IC: {{l1_engineer}}
Symptom: {{symptom}}
Customer-visible: {{yes_no}}
Slack channel: #incident-{{incident_id}}
PD: {{pd_incident_url}}
Runbook: {{runbook_url}}
```

### 7.7 Internal Slack — SEV3 ticket

```
:information_source: SEV3 — {{incident_id}}
Owner: {{l1_engineer}}
Symptom: {{symptom}}
PD: {{pd_incident_url}}
Target resolution: {{target_business_day}}
```

### 7.8 Legal-comms holding statement (SEV1 cross-tenant or supply-chain — pre-drafted by legal-on-call within 30 min)

```
We've identified a potential security incident affecting {{scope}}. We are actively
investigating with our security team and outside counsel. We will provide a detailed
update as soon as we have verified information. We take the security of customer
data extremely seriously and will communicate transparently throughout this process.
```

> **Hold-statement constraint:** No claims about scope, blast radius, or affected parties until forensics confirms. See `specs/_legal/` for full breach-notification timing rules (GDPR Art. 33 — 72h to supervisory authority from awareness; US state laws vary — legal-on-call holds the canonical timeline).

## 8. Drill integration

Each drill in `BCP-DR-DRILL-CADENCE.md` exercises this matrix:

| Drill | Tiers exercised | Comms templates exercised |
|---|---|---|
| DR-001..004 (P1) | L1 only | §7.7 (SEV3 ticket pattern) |
| DR-005..008 (P2) | L1 + L2 | §7.1, §7.2, §7.6 |
| DR-009..010 (P3 SEV1) | L1 + L2 + L3 | §7.1–§7.5, §7.8 |
| DR-011..014 (X) | L1 (+ L2 for DR-014 manual gate) | §7.7 (SEV3 ticket) |

## 9. Cross-references

- `specs/_runbooks/RB-ONCALL-POLICY.md` — rotation, fatigue, handoff (this doc supersedes its §5 escalation table)
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — postmortem workflow
- `specs/_runbooks/RB-TABLETOP-TEMPLATE.md` — tabletop frame for SEV1 rehearsal
- `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` — lighthouse-specific comms
- `specs/_runbooks/RB-BYOK-REVOKE.md` — L3-auth CMK revoke
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — drill cadence parent
- `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` — evidence template
- `crates/corelink-ops/src/oncall/` — PD schedule + fatigue + page audit ledger (absorbed Wave 35 P2 from `corelink-oncall`)
- `ROADMAP-TO-GA.md` §6 — R-6 Wave; H-3 PagerDuty account provisioning
