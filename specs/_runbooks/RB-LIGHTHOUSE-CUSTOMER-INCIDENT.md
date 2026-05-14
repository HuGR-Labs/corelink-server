---
id: "RB-LIGHTHOUSE-CUSTOMER-INCIDENT"
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
parent: "WI-S20-004"
tags: ["runbook", "p0", "lighthouse", "incident", "sla", "s20", "30d-observation"]
---

# RB-LIGHTHOUSE-CUSTOMER-INCIDENT — Lighthouse Customer Incident Response (P0)

> **Severity floor:** **P0** — lighthouse customers are GA Evidence Gate D+60 critical (WI-S20-004 §6.1.7).
> **Detect:** ≤ 5 min · **Acknowledge:** ≤ 10 min · **Engage Engineering Lead:** ≤ 15 min · **Customer-notify:** ≤ 1 h
> **State-machine impact:** an unresolved P0 may force `Observing → Withdrawn` and trigger a backfill cycle (`Engaged → Migrating → Observing`).

> Lighthouse customers (`LH-FORGE`, `LH-OSS-01`, `LH-ENT-BYOK-01`) are **named in audit metadata** during the 30-day observation window. Any incident touching them is P0 regardless of customer-visible scope.

---

## 1. Trigger conditions

This runbook fires for **any** of the following on a lighthouse customer:

| # | Trigger | Source |
|---|---|---|
| 1 | `corelink_lighthouse_customer_slo_violation_total{customer_id=LH-*}` > 0 | Prometheus alert |
| 2 | `corelink_lighthouse_illegal_transition_total{customer_id=LH-*}` > 0 | Prometheus alert |
| 3 | Customer-reported incident inbound to support tag `lighthouse` | Support inbox |
| 4 | BYOK kill-switch fired for `LH-ENT-BYOK-*` | `corelink_byok_kill_switch_fired_total` |
| 5 | DSR triggered for any `LH-*` customer | `corelink_dsr_request_received_total` |
| 6 | Any SEV-1 / SEV-2 incident page where impact analysis lists ≥ 1 lighthouse customer tenant_id | PagerDuty + IR triage |
| 7 | `lighthouse_sla_samples` daily insert shows ANY SLO not met | CF Cron daily emitter |

---

## 2. Detection

- **Alert** `corelink_lighthouse_customer_slo_violation_total{customer_id, slo_id, plan}` > 0 (CRITICAL; P0; auto-page).
- **Alert** `corelink_lighthouse_illegal_transition_total{customer_id, from_state, to_state}` > 0 (CRITICAL; P0; auto-page — indicates operator bug or audit-chain corruption).
- **Alert** `corelink_lighthouse_customer_observation_status_gauge` flips from `2 (sla_met)` to `0 (blocked)` (CRITICAL).
- **Cron** weekly sweep cross-references `lighthouse_sla_samples` against the 30d window; any sustained-miss surfaces here.

---

## 3. Comunicação + escalation

Lighthouse customers are P0 priority. Escalation is **not** scoped by customer-visible impact alone — even a 5-minute degradation must page.

| Severity assessment | Page targets | Customer outreach |
|---|---|---|
| **P0 — any lighthouse SLO violation** | SRE on-call + **Engineering Lead** + Customer Success engineer assigned to slot | Phone call to customer technical lead within 1h |
| **P0 + DPA / privacy sub-mode** (e.g. DSR error, BYOK kill-switch error, cross-region leak) | + Privacy Officer + Legal Counsel + DPO | Phone call within 30 min; written breach notification ≤ 24h |
| **P0 + state machine illegal transition** | + Auditor + Engineer S-20 lead | None until root cause identified (could be benign) |

- **Internal Slack:** `#sev-lighthouse-{slot}` channel opens automatically when alert fires.
- **Status page:** **DO NOT** post lighthouse-customer-specific status updates publicly; lighthouse status is internal-only. Use customer-specific email/phone instead.
- **Sales / Founder loop:** any P0 sustained > 2h escalates to Founder (lighthouse customers are revenue + reference-call critical).

---

## 4. Immediate mitigation

1. **Confirm scope** — query `lighthouse_sla_samples` for the affected slot:
   ```sql
   SELECT customer_id, sampled_at,
          avail_cas_put_met, avail_cas_get_met,
          lat_cas_get_p99_met, fresh_billing_met,
          byok_key_health_ok
     FROM lighthouse_sla_samples
    WHERE customer_id = 'LH-FORGE'  -- or LH-OSS-01 / LH-ENT-BYOK-01
      AND sampled_at > strftime('%s', 'now', '-24 hours')
    ORDER BY sampled_at DESC;
   ```

2. **Identify SLO** that fired:
   - `lat_cas_get_p99_met = 0` → routing / region-failover / R2 latency (consult `RB-region.md`).
   - `avail_cas_put_met = 0` → CAS pipeline (consult `RB-CHAOS-CATALOG.md` Stripe / R2 outage scenarios).
   - `fresh_billing_met = 0` → billing reconciler drift (S-12 billing-reconcile crate).
   - `byok_key_health_ok = 0` → BYOK provider outage (consult `RB-BYOK-REVOKE.md`).

3. **Apply per-SLO mitigation** from the relevant runbook above.

