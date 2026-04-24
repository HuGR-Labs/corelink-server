---
id: "OBSERVABILITY-MODEL"
type: "observability_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-23"
updated: "2026-04-23"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "observability", "metrics", "logs", "traces", "slo"]
---

# Observability Model — Metrics, Logs, Traces, Dashboards, Alerts

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do modelo de observabilidade. Consumido por:
> - `specs/_templates/work_item.md §21` (observability delta do WI)
> - `specs/_templates/sprint_contract.md §14` (observability rollup do sprint)
> - `specs/_templates/production_readiness_review.md §9` (verificação em produção)
>
> Regra: WI/Sprint/PRR que mudem observability **DEVEM** declarar `inherits_from: ["OBSERVABILITY-MODEL"]` e referenciar naming conventions + SLI/SLO daqui; não redefinir.

> **🚦 Phase boundary (F-12 audit Lote 3+4):**
> CoreLink GA inicial cobre **Fase 1 — Remote Cache** (CAS + AC + GC + REAPI cache-only).
> **Fase 2 — Remote Execution** (`execute-action`, executor identity, sandbox runtime) é **futuro** (roadmap pós-GA).
> Seções/CTRLs/SLOs/labels marcados com `(Fase 2)` ou `execute-action` referem-se a planejamento; em GA inicial podem ser omitidos do scope mínimo.



---

## Sumário

