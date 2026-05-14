---
id: "WI-S20-006"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-05-14"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009", "FF-HR-010"]
parent: "S-20"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s20", "ga", "incident-response", "24-7", "pagerduty", "synthetic-page", "high-risk", "sealed"]
sealed_artifacts:
  rust_crate: "crates/corelink-synthetic-pager"
  d1_migration: "migrations/d1/0043_synthetic_page_drills.sql"
  runbook_extended: "specs/_runbooks/RB-ONCALL-POLICY.md (v1.1.0 — §11 added)"
  runbook_new: "specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md (v1.0.0)"
  cron_trigger: "wrangler.toml [triggers] crons += '0 14 * * 1'"
  dashboard: "dashboards/grafana/DASH-ONCALL-24-7.json"
  readiness_audit: "specs/_audits/2026-05-14-s20-oncall-24-7-readiness.md"
---

# WI-S20-006 — Incident Response 24/7 PagerDuty Schedule Live em 3 Regiões (US Pacific + US Eastern + EU; APAC Eventual Scaling Pós-GA Q1 Demand-Driven) + On-Call Manager Rotation (Owner + Final Approver Dual-Hat Solo-Tier per ADR-0034 Option A; OR Contract-Based Escalation Tier-1 SRE Engineering Pré-GA Hire) + Escalation Matrix `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md` (P0 → Page On-Call → Escalate Manager 5 min → Escalate CEO/Founder 15 min) + Response < 5 min Testadas via Synthetic Page Weekly (Cron Job Creates Synthetic SEV-2 Incident; On-Call Must Ack Within 5 min p99) + Synthetic Page Weekly < 5 min Response Sustained 30d (GA Evidence Gate D+60 Criterion) + DASH-INCIDENT-RESPONSE Embedded em DASH-GA-READINESS + EVT-026 Evidence

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-20](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S20-006 |
| Título | Incident response 24/7 PagerDuty 3 regions + on-call manager + escalation matrix + synthetic page weekly < 5 min response sustained 30d |
| Sprint | S-20 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controle final pré-GA — operational baseline) + FF-HR-009 (contratos com customers vão production = SLA breach response time) + FF-HR-010 (primeira entrega regulatory live em escala) |

## 1. Objetivo + JTBD

**Objetivo**: Operacionalizar **incident response 24/7** com PagerDuty schedule live em 3 regiões (US/EU; APAC eventual scaling pós-GA Q1 demand-driven) + on-call manager rotation + escalation matrix + synthetic page weekly < 5 min response sustained 30d (GA Evidence Gate D+60 criterion).

**JTBD**: "Como SRE Lead / On-Call Engineer / CISO em prospect enterprise, preciso evidência verificável que: (a) **incident response 24/7 ready** com PagerDuty schedule live em 3 regiões (US Pacific + US Eastern + EU; cobertura 24/7 sem gaps; APAC eventual scaling pós-GA Q1 demand-driven); (b) **on-call manager rotation** (Owner + Final Approver dual-hat solo-tier per ADR-0034 Option A pré-staffing; OR contract-based escalation Tier-1 SRE engineering pré-GA hire); (c) **escalation matrix** documented em `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md` (P0 → page on-call → escalate manager 5 min → escalate CEO/Founder 15 min); (d) **response < 5 min testadas via synthetic page weekly** (cron job creates synthetic SEV-2 incident; on-call must ack within 5 min p99); (e) **synthetic page weekly < 5 min response sustained 30d** (GA Evidence Gate D+60 criterion per spec contract §10.s20.8); (f) **DASH-INCIDENT-RESPONSE** embedded em DASH-GA-READINESS dashboard."

GA-go binary engineering gate.

## 2. Scope

### 2.1 In-scope

1. **PagerDuty schedule live em 3 regiões**:
   - **US Pacific** (covering 12am-12pm UTC; on-call Owner + Final Approver dual-hat OR contracted SRE).
   - **US Eastern** (covering 9am-9pm UTC; on-call Owner + Final Approver dual-hat OR contracted SRE).
   - **EU** (covering 6am-6pm UTC; on-call contracted SRE).
   - **APAC eventual scaling pós-GA Q1 demand-driven** (não at GA; 4 regions WNAM/ENAM/WEUR/SAM stable from S-14 sufficient at GA).
   - 24/7 coverage sem gaps via overlap entre regions.

