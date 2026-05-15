---
id: "ACTIVE-FAILOVER-DRILL-SPEC-2026-05-15"
type: "compliance_drill_spec"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-6"
parent_wi: "WT-R6-ACTIVE-FAILOVER"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "dr", "active-failover", "warm-failover", "soc2-cc7-5", "soc2-cc9-1", "soc2-a1-2", "iso-27031", "rb-active-failover", "dr-16", "r-6"]
---

# Active-Region Failover Drill Spec — Warm Switch With Minimal Data Loss

> **doc_status:** DRAFT · **scope:** simulate **degraded** primary region (high latency / partial outage / SLO breach but **not zero**) and validate a warm switch of the active region to its sibling with **RTO ≤ 15 min** + **RPO ≤ 5 min**. Complements `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` (DR-15) which targets the *total loss* / cold-restore scenario.
>
> **Distinguishing trait:** cold-restore (DR-15) rebuilds infra from N-1 backups when the region is *destroyed*; active-failover (DR-16) flips the active write lease to the sibling while the primary is *still partially reachable*. The two drills share zero state but reuse the same residency-graph (`crates/corelink-failover-router::ResidencyGraph`).
>
> **Anchors:** `crates/corelink-failover-router` (multi-signal detection + read routing + write block), `crates/corelink-region` (region taxonomy + provisioning events), `crates/corelink-replica-worker` (ResidencyGraph + replication lag SLO), `specs/_runbooks/RB-ACTIVE-FAILOVER.md` (7-step runbook), `scripts/active-failover-drill.sh` (orchestrator — 3 modes), `tests/e2e-failover-router/` (5-scenario E2E harness).
>
> **Companion:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md` DR-005 (warm failover, weekly P2) — DR-005 is a *single-direction* CAS sibling-route exercise; DR-16 is the *full active write-lease handover* including DNS / CF Worker routing flip plus deterministic failback.
>
> **SOC 2 control coverage:** A1.2 (environmental protections + availability — warm failover exercise), CC7.5 (recovery from disruptions), CC9.1 (risk identification + mitigation testing — active-failover variant).
>
> **ISO 27031 alignment:** §8.4 (ICT readiness tests — warm failover scenario), §9.1 (review + improvement of recovery procedures).
>
> **NIST SP 800-34 alignment:** §3.4.2 parallel testing tier (partial-interruption, write-lease handover).

## 1. Scope + non-goals

### 1.1 In scope (what this drill simulates)

- **Degraded** primary region — *not destroyed*. Definition of "degraded" used as drill trigger:
  - p95 user-facing latency sustained **> 5000 ms** for ≥ 5 minutes; **OR**
  - 5xx error rate sustained **> 5%** for ≥ 5 minutes; **OR**
  - both of the above with the failover-router (`InMemoryFailoverRouter` production analog) already flagging multi-signal Degraded for ≥ 5 minutes.
- **Recovery target:** flip the active write lease from the degraded primary to its sibling region per `ResidencyGraph` (WNAM↔ENAM, WEUR↔SAM); re-route reads + writes via DNS + CF Worker routing; verify zero residency-tag leak; reverse the flip (failback) once the primary recovers.
- **Data-loss budget:** ≤ 5 min of writes (RPO 5 min) — bounded by the cross-region replication checkpoint cadence the failover-router relies on for sibling read consistency.

### 1.2 Out of scope (handled by adjacent drills)

- Full destruction of a region (cold restore — covered by `COLD-RESTORE-DRILL-SPEC.md` / DR-15).
- Single-primitive warm failover (CAS sibling-route only — covered by DR-005 weekly).
- BYOK CMK rotation / compromise (DR-007 / DR-008).
- Customer-comms tabletop with real customers (drill is **staging-only**; production mode exists for *real* warm-failover incidents only).
- Synthetic page exercise (DR-012 weekly).

### 1.3 Threat-model trigger (when DR-16 is invoked, in drill or real)

Active-region failover is invoked when **all three** hold:

1. Primary region is reachable (HTTP returns *some* responses) but exceeds the SLO breach thresholds in §1.1 for ≥ 5 min; **AND**
2. Sibling region per `ResidencyGraph` is itself Healthy (no multi-signal degradation in the last 15 min); **AND**
3. Cross-region replication lag is **within RPO budget** (≤ 5 min) at the moment of declaration — measured against the most recent `corelink-replica-worker` checkpoint timestamp.

If trigger condition 1 holds but the breach has been sustained **≥ 10 min**, the automatic-failover lane fires (see §6) — manual confirmation is **not** required. If only ≥ 5 min, the runbook requires manual escalation-tree authorization (see §6.2).

This is **not** total destruction. If the primary becomes unreachable for ≥ 4 hours with no ETA, escalate to cold-restore (DR-15) per `RB-COLD-RESTORE-FROM-ZERO.md` §1.

## 2. Prerequisites (validated by `--simulate` mode before every staging drill)

### 2.1 Standing prerequisites

- [ ] `crates/corelink-failover-router` deployed at current `main` SHA; `is_acyclic()` integration test green in last 24h.
- [ ] Cross-region replication lag p99 ≤ 60s sustained over last 24h (`SLO-REPLICATION-LAG-P99` per `corelink-replica-worker::REPLICATION_LAG_P99_SLO_SECS`).
- [ ] Sibling region for the target primary is Healthy (`RegionHealth::Healthy`) at T-2h.
- [ ] Residency-graph invariants reviewed by Privacy Lead in last 30 days (no EU→US illegal flows; no SAM→non-WEUR illegal flows).
- [ ] DNS TTL on the failover-controlled hostnames ≤ 60s (required for ≤ 15 min RTO ceiling).
- [ ] CF Worker routing rule for failover hostnames staged + dry-runnable (`wrangler routes list` returns the canonical map).
- [ ] PagerDuty drill rotation overrides primary rotation for the drill 30-min window.
- [ ] Audit-chain checkpoint within last 5 min in surviving region D1 (`audit_chain_checkpoints` table).

### 2.2 Per-drill prerequisites (T-2h gate)

- [ ] Customer-comms (staging-only banner): "Staging-only active-failover drill on YYYY-MM-DD HH:MM UTC; no production impact expected" — posted T-2h.
- [ ] Baseline-metrics snapshot captured (Grafana export of: `gw_request_latency_p95`, `gw_request_error_rate_5xx`, `corelink_region_health_status`, `replication_lag_seconds_p99`, `corelink_active_region_label`).
- [ ] IC + Scribe + L2 Eng-Mgr + Privacy Lead on PD acked.
- [ ] War-room channel pre-created: `#drill-active-failover-YYYY-MM-DD`.

