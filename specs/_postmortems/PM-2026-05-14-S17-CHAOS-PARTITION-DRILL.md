---
id: "PM-2026-05-14-S17-CHAOS-PARTITION-DRILL"
type: "post_mortem"
incident_id: "INC-2026-05-14-001"
severity: "SEV-2"
doc_status: "DRAFT"
owner: "Gustavo Schneiter"
reviewers:
  - role: "sre_lead"
    name: "Gustavo Schneiter"
  - role: "eng"
    name: "Peer Reviewer (outside affected team)"
final_approver: "Gustavo Schneiter"
blameless_pledge: true
sprint_link: "S-17"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
supersedes: null
superseded_by: null
tags:
  - "postmortem"
  - "blameless"
  - "sev-2"
  - "synthetic"
  - "chaos"
  - "partition"
  - "s17"
  - "wi-s17-004"
  - "wi-s17-001"
  - "wi-s17-003"
---

# PM-2026-05-14-S17-CHAOS-PARTITION-DRILL — Synthetic SEV-2: Staging KV→D1 Partition Drill

> **Synthetic test postmortem** produced as part of WI-S17-004 acceptance
> criterion 10.s17.004.4 ("1 synthetic SEV-2 incident test full flow").
> The "incident" is a chaos-engineering drill executed in **staging only**;
> no production tenants were affected. This document exercises the
> blameless postmortem template end-to-end against a realistic scenario.
>
> **Blameless pledge**: every contributor to this doc focuses on the
> conditions and systems that allowed the simulated incident to escalate,
> not on the people who happened to be on the keyboard during the drill.

---

## 1. Summary

On **2026-05-14 14:02 UTC**, the S-17 chaos scheduler (WI-S17-001)
injected a **network partition** between the `corelink-staging` Workers
runtime and the regional D1 audit database for **8 minutes** in the
`weur` region. The synthetic incident was intentionally escalated to
SEV-2 to drill the full oncall flow.

Detection fired in **47 s** via the `corelink_audit_outbox_lag_seconds`
alert. Oncall acknowledged in **2 m 11 s**, executed
`RB-FM-202` (audit-outbox stall) and `RB-FM-305` (region degrade), and
verified recovery at **14:14 UTC** (total drill duration **12 m**,
within SEV-2 target MTTR < 2 h).

The drill surfaced two real systemic issues: (a) the audit-outbox
back-pressure alarm fires on a lagging signal that lags the actual
partition by ~30 s, and (b) the staging dashboard for cross-region
audit replay had a stale panel referencing a deprecated metric name.
Both produced action items. **No production traffic was affected.**

---

## 2. Impact

- **Severity**: SEV-2 (synthetic; chaos-injected).
- **Duration of customer impact**: 0 (staging only; no production
  tenant routing during the window).
- **Tenants / users affected**: internal only — staging synthetic tenant
  `t-staging-chaos-01`.
- **SLO budget burned**: 0 (staging excluded from SLO accounting per
  `specs/03_architecture/slo_catalog.md` §4.2).
- **Revenue / contractual impact**: none.
- **Trust / reputational impact**: none. Drill was pre-announced 24 h
  in advance to the engineering channel per WI-S17-001 chaos catalog
  rules.

---

## 3. Root Causes

### 3.1 Trigger

At **2026-05-14T14:02:11Z**, the chaos scheduler executed
`chaos-network-partition-weur-d1` (catalog entry 4 of 8, WI-S17-001).
The partition was the intended trigger.

### 3.2 Contributing conditions

The drill was intentional, but two **latent** conditions were
exposed by the partition that would have made a real partition worse:

1. **C1 — Lagging alert signal.** `corelink_audit_outbox_lag_seconds`
   is computed from the *last successful flush ts*, which is itself
   written through the partitioned path. During the partition the
   signal stops advancing rather than spiking, delaying alert
   evaluation by ~30 s versus a direct connectivity probe.
2. **C2 — Stale dashboard panel.** The "Audit replay backlog (weur)"
   panel in the staging Grafana board still queried
   `corelink_audit_replay_backlog_total`, a metric renamed to
   `corelink_audit_outbox_pending_total` in S-15 (WI-S15-002).
   Oncall lost ~90 s diagnosing an empty panel before pivoting to the
   replacement panel.

### 3.3 5-Why analysis

1. **Why did detection take 47 s when the partition was instantaneous?**
   Because the alert fires on a derived signal that depends on the
   partitioned write path.
