# Sprint Contract — S-XX: {{Nome do Sprint}}

> **Template Version:** 1.1.0

```yaml
---
id: S-XX
type: sprint
doc_status: DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
work_status: PROPOSED | READY | IN_PROGRESS | REVIEWING | COMPLETE | SEALED | FAILED
version: 1.0.0
created: YYYY-MM-DD
updated: YYYY-MM-DD
owner: {{Sprint Owner}}
final_approver: {{Nome}}
reviewers:
  - role: tech_lead
    name: {{Nome}}
  - role: product
    name: {{Nome}}
  - role: security
    name: {{Nome}}
  - role: ops
    name: {{Nome}}
  - role: qa
    name: {{Nome}}
predecessor_sprints: [S-XX, S-XX]
successor_sprints: [S-XX]
related_adrs: [ADR-XXXX]
supersedes: null
superseded_by: null
tags: [{{subsistema}}]
---
```

> **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
> **work_status:** PROPOSED | READY | IN_PROGRESS | REVIEWING | COMPLETE | SEALED | FAILED
> **Versão:** 1.0.0
> **Última atualização:** YYYY-MM-DD
> **Owner:** {{Nome}}
> **Aprovador Final:** {{Nome}}

> **AVISO INVIOLÁVEL:** Este sprint só pode transicionar para `COMPLETE` quando **TODAS** as caixas de **Exit Criteria (§7)** e **Completeness Checklist (§17)** estiverem ✅. Não existe "parcialmente completo". Não existe "falta só X, fechamos sprint mesmo assim". Sprint que não cumpre integralmente **NÃO** passa — é estendido, decomposto ou declarado falho com aprendizado registrado.

---

## Sumário

