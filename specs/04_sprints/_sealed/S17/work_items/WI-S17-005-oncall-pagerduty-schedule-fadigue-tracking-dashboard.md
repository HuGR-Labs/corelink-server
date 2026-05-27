---
id: "WI-S17-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-17"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
tags: ["wi", "s17", "ops", "oncall", "pagerduty", "fadigue-tracking", "rotation-health", "burnout-prevention", "standard"]
---

# WI-S17-005 — Oncall PagerDuty Schedule (Tier 1 Primary Responder + Tier 2 Escalate + Tier 3 Architect/Security if Needed; Shift Duration 7 Dias Máximo per Google SRE Best Practice; Followed by 2-week Protection Period — No Oncall in Protection per Spec Contract §5.5 R-S17-12; 24/7 Rotation com 3 Regions US/EU/APAC Future-proof — APAC GA Pós-S-20; Rotation Health Published) + Fadigue Metric (SEV-1s per Shift Counter; > 2 SEV-1/shift = Alert + Manager Review; SEV-2s per Shift Counter; > 5 SEV-2/shift = Alert; Total Page Count per Month per Oncall; > 10 Alerts in Non-rotation = Burnout Signal per Spec Contract §5.5 R-S17-13) + Rotation Health Dashboard `corelink_oncall_*` Métricas Prometheus Emitting (Snake_case Canonical per Observability_model §3.1; INV-OBS-CARDINALITY-BUDGET Respeitado — Tenant-agnostic) + MTTA < 5min + MTTR < 30min Targets para SEV-1 Instrumented (per Spec Contract §9.8) + Oncall Handoff Template + 15-min Sync per Shift Change (Knowledge Transfer Canonical per Spec Contract §15 Row 9) + Runbook URL Standardized em Alert Payload (per Spec Contract §15 Row 10; S-09 R-S09-14 Reuse)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-17](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S17-005 |
| Título | Oncall rotation PagerDuty + fadigue tracking + dashboard + handoff template + MTTA/MTTR instrumentation. |
| Sprint | S-17 |
| Lane | STANDARD |
| Forcing factors | none (oncall rotation + fadigue tracking são process discipline; não introduz tenant data path novo) |

## 1. Intent

Oncall PagerDuty schedule (Tier 1/2/3; shift ≤ 7d + 2w protection period; 24/7 rotation 3 regions future-proof) + fadigue metric (SEV-1 > 2/shift alert + manager review; SEV-2 > 5/shift alert; > 10 pages/month em non-rotation = burnout signal) + rotation health dashboard `corelink_oncall_*` métricas Prometheus + MTTA/MTTR targets SEV-1 instrumented + oncall handoff template + runbook URL standardized em alert payload.

```yaml
# File: infra/pagerduty/schedule.yaml (PagerDuty Terraform / API config)
schedules:
  - name: corelink-oncall-tier-1
    time_zone: UTC
    layers:
      - name: tier-1-primary
        users: [user-a, user-b, user-c]  # rotation
        rotation_turn_length_seconds: 604800  # 7 days max
        rotation_virtual_start: 2026-05-01T09:00:00Z
        restrictions:
          - type: weekly_restriction
            start_time_of_day: 09:00:00
            duration_seconds: 28800  # 8 hours per shift segment

protection_periods:
  rule: "after_shift_end_no_oncall_2_weeks"
  duration_seconds: 1209600  # 14 days

regions_future_proof:
  - us-east  # GA at S-17
  - eu-central  # GA at S-17
  - apac  # GA pós-S-20 (APAC GA)

escalation_policies:
  - tier_1_to_tier_2_after_seconds: 300  # 5min unack escalates
  - tier_2_to_tier_3_after_seconds: 600  # 10min unack escalates
```

## 2. Narrative

Google SRE Workbook Ch 8 (On-Call) é canonical reference. PagerDuty Incident Response Documentation reuse. CoreLink S-17 entrega oncall rotation production-grade com:
- Tier 1 primary responder (oncall executes).
- Tier 2 escalate (covers Tier 1 unack).
- Tier 3 architect/security if needed (deep escalation).
- Shift ≤ 7d (Google SRE best practice; burnout prevention).
- 2-week protection period (no oncall after shift; sustained recovery).
- 24/7 rotation 3 regions future-proof (APAC GA pós-S-20).
- Fadigue metric thresholds: SEV-1 > 2/shift alert; SEV-2 > 5/shift alert; > 10 pages/month em non-rotation = burnout signal.
- Rotation health dashboard.
- MTTA < 5min + MTTR < 30min targets SEV-1 (canonical per spec contract §9.8).
- Handoff template + 15-min sync per shift change (knowledge transfer canonical).
- Runbook URL em alert payload (S-09 R-S09-14 reuse).