1. [Princípios](#1-princípios)
2. [Pilares (Metrics / Logs / Traces / Events)](#2-pilares-metrics--logs--traces--events)
3. [Labels/attributes canônicos](#3-labelsattributes-canônicos)
4. [Métricas — taxonomia e naming](#4-métricas--taxonomia-e-naming)
5. [Logs estruturados — schema](#5-logs-estruturados--schema)
6. [Traces distribuídos — spans e propagação](#6-traces-distribuídos--spans-e-propagação)
7. [Eventos de audit & business](#7-eventos-de-audit--business)
8. [Dashboards — catálogo](#8-dashboards--catálogo)
9. [Alerting — SLO-based + pager discipline](#9-alerting--slo-based--pager-discipline)
10. [PII / secret hygiene em telemetria](#10-pii--secret-hygiene-em-telemetria)
11. [Retention & cardinality budget](#11-retention--cardinality-budget)
12. [Testes de observability](#12-testes-de-observability)
13. [Referências](#13-referências)

---

## 1. Princípios

1. **Instrumented-by-default.** Toda função no hot path deve emitir pelo menos 1 métrica + 1 span. Emissão é mais barata do que debugar um incidente sem dados.
2. **Cardinality com orçamento.** Cada label expande o combinatorial. `tenant_id` é caro; `error_code` é barato. Budget definido por time-series backend (Grafana Cloud / Cloudflare Analytics Engine).
3. **Copiloto de SLO.** Cada SLO tem SLI emitida; cada SLI tem query fixa e dashboards; cada dashboard aponta para alerts; cada alert aponta para runbook. Loop fechado.
4. **Structured > free-text.** Logs são JSON com schema validado. Grep em prod = anti-pattern.
5. **Privacy by default.** `tenant_id` é um anonymous identifier (UUIDv7), não o nome comercial do cliente. PII nunca em labels.
6. **Production parity.** Métricas/logs/traces em dev ≡ prod em shape; volume é que muda. Facilita desenvolvimento e testing.
7. **Tenant-scoped visibility.** Tenant pode ver apenas a própria telemetria via API de self-service; operador vê tudo (com audit de acesso).

---

## 2. Pilares (Metrics / Logs / Traces / Events)

| Pilar    | Ferramenta (CoreLink default)                          | Formato        | Retention base | Custo/cardinalidade |
|----------|--------------------------------------------------------|----------------|-----------------|----------------------|
| Metrics  | Prometheus text via Workers Analytics Engine → Grafana  | OpenMetrics    | 30d high-res / 400d downsampled | Alta cardinalidade cara |
| Logs     | Workers → Logpush → R2 + Grafana Loki                   | JSON-lines      | 90d quente / 7y frio (audit) | Baixa cardinalidade livre |
| Traces   | Workers Tail → OTLP → Grafana Tempo                     | OpenTelemetry   | 30d             | Sampling (default 1%, 100% erros) |
| Events   | R2 bucket `events/` append + Kafka-style fan-out         | CloudEvents 1.0 JSON | 7y          | N/A (cada evento único por design) |

> **Regra de seleção:** quando em dúvida entre métrica e log, pergunte: "quero agregar/alertar?" → métrica. "quero investigar 1 caso?" → log. "quero reconstruir causalidade cross-service?" → trace. "quero billing / audit / legal?" → evento.

---

## 3. Labels/attributes canônicos

Labels **obrigatórias** em TODA métrica/log/trace emitido pelo CoreLink:

| Label            | Tipo    | Valores                             | Custo cardinalidade | Notas                         |
|------------------|---------|-------------------------------------|---------------------|--------------------------------|
| `service`        | string  | enum: `worker-cp`, `container-exec`, `worker-gc`, `worker-billing`, `cli` | baixa | Identifica o componente.       |
| `env`            | string  | enum: `prod`, `staging`, `dev`, `test` | baixa  | Nunca mix.                    |
| `region`         | string  | enum: `wnam`, `enam`, `weur`, `apac`, `sam`, `afr` | média | CF deploy region.     |
| `version`        | string  | semver + commit (`v1.2.3-abc1234`)   | alta (por release)   | Emitir mas high-cardinality na query só sob demanda. |
| `tenant_id`      | string  | UUIDv7                              | **CRITICAL** alta   | Default **disponível** mas subject a sampling/drop em painéis globais. |
| `plan`           | string  | enum: `free`, `team`, `enterprise`  | baixa               | Útil para segmentar impacto.   |

Labels **contextuais** (adicionadas por pilar):

| Label            | Aplica a      | Exemplos                              |
|------------------|---------------|----------------------------------------|
| `op`             | métricas/logs | `cas.get`, `cas.put`, `ac.get`, `ac.put`, `execute.action`, `gc.mark`, `gc.sweep` |
| `outcome`        | métricas/logs | enum: `ok`, `miss`, `denied`, `timeout`, `error_client`, `error_server`, `rate_limited`, `quota_exceeded` |
| `error_code`     | métricas/logs | snake_case; ver §5                     |
| `status_class`   | métricas      | `2xx`, `3xx`, `4xx`, `5xx`             |
| `digest_algo`    | métricas/logs | `blake3`, `sha256`                     |
| `blob_size_bucket`| métricas     | `<1KiB`, `<64KiB`, `<1MiB`, `<16MiB`, `<256MiB`, `<5GiB`, `≥5GiB` |
| `protocol`       | métricas/logs | `reapi_v2`, `s3`, `custom_v1`          |

> **Proibido como label:** `path_full` (cardinalidade infinita), `digest_full` (use `digest_hex_first8` se precisar), nomes de arquivos, IPs (usar `asn` ou `country_code`).

---

## 4. Métricas — taxonomia e naming

### 4.1 Convenção de nomes

```
corelink_<pilar>_<nome>_<unit>[_suffix]
```

- `pilar` ∈ `{ cas, ac, exec, gc, billing, auth, ratelimit, queue, deps }`.
- `unit` ∈ `{ total, bytes, seconds, ratio, count }` (unit em SI, conforme Prom convention).
- `suffix` ∈ `{ _bucket, _count, _sum }` para histogramas (auto Prometheus).

### 4.2 RED por operação (mandatory)

Para toda `op`:

| Métrica                         | Tipo       | Labels                                              | Notas                         |
|---------------------------------|------------|-----------------------------------------------------|--------------------------------|
| `corelink_<op>_requests_total`  | Counter    | todos canônicos + `op`, `outcome`, `status_class`  | Rate                          |
| `corelink_<op>_errors_total`    | Counter    | todos + `op`, `error_code`                         | Errors (subset do anterior)   |
| `corelink_<op>_duration_seconds`| Histogram  | todos + `op`, `outcome`                            | Duration; buckets p50..p999   |

Buckets padrão (seconds): `0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30`. Override em ops lentas (execute-action): `1, 5, 10, 30, 60, 180, 600, 1800, 3600`.

### 4.3 USE para recursos

Para cada recurso com contention:

| Métrica                               | Tipo     | Notas                                  |
|---------------------------------------|----------|-----------------------------------------|
| `corelink_resource_utilization_ratio` | Gauge    | labels `resource` = `r2_pps`, `d1_qps`, `do_storage`, `container_slots`, `kv_ops` |
| `corelink_resource_saturation_ratio`  | Gauge    | `0..1`; threshold de alerta em 0.8      |
| `corelink_resource_errors_total`      | Counter  | por backend                             |

### 4.4 Métricas de produto (business)

| Métrica                                 | Tipo      | Notas                                                |
|------------------------------------------|-----------|------------------------------------------------------|
| `corelink_cache_hit_ratio`              | Gauge     | `ac` e `cas` separados; per-tenant sampled            |
| `corelink_storage_used_bytes`           | Gauge     | per-tenant; emitido 1×/min                           |
| `corelink_bandwidth_egress_bytes_total` | Counter   | split por `protocol`                                 |
| `corelink_dedup_ratio`                  | Gauge     | bytes lógicos / bytes físicos                         |
| `corelink_blob_count`                   | Gauge     | per-tenant                                            |
| `corelink_gc_reclaimed_bytes_total`     | Counter   | labels `tier` (hot/warm/cold)                        |

### 4.5 Métricas de SLO (pré-calculadas)

Para cada SLO em `slo_catalog.md`:

- `corelink_slo_target_ratio{slo}` — gauge, valor estático (ex: 0.999).
- `corelink_slo_good_total{slo}` — counter.
- `corelink_slo_total_total{slo}` — counter (eventos válidos).
- `corelink_slo_burn_rate{slo, window}` — gauge; window ∈ `{5m, 1h, 6h, 24h}`.

Alerting via multi-window multi-burn-rate (Google SRE Workbook §5).

---

## 5. Logs estruturados — schema

### 5.1 Schema base (JSON-lines)

```json
{
  "ts":            "2026-04-23T14:02:33.124Z",
  "level":         "INFO",
  "service":       "worker-cp",
  "env":           "prod",
  "region":        "wnam",
  "version":       "v1.2.3-abc1234",
  "request_id":    "01HKE3...",
  "trace_id":      "0af7651916cd43dd8448eb211c80319c",
  "span_id":       "b7ad6b7169203331",
  "tenant_id":     "01938af0-...",
  "op":            "cas.get",
  "outcome":       "ok",
  "status_code":   200,
  "duration_ms":   12.4,
  "blob_size":     131072,
  "digest_algo":   "blake3",
  "digest_hex8":   "a1b2c3d4",
  "msg":           "cas get served",
  "extra":         { ... } 
}
```

Campos **obrigatórios**: `ts, level, service, env, request_id`. Validação em CI via schema `specs/_schemas/log_event.schema.json` (dep futura tracked em roadmap observability).

### 5.2 Levels

| Level   | Uso                                                                 | Retention |
|---------|---------------------------------------------------------------------|-----------|
| FATAL   | Processo vai terminar                                                | 7y        |
| ERROR   | Falha que afeta user; investigar                                     | 7y        |
| WARN    | Degradação; investigar se padrão                                     | 1y        |
| INFO    | Eventos operacionais (start, stop, config change, deploy)            | 90d       |
| DEBUG   | Detalhes de implementação; apenas em staging/dev                     | 7d        |
| TRACE   | Raríssimo em prod; habilitado per-request via header (kill-switch)    | 7d        |

### 5.3 Error taxonomy (`error_code`)

Conjunto **fechado** e versionado (CI valida novos valores):

```
AUTH_*     = AUTH_INVALID_TOKEN, AUTH_EXPIRED, AUTH_REVOKED, AUTH_WRONG_SCOPE, AUTH_REPLAY
TENANT_*   = TENANT_SUSPENDED, TENANT_QUOTA_EXCEEDED, TENANT_BILLING_BLOCK
CAS_*      = CAS_NOT_FOUND, CAS_HASH_MISMATCH, CAS_TOO_LARGE, CAS_MALFORMED_DIGEST
AC_*       = AC_NOT_FOUND, AC_INVALID_MERKLE, AC_EXPIRED
EXEC_*     = EXEC_TIMEOUT, EXEC_OOM, EXEC_NETWORK_DENIED, EXEC_SANDBOX_VIOLATION
UPSTREAM_* = UPSTREAM_R2, UPSTREAM_D1, UPSTREAM_NEON, UPSTREAM_KV, UPSTREAM_DO, UPSTREAM_CF_EDGE
INPUT_*    = INPUT_INVALID_PATH, INPUT_SCHEMA, INPUT_TOO_LARGE, INPUT_ENCODING
INTERNAL_* = INTERNAL_PANIC, INTERNAL_STATE, INTERNAL_ASSERTION
RATE_*     = RATE_LIMIT_TENANT, RATE_LIMIT_IP, RATE_LIMIT_GLOBAL
```

Adição de novo código = ADR + atualização de alert rules.

---

## 6. Traces distribuídos — spans e propagação

### 6.1 Propagação

- **W3C Trace Context** obrigatório (`traceparent`, `tracestate`).
- `trace_id` gerado no edge (CF Worker entrypoint) e propagado para Container/D1/Neon via headers/metadata.
- Clients (CLI, CI) podem enviar `traceparent`; CoreLink aceita *somente se* request assinada (evita injection).

### 6.2 Spans canônicos

Nomes: `<service>.<op>[.<stage>]`.

| Span                       | Origem          | Attributes obrigatórios                        |
|----------------------------|-----------------|------------------------------------------------|
| `worker-cp.cas.get`        | Worker          | `tenant_id, digest_algo, blob_size, outcome`   |
| `worker-cp.cas.put`        | Worker          | `tenant_id, digest_algo, blob_size, outcome`   |
| `worker-cp.ac.get`         | Worker          | `tenant_id, outcome`                            |
| `worker-cp.authz`          | Worker          | `tenant_id, scope, decision`                    |
| `worker-cp.storage.r2.get` | Worker → R2     | `key_hash8, size, http_status`                  |
| `worker-cp.storage.d1.q`   | Worker → D1     | `query_name, rows`                              |
| `worker-cp.storage.neon.q` | Worker → Neon   | `query_name, rows`                              |
| `container-exec.action`    | Container       | `action_digest_hex8, deadline_s, exit_code`    |
| `worker-gc.mark`           | Worker (GC)     | `tenant_id, scanned, kept`                      |
| `worker-gc.sweep`          | Worker (GC)     | `tenant_id, reclaimed_bytes, reclaimed_objects` |

### 6.3 Sampling

- Default: **1%** head-based sampling.
- **100%** para traces com qualquer span `ERROR`.
- **100%** para `tenant.plan = enterprise` (cliente-pago).
- Per-request override via header `X-CoreLink-Trace: always` (apenas se PAT tem scope `trace-override`).

---

## 7. Eventos de audit & business

### 7.1 CloudEvents schema

```json
{
  "specversion": "1.0",
  "type":        "dev.hugr.corelink.cas.written.v1",
  "source":      "corelink/worker-cp",
  "id":          "01HKE3Z9A...",
  "time":        "2026-04-23T14:02:33Z",
  "subject":     "tenant/01938af0/blob/blake3:a1b2c3d4",
  "datacontenttype": "application/json",
  "data": {
    "tenant_id":   "01938af0-...",
    "actor":       { "kind":"pat", "token_hash":"..." },
    "op":          "cas.put",
    "digest":      "blake3:a1b2c3d4...",
    "size_bytes":  131072,
    "request_id":  "01HKE3..."
  }
}
```

### 7.2 Tipos de evento (catalog)

| Tipo (dev.hugr.corelink.*)       | Uso                                     | Retention |
|-----------------------------------|-----------------------------------------|-----------|
| `cas.written.v1`                  | Billing + audit                         | 7y        |
| `cas.read.v1`                     | Billing apenas (sampled 1%)             | 30d       |
| `cas.deleted.v1`                  | GC audit                                | 7y        |
| `ac.written.v1`                   | Audit                                   | 7y        |
| `ac.invalidated.v1`               | Audit                                   | 7y        |
| `execute.completed.v1`            | Billing + audit                         | 7y        |
| `tenant.created.v1`               | Audit                                   | 7y (legal) |
| `tenant.suspended.v1`             | Audit                                   | 7y (legal) |
| `pat.issued.v1`                   | Audit                                   | 7y        |
| `pat.revoked.v1`                  | Audit                                   | 7y        |
| `admin.config.changed.v1`         | Audit                                   | 7y        |
| `policy.waiver.granted.v1`        | Governança                              | 7y        |

Events cross-link com logs via `request_id` e com traces via `trace_id` (propagados em attributes).

---

## 8. Dashboards — catálogo

Dashboards **DEVEM** existir como código (Grafana JSON em `observability/dashboards/`), PR-reviewed.

| ID                     | Nome                           | Audiência         | Top widgets                                                 |
|------------------------|--------------------------------|-------------------|-------------------------------------------------------------|
| `DASH-GLOBAL-HEALTH`   | Global Health                  | SRE on-call       | Uptime, availability SLO burn, error rate, p99 latency      |
| `DASH-GLOBAL-PRODUCT`  | Business Pulse                 | Product + Exec    | Tenants ativos, cache hit ratio, storage used, MRR proxy    |
| `DASH-TENANT`          | Tenant deep-dive               | Support / SRE     | Hit ratio, latência, quota, errors top codes                |
| `DASH-CAS`             | CAS operações                  | Dev CAS           | RPS, latência p50/p99, misses, size distribution, dedup     |
| `DASH-AC`              | Action Cache operações          | Dev AC            | RPS, hit ratio, Merkle failures, size, age                 |
| `DASH-EXEC`            | Execute Action                 | Dev exec / SRE    | Active slots, queue depth, durations, exit codes            |
| `DASH-GC`              | Garbage Collection             | SRE               | Mark/sweep latency, reclaimed bytes, refcount distribution, errors |
| `DASH-SUPPLY-CHAIN`    | Supply chain                    | Security Lead     | SBOM drift, vuln counts, release verify status              |
| `DASH-SECURITY`        | Security posture                | Security Lead     | AuthN failures, anomalous tenant behavior, abuse signals    |
| `DASH-PRIVACY`         | Privacy controls                | Privacy Officer    | DSR requests, erasure SLA, data residency exceptions        |
| `DASH-COST`            | Cost per tenant                 | Finance / Eng Mgr  | R2 $$, Workers CPU-ms, Containers minutes, Neon vCPUh       |
| `DASH-SLO-CATALOG`     | Todos SLOs                      | SRE + Product     | Per-SLO burn-rate; links para runbooks                      |

Convenção de URL: `grafana/d/corelink-<id-em-lowercase>`.

---

## 9. Alerting — SLO-based + pager discipline

### 9.1 Alert as code

- Definidos em `observability/alerts/*.yaml` (Prometheus / Grafana / Alertmanager).
- PR-reviewed; CI valida syntax + unit test via `promtool test rules`.
- Campos obrigatórios: `name`, `severity`, `slo` OR `rationale`, `runbook_url`, `owner_team`.

### 9.2 Severities (com clock semantics — F-11)

> **Convenção de clock para Response SLA (audit Lote 3+4 F-11):**
> - `clock_start` = momento do **alert fire** (PagerDuty/Slack timestamp do trigger automático). Não usa "first human view".
> - `clock_stop` = `human_acknowledged_at` (oncall confirma `/ack` no PagerDuty ou reage no Slack thread).
> - **Mitigation SLA** (separado): `clock_start = ack_time`, `clock_stop = mitigation_complete_announced` em status page ou Slack.
> - **Calendário**: 24/7 para SEV-1/SEV-2; horário comercial (9h-18h local da região oncall) para SEV-3/SEV-4.

| Severity | Pager?          | Response SLA (alert→ack) | Mitigation SLA (ack→mitigated) | Exemplo |
|----------|------------------|---------------------------|---------------------------------|---------|
| SEV-1    | Yes, PagerDuty  | 5 min (24/7)              | 30 min                          | Availability SLO 5m fast-burn > 14.4; cross-tenant breach |
| SEV-2    | Yes, PagerDuty  | 30 min (24/7)             | 4 horas                         | SLO 1h burn > 6; backup falhou |
| SEV-3    | Slack `#oncall` | 1 business day            | 5 business days                 | Saturation > 0.8; vuln HIGH em dep |
| SEV-4    | Ticket           | 5 business days           | 30 business days                | Drift de métricas; docs faltando |

### 9.3 Multi-window multi-burn-rate (SRE Workbook)

Para SLO 99.9%:

| Window  | Threshold (consumir X% do budget) | Severity | Alert suffix |
|---------|-----------------------------------|-----------|---------------|
| 5 min    | 2% (page fast)                    | SEV-1     | `-fast`       |
| 1 h      | 5% (page slow)                    | SEV-1     | `-slow`       |
| 6 h      | 10%                               | SEV-2     | `-medium`     |
| 24 h     | 50%                               | SEV-3     | `-ticket`     |

Regra: alert SEV-1 requer confirmação com janela 5m AND 1h simultaneamente (reduz flapping).

### 9.4 Pager discipline

- **Actionable:** todo alert tem runbook com primeiras 3 ações. Se um alert disparou 3× sem ação → refator ou supress.
- **No noise policy:** flapping > 2× por semana → freeze do serviço + PR obrigatório para ajustar threshold ou root-cause.
- **Oncall fatigue budget:** ≥ 2 SEV-1 por oncall shift de 12h → post-mortem obrigatório.

---

## 10. PII / secret hygiene em telemetria

Coordenado com `privacy_model.md` + CTRL-PRIV-001 (security_model §6.10).

### 10.1 Proibido em metrics/logs/traces/events

- Email, nome, telefone, endereço, CPF/CNPJ.
- Conteúdo do blob (body).
- Path completo contendo nome de arquivo.
- Token PAT (nem prefix).
- `Authorization:` header.
- `tenant_name` (apenas `tenant_id` UUID).

### 10.2 Permitido mas sampled/truncado

- `digest_hex` → expor apenas `digest_hex8` (primeiros 8 chars).
- `user_agent` → top-20 enumerado; resto bucket `other`.
- `ip` → derivar apenas `country_code` e `asn`.

### 10.3 Enforcement

- Linter CI `log-schema` valida schema antes do merge.
- SAST grep por padrões suspeitos: `email|cpf|cnpj|ssn|authorization\s*:`.
- Red team trimestral tenta encontrar PII em amostras de 10k logs.

---

## 11. Retention & cardinality budget

### 11.1 Retention

| Tier       | Backend                  | Pilares            | Retention default   |
|------------|--------------------------|--------------------|----------------------|
| Hot        | Grafana Cloud (Prom/Loki/Tempo) | Metrics, logs, traces | 30d          |
| Warm       | R2 via Logpush            | Logs               | 90d                  |
| Cold (audit) | R2 Object Lock         | Events (audit)     | 7 anos (SOC 2)       |
| Cold (ops) | R2                        | Logs INFO          | 400d                 |

### 11.2 Cardinality budget

| Dimension          | Limit                      | Notas                                                            |
|--------------------|---------------------------|-------------------------------------------------------------------|
| `tenant_id`        | 100k (provisionado)        | Sampled em dashboards globais; scopeados em tenant-deepdive      |
| `op`               | 30                        | Enum fechado; PR valida                                          |
| `error_code`       | 200                       | Enum fechado; ADR para adicionar                                 |
| `user_agent`       | 20 (top) + `other`        | Agregador determinístico                                          |
| `region × env × service` | 4 × 4 × 5 = 80         | Baixo                                                             |

Alerta se cardinality > 2× budget esperado → investigar cardinality explosion antes de aumentar budget.

---

## 12. Testes de observability

| Teste                                       | Frequência  | Evidence                    |
|---------------------------------------------|-------------|------------------------------|
| Smoke test de emissão (métrica/log/trace)   | CI every PR | EVT-002   |
| Schema validation de logs (unit)            | CI every PR | EVT-026       |
| Alert rule syntax (`promtool test rules`)   | CI every PR | EVT-001                  |
| Dashboard-as-code lint (grafonnet/jsonnet)  | CI every PR | EVT-001                  |
| Cardinality budget check                     | Nightly     | EVT-013      |
| Synthetic uptime canary                      | Contínuo    | EVT-031        |
| Runbook drill (oncall simulation)            | Mensal      | EVT-017      |
| Chaos test: inject latency e verificar SLO alert disparou em X | Mensal (staging) | EVT-023 |

---

## 13. Referências

- **Google SRE Book + Workbook** — especialmente capítulos de SLO e alerting multi-window.
- **OpenTelemetry** semantic conventions.
- **Prometheus** naming best practices.
- **CloudEvents** v1.0.
- **Grafana Labs LGTM stack** (Loki, Grafana, Tempo, Mimir).
- **Honeycomb** — inspiração para high-cardinality por request_id.
- **Datadog USE + RED methods**.

### 13.1 Concorrentes

- **BuildBuddy** — dashboard forte de cache hit + invocations + flaky tests. Referência visual.
- **NativeLink** — Prometheus metrics integradas; docs fracas em alerting.
- **JFrog Artifactory** — Xray + Insight; telemetria de vulnerabilidades integrada.

---

**Fim de OBSERVABILITY-MODEL.** Mudanças de schema de log/evento requerem ADR com bump minor mínimo (consumidores downstream quebram silenciosamente).