0. [Identificação e Metadata](#0-identificação-e-metadata)
1. [Executive Summary](#1-executive-summary)
2. [Intent](#2-intent)
3. [Capability Mapping](#3-capability-mapping)
4. [Status do Sprint](#4-status-do-sprint)
5. [Entry Criteria (Definition of Ready)](#5-entry-criteria-definition-of-ready)
6. [Work Items](#6-work-items)
7. [Exit Criteria (Definition of Done)](#7-exit-criteria-definition-of-done)
8. [Invariants Operacionais Durante o Sprint](#8-invariants-operacionais-durante-o-sprint)
9. [Test Strategy](#9-test-strategy)
10. [Risk Register](#10-risk-register)
11. [Artifacts Produced](#11-artifacts-produced)
12. [Anti-Scope](#12-anti-scope)
13. [Dependências](#13-dependências)
14. [Observability Plan](#14-observability-plan)
15. [Rollback / Recovery Plan](#15-rollback--recovery-plan)
16. [Security & Compliance Plan](#16-security--compliance-plan)
17. [Completeness Checklist (SOTA)](#17-completeness-checklist-sota)
18. [Traceability Matrix](#18-traceability-matrix)
19. [Review Checkpoints](#19-review-checkpoints)
20. [Sign-off](#20-sign-off)
21. [Retrospective (post-COMPLETE)](#21-retrospective-post-complete)
22. [Change Log](#22-change-log)

---

## 0. Identificação e Metadata

| Field | Value |
|---|---|
| **Sprint ID** | S-XX |
| **Nome** | {{nome curto e descritivo}} |
| **Versão do contrato** | 1.0.0 |
| **Status** | PROPOSED |
| **Duração planejada** | {{X semanas / Y dias úteis}} |
| **Data de início (planejada)** | YYYY-MM-DD |
| **Data de fim (planejada)** | YYYY-MM-DD |
| **Data de início (real)** | YYYY-MM-DD |
| **Data de fim (real)** | YYYY-MM-DD |
| **Sprint Owner** | {{Nome}} |
| **Tech Lead Reviewer** | {{Nome}} |
| **Product Reviewer** | {{Nome}} |
| **Security Reviewer** | {{Nome}} |
| **Ops Reviewer** | {{Nome}} |
| **QA Reviewer** | {{Nome}} |
| **Aprovador Final** | {{Nome}} |
| **Sprint(s) predecessor(es)** | S-XX, S-XX (devem estar `SEALED`) |
| **Sprint(s) sucessor(es) imediato(s)** | S-XX (este sprint desbloqueia) |
| **ADRs relacionados** | ADR-XXXX, ADR-XXXX |
| **Specs upstream consumidas** | `02_product/capabilities.md`, `03_architecture/protocols/reapi.md`, etc |
| **Tags** | {{subsistema, tipo, prioridade}} |
| **Repositório** | corelink-server |
| **Branch** | sprint/S-XX-{{slug}} |
| **PR de tracking** | {{URL quando criado}} |

---

## 1. Executive Summary

> *(3 a 5 sentenças. Linguagem acessível a stakeholder não-técnico. Responder: o que entrega, para qual persona, qual valor, que risco principal mitiga ou cria.)*

**Exemplo:**
> Este sprint entrega o servidor REAPI v2 mínimo (CAS + Action Cache + Capabilities + ByteStream) sobre Cloudflare Containers e R2, com isolamento por *tenant* via Clerk. Habilita o caso de uso de cache de build do Bazel/Buck2/sccache para *single-tenant*. Não cobre ainda: pacotes (npm/PyPI/Cargo), eventos em tempo real, dashboard, multi-region. Risco principal: maturidade de Cloudflare Containers como plataforma — mitigado por *fallback plan* documentado para Fly.io.

---

## 2. Intent

> *(UMA frase declarativa. Capability principal entregue.)*

**Exemplo:**
> Implementar e operar em produção o servidor REAPI v2 com isolamento de *tenant* via Clerk, suportando Bazel/Buck2/sccache contra storage Cloudflare R2.

---

## 3. Capability Mapping

| CAP-ID | Status entregue neste sprint | Escopo coberto | Escopo restante | Verificação |
|---|---|---|---|---|
| CAP-XXX | FULL | Implementação completa | — | `tests/cap_xxx_acceptance.rs` |
| CAP-YYY | PARTIAL | Subseções A, B | Subseção C → S-ZZ | `tests/cap_yyy_partial_a_b.rs` |
| CAP-ZZZ | FOUNDATION | Infra base habilitando CAP futuras | Implementação user-facing → S-WW | `tests/cap_zzz_foundation.rs` |

**Legenda de Status:**
- `FULL` — capability entregue 100%, sem subset.
- `PARTIAL` — subset declarado da capability; resto rastreado em sprint(s) futuro(s).
- `FOUNDATION` — infraestrutura/scaffolding necessário para a capability, sem entrega user-facing ainda.

---

## 4. Status do Sprint

| Estado | Quando atinge | Quem autoriza |
|---|---|---|
| `PROPOSED` | Documento criado | Sprint Owner |
| `READY` | DoR §5 100% ✅ | Sprint Owner + Tech Lead |
| `IN_PROGRESS` | Trabalho iniciado | Sprint Owner |
| `REVIEWING` | DoD §7 em verificação | Sprint Owner |
| `COMPLETE` | DoD §7 100% ✅ | Aprovador Final |
| `SEALED` | Sign-off §20 100% ✅; sprint imutável | Aprovador Final |
| `FAILED` | Sprint não cumpriu critérios e foi declarado falho | Aprovador Final |

**Transição corrente:** {{atual estado → próximo}}

**Histórico de transições:**
| De | Para | Data | Quem |
|---|---|---|---|
| — | PROPOSED | YYYY-MM-DD | {{Nome}} |

---

## 5. Entry Criteria (Definition of Ready)

> **REGRA INVIOLÁVEL:** Sprint **NÃO PODE** transicionar de `PROPOSED → READY` enquanto qualquer item desta seção estiver `❌` ou `?`. Cada item é binário e tem método de verificação explícito.

### 5.1 Upstream Artifacts Frozen

| Item | Status | Verificação |
|---|---|---|
| Spec(s) de Nível 2 referenciada(s) por este sprint estão `FROZEN` | ❌ | `git log specs/02_product/capabilities.md` mostra commit com tag `frozen-vX` |
| Todos ADRs referenciados estão `ACCEPTED` | ❌ | Header de cada ADR mostra `Status: ACCEPTED` |
| Sprint(s) predecessor(es) estão `SEALED` | ❌ | Header de cada predecessor mostra `Status: SEALED` |

### 5.2 Team Readiness

| Item | Status | Verificação |
|---|---|---|
| Sprint Owner nomeado e disponível pela duração | ❌ | Calendário confirmado |
| Todos revisores requeridos (Tech, Product, Security, Ops, QA) confirmaram disponibilidade | ❌ | Sign-off §20 tem nomes preenchidos |
| Skills críticos presentes no time (listar): {{skills}} | ❌ | Roster check |
| Aprovador Final disponível para sign-off | ❌ | Calendário confirmado |

### 5.3 Environmental Readiness

| Item | Status | Verificação |
|---|---|---|
| Cloud accounts provisionados (Cloudflare, Neon, Clerk, Stripe) | ❌ | Login bem-sucedido em cada console |
| Secrets necessários disponíveis em ambiente dev | ❌ | `wrangler secret list` mostra todos |
| Dev environment reproduzível em <30 min | ❌ | `make dev-setup` rodado em máquina limpa |
| CI/CD configurado e verde no `main` | ❌ | GitHub Actions verde |
| Branch criada: `sprint/S-XX-{{slug}}` | ❌ | `git branch -a` |

### 5.4 Information Readiness

| Item | Status | Verificação |
|---|---|---|
| Toda CAP entregue por este sprint tem spec `FROZEN` | ❌ | Verificação manual + trace checker |
| Todo invariante relevante está enunciado em `02_product/invariants.md` | ❌ | Verificação manual |
| Toda NFR aplicável está enunciada em `02_product/quality_attributes.md` | ❌ | Verificação manual |
| Dependências técnicas externas identificadas | ❌ | §13 preenchida |
| Threat model do escopo deste sprint revisado | ❌ | §16 preenchida + assinatura Security Reviewer |

### 5.5 Legal/Compliance Readiness (se aplicável)

| Item | Status | Verificação |
|---|---|---|
| Data handling do escopo revisado | ❌ ou N/A | Aprovação documentada |
| Vendor agreements em ordem (se vendors novos) | ❌ ou N/A | Contratos arquivados |
| Privacy Impact Assessment se há novas categorias de dado | ❌ ou N/A | PIA arquivada |
| Conformidade com classificações existentes (PII, regulado) | ❌ ou N/A | Documentado |

### 5.6 Quality Foundation Readiness

| Item | Status | Verificação |
|---|---|---|
| Test infrastructure operacional (unit, integration, property-based) | ❌ | `cargo test` verde |
| Coverage tooling configurado (`cargo-llvm-cov`) | ❌ | Coverage report gerado |
| Property-based testing crate disponível (`proptest` ou `quickcheck`) | ❌ | `cargo build` ok |
| Mutation testing tooling (`cargo-mutants`) — se aplicável | ❌ ou N/A | Tooling instalado |
| Performance benchmarking (`criterion`) configurado | ❌ | Bench rodável |
| Observability stack acessível (Grafana Cloud, Sentry) | ❌ | Login + dashboard existe |

### 5.7 Decisão de transição

- [ ] Total de itens nesta seção: {{N}}
- [ ] Itens ✅: {{N}}
- [ ] Itens ❌: {{N}}
- [ ] **Aprovação para `READY`:** ❌ (só fica ✅ quando todos os itens acima estão ✅)

---

## 6. Work Items

> **REGRA INVIOLÁVEL:** Cada *Work Item* (WI) é um **contrato próprio** com rigor SOTA. WI **NÃO DEVE** ser escrito inline no sprint contract exceto em forma-sumário; **DEVE** existir como arquivo separado seguindo o template canônico `_templates/work_item.md` (33 seções totais §0–§32, incluindo Completeness Criteria, DoD, Invariants, Quality Standards).
>
> Sub-tasks seguem o template `_templates/subtask.md` (19 seções totais §0–§18), com rigor proporcional ao escopo atômico.

### 6.1 Relação hierárquica

```
Sprint (S-XX)
  └── Work Item (WI-SXX-NNN)               ← template: work_item.md
        ├── Sub-task (ST-001)              ← template: subtask.md
        ├── Sub-task (ST-002)
        └── Sub-task (ST-003)
```

Cada nível tem contrato próprio. Cada ST é verificável independentemente.

### 6.2 Localização dos arquivos

Para este sprint:
- WIs em: `04_sprints/SXX/work_items/WI-SXX-NNN.md`
- Sub-tasks em: `04_sprints/SXX/work_items/WI-SXX-NNN/subtasks/ST-MMM.md`
  *(ou instanciados inline dentro do arquivo do WI se sub-tasks ≤ 5)*

### 6.3 Tabela-sumário dos WIs deste sprint

Preencher como índice. Detalhes vivem nos arquivos individuais.

| WI-ID | Título | Tipo | Assignee | Tamanho | Status | Trace (CAP/INV) | Link |
|---|---|---|---|---|---|---|---|
| WI-SXX-001 | {{título}} | feature | {{Nome}} | M | TODO | CAP-XXX | [→](work_items/WI-SXX-001.md) |
| WI-SXX-002 | {{título}} | infra | {{Nome}} | S | TODO | CAP-YYY, INV-ZZZ | [→](work_items/WI-SXX-002.md) |

### 6.4 Regras de decomposição

- **REG-WI-001**: WI de tamanho XL **DEVE** ser decomposto em sub-tasks ou sub-WIs.
- **REG-WI-002**: Sub-task atômica é unidade que **pode ser trabalhada em paralelo** com outra sub-task do mesmo WI e tem *acceptance* independente. Se é sequencial-indivisível, é *step*, não sub-task.
- **REG-WI-003**: Todo WI **DEVE** ter ao menos 1 sub-task, exceto WIs XS (≤ 2h) que podem ser trabalhados como unidade única.
- **REG-WI-004**: Nenhum WI **PODE** transicionar para `DONE` enquanto qualquer sub-task não estiver `DONE`.
- **REG-WI-005**: Todo WI e toda ST **DEVEM** ter DoD, Completeness Criteria, Invariants (operacionais + os que preserva), Quality Standards — conforme templates canônicos.

### 6.5 Progresso agregado

| Métrica | Valor |
|---|---|
| Total WIs | {{N}} |
| WIs `DONE` | {{N}} |
| WIs `DOING` | {{N}} |
| WIs `BLOCKED` | {{N}} |
| Total sub-tasks agregadas | {{N}} |
| Sub-tasks `DONE` | {{N}} |
| % sprint completo (pelas caixas ✅) | {{N}}% |

---

## 7. Exit Criteria (Definition of Done)

> **REGRA INVIOLÁVEL:** Sprint **NÃO PODE** transicionar para `COMPLETE` enquanto qualquer item desta seção estiver `❌`. Sem exceções.

### 7.1 Funcional

| Item | Status | Verificação |
|---|---|---|
| Todas CAPs em §3 com status `FULL` ou `PARTIAL` foram entregues conforme escopo declarado | ❌ | Acceptance test correspondente verde |
| Acceptance criteria de todos os WIs em §6 estão ✅ | ❌ | Inspeção do checklist |
| Nenhuma regressão em CAPs já entregues em sprints anteriores | ❌ | Regression suite verde |
| Toda nova CAP é descobrível via API discovery (gRPC reflection / OpenAPI) | ❌ | `grpcurl ls` ou `curl /openapi.json` |

### 7.2 Quality Gates — Testing

| Item | Status | Verificação | Threshold |
|---|---|---|---|
| Cobertura unit do código novo | ❌ | `cargo llvm-cov` | ≥ 85% |
| Cobertura unit total do projeto | ❌ | `cargo llvm-cov` | ≥ 80% |
| Mutation kill rate (paths críticos) | ❌ | `cargo-mutants` | ≥ 75% |
| Integration tests | ❌ | `cargo test --test '*'` | 100% pass |
| Property-based tests para todas INVs do escopo | ❌ | dedicated test files | 100% pass, 1000+ iterations |
| Contract tests REAPI (se aplicável) | ❌ | `bazel-remote-test` ou similar | 100% conformance v2.X |
| E2E tests dos *user journeys* afetados | ❌ | `tests/e2e/` | 100% pass |
| Testes deterministicamente reproduzíveis (sem flaky) | ❌ | 100 execuções consecutivas | 100% pass |

### 7.3 Quality Gates — Performance

| Item | Status | Verificação | SLO |
|---|---|---|---|
| Latência p50 de cada novo endpoint | ❌ | `criterion` benchmark | NFR-XXX |
| Latência p99 de cada novo endpoint | ❌ | `criterion` benchmark | NFR-XXX |
| Throughput sustentado | ❌ | Load test | NFR-XXX |
| Memória RSS sob carga | ❌ | Soak test 24h | NFR-XXX |
| Sem regressão >5% nos benchmarks de baseline | ❌ | Comparativo de bench | — |

### 7.4 Quality Gates — Security

| Item | Status | Verificação |
|---|---|---|
| Threat model atualizado para o novo escopo | ❌ | `specs/03_architecture/security_model.md` versão atualizada |
| SAST scan limpo (no findings High/Critical) | ❌ | CodeQL ou semgrep |
| DAST scan limpo (se endpoints novos) | ❌ | OWASP ZAP ou equivalente |
| Dependency vuln scan limpo (no High/Critical) | ❌ | `cargo audit` + `cargo deny` |
| Secrets rotation testada (se aplicável) | ❌ | Documentado |
| AuthN coverage: todo endpoint novo requer autenticação válida | ❌ | Test suite cobre paths sem auth |
| AuthZ coverage: tenant isolation testado em todos endpoints novos | ❌ | Test suite com 2 tenants |
| Input validation: fuzzing dos endpoints públicos | ❌ | `cargo fuzz` rodado, sem panics |

### 7.5 Quality Gates — Reliability

| Item | Status | Verificação |
|---|---|---|
| Chaos tests para novos failure modes | ❌ | `tests/chaos/` |
| Graceful degradation verificada (R2 down, DB down, etc.) | ❌ | Cenários documentados executados |
| Rate limits respeitados sob carga | ❌ | Load test de excesso |
| Circuit breakers testados | ❌ | Test cases dedicados |
| Timeouts configurados em toda chamada externa | ❌ | Code review + grep `.timeout(` |
| Idempotência verificada para operações relevantes | ❌ | Test de duplo-write |

### 7.6 Quality Gates — Observability

| Item | Status | Verificação |
|---|---|---|
| Toda métrica planejada em §14 sendo emitida | ❌ | Query no Grafana Cloud |
| Cardinality budget respeitado | ❌ | Análise de labels |
| Toda log estruturada com campos requeridos | ❌ | Sample inspection |
| Toda trace span criada nos pontos críticos | ❌ | Trace search no observability backend |
| Dashboards criados/atualizados | ❌ | URLs ativas |
| Alertas configurados com runbook link | ❌ | Lista §14 + verificação |
| Runbook(s) escrito(s) e testado(s) (dry-run) | ❌ | `runbooks/{nome}.md` existente e revisado |

### 7.7 Quality Gates — Documentation

| Item | Status | Verificação |
|---|---|---|
| Rustdoc 100% em itens públicos | ❌ | `cargo doc --no-deps` sem warnings |
| README atualizado refletindo estado real | ❌ | Diff |
| Architecture diagrams atualizados | ❌ | C4 diagrams refletem código |
| ADRs novos escritos para decisões tomadas | ❌ | `specs/03_architecture/adrs/` |
| CHANGELOG.md do servidor atualizado | ❌ | Entrada nova |
| API docs publicadas (gRPC reflection + OpenAPI) | ❌ | Endpoint `/docs` ou similar |
| Glossário atualizado com novos termos | ❌ | `specs/00_framework.md §40` |

### 7.8 Quality Gates — Compliance

| Item | Status | Verificação |
|---|---|---|
| Audit logs emitidos para operações sensíveis | ❌ | Test verifica entrada em audit log |
| Data classification aplicada a novos dados | ❌ | Schema documenta classificação |
| Retention policies aplicadas | ❌ | TTL configurado / cron de purge |
| Direito de exclusão (GDPR/LGPD) testado em fluxo novo | ❌ | E2E test de exclusão |
| Data residency respeitada se aplicável | ❌ | Test de routing |
| PII/PHI handling revisado | ❌ ou N/A | Sign-off Security Reviewer |

### 7.9 Operational Readiness

| Item | Status | Verificação |
|---|---|---|
| Deployed em ambiente dev e estável por ≥ 24h | ❌ | Logs + métricas |
| Deploy procedure 100% automatizado (zero manual steps) | ❌ | `wrangler deploy` ou pipeline equivalente |
| Rollback procedure 100% automatizado | ❌ | Procedure documentada e testada |
| On-call briefing realizado (se há mudança operacional) | ❌ | Sign-off ops reviewer |
| Feature flags configurados (se aplicável) | ❌ ou N/A | Documentado |
| Incident response plan atualizado se há novo failure mode | ❌ | Documento atualizado |

### 7.10 Decisão de transição

- [ ] Total de itens em §7: {{N}}
- [ ] Itens ✅: {{N}}
- [ ] Itens ❌: {{N}}
- [ ] **Aprovação para `COMPLETE`:** ❌ (só fica ✅ quando todos os itens acima estão ✅)

---

## 8. Invariants Operacionais Durante o Sprint

> Propriedades que **DEVEM** valer em todo momento durante e após o sprint. Diferente de invariants do produto (que são propriedades do sistema rodando), estas são invariants do *processo*.

### 8.1 Safety Invariants (nada ruim acontece)

| ID | Invariant | Verificação contínua |
|---|---|---|
| SPRINT-INV-001 | `main` branch sempre verde | CI status check obrigatório |
| SPRINT-INV-002 | Toda mudança chega via PR (nenhum push direto) | GitHub branch protection |
| SPRINT-INV-003 | Toda mudança em spec `FROZEN` requer thaw documentado | CI check |
| SPRINT-INV-004 | Nenhum secret commitado | git-secrets / pre-commit hook |
| SPRINT-INV-005 | Nenhum endpoint novo sem auth (verificável em PR diff) | Code review |

### 8.2 Liveness Invariants (algo bom eventualmente acontece)

| ID | Invariant | Verificação contínua |
|---|---|---|
| SPRINT-INV-006 | Todo PR resolvido (mergeado ou fechado) em ≤ 5 dias úteis | Check semanal |
| SPRINT-INV-007 | Todo bloqueador (status BLOCKED em §6) escalado em ≤ 24h | Standup ou async update |
| SPRINT-INV-008 | DoR/DoD checklist atualizado pelo menos diariamente | Inspeção |

---

## 9. Test Strategy

> Detalhamento por camada de teste para o escopo deste sprint.

### 9.1 Unit Tests

- **Coverage target:** ≥ 85% no código novo, ≥ 80% global.
- **Framework:** `cargo test`.
- **Localização:** `src/**/*.rs` com módulos `#[cfg(test)]`, ou `tests/unit/`.
- **Mutation testing:** `cargo-mutants` em paths críticos; threshold de kill rate ≥ 75%.
- **Determinismo:** zero flakies; teste reprovado em 100 execuções é falha.

### 9.2 Integration Tests

- **Localização:** `tests/integration/`.
- **Escopo:** boundaries entre módulos do servidor e dependências externas (R2, Postgres, Clerk).
- **Dependências externas:** mocks via `wiremock` ou containers efêmeros (testcontainers).

### 9.3 Property-Based Tests

- **Framework:** `proptest`.
- **Cobertura:** toda INV declarada em §8 e em `02_product/invariants.md` relevante ao sprint **DEVE** ter property test.
- **Iterations padrão:** ≥ 1000.
- **Estratégia de geradores:** documentada para cada teste.

### 9.4 Contract Tests

- **REAPI conformance:** `bazel build --remote_cache=...` rodando suite oficial Bazel; `bazel-remote-test` se disponível.
- **Outros protocolos:** suites próprias citando spec source.

### 9.5 End-to-End Tests

- **Localização:** `tests/e2e/`.
- **Escopo:** *user journeys* afetados pelo sprint (referência §3 + `02_product/user_journeys/`).
- **Ambiente:** ambiente staging (após deploy do sprint).

### 9.6 Chaos Tests

- **Localização:** `tests/chaos/`.
- **Escopo:** failure modes documentados em `03_architecture/failure_modes.md` que tocam o escopo do sprint.
- **Ferramentas:** `chaos-mesh` para infra, mocks de falha para deps externas.

### 9.7 Performance Tests

- **Framework:** `criterion`.
- **Escopo:** todo endpoint/função afetada pelo sprint.
- **SLOs validados:** todos NFRs aplicáveis em `02_product/quality_attributes.md`.

### 9.8 Security Tests

- **SAST:** CodeQL / semgrep configurados em CI.
- **Dependency scanning:** `cargo audit`, `cargo deny`.
- **DAST:** OWASP ZAP em endpoints HTTP novos (deploy em staging).
- **Fuzzing:** `cargo fuzz` em parsers/decoders novos.
- **Pen test:** fora do escopo de sprint individual; pré-requisito para `S-LANCAMENTO`.

### 9.9 Test Data Management

- **PII em testes:** **NÃO** usar dados reais; usar `faker-rs` ou fixtures sintéticas.
- **Cleanup:** todo teste **DEVE** cleanup seu estado (idempotência de re-execução).
- **Isolamento:** testes paralelos **NÃO DEVEM** compartilhar tenant_id.

---

## 10. Risk Register

| ID | Risco | Probabilidade | Impacto | Detecção | Mitigação | Contingência | Owner |
|---|---|---|---|---|---|---|---|
| R-001 | {{descrição}} | L \| M \| H | L \| M \| H | {{como descobrimos}} | {{ação preventiva}} | {{plano se materializar}} | {{Nome}} |

### Risk Score
- Probabilidade L=1, M=2, H=3
- Impacto L=1, M=2, H=3
- Score = P × I (1–9)

| Score | Ação |
|---|---|
| 1–2 | Aceitar |
| 3–4 | Mitigar |
| 6–9 | Mitigar + plano de contingência ativo + revisão semanal |

---

## 11. Artifacts Produced

### 11.1 Código

| Arquivo / Módulo | Propósito | Status |
|---|---|---|
| `src/{{path}}` | {{descrição}} | TODO \| DONE |

### 11.2 Testes

| Arquivo | Tipo | Status |
|---|---|---|
| `tests/{{path}}` | unit \| integration \| e2e \| property \| chaos | TODO \| DONE |

### 11.3 Documentação

| Arquivo | Tipo | Status |
|---|---|---|
| `specs/03_architecture/adrs/ADR-XXXX.md` | ADR | TODO \| DONE |
| `runbooks/{{nome}}.md` | Runbook | TODO \| DONE |
| `README.md` | atualizado | TODO \| DONE |

### 11.4 Infra-as-Code

| Arquivo | Mudança | Status |
|---|---|---|
| `wrangler.toml` | {{o quê}} | TODO \| DONE |
| `migrations/000X_{{nome}}.sql` | nova migration | TODO \| DONE |

### 11.5 Observability

| Recurso | Tipo | Localização |
|---|---|---|
| `corelink_xxx_total` | métrica counter | Grafana Cloud |
| `Dashboard CoreLink Sprint-XX` | dashboard | URL |
| `CoreLinkXxxAlert` | alerta | URL |

---

## 12. Anti-Scope

> Itens que **explicitamente NÃO** estão neste sprint. Cada item rastreado para onde será atendido.

| Item | Razão de exclusão | Onde será atendido |
|---|---|---|
| {{feature}} | Não está na intent (§2) | S-YY |
| {{feature}} | Bloqueado por dependência X | Após dep resolvida (S-ZZ) |
| {{feature}} | Rejeitado | ADR-XXXX |

---

## 13. Dependências

### 13.1 Upstream (este sprint depende de)

| Dependência | Tipo | Status | Bloqueia? |
|---|---|---|---|
| Sprint S-XX `SEALED` | sprint | ❌ | sim |
| ADR-XXXX `ACCEPTED` | adr | ❌ | sim |
| Spec `02_product/capabilities.md` `FROZEN` | spec | ❌ | sim |

### 13.2 Downstream (este sprint desbloqueia)

| Dependente | O que desbloqueia |
|---|---|
| S-YY | {{capabilities}} |

### 13.3 External (vendors, APIs, contratos)

| Dependência externa | SLA / disponibilidade | Owner externo |
|---|---|---|
| Cloudflare Containers | GA disponível | Cloudflare |
| Clerk | API estável | Clerk |
| Neon | API estável | Neon |
| Stripe | API estável | Stripe |

---

## 14. Observability Plan

### 14.1 Métricas

| Nome | Tipo | Unidade | Labels | Cardinality budget | Alerta |
|---|---|---|---|---|---|
| `corelink_xxx_total` | counter | requests | `tenant_id, method, status` | ≤ 10k | rate ↗ +N% |
| `corelink_xxx_duration_seconds` | histogram | seconds | `tenant_id, method` | ≤ 5k | p99 > Xs |

### 14.2 Logs

| Evento | Nível | Campos requeridos | Sampling | Retention |
|---|---|---|---|---|
| `xxx.success` | INFO | `trace_id, tenant_id, ...` | 1.0 | 30d |
| `xxx.error` | ERROR | `trace_id, tenant_id, error_kind, ...` | 1.0 | 90d |

### 14.3 Traces

| Span | Atributos | Parent |
|---|---|---|
| `xxx` | `tenant_id, ...` | request root |

### 14.4 Dashboards

| Nome | URL | Painéis |
|---|---|---|
| `CoreLink Sprint-XX Delivery` | TBD | Adoption, Latency, Errors |

### 14.5 Alertas

| Nome | Condição | Severity | Runbook |
|---|---|---|---|
| `CoreLinkXxxLatencyHigh` | p99 > Xs por 5m | warning | `runbooks/xxx-latency.md` |

---

## 15. Rollback / Recovery Plan

### 15.1 Cenários de rollback

#### Cenário A: Deploy quebra produção

- **Detecção:** alerta `CoreLinkXxxErrorRateHigh` dispara
- **Ação imediata (<5 min):** `wrangler rollback` para versão anterior
- **Verificação:** alerta limpa, dashboard volta ao baseline
- **Post-mortem:** obrigatório dentro de 48h

#### Cenário B: Migration de schema incompatível

- **Detecção:** erros de DB em logs
- **Ação:** Neon point-in-time restore para snapshot pré-migration
- **Verificação:** queries voltam a funcionar
- **Trabalho de remediação:** rollback de schema + investigação

#### Cenário C: Vazamento de dados entre tenants (GRAVE)

- **Detecção:** alerta crítico `TenantIsolationViolation` ou denúncia
- **Ação imediata:** desligar feature flag relacionada; rolar back para versão segura
- **Comunicação:** plano de comunicação a clientes afetados em ≤ 4h
- **Post-mortem:** **obrigatório**, com correção arquitetural

### 15.2 Reversibilidade de migrations de schema

- [ ] Toda migration tem migration reversa (`down`)
- [ ] Se migration é destrutiva (drop column), schema-evolution segue padrão "expand-contract":
  1. Expand: add new col, dual-write
  2. Migrate: backfill
  3. Contract: stop writing old, drop col

### 15.3 Feature flags

| Flag | Default | Pode rollback via flag? |
|---|---|---|
| `feature.xxx` | false | sim, em <30s via API |

---

## 16. Security & Compliance Plan

### 16.1 Threat Model do Sprint

> *(Referência ao threat model master em `03_architecture/security_model.md` + delta para o escopo deste sprint.)*

#### Novos trust boundaries introduzidos
- {{boundary}}

#### Novos assets
- {{asset}}: classificação {{public/internal/confidential/restricted}}

#### Análise STRIDE
| Categoria | Ameaças identificadas | Mitigação |
|---|---|---|
| Spoofing | {{lista}} | {{controle}} |
| Tampering | {{lista}} | {{controle}} |
| Repudiation | {{lista}} | {{controle}} |
| Information disclosure | {{lista}} | {{controle}} |
| Denial of service | {{lista}} | {{controle}} |
| Elevation of privilege | {{lista}} | {{controle}} |

### 16.2 Controles de segurança implementados

| Controle | Onde | Verificação |
|---|---|---|
| AuthN obrigatória | middleware | test |
| Tenant isolation | middleware + repo layer | property test |
| Audit log | sensitive ops | integration test |
| Rate limit | middleware | load test |
| Input validation | handlers | fuzz test |

### 16.3 Compliance checklist

| Item | Aplicável? | Status |
|---|---|---|
| GDPR (EU users) | sim/não | ❌ |
| LGPD (BR users) | sim/não | ❌ |
| HIPAA (BAA customers) | sim/não/N-A | ❌ |
| SOC 2 controls relevantes | sim | ❌ |

---

## 17. Completeness Checklist (SOTA)

> Rigor SOTA. Cada caixa verificável objetivamente. Sem caixa "parcial".

### 17.1 Code Completeness

- [ ] Toda função pública tem Rustdoc
- [ ] Toda struct/enum pública tem Rustdoc
- [ ] Todo módulo tem doc-comment de módulo
- [ ] Todo `pub use` tem justificativa (re-export consciente)
- [ ] Nenhuma `unwrap()` em código não-teste exceto onde provadamente seguro (com comentário `// SAFETY:`)
- [ ] Nenhuma `expect()` sem mensagem informativa
- [ ] Nenhum `panic!()` reachable por user input
- [ ] Nenhum `unimplemented!()` ou `todo!()`
- [ ] Nenhum TODO/FIXME órfão (todos têm issue link)
- [ ] `clippy::all -- -D warnings` clean
- [ ] `clippy::pedantic` reviewed (não exigido pass mas justificado se ignored)
- [ ] `rustfmt --check` clean
- [ ] `cargo udeps` (unused deps) clean
- [ ] `cargo deny` (license, vulns, bans) clean
- [ ] Sem `#[allow(...)]` sem comentário de justificativa

### 17.2 Test Completeness

- [ ] Todo happy path testado
- [ ] Todo error path testado
- [ ] Toda boundary condition testada (empty, max, off-by-one)
- [ ] Todo invariante tem property test
- [ ] Todo integration point tem contract test
- [ ] Testes determinísticos (não flaky em 100 runs)
- [ ] Testes isolados (sem shared state global)
- [ ] Test names descritivos (`test_cas_upload_rejects_oversized_blob`)
- [ ] `#[should_panic]` apenas se documentado por que
- [ ] Coverage do código novo ≥ 85%
- [ ] Mutation kill rate de paths críticos ≥ 75%

### 17.3 Documentation Completeness

- [ ] README reflete estado atual (não menciona features removidas, lista features novas)
- [ ] Architecture diagrams atualizados (C4 levels relevantes)
- [ ] Sequence diagrams para novos fluxos críticos
- [ ] Runbook para cada nova procedure operacional
- [ ] Troubleshooting guide atualizado
- [ ] Glossário atualizado
- [ ] CHANGELOG.md atualizado
- [ ] OpenAPI / gRPC reflection refletindo APIs reais

### 17.4 Operational Completeness

- [ ] Deploy automatizado (zero passos manuais documentados como "rodar à mão")
- [ ] Rollback automatizado (mesmo critério)
- [ ] Toda métrica de sucesso tem alerta inverso (sem cobertura cega)
- [ ] Toda métrica de falha tem alerta com runbook
- [ ] Alertas testados (synthetic alert disparado, runbook executado)
- [ ] Dashboard "CoreLink {{escopo}}" criado e linkado
- [ ] On-call documentation atualizada
- [ ] Backup/restore verificado em janela ≤ 24h

### 17.5 Security Completeness

- [ ] AuthN em todo endpoint público
- [ ] AuthZ em toda operação sensível
- [ ] Tenant isolation enforced em layer de repositório
- [ ] Todo segredo em secrets manager (zero hardcoded)
- [ ] Rate limit por tenant em todo endpoint custoso
- [ ] Audit log para toda operação de mutation em recurso sensível
- [ ] Error messages não vazam internals (stack traces, queries SQL, paths internos)
- [ ] Dependencies scanned (`cargo audit`)
- [ ] Inputs validados em fronteira de sistema
- [ ] CORS configurado restritivamente
- [ ] Headers de segurança HTTP (HSTS, CSP, X-Content-Type-Options, etc.)

### 17.6 Performance Completeness

- [ ] Benchmarks adicionados para hot paths novos
- [ ] Load test ≥ 2× tráfego prod esperado
- [ ] Soak test (≥ 24h) para detectar leaks
- [ ] Profiling de allocator/GC behavior (se aplicável)
- [ ] Worst-case latency limitada (timeouts)
- [ ] Backpressure implementada onde aplicável
- [ ] Connection pooling configurado e dimensionado

### 17.7 Compliance Completeness

- [ ] Data classification aplicada (no schema + docs)
- [ ] Retention configurada (TTL ou cron de purge)
- [ ] Direito de exclusão (GDPR/LGPD) exercitado em E2E test
- [ ] Data residency respeitada (testar routing)
- [ ] Privacy impact reviewed se categoria nova
- [ ] PII / dados regulados marcados explicitamente

### 17.8 Spec Completeness (sobre este sprint)

- [ ] Toda seção deste contrato preenchida (sem `{{...}}` deixado)
- [ ] Toda CAP em §3 mapeada a WI em §6
- [ ] Toda WI em §6 mapeada a teste em §7.2
- [ ] Toda métrica em §14 sendo emitida pelo código
- [ ] Toda alerta em §14 configurada com runbook em §11.3
- [ ] §18 (Traceability matrix) 100% preenchida

---

## 18. Traceability Matrix

> Tabela mestra que liga **cada CAP a tudo o que a verifica e implementa**. Geração automática via tooling de trace check (§13 do framework 00).

| CAP | INV | NFR | ADR | WI | Código | Testes | Métricas | Alertas |
|---|---|---|---|---|---|---|---|---|
| CAP-XXX | INV-YYY, INV-ZZZ | NFR-AAA | ADR-BBBB | WI-SXX-001, WI-SXX-005 | `src/path/file.rs` | `tests/file_test.rs`, `tests/props/inv_yyy.rs` | `corelink_xxx_total` | `CoreLinkXxxAlert` |

**Validação automática:** script `scripts/trace_check.sh` deve sair com exit 0 antes de COMPLETE.

---

## 19. Review Checkpoints

### 19.1 Mid-Sprint Review

- **Quando:** ponto médio do sprint (~50% duração)
- **Participantes:** Sprint Owner + Tech Lead Reviewer + Aprovador Final
- **Pauta:**
  - Status dos WIs (DONE / DOING / TODO / BLOCKED)
  - Atualização do Risk Register §10
  - Bloqueios escalados
  - Decisão: continua, decompõe ou aborta
- **Output:** ata anexada ao sprint contract

### 19.2 Pre-Close Review

- **Quando:** D-2 (dois dias antes do fim planejado)
- **Participantes:** Sprint Owner + todos revisores requeridos
- **Pauta:**
  - Dry run de DoD §7 — predizer quais ficarão ❌
  - Plano para fechar gaps
  - Decisão: fechamos no prazo, estendemos, ou declaramos `FAILED`
- **Output:** ata + lista de items remanescentes

### 19.3 Sprint Close Review

- **Quando:** dia D (fim planejado, ou estensão)
- **Participantes:** todos revisores + Aprovador Final
- **Pauta:**
  - Verificação item-a-item de DoD §7
  - Coleta de sign-offs §20
- **Output:** transição `REVIEWING → COMPLETE → SEALED`, ou `→ FAILED`

---

## 20. Sign-off

> **REGRA INVIOLÁVEL:** Todas as assinaturas abaixo são necessárias para `COMPLETE → SEALED`. Faltando uma, sprint **NÃO** está sealed.

| Papel | Nome | Critério de aprovação | Assinatura | Data |
|---|---|---|---|---|
| **Sprint Owner** | {{Nome}} | Sprint executado conforme contrato; gaps documentados | _________________ | YYYY-MM-DD |
| **Tech Lead** | {{Nome}} | §17.1 (Code), §17.2 (Test), §17.6 (Performance) ✅ | _________________ | YYYY-MM-DD |
| **Security Reviewer** | {{Nome}} | §16 (Security plan) executado, §17.5 ✅ | _________________ | YYYY-MM-DD |
| **Ops Reviewer** | {{Nome}} | §14 (Observability), §15 (Rollback), §17.4 ✅ | _________________ | YYYY-MM-DD |
| **QA Reviewer** | {{Nome}} | §9 (Test Strategy) executado, §17.2 ✅ | _________________ | YYYY-MM-DD |
| **Product Reviewer** | {{Nome}} | CAPs em §3 entregues conforme escopo | _________________ | YYYY-MM-DD |
| **Aprovador Final** | {{Nome}} | Sprint atende ao contrato integralmente | _________________ | YYYY-MM-DD |

### Apêndice: Dissents (se houver)

> Discordâncias minoritárias que foram registradas mas não impediram aprovação.

*(vazio se nenhuma)*

---

## 21. Retrospective (post-COMPLETE)

> A ser preenchida após `COMPLETE`, antes de `SEALED`.

### 21.1 What went well
- {{...}}

### 21.2 What didn't
- {{...}}

### 21.3 Surprises
- {{...}}

### 21.4 Learnings capturadas
- {{...}}

### 21.5 Métricas finais

| Métrica | Planejado | Realizado | Δ |
|---|---|---|---|
| Duração (dias úteis) | {{X}} | {{Y}} | {{±%}} |
| WIs completos | {{X}} | {{Y}} | — |
| WIs reescopados | — | {{N}} | — |
| Bugs descobertos durante sprint | — | {{N}} | — |
| Bugs encontrados pós-deploy | — | {{N}} | — |
| Coverage final | {{target}} | {{atual}} | — |

### 21.6 Ações para sprints futuros
- [ ] {{ação concreta com owner e prazo}}

### 21.7 Refinamentos propostos ao framework de spec
- [ ] {{proposta de melhoria do template, DoR, DoD}}

---

## 22. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | Sprint contract criado em status PROPOSED |
| 1.0.1 | YYYY-MM-DD | {{Nome}} | DoR atualizado após review |
| 1.1.0 | YYYY-MM-DD | {{Nome}} | Adicionado WI-SXX-007 |

---

**Fim do Sprint Contract S-XX.**
