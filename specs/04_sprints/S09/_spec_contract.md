---
id: "SPEC-CONTRACT-S09"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-24"
updated: "2026-05-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s09", "observability", "metrics", "logs", "traces", "alerts", "slo", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-09: Observability Stack (RED + USE + Logs/Traces/Audit/Alerts)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-09 |
| Nome | Observability Stack |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-003 (PII em logs/audit), FF-HR-005 (CTRL-AUDIT-001 audit chain integrity) |
| Duração estimada | 2.5 semanas |
| WIs antecipados | 7 |
| SOTA target | Production-grade SRE observability — 4 pillars (metrics, logs, traces, audit) + multi-burn-rate SLO alerts + cardinality guardrails |

## 1. Objetivo

Implementar **stack completo de observability SOTA** que torna o CoreLink **operável em produção** desde o dia 1 da GA: 4 pilares conforme Google SRE Workbook (metrics RED+USE, structured logs, distributed traces, audit events) + alertas SLO-driven multi-burn-rate (Sloth/SRE-style) + 12 dashboards canônicos + PII redaction enforcement + PagerDuty integration end-to-end. Sem observability sólida não há operabilidade, e sem operabilidade não há GA.

**Por que SOTA:** competitors (BuildBuddy, NativeLink) têm dashboards básicos mas faltam:
- (a) cardinality guardrails (explosão de label cartesianas é o #1 root cause de blowup de custo Prom);
- (b) multi-burn-rate alerts (vs threshold-based clássicos = false positives);
- (c) PII redaction tipada nos logs (zero-trust on log pipeline);
- (d) audit trail separado de logs operacionais (compliance);
- (e) exemplars conectando métricas ↔ traces (debug 10× mais rápido).

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-003** (PII handling): pipelines de logs/audit processam PII em scrutiny; CTRL-PRIV-001 bypass = leak regulatory.
- **FF-HR-005** (CTRL-AUDIT-001 audit chain integrity): hash chain break = compliance gap (SOC 2 CC7.2).
- **Justificativa rigor**: observability é foundation para todos os outros sprints; falha = blind production = SEV-1 inevitável.

## 3. Inherits_from

```yaml
inherits_from:
  - "OBSERVABILITY-MODEL"      # 12 dashboards canônicos + métricas RED §4
  - "SLO-CATALOG"              # objetivos numéricos para alerts multi-burn-rate
  - "FAILURE-MODES"            # FM-NNN → alerts mapping + runbook crossref
  - "RESILIENCE-PATTERNS"      # PAT-* circuit breaker / retry observability
  - "PRIVACY-MODEL"            # CTRL-PRIV-001 PII redaction enforcement
  - "SECURITY-MODEL"           # CTRL-AUDIT-001 audit log integrity
  - "DATA-MODEL"               # cardinality budget per dimension
  - "INVARIANT-REGISTRY"       # INV-OBS-CARDINALITY-BUDGET, INV-OBS-AUDIT-CHAIN-INTEGRITY, INV-AUDIT-APPEND-ONLY (Lote 9.4)
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-OBS-001** | Metrics emission (RED + USE) | Worker → Cloudflare Analytics Engine → Prom remote write → Grafana Mimir; cardinality budget enforced. |
| **CAP-OBS-002** | Structured logs (JSON-lines) | Logpush → R2 (cold 400d) + Grafana Loki (warm 90d); schema-validated em CI. |
| **CAP-OBS-003** | Distributed tracing (OTLP) | W3C Trace Context propagation; head-based sampling 1% + tail-based sampling para errors. Tempo backend. |
| **CAP-OBS-004** | CloudEvents audit log | Append-only R2 (Object Lock Governance Mode 7y); fan-out via Pub/Sub-equivalent (Queue) para SIEM. |
| **CAP-OBS-005 (Lote 10.9bis P0-D corrected)** | 12 dashboards canônicos (aligned com observability_model.md §10 Nível-3 canonical) | DASH-GLOBAL-HEALTH, DASH-GLOBAL-PRODUCT, DASH-TENANT, DASH-CAS, DASH-AC, DASH-EXEC, DASH-GC, DASH-SUPPLY-CHAIN, DASH-SECURITY, DASH-PRIVACY, DASH-COST, DASH-SLO-CATALOG. JSON-as-code. Note: prior list (DASH-AUTH/BILLING/RATE-LIMIT/DEDUP/CHAOS) refactored into panels em parent dashboards (Lote 10.9bis P0-D Phase 2). |
| **CAP-OBS-006** | Multi-burn-rate SLO alerts | Sloth-style 4 windows (1h/6h, 5min/30min) per SLI; reduces false positives vs threshold simples. |
| **CAP-OBS-007** | PagerDuty integration | SEV-1 / SEV-2 paging; runbook URL no alert payload (deep link); silence policies por window de change. |
| **CAP-OBS-008** | PII redaction enforcement | `corelink-log-schema` crate + CI check + DLP scanner em logs de staging para regression test. |
| **CAP-OBS-009** | Exemplars (metrics ↔ traces) | OpenMetrics exemplars expor trace_id em métrica histogram → click no Grafana abre trace correspondente. |

## 5. Requirements específicos

### 5.1 Métricas (CAP-OBS-001)

- **R-S09-1**: Worker analytics engine bindings emitindo métricas RED conforme `observability_model.md §4.2`:
  - `corelink_cas_put_requests_total{tenant_tier, region, result}`
  - `corelink_cas_put_duration_seconds{tenant_tier, region, p50/p95/p99}`
  - `corelink_cas_get_bytes_total{tenant_tier, region}`
  - `corelink_ac_lookup_requests_total{tenant_tier, region, hit/miss}`
  - `corelink_gc_runs_total{phase, status}`
  - `corelink_dedup_ratio{tenant_tier, region}` (S-07)
  - `corelink_rate_limit_rejects_total{layer, tenant_tier, reason}` (S-08)
  - `corelink_privacy_dsr_active_total{type}` (S-11)
  - `corelink_billing_events_emitted_total{type, region}` (S-10)
- **R-S09-2**: **Cardinality budget**: máximo **20k séries únicas por métrica em produção**, **100k global**. Validator em CI rejeitando label sets além do budget. Reference: `observability_model.md §11.2`.
- **R-S09-3**: USE metrics (Utilization/Saturation/Errors) para infraestrutura cloudflare: Worker CPU time (cf.cpu_time), R2 ops/sec, D1 row scans, KV read/write quotas, DO storage size.

### 5.2 Logs (CAP-OBS-002 + CAP-OBS-008)

- **R-S09-4**: Structured log schema **`specs/_schemas/log_event.schema.json`** com:
  - Required: `ts (ISO8601 RFC 3339)`, `level (DEBUG|INFO|WARN|ERROR)`, `event (snake_case)`, `tenant_id (UUID)`, `region`, `request_id`, `trace_id`.
  - Forbidden in payload (CTRL-PRIV-001): email patterns, IPv4/IPv6 (exceto allowlist `client_ip` redacted to /24), bearer tokens, blob digests sem hash truncation > 16 chars.
  - Validator de schema em CI + rejeição de PR com log inválido.
- **R-S09-5**: Logpush configuration → R2 com lifecycle: hot 30d (Loki query), warm 90d (Logpush index), cold 400d (R2 Glacier-equivalent), purge > 400d.
- **R-S09-6**: PII redaction lib `corelink-log-schema` (Rust crate) com:
  - `redact!(field, value)` macro que aplica tipo-driven redaction;
  - allowlist explicit: `RedactPolicy::Allowlist([&str])` types só em `INFO` ou superior;
  - DLP scanner test: gerar 10k log lines com PII fixtures → expect 0 leaks.

### 5.3 Tracing (CAP-OBS-003 + CAP-OBS-009)

- **R-S09-7**: OTLP middleware (W3C Trace Context — `traceparent` header propagation).
- **R-S09-8**: Sampling: head-based 1% default + tail-based 100% para spans com `error=true` ou `latency_p99_breach=true`. Reference: OpenTelemetry SDK SpanProcessor.
- **R-S09-9**: **Exemplars**: histograms (`*.duration_seconds`) emitem trace_id no exemplar — Grafana Tempo deep link (CAP-OBS-009).

### 5.4 Audit (CAP-OBS-004)

- **R-S09-10 (Lote 10.9bis P0-H corrected)**: CloudEvents v1.0 emitter conforme `observability_model.md §7`:
  - Subject: 8 canonical: `tenant:<id>`, `cas:put`, `cas:get`, `ac:lookup`, `gc:purge`, `auth:login`, `quota:exceeded`, `abuse:detected` (Lote 10.9bis P0-H added abuse:detected to align with DoD §6 "8 subjects" + S-08 abuse score events compliance auditing).
  - Sink: R2 bucket `audit-events` com Object Lock Governance Mode 7y (CTRL-AUDIT-001).
  - Hash chain: cada event tem `prev_hash` + own digest → daily verifier job.
- **R-S09-11**: Fan-out via Cloudflare Queue → SIEM webhook (configurable per region) para customer compliance integration.

### 5.5 Dashboards & Alerts (CAP-OBS-005 + CAP-OBS-006 + CAP-OBS-007)

- **R-S09-12**: 12 Grafana dashboards-as-code em `observability/dashboards/*.json` versionados; provisionamento via Grafana API.
- **R-S09-13**: Multi-burn-rate SLO alerts seguindo Google SRE Workbook §5 ("burn-rate alerting"):
  - Para cada SLI no SLO-CATALOG, gerar 4 alerts: page if `burn_rate(1h) > 14.4 AND burn_rate(5min) > 14.4`; ticket if `burn_rate(6h) > 6 AND burn_rate(30min) > 6`.
  - Reference: <https://sre.google/workbook/alerting-on-slos/>.
- **R-S09-14**: PagerDuty service per environment (staging, prod-us, prod-eu); SEV-1 → on-call page; SEV-2 → ticket; runbook URL no alert payload (deep link).

## 6. Definition of Done

- [ ] **WIs SEALED**: 7/7.
- [ ] **Métricas**: 100% das métricas em `observability_model.md §4.2` emitindo em staging com cardinality budget respeitado. Validador `cardinality_check.py` verde.
- [ ] **Logs**: schema validation em CI + PII redaction DLP test 0 leaks em 10k fixtures.
- [ ] **Tracing**: traces W3C-compliant + exemplars funcionando (click em Grafana abre Tempo).
- [ ] **Audit**: CloudEvents emitidos para todos os 8 subjects + chain verify daily job verde por 7d.
- [ ] **Dashboards**: 12/12 live em Grafana com data flowing; screenshot per dashboard arquivada em `docs/dashboards/`.
- [ ] **Alerts**: `promtool test rules` verde para 100% das regras; multi-burn-rate alert dry-run via injeção de SLO breach sintético confirma fire em < 5min.
- [ ] **PagerDuty**: SEV-1 + SEV-2 dispatch end-to-end testado (synthetic page) ack < 5min.
- [ ] **Synthetic canary**: 24/7 de 3 regiões (us-east, eu-west, ap-south) sustentado 72h sem gap.
- [ ] **Runbook dry-run** (EVT-017): `RB-FM-153` (Grafana Cloud outage) + `RB-OBS-CARDINALITY-001` (cardinality explosion) executados em staging.
- [ ] **Sign-offs (HIGH_RISK 10–12)**: SRE lead + Privacy officer + Security lead + Engineer responsável + QA + Product + Compliance officer + Architect + AppSec advisor + DPO interim + 1 peer reviewer.

## 7. Completeness Criteria (delta local)

- [ ] **10.s09.1** Cada métrica canônica de `observability_model.md §4.2` emitindo em staging por ≥ 24h.
- [ ] **10.s09.2** Log retention lifecycle R2 configurado: 90d warm Loki / 400d cold R2 / purge > 400d (privacy_model §6).
- [ ] **10.s09.3** Audit event retention 7y via R2 Object Lock Governance verified via `aws s3api get-object-lock-configuration`.
- [ ] **10.s09.4** **PagerDuty MTTA < 5min** (synthetic test sustained 7d).
- [ ] **10.s09.5** **Cardinality budget enforced em CI** — PR que adiciona label fora do allowlist → fail.
- [ ] **10.s09.6** **Exemplars working**: histogram → click trace_id → Tempo abre o trace (3 fluxos testados: cas.put, cas.get, ac.lookup).
- [ ] **10.s09.7** **DLP regression test**: PII injection em 10k log lines → 0 leaks (CTRL-PRIV-001).
- [ ] **10.s09.8** **Audit chain verify job**: rodando daily, alerta se `prev_hash` chain quebra; 7d clean.

## 8. Invariants

### Mantidas (heredadas de canonical sources)

- **CTRL-PRIV-001** (PII em logs): zero findings em DLP scan test em CI.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, herda S-06): audit events emitidos são append-only via R2 Object Lock; chain verify job confirma daily.
- **CTRL-AUDIT-001**: integrity chain digital signed via per-region key.

### Novas (introduzidas por S-09 — adicionar a invariant_registry.md)

- **INV-OBS-CARDINALITY-BUDGET** (HIGH — novo): nenhuma métrica excede 20k séries únicas; nenhum total > 100k. **Why:** explosão cartesiana de labels = OOM em Prom + 100× cost spike. **Enforce:** validator CI + Grafana Mimir tenant limit.
- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH — novo): hash chain de audit events é unbroken; daily verifier job alerta em break. **Why:** audit chain quebrado = compliance gap (SOC 2 CC7.2). **Enforce:** background job + INV-AUDIT-APPEND-ONLY.

## 9. Quality Standards (delta local)

- **14.s09.1 Cardinality discipline**: PRs que adicionam métrica nova passam por `cardinality_check.py` que estima séries via cardinalidade observada em staging. PRs que excedem orçamento são auto-rejected.
- **14.s09.2 Alert discipline**: alertas que disparam > 3× por semana sem causa real (flapping) → auto-quarantine via PagerDuty API até root-cause analysis. Reference: SRE Workbook Ch 8.
- **14.s09.3 Runbook coverage**: cada alerta SEV-1/SEV-2 tem runbook URL no payload. PR que cria alerta sem runbook URL → fail no schema validator.
- **14.s09.4 Tracing coverage**: ≥ 90% das requests CAS/AC têm trace_id; ≥ 100% das requests com latency p99 breach têm trace tail-sampled (visível em Tempo).
- **14.s09.5 Log volume budget**: produção emite ≤ 1 GB/dia/tenant em logs (warning), ≤ 5 GB/dia (alert). Reference: privacy_model §11.4 cost control.
- **14.s09.6 Dashboard freshness**: cada dashboard tem `lastUpdated` annotation; PR de mudança de funcionalidade core deve atualizar dashboard correspondente (CI hook).
- **14.s09.7 Cost regression gate** (Lote 9.5b — meta §14.10): cardinality budget é literal Prom cost — gate é o pontocentral. Grafana Mimir tenant cost projection $USD/month per tier; PR adicionando label > 10% cardinality estimate bloqueia merge sem ADR. Logpush volume + Tempo trace storage included em estimate.

## 10. Anti-scope

- ❌ Custom observability backend / self-hosted Prom — Grafana Cloud é o padrão.
- ❌ User-facing observability (customer self-service dashboards, billing visualizations) — S-13 admin plane / S-16 frontend admin UI.
- ❌ APM full feature parity (Datadog/NewRelic) — escolhemos OTel-native + Grafana stack; não somos APM vendor.
- ❌ Real User Monitoring (RUM) — não temos frontend público com escala que justifique.
- ❌ AI-driven anomaly detection — fora de escopo Fase 1; pode entrar Fase 2.
- ❌ Synthetic monitoring extra-regional (>3 regiões) — 3 é suficiente para confidence inicial.
- ❌ Customer-facing audit log export (UI/API) — escopo S-13 admin plane.

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS write) — métricas reais emitindo.
- **S-02 SEALED** (CAS read) — métricas reais emitindo.
- **S-03 SEALED** (Auth) — tenant_id em todos os logs/traces.

### Soft blockers (preferencial mas não impeditivo)

- **S-04 SEALED** (AC) — métricas AC.
- **S-06 SEALED** (GC) — métricas GC + INV-AUDIT-APPEND-ONLY foundation.
- **S-07 SEALED** (Dedup) — CAP-OBS-005 dashboard DASH-DEDUP.
- **S-08 SEALED** (Rate limit) — CAP-OBS-005 dashboard DASH-RATE-LIMIT.

### Outbound (sprints que dependem deste)

- S-10 (billing) — usa audit events para reconciliation.
- S-11 (privacy) — usa logs DLP scan para CTRL-PRIV-001.
- S-13 (admin plane) — re-usa dashboards.
- S-14 (BYOK) — adiciona métricas key.
- S-17 (chaos) — usa SLO alerts para validação.
- S-20 (GA readiness) — exige 12/12 dashboards live + 72h staging clean.

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | Optimistic | Most-likely | Pessimistic | PERT estimado |
|---|---|---|---|---|---|---|
| **WI-S09-001** | Worker Analytics Engine + métricas RED + cardinality validator | bindings AE; emit lib; cardinality_check.py; 9 métricas core; CI hook | 12h | 18h | 28h | **18.7h** |
| **WI-S09-002** | Logpush → R2 + Loki + log schema + PII redaction | log_event.schema.json; redact! macro; Logpush config; lifecycle; DLP test | 14h | 22h | 36h | **23.0h** |
| **WI-S09-003** | OTLP tracing middleware + W3C + sampling + exemplars | trace propagation; head + tail sampling; exemplar emission; Tempo integration | 10h | 16h | 24h | **16.3h** |
| **WI-S09-004** | CloudEvents emitter + R2 audit bucket + hash chain + daily verify | emitter; bucket Object Lock; chain logic; daily verify job; SIEM fan-out | 12h | 18h | 28h | **18.7h** |
| **WI-S09-005** | 12 Grafana dashboards-as-code | JSON-as-code 12 dashboards; provisionamento; screenshots; freshness hook | 16h | 24h | 36h | **24.7h** |
| **WI-S09-006** | Multi-burn-rate SLO alerts + promtool test + PagerDuty integration | alert rules; promtool tests; PagerDuty service config; runbook URL hook | 10h | 14h | 22h | **14.7h** |
| **WI-S09-007** | Synthetic canary 3 regiões + dashboard health-check + runbook dry-run | canary script; dashboard; RB-FM-153 dry-run; RB-OBS-CARDINALITY-001 dry-run | 8h | 12h | 18h | **12.3h** |

**Total PERT:** ~128h ≈ 16 dias work × 1 eng. Buffer 3 dias confere com 2.5 semanas.

## 13. Duração + Timeline

- **Duração:** 2.5 semanas (12 dias úteis) + buffer 3 dias.
- **Marcos:**
  - **D+3:** WI-001 + WI-002 SEALED → métricas + logs flowing.
  - **D+6:** WI-003 + WI-004 SEALED → tracing + audit flowing.
  - **D+9:** WI-005 SEALED → 12 dashboards live.
  - **D+11:** WI-006 + WI-007 SEALED → alerts + canary live.
  - **D+12:** Synthetic canary 72h confirmation start.
  - **D+15:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + Synthetic canary rodando 72h sem gap.
- 12/12 dashboards live + alerts multi-burn-rate testados.
- PII redaction DLP test 0 leaks.
- Audit chain verify 7d clean.
- Oncall rotation starts (PagerDuty schedule live).
- PRR-S09 approved (HIGH_RISK lane: 11 sign-offs conforme §6 DoD).

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Cardinality explosion** (tenant_id × op × region cartesian) | M | H | HIGH (cost 100×) | M*H = M | LOW | Cardinality budget validator CI + Grafana Mimir tenant limit; alert se aproxima 80% budget. |
| **PII leak em log** (CTRL-PRIV-001 bypass) | M | M | CRITICAL (compliance + reputation) | H | LOW | DLP scanner CI + redact! macro tipo-driven + per-PR hook; CTRL-PRIV-001 enforcement + privacy review. |
| **Alert flapping = oncall fatigue** | H | H | MEDIUM | H | LOW | Multi-burn-rate (vs threshold) + auto-quarantine + alert review weekly + 14.s09.2 quality. |
| **Grafana Cloud outage** (FM-153) | L | M | MEDIUM (alerts down) | L*M = L | LOW | Synthetic canary independente (CF Workers cron) + RB-FM-153 dry-run + 2nd-tier email fallback. |
| **Tail-sampling overhead** em high-RPS Workers | M | M | MEDIUM (latency tax > 5%) | M | LOW | Sample 100% só para errors; default 1% head sampled; benchmark before merge. |
| **Audit chain break por region split-brain** | L | H | HIGH (compliance gap) | M | LOW | Per-region chain (não global); chain verify daily + alert imediato em break. |
| **Logpush latency > 30s** (data lag) | M | L | LOW (debug delay) | L | LOW | Aceitável; documentar em SLO; canário independente em < 1s. |
| **PagerDuty webhook deduplication** loss | L | L | MEDIUM (missed page) | L | LOW | dedup_key estável por incident_id; cron synthetic page weekly to validate dispatch. |
| **Tracing cardinality from trace_id labels** em métricas | M | H | HIGH (cardinality explode) | H | LOW | trace_id NUNCA em label; só em exemplar field separate; lint check. |

## 16. Benchmarks SOTA externos (target qualitativo)

| Critério | Datadog | Grafana Cloud | Honeycomb | **CoreLink target S-09** |
|---|---|---|---|---|
| Métricas cardinality budget enforcement | Limited (custom_metrics quota) | Mimir tenant limit | Per-dataset limit | **Validator CI + per-metric explicit budget** |
| Multi-burn-rate alerting | Manual config | Sloth-supported | Native | **Sloth-style 4-window per SLI no SLO-CATALOG** |
| Exemplars (metrics ↔ traces) | Yes | Yes (Tempo) | Native | **Yes — OpenMetrics standard** |
| PII redaction nos logs | Plug-in | Custom processor | Manual | **`redact!` macro tipo-driven em Rust + DLP CI** |
| Audit log separado de operational | Plug-in | Manual | Manual | **CloudEvents R2 Object Lock 7y separado** |
| Sampling tail-based para errors | Yes (APM) | Yes (Tempo) | Native | **Yes — head 1% + tail 100% errors** |
| Cardinality budget per dimension | Plug-in | Manual | Native | **Explicit `observability_model.md §11.2`** |

**Veredito:** S-09 v1.1 atinge feature parity com Datadog/Grafana Cloud em **8/8 dimensões**, com vantagem em PII redaction (tipo-driven Rust > runtime processors).

## 17. References (RFCs, papers, standards)

- **OpenTelemetry Specification v1.32** — semantic conventions for HTTP, RPC <https://opentelemetry.io/docs/specs/>.
- **W3C Trace Context Recommendation** <https://www.w3.org/TR/trace-context/>.
- **OpenMetrics 1.0** (CNCF) — exemplars spec <https://openmetrics.io/>.
- **CloudEvents Specification v1.0.2** (CNCF) <https://cloudevents.io/>.
- **RFC 5424** — Syslog Protocol (severity levels).
- **RFC 3339** — Date and Time on the Internet (timestamps).
- **Google SRE Workbook Ch 5** — Alerting on SLOs (multi-burn-rate).
- **Google SRE Workbook Ch 6** — Distributed Tracing.
- **Google SRE Workbook Ch 8** — On-call (alert discipline).
- **Sloth project** — SLO alerting as code <https://sloth.dev/>.
- **NIST SP 800-92** — Guide to Computer Security Log Management.
- **SOC 2 CC7.2** — System Monitoring (audit log integrity requirement).

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** em `specs/_postmortems/` mesmo sem SEV declarado:

- Cardinality explosion (uma métrica > 80% do budget) → 5-Why mandatório (root cause: schema, code path).
- Alert flapping > 3× semana mesmo após auto-quarantine → 5-Why obrigatório.
- PII leak detectado em produção (não em CI) → SEV-1 + post-mortem mandatório dentro de 48h.
- Audit chain break em produção → SEV-1 + post-mortem + compliance officer notificado.
- Tracing tail-sampling drop rate > 5% (lost spans) → post-mortem (debug capability degradada).

## 19. Waiver policy

S-09 não pode promover via waiver dos seguintes itens:

- ❌ DoD "Synthetic canary 24/7 sustentado 72h" — sem isso não há confidence em production.
- ❌ DoD "PII redaction DLP test 0 leaks" — CTRL-PRIV-001 é regulatory requirement.
- ❌ DoD "Audit chain verify daily 7d clean" — INV-AUDIT-APPEND-ONLY foundation.
- ❌ Cardinality budget enforcement — sem isso runaway cost a primeira semana de produção.

Itens waivable com justificativa explícita + sign-off SRE lead + Privacy officer + ADR:

- ⚠️ Reduzir 12 → 10 dashboards iniciais se 2 dependem de sprints futuras (mas commitar prazo).
- ⚠️ Tail sampling 100% errors → 50% errors se observed overhead > 5%.

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-24 | Gustavo (initial design) | Initial spec contract S-09 (Observability Stack RED+USE+Logs/Traces/Audit/Alerts; HIGH_RISK lane; FF-HR-003 PII handling + FF-HR-005 CTRL-AUDIT-001 audit chain integrity). |
| 1.3.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S09-002 SEAL) | **WI-S09-002 SEALED — `crates/corelink-logpush/` v0.1.0 shipped + `migrations/d1/0016_log_schema.sql` (NEW tables; per-region log-schema version mirror + 5-pattern redaction config mirror; CTRL-PRIV-001 enforcement source-of-truth).** Implementation diverges from the WI-design's `redact!` proc-macro tipo-driven enforcement to a `PiiRedactor` trait + `InMemoryPiiRedactor` 5-pattern hand-rolled byte-level scanner surface aligned with the in-flight S-09 emit primitive trait-abstraction-defer pattern (per `corelink-analytics::CardinalityValidator` ship gate from WI-S09-001 SEAL); the runtime scanner IS the load-bearing CTRL-PRIV-001 falsifiability target — the `redact!` macro + 4 wrapper-type redactors (EmailAddress / IpAddress / BearerToken / BlobDigest) are deferred to WI-S09-007 PRR ship gate alongside production CF Logpush + R2 + Loki + Terraform IaC + ajv-cli CI gate (`trait-abstraction-defer` charter pattern). New crate ships 6 source modules (~2842 LOC src + ~991 LOC tests + 160 LOC migration = ~3993 LOC total): `record` (`LogRecord` CloudEvents 1.0 aligned: `specversion` `"1.0"` hard-pinned + `event_type` `LogEventType` `#[non_exhaustive]` 4-event canonical taxonomy `request_served` / `auth_attempt` / `admin_action` / `billing_event` + `source` Worker name + `id` UUIDv7 + `time_ms` Unix epoch ms + `tenant_id Option<Uuid>` pseudonymous TenantCtx-only enforcement per S-03 + `region` mirrored from `corelink_analytics::Region` 22-canonical CF colocode + `data` redacted serde_json::Value; canonical NDJSON serializer round-trip pinned; `SCHEMA_VERSION` const = 16); `redaction` (`PiiPatternKind` `#[non_exhaustive]` 5-canonical taxonomy email/ip/token/pan/cpf_cnpj + 6 canonical placeholders `<EMAIL_REDACTED>` / `<IP_REDACTED>` / `<TOKEN_REDACTED>` / `<PAN_REDACTED>` / `<CPF_REDACTED>` / `<CNPJ_REDACTED>` + `PiiRedactor` trait + `InMemoryPiiRedactor` per-instance F-001 closure with hand-rolled byte-level scanners — NO regex crate per anti-scope INV-AVAIL-DOS canary; match precedence Token → IP → CPF/CNPJ → PAN → Email so IPv4-shaped digit runs never collide with the 11/14-digit CPF/CNPJ mod-11 validators; Luhn-validated PAN; Brazilian CPF/CNPJ mod-11 with canonical multiplier weights + all-same-digit reject); `sink` (`LogSink` trait + `InMemoryLogSink` orchestrator wiring `corelink-analytics::CardinalityValidator` cardinality guard + `PiiRedactor::redact_json` walk-tree-redact + audit-emit-BEFORE-mutation fail-closed envelope on every decision arm + NDJSON serialize + per-instance `Arc<Mutex<>>` F-001 closure buffer push; `record_sink_failure` arm for the production-side fail-OPEN sink-failure ledger `corelink_logs_ingest_failures_total` SEV-3 alert source; `CapturedLogSink` + `FailingLogSink` adversarial fixtures); `audit` (`LogAuditEventType` `#[non_exhaustive]` 4-event canonical taxonomy `corelink.logpush.{record_emitted, redaction_applied, redaction_failure, sink_failure}` + `LogAuditRecord` typed shape + `LogAuditSink` trait + `InMemoryLogAuditSink` capture + `FailingLogAuditSink` adversarial fixture; SEV-1 classification on `RedactionFailure` + SEV-3 classification on `SinkFailure` per WI §6.1.10); `error` (`LogpushError` `#[non_exhaustive]` taxonomy with `RedactionFailure` + `Audit` + `Sink` + `CardinalityBudgetExceeded` per_metric/global scope discriminator + `Internal` mutex poisoning); `lib` (re-exports + `MIGRATION_0016_LOG_SCHEMA` embedded SQL artifact + `logpush_schema_version()` const fn = 16). Migration `0016_log_schema.sql`: 2 tables `log_schema_versions` (per-region schema version mirror PK region 3-char) + `log_redaction_patterns` (composite PK region+pattern_id; pattern_id constrained to canonical 5-element allowlist `email`/`ip`/`token`/`pan`/`cpf_cnpj`); 2 indices for cold-start hydration JOIN + admin forensic surface; idempotent + additive; canonical schema version 16; column drift no `_ms` suffix per Lote 10.7bis P0-3. Tests: **77 tests across all targets, 0 failures, parallel-safe**: 51 inline lib unit + 10 migration_canonical_0016 + **1 `pii_redaction_100k_synthetic_zero_leakage` deterministic seeded ChaCha20Rng 100k zero-leak gate** (§6 DoD; 5 categories × 20k = 100k samples; release run 0.21s; verified ZERO PII leakage; statistical 95% CI Wilson upper-bound leak rate < 0.0037%) + 15 prop_logpush including the 7 required property tests at 10k iter PR-gate (`PROPTEST_CASES` env var override at runtime per S-07 P1-2 fix): `prop_pii_redaction_no_leakage` (CTRL-PRIV-001 enforcement; raw email + IPv4 + bearer never appear in redacted output), `prop_redaction_idempotent` (`redact(redact(x)) == redact(x)` + 0 hits on second pass), `prop_redaction_preserves_non_pii` (clean ASCII text passes through unchanged), `prop_log_schema_serialization_roundtrip` (NDJSON serialize → deserialize → equal), `prop_tenant_isolation` (tenant A body data never references tenant B; INV-TENANT-ISOLATION canary), `prop_audit_emit_per_event_type` (every LogEventType emits canonical RecordEmitted audit), `prop_cardinality_budget_respected` (over-budget tuple addition rejected; INV-OBS-CARDINALITY-BUDGET integration). Plus surface-pinning regression tests covering canonical event-type strings + audit event strings + canonical pii pattern kinds + 6 canonical placeholder constants + schema version 16 + audit fail-closed envelope. **Trait-abstraction-defer per charter**: real CF Logpush job + R2 lifecycle Terraform IaC + Loki tenant config + LogQL canonical queries + ajv-cli CI gate + 100k nightly via `PROPTEST_CASES=100000` + chaos 11 + JSON Schema 2020-12 strict + `redact!` proc-macro + 4 wrapper-type redactors EmailAddress/IpAddress/BearerToken/BlobDigest — all consolidated alongside WI-S09-007 PRR ship gate per `trait-abstraction-defer` charter pattern. Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-logpush` member + workspace dep + `serde` feature flag on uuid; reuses `corelink-analytics::Region` + `Tier` + `MetricLabelTuple` + `CardinalityValidator` + `RedMetricKind` + `InMemoryAnalyticsAuditSink` (no drift). Quality gates verde: `cargo test -p corelink-logpush --all-targets` 77 tests 0 failures debug + release; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (279 schema + 6 YAML = 285 docs); `check_migrations_additive.py` clean (16 migrations including new 0016). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-09 corpus. |
| 1.2.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S09-001 SEAL) | **WI-S09-001 SEALED — `crates/corelink-analytics/` v0.1.0 shipped + `migrations/d1/0015_analytics_cardinality_budgets.sql` (NEW tables; per-metric cardinality budget durable mirror + per-metric observed-tuple snapshot ledger; INV-OBS-CARDINALITY-BUDGET enforcement source-of-truth).** New crate ships 7 source modules (~3000 LOC src + ~1100 LOC tests = ~4100 LOC total): `canonical` (`RedMetricKind` `#[non_exhaustive]` 9-RED + 6-USE = 15 canonical metric kinds enum; closed taxonomy at compile-time per WI §9.2 + Lote 10.8bis discipline; canonical Prom-compatible underscore slugs per Lote 10.9bis P0-E + OpenMetrics 1.0 naming convention); `labels` (enum-typed label cartesian — `Tier` re-exported from `corelink-eviction` 5-tier canonical Lote 10.7bis P0-7 vocabulary FROZEN at the data model layer + `Region` 22-canonical CF colocode + `RedResultLabel` Success/4xx/5xx + `AcHitMissLabel` + `GcPhaseLabel` 4-canonical + `RateLimitLayerLabel` 4-canonical + `RateLimitReasonLabel` 6-canonical + `DsrTypeLabel` 5-canonical + `BillingEventTypeLabel` 5-canonical + `R2BucketLabel` 5-canonical + `R2OpTypeLabel` 4-canonical + `KvNamespaceLabel` 5-canonical + `DoClassLabel` 4-canonical + `MetricLabelTuple` enum-typed cartesian struct with NO `String` slot — forbidden labels `trace_id`/`tenant_id`/`request_id`/`blob_digest` not representable by construction + `FORBIDDEN_LABEL_NAMES` const for CI lint secondary defense per WI §1 invariant 2/3 + Lote 10.8bis cardinality discipline absorbed); `config` (`AnalyticsConfig` per-instance F-001 closure + canonical 20k per-metric budget + 100k global budget per WI §1 invariant 1 + sprint contract §5.1 R-S09-2 + canonical 11-boundary OpenMetrics 1.0 histogram bucket boundaries 5ms..10s per Lote 10.9bis P0-I + 80% cardinality_approaching_pct SEV-3 trigger threshold + `with_budgets` constructor for cold-start hydration from durable D1 mirror); `error` (`AnalyticsError` `#[non_exhaustive]` taxonomy with `CardinalityBudgetExceeded` carrying per_metric/global scope discriminator + `Audit` lifting audit fail-closed envelope failures + `Observer` lifting backend transport failures + `Internal` for mutex poisoning); `audit` (`AnalyticsEventType` `#[non_exhaustive]` 3-event canonical taxonomy `corelink.analytics.{metric_emitted, cardinality_rejected, budget_exceeded}` + `AnalyticsAuditRecord` typed shape + `AnalyticsAuditSink` trait + `InMemoryAnalyticsAuditSink` capture + `FailingAnalyticsAuditSink` adversarial fixture; SEV-2 classification on `CardinalityRejected` + `BudgetExceeded` per WI §6.1.11); `observer` (`RedMetricsObserver` trait surface — `record_rate` + `record_error` + `record_duration` + `set_gauge` — RED triple per Google SRE Workbook §6 + gauge for USE; `InMemoryRedMetrics` capture sink with per-instance `Arc<Mutex<>>` HashMap counters + `BucketHistogram` with canonical 11+1 boundaries; `FailingRedMetrics` adversarial fixture; canonical bucket-index helper exposed for property-test assertions); `validator` (the **load-bearing piece**: `CardinalityValidator` enforcing `INV-OBS-CARDINALITY-BUDGET` HIGH per-metric ≤ 20k + global ≤ 100k at the emit boundary; per-metric unique-tuple ledger under per-instance `Arc<Mutex<HashMap<RedMetricKind, HashSet<MetricLabelTuple>>>>` F-001 closure; **audit-emit-BEFORE-mutation fail-closed envelope** per Lote 10.6bis pattern + S-07 sprint-close P1-1 fix on every decision arm — Registered/AlreadyRegistered/CardinalityRejected/BudgetExceeded; idempotent repeat tuples don't inflate count; per-metric + global rejection counters as canonical SEV-2 alert source). Migration `0015_analytics_cardinality_budgets.sql`: 2 tables `analytics_cardinality_budgets` (per-metric budget mirror PK metric_name; CHECK budget_unique_tuples ≥ 1 + ≤ 100000) + `analytics_cardinality_observed` (per-metric observed-tuple snapshot ledger PK metric_name; CHECK observed_unique_tuples ≥ 0); 2 indices including idx_analytics_observed_recent for DASH-COST + idx_analytics_observed_metric for budget-near-violation JOIN; idempotent + additive; canonical schema version 15; column drift no `_ms` suffix per Lote 10.7bis P0-3. Tests: **81 tests across all targets, 0 failures, parallel-safe**: 56 inline lib unit (canonical 7 + labels 7 + config 6 + error 4 + audit 6 + observer 11 + validator 12 + plus impl-level integration tests) + 8 migration_canonical_0015 + 17 prop_analytics tests including the 8 required property tests at 10k iter PR-gate (`PROPTEST_CASES` env var override at runtime per S-07 P1-2 fix): `prop_cardinality_budget_enforced` (HIGH; INV-OBS-CARDINALITY-BUDGET enforcement; accepted ≤ budget invariant), `prop_cardinality_idempotent_repeat_label_set` (repeat tuple count = 1), `prop_red_rate_monotone` (OpenMetrics 1.0 §counter), `prop_red_errors_monotone` (OpenMetrics 1.0 §counter), `prop_red_duration_histogram_bucket_correct` (canonical bucket placement per Lote 10.9bis P0-I), `prop_tenant_isolation` (Tier A label tuples never affect Tier B per-metric ledger; INV-TENANT-ISOLATION canary), `prop_audit_emit_per_decision_arm` (audit count = call count; fail-closed envelope), `prop_idempotent_zero_value_emit` (validator counts unique tuple; observer value=0 no-op). Plus surface-pinning regression tests covering canonical metric names + audit event strings + canonical budget constants + forbidden label names + 22 canonical regions + 5 canonical tiers. **Trait-abstraction-defer per charter**: real CF Workers Analytics Engine binding via `worker::send_future` fire-and-forget + Prom remote write → Grafana Mimir + CI hook `scripts/cardinality_check.py` static validator + 100k nightly + chaos 10 + Tempo exemplar deep-link integration — all consolidated alongside WI-S09-007 PRR ship gate per `trait-abstraction-defer` charter pattern. Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-analytics` member + workspace dep; reuses `corelink-eviction::Tier` (no drift). Quality gates verde: `cargo test -p corelink-analytics --all-targets` 81 tests 0 failures debug + release; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (279 schema + 6 YAML = 285 docs); `check_migrations_additive.py` clean (15 migrations including new 0015). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-09 corpus. |

---

**Fim spec contract S-09 v1.3.0 SOTA** (Upgrade history: v1.1 initial design; v1.2 WI-S09-001 SEAL; v1.3 WI-S09-002 SEAL).
