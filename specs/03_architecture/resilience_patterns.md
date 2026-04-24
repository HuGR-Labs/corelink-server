---
id: "RESILIENCE-PATTERNS"
type: "resilience_patterns"
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
tags: ["architecture", "reliability", "patterns", "retry", "circuit-breaker"]
---

# Resilience Patterns — Catálogo Canônico (PAT-XXX)

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** *(a definir — SRE Lead, Architect, Reliability Engineer)*
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) dos patterns de resiliência usados no CoreLink. Consumido por:
> - `failure_modes.md` (cada FM referencia PAT aplicável)
> - `specs/_templates/work_item.md §25` (rollback + PAT específico)
> - `specs/_templates/sprint_contract.md §15` (failure scenarios + mitigação)
>
> Regra: novos patterns **DEVEM** ter ADR + entry aqui + associação a pelo menos 1 FM; "novo pattern ad-hoc no WI" é smell.

---

## Sumário

1. [Princípios](#1-princípios)
2. [Taxonomia](#2-taxonomia)
3. [Catálogo PAT-XXX](#3-catálogo-pat-xxx)
4. [Anti-patterns (proibidos sem ADR)](#4-anti-patterns-proibidos-sem-adr)
5. [Teste de patterns](#5-teste-de-patterns)
6. [Referências](#6-referências)

---

## 1. Princípios

1. **Assume falha.** Toda chamada cruzando boundary (rede, processo, storage) falha ocasionalmente. Código deve ter política explícita: retry? fail fast? degrade? fallback?
2. **Budget over best-effort.** Retries, timeouts, backoffs têm orçamento fixo configurado. Sem "until success" loops.
3. **Isolation over sharing.** Pools separados por tenant/feature (bulkheads).
4. **Measure before tuning.** Toda política tem métrica associada — nunca tunar sem dados.
5. **Idempotency first.** Antes de retry, idempotency. Sem idempotency, retry é corruption risk.
6. **Graceful degradation.** Quando tudo falha, preferir **servir 503** do que servir resposta incorreta.
7. **Observable under failure.** Patterns emitem métricas próprias (`corelink_pattern_<name>_*`).

---

## 2. Taxonomia

| Classe         | Propósito                                              |
|----------------|--------------------------------------------------------|
| **Time-bound** | Limitar latência (timeout, deadline, budget)            |
| **Retry**      | Lidar com falhas transientes (retry, backoff, jitter)   |
| **Isolation**  | Conter blast radius (bulkhead, circuit breaker, quota)  |
| **Redundancy** | Evitar SPOF (replica, failover, quorum)                 |
| **Graceful**   | Degradação elegante (fallback, cached, degrade mode)   |
| **Integrity**  | Preservar correctness (soft-delete, reconciliation, idempotency) |
| **Operational**| Ops safety (progressive rollout, dual-approval, drift) |
| **Observability** | Facilitar diagnose (structured error, correlation, trace propagation) |

---

## 3. Catálogo PAT-XXX

### 3.1 Time-bound

#### PAT-TIMEOUT-001 — Request timeout com budget hierárquico
- **Problema:** chamadas pendentes acumulam latência e esgotam recursos.
- **Solução:** cada boundary define timeout; timeouts filhos < timeout do pai.
- **Invariante:** `timeout(child) ≤ timeout(parent) - observed_latency_margin`.
- **Config:**
  - Worker-CP total handler: **5s p99**
  - R2 single op: **2s**
  - D1 query: **1s**
  - Neon query: **3s**
  - KV get: **200ms**
  - Container execute-action: configurável pelo cliente (cap **3600s**)
- **Evidence:** EVT-LOAD_TEST (distributions) + EVT-UNIT_TEST_PASS (enforcement)
- **FMs mitigados:** FM-001, FM-004, FM-055, FM-057

#### PAT-BUDGET-001 — CPU budget per-request
- **Problema:** CF Workers mata isolate em > 50ms CPU default.
- **Solução:** CPU accounting interno; branches "cold" dispatcham pra async job.
- **Métrica:** `corelink_handler_cpu_ms_bucket` (histogram).
- **FMs:** FM-001.

#### PAT-DEADLINE-PROP-001 — Propagação de deadline
- **Problema:** componente downstream não sabe quanto tempo o caller ainda tem.
- **Solução:** header `X-CoreLink-Deadline: <unix_ms>`; downstream cancela se deadline ultrapassado.
- **FMs:** FM-001, FM-055.

### 3.2 Retry

#### PAT-RETRY-001 — Retry idempotente com backoff exponencial + jitter
- **Problema:** falhas transientes em storage/rede.
- **Solução:** retry se *e somente se* op é idempotente; exp backoff com full jitter (AWS paper).
- **Config:** `base=50ms, factor=2.0, max=5s, max_attempts=3, jitter=full`.
- **Proibido:** retry em `error_code IN (AUTH_*, INPUT_*, TENANT_*)`.
- **Métrica:** `corelink_retry_attempts_total{op, outcome}`.
- **FMs:** FM-003, FM-053, FM-104, FM-150.

#### PAT-RETRY-BACKOFF-001 — Backoff com ceiling adaptativo
- **Solução:** se o mesmo error_code domina na última janela, aumentar ceiling (até 60s).
- **FMs:** FM-053.

#### PAT-HEDGED-REQUEST-001 — Hedged request (opcional, cauteloso)
- **Problema:** cauda longa p99 em storage reads.
- **Solução:** após p95 do baseline, emitir segunda request; primeira que responder vence.
- **Custo:** 5% de duplicação; apenas em reads GET idempotentes; desabilitado por padrão.
- **FMs:** FM-055, FM-057.

### 3.3 Isolation

#### PAT-CIRCUIT-001 — Circuit breaker
- **Problema:** downstream lento causa pile-up upstream (retry storm).
- **Solução:** três estados (closed/open/half-open); trip em error_rate > 50% sobre últimas N requests (min=20).
- **Recovery:** probe em intervalo crescente.
- **Métrica:** `corelink_circuit_state{target}` (gauge 0/1/2).
- **FMs:** FM-057, FM-400.

#### PAT-BULKHEAD-001 — Bulkhead / pool por target
- **Problema:** um target lento exaure pool global.
- **Solução:** pools separados: `{r2-cas, r2-ac, r2-audit, d1, neon, kv, do}`. Cada um com max_inflight e queue_depth.
- **FMs:** FM-058.

#### PAT-RATE-LIMIT-001 — Rate limit multi-camada
- **Problema:** single tenant ou IP degrada o sistema.
- **Solução:** token bucket camadas: global, per-tenant, per-IP, per-PAT.
- **Implementação:** DO singleton por tenant; edge CF rules para IP.
- **Referência:** CTRL-RATE-001 (security).
- **FMs:** FM-250, FM-251.

#### PAT-QUOTA-ALERT-001 — Quota enforcement + alert antes do hard limit
- **Problema:** quota exceda silenciosamente.
- **Solução:** alert em 80% + email ao tenant; hard block em 100% + graceful error.
- **FMs:** FM-059, FM-255.

### 3.4 Redundancy

#### PAT-REGION-FAILOVER-001 — Region failover para reads (cached)
- **Problema:** região R2 down; reads em blobs populares podem servir de replica.
- **Solução:** replica on-demand de blobs "hot" (top 1%) em 2a região; read fallback se primária erra.
- **Constraint:** apenas reads; writes continuam na primária (pinning por tenant).
- **FMs:** FM-050, FM-101.

#### PAT-DO-MIGRATION-AWARE-001 — DO migration-aware state
- **Problema:** DOs podem migrar entre datacenters; latência spike.
- **Solução:** state do DO é eventual; writes críticos persistem em D1; DO é cache.
- **FMs:** FM-005.

### 3.5 Graceful

#### PAT-DEGRADE-001 — Degrade mode global
- **Problema:** parcial outage; total degradação > falha catastrófica.
- **Solução:** flag global `degrade_mode={none, read-only, cache-only, emergency}`:
  - `read-only`: writes retornam 503 com Retry-After; reads continuam.
  - `cache-only`: GC pausado; dedup tenant-local forçado; evictions pausadas.
  - `emergency`: apenas health endpoint; tudo mais 503.
- **Controle:** admin API dual-approval.
- **FMs:** FM-050, FM-057, FM-101, FM-105.

#### PAT-FALLBACK-001 — Fallback explícito por op
- **Problema:** dep quebrada; resposta "undefined" é pior.
- **Solução:** cada op define fallback declarativo: `{ on_error: { AUTH_UPSTREAM: continue_with_stale_cache_ttl_60s, UPSTREAM_D1: fail_503, ... } }`.
- **Métrica:** `corelink_fallback_used_total{op, fallback}`.
- **FMs:** FM-056, FM-152, FM-153.

#### PAT-STALE-OK-001 — Stale-while-revalidate (cache)
- **Problema:** upstream lento ocasional.
- **Solução:** TTL lógico + `stale_ttl`; entre TTL e stale_ttl, servir stale e disparar async refresh.
- **Aplicável:** metadata (não no CAS body).
- **FMs:** FM-352.

### 3.6 Integrity

#### PAT-SOFT-DELETE-001 — Soft-delete + grace period
- **Problema:** delete irreversível propaga bugs.
- **Solução:** todo delete primeiro marca `deleted_at`; sweep depois de grace (24h default; CAS 72h).
- **Reversível em incidente.**
- **FMs:** FM-300, FM-205.

#### PAT-RECONCILE-001 — Reconciliation jobs
- **Problema:** drift entre counters (billing, storage used, ref count).
- **Solução:** batch job diário que reconstrói counters de eventos e compara com running counter; alerta se drift > threshold.
- **Métrica:** `corelink_reconcile_drift{counter}`.
- **FMs:** FM-302, FM-305.

#### PAT-IDEMPOTENCY-001 — Idempotency keys
- **Problema:** retry de write não-idempotente corrompe.
- **Solução:** clients enviam `Idempotency-Key`; servidor dedupe em janela 24h.
- **FMs:** FM-003, FM-301.

#### PAT-AUDIT-VERIFY-001 — Audit chain verification
- **Problema:** corrupção silenciosa de audit log.
- **Solução:** hash chain; job diário verifica continuidade; alert em break.
- **FMs:** FM-304.

#### PAT-SCRUB-001 — Content scrub periódico
- **Problema:** bit rot silencioso em R2.
- **Solução:** job varre blobs aleatórios, recomputa hash, compara; suspicious → isolate + alerta.
- **FMs:** FM-051.

### 3.7 Operational

#### PAT-PROGRESSIVE-ROLLOUT-001 — Progressive rollout com auto-rollback
- **Problema:** mau release atinge todos simultaneamente.
- **Solução:** deploy em waves (1% → 10% → 50% → 100%); auto-rollback em error budget burn > 3× baseline por 10 min.
- **Controle:** CF gradual deploy + monitoring hooks.
- **FMs:** FM-200.

#### PAT-DUAL-APPROVAL-001 — Dual approval para ops destructive
- **Problema:** erro humano catastrófico.
- **Solução:** CLI/UI obriga 2 aprovações (pessoas distintas) para: delete, config crypto, quota change, retention reduce.
- **FMs:** FM-201, FM-205.

#### PAT-DRIFT-DETECTION-001 — Infra drift detection
- **Problema:** estado real ≠ infraestrutura declarada.
- **Solução:** `terraform plan` diário em pipeline; alerta em diffs não esperados.
- **FMs:** FM-206.

#### PAT-ROLL-FORWARD-001 — Roll-forward deploy para secret rotation
- **Problema:** rotação quebra em-flight.
- **Solução:** overlap period (old + new válido por 24h); novos tokens assinados com new; old invalidado após overlap.
- **FMs:** FM-204.

#### PAT-MIGRATION-IDEM-001 — Online migrations idempotentes
- **Problema:** migration reapplied corrompe schema.
- **Solução:** migrations via `sqlx migrate` com `CREATE IF NOT EXISTS` / `ALTER ... IF NOT`; tracking table; lockfile.
- **FMs:** FM-056, FM-301.

### 3.8 Emergent control

#### PAT-SINGLEFLIGHT-001 — Single-flight (dedupe in-flight)
- **Problema:** thundering herd em cache miss.
- **Solução:** lib `singleflight`; segundo caller aguarda resultado do primeiro.
- **FMs:** FM-401.

#### PAT-JITTER-001 — Jitter em timers
- **Problema:** sincronização causa spikes.
- **Solução:** todo timer periódico recebe jitter ±10%.
- **FMs:** FM-352, FM-400.

#### PAT-RESTART-JIT-001 — Periodic restart para leaks
- **Problema:** leak latente em long-running Containers.
- **Solução:** kill + respawn após X requests ou Y horas; jitter para não sincronizar.
- **FMs:** FM-403.

#### PAT-DAMPENING-001 — Alert dampening
- **Problema:** feedback loop alert→action→alert.
- **Solução:** squelch window após action automática; alerta operador se squelch expira sem resolução.
- **FMs:** FM-402.

### 3.9 Observability helpers

#### PAT-CORRELATION-ID-001 — Request ID propagation
- Todo request ganha ULID `request_id`; propagado em logs, traces, events, respostas de erro.
- **FMs:** todos P0/P1 (facilita debugging).

#### PAT-STRUCTURED-ERROR-001 — Error envelope
- Todo erro retornado ao cliente: `{ error: { code, message, hint, request_id, documentation_url } }`.
- `code` ∈ error_code enum (§5.3 obs).

---

## 4. Anti-patterns (proibidos sem ADR)

| Anti-pattern                                 | Por que proibido                                  |
|----------------------------------------------|---------------------------------------------------|
| Retry sem backoff ou jitter                  | Retry storm + downstream DoS (FM-400)             |
| Retry em op não-idempotente                  | Corrupção (FM-301)                                 |
| Timeout infinito                              | Resource exhaustion                                |
| Circuit breaker sem recovery                  | Permanent outage                                   |
| Bulkhead global único                         | Single resource contention → DoS                  |
| "Catch-all" que esconde erro                  | Silenciador (FM-006)                               |
| Cache sem TTL                                 | Stale indefinido; inconsistência                   |
| Delete hard sem grace                         | Catastrófico em bug (FM-300)                      |
| Manual secret rotation sem overlap            | Downtime (FM-204)                                  |
| Unbounded queue                               | Memory leak + HoL                                  |
| Best-effort background job sem observability  | Drift silencioso                                   |

---

## 5. Teste de patterns

Cada pattern **DEVE** ter test suite dedicada:

| Pattern class      | Teste                                              | Evidence                         |
|--------------------|----------------------------------------------------|----------------------------------|
| Timeout            | Slow-mock injection; assert kill < cap            | EVT-UNIT_TEST_PASS              |
| Retry              | Flaky-mock; assert max_attempts respected          | EVT-UNIT_TEST_PASS              |
| Circuit breaker    | Error-rate injection; assert state transitions     | EVT-UNIT_TEST_PASS              |
| Bulkhead           | Saturation injection; assert isolation             | EVT-LOAD_TEST                   |
| Rate limit         | Load test > limit; assert shaping                  | EVT-LOAD_TEST                   |
| Graceful degrade   | Chaos test; assert 503s with Retry-After           | EVT-CHAOS_REPORT                |
| Soft-delete        | Delete + undelete in grace window                   | EVT-INTEGRATION_TEST_PASS       |
| Reconciliation     | Drift injection; assert job catches                | EVT-INTEGRATION_TEST_PASS       |
| Progressive rollout | Error budget burn simulation; assert rollback     | EVT-LOAD_TEST                   |

Chaos engineering: runs semanais em staging (FM-400..404 como cenários).

---

## 6. Referências

- **Netflix OSS** — Hystrix (legacy), Resilience4j conceptual.
- **Google SRE** — *Handling Overload*, *Addressing Cascading Failures*.
- **AWS** — *Exponential Backoff and Jitter* (paper clássico).
- **Michael Nygard** — *Release It!* (stability patterns).
- **Marc Brooker** — blog posts sobre cell-based architecture.
- **Cloudflare** — *Reliability patterns on Workers*.

### 6.1 Concorrentes

- **NativeLink** — retry simples; sem circuit breaker formal.
- **BuildBuddy** — progressive rollout maduro; SLO-driven alerts.
- **JFrog** — HA modes com master-master; failover doc formal.

---

**Fim de RESILIENCE-PATTERNS.** Novos PAT-XXX requerem ADR + pelo menos 1 FM-ID associado + test obrigatório em §5.
