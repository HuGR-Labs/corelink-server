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
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
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
- **Evidence:** EVT-024 (distributions) + EVT-002 (enforcement)
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

#### PAT-RETRY-IDEMPOTENT-001 — Retry regulatory-grade fail-CLOSED (Lote 10.11.0-bis)
- **Problema:** PAT-RETRY-001 é genérico (transient errors em hot path); regulatory pipelines (DSR erasure, audit append, breach notification) exigem **fail-CLOSED** com cross-backend ack idempotency, dead-letter quarantine e on-call paging em retry exhaustion. Aplicar PAT-RETRY-001 em pipeline regulatório arrisca silent loss.
- **Solução:** especialização de PAT-RETRY-001 com:
  - **Idempotency key obrigatória** (`Idempotency-Key: dsr-<ticket_id>-<step>` per backend ack); replay safe via dedup table TTL 7d.
  - **Dead-letter queue** após `max_attempts` exausto (default 5; vs PAT-RETRY-001 default 3): worker move job para `dlq.<pipeline>`, on-call page acionada em ≤ 5 min.
  - **Cross-backend coordination:** ack de N backends é tracked em D1 `dsr_backend_acks(ticket_id, backend, ack_at, attempt_n)`; só transição `in_progress → completed` quando todos N ack'd.
  - **Fail-CLOSED audit:** failure_reason persisted em `dsr_tickets.failure_reason`; audit chain entry `dsr.failed.v1` antes do `dsr.completed.v1` ser permitido.
  - **Backoff:** `base=200ms, factor=2.0, max=30s, max_attempts=5, jitter=full` (mais conservador que PAT-RETRY-001).
  - **Proibido:** retry em `error_code IN (LEGAL_HOLD_*, JURISDICTION_*, FRAUD_CHECK_*, CONSENT_LAPSED_*)`.
- **Aplicabilidade:** S-11 DSR pipeline (12 backends), S-09 audit append, S-09 SIEM alert delivery, S-11 sub-processor broadcast.
- **Métrica:** `corelink_pipeline_retry_attempts_total{pipeline, op, outcome}`, `corelink_pipeline_dlq_depth{pipeline}`.
- **FMs:** FM-003, FM-053, FM-450, FM-451, FM-452, FM-453.
- **TLA+ obrigatório:** spec `dsr_erasure_atomicity` (S-11 WI-S11-008) prova `INV-DATA-ERASURE-COMPLETE` sob retry.

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

#### PAT-ROUTING-PINNED-001 — Region-pinned routing fail-CLOSED (Lote 10.11.0-bis)
- **Problema:** custom domain ou misconfigured DNS pode rotear request para região errada (residency violation INV-DATA-RESIDENCY); fallback "passthrough" cria fail-OPEN privacy leak. Endereça GPT P1-7 round-1.
- **Solução:** edge router resolve `tenant.primary_region` via KV cache (TTL 5min) ANTES de qualquer storage op; mismatch entre região do PoP e `primary_region`:
  - **Default:** redirect 307 com `Location:` apontando para custom domain regional (custo: extra hop ≤ 50ms).
  - **Fail-CLOSED policy (regulatory):** se redirect indisponível ou tenant tem `residency_strict=true`, retorna 451 `legal_residency_violation` com audit log entry; NUNCA passthrough silencioso.
  - **No "break-glass" passthrough flag:** flag `residency_strict` é write-once-true (não revertível pra false sem dual approval + ANPD/EDPB notification).
- **Aplicabilidade:** S-11 WI-S11-007 residency E2E + custom domain routing.
- **Métrica:** `corelink_routing_misroute_total{tenant_region, edge_pop, action}` (action ∈ {redirected, blocked}).
- **TLA+:** spec property test ≥ 20k cases (WI-S11-007) prova `\A r \in Region : write(b, r) ⇒ stored(b) \in r`.
- **FMs:** FM-100, FM-451.

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