2. **On-call manager rotation**:
   - **Option A solo-tier per ADR-0034**: Owner + Final Approver dual-hat (Gustavo Schneiter solo founder); explicit ADR documenting accepted residual risk + post-staffing review cadence.
   - **Option B contract-based**: Tier-1 SRE engineering pré-GA hire (engagement Q3-Q4 antes do sprint start; 6-12-week lead time; budget $80-200k full S-20 PRR including external advisor pool per ADR-0034 Option C).
   - **Recommended path** (current state): Option A com ADR-0034 + Option C parallel staffing track (external SRE advisor pool engaged Q3-Q4).

3. **Escalation matrix** `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md`:
   ```
   P0 (SEV-1):
     T+0: PagerDuty pages on-call (region-rotated).
     T+5 min: If unacknowledged, escalate to manager (Owner + Final Approver dual-hat).
     T+15 min: If still unacknowledged, escalate to CEO/Founder (Gustavo Schneiter; same as Owner solo-tier).
     T+30 min: External advisor pool engagement OR media response prep (PR/Legal Counsel).
   P1 (SEV-2):
     T+0: PagerDuty pages on-call.
     T+15 min: If unacknowledged, escalate to manager.
   P2 (SEV-3):
     T+0: PagerDuty notifies on-call (non-urgent).
     T+1h: If unacknowledged, escalate to manager.
   P3 (SEV-4):
     T+0: PagerDuty notifies on-call (non-urgent).
   ```

4. **Synthetic page weekly < 5 min response**:
   - Cron job creates synthetic SEV-2 incident weekly (e.g., every Wednesday 10am UTC).
   - On-call must ack within 5 min p99.
   - Cron job verifies ack timestamp; alerts if miss.
   - Sustained 30d (GA Evidence Gate D+60 criterion per spec contract §10.s20.8).

5. **DASH-INCIDENT-RESPONSE** embedded em DASH-GA-READINESS dashboard:
   - Synthetic page response timeline (≤ 5 min p99 sustained).
   - Per-region on-call coverage gauge (3 regions × 24h).
   - Escalation events counter (P0 + P1 + P2 + P3).
   - On-call fatigue gauge (paged_count_per_engineer per week).
   - Acked timeline per synthetic page.

6. **EVT-026 evidence captured** em PRR-GA-001 evidence pack.

### 2.2 Anti-scope

- ❌ APAC region 24/7 oncall (pós-GA Q1 demand-driven; 3 regions US/EU sufficient at GA).
- ❌ Multi-DPO escalation workflow (pós-GA enterprise).
- ❌ Customer-facing incident response dashboard UI (pós-GA enterprise).
- ❌ Real (non-synthetic) incident drill durante S-20 (chaos automation S-17 cumulative covers; S-20 entrega synthetic page weekly only).
- ❌ Bug bounty program integration (pós-GA Q1).
- ❌ External (third-party) incident response audit (pós-GA SOC 2 Type I 6m).

## 3. Capability mapping

