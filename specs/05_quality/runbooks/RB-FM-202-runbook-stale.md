---
id: "RB-FM-202"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "operational", "meta"]
---

# RB-FM-202 — Runbook Desatualizado em Incident

> **FM:** FM-202 (RPN=36, P1) | **PAT:** PAT-RUNBOOK-DRILL-001 | **SLA:** atualizar runbook ≤ 24h pós incident

## Detecção (durante incident)

- Oncall reporta "este passo do runbook não funciona / API mudou / serviço renomeado".

## Mitigação imediata

1. Marcar runbook como `STALE` (header) + abrir issue/ticket.
2. Improvisar mitigação documentando passos que funcionaram.
3. Capturar screenshots/output do que mudou.

## Mitigação completa (≤ 24h)

1. PR atualizando runbook com correções.
2. Bump version (patch).
3. Re-validate executando dry-run em staging (EVT-017).
4. Se mudança fundamental: ADR + revisão de FMs relacionados.

## Detection & Index (canonical S-17 operational path)

Drill cadence + overdue scan is automated by `corelink-runbook-tracker` (WI-S17-003). The canonical catalog of P0/P1 runbooks subject to monthly cadence is `specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md`. The CF Cron at `0 12 1 * *` UTC:

1. Loads RB-RUNBOOK-DRILL-INDEX catalog.
2. Queries D1 `runbook_drills` for each `runbook_id`'s `MAX(executed_at)`.
3. Emits `OverdueAlert` event (CloudEvent `dev.hugr.corelink.runbook.overdue.v1`) when last drill ≥ 30 days stale.
4. Prometheus counter `corelink_runbook_dry_run_total{outcome="overdue"}` increments.
5. Slack page to `#sre-oncall`.

When `OverdueAlert` fires for a runbook: that runbook is *probably* stale (untested ≥ 30d). Treat as FM-202 risk and prioritize next drill in the rotation. If the drill execution reveals "instructions don't work" → this runbook (RB-FM-202) applies for the recovery action.

**Escalation path:**

| Time         | Who                              | Criteria                                  |
|--------------|----------------------------------|-------------------------------------------|
| 0            | On-call SRE (Slack `#sre-oncall`)| `OverdueAlert` fires OR drill reveals stale |
| 24h          | Runbook owner (per RB front-matter)| Update PR not opened                      |
| 30d cumulative| SRE Lead                        | > 3 P0/P1 RBs simultaneously overdue      |

**Comms (internal):**

```
RB-FM-202 — Runbook stale detected
Runbook: <RB id>; last drill: <date>; drift signal: <overdue|instructions-fail>.
Owner: @{handle}. Patch PR ETA: ≤ 24h.
```

## Post-incident

- All updates to a stale RB must include a *fresh dry-run* row in `runbook_drills` table (executed within 7 days of patch merge).
- Audit: every quarter, count `corelink_runbook_dry_run_total{outcome="overdue"}` and report in `DASH-SLO-CATALOG`.

## Related

- **FM:** FM-202 (RPN=36, P1).
- **Pattern:** PAT-RUNBOOK-DRILL-001 (monthly mandatory dry-run).
- **Canonical index:** `specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md`.
- **WI:** WI-S17-003 (runbook tracker).
- **Sister runbooks:** every P0/P1 RB listed in the index.

## Prevenção

- PAT-RUNBOOK-DRILL-001: oncall faz 1 dry-run de runbook P0/P1 por mês.
- Output do dry-run em EVT-017 obrigatório.
- Runbook sem dry-run > 6 meses → marcar `audit_status: AUDIT_PENDING`.