#### PAT-FORMAL-VERIFICATION-001 — TLA+ obrigatório para HIGH_RISK invariants (Lote 10.11.0-bis)
- **Problema:** invariants CRITICAL (audit append-only, DSR erasure atomicity, residency pinning, consent proof) não podem depender só de testes unit/integration — race conditions sob concorrência multi-region escapam fuzzing/property tests sem state-space exhaustion. Falhas silentes = regulatory exposure.
- **Solução:** invariants marcados `severity=CRITICAL` em `invariant_registry.md` MUST ter spec TLA+ executável:
  - **Bootstrap:** TLC v1.8.0 SHA-256 pinned (ADR-0042 §A1) via `scripts/run_tlc_corelink.sh` wrapper (mandatory SHA pin verify; opt-out via `TLC_SHA256_SKIP=1` apenas local dev). Lote 10.11.0-bis-prime cycle 2: scripts canonical em `scripts/run_tlc_corelink.sh` (NOT `specs/tla/bootstrap.sh` legado).
  - **CI gate:** `.github/workflows/tla_check.yml` roda em PR que toca `specs/tla/**`; invoca runner per spec; falha = block merge. Lote 10.11.0-bis-prime cycle 2 canonical (NOT `tla-check` workflow legado).
  - **Spec quality:** Init + Next + Spec=Init /\ [][Next]_vars /\ Fairness; CONSTANTS declarados; helpers documentados; bounded state space (TypeOK constraint); SHA-256 pin.
  - **Invariants enforced:** `THEOREM Spec => []TypeOK /\ []InvX` para cada invariant CRITICAL ligado.
  - **Fairness conditions:** WF/SF explicit; deadlock check (`tlc -deadlock`).
  - **Liveness:** properties como `<>completed` para temporal claims (DSR ≤ 24h, breach ≤ 72h).
  - **No tautologies:** banido `\/ TRUE`, `\A x : TRUE`, etc. (Lote 10.11.0 finding).
- **Aplicabilidade:** 5 specs canonical declaradas em `specs/tla/README.md` (tenant_isolation, gc_correctness, cas_integrity, audit_immutability, dsr_erasure_atomicity); +N specs novas a partir de S-12.
- **FMs:** apply em invariants — não em FMs específicos (cross-cutting governance pattern).
- **Evidence:** EVT-022 (TLA+ result + commit SHA + TLC version + state count).

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

#### PAT-AUTHZ-001 — Dupla assertion de tenant_id em storage calls
- **Problema:** confused-deputy — Worker chama storage em nome do tenant errado.
- **Solução:** toda storage call recebe `tenant_id` explícito como parâmetro *e* executa assertion `assert_eq!(computed_hmac_prefix, expected_hmac_prefix)` antes de emitir a request.
- **FMs mitigados:** FM-253, FM-303, FM-E-006.
- **Evidence:** EVT-022 (modelo TLA+ de INV-TENANT-ISOLATION cobre) + EVT-002 (property test).

### 3.10 Patterns adicionais (Lote 5.4 — preenchimento de dangling refs do audit)

Esses patterns completam gaps identificados em `failure_modes.md` e `slo_catalog.md` que referenciavam PAT-XXX ainda não catalogados.

#### PAT-ABUSE-DETECT-001 — Detecção de abuse comportamental
- **Problema:** Tenant pago abusa exec-action para criptominer, ou bot scraper mascara-se como CI legítimo.
- **Solução:** heurísticas em stream: (a) CPU/wallclock ratio > threshold sustained; (b) egress bytes / exec minutes fora de distribuição; (c) action_digest entropy baixa + alta frequência. Dispara downgrade silencioso + alert ops.
- **Métrica:** `corelink_abuse_score{tenant_id}`.
- **FMs mitigados:** FM-255.
- **Evidence:** EVT-013 + EVT-017 (runbook para triage).

#### PAT-BACKOFF-001 — Backoff exponencial + jitter genérico
- **Problema:** CF API / downstream throttled intermitentemente.
- **Solução:** lib interna `backoff::jittered`. Config default `base=100ms, factor=2.0, max=10s, attempts=5`. Generalização de PAT-RETRY-001 para não-idempotent-retry APIs.
- **FMs mitigados:** FM-150.
- **Evidence:** EVT-002 (convergence test).

