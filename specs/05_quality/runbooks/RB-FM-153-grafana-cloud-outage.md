---
id: "RB-FM-153"
type: "runbook"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "observability", "grafana", "vendor-outage", "rb-fm-153", "wi-s09-007", "dry-run-executed"]
dry_run_executed: "2026-05-03"
dry_run_evidence: "specs/_audits/sealed/2026-05-03-rb-fm-153-dry-run.md"
---

# RB-FM-153 — Grafana Cloud Outage (Observability Backend Down)

> **FM:** FM-153 (S=3, O=2, D=2, RPN=12, P2) | **CTRL:** CTRL-OBS-001 | **SLA:** detect ≤ 10 min, mitigate ≤ 1h

## Detecção

- Grafana status page <https://status.grafana.com> reports incident.
- Internal probe `synthetic_grafana_query_test` fails > 5 consecutive runs.
- Alert delivery silence > 30 min (no PagerDuty pages despite known issues).
- Métrica Prom remote write 5xx spike on Grafana Mimir endpoint.

## Comunicação

- **SEV-2** (não SEV-1: produção continua funcionando; perdemos visibility).
- Page SRE on-call + Engineer responsável por observability stack.
- Internal channel `#incidents-corelink-observability`.
- Não requer customer notification (interno tooling outage).

## Mitigação imediata (≤ 30 min)

1. **Confirmar outage** via Grafana status page + Twitter @grafana_status.
2. **Activate fallback observability**:
   - Cloudflare Workers Analytics Engine queries (raw data ainda disponível).
   - R2 logs (Logpush continua escrevendo) — query via `wrangler r2 object` ou direct R2 API.
3. **Synthetic canary independente**: CF Workers cron job que testa SLO endpoints + email fallback alert via SendGrid (não dependent de Grafana).
4. **Manual SLO check** via worker analytics direct query (CF dashboard).
5. **PagerDuty independent path**: PD direct API call from Worker cron (not via Grafana alertmanager) for SEV-1 paths only.

## Diagnóstico

1. Verificar com Grafana support (paid SLA);
2. Determine impact window (durou quanto, region affected, services affected).
3. Inventory: alerts perdidos? Data perdida? Dashboards offline?

## Resolução

- Aguardar Grafana recovery (typically < 4h for major incidents per SLA).
- Verify metrics backfill funcionou pós-recovery (sample queries com timestamps within outage window).
- Re-validate alerts triggered durante outage (via raw R2 logs query).

## Post-incident

- Post-mortem dentro de 7d.
- Review Grafana SLA + tier (upgrade considered se outage > 1h frequente).
- Improve fallback path (synthetic canary independente já existe S-09 R-S09-7?; stress test).
- Document em `docs/runbooks/observability-fallback.md` que produção continua sem Grafana (visibility loss only).

## Evidence

- Grafana status page screenshot.
- Internal probe metrics.
- Synthetic canary fallback alerts.
- Customer-facing impact (typically zero).

## References

- `failure_modes.md` FM-153 entry.
- `observability_model.md §11` fallback observability strategy.
- `specs/04_sprints/_sealed/S09/_spec_contract.md` (canary independente).
- Grafana Cloud SLA: <https://grafana.com/legal/sla/>.
