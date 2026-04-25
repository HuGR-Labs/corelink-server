---
id: "SLO-CATALOG"
type: "slo_catalog"
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
tags: ["architecture", "slo", "sli", "error-budget", "reliability"]
---

# SLO Catalog — SLIs, SLOs, Error Budgets

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) dos SLIs/SLOs/error budgets do CoreLink. Consumido por:
> - `specs/_templates/production_readiness_review.md §4` (confirmar quais SLOs aplicam à PRR)
> - `observability_model.md §9` (alerting multi-burn-rate)
> - `failure_modes.md` (classe P0/P1 amarrada em impacto SLO)
>
> Regra: novos SLOs **DEVEM** ter ADR + evidência de baseline medido ≥ 30 dias + owner on-call assinado.

> **🚦 Phase boundary (F-12 audit Lote 3+4):**
> CoreLink GA inicial cobre **Fase 1 — Remote Cache** (CAS + AC + GC + REAPI cache-only).
> **Fase 2 — Remote Execution** (`execute-action`, executor identity, sandbox runtime) é **futuro** (roadmap pós-GA).
> Seções/CTRLs/SLOs/labels marcados com `(Fase 2)` ou `execute-action` referem-se a planejamento; em GA inicial podem ser omitidos do scope mínimo.



---

## Sumário