#### PAT-DNS-TTL-001 — DNS TTL conservador + monitor
- **Problema:** DNS outage em registrar externo derruba resolução mesmo com CF infra up.
- **Solução:** TTL de records críticos = 300s (não-default 86400s); CF DNS com secondary registrar; synthetic monitor resolvem de múltiplas regiões.
- **FMs mitigados:** FM-100.
- **Evidence:** EVT-031 (synthetic monitor).

#### PAT-ERROR-ISOLATE-001 — Panic/error isolation no Worker
- **Problema:** panic em Rust (sem catch_unwind) derruba isolate inteiro (N requests em batch).
- **Solução:** envelopar hot path em `std::panic::catch_unwind` (equivalente WASM); converter panic → 500 com request_id; emit `corelink_panic_total` counter; non-zero counter vira alert.
- **FMs mitigados:** FM-006.
- **Evidence:** EVT-002 (panic injection test) + EVT-013.

#### PAT-GC-HEALTHCHECK-001 — GC scheduler health
- **Problema:** GC silenciosamente para → tombstones acumulam → storage infla.
- **Solução:** GC emite heartbeat em cada phase start/end em métrica `corelink_gc_last_run_seconds`; alert se > 36h (janela de 1.5× do período target de 24h).
- **FMs mitigados:** FM-305.
- **Evidence:** EVT-013 + EVT-017.

#### PAT-INPUT-HARDEN-001 — Input hardening agressivo
- **Problema:** deserialization RCE, path traversal, malformed digest.
- **Solução:** (a) serde com `deny_unknown_fields` + tipos explícitos; (b) path canonicalization rejeita `..`, `\0`, UTF-8 inválido; (c) digest regex `^[a-f0-9]{64}$` antes de qualquer parse.
- **CTRLs associados:** CTRL-INPUT-001..004 (security_model §6.5).
- **FMs mitigados:** FM-007.
- **Evidence:** EVT-005 + EVT-008 (fuzz targets específicos em parsers).

#### PAT-KV-TTL-001 — KV com TTL curto + never-source-of-truth
- **Problema:** KV global eventual consistency > 60s causa leitura stale.
- **Solução:** TTL máximo 300s em KV; nenhum dado que não tolere 60s de stale fica em KV; D1/R2 são source of truth; KV só caches derivados.
- **FMs mitigados:** FM-054.
- **Evidence:** EVT-026 (schema validation rejeita uso inapropriado) + EVT-002.

#### PAT-MEMORY-001 — Memory budget per-isolate
- **Problema:** CF Worker tem limite 128 MB heap; OOM kill é silencioso.
- **Solução:** allocator lib que rastreia high-watermark; reject requests com payload > 5 MiB no hot path (redirecionar para streaming); emit `corelink_isolate_heap_peak_bytes`.
- **FMs mitigados:** FM-002.
- **Evidence:** EVT-024 (load test com payload grande).

#### PAT-MONOTONIC-001 — Clock monotonic para timestamps de ordem
- **Problema:** clock skew entre Worker regiões → timestamps fora de ordem.
- **Solução:** timestamps de audit/ULID usam clock monotonic local + sync NTP tolerância ±1s; conflitos resolvidos por `request_id` ULID (tie-breaker).
- **FMs mitigados:** FM-350.
- **Evidence:** EVT-002 (property test de monotonicidade).

#### PAT-ONLINE-MIGRATE-001 — Migration online D1/Neon
- **Problema:** ALTER TABLE bloqueia tabela; migration no hot path derruba SLO.
- **Solução:** expand-migrate-contract: (1) adicionar coluna nullable; (2) dual-write + backfill batched (PAT-JITTER-001 entre batches); (3) switch reads; (4) remove old coluna em release futura.
- **FMs mitigados:** FM-056, FM-301.
- **Evidence:** EVT-018 (migration applied) + EVT-002.

#### PAT-PATCH-SLA-001 — SLA de aplicação de patch CVE
- **Problema:** CVE HIGH publicado em dep; tempo para deploy fix.
- **Solução:** SLA: CRITICAL ≤ 48h, HIGH ≤ 7d, MEDIUM ≤ 30d; automation via `cargo-audit` + GitHub Dependabot + release cadence semanal (pode forçar off-cycle para CRITICAL).
- **FMs mitigados:** FM-155.
- **Evidence:** EVT-007 (dependency scan) + EVT-038 (deploy log).