**Risk justification STANDARD lane**:
- Oncall rotation = process discipline (não cripto-load-bearing).
- Reuse PagerDuty SaaS (não custom IRM build).
- Não introduz tenant data path novo.

## 3. Customer Impact & Journey

**Persona — Oncall Engineer**:
- Shift ≤ 7d + 2w protection = burnout prevention.
- Tier 2/3 escalation + handoff template = sustainability.
- Fadigue tracking SEV-1 > 2/shift alert + manager review = system protection.

**Persona — Oncall Manager**:
- Rotation health dashboard = real-time burnout signals.
- > 10 pages/month em non-rotation = anomaly investigation trigger.
- Handoff template + 15-min sync = knowledge transfer baseline.

**Persona — Compliance auditor**:
- PagerDuty schedule live + 30d sustained = SOC 2 incident response baseline.
- Fadigue dashboard sustained = burnout prevention evidence.
- MTTA/MTTR targets SEV-1 = response time accountability.

## 4. Capability Mapping

- **CAP-OPS-005** (oncall rotation + fadigue tracking) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + Google SRE Workbook Ch 8 + PagerDuty docs.

## 5. Tipo

Oncall infrastructure WI; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **PagerDuty schedule** em `infra/pagerduty/schedule.yaml`:
   - Tier 1 primary (3+ users rotation; 7d shift max).
   - Tier 2 escalate (5min unack).
   - Tier 3 architect/security (10min unack).
   - 24/7 rotation 3 regions future-proof (US/EU/APAC; APAC GA pós-S-20).
   - 2-week protection period after shift end.

