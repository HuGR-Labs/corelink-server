# Production Readiness Review (PRR) — {{Feature / WI}}

> **Template Version:** 1.1.0

```yaml
---
id: PRR-SXX-NNN
type: prr
doc_status: DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
work_status: NOT_STARTED | IN_REVIEW | CONDITIONALLY_APPROVED | APPROVED | REJECTED
version: 1.0.0
created: YYYY-MM-DD
updated: YYYY-MM-DD
owner: {{Nome}}
final_approver: {{Nome}}
reviewers:
  - role: sre_lead
    name: {{Nome}}
  - role: security_lead
    name: {{Nome}}
  - role: privacy_lead
    name: {{Nome}}
  - role: product_lead
    name: {{Nome}}
  - role: architect
    name: {{Nome}}
  - role: finance
    name: {{Nome}}
  - role: legal
    name: {{Nome}}  # se §19 aplicável
feature_wi: WI-SXX-NNN
capabilities: [CAP-XXX]
prod_target_date: YYYY-MM-DD
supersedes: null
superseded_by: null
tags: [{{subsistema}}]
---
```

> **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
> **work_status:** NOT_STARTED | IN_REVIEW | CONDITIONALLY_APPROVED | APPROVED | REJECTED
> **Versão:** 1.0.0
> **Última atualização:** YYYY-MM-DD
> **Feature / WI:** WI-SXX-NNN / capability CAP-XXX
> **Produção target date:** YYYY-MM-DD

> **CONTRATO INVIOLÁVEL:**
>
> PRR é **gate separado** do DoD do WI. DoD confirma que WI está implementado corretamente. PRR confirma que WI/feature está **pronta pra enfrentar customers em produção** com a resiliência, observability, reversibilidade e suporte necessários.
>
> **Regra de rollout por `work_status` (inviolável):**
>
> | `work_status` | Rollout permitido | Observações |
> |---|---|---|
> | `NOT_STARTED` / `IN_REVIEW` / `REJECTED` | **Nenhum** | PRR incompleto ou reprovado |
> | `CONDITIONALLY_APPROVED` | **Somente canary ≤ 10%** | Deploy maior **proibido** até `APPROVED`. Caveats em §20 com **expiration-date obrigatória**. Expiração força re-review (retorna a `IN_REVIEW` automaticamente). |
> | `APPROVED` | Qualquer fase, incluindo GA (100%) | Sem caveats pendentes. |
>
> `CONDITIONALLY_APPROVED` **NÃO É** deploy livre com bilhete. É gate condicional que exige expansão para `APPROVED` antes de qualquer rollout > 10%.

---

## Legenda de tags

| Role | Automation |
|---|---|
| 🔧 ENG, 🔒 SEC, 🔏 PRIV, 📊 SRE, 🎨 PROD, ✓ QA, 🏛 ARCH, 💰 FIN, 📜 LEGAL | 🤖 automatizado / 👤 manual / 🤖👤 híbrido |

---

## Sumário