2. **Why is the alert built on the derived signal?**
   Because at the time the alert was authored (S-15), only the
   write-path lag metric was emitted; an independent connectivity
   probe did not exist.
3. **Why was no independent probe added later?**
   Because the work item that introduced the second metric
   (`corelink_audit_outbox_pending_total`, WI-S15-002) updated the
   dashboards but **not** the alert definitions — the alert review
   step was not in the WI's DoD.
4. **Why was alert review not in the DoD?**
   Because the S-15 work-item template at the time treated alerts as
   "observability scaffolding" rather than as production-load-bearing
   contracts; the spec contract has since (S-16) added an
   "alerts touched?" gate to the WI DoD, but back-fills against
   pre-S-16 alerts have not been scheduled.
5. **Why have back-fills not been scheduled?** (systemic root)
   Because there is no inventory of alerts whose authoring predates
   the S-16 alert-review gate. **Action AI-1** addresses this.

---

## 4. Trigger

`chaos-network-partition-weur-d1` chaos run executed at
`2026-05-14T14:02:11Z` by the chaos scheduler (WI-S17-001), correlation
ID `9e3c7f12-…-staging-drill`. Run artifact:
`specs/_audits/2026-05-14-synthetic-incident-test.md` (to be authored
alongside this PM).

---

## 5. Resolution

- **Mitigation (14:09:47Z)**: oncall executed `RB-FM-305` "region
  degrade" §3 — flipped the `weur` audit write target to the
  cross-region fallback (`wnam`) via the audit-router DO config.
- **Verification (14:14:02Z)**:
  `corelink_audit_outbox_pending_total{region="weur"}` decreased to
  zero; `corelink_audit_chain_integrity_check_total{outcome="ok"}`
  resumed monotonic increase.
- **Permanent fix status**: not required — the partition was
  drill-injected and ended on its scheduled timer at
  `2026-05-14T14:10:11Z`. The mitigation runbook itself worked as
  designed; action items address the **detection** and **dashboard**
  gaps, not the resolution path.
- **Data-repair**: none. Audit outbox drained on partition heal with
  zero loss (verified via Merkle chain check
  `corelink_audit_chain_integrity_check_total`).

---

## 6. Detection

- **How detected**: `corelink_audit_outbox_lag_seconds` exceeded its
  60 s threshold; PagerDuty alert `audit-outbox-stall-weur` fired.
- **TTD (time-to-detect)**: 47 s from partition injection to first
  alert. **Target for SEV-2 was ≤ 60 s — met, but only barely.**
- **MTTA (time-to-acknowledge)**: 2 m 11 s (target SEV-2 ≤ 15 m — met).
- **MTTR (time-to-resolve)**: 11 m 51 s end-to-end
  (target SEV-2 ≤ 2 h — met with large margin).
- **Detection gaps**:
  - The derived-signal lag noted in §3.3 — addressed by AI-2.
  - No independent partition-detection alert — addressed by AI-2.

---

## 7. Action Items + Lessons Learned

### 7.1 Action items

| ID | Type | Item | Owner team | Due | Sprint decision | Status |
|---|---|---|---|---|---|---|
| AI-1 | process | Inventory all alerts whose definitions predate the S-16 alert-review gate; back-fill review for any whose target system has changed. | SRE | 2026-06-15 | accepted (S-17 sprint owner) | open |
| AI-2 | detect | Add an independent `corelink_kv_to_d1_probe_rtt_seconds` synthetic probe + alert with a 15 s threshold, independent of the audit write path. | Platform | 2026-05-30 | accepted | open |
| AI-3 | documentation | Audit all staging Grafana boards for references to metrics renamed in S-15 (`corelink_audit_replay_backlog_total` → `corelink_audit_outbox_pending_total`, and any others). Replace or remove. | SRE | 2026-05-21 | accepted | open |
| AI-4 | prevent | Add a CI lint that fails any PR introducing a Grafana panel referencing a metric name that does not exist in the current metrics catalog. | Platform | 2026-06-15 | deferred to S-18 (capacity) | open |
| AI-5 | process | Add "drill-discovered detection gaps" as a standing input to the S-17 runbook-drill tracker (WI-S17-003) review log. | SRE | 2026-05-21 | accepted | open |

### 7.2 Lessons learned

**What went well**

- The chaos scheduler injected exactly the configured fault, ended on
  its timer, and produced clean correlation IDs end-to-end.
- The audit Merkle chain verified zero loss across the partition — the
  outbox + chain design (S-08 / S-15) worked as advertised.