#### PAT-QUEUE-EVENTS-001 — Queue para eventos externos (Stripe)
- **Problema:** Stripe API outage durante billing call quebra fluxo.
- **Solução:** billing events são enfileirados em R2 + worker que retenta com exp backoff + dedupe via idempotency key; outage de Stripe ≠ outage do CoreLink.
- **FMs mitigados:** FM-151.
- **Evidence:** EVT-002 + EVT-023 (chaos test inject Stripe failure).

#### PAT-READ-YOUR-WRITES-001 — Read-your-writes para eventual storage
- **Problema:** R2 LIST é eventual; PUT + immediate LIST pode não ver.
- **Solução:** app mantém in-memory write-through cache de writes recentes (TTL 60s); LIST consulta cache primeiro + R2.
- **FMs mitigados:** FM-052.
- **Evidence:** EVT-002 (property test concurrent PUT+LIST).

#### PAT-RUNBOOK-DRILL-001 — Runbook dry-run mensal
- **Problema:** runbook desatualizado descoberto em incident real.
- **Solução:** oncall faz dry-run de 1 runbook P0/P1 por mês; output em EVT-017; atualiza runbook se discrepância.
- **FMs mitigados:** FM-202.
- **Evidence:** EVT-017.

#### PAT-SESSION-CONSISTENCY-001 — D1 Sessions API para reads
- **Problema:** D1 primary latency spike em cold region.
- **Solução:** reads sequenciais do mesmo request usam D1 Sessions API (bookmark-based consistency) → leituras consistentes sem sempre ir ao primary.
- **FMs mitigados:** FM-055.
- **Evidence:** EVT-002 (read-after-write test).

#### PAT-SWEEPER-001 — Sweeper de orphans (multipart incomplete)
- **Problema:** multipart upload abortado deixa parts órfãos que consomem storage silenciosamente.
- **Solução:** job diário lista multipart em-flight > 7d e aborta (`AbortMultipartUpload`).
- **FMs mitigados:** FM-060.
- **Evidence:** EVT-017.

#### PAT-TIMEOUT-002 — Deadline hard em exec-action com kill
- **Problema:** exec-action do cliente pode travar indefinidamente.
- **Solução:** deadline enforced por Container runtime (CF Containers); SIGKILL após deadline_s; grace 2s para SIGTERM graceful.
- **FMs mitigados:** FM-004.
- **Evidence:** EVT-002 (deadline test) + EVT-023 (chaos com hang).

#### PAT-TTL-JITTER-001 — Jitter em TTLs de cache
- **Problema:** N entries expiram ao mesmo tempo → thundering herd.
- **Solução:** TTL setado com jitter ±10% (hash-based para determinismo).
- **FMs mitigados:** FM-352.
- **Evidence:** EVT-002.

#### PAT-AUTHZ-002 — (alias histórico; ver PAT-AUTHZ-001)
- **Nota:** `slo_catalog.md §4.10` citava `PAT-AUTHZ-002` — mapeamento canônico é **PAT-AUTHZ-001** acima. Referência mantida para compat dos audits Lote 3+4.

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
| Timeout            | Slow-mock injection; assert kill < cap            | EVT-002              |
| Retry              | Flaky-mock; assert max_attempts respected          | EVT-002              |
| Circuit breaker    | Error-rate injection; assert state transitions     | EVT-002              |
| Bulkhead           | Saturation injection; assert isolation             | EVT-024                   |
| Rate limit         | Load test > limit; assert shaping                  | EVT-024                   |
| Graceful degrade   | Chaos test; assert 503s with Retry-After           | EVT-023                |
| Soft-delete        | Delete + undelete in grace window                   | EVT-002       |
| Reconciliation     | Drift injection; assert job catches                | EVT-002       |
| Progressive rollout | Error budget burn simulation; assert rollback     | EVT-024                   |

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