- **CAP-GA-006** (Incident response 24/7 ready): IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1 R-S20-6 + §6.1 + §10.s20.8 + §15 risk row 8 (PagerDuty 24/7 schedule infeasible com team size)` + `resilience_patterns.md (PAT-AUTO-ROLLBACK-001 + PAT-PROGRESSIVE-ROLLOUT-001 cumulative)` + `failure_modes.md (cumulative FMs)` + `00_framework.md §33.5.4.3 + ADR-0034 (solo-tier waiver Option A)`.

## 4. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S20-006-D1 | PagerDuty schedule live 3 regions | PagerDuty platform live | 24/7 coverage US Pacific + US Eastern + EU; sem gaps |
| S20-006-D2 | On-call manager rotation | per ADR-0034 Option A solo-tier OR Option C external advisor pool | Owner + Final Approver dual-hat OR contracted SRE; post-staffing review cadence |
| S20-006-D3 | Escalation matrix | `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md` | P0 → 5 min → manager → 15 min → CEO; P1..P3 cadence; committed runbook |
| S20-006-D4 | Synthetic page cron + weekly verification | `crates/corelink-synthetic-page/` + cron job | weekly synthetic SEV-2 incident; on-call must ack < 5 min p99 |
| S20-006-D5 | Synthetic page weekly < 5 min response sustained 30d | DASH-INCIDENT-RESPONSE | sustained 30d (GA Evidence Gate D+60); EVT-026 |
| S20-006-D6 | EVT-026 evidence | PRR-GA-001 Annex A | synthetic page response timeline + per-region coverage + escalation events linked |

## 5. Detailed design

### 5.1 PagerDuty schedule structure

```yaml
schedule:
  name: "CoreLink GA on-call 24/7"
  timezone: "UTC"
  layers:
    - name: "US Pacific"
      coverage: "00:00-12:00 UTC daily"
      rotation: "weekly"
      members:
        - "Gustavo Schneiter (solo-tier Option A)"
        - "TBD Tier-1 SRE pré-GA hire (Option B)"
    - name: "US Eastern"
      coverage: "09:00-21:00 UTC daily"
      rotation: "weekly"
      members:
        - "Gustavo Schneiter (solo-tier Option A)"
        - "TBD Tier-1 SRE pré-GA hire (Option B)"
    - name: "EU"
      coverage: "06:00-18:00 UTC daily"
      rotation: "weekly"
      members:
        - "TBD external SRE advisor pool (Option C)"
escalation_policy:
  - level_1: "on-call (region-rotated)"
  - level_2: "manager (5 min)"
  - level_3: "CEO/Founder (15 min)"
```

### 5.2 Synthetic page cron

```rust
// crates/corelink-synthetic-page/src/cron.rs
pub async fn run_synthetic_page_weekly() {
    let incident = SyntheticIncident {
        severity: Severity::Sev2,
        title: "Synthetic page weekly health check",
        description: "Verify on-call response < 5 min p99 sustained 30d",
        scheduled_at: Utc::now(),
    };
    pagerduty_client.create_incident(incident).await?;
    
    // Watcher: verify ack within 5 min
    let timeout = Duration::from_secs(5 * 60);
    match tokio::time::timeout(timeout, watch_ack(incident.id)).await {
        Ok(Ok(ack_at)) => {
            let response_ms = (ack_at - incident.scheduled_at).num_milliseconds();
            metric_synthetic_page_response_seconds(response_ms / 1000);
            if response_ms > 5 * 60 * 1000 {
                alert_synthetic_page_missed(incident.id);
            }
        }
        _ => {
            alert_synthetic_page_missed(incident.id);
        }
    }
}
```

### 5.3 RB-INCIDENT-ESCALATION-MATRIX.md structure

```markdown
# Runbook — Incident Escalation Matrix (RB-INCIDENT-ESCALATION-MATRIX)

## Severity definitions
- **SEV-1 (P0)**: Production outage; customer SLA breach imminent; critical data loss risk.
- **SEV-2 (P1)**: Degraded service; non-critical SLA miss possible; user-visible.
- **SEV-3 (P2)**: Minor issue; internal workaround available; non-urgent.
- **SEV-4 (P3)**: Cosmetic / documentation; low priority.

## Escalation cadence per severity
[see §5.1 Detailed design]

## On-call responsibilities
- Acknowledge page within target SLA.
- Triage + initial diagnosis ≤ 15 min.
- Open SEV channel #incident-{id}.
- Update status page (status.corelink.dev).
- Post-mortem within 72h SEV-1 / 7d SEV-2 (per S-17 framework).