- Oncall followed `RB-FM-305` step-for-step; no improvisation needed.
  The runbook itself was last dry-run on **2026-04-22** (WI-S17-003
  drill log entry RB-FM-305-2026-04), so it was recent enough to be
  trusted.

**What went poorly** (system / process gaps, not people)

- Detection rode on a derived signal that depends on the partitioned
  path (§3.3) — a structural observability anti-pattern that needs a
  layered probe to break.
- One staging dashboard panel still referenced a metric name renamed
  five months ago, delaying triage by ~90 s.
- The S-15 metric-rename WI did not include an "update alerts" step
  in its DoD; that gap was already closed at the template level in
  S-16 but pre-existing alerts have not been audited.

**Where we got lucky**

- The drill ran in **staging**. Had a real `weur` partition occurred
  before this drill exposed the lagging-signal issue, MTTA would
  likely have been worse and might have eaten visible SLO budget.

---

## 8. Timeline

All timestamps UTC, ISO-8601. Correlation ID
`9e3c7f12-…-staging-drill` propagated end-to-end (PAT-CORRELATION-ID-001).

| ts (UTC) | actor | event | source |
|---|---|---|---|
| 2026-05-13T14:00:00Z | chaos scheduler | drill announced in `#eng-chaos` 24 h prior | Slack archive |
| 2026-05-14T14:02:11Z | chaos scheduler | `chaos-network-partition-weur-d1` injected | WI-S17-001 catalog, run log |
| 2026-05-14T14:02:58Z | alerting | `audit-outbox-stall-weur` fires (TTD 47 s) | PagerDuty incident PD-2026-05-14-staging-001 |
| 2026-05-14T14:05:09Z | oncall (synthetic IC) | acknowledged page (MTTA 2 m 11 s) | PagerDuty |
| 2026-05-14T14:05:30Z | oncall | opened drill bridge; began RB-FM-202 §3 | bridge log |
| 2026-05-14T14:06:55Z | oncall | dashboard panel "audit replay backlog (weur)" empty → pivoted to replacement panel | Grafana audit log |
| 2026-05-14T14:08:20Z | oncall | confirmed partition (not a flush stall) — switched to RB-FM-305 §3 | bridge log |
| 2026-05-14T14:09:47Z | oncall | executed audit-router DO failover `weur → wnam` | RB-FM-305 §3 |
| 2026-05-14T14:10:11Z | chaos scheduler | partition ended on scheduled timer | WI-S17-001 run log |
| 2026-05-14T14:14:02Z | oncall | verified recovery: outbox pending = 0, chain integrity ok | metrics + bridge log |
| 2026-05-14T14:18:00Z | oncall | drill closed; PM draft scheduled within 5 days | bridge log |
| 2026-05-14T16:30:00Z | author (this doc) | PM draft posted for SRE-lead + peer review | this document |

---

## 9. Reviews & sign-off

| Role | Name | Decision | Date |
|---|---|---|---|
| Author | Gustavo Schneiter | drafted | 2026-05-14 |
| SRE Lead (canonical reviewer) | _pending_ | _pending_ | _pending_ |
| Peer reviewer (outside affected team) | _pending_ | _pending_ | _pending_ |
| Security Lead | n/a (no security incident) | — | — |
| Privacy Officer | n/a (no PII in scope; staging synthetic tenant) | — | — |
| Final approver | Gustavo Schneiter | _pending_ | _pending_ |

Two reviewers required (SRE lead + one peer outside the affected team).
Author has not self-certified blamelessness — final state requires both
external sign-offs per `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` §5.

---

## 10. Cross-references

- **Drill source**: WI-S17-001 chaos catalog, entry
  `chaos-network-partition-weur-d1`.
- **Drill-tracker evidence** (WI-S17-003 parallel): drill response was
  recorded against `RB-FM-305` in
  `specs/04_runbooks/_dry_run_log.md` row `RB-FM-305-2026-05-drill`
  (to be appended by WI-S17-003 cycle owner).
- **Runbooks executed**: `RB-FM-202`, `RB-FM-305`.
- **Spec contract clauses honored**: §5.3 (runbook cadence), §5.4
  R-S17-11 (sprint linkage), §9.8 (MTTA / MTTR targets).
- **Templates exercised**: `templates/incident.md`,
  `templates/postmortem.md` (this doc).

---

**End PM-2026-05-14-S17-CHAOS-PARTITION-DRILL.**