1. [Princípios](#1-princípios)
2. [Taxonomia de SLIs](#2-taxonomia-de-slis)
3. [SLO tier strategy](#3-slo-tier-strategy)
4. [Catálogo de SLOs](#4-catálogo-de-slos)
5. [Error budget policy](#5-error-budget-policy)
6. [Reporting & review cadence](#6-reporting--review-cadence)
7. [Referências](#7-referências)

---

## 1. Princípios

1. **Cliente ≠ eng interna.** SLOs devem refletir o que o cliente percebe. Latência de GC interno não é SLO; latência percebida no REAPI sim.
2. **Menos SLOs, mais significativos.** Começar com 5–7 top-level SLOs; resistir à inflação. Cada SLO ≥ 1 alert direto.
3. **Mensurável hoje.** Sem instrumentação, sem SLO. Propor SLO sem ter SLI operando em prod ≥ 30 dias = red flag.
4. **Error budget é real.** Burn define pace de deploy / rollback. Budget esgotado = freeze.
5. **Transparência.** SLOs publicados em `status.corelink.dev`; burn em tempo real.
6. **Multi-tenant awareness.** SLOs globais **e** per-enterprise-tenant (em contratos com SLA).

---

## 2. Taxonomia de SLIs

Cada SLI é uma razão `good_events / valid_events` sobre uma janela. 4 tipos principais no CoreLink:

| Tipo          | Formato SLI                                                          | Exemplo |
|---------------|----------------------------------------------------------------------|---------|
| Availability  | `(# requests não-5xx e não-timeout) / (# requests válidas)`         | SLI-AVAIL-CAS-GET |
| Latency       | `(# requests com dur ≤ threshold) / (# requests válidas)`            | SLI-LAT-CAS-GET |
| Correctness   | `(# requests com resposta válida verificada) / (# requests válidas)` | SLI-CORRECT-CAS |
| Freshness     | `(# events processados dentro de SLA) / (# events válidos)`          | SLI-FRESH-BILLING |

**Filters "valid":** excluir requests que são intencionalmente erro do cliente (4xx não-429, malformado, escopo errado). Rate-limited (429) **conta como** failure de availability se o cliente estiver dentro de sua quota ⇒ bug nosso.

---

## 3. SLO tier strategy

CoreLink opera em **5 tiers** (alinhado com `remote_cache_product_profile.md §8.2`; corrigido S-13 do audit Lote 3+4).

| Tier           | Audiência                    | Availability alvo | Latency alvo p99 (CAS GET) | Suporte             |
|----------------|------------------------------|--------------------|------------------------------|----------------------|
| `free`         | OSS / trial                  | 99.5% (3.6h/mo)    | 500ms                        | Community / best-effort |
| `solo`         | Dev individual / pequeno time | 99.7% (2.2h/mo)    | 400ms                        | Email, 5 business days |
| `team`         | Mid-size eng org             | 99.9% (43m/mo)     | 300ms                        | Email, 1 business day |
| `business`     | Grande org sem contrato custom | 99.93% (30m/mo)    | 250ms                        | Pager business hours, credits |
| `enterprise`   | Grandes corps c/ contrato custom | 99.95% (21m/mo)    | 200ms                        | Pager 24/7, credits, BYOK opcional |

> Mais fortes (99.99%) requerem contrato bespoke + ADR + baseline ≥ 30 dias (§1.3).

Budgets são independentes por tier. Métricas agregadas em `corelink_slo_*{tier}`.

### 3.1 Targets por tier aplicáveis aos SLOs em §4.x (T-06)

Adicionado em Lote 6.7 endereçando T-06 do re-audit (SLOs §4.x listavam apenas team+enterprise; solo/business ficavam sem target explícito). Regra de interpolação:

- **SLO com target declarado para team e enterprise** (ex: SLO-AVAIL-CAS-GET, SLO-AVAIL-CP): tier `free` usa target de team **menos 0.4pp** (ex: team 99.9% → free 99.5%); tier `solo` usa team **menos 0.2pp** (99.7%); tier `business` usa team **mais 0.03pp** (99.93%).
- **SLOs latency**: free = team×1.6; solo = team×1.3; business = team×0.85; enterprise = team×0.67.
- **Correctness SLOs (100%, sem budget)** — aplicam a **todos os 5 tiers** igualmente (correctness não escala com tier).
- **Freshness billing/DSR** — aplicam a todos os tiers igualmente (requisitos regulatórios).

SLOs individuais (§4.x) podem override interpolation via linha explícita "Target ({tier})". Defaults aplicam se target não listado.

---

## 4. Catálogo de SLOs

### 4.1 Availability — Control Plane (Worker API)

**SLO-AVAIL-CP**: availability do control plane (endpoints de auth, admin, non-storage).

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_cp_requests_total{status!~"5.."} / corelink_cp_requests_total{}` |
| Target (team)      | 99.9%                                                         |
| Target (enterprise)| 99.95%                                                        |
| Window             | 30 dias rolling                                              |
| Alert windows      | 5m/1h/6h/24h (multi-burn-rate)                               |
| Owner              | SRE                                                           |
| FMs cobertos       | FM-101, FM-106, FM-152                                        |
| Runbook            | [`RB-SLO-AVAIL-CP`](../../05_quality/runbooks/RB-SLO-AVAIL-CP.md) |

### 4.2 Availability — CAS GET

**SLO-AVAIL-CAS-GET**: CAS read path é o hot path do valor do produto.

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI (numerador)    | `corelink_cas_get_requests_total{outcome IN ("ok","miss")}` |
| SLI (denominador)  | `corelink_cas_get_requests_total{outcome NOT IN ("error_client_4xx_legitimate")}` — inclui `rate_limited_within_quota` (failure nosso, ver F-05 audit Lote 3+4) |
| Target (team)      | 99.9%                                                        |
| Target (enterprise)| 99.95% (alinhado com tier table §3; 99.99% requer baseline ≥ 30 dias + ADR — proibido sem evidence) |
| Window             | 30 dias                                                      |
| FMs                | FM-050, FM-057, FM-250                                        |
| Notas              | `miss` conta como OK (servimos 200 sem body, é semântica correta); `rate_limited_within_quota` (cliente dentro do plan + nosso DO disse 429) **conta como failure** — bug nosso. `rate_limited_over_quota` (cliente excedeu plan) excluído (legítimo). Corrigido S-08 + F-05. |

### 4.3 Availability — CAS PUT

**SLO-AVAIL-CAS-PUT**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_cas_put_requests_total{status IN (200,201,204,409)} / total` |
| Target (team)      | 99.9%                                                        |
| Target (enterprise)| 99.95%                                                       |
| Window             | 30 dias                                                      |
| FMs                | FM-050, FM-052                                                |
| Notas              | 409 = content already exists (hash collision benigna) conta como OK |

### 4.4 Availability — AC (Action Cache)

**SLO-AVAIL-AC**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_ac_requests_total{status != 5xx} / total`          |
| Target (team)      | 99.9%                                                        |
| Target (enterprise)| 99.95%                                                       |

### 4.5 Availability — Execute Action

**SLO-AVAIL-EXEC**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_exec_completed_total{outcome != "exec_infra_error"} / total` |
| Target (team)      | 99.5%                                                        |
| Target (enterprise)| 99.9%                                                        |
| Notas              | `exec_sandbox_violation`, `exec_timeout` = **cliente**; não conta como infra error |

### 4.6 Latency — CAS GET p99

**SLO-LAT-CAS-GET**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_cas_get_duration_seconds_bucket{le≤0.3} / total`   |
| Target (team)      | 99% < 300ms                                                   |
| Target (enterprise)| 99% < 200ms                                                   |
| Window             | 30 dias                                                      |
| Notas              | Medido na edge (CF Worker), inclui R2 get + egress           |

### 4.7 Latency — CAS PUT p99

**SLO-LAT-CAS-PUT**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `bucket{le≤1.0} / total`  (blobs ≤ 16 MiB)                   |
| Target (team)      | 99% < 1s  (para blobs ≤ 16 MiB)                              |
| Target (enterprise)| 99% < 600ms                                                  |
| Notas              | Blobs maiores → multipart; SLO separado opcional (SLO-LAT-CAS-PUT-MULTIPART). |

### 4.8 Latency — AC hit

**SLO-LAT-AC-HIT**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_ac_get_duration_seconds_bucket{outcome="hit", le≤0.15} / total` |
| Target             | 99% < 150ms                                                  |

### 4.8.1 Efficiency — Dedup Ratio (S-07)

**SLO-DEDUP-RATIO**: dedup ratio per-tenant intra-tenant chunk-level (storage cost efficiency SLI; revenue/marketing claim defensibility) (Lote 10.7bis P1-1 fix — was forward-stub whitelist; now formally defined).

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_dedup_bytes_saved_total{type="chunk", tenant_id} / corelink_dedup_bytes_uploaded_total{tenant_id}` (per-tenant ratio aggregated 7d window via `rate()`) |
| Target (free)      | ≥ 2.0× sustained (lower expectation for ad-hoc workloads)   |
| Target (solo)      | ≥ 2.5× sustained 7d                                         |
| Target (team)      | ≥ 2.5× sustained 7d (sprint contract S-07 §6 DoD baseline)  |
| Target (business)  | ≥ 2.8× sustained 7d                                         |
| Target (enterprise)| ≥ 3.0× sustained 7d (target SOTA matching BuildBuddy claim) |
| Error budget       | 5% of 30d window (1.5d/30d em violation tolerada)            |
| Multi-burn windows | 1h fast-burn (10× SLO violation rate; SEV-2) + 6h slow-burn (3× rate; SEV-3) per Google SRE Workbook Ch. 5 pattern |
| Rationale          | Dedup ratio é revenue/cost SLI; below target = customer cost overhead; above target = competitive advantage. Anomaly detection layer SEV-3 (drop > 30% WoW) é AT-LEAST-ONE-OF-2 burn-rate alerting. |
| Owner              | WI-S07-001 (lookup) + WI-S07-005 (dashboard); SRE on-call    |
| References         | Sprint contract S-07 §6 DoD; benchmark NativeLink ~2.1× / BuildBuddy ~2.8× / target SOTA 3× |

### 4.9 Correctness — CAS integrity

**SLO-CORRECT-CAS**: % de responses verificadas pelo client-side hash.

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_cas_client_verify_total{outcome="match"} / total`  |
| Target             | 100.0% (sem budget — qualquer corruption = SEV-1)            |
| Notas              | Cliente **sempre** verifica hash antes de usar. Envia metric de telemetria (opt-out). Server mismatches → freeze + runbook. |

### 4.10 Correctness — Tenant isolation

**SLO-CORRECT-ISO**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_isolation_assertion_total{outcome="pass"} / total` |
| Target             | 100.0% (sem budget)                                          |
| Fonte              | Assertions de `tenant_id` em toda storage call (PAT-AUTHZ-001 em `resilience_patterns.md §3.9`; CTRL-AUTHZ-002 em `security_model.md §6.1`) |
| Notas              | Qualquer violação → incident SEV-1 + TLA+ re-check            |

### 4.11 Freshness — Billing events

**SLO-FRESH-BILLING**: eventos de uso processados até billing within 15 min.

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_billing_event_age_seconds_bucket{le≤900} / total` |
| Target             | 99.9%                                                        |
| Notas              | Billing inaccuracy = fraud risk + customer trust loss        |

### 4.12 Freshness — DSR SLA

**SLO-FRESH-DSR-ERASURE**

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `corelink_dsr_resolution_hours_bucket{request="erasure", le≤720} / total`  (720h = 30d) |
| Target             | 99% dentro de 30 dias                                        |
| Legal              | LGPD/GDPR compliance; missed = potencial multa               |

### 4.13 Operational — Deploy safety

**SLO-DEPLOY-SAFE** (interno, não exposto a cliente)

| Campo              | Valor                                                        |
|--------------------|--------------------------------------------------------------|
| SLI                | `# deploys sem rollback automático / # deploys total`        |
| Target             | ≥ 95%                                                        |
| Notas              | < 95% → review do progressive rollout; FM-200                |

---

## 5. Error budget policy

### 5.1 Cálculo

Para um SLO alvo `S` numa janela `W`:
```
budget = (1 - S) × W × rate
burn_rate = (1 - SLI_observed) / (1 - S)   [fator]
```

Exemplo: SLO 99.9% em 30d (43 min permitidos). Burn rate 2 = consumindo o budget em 15d.

### 5.2 Decisões automáticas por burn

| Burn rate sustained (1h) | Ação                                              |
|---------------------------|---------------------------------------------------|
| ≥ 14.4                   | Auto-rollback do deploy mais recente; page P0     |
| ≥ 6.0                    | Freeze deploy; escalate P1                         |
| ≥ 2.0                    | Warning no canal oncall; block risky deploys       |
| < 2.0                    | Nenhuma ação                                       |

### 5.3 Budget exhausted

Se budget esgotado **antes** do fim da janela:

1. **Feature freeze** (só fix + retomar budget).
2. Post-mortem obrigatório (`post_mortem.md`).
3. SRE + Product review: re-priorizar reliability work antes de features.
4. Notificação a tenants enterprise (communicação + credits conforme contrato).

### 5.4 Budget surplus

Se budget sobra consistentemente (>50% sobra em 3 janelas):

- Revisar SLO target: talvez subir.
- OU: realocar capacidade para features (mas com dados, nunca anedota).

---

## 6. Reporting & review cadence

| Cadência        | Artefato                                               | Audiência         |
|-----------------|--------------------------------------------------------|-------------------|
| Contínuo        | Dashboard `DASH-SLO-CATALOG`                           | Eng + Product     |
| Semanal         | Report em Slack `#slo-review`                          | SRE + leads       |
| Mensal          | SLO review meeting; alterações só aqui (via ADR)       | SRE + Product + Eng Mgr |
| Trimestral      | Executive summary; impacto em contratos                | Exec + Finance    |
| Anual           | Revisão de targets vs. mercado (BuildBuddy, JFrog)     | Exec              |

### 6.1 Critério para mudar SLO target

- Mudança requer ADR.
- Evidência: baseline medido ≥ 30 dias com novo target proposto.
- Aprovação: SRE Lead + Product Owner.
- Comunicação aos clientes enterprise com 30d antecedência (se target degradar).

### 6.2 Dashboards obrigatórios

- `DASH-SLO-CATALOG` — burn rates, budgets consumidos, trend 90d.
- `DASH-SLO-TENANT-<id>` — por enterprise tenant (com SLA contratual).
- `DASH-SLO-INCIDENTS` — quais SLOs afetados por incident recente (correlation).

---

## 7. Referências

- **Google SRE Workbook** — capítulos *Alerting on SLOs* e *Implementing SLOs*.
- **SRE Book** Ch. 4 (Service Level Objectives).
- **OpenSLO** — spec format.
- **Datadog SLO** platform.
- **Nobl9** — SLO-driven alerting reference.
- **AWS Well-Architected Reliability Pillar**.

### 7.1 Concorrentes

- **BuildBuddy** — publica SLOs transparentemente (referência).
- **NativeLink** — self-hosted (cliente define SLO).
- **JFrog** — SLA contratual em planos enterprise; público menos.

---

**Fim de SLO-CATALOG.** Mudanças de SLO target que degradem: comunicação + ADR; que melhorem: bump version + ADR + re-baseline.