## 3. RTO / RPO targets

These are the **hard ceilings** the drill validates. Any single miss drives a YELLOW or RED outcome and an action item.

| Metric | Target | Source-of-truth |
|---|---|---|
| **Detection-to-IC-establishment** | ≤ 5 min | PD ack stamp; runbook step 1 (Detect) |
| **Decision-to-Drain** | ≤ 5 min | runbook step 2→3 timestamps |
| **RTO (write-lease flip)** | ≤ 15 min from drill start ("incident declared") to first successful write against the *new* active region | runbook steps 1–6; verified by smoke test |
| **RPO (data loss window)** | ≤ 5 min — measured as `now() − last_replication_checkpoint_ts` at moment of Drain step | `audit_chain_checkpoints.checkpoint_ts` + `replication_lag_seconds_p99` |
| **Failover overhead** | ≤ 50 ms p99 routing overhead | `SLO_FAILOVER_OVERHEAD_MS` from `corelink-failover-router::health` |
| **Audit-chain continuity** | Pre-drill Merkle root chains continuously through Drain + Promote + Reroute (no gap, no re-fork) | post-drill Merkle-root walker |
| **Failback (Reverse) RTO** | ≤ 30 min from "primary recovered" declaration to active-lease back on primary + replication caught up | runbook step 7 (Reverse) |
| **No split-brain** | Zero writes accepted by *both* regions in the same epoch window | `corelink-failover-router::WriteMode` audit log |

> **Why 15 min?** Cold-restore targets 4h read / 8h write because it rebuilds infra. Active failover only flips a lease — DNS TTL ≤ 60s + Worker routing flip ≤ 5s + write-lease handover ≤ 60s + smoke verification ≤ 5 min comfortably fits 15 min. Tighter than DR-005 (1800s = 30 min) because DR-005 is read-only sibling-route; DR-16 includes write-lease handover.
>
> **Why 5 min RPO?** Cross-region replication-lag SLO is 60s p99; we set 5 min as the *drill ceiling* (5× slack) to absorb the Drain step's queue-flush window where new writes are queued in the DO rather than committed.

## 4. Drill mode taxonomy

The drill orchestrator (`scripts/active-failover-drill.sh`) supports three modes; only `--simulate` and `--staging` are scheduled.