4. **Customer notification** — phone call FIRST, then written summary in email:
   - "We detected an SLO breach in your CoreLink usage at HH:MM UTC. We are actively working on resolution. ETA to next update: 30 minutes."
   - Use the lighthouse-customer paging tree (named contact per `LH-*` slot stored in CRM, NOT in this repo).

5. **State-machine recording** — emit an SLA sample row with the offending SLO = false. Library will set `sla_breach_recorded = true` on the customer record. This **blocks** the `Observing → Attested` transition until operator-driven remediation clears it.

---

## 5. Diagnóstico

1. Query Prometheus for the SLO breach burn rate over the previous 24h.
2. Cross-reference S-09 OTel tracing for the customer's `tenant_id`.
3. Check region health page + R2 health + KMS provider health (Enterprise BYOK).
4. Inspect audit chain for `lighthouse_lifecycle_transition` events on the affected slot — confirm state machine integrity.
5. If `illegal_transition_total` fired: **STOP** operator actions, page Auditor + Engineer S-20 lead; do NOT attempt to manually reconcile the state machine.

---

## 6. Resolução

| Trigger | Hot fix | Cold fix |
|---|---|---|
| SLO breach (latency / availability) | Per-SLO runbook (RB-region, RB-CHAOS-CATALOG, etc.) | Post-mortem mandatory; review whether SLO targets are realistic at GA |
| Illegal state transition | DO NOT manually edit D1; investigate root cause; possibly replay from audit chain | Strengthen state-machine property tests; bump `PROPTEST_CASES` |
| BYOK provider outage | Switch to secondary BYOK provider (S-14 matrix); maintain encrypted-at-rest property | Customer relationship review; possibly upgrade their BYOK matrix |
| DSR exceeded SLO | Manual erasure runner (S-12 privacy-erasure-worker); Ed25519 attestation signed manually with audit | Capacity planning for DSR worker; review erasure batch sizing |

---

## 7. Decision tree (operator)

```
[lighthouse customer alert fires]
   │
   ├─ Is this an illegal_transition_total? ──── YES ──► STOP. Page Auditor + Engineer S-20 lead.
   │      │                                            DO NOT touch D1 lighthouse_customers row.
   │      NO
   │      ▼
   ├─ Is this an SLA sample with SLO miss? ──── YES ──► Per-SLO runbook + customer phone call
   │      │                                            + flag sla_breach_recorded on customer.
   │      NO
   │      ▼
   ├─ Is this a customer-inbound report?    ──── YES ──► Triage severity; if confirms SLO violation,
   │      │                                            backfill sample row + flag breach.
   │      NO
   │      ▼
   ├─ Is this DPA / Privacy related?        ──── YES ──► Page Privacy + Legal + DPO; ≤ 24h breach
   │                                                   notification window opens.
   │
   └─► Customer-notify ≤ 1h + post-mortem mandatory + Founder informed if sustained > 2h.
```

---

## 8. State-machine impact

After resolution, the operator decides between two paths:

1. **Continue observation** — if breach was a single isolated SLO sample and customer agrees to keep observing, the operator may keep the customer in `Observing` and reset `sla_breach_recorded = false` via a deliberate remediation cycle (re-engage from `Engaged → Migrating → Observing` with a fresh 30d window). This MUST be recorded in audit chain.

2. **Withdraw + backfill** — if the customer no longer wants to be a lighthouse, transition `Observing → Withdrawn` (legal per state machine; see `corelink-lighthouse-tracker::LifecycleState::allowed_next`). Then engage a backup candidate from the recruitment shortlist and start their cycle.

Either path requires Engineering Lead approval (lighthouse decisions are not on-call-engineer-alone calls).

---

## 9. Post-incident

- **Post-mortem mandatory** for any P0 affecting a lighthouse customer (no severity-based exemption; the lighthouse-customer status itself triggers it).
- **Customer outreach** within 24h with written incident report; offer extended trial / additional free months if commercial impact.
- **Privacy Officer + DPO review** if any DPA-related sub-mode (BYOK kill-switch error, DSR exceeded SLO, cross-region leak).
- **WI-S20-004 ledger update** — mark the slot with breach + remediation evidence in `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-incident-{N}.md`.
- **GA Evidence Gate D+60 impact assessment** — does this breach jeopardize the 3/3 SLA-met-sustained-30d criterion? If yes, escalate to Founder + apply spec contract §19 waiver path (3 → 2 + plan to add 1 within 60d).

---

## 10. Cross-references

- `specs/_lighthouse/lighthouse-customer-program.md` — program framework.
- `specs/_lighthouse/sla-attestation-template.md` — attestation form gate.
- `crates/corelink-lighthouse-tracker/src/lib.rs` — state machine + `SlaSample` shape.
- `migrations/d1/0042_lighthouse_customers.sql` — D1 persistence.
- `specs/_runbooks/RB-CHAOS-CATALOG.md` — chaos scenarios that map to lighthouse SLOs.
- `specs/_runbooks/RB-ONCALL-POLICY.md` — on-call rotation policy.
- `specs/05_runbooks/RB-BYOK-REVOKE.md` — BYOK revocation (Enterprise BYOK only).
- `specs/05_runbooks/RB-region.md` — region failover.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — post-mortem process.

---

**Fim RB-LIGHTHOUSE-CUSTOMER-INCIDENT.**
