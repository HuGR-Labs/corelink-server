---
id: "SPEC-CONTRACT-S09"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s09", "observability", "prometheus", "grafana", "standard"]
---

# Spec Contract — S-09: Observability Stack (Metrics/Logs/Traces/Alerts)

## 0. Metadata

| Sprint ID | S-09 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2.5 semanas | WIs | 6 |

## 1. Objetivo

Implementar stack completo de observability: Grafana Cloud (Prom/Loki/Tempo/Mimir) + Logpush → R2 + PagerDuty integration + todos os dashboards canônicos (`observability_model.md §8`) + alerts multi-burn-rate. Sem observability sólida, não há operabilidade.

## 2. Lane + forcing factors

- **Lane:** STANDARD. Foundation para operar produção.

## 3. Inherits_from

```yaml
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "PRIVACY-MODEL"
```

## 4. CAPs entregues

- **CAP-OBS-001**: Prometheus metrics emission (worker → analytics engine → Grafana).
- **CAP-OBS-002**: Structured logs JSON-lines (Logpush → R2 + Grafana Loki).
- **CAP-OBS-003**: Distributed tracing (OTLP → Grafana Tempo).
- **CAP-OBS-004**: CloudEvents audit log (R2 append-only + fan-out).
- **CAP-OBS-005**: 12 dashboards canônicos (DASH-GLOBAL-HEALTH, DASH-CAS, DASH-AC, etc).
- **CAP-OBS-006**: Alertas multi-burn-rate SLO-driven.
- **CAP-OBS-007**: PagerDuty integration (SEV-1/SEV-2 paging).

## 5. Requirements específicos

- **R-S09-1**: Worker analytics engine bindings; métricas conforme `observability_model.md §4`.
- **R-S09-2**: Structured log schema `specs/_schemas/log_event.schema.json` + validator CI.
- **R-S09-3**: OTLP tracing middleware (W3C Trace Context).
- **R-S09-4**: CloudEvents emitter (conforme §7 `observability_model.md`).
- **R-S09-5**: Grafana dashboards-as-code (`observability/dashboards/*.json`).
- **R-S09-6**: Alertmanager rules (`observability/alerts/*.yaml`).
- **R-S09-7**: PII redaction lib (`corelink-log-schema` com allowlist CTRL-PRIV-001).

## 6. DoD

- [ ] 6 WIs SEALED.
- [ ] 12/12 dashboards canônicos live em Grafana.
- [ ] Todos os alertas em `specs/observability/alerts/` validados via `promtool test rules`.
- [ ] PII redaction test: inject PII em logs → redacted em output.
- [ ] Synthetic canary 24/7 de 3 regiões.

## 7. Completeness (delta)

- [ ] **10.s09.1** Cada métrica canônica (`observability_model.md §4.2`) está emitindo em staging.
- [ ] **10.s09.2** Log retention lifecycle rule R2 configurado (90d warm / 400d cold).
- [ ] **10.s09.3** Audit event retention 7y via R2 Object Lock verified.

## 8. Invariants

- CTRL-PRIV-001 (PII em logs): zero findings em DLP scan test.
- INV-AUDIT-APPEND-ONLY: audit events emitidos são append-only (herda S-06).

## 9. Quality Standards

- Métrica cardinality budget respeitado (`observability_model.md §11.2`).
- Alertas: zero flapping por > 3× semana (pager discipline).

## 10. Anti-scope

- ❌ Custom observability backend (Grafana Cloud é o padrão).
- ❌ User-facing observability (customer self-service dashboards — S-16 admin UI).

## 11. Dependencies

- Blocker: S-01+S-02+S-03 (services emitindo métricas reais).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S09-001 | Worker analytics engine + métricas RED |
| WI-S09-002 | Logpush → R2 + Loki + log schema |
| WI-S09-003 | OTLP tracing middleware |
| WI-S09-004 | CloudEvents emitter + R2 audit bucket |
| WI-S09-005 | 12 Grafana dashboards-as-code |
| WI-S09-006 | Alerts multi-burn-rate + PagerDuty integration |

## 13. Duração

2.5 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + synthetic canary rodando 72h sem gap.
- Oncall rotation starts.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Cardinality explosion (tenant_id × op × region) | M | HIGH |
| PII leak em log (CTRL-PRIV-001 bypass) | M | CRITICAL |
| Alertas flapping = oncall fatigue | M | MEDIUM |
| Grafana Cloud outage | L | MEDIUM (FM-153) |

---