| Mode | What it does | Required env | Safe to run? |
|---|---|---|---|
| `--simulate` | Logical-only — drives the in-memory `InMemoryFailoverRouter` through Detect→Decide→Drain→Promote→Reroute→Verify→Reverse via `corelink-failover-router` test fixtures; touches no real infrastructure; emits structured log. | none | Always safe; runnable in CI. |
| `--staging` | Runs the full 7-step runbook against the staging-equivalent CF account: real WAF block to simulate degradation, real DNS flip (staging hostnames only), real Worker routing flip, real smoke tests, real failback. | `CORELINK_ENV=staging`, `STAGING_PRIMARY_REGION`, `STAGING_SIBLING_REGION` | Safe; staging hostnames only. |
| `--prod` | Real-incident path — flips active write lease in **production**. ONLY for real warm-failover incidents. NOT scheduled. | `CORELINK_ENV=production`, `CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD`, SRE-Lead PD ack within 5 min. | NOT for drill; real-incident only. |

Cadence: `--simulate` weekly in CI (cron `0 14 * * 3`); `--staging` monthly (first Wed of month, 14:00 UTC). `--prod` is never scheduled.

## 5. Pre-drill checklist (T-2h gate)

The IC must walk this checklist 2h before scheduled drill. **Any unchecked item delays the drill** per `BCP-DR-DRILL-CADENCE.md` §9.

### 5.1 Customer-comms (T-2h)

- [ ] Status-page banner posted: "Staging-only active-failover drill — no customer impact expected" — schedules for the drill 30-min window.
- [ ] Lighthouse-customer account managers notified by email; opt-in observer permitted (logged `drill_observer`).
- [ ] No production SLAs engaged — drill targets staging hostnames only.

### 5.2 Operational freezes (T-2h)

- [ ] Deploy freeze: no merges to `main` between T-30min and T+drill_end + 30min.
- [ ] PagerDuty drill rotation overrides primary rotation for drill 30-min + 30min post-drill window.

### 5.3 Baseline-metrics snapshot (T-2h)

- [ ] Grafana export: `dashboards-failover`, `dashboards-replication-lag`, `dashboards-region-health`, `dashboards-slo-burn`.
- [ ] D1 audit-chain checkpoint hash captured: `SELECT merkle_root FROM audit_chain_checkpoints ORDER BY checkpoint_ts DESC LIMIT 1` — stored in pre-drill evidence doc.
- [ ] `corelink_active_region_label` Prometheus gauge value captured pre-drill (assertion: matches expected primary).
- [ ] Synthetic-tenant fixture refreshed: `specs/_compliance/drill-evidence/fixtures/active-failover-tenant-pre-drill.json` updated within 24h — contains tenant_id_hash + last 3 audit event IDs + last write timestamp.

### 5.4 War-room readiness (T-30min)

- [ ] `#drill-active-failover-YYYY-MM-DD` Slack channel created + bookmarked (links to spec + runbook + dashboards).
- [ ] Runbook `RB-ACTIVE-FAILOVER.md` printed/PDF'd as offline fallback.
- [ ] Conference bridge URL posted.
- [ ] All participants confirmed in PD; no SEV1/SEV2 active.

## 6. Drill execution (overview — full procedure in `RB-ACTIVE-FAILOVER.md`)

The drill executes the 7-step runbook timeline. Each step has a target time budget. The orchestrator script emits structured `LOG_STEP=N_TITLE` markers.

| Step | Title | Budget | Cumulative |
|---|---|---|---|
| 1 | Detect (alerts fire; multi-signal Degraded) | 2 min | T+0:02 |
| 2 | Decide (escalation tree; authorize flip) | 3 min | T+0:05 |
| 3 | Drain (stop new writes to primary; queue in DO) | 2 min | T+0:07 |
| 4 | Promote (secondary takes write lease) | 3 min | T+0:10 |
| 5 | Reroute (DNS / CF Worker routing switch) | 2 min | T+0:12 |
| 6 | Verify (synthetic page + 3-tenant smoke) | 3 min | T+0:15 |
| 7 | Reverse (failback once primary recovers — drill or scheduled) | 30 min | T+0:45 |

**RTO ceiling: T+0:15 (15 min for the write-lease flip, steps 1–6).** Step 7 (Reverse) runs after primary recovers and is scoped to its own 30-min budget that does NOT count against the 15-min RTO.

### 6.1 Trigger lanes

| Lane | Trigger | Authorization |
|---|---|---|
| **Automatic** | SLO breach sustained ≥ **10 min** + sibling Healthy | Auto-flip; PD page + audit log; no manual ack required to begin Drain. |
| **Manual** | SLO breach sustained ≥ **5 min** AND < 10 min | Manual confirmation by SRE primary on-call + L2 Eng Mgr; PD page with `requires_ack=true`. |
| **Override** | SLO not breached but IC declares (e.g. pending CF maintenance) | L3 (CTO) override required; audit-emit BEFORE state flip with `override_reason` field. |