0. [Identificação](#0-identificação)
1. [Executive Summary](#1-executive-summary)
2. [Scope](#2-scope)
3. [Customer Impact & Rollout Plan](#3-customer-impact--rollout-plan)
4. [SLO / SLI / Error Budget](#4-slo--sli--error-budget)
5. [Capacity Planning](#5-capacity-planning)
6. [Load Testing](#6-load-testing)
7. [Chaos Engineering & Failure Readiness](#7-chaos-engineering--failure-readiness)
8. [Disaster Recovery](#8-disaster-recovery)
9. [Observability Readiness](#9-observability-readiness)
10. [Runbooks & On-call Readiness](#10-runbooks--on-call-readiness)
11. [Security & Compliance Readiness](#11-security--compliance-readiness)
12. [Privacy Readiness](#12-privacy-readiness)
13. [Supply Chain & Artifacts](#13-supply-chain--artifacts)
14. [Rollback & Kill-switch](#14-rollback--kill-switch)
15. [Deployment Strategy](#15-deployment-strategy)
16. [Customer Support Readiness](#16-customer-support-readiness)
17. [Communication Plan](#17-communication-plan)
18. [Cost & Finance Readiness](#18-cost--finance-readiness)
19. [Legal & Contract Readiness](#19-legal--contract-readiness)
20. [Open Issues & Caveats](#20-open-issues--caveats)
21. [Sign-off](#21-sign-off)
22. [Change Log](#22-change-log)

---

## 0. Identificação

| Field | Value |
|---|---|
| **PRR ID** | PRR-SXX-NNN |
| **Feature / WI** | {{WI-SXX-NNN}} |
| **Capability** | CAP-XXX |
| **Versão PRR** | 1.0.0 |
| **Status** | NOT_STARTED |
| **Data proposta** | YYYY-MM-DD |
| **Data target GA** | YYYY-MM-DD |
| **Owner** | {{Nome}} |
| **Reviewers** | SRE: {{Nome}}; SEC: {{Nome}}; PROD: {{Nome}}; ARCH: {{Nome}} |
| **Aprovador Final** | {{Nome}} |

---

## 1. Executive Summary

> *(3–5 sentenças. Para stakeholder não-técnico. Responder: O que vai pra prod, para quem, quando, com que risco principal.)*

**Exemplo:**
> Ativar `CAS::FindMissingBlobs` em produção via canary 5% → 50% → 100% em 2 semanas, atendendo todos os tenants Paid. Esperamos redução de 30% em latência p99 de first-build. Principal risco: saturação de R2 em picos; mitigada por rate limit + circuit breaker validado em chaos testing.

---

## 2. Scope

### 2.1 O que está em produção após aprovação

- {{feature}}
- {{endpoint}}
- {{componente}}

### 2.2 Tenants / Regiões / Tiers afetados

| Dimensão | Inicial (canary) | Final (GA) |
|---|---|---|
| Tiers | Free only (5%) | Todos |
| Regiões | GRU primeiro | GRU + IAD + EU |
| Tenants | Opt-in | Default-on |

### 2.3 Fora do escopo

- {{item NOT in this release}}

---

## 3. Customer Impact & Rollout Plan

### 3.1 Expected customer-visible impact

| Métrica | Baseline | Alvo | Janela |
|---|---|---|---|
| Latência p99 | {{X}}ms | {{Y}}ms (↓{{%}}) | rolling 7d |
| Cache hit rate | {{X}}% | {{Y}}% (↑{{%}}) | rolling 7d |
| Error rate | {{X}}% | ≤{{Y}}% (≈same) | rolling 7d |

### 3.2 Rollout phases

| Fase | % traffic / tenants | Duração mínima | Critério pra próxima fase |
|---|---|---|---|
| Canary | 1% | 48h | zero SEV-1/2, error rate ≤ baseline + 10% |
| Expansion | 10% | 72h | métricas estáveis, zero regressão de SLO |
| Broad rollout | 50% | 72h | idem |
| GA | 100% | N/A | idem |

### 3.3 Rollout checklist

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 3.3.1 | Feature flag configurada e testada | 👤 | 🔧 | flag service screenshot |
| 3.3.2 | Rollout pode ser pausado a qualquer momento | 👤 | 📊 | dry-run pause evidence |
| 3.3.3 | Rollout pode ser revertido em ≤ 10 min | 👤 | 📊 | dry-run rollback time |
| 3.3.4 | Métricas de cada fase monitoradas em dashboard dedicado | 🤖 | 📊 | dashboard URL |
| 3.3.5 | Critérios de promoção / abort automáticos | 👤 | 📊 | policy doc |

### 3.4 Stop conditions (abort rollout)

- Error rate > baseline + 20%
- Latência p99 > baseline + 30%
- SEV-1 incident
- Violação de tenant isolation
- Customer complaints > {{N}} em janela de {{T}}

---

## 4. SLO / SLI / Error Budget

### 4.1 SLIs aplicáveis

| SLI-ID | Definition | Measurement source |
|---|---|---|
| SLI-001 | `rate(cas_find_missing_success_total[5m]) / rate(cas_find_missing_total[5m])` | Prometheus |
| SLI-002 | `histogram_quantile(0.99, cas_find_missing_duration_seconds)` | Prometheus |

### 4.2 SLOs alvo

| SLO-ID | SLI | Target | Janela | Error Budget |
|---|---|---|---|---|
| SLO-001 | availability (SLI-001) | ≥ 99.9% | rolling 30d | 43.2 min/mês |
| SLO-002 | latency (SLI-002) | ≤ 100ms p99 | rolling 30d | {{calc}} |

### 4.3 Burn rate alerts configurados

| Alerta | Burn rate threshold | Window | Runbook |
|---|---|---|---|
| `CoreLinkFindMissingFastBurn` | 14.4× burn em 1h | 1h | rb-cas-slo-burn.md |
| `CoreLinkFindMissingSlowBurn` | 6× burn em 6h | 6h | rb-cas-slo-burn.md |

### 4.4 Error Budget Policy

- [ ] Quando error budget 50% queimado → incidente de ops aberto
- [ ] Quando error budget 100% queimado → **feature freeze** até SLO restaurado

### 4.5 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 4.5.1 | SLIs implementados e emitindo | 🤖 | 📊 | Grafana |
| 4.5.2 | SLOs documentados em `slo_catalog.md` | 👤 | 📊 | diff |
| 4.5.3 | Burn rate alerts configurados e testados | 🤖👤 | 📊 | synthetic test log |
| 4.5.4 | Error budget policy documentada | 👤 | 📊 | link policy |

---

## 5. Capacity Planning

### 5.1 Expected load profile

| Dimensão | Valor |
|---|---|
| Peak RPS esperado | {{N}} |
| Average RPS esperado | {{M}} |
| Diurnal pattern | {{descrição}} |
| Burst multiplier | {{X}}× peak em janela de Y seg |
| Request size distribuição | {{estatística}} |
| Growth forecast 6 meses | {{%}} |

### 5.2 Capacidade provisionada

| Componente | Dimensionamento | Justificativa |
|---|---|---|
| Cloudflare Containers | max_instances = {{N}} | suporta 3× peak |
| R2 ops budget | {{N}} ops/mês | limite soft $X |
| KV reads | {{N}}/mês | dentro free tier + margem |
| Postgres connections | {{N}} | pool config |

### 5.3 Cushion / headroom

- [ ] Provisionado ≥ 2× expected peak
- [ ] Headroom documentado e monitorado
- [ ] Alerta quando utilização > 70% provisionado

### 5.4 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 5.4.1 | Load profile estimado com base em dados | 👤 | 📊 | modelo + doc |
| 5.4.2 | Capacidade confirma suportar ≥ 2× peak | 🤖 | 📊 | load test |
| 5.4.3 | Alerta de saturação configurado | 🤖 | 📊 | alert config |

---

## 6. Load Testing

### 6.1 Cenários testados

| Cenário | RPS | Duração | Resultado esperado |
|---|---|---|---|
| Steady state at peak | {{N}} | 1h | p99 ≤ SLO |
| Burst 3× peak | {{3N}} | 5 min | degradação graceful, sem crash |
| Sustained at 2× peak | {{2N}} | 24h | sem memory leak |
| Ramp from 0 → 2× peak | — | 30 min | SLOs preservados durante ramp |

### 6.2 Ferramentas

- `wrk`, `oha`, `vegeta`, ou `k6`

### 6.3 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 6.3.1 | Load test ≥ 2× peak executado | 🤖 | 📊 | report |
| 6.3.2 | Soak test 24h executado (memory stable) | 🤖 | 📊 | memory profile |
| 6.3.3 | Resultados documentados | 👤 | 📊 | `docs/load_tests/WI-SXX-NNN.md` |
| 6.3.4 | Regressões identificadas e endereçadas | 👤 | 🔧 | issues linked |

---

## 7. Chaos Engineering & Failure Readiness

### 7.1 Failure modes endereçados

| FM-ID | Nome | Testado em staging? | Resultado |
|---|---|---|---|
| FM-001 | R2 slow (500ms injected) | ✅ | Graceful, P99 200ms |
| FM-002 | R2 outage (503 100%) | ✅ | 503 retryable, no crash |
| FM-003 | KV outage | ✅ | Fallback a Postgres |
| FM-004 | Postgres connection pool exhaustion | ✅ | back-pressure ativado |
| FM-005 | Tenant abusivo (rate limit trigger) | ✅ | 429 por tenant |

### 7.2 Chaos experiments agendados recorrentemente

- [ ] Chaos suite roda mensalmente em staging
- [ ] Failure modes cobertos em game day trimestral

### 7.3 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.3.1 | Todos FM relevantes testados | 🤖 | 📊 | chaos reports |
| 7.3.2 | Graceful degradation confirmada | 🤖 | 📊 | test logs |
| 7.3.3 | Chaos incluído em CI/CD recurrente | 👤 | 📊 | schedule |

---

## 8. Disaster Recovery

### 8.1 RTO / RPO

| Métrica | Target |
|---|---|
| RTO (Recovery Time Objective) | ≤ {{X}} min |
| RPO (Recovery Point Objective) | ≤ {{Y}} min de data loss aceitável |

### 8.2 Backup strategy

| Dado | Backup | Retention | Verificação |
|---|---|---|---|
| Neon Postgres | point-in-time recovery | 14d | restore test mensal |
| R2 blobs | versioning / cross-region replication | per policy | periodic checksum |
| Audit logs | cold archive | 7 anos | annual restore test |

### 8.3 DR scenarios tested

| Cenário | Tested? | Resultado |
|---|---|---|
| Região primária (GRU) inteira fora | ✅ | failover para IAD em {{N}} min |
| Database corruption | ✅ | restore de point-in-time bem-sucedido |
| Storage loss parcial | ✅ | versioning recover |

### 8.4 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 8.4.1 | RTO/RPO documentados e acordados | 👤 | 📊 | policy doc |
| 8.4.2 | Backup strategy testada | 🤖👤 | 📊 | restore test log |
| 8.4.3 | DR runbook testado (game day) | 👤 | 📊 | game day report |

---

## 9. Observability Readiness

### 9.1 Métricas — Four Golden Signals

| Signal | Métrica | Dashboard | Alerta |
|---|---|---|---|
| Latency | `cas_find_missing_duration_seconds` | ✅ | ✅ |
| Traffic | `cas_find_missing_total` | ✅ | (auto-scale trigger) |
| Errors | `cas_find_missing_errors_total` | ✅ | ✅ |
| Saturation | CPU, memory, queue depth | ✅ | ✅ |

### 9.2 Logs

- [ ] Todos eventos importantes logados com campos obrigatórios
- [ ] Log aggregation backend (Grafana Loki / similar) configurado
- [ ] Log retention conforme policy

### 9.3 Traces

- [ ] Distributed tracing via OpenTelemetry ativo
- [ ] Sampling rate apropriada
- [ ] Trace backend (Grafana Tempo / similar) configurado

### 9.4 Dashboards

- [ ] Dashboard operacional (latência, throughput, error)
- [ ] Dashboard de negócio (customer-visible metrics)
- [ ] Dashboard de saturação (infra)
- [ ] Dashboard de SLO burn rate

### 9.5 Alertas

- [ ] Alertas seguem severity guidance (SEV-1/2/3/4)
- [ ] Cada alerta tem runbook linkado
- [ ] Alertas testados via synthetic firing

### 9.6 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 9.6.1 | Four Golden Signals emitindo | 🤖 | 📊 | dashboard |
| 9.6.2 | Dashboards criados e linkados | 👤 | 📊 | URLs |
| 9.6.3 | Alertas configurados com runbooks | 🤖👤 | 📊 | alerting rules |
| 9.6.4 | Synthetic alert test executado | 👤 | 📊 | ticket de teste |

---

## 10. Runbooks & On-call Readiness

### 10.1 Runbooks criados / atualizados

| Runbook | Status | Testado dry-run? |
|---|---|---|
| `rb-cas-high-latency.md` | ✅ | ✅ YYYY-MM-DD |
| `rb-cas-errors.md` | ✅ | ✅ |
| `rb-cas-slo-burn.md` | ✅ | ✅ |

### 10.2 On-call briefing

- [ ] Briefing presencial / async realizado
- [ ] Q&A documentado
- [ ] On-call sabe como verificar saúde, rollback, escalar

### 10.3 Escalation paths

| Tipo de problema | Primeira linha | Segunda linha | Tempo de escalação |
|---|---|---|---|
| Latency > SLO | On-call SRE | Tech lead | 15 min |
| Security incident | On-call SRE | 🔒 Security lead | imediato |
| Data loss suspeito | On-call SRE | Incident Commander | imediato |

### 10.4 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 10.4.1 | Todos alertas têm runbook linkado | 🤖 | 📊 | alerting config |
| 10.4.2 | Runbooks testados em dry-run | 👤 | 📊 | logs de execução |
| 10.4.3 | On-call briefed | 👤 | 📊 | ata de briefing |

---

## 11. Security & Compliance Readiness

### 11.1 Threat model atualizado

- [ ] `security_model.md` reflete mudanças
- [ ] STRIDE análise completa em `WI-SXX-NNN.md §26`
- [ ] Pen-testing agendado / executado (se feature de alto risco)

### 11.2 Controles operacionais

- [ ] AuthN / AuthZ testados em prod (smoke test)
- [ ] Rate limiting ativo por tenant
- [ ] Audit log emitindo e imutável
- [ ] Secrets em secrets manager (rotação testada)
- [ ] Headers HTTP seguros (HSTS, CSP, etc)
- [ ] TLS 1.3 enforced

### 11.3 Vuln scanning

- [ ] `cargo audit` clean no release candidate
- [ ] SAST scan clean
- [ ] Container image scan (se aplicável) clean
- [ ] DAST scan em endpoints HTTP clean

### 11.4 Compliance

| Regulação | Aplicável | Review feito? |
|---|---|---|
| GDPR | sim | ✅ |
| LGPD | sim | ✅ |
| SOC 2 controls | sim | ✅ |
| HIPAA (se BAA) | N/A ou ✅ | — |

### 11.5 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 11.5.1 | Security signoff | 👤 | 🔒 | sign |
| 11.5.2 | Vuln scans limpos | 🤖 | 🔒 | CI |
| 11.5.3 | Compliance matrix atualizada | 👤 | 📜 | diff |

---

## 12. Privacy Readiness

Aplicável se processa PII / dados regulados.

### 12.1 Controles

- [ ] Data classification aplicada
- [ ] Retention configurada e testada
- [ ] Audit log de acesso a dados pessoais
- [ ] LINDDUN análise completa
- [ ] Direitos titular (acesso, retificação, exclusão, portabilidade) funcionam end-to-end
- [ ] DPA atualizado se novo sub-processor

### 12.2 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 12.1.1 | Privacy signoff | 👤 | 🔏 | sign |
| 12.1.2 | Retention testada | 🤖 | 🔏 | test |
| 12.1.3 | Direito exclusão testado E2E | 🤖 | 🔏 | test log |

---

## 13. Supply Chain & Artifacts

> **Regra inviolável:** target SLSA do framework (§26.1 de `00_framework.md`) é **Level 3** para builds de produção até GA. Este PRR obriga esse target por faixa de rollout. Aceitar Level 2 como permanente viola o framework.

### 13.1 Requisito SLSA por faixa de rollout

| Faixa de rollout | SLSA Level mínimo | Justificativa |
|---|---|---|
| Canary ≤ 5% | ≥ 2 | Exposição limitada, ainda em validação |
| Canary > 5% até ≤ 10% | ≥ 2 | Idem (com CONDITIONALLY_APPROVED apenas) |
| Expansion 10–49% | **≥ 3** | Exposição material requer provenance completa |
| Broad rollout ≥ 50% | **3** | Gate para GA |
| GA (100%) | **3** | Obrigatório (framework §26.1) |

### 13.2 Build & artifacts

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 13.2.1 | Build reprodutível (hash comparison idêntico em 2 re-builds do mesmo commit) | 🤖 | 🔧 | CI log + hash diff |
| 13.2.2 | Binário assinado via `cosign` | 🤖 | 🔒 | signature verification log |
| 13.2.3 | SBOM gerado (formato SPDX 2.3+) | 🤖 | 🔒 | SBOM artifact URL |
| 13.2.4 | Dependencies pinned (`Cargo.lock` commitado, sem `*` ou ranges em paths críticos) | 🤖 | 🔒 | `Cargo.lock` diff |
| 13.2.5 | Provenance attestation conforme faixa §13.1 (SLSA Level 2 ou 3) | 🤖 | 🔒 | attestation artifact |

### 13.3 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 13.3.1 | SBOM publicado e acessível | 🤖 | 🔒 | URL |
| 13.3.2 | Binary signature válida (re-verificada em pipeline) | 🤖 | 🔒 | verification log |
| 13.3.3 | Ausência de dependências com vuln `High`/`Critical` sem waiver | 🤖 | 🔒 | `cargo audit` + `cargo deny` clean |
| 13.3.4 | Se faixa ≥ 50%: SLSA Level 3 atestado | 🤖👤 | 🔒 | attestation + human audit |

---

## 14. Rollback & Kill-switch

### 14.1 Rollback plan

| Cenário | Trigger | Ação | MTTR estimado |
|---|---|---|---|
| Deploy quebra prod | error rate spike | `wrangler rollback` | ≤ 5 min |
| Migration corrompe dados | data integrity alert | Neon PITR | ≤ 30 min |
| Tenant isolation violada | automated check detecta | kill-switch feature flag | ≤ 1 min |

### 14.2 Kill-switches disponíveis

| Switch | Efeito | Quem pode acionar |
|---|---|---|
| `feature.cas_find_missing_v2` | desliga feature, fallback a v1 | on-call via feature flag dashboard |

### 14.3 Rollback testing

- [ ] Rollback exercitado em staging
- [ ] MTTR medido e documentado
- [ ] Data migration reversível ou PITR tested

### 14.4 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 14.4.1 | Rollback testado | 👤 | 📊 | log |
| 14.4.2 | Kill-switch accessível pro on-call | 👤 | 📊 | config |

---

## 15. Deployment Strategy

### 15.1 Strategy

- [ ] Canary (default)
- [ ] Blue-green
- [ ] Rolling
- [ ] Shadow traffic

### 15.2 Promotion criteria entre fases

| De | Para | Critério automático |
|---|---|---|
| Canary 1% | Canary 5% | 48h estável, SLO preservado |
| Canary 5% | 10% | 48h |
| 10% | 50% | 72h |
| 50% | 100% | 72h |

### 15.3 Automated rollback triggers

- Error rate > baseline + 20% por 5 min
- Latência p99 > baseline + 50% por 5 min
- SLO burn rate > 14.4× por 1h

### 15.4 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 15.4.1 | Progressive delivery configurado | 👤 | 📊 | config |
| 15.4.2 | Auto-rollback triggers testados | 🤖 | 📊 | chaos test |

---

## 16. Customer Support Readiness

- [ ] Help Center articles criados/atualizados
- [ ] CS team briefing realizado
- [ ] Troubleshooting guide para CS
- [ ] Escalation path CS → Eng documentado

### 16.1 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 16.1.1 | Help center articles publicados | 👤 | 🎨 | URLs |
| 16.1.2 | CS briefing feito | 👤 | 🎨 | ata |

---

## 17. Communication Plan

### 17.1 External (customers)

- [ ] Changelog público
- [ ] Email announcement (se material)
- [ ] In-app notification
- [ ] Social / blog post (se launch marcante)

### 17.2 Internal (team)

- [ ] Anúncio interno em canal principal
- [ ] All-hands mention (se release major)
- [ ] Docs hub atualizado

### 17.3 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 17.3.1 | Communication timeline aprovada | 👤 | 🎨 | plan doc |

---

## 18. Cost & Finance Readiness

### 18.1 Cost model confirmado

| Item | Delta mensal esperado | Confirmado por 💰 FIN? |
|---|---|---|
| R2 operations | +$X | ✅ |
| Compute | +$Y | ✅ |
| KV | +$Z | ✅ |

### 18.2 Cost alerts configurados

- [ ] Alert quando delta > 120% do esperado
- [ ] Alert quando custo absoluto > threshold

### 18.3 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 18.3.1 | Cost model aprovado por 💰 FIN | 👤 | 💰 | sign |
| 18.3.2 | Cost alerts ativos | 🤖 | 💰 | config |

---

## 19. Legal & Contract Readiness

Aplicável se muda contratos, DPA, ou introduz sub-processor.

- [ ] N/A — sem impacto legal/contratual
- [ ] Aplicável — completar:

### 19.1 Review gates

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 19.1.1 | DPA atualizado (se novo sub-processor) | 👤 | 📜 | doc |
| 19.1.2 | ToS/SLA revisados se mudança de compromisso | 👤 | 📜 | doc |
| 19.1.3 | Sign-off legal | 👤 | 📜 | sign |

---

## 20. Open Issues & Caveats

> Issues conhecidas não-bloqueantes que justificam `CONDITIONALLY_APPROVED` em vez de `APPROVED`. **Cada caveat DEVE ter `expires_at`** — sem exceção. Ao expirar, PRR retorna automaticamente para `IN_REVIEW`.

| # | Issue | Severity | Owner | Compensating control | Expires at | Automated re-review trigger | Risk aceito? |
|---|---|---|---|---|---|---|---|
| — | — | LOW/MED/HIGH | Nome | Qual proteção temporária existe | YYYY-MM-DD | Event/alert que força re-review antes de expires_at | sim/não |

### 20.1 Enforcement automático de expiração

- [ ] Cada caveat tem entrada no sistema de calendário/tracking (ex: `tickets/prr-caveats/{{PRR-ID}}-{{#}}.yaml`).
- [ ] Job periódico (diário) verifica `expires_at`; se expirado e caveat não resolvido:
  - `work_status` do PRR vira `IN_REVIEW` automaticamente.
  - Rollout > 10% é bloqueado automaticamente via feature flag policy.
  - On-call de produto recebe notification.
- [ ] Caveats resolvidos movem para §20.2 (resolução histórica) e não contam para validação.

### 20.2 Resolução histórica de caveats

| # | Issue | Resolvido em | Como | Assinado por |
|---|---|---|---|---|
| — | — | YYYY-MM-DD | — | — |

---

## 21. Sign-off

| Papel | Nome | Critério | Assinatura | Data |
|---|---|---|---|---|
| 📊 **SRE Lead** | {{Nome}} | §4 + §5 + §6 + §7 + §8 + §9 + §10 + §14 + §15 ✅ | _____________ | YYYY-MM-DD |
| 🔒 **Security Lead** | {{Nome}} | §11 + §13 ✅ | _____________ | YYYY-MM-DD |
| 🔏 **Privacy Lead** | {{Nome}} | §12 ✅ (se aplicável) | _____________ | YYYY-MM-DD |
| 🎨 **Product Lead** | {{Nome}} | §3 + §16 + §17 ✅ | _____________ | YYYY-MM-DD |
| 🏛 **Architect** | {{Nome}} | arquitetura coerente com ADRs | _____________ | YYYY-MM-DD |
| 💰 **Finance** | {{Nome}} | §18 ✅ | _____________ | YYYY-MM-DD |
| 📜 **Legal** (se aplicável) | {{Nome}} | §19 ✅ | _____________ | YYYY-MM-DD |
| **PRR Owner** | {{Nome}} | PRR completa e documentada | _____________ | YYYY-MM-DD |
| **Aprovador Final** | {{Nome}} | **pode ir a produção** | _____________ | YYYY-MM-DD |

### Decisão final

- [ ] **APPROVED** — pode promover a produção conforme plan §3 **em qualquer fase, incluindo GA (100%)**
- [ ] **CONDITIONALLY_APPROVED** — rollout **permitido APENAS até canary ≤ 10%**. Caveats em §20 com `expires_at` obrigatória. Promoção para fase > 10% **proibida** até PRR transicionar para `APPROVED`.
- [ ] **REJECTED** — não pode promover; razões registradas abaixo

### Razões de rejeição (se REJECTED)

> {{...}}

---

## 22. Change Log

| Versão | Data | Autor | Estado | Mudança |
|---|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | NOT_STARTED | PRR criada |
| 1.0.1 | YYYY-MM-DD | {{Nome}} | IN_REVIEW | capacity section completa |
| 1.1.0 | YYYY-MM-DD | {{Nome}} | CONDITIONALLY_APPROVED | aprovado com caveats §20 |
| 2.0.0 | YYYY-MM-DD | {{Nome}} | APPROVED | todos caveats resolvidos |

---

**Fim de PRR-SXX-NNN.**

---

### Notas para autores

- **Separação DoD vs PRR**: DoD é "WI implementado correto". PRR é "pronto pra prod". Dois gates distintos.
- **Não pular**: features sem PRR `APPROVED` NÃO PODEM ser default-on para customers.
- **CONDITIONALLY_APPROVED** é ferramenta válida mas com prazo duro; não vira crônico.
- **Adversarial review**: PRR beneficia de red team (§38 framework) — uma pessoa tentando encontrar buracos antes da prod encontrar.
