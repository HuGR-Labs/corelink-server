---
id: "FAILURE-MODES"
type: "failure_modes"
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
tags: ["architecture", "reliability", "fmea", "failure-analysis", "incident"]
---

# Failure Modes — FMEA Catálogo Canônico

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do catálogo de modos de falha (FMEA). Consumido por:
> - `specs/_templates/work_item.md §18` (riscos operacionais) e `§25` (rollback específico)
> - `specs/_templates/sprint_contract.md §15` (failure scenarios do sprint)
>
> Coordenado com `resilience_patterns.md` (como mitigar), `security_model.md` (componente adversário), `privacy_model.md` (impacto em titulares). Regra: qualquer WI que introduza novo FM catalogado **DEVE** declarar `inherits_from: ["FAILURE-MODES"]` e referenciar IDs `FM-XXX`.

---

## Sumário

1. [Método (FMEA adapted) + severity/occurrence/detectability](#1-método-fmea-adapted--severityoccurrencedetectability)
2. [Taxonomia de falhas](#2-taxonomia-de-falhas)
3. [Catálogo FM-XXX](#3-catálogo-fm-xxx)
4. [Matriz de blast radius](#4-matriz-de-blast-radius)
5. [Mapeamento FM → Pattern de resiliência](#5-mapeamento-fm--pattern-de-resiliência)
6. [Runbook stubs por FM](#6-runbook-stubs-por-fm)
7. [Processo de atualização do catálogo](#7-processo-de-atualização-do-catálogo)

---

## 1. Método (FMEA adapted) + severity/occurrence/detectability

Adaptamos **FMEA** (Failure Mode & Effects Analysis) do SAE J1739 para software de infra:

Cada FM tem scores 1–5 em:

- **S — Severity (impacto):** 5 = corruption/poisoning cross-tenant; 4 = outage global; 3 = degradação SLO; 2 = tenant single impacted; 1 = internal, não visível.
- **O — Occurrence (probabilidade anualizada):**
  - 5 = ≥ diário
  - 4 = semanal
  - 3 = mensal
  - 2 = trimestral/anual
  - 1 = nunca observado em sistemas comparáveis
- **D — Detectability inversa (dificuldade de detectar):**
  - 5 = silencioso (dado corrompido sem alerta)
  - 4 = latente (só aparece em auditoria)
  - 3 = métrica revela em horas
  - 2 = alert em minutos
  - 1 = alert em segundos / crash visível

**RPN = S × O × D** (1–125). Threshold:

- RPN ≥ 60 **E** S ≥ 4 → classe **P0**: requer TLA+ ou fuzz **obrigatório** + runbook com SLA ≤ 15 min.
- 30 ≤ RPN < 60 → **P1**: runbook + test automatizado obrigatório.
- RPN < 30 → **P2**: documentado; mitigação best-effort.

**Override por severidade (regra explícita, S-09 do audit Lote 3+4):**

- `S = 5` força minimum **P1** independente de RPN. Justificativa: severity 5 implica blast radius cross-tenant ou data loss; mesmo com O baixo, o impacto demanda runbook + test obrigatório.
- Override **DEVE** ser anotado na coluna Classe como "P1 (S=5 → upgrade)" para auditabilidade.

**Score O sob mitigação ausente (regra FMEA original, S-19 do audit Lote 3+4):**

- O reflete frequência **com mitigações ausentes** (FMEA original SAE J1739 §B.2.4), não com mitigações presentes.
- Razão: subscoring de O com mitigações leva a falsa sensação de segurança; quando mitigação falhar (deploy regressou, CTRL não foi implantado), O efetivo é o "raw" — esse é o número que importa em RPN.
- FMs adversariais (FM-253, FM-254, FM-303) têm O **mínimo 2** mesmo sem evidência observada, porque "esperado em pen-test não-trivial".

---

## 2. Taxonomia de falhas

| Classe            | Fonte                                                         |
|-------------------|---------------------------------------------------------------|
| `compute`         | Worker, Container, DO                                          |
| `storage`         | R2, D1, Neon, KV, DO storage                                    |
| `network`         | Cloudflare edge, inter-region, DNS                              |
| `dependency`      | Dep crate, sub-processor (Stripe, Neon, etc)                    |
| `operational`     | Ops humana, deploy, config change                                |
| `adversarial`     | Abuse, DoS, supply chain, credential leak                        |
| `data-integrity`  | Corrupção silenciosa, bit rot, protocol bug                      |
| `clock-state`     | Skew, leap second, ntp drift                                     |
| `emergent`        | Feedback loop, cascading overload, retry storm                   |

---

## 3. Catálogo FM-XXX

### 3.1 Compute

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-001 | Worker CPU timeout (isolate killed by CF runtime)       | 3 | 3 | 2 | 18  | P2      | PAT-TIMEOUT-001, PAT-BUDGET-001|
| FM-002 | Worker OOM (memória > 128 MB limite CF)                 | 3 | 3 | 2 | 18  | P2      | PAT-MEMORY-001                 |
| FM-003 | Container exit non-zero em execute-action               | 2 | 4 | 1 | 8   | P2      | PAT-RETRY-001 (idempotent only)|
| FM-004 | Container deadline hit (> 60min)                        | 2 | 3 | 1 | 6   | P2      | PAT-TIMEOUT-002 + CTRL-EXEC-001, user notification |
| FM-005 | DO actor rebalance causa spike de latência              | 3 | 2 | 3 | 18  | P2      | PAT-DEGRADE-001                |
| FM-006 | Panic em Rust não capturado (Worker)                    | 4 | 2 | 2 | 16  | P2      | PAT-ERROR-ISOLATE-001, std sanitizer |
| FM-007 | Deserialization RCE (dep corrupted or bad serde config) | 5 | 1 | 5 | 25  | P1 (S=5 → upgrade)         | PAT-INPUT-HARDEN-001 + CTRL-INPUT-003 + CTRL-INPUT-004 + CTRL-EXEC-002..003 + RB-FM-007 |

### 3.2 Storage

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-050 | R2 bucket parcialmente indisponível (região)           | 4 | 2 | 2 | 16  | P2      | PAT-REGION-FAILOVER-001        |
| FM-051 | R2 bit rot detectado (hash mismatch on read)            | 5 | 1 | 4 | 20  | P1 (S=5 → upgrade) | CTRL-CAS-002 verify + scrub periódico |
| FM-052 | R2 eventual consistency (LIST não vê PUT recente)       | 3 | 3 | 3 | 27  | P2       | PAT-READ-YOUR-WRITES-001       |
| FM-053 | R2 IAM change não propagado (403 intermitente)          | 3 | 2 | 4 | 24  | P2       | PAT-RETRY-BACKOFF-001 + alert  |
| FM-054 | KV eventual consistency (stale > 60s esperado)          | 2 | 5 | 3 | 30  | P1       | PAT-KV-TTL-001; nunca usar KV pra verdade |
| FM-055 | D1 primary latency spike (cold region)                  | 3 | 3 | 2 | 18  | P2      | PAT-SESSION-CONSISTENCY-001    |
| FM-056 | D1 schema migration lock                                 | 3 | 2 | 3 | 18  | P2      | PAT-ONLINE-MIGRATE-001         |
| FM-057 | Neon failover (primary promotes; 30-90s)                | 4 | 2 | 2 | 16  | P2      | PAT-CIRCUIT-001 + PAT-DEGRADE-001 |
| FM-058 | Neon connection pool exhaustion                          | 4 | 2 | 2 | 16  | P2      | PAT-BULKHEAD-001               |
| FM-059 | DO storage quota exceeded (32 MiB por DO)               | 3 | 3 | 3 | 27  | P2       | PAT-QUOTA-ALERT-001 + shard    |
| FM-060 | R2 multipart upload orphaned (incomplete)                | 2 | 3 | 4 | 24  | P2       | PAT-SWEEPER-001 (abort multipart > 7d) |
| FM-061 | Audit log R2 Object Lock impede emergency redaction     | 2 | 2 | 3 | 12  | P2       | RB-GDPR-ERASURE-HOLD (legal path) |
| FM-062 | CAS/AC hash collision (criptográfico — BLAKE3)           | 5 | 1 | 5 | 25  | P1 (S=5 → upgrade)| Defense in depth: dupla hash algo opcional |

### 3.3 Network

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-100 | DNS outage (registrar / CF DNS)                          | 5 | 1 | 2 | 10  | P1 (S=5 → upgrade) | PAT-DNS-TTL-001 + CTRL-NET-001 + CTRL-NET-002 + RB-FM-100 |
| FM-101 | CF edge outage (região ou global)                        | 5 | 1 | 1 | 5   | P1 (S=5 → upgrade) | Status page + comms plan       |
| FM-102 | BGP leak afeta CF IPs                                    | 4 | 1 | 3 | 12  | P2      | CF Magic; mitigation ops        |
| FM-103 | TLS cert expiry / revoked                                | 4 | 1 | 2 | 8   | P2      | Auto-renew + canary check       |
| FM-104 | mTLS binding between Worker/Container drops              | 3 | 2 | 3 | 18  | P2      | PAT-RETRY-001 + alert           |
| FM-105 | Inter-region latency spike afeta cross-region dedup      | 2 | 3 | 3 | 18  | P2      | PAT-DEGRADE-001                |
| FM-106 | Proxy head-of-line blocking                               | 2 | 3 | 3 | 18  | P2      | HTTP/3 rollout                  |

### 3.4 Dependencies / Sub-processors

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-150 | Cloudflare API limits hit (control plane op)            | 2 | 3 | 1 | 6   | P2      | PAT-BACKOFF-001                |
| FM-151 | Stripe API outage (billing)                              | 2 | 2 | 1 | 4   | P2      | PAT-QUEUE-EVENTS-001 (retry) + RB-FM-151 |
| FM-152 | Neon outage                                              | 4 | 2 | 1 | 8   | P2      | Read-only mode + alert         |
| FM-153 | Grafana Cloud outage                                     | 1 | 2 | 1 | 2   | P2      | Metrics em R2 Logpush como fallback |
| FM-154 | Dep crate yank mid-deploy                                | 3 | 2 | 3 | 18  | P2      | `Cargo.lock` pinned + CI check |
| FM-155 | Dep CVE HIGH descoberto                                   | 3 | 3 | 2 | 18  | P2      | PAT-PATCH-SLA-001 + cargo-audit CI |
| FM-156 | Dep com maintainer malicioso (supply chain TA-5)        | 5 | 1 | 5 | 25  | P1 (S=5 → upgrade) | SLSA L3 + review + signed commits |

### 3.5 Operational

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-200 | Deploy introduz regressão não testada                   | 4 | 3 | 2 | 24  | P2      | PAT-PROGRESSIVE-ROLLOUT-001    |
| FM-201 | Config change causa rate-limit drop                     | 3 | 3 | 2 | 18  | P2      | PAT-DUAL-APPROVAL-001 + auto-rollback |
| FM-202 | Runbook desatualizado em incident                        | 3 | 4 | 3 | 36  | P1       | PAT-RUNBOOK-DRILL-001 (mensal)    |
| FM-203 | Oncall sobrecarregado (fadiga → missed alert)            | 4 | 2 | 3 | 24  | P2      | Pager discipline (§9 obs)     |
| FM-204 | Secret rotation quebra serviço                            | 4 | 2 | 2 | 16  | P2      | PAT-ROLL-FORWARD-001 (overlap period) + CTRL-KEY-005 + CTRL-KEY-006 + CTRL-CRED-003 |
| FM-205 | Manual intervention apaga dado (admin mistake)           | 5 | 2 | 3 | 30  | P1       | PAT-DUAL-APPROVAL-001 + soft-delete |
| FM-206 | Terraform drift (estado real ≠ definido)                  | 3 | 3 | 4 | 36  | P1       | PAT-DRIFT-DETECTION-001       |

### 3.6 Adversarial

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-250 | DDoS volumetric no edge                                  | 3 | 3 | 1 | 9   | P2      | CF DDoS managed; CTRL-RATE-001 |
| FM-251 | Credential stuffing / brute force                         | 3 | 4 | 1 | 12  | P2      | CF WAF + lockout policy        |
| FM-252 | PAT leaked em repo público                                | 3 | 3 | 3 | 27  | P2       | Secret scanning + auto-revoke  |
| FM-253 | Cross-tenant read (security bug)                         | 5 | 2 | 4 | 40  | P1 (S=5 → upgrade; O 1→2 em S-19 audit Lote 3+4) | TLA+ INV-TENANT-ISOLATION + RB-FM-253 + CTRL-ISO-001 + CTRL-ISO-002 + CTRL-ISO-003 + CTRL-ISO-004 + CTRL-ISO-005 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 |
| FM-254 | Cache poisoning (TA-3 inserir blob com hash forjado)     | 5 | 2 | 5 | 50  | P1 (S=5 → upgrade; O 1→2 em S-19) | CTRL-CAS-001 + client verify  |
| FM-255 | Tenant-pago abusa execute-action para criptominer        | 3 | 3 | 2 | 18  | P2      | PAT-ABUSE-DETECT-001 + quota    |
| FM-256 | Compression bomb em CAS write                             | 3 | 2 | 2 | 12  | P2      | CTRL-COMP-001                  |
| FM-257 | Replay attack com token válido capturado                  | 3 | 2 | 3 | 18  | P2      | CTRL-AUTH-007 nonce window      |
| FM-258 | Insider data exfil via support tool                       | 5 | 1 | 5 | 25  | P1 (S=5 → upgrade) | CTRL-PRIV-014 + CTRL-PRIV-016 + CTRL-AUTH-010 + CTRL-AUDIT-003 + RB-FM-258 |

### 3.7 Data Integrity

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-300 | GC deleta blob ainda referenciado (refcount bug)         | 5 | 2 | 4 | 40  | P1 (FF-HR-011 aplicável — ADR-0012) | INV-GC-001 TLA+ + PAT-SOFT-DELETE-001 + RB-FM-300 |
| FM-301 | Migration doble-apply (idempotency bug)                  | 4 | 2 | 3 | 24  | P2      | PAT-MIGRATION-IDEM-001         |
| FM-302 | Billing counter não incrementa (silent revenue leak)     | 3 | 2 | 5 | 30  | P1                  | Reconciliation diária           |
| FM-303 | AC entry aponta pra blob de outro tenant (bug)           | 5 | 2 | 4 | 40  | P1 (S=5 → upgrade; O 1→2 em S-19) | INV-TenantIsolation + test integração + RB-FM-303 |
| FM-304 | Corrupção em audit chain (hash chain broken)             | 4 | 1 | 4 | 16  | P2 (S=4) | PAT-AUDIT-VERIFY-001 diário     |
| FM-305 | Tombstone lost (GC não roda; storage infla)              | 3 | 3 | 3 | 27  | P2       | PAT-GC-HEALTHCHECK-001          |

### 3.8 Clock / State

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-350 | Clock skew > 5s entre Worker e D1                       | 3 | 3 | 3 | 27  | P2       | PAT-MONOTONIC-001 + ULID ts   |
| FM-351 | Leap second malhandled                                   | 2 | 1 | 3 | 6   | P2       | UTC smoothing; runbook         |
| FM-352 | Cache TTL race (stale read)                               | 2 | 3 | 3 | 18  | P2       | PAT-TTL-JITTER-001             |

### 3.9 Emergent

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-400 | Retry storm amplifica outage downstream                   | 4 | 3 | 3 | 36  | P1       | PAT-CIRCUIT-001 + jitter      |
| FM-401 | Thundering herd em cache miss                             | 3 | 3 | 2 | 18  | P2      | PAT-SINGLEFLIGHT-001           |
| FM-402 | Feedback loop: alert dispara action que dispara alert      | 4 | 1 | 4 | 16  | P2 (S=4) | Dampening + runbook check     |
| FM-403 | Latent leak em long-running Container                      | 3 | 3 | 4 | 36  | P1       | PAT-RESTART-JIT-001           |
| FM-404 | GC sweep conflita com write (refcount racy)                | 5 | 1 | 4 | 20  | P1 (S=5 → upgrade; FF-HR-011 aplicável — ADR-0012) | INV-GC-001 + INV-GC-004 TLA+ linearizability + RB-FM-404 |

### 3.10 Privacy / Regulatory (Lote 10.11.0-bis — S-11 Privacy Pipeline)

| ID     | Descrição                                                | S | O | D | RPN | Classe | CTRLs / Patterns              |
|--------|----------------------------------------------------------|---|---|---|-----|---------|--------------------------------|
| FM-450 | DSR pipeline failure: cross-backend ack timeout, retry exausto, 24h SLO breach | 5 | 2 | 3 | 30 | P1 (S=5 → upgrade) | PAT-RETRY-IDEMPOTENT-001 + INV-DATA-ERASURE-COMPLETE TLA+ + CTRL-PRIV-DSR-RECEIPT + RB-DSR-ERASURE-INCOMPLETE + dead-letter quarantine + on-call page ≤5min |
| FM-451 | Residency leak (cross-region write detected; mismatched tenant.primary_region vs backend region) | 2 | 4 | 5 | 40 | **P0 (S=5 → upgrade FF-HR-010)** — Schrems II legal exposure | INV-DATA-RESIDENCY CRITICAL + custom domain routing (PAT-ROUTING-PINNED-001 fail-CLOSED 451) + insert checks (D1 trigger trg_blob_meta_region_match + Worker pre-flight) + property test 20k CI (0 leaks baseline) + RB-DATA-RESIDENCY-LEAK + SEV-1 alert + ANPD/EDPB pre-notification path; 2 CloudEvents `dev.hugr.corelink.residency.write_rejected_cross_region.v1` audit emit fail-CLOSED; WI-S11-007 SEALED runtime cobertura |
| FM-452 | Consent ledger fork: parallel grant/revoke writes break append-only chain (race condition cross-region) | 5 | 1 | 5 | 25 | P1 (S=5 → upgrade) | PAT-AUDIT-VERIFY-001 + INV-CONSENT-PROOF-VERIFIABLE + INV-AUDIT-APPEND-ONLY + CTRL-PRIV-CONSENT-003 + Lamport clock per ledger + RB-CONSENT-TAMPERING |
| FM-453 | Sub-processor breach upstream (Stripe/Cloudflare/etc.) forces our 72h notification clock; broadcast channel down | 5 | 2 | 3 | 30 | P1 (S=5 → upgrade) | PAT-RETRY-IDEMPOTENT-001 broadcast + EDPB Guidelines 9/2022 + LGPD ANPD Res. 15/2024 + RB-SUB-PROCESSOR-BROADCAST-MISS + 30d objection flow |

---

## 4. Matriz de blast radius

| FM       | Blast radius                                        |
|----------|-----------------------------------------------------|
| FM-001..007 | 1 request ou 1 Worker isolate; raramente global  |
| FM-050..051 | Região inteira (potencial corrupção global se não detectado) |
| FM-052..056 | Tenant-local em leitura; global via D1 migration  |
| FM-057..058 | Global (D1/Neon = single source)                  |
| FM-100..106 | Global; afeta todos os tenants                    |
| FM-253..254 | **Cross-tenant** — catastrófico; disable feature + incident |
| FM-300..305 | Variável — FM-300 e FM-303 cross-tenant; FM-302 financial |

---

## 5. Mapeamento FM → Pattern de resiliência

Referências a IDs `PAT-XXX` serão formalizadas em `resilience_patterns.md`. Sumário:

- **Compute** → timeout, memory guard, input hardening, error isolation.
- **Storage** → read-your-writes, session consistency, bulkhead, quota alert, sweeper, scrub.
- **Network** → retry with jitter, circuit breaker, degrade mode.
- **Operational** → progressive rollout, dual-approval, auto-rollback, drift detection, runbook drill.
- **Adversarial** → rate limit, abuse detection, TLA+ invariant enforcement.
- **Data integrity** → soft-delete + grace, reconciliation, formal verification.
- **Emergent** → singleflight, dampening, periodic restart.

---

## 6. Runbook stubs por FM

Cada FM P0/P1 **DEVE** ter runbook em `specs/05_quality/runbooks/RB-<FM-ID>.md` (criados Lote 5.7+5.12+6.4; 26 RBs catalogados em §6.1). Template mínimo:

1. Detecção: qual alert disparou? Quais métricas checar?
2. Comunicação: quem notificar? Status page?
3. Mitigação imediata (≤ 15 min): degrade mode, failover, rollback.
4. Mitigação completa (≤ 1h): hot fix, config change.
5. Root cause: queries pra investigar, snapshots de estado.
6. Post-mortem: trigger para criação (`incident.md`).

### 6.1 Runbooks catalogados (todos os 26; atualizado Lote 6.4)

**FM-level runbooks:**

- `RB-FM-007` (deserialization RCE) → trimestral; fuzz corpus review.
- `RB-FM-051` (R2 bit rot) → trimestral; scrub verification.
- `RB-FM-054` (KV stale > 60s) → anual.
- `RB-FM-057` (Neon failover) → semestral.
- `RB-FM-062` (hash collision) → anual (probability ~2^-128; tabletop).
- `RB-FM-100` (DNS outage) → semestral.
- `RB-FM-101` (CF edge outage) → semestral; comms drill.
- `RB-FM-156` (dep maintainer malicioso) → semestral.
- `RB-FM-202` (runbook stale) → mensal (meta drill).
- `RB-FM-205` (admin mistake) → anual (tabletop).
- `RB-FM-206` (terraform drift) → mensal.
- `RB-FM-253` (cross-tenant read) → **HIGHEST priority**; dry-run trimestral.
- `RB-FM-254` (cache poisoning) → trimestral.
- `RB-FM-258` (insider exfil) → anual (tabletop with HR/Legal).
- `RB-FM-300` (GC refcount bug) → semestral (destructive; staging only).
- `RB-FM-151` (Stripe outage; FM-151 mitigation) → semestral; staging dry-run prerequisite (sprint contract S-10 §6 DoD; criado Lote 10.10bis).
- `RB-FM-302` (billing leak) → mensal (reconciliation checks).
- `RB-FM-303` (AC entry cross-tenant) → trimestral; TLA+ replay.
- `RB-FM-400` (retry storm) → trimestral; chaos test mensal.
- `RB-FM-403` (container memory leak) → trimestral.
- `RB-FM-404` (GC sweep + write race) → semestral; TLA+ regression test.

**Non-FM runbooks (cross-cutting):**

- `RB-BREACH-NOTIF` (data breach notification) → semestral tabletop.
- `RB-KEY-COMPROMISE` (key compromise) → semestral dry-run.
- `RB-HSM-UNAVAILABLE` (HSM outage) → semestral.
- `RB-BYOK-REVOKE` (customer kill switch) → semestral dry-run.
- `RB-GDPR-ERASURE-HOLD` (legal hold vs DSR) → anual.
- `RB-SLO-AVAIL-CP` (SLO availability burn) → contínuo (alert-driven).

---

## 7. Processo de atualização do catálogo

### 7.1 Triggers

- **Pós-incident**: todo post-mortem **DEVE** mapear root cause para FM existente OU criar novo (ADR curto).
- **Pós-audit** (security, privacy, SOC 2): novos FMs surgidos vão pro catálogo.
- **Semestral review**: reavaliar RPNs; FMs com observed > predicted → upgrade.
- **Pré-release major**: review integral.

### 7.2 Versionamento

- Bump minor do doc: adicionar FM ou atualizar CTRL.
- Bump major: re-categorização de classe ou mudança em threshold RPN.

### 7.3 Sign-off

- Novos FMs P0/P1 requerem aprovação: **SRE Lead + Security Lead + Architect**.
- Mudanças de classe requerem evidência quantitativa (dados de campo).

---

**Fim de FAILURE-MODES.** Todo incident marcado como P0/P1 **DEVE** referenciar o FM-ID em `post_mortem.md` § root cause; se nenhum FM existente aplica, criar novo no mesmo PR do post-mortem.