2. **Fadigue metric tracking + hard cutoff actions (Lote 10.17 codex P1 canonical fix; alerts alone NÃO suficientes — codex P1 finding "alert+review, not hard cutoff/protection")**:
   - SEV-1s per shift counter (`corelink_oncall_sev1_per_shift{tier}`):
     - **Soft threshold > 2 SEV-1/shift = alert + manager review** (existing).
     - **HARD threshold > 3 SEV-1/shift = AUTOMATIC ROTATION HANDOFF** (Lote 10.17 codex P1 canonical fix): PagerDuty schedule auto-rotates to Tier-2 backup; outgoing oncall enters mandatory 48h protection (no further pages); incident commander confirms handoff per oncall_handoff.md template; manager + SRE lead notified.
   - SEV-2s per shift counter (`corelink_oncall_sev2_per_shift{tier}`):
     - Soft threshold > 5 SEV-2/shift = alert (existing).
     - **HARD threshold > 8 SEV-2/shift = automatic rotation handoff** (same flow as SEV-1).
   - Total page count per month per oncall (`corelink_oncall_pages_per_month_total{tier, in_rotation}`):
     - Soft threshold > 10 em non-rotation = burnout signal (existing).
     - **HARD threshold > 15 em non-rotation = mandatory 1-month rotation block** (oncall removed from rotation 1 month + manager + manager's manager notified + HR conversation per company policy).
   - **Shift duration hard cap canonical**: 7 dias max per shift (PagerDuty schedule enforces; cannot extend without HR approval); 2-week protection post-shift (cannot be paged em protection window per PagerDuty exception list).
   - Implementation: PagerDuty webhook fires automatic handoff via API on threshold breach; audit emit `corelink.oncall.fadigue_threshold_breached`; PR review identified post-incident.

3. **Rotation health dashboard** em `infra/dashboards/oncall_health.json` (Grafana Cloud):
   - Métricas listed em §6.3 visualized.
   - Thresholds + alerts wired.
   - Per-tier breakdown.

4. **MTTA/MTTR targets SEV-1 instrumented**:
   - `corelink_incident_mtta_seconds{severity=sev1}` target < 300s.
   - `corelink_incident_mttr_seconds{severity=sev1}` target < 1800s.
   - Alerts wired.

5. **Oncall handoff template** em `specs/_templates/oncall_handoff.md`:
   - Header: outgoing oncall + incoming oncall + ts + correlation_ids active.
   - Active incidents: list + status.
   - Active alerts: list + status.
   - Pending follow-ups: list.
   - Knowledge transfer notes.
   - 15-min sync per shift change canonical.

6. **Runbook URL standardized em alert payload**:
   - S-09 R-S09-14 reuse (alert payload includes runbook URL).
   - PR fail se alert sem runbook URL.

7. **Métricas Prometheus** snake_case canonical (full §6.3 list reuse from sprint.md).

### 6.2 Out-of-scope (deferred)

- Multi-vendor IRM (xMatters, FireHydrant; PagerDuty only at GA per spec contract §10).
- AI-powered oncall scheduling (pós-GA Q1+).
- Customer-facing status page automation (manual at GA).
- APAC oncall coverage at GA (pós-S-20 deferred per spec contract §5.5).

## 7. Anti-Scope

- Skip 7d shift max (burnout prevention violation).
- Skip 2w protection period (sustainability violation).
- Skip fadigue tracking (RPN-203 mitigation gap).
- Skip handoff template (knowledge transfer gap).
- Skip runbook URL em alert (S-09 R-S09-14 gap).
- Custom IRM build (use PagerDuty SaaS).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Oncall PagerDuty + fadigue + dashboard

  Scenario: PagerDuty schedule live
    Given infra/pagerduty/schedule.yaml committed
    When PagerDuty API applied
    Then Tier 1/2/3 schedule live
    And shift ≤ 7d enforced
    And 2-week protection period after shift end
    And rotation iniciada

  Scenario: Fadigue alert SEV-1 > 2/shift
    Given oncall shift active
    When SEV-1 count > 2
    Then alert fires
    And manager review triggered
    And dashboard shows red status

  Scenario: Fadigue alert SEV-2 > 5/shift
    Given oncall shift active
    When SEV-2 count > 5
    Then alert fires

  Scenario: Burnout signal > 10 pages/month em non-rotation
    Given user pages/month > 10
    When user not em active rotation period
    Then burnout signal raised
    And manager review triggered

  Scenario: MTTA < 5min target SEV-1
    Given SEV-1 incident triggered
    When acknowledged em < 300s
    Then MTTA target met
    And dashboard reflects

  Scenario: MTTR < 30min target SEV-1
    Given SEV-1 incident triggered
    When resolved em < 1800s
    Then MTTR target met

  Scenario: Oncall handoff template + 15-min sync
    Given shift change ts
    When 15-min sync executed
    Then handoff template populated
    And outgoing/incoming oncall identified
    And active incidents/alerts/follow-ups documented

  Scenario: Runbook URL standardized em alert payload
    Given alert generated
    When payload inspected
    Then runbook_url field present
    And URL valid + reachable

  Scenario: Métricas Prometheus snake_case
    Given oncall rotation active
    When métricas emitted
    Then corelink_oncall_* counters/gauges emitted
    And labels {tier, in_rotation} canonical
    And NUNCA per-tenant labels

  Scenario: 24/7 future-proof regions
    Given schedule config
    When inspected
    Then US + EU em rotation at GA
    And APAC region placeholder for pós-S-20
```

## 9. Design Decisions

### 9.1 Why PagerDuty (não custom IRM build)

- PagerDuty SaaS = industry-leading IRM.
- Custom build = anti-scope (per spec contract §10).
- PagerDuty free tier sufficient for GA baseline.

### 9.2 Why 7d shift max + 2w protection period

- Google SRE Workbook Ch 8 best practice.
- Burnout prevention evidence-based.
- Per spec contract §5.5 R-S17-12 canonical.

### 9.3 Why 3 tiers (não 2 ou 4)

- Tier 1 = primary responder.
- Tier 2 = escalate (5min unack).
- Tier 3 = architect/security (deep escalation; 10min unack).
- 4+ tiers = scope creep; insufficient differentiation.

### 9.4 Why fadigue thresholds 2 SEV-1 + 5 SEV-2 + 10 pages/month

- Per spec contract §5.5 R-S17-13 canonical.
- Industry-leading targets; balance signal + noise.

### 9.5 Why MTTA < 5min + MTTR < 30min SEV-1 (não 10min/60min)

- Industry-leading SEV-1 targets (Google SRE / Cloudflare Internal).
- Per spec contract §9.8 canonical.

### 9.6 Why APAC deferred pós-S-20

- US + EU sufficient for GA (24/7 coverage via 2 regions).
- APAC GA pós-S-20 demand-driven (per spec contract §5.5).

### 9.7 ADR potencial?

- Não — reuse PagerDuty + Google SRE patterns.

## 10. Completeness Criteria

- [ ] **10.s17.005.1** PagerDuty schedule live (Tier 1/2/3; 7d shift max + 2w protection) (EVT-026 if exists; EVT-018 alternative).
- [ ] **10.s17.005.2** Rotation iniciada.
- [ ] **10.s17.005.3** Fadigue tracking métricas live (SEV-1/shift + SEV-2/shift + pages/month).
- [ ] **10.s17.005.4** Rotation health dashboard live (EVT-021).
- [ ] **10.s17.005.5** MTTA < 5min + MTTR < 30min targets SEV-1 instrumented (per spec contract §9.8).
- [ ] **10.s17.005.6** Oncall handoff template committed.
- [ ] **10.s17.005.7** Runbook URL standardized em alert payload (S-09 R-S09-14 reuse verified).
- [ ] **10.s17.005.8** 30d fadigue baseline established (GA Evidence Gate per Completeness Criteria 10.s17.1).

## 11. DoD

- [ ] PagerDuty schedule live + rotation iniciada.
- [ ] Fadigue dashboard live.
- [ ] MTTA/MTTR instrumented.
- [ ] Handoff template committed.
- [ ] Runbook URL em alert payload verified.
- [ ] Métricas emitting.
- [ ] Adversarial scenarios 4+ documented.

## 12. Invariants Validated

- **PAT-CORRELATION-ID-001** (correlation_id em handoff template).
- **CTRL-PRIV-001** (zero PII em fadigue metrics aggregate; no individual targeting beyond manager review).
- **Não introduz INVs novas** (sprint operational; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| PagerDuty schedule config | `infra/pagerduty/schedule.yaml` | YAML |
| Rotation health dashboard | `infra/dashboards/oncall_health.json` | JSON Grafana |
| Oncall handoff template | `specs/_templates/oncall_handoff.md` | Markdown |
| Fadigue alert config | `infra/pagerduty/fadigue_alerts.yaml` | YAML |

## 14. Quality Standards

- **14.s17.005.1** Oncall fadigue management: shift duration ≤ 7d + 2w protection period; SEV-1 > 2/shift = alert (per Quality Standard 14.s17.4).
- **14.s17.005.2** MTTA < 5min + MTTR < 30min SEV-1 (per Quality Standard 14.s17.8).
- **14.s17.005.3** Cost regression gate: PagerDuty + Grafana ≤ $50/mês.

## 15. Test Plan

### PagerDuty schedule applied
- Verify Tier 1/2/3 schedule live em PagerDuty UI.
- Verify rotation turn 7d.
- Verify 2w protection period rule.

### Fadigue metric tests
- Synthetic SEV-1 burst > 2/shift; verify alert fires.
- Synthetic SEV-2 burst > 5/shift; verify alert fires.
- Synthetic > 10 pages/month em non-rotation; verify burnout signal raised.

### MTTA/MTTR tests
- Synthetic SEV-1 trigger; verify ack < 300s captured; resolve < 1800s captured.

### Handoff template test
- Synthetic shift change; populate template; 15-min sync executed.

### Runbook URL test
- Synthetic alert; verify payload includes runbook_url + URL reachable.

### Adversarial scenarios (4+)
1. Oncall fadigue fatal (leak to prod via missed page; per spec contract §15 row 2): fadigue tracking + > 2 SEV-1/shift alert + manager review + 2w protection period.
2. Oncall handoff issues (knowledge transfer gap; per spec contract §15 row 9): handoff template + 15-min sync per shift change.
3. Runbook URL not in alert payload (per spec contract §15 row 10): S-09 R-S09-14 covers (PR fail if alert sem runbook URL).
4. Burnout signal undetected: > 10 pages/month em non-rotation alert + manager 1:1.

## 16. Failure Modes

- **FM-203** (oncall sobrecarregado / fadiga → missed alert): IMPLEMENTA primary mitigation via fadigue tracking + 7d shift max + 2w protection.
- **FM-205** (manual intervention apaga dado / admin mistake): mitigation via dual-approval (S-13 reuse) + soft-delete; not in this WI scope but listed for context.

## 17. Controls

- **PAT-CORRELATION-ID-001** in handoff template.
- **CTRL-PRIV-001** zero PII em fadigue metrics.

## 18. Resilience Patterns

- Fadigue tracking (burnout prevention pattern).
- 7d shift max + 2w protection (sustainability pattern).
- Tier 1/2/3 escalation (defense-in-depth pattern).
- Handoff template (knowledge transfer pattern).

## 19. Observability

`corelink_oncall_*` métricas Prometheus snake_case (full list em sprint.md §6.3):
- `corelink_oncall_sev1_per_shift{tier}` (gauge; alert > 2).
- `corelink_oncall_sev2_per_shift{tier}` (gauge; alert > 5).
- `corelink_oncall_pages_per_month_total{tier, in_rotation}` (counter; alert > 10 non-rotation).
- `corelink_incident_mtta_seconds{severity=sev1}` (gauge; alert > 300s).
- `corelink_incident_mttr_seconds{severity=sev1}` (gauge; alert > 1800s).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PagerDuty schedule auth via SSO + MFA; oncall handoff template ts attribution.
- **Tampering**: schedule config git-tracked; fadigue metrics ingestion validated.
- **Repudiation**: PagerDuty incident audit trail forensic-grade; handoff template git-tracked.
- **Information disclosure**: fadigue metrics aggregate (no individual targeting beyond manager review for burnout); CTRL-PRIV-001 enforced.
- **DoS**: PagerDuty rate-limited; tier escalation prevents flood.
- **Elevation of privilege**: schedule config requires admin auth.

**LINDDUN delta**:
- Linkability: oncall métricas tenant-agnostic.
- Identifiability: fadigue manager review aggregate baseline + 1:1 conversation if burnout signal (privacy-respecting).
- Non-repudiation: PagerDuty audit trail.
- Detectability: fadigue alerts.
- Disclosure: oncall scope-limited internal.
- Unawareness: handoff template + 15-min sync = knowledge transfer transparency.

## 21. Dependencies

### Hard blockers
- PagerDuty account + SSO configured.
- Grafana Cloud dashboard access.
- S-09 SEALED (audit events R2 + multi-burn-rate alerts; runbook URL em alert payload R-S09-14).

### Soft blockers
- WI-S17-004 SEALED (incident template uses correlation_id; handoff template references).

### Outbound
- WI-S17-006 (game day exercise leverages oncall schedule for tabletop).

## 22. Effort PERT

O: 10h, M: 14h, P: 22h → PERT **14.7h** (per spec contract §12; PD schedule + fadigue métricas + dashboard + handoff template).

## 23. Cost Analysis

**Direct cost**:
- PagerDuty free tier: $0/mês (or $19/user/mês paid; depending tier).
- Grafana Cloud dashboard: $0/mês (free tier).

**Total**: ~$0-100/mês (depends PagerDuty tier).

**Indirect cost**: Burnout prevention + missed page avoided (FM-203 mitigation) = priceless.

## 24. Post-mortem Hooks

- Oncall fadigue alert (> 2 SEV-1/shift) → manager review + system improvements.
- Oncall handoff issues → review + handoff template enforcement.
- Runbook URL missing em alert → PR fail S-09 R-S09-14 enforcement.
- MTTA/MTTR target missed sustained → review + process improvements.

## 25. Rollback / Recovery

PagerDuty schedule rollback: revert config; rotation pause se needed.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Oncall fadigue fatal (leak to prod via missed page) | L | M | HIGH | M | LOW | Fadigue tracking + > 2 SEV-1/shift alert + manager review + 2-week protection period |
| R-002 | Oncall handoff issues (knowledge transfer) | M | M | MEDIUM | M | LOW | Handoff template + 15-min sync per shift change; runbook URL standardized |
| R-003 | Runbook URL not in alert payload | L | L | LOW | L | LOW | S-09 R-S09-14 covers (PR fail if alert sem runbook URL) |
| R-004 | APAC gap pós-GA pre-S-20 | M | L | LOW | L | LOW | US + EU 24/7 sufficient; APAC pós-S-20 deferred per spec contract |

## 27. Knowledge Transfer

- Tech talk (1h): "Oncall em CoreLink — PagerDuty + fadigue tracking + handoff discipline".
- Doc `docs/internal/s17-oncall-procedure.md` — oncall procedure.
- Onboarding test (4 questions): 7d shift max + 2w protection rationale + fadigue thresholds + MTTA/MTTR targets.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _TBD; emphatic — oncall rotation discipline_ | _pending_ | _pending_ |
| 4 | Engineer | Gustavo Schneiter | _pending_ | _pending_ |
| 5 | Oncall Manager | _TBD; emphatic — fadigue review + manager 1:1 escalation_ | _pending_ | _pending_ |
| 6 | QA | _TBD_ | _pending_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — fadigue metrics privacy review_ | _pending_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S17-005 (cycle 12.S17.0; STANDARD lane; oncall PagerDuty + fadigue tracking + dashboard + handoff template + MTTA/MTTR). |

## 30. Anti-patterns evitados

- Skip 7d shift max.
- Skip 2w protection period.
- Skip fadigue tracking.
- Skip handoff template.
- Skip runbook URL em alert.
- Custom IRM build (use PagerDuty SaaS).
- APAC oncall coverage at GA (defer pós-S-20).

---

**Fim WI-S17-005.**