### 6.2 Escalation tree (Decide step)

| Role | Tier | Can authorize? |
|---|---|---|
| SRE primary on-call | L1 | Co-authorize (with L2) under Manual lane |
| Eng Mgr | L2 | Co-authorize (with L1) under Manual lane; sole authorizer under Override if L3 unavailable |
| CTO | L3 | Sole authorizer under Override; required for any --prod drill mode |

Automatic lane requires **no human ack to begin Drain** but emits a `failover.detected` audit event with `auto_trigger=true` and pages the IC + Eng Mgr + Privacy Lead simultaneously.

## 7. Success criteria (auditor-readable)

The drill is **PASS** iff all 8 criteria below evaluate to true:

1. **RTO write-flip met:** first successful write against the new active region completes at T ≤ 0:15 wall-clock.
2. **RPO met:** `last_replication_checkpoint_ts` at moment of Drain step ≤ 5 min stale.
3. **Failover overhead met:** routing-overhead p99 ≤ 50 ms over the drill window.
4. **Audit-chain continuity:** Merkle root walks pre-drill → Drain → Promote → Reroute → Verify with no gap; no re-fork.
5. **No split-brain:** zero writes accepted by *both* the demoted primary AND the promoted sibling in any overlapping epoch (verified via DO write-lease audit log).
6. **Residency invariants:** zero residency-tag violations (no EU-tagged data routed to US sibling, no BR-tagged data routed to WEUR sibling) — verified by `corelink-replica-worker::INV-REGION-NO-CROSS-LEAK` checker.
7. **Failback RTO met:** Reverse step completes at T ≤ 0:45 (15-min flip + 30-min Reverse).
8. **No real customer impact:** zero production-tenant 5xx in drill window; zero production-audit-chain rows missing.

## 8. Failure thresholds + escalation

| Drill outcome | Criteria | Action |
|---|---|---|
| **PASS** | All 8 criteria green | Seal evidence; refresh DR-16 cadence row; file in Drata `incident_response`. |
| **PARTIAL (YELLOW)** | 1–2 criteria amber (within 25% of target) | File AI within 7d; reschedule within 30d. |
| **FAIL (RED)** | ≥ 3 criteria amber OR any 1 criterion red OR RTO > 30 min (2× target) OR ANY split-brain detected | SEV-2 declared (drill, not real); root-cause within 14d; DR-16 fail blocks Type II audit pass. |
| **ABORT** | Real SEV1/SEV2 hits during drill window | Halt drill immediately; resume real incident response; reschedule within 14d. |

Split-brain detection is a **hard red** — even if every other criterion is green, any cross-write in overlap epoch fails the drill outright because the safety invariant is non-negotiable.

## 9. Post-drill evidence (sealed within 24h)

Evidence doc lives at `specs/_compliance/drill-evidence/YYYY-MM-active-failover-{simulate|staging}.md` plus three active-failover-specific sections:

- **§A** RTO/RPO measurements vs targets (table from §3 above).
- **§B** Per-step timing vs budget (table from §6 above with actual durations).
- **§C** Split-brain audit (DO write-lease log dump showing exactly one region accepted writes per epoch).

The evidence doc MUST be uploaded to Drata via `corelink-drata-sync` CLI as an `incident_response` payload.

## 10. Cadence + ownership

| Attribute | Value |
|---|---|
| **Cadence (pre-GA)** | One mandatory `--simulate` in CI; one mandatory `--staging` before GA tag. |
| **Cadence (post-GA)** | Monthly `--staging` (first Wednesday 14:00 UTC); `--simulate` weekly in CI. |
| **Owner** | SRE Lead. |
| **IC role** | Senior SRE on rotation; backup IC = Eng Manager. |
| **Reviewers** | Compliance Officer (drill_status), Security Lead (audit-chain + split-brain checks), Privacy Lead (residency-tag preservation). |
| **Sign-off (post-drill)** | IC + Scribe + SRE Lead + Compliance Officer; doc_status frozen within 24h. |

## 11. References

- `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` — companion total-loss drill (DR-15)
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — overall DR cadence (DR-16 row added)
- `specs/_runbooks/RB-ACTIVE-FAILOVER.md` — 7-step operational runbook
- `scripts/active-failover-drill.sh` — 3-mode orchestrator
- `tests/e2e-failover-router/` — 5-scenario E2E harness
- `crates/corelink-failover-router/` — multi-signal detection + routing primitives
- `crates/corelink-region/` — region taxonomy + provisioning events
- `crates/corelink-replica-worker/` — ResidencyGraph + replication lag SLO
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` — A1.2 evidence cross-link
- `ROADMAP-TO-GA.md` §6 Wave R-6