## Tools
- PagerDuty (paging + escalation).
- Slack #incident-{id} (war room).
- Status page: status.corelink.dev.
- Runbook library: specs/05_runbooks/RB-FM-*.md.
```

## 6. Acceptance criteria

### 6.1 Positive paths

1. **PagerDuty schedule live 3 regions** com 24/7 coverage sem gaps.
2. **On-call manager rotation** documented (ADR-0034 Option A OR Option B/C).
3. **Escalation matrix runbook committed** em `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md`.
4. **Synthetic page weekly cron live**; weekly synthetic SEV-2 incident; on-call ack < 5 min p99.
5. **Synthetic page weekly < 5 min response sustained 30d** (GA Evidence Gate D+60).
6. **DASH-INCIDENT-RESPONSE** embedded em DASH-GA-READINESS.
7. **EVT-026 evidence captured** em PRR-GA-001 Annex A.

### 6.2 Negative paths (≥ 4 mandatory)

1. **PagerDuty 24/7 schedule infeasible com team size** (per spec contract §15 row 8) → 3 regions com on-call manager rotation; prioritize coverage US + EU at GA; APAC followers; Option A solo-tier ADR-0034 + Option C external advisor pool fallback.
2. **PagerDuty response > 5 min sustained** → post-mortem + oncall reinforce; iterate process; potential delay GA até sustained.
3. **On-call fatigue gauge spike** (paged_count_per_engineer > threshold) → escalation review + Option B/C staffing + rotation rebalance.
4. **Synthetic page miss** (on-call não ack within 5 min) → alert escalation + post-mortem + RB review.
5. **APAC region missing causes customer SLA breach** → APAC scaling fast-track pós-GA Q1 demand-driven.
6. **Tier-1 SRE engineering pré-GA hire não staffed** (Option B fail) → fallback Option A solo-tier ADR-0034 + Option C external advisor pool.

## 7. Test plan

### 7.1 PagerDuty schedule validation

- 3 regions live com 24/7 coverage verified.
- Rotation weekly; members assigned.
- Escalation policy P0 → 5 min → 15 min cadence verified.

### 7.2 Synthetic page weekly validation

- Cron job runs weekly (cron schedule verified).
- Synthetic SEV-2 incident created automatically.
- Watcher verifies ack within 5 min.
- Alert fires if miss.

### 7.3 30d sustained validation

- 4+ weekly synthetic page cycles (D+0..D+30 + D+30..D+60).
- Each cycle: ack < 5 min p99.
- DASH-INCIDENT-RESPONSE timeline shows sustained.

## 8. Failure modes

- **FM-PAGERDUTY-247-INFEASIBLE-TEAM-SIZE** (per spec contract §15 row 8): mitigation = 3 regions oncall manager rotation + Option A solo-tier ADR-0034 + Option C external advisor pool.
- **FM-PAGERDUTY-RESPONSE-MISS-5MIN** (per spec contract §18 post-mortem hook): mitigation = post-mortem + oncall reinforce + iterate process.
- **FM-ONCALL-FATIGUE-SPIKE** (novo S-20): mitigation = rotation rebalance + Option B/C staffing.
- **FM-SYNTHETIC-PAGE-MISS** (novo S-20): mitigation = alert escalation + post-mortem + RB review.

Cumulative coverage: FMs S-13..S-19 ratificadas em incident response posture.

## 9. Invariants (cumulative ratification)

ALL 27+ INVs from S-13..S-19 + canonical sources active during incident response readiness.

S-20 NÃO introduz novos INVs.

## 10. Controls (cumulative)

CTRL-XXX cumulative validated em incident response:
- **CTRL-AUDIT-* + CTRL-OBS-*** (audit chain + observability cumulative; incident response emits audit + métrica).
- SOC 2 (CC6.7 change management — incident response process).
- ISO/IEC 27001:2022 A.16 (Information security incident management).

## 11. Resilience patterns (cumulative)

PAT-XXX cumulative referenced em incident response:
- PAT-AUTO-ROLLBACK-001 (error budget burn → automatic rollback).
- PAT-PROGRESSIVE-ROLLOUT-001 (progressive rollout 4-stage).
- PAT-DUAL-APPROVAL-001 (admin operations during incident).

## 12. Observability (cumulative)

Métricas Prometheus snake_case underscored (label `plan` aplicável; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado):

- `corelink_synthetic_page_response_seconds_bucket{region, plan}` (histogram p99 ≤ 5 min target).
- `corelink_synthetic_page_acked_total{region, outcome, plan}` (counter; outcome ∈ acked|missed|escalated).
- `corelink_oncall_paged_count_per_week_gauge{engineer, region, plan}` (gauge; on-call fatigue tracking).
- `corelink_oncall_escalation_events_total{severity, level, plan}` (counter; level ∈ on_call|manager|ceo).
- `corelink_oncall_coverage_status_gauge{region, plan}` (gauge; 0=gap|1=covered).

DASH-INCIDENT-RESPONSE panel embedded em DASH-GA-READINESS dashboard:
- Synthetic page response timeline (≤ 5 min p99 sustained).
- Per-region on-call coverage gauge (3 regions × 24h).
- Escalation events counter (P0 + P1 + P2 + P3).
- On-call fatigue gauge (paged_count_per_engineer per week).

SLO novo S-20:
- **SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE p99 < 5 min** sustained 30d.

## 13. Security & Privacy

**STRIDE delta**: incident response posture validated; chaos automation S-17 cumulative concurrent.

**LINDDUN delta**:
- **Linkability**: engineer name em on-call schedule (operational accountability).
- **Identifiability**: engineer email para PagerDuty paging.
- **Non-repudiation**: PagerDuty event log = forensic-grade audit trail.
- **Detectability**: synthetic page weekly verifies detectability + response time.
- **Disclosure**: status page status.corelink.dev customer-facing transparency.
- **Unawareness**: customer notified per status page + breach notification SLA per DPA v1.
- **Non-compliance**: SOC 2 + ISO/IEC 27001:2022 A.16 satisfied.

## 14. Dependencies

### Hard blockers
- PagerDuty platform engaged Q3 (subscription budget ~$1k-3k/mês 3 regions).
- ADR-0034 Option A documented OR Option B/C staffing engaged.

### Soft blockers
- Cumulative S-17 chaos automation 4w concurrent observation.

### Outbound
- WI-S20-001 PRR-GA-001 evidence pack EVT-026 depende de WI-S20-006 24/7 oncall live + synthetic page sustained.
- Sprint S-20 GA Evidence Gate D+60 = GA-GO depende de WI-S20-006 synthetic page weekly < 5 min sustained 30d.

## 15. PERT estimate

| Estimate | Hours | Notes |
|---|---|---|
| **Optimistic (O)** | 10h | PagerDuty smooth + escalation matrix + synthetic page cron + smooth 30d sustained. |
| **Most Likely (M)** | 16h | PagerDuty 3 regions + escalation matrix + synthetic page cron + DASH-INCIDENT-RESPONSE + 30d sustained. |
| **Pessimistic (P)** | 26h | On-call fatigue + Option B/C staffing iteration + synthetic page miss cycles. |
| **PERT** | (10 + 4×16 + 26) / 6 = **16.7h** | Per spec contract §12. |
| **Variance (σ²)** | ((26-10)/6)² = 7.1 | Std dev ≈ 2.7h. |

## 16. Sign-off canonical (HIGH_RISK 13)

13 roles per spec contract §5.1 R-S20-1 (v1.4.0 canonical — Lote 11.21 round-1 P0-S20-001 fix). **Authoritative roster (see `_spec_contract.md §5.1` for full text):** (1) Owner, (2) Final Approver, (3) Engineer Lead, (4) QA Lead, (5) Security Lead (AppSec advisor folded), (6) Privacy Officer (DPO-interim; DPO folded), (7) Legal Counsel, (8) Compliance Officer, (9) Product Lead, (10) SRE Lead, (11) CTO, (12) Architect (Crypto SME folded per ADR-0034), (13) External Auditor (pentest firm rep). Finance is NOT in the canonical 13. Any role label below that diverges MUST be read as referring to its folded canonical slot per the mapping above.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect (Crypto SME folded) | _TBD_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead (PagerDuty schedule + escalation matrix + synthetic page weekly + on-call fatigue tracking primary) | _TBD via external advisor pool_ | _pending_ | _pending_ |
| 6 | Engineer (S-20 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead (synthetic page validation) | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer (ISO/IEC 27001:2022 A.16) | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD_ | _pending_ | _pending_ |
| 12 | Legal Counsel (breach notification SLA per DPA v1 alignment) | _TBD_ | _pending_ | _pending_ |
| 13 | Finance (PagerDuty subscription $1-3k/mês 3 regions; Tier-1 SRE pré-GA hire budget Option B) | _TBD_ | _pending_ | _pending_ |

## 17. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S20-006 (cycle 12.S20.0; incident response 24/7 PagerDuty 3 regions + on-call manager rotation + escalation matrix + synthetic page weekly < 5 min response sustained 30d GA Evidence Gate D+60 criterion; ADR-0034 Option A solo-tier waiver + Option C external advisor pool fallback). |

---

**Fim WI-S20-006.**
