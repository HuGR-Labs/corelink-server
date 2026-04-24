---
id: "FRAMEWORK-00"
type: "framework"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.5.0"
created: "2026-04-23"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["meta", "process", "framework"]
---

# 00 — Specification Framework

> **doc_status:** DRAFT
> **Versão:** 0.5.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** *(a definir)*
> **Supersedes:** —
> **Superseded By:** —
>
> Este é o **meta-documento** do sistema de especificação do **HuGR CoreLink**. Define *como* especificamos o produto — princípios, hierarquia, vocabulário, processos, automações. Nenhum outro documento de spec pode existir sem este estar *frozen*.
>
> Este framework aplica a si próprio: é versionado, revisado, auditado, frozen, como qualquer outro artefato.

---

## Sumário

### Parte I — Fundações

0. [Meta](#0-meta)
1. [Propósito](#1-propósito)
2. [Princípios Fundamentais](#2-princípios-fundamentais)
3. [Working Backwards](#3-working-backwards)

### Parte II — Estrutura

4. [Hierarquia dos 5 Níveis](#4-hierarquia-dos-5-níveis)
5. [Rastreabilidade Bidirecional](#5-rastreabilidade-bidirecional)
6. [Numeração e Identificadores Estáveis](#6-numeração-e-identificadores-estáveis)
7. [Ciclo de Vida — `doc_status` × `work_status`](#7-ciclo-de-vida--doc_status--work_status)
8. [Relações Entre Artefatos](#8-relações-entre-artefatos)

### Parte III — Linguagem e Vocabulário

9. [RFC 2119 — Vocabulário Normativo](#9-rfc-2119--vocabulário-normativo)
10. [Política de Linguagem](#10-política-de-linguagem)
11. [Writing Style Guide](#11-writing-style-guide)
12. [Ubiquitous Language (DDD)](#12-ubiquitous-language-ddd)

### Parte IV — Rigor Formal

13. [Formal Methods e TLA+](#13-formal-methods-e-tla)
14. [Quality Standards ISO/IEC 25010](#14-quality-standards-isoiec-25010)
15. [Types of Decision Records](#15-types-of-decision-records)

### Parte V — Preocupações Transversais (Cross-Cutting)

16. [Cross-Cutting Concerns — Visão Geral](#16-cross-cutting-concerns--visão-geral)
17. [Segurança (Secure by Default, Zero Trust)](#17-segurança)
18. [Privacidade (Privacy by Design)](#18-privacidade)
19. [Threat Modeling (STRIDE + LINDDUN)](#19-threat-modeling)
20. [Compliance e Regulatório](#20-compliance-e-regulatório)
21. [Data Governance](#21-data-governance)
22. [Service Level Methodology — SRE](#22-service-level-methodology--sre)
23. [Reliability & Resilience Patterns](#23-reliability--resilience-patterns)
24. [Performance e Capacity Planning](#24-performance-e-capacity-planning)
25. [Accessibility e Internationalization](#25-accessibility-e-internationalization)
26. [Supply Chain Security](#26-supply-chain-security)
27. [AI/LLM Governance](#27-aillm-governance)

### Parte VI — Engenharia e Entrega

28. [Feature Lifecycle](#28-feature-lifecycle)
29. [Progressive Delivery](#29-progressive-delivery)
30. [Test Strategy Philosophy](#30-test-strategy-philosophy)
31. [Incident Response e Post-Mortem](#31-incident-response-e-post-mortem)
32. [Architecture Fitness Functions](#32-architecture-fitness-functions)
33. [Technical Debt Management](#33-technical-debt-management)
33.5. [Risk Lanes — Classificação de Trabalho por Risco](#335-risk-lanes--classificação-de-trabalho-por-risco)

### Parte VII — Processo

34. [Templates](#34-templates)
35. [Diagramas](#35-diagramas)
35.5. [Control Inheritance — Canonical Source per Domain](#355-control-inheritance--canonical-source-per-domain)
35.6. [Waivers — Exceções Formais com Expiração Obrigatória](#356-waivers--exceções-formais-com-expiração-obrigatória)
35.7. [Evidence Taxonomy — Tipos Formais de Prova para Gates](#357-evidence-taxonomy--tipos-formais-de-prova-para-gates)
36. [Processo de Revisão e Sign-off](#36-processo-de-revisão-e-sign-off)
37. [Tooling e CI](#37-tooling-e-ci)
38. [Red Team e Adversarial Review](#38-red-team-e-adversarial-review)

### Parte VIII — Referência

39. [Anti-padrões](#39-anti-padrões)
40. [Glossário](#40-glossário)
41. [Meta-regras (Evolução deste Framework)](#41-meta-regras-evolução-deste-framework)
42. [Change Log](#42-change-log)
43. [Revisão](#43-revisão)

---

# Parte I — Fundações

## 0. Meta

### 0.1 Propósito deste documento

Este framework existe para garantir que **todo artefato de especificação do CoreLink** tenha:

- **Rigor formal** — zero ambiguidade, toda afirmação verificável, invariantes formalizáveis (quando justificável via TLA+).
- **Rastreabilidade total bidirecional** — cada linha de código rastreável até uma *capability*, uma *invariant*, um *sprint contract*, e vice-versa.
- **Qualidade SOTA** — alinhado com standards internacionais (RFC 2119, ISO/IEC 25010, SRE Google, STRIDE, OWASP) e frameworks industriais modernos (Amazon Working Backwards, Basecamp Shape Up, Bezos one-way/two-way).
- **Disciplina de processo** — hierarquia inviolável, *frozen flags*, revisões explícitas, automações obrigatórias.
- **Resiliência a mudança** — o próprio framework é evolvível sob regras explícitas.

### 0.2 Público-alvo

| Audiência | Uso primário |
|---|---|
| Fundador / Product Owner | Definir visão, aprovar *freezes*, resolver trade-offs, escalação de dissent |
| Engenheiros | Implementar sprints sob contrato inviolável |
| Revisores de Segurança | Auditar *threat models*, invariants, controles, compliance |
| Revisores de Privacidade | Auditar Privacy by Design, data handling, direitos do titular |
| Revisores de Operações | Validar *observability*, SLOs, runbooks, incident response |
| Revisores de Compliance | Validar SOC 2, ISO 27001, GDPR, LGPD, HIPAA aplicáveis |
| Stakeholders externos | Compreender decisões arquiteturais e *roadmap* |
| Agentes de IA (Claude, etc.) | Produzir specs consistentes entre sessões, com contexto persistente |

### 0.3 O que este documento **NÃO** cobre

- **O produto em si** — coberto pelos Níveis 1–5.
- **Implementação concreta de código** — coberto pelos sprints.
- **Marketing / GTM / vendas** — coberto por documentos fora do escopo de specs técnicas.
- **Estratégia de negócio / financiamento** — coberto por documentos confidenciais separados.

### 0.4 Relação com outros frameworks existentes

Este framework é **compatível com** e **toma emprestado de**:

| Framework / Standard | Relação |
|---|---|
| **Amazon Working Backwards** | Metodologia base de §3 |
| **RFC 2119** | Vocabulário normativo (§9) |
| **ISO/IEC 25010** | Taxonomia de qualidade de software (§14) |
| **Google SRE Book** | SLI/SLO/Error Budget (§22) |
| **STRIDE (Microsoft)** | Threat modeling (§19) |
| **LINDDUN** | Privacy threat modeling (§19) |
| **OWASP ASVS, OWASP Top 10** | Controles de segurança (§17) |
| **Cavoukian 7 Principles** | Privacy by Design (§18) |
| **NIST Cybersecurity Framework** | Controles e maturidade (§17) |
| **SLSA (Supply chain)** | Supply chain security (§26) |
| **Michael Nygard ADR** | Template de ADR (§15) |
| **Domain-Driven Design (Evans)** | Ubiquitous Language (§12) |
| **C4 Model (Simon Brown)** | Diagramas arquiteturais (§35) |
| **TLA+ (Lamport)** | Specification formal de invariants críticos (§13) |
| **Gherkin / BDD** | Acceptance criteria em Given-When-Then (§30) |
| **Bezos one-way/two-way door** | Reversibility (§15 ADR template) |
| **Basecamp Shape Up** | Inspiração para pitch-style specs (não adotado integralmente) |

### 0.5 Convenções internas deste framework

- Todo princípio identificado como **PRINC-XXX**.
- Toda regra de processo como **REG-XXX** (ex: REG-WB-001, REG-LANG-002).
- Todo anti-padrão como **AP-XXX**.
- Termos em **maiúsculas negritas** (DEVE, NÃO DEVE, etc.) seguem RFC 2119 (§9).
- Termos em *itálico* são termos-da-arte em inglês (em contexto pt-BR).
- Blocos `código formatado` indicam identificadores, comandos, caminhos, ou código.

---

## 1. Propósito

O CoreLink é um produto de **alta complexidade técnica**, **alta aposta operacional** e **alta exposição de segurança/privacidade**:

- Serve artefatos de múltiplos *tenants* com isolamento inviolável.
- Opera em espaço competitivo SOTA (NativeLink, BuildBuddy, JFrog Artifactory, Sonatype Nexus, Depot.dev).
- Processa dados que podem incluir código proprietário, credenciais acidentalmente cacheadas, artefatos sujeitos a regulação (HIPAA, GDPR, LGPD).
- Precisa de latência baixa previsível sob carga alta.
- Tem padrão de uso multi-região, multi-fuso, multi-idioma.

Um produto dessa natureza **NÃO PODE** ser construído com spec solta. Cada decisão **DEVE** ser:

1. **Explícita** — está escrita, não no *head canon* de alguém.
2. **Justificada** — tem *rationale* rastreável e registrado.
3. **Auditável** — qualquer revisor qualificado pode validar.
4. **Reversível ou consciente de sua irreversibilidade** — *one-way doors* vs *two-way doors* identificados.
5. **Verificável** — método de verificação declarado na própria spec.
6. **Observável** — métricas, logs, traces pertencentes à spec, não adicionados depois.

Este framework é a **infraestrutura de rigor** que permite isso.

---

## 2. Princípios Fundamentais

> Invariantes do processo de especificação. Violação de qualquer princípio invalida o artefato em questão.
>
> Agrupados em 4 categorias: **Processo Nuclear**, **Rigor de Engenharia**, **Segurança e Privacidade**, **Entrega e Operação**.

### 2.1 Processo Nuclear (PRINC-001 a PRINC-010)

#### PRINC-001 — Specs descrevem produto completo, não MVP

> **DEVE** todo artefato de spec descrever o produto em sua **forma final completa**.
> **NÃO DEVE** nenhum artefato conter formulações como "no MVP" ou "inicialmente fazemos X, depois expandimos".
> MVP **NÃO É** conceito de spec; é conceito de *sprint* (Nível 4).

O **destino** (produto completo) é descrito nas specs e é **imutável** exceto por evolução explícita via *thaw*. O **caminho** (ordem de entrega) vive nos sprints e é incremental.

#### PRINC-002 — Working backwards é inviolável

> **DEVE** toda spec ser produzida na ordem: Nível 1 → 2 → 3 → 4 → 5.
> **NÃO DEVE** um documento de nível N referenciar artefatos ainda não *frozen* de nível ≤ N.
> **NÃO DEVE** Nível 4 (sprints) ser iniciado enquanto Níveis 1–3 relevantes não estão *frozen*.

#### PRINC-003 — Zero ambiguidade (RFC 2119)

> **DEVE** toda afirmação normativa usar o vocabulário de RFC 2119 (§9).
> **NÃO DEVE** haver palavras ambíguas como "provavelmente", "deveria ser rápido", "idealmente".

#### PRINC-004 — Toda afirmação é verificável

> **DEVE** toda afirmação normativa ter **método de verificação declarado** no próprio artefato (teste, métrica, revisão humana explícita, automação).
> **NÃO DEVE** existir afirmação que não possa ser validada objetivamente.

#### PRINC-005 — Rastreabilidade bidirecional total

> **DEVE** todo artefato de nível N referenciar os artefatos de nível N-1 que o originam.
> **DEVE** existir a qualquer momento capacidade de navegar:
> - **Top-down**: Visão → JTBD → CAP → { INV, NFR } → ADR → C4 Componente → Sprint WI → Código → Testes → Observability.
> - **Bottom-up**: uma linha de código ↔ trace reverso até uma *capability* ou uma *invariant*.
>
> Um artefato que quebra rastreabilidade **NÃO PODE** ser *frozen*.

#### PRINC-006 — Numeração estável

> **NÃO DEVE** nenhum identificador estável (CAP-XXX, INV-XXX, ADR-XXXX, NFR-XXX, JTBD-XXX, PERSONA-XX, S-XX, WI-SXX-NNN, AP-XXX, PRINC-XXX, REG-XXX, FM-XXX, METRIC-XXX) ser **renumerado** após criação.
> Artefato descontinuado vira `DEPRECATED` ou `SUPERSEDED_BY`, mas o identificador permanece reservado para sempre.

#### PRINC-007 — Frozen é sério

> **DEVE** um documento *frozen* permanecer imutável exceto por processo de *thaw* explícito (§7).
> **DEVE** qualquer *thaw* disparar auditoria obrigatória dos artefatos downstream dependentes.
> **NÃO DEVE** mudança "silenciosa" existir em documento não-`DRAFT`.

#### PRINC-008 — Diff é explicação

> **DEVE** toda mudança em documento não-`DRAFT` ser acompanhada de entrada em *Change Log* com: versão, data, autor, mudança resumida, razão/trigger.
> **DEVE** PRs em specs terem descrição prosa explicando o *porquê*, não apenas o *quê*.

#### PRINC-009 — Quality gates são binários

> **DEVE** todo *gate* de qualidade ter resultado binário (✅ / ❌).
> **NÃO DEVE** existir "parcialmente atendido" em *exit criteria*.
> Se um *gate* parece exigir "70% atendido", **DEVE** ser decomposto em múltiplos *gates* binários.

#### PRINC-010 — Anti-scope é tão importante quanto scope

> **DEVE** todo artefato relevante explicitar o que **NÃO** cobre.
> **DEVE** todo item de anti-scope rastrear para: onde será coberto (futuro), ou por que foi rejeitado (ADR), ou qual constraint impede.
> Ambiguidade sobre anti-scope é a causa raiz de *scope creep* e *sprints* que não terminam.

### 2.2 Rigor de Engenharia (PRINC-011 a PRINC-018)

#### PRINC-011 — Alternativas consideradas são registradas

> **DEVE** todo ADR registrar pelo menos **2 alternativas consideradas**, incluindo *status quo* quando aplicável.
> **DEVE** cada alternativa ter: descrição, pros, cons, razão de rejeição, evidência consultada, condições de reconsideração.

O exercício de alternativas separa **decisão** de **default inconsciente**.

#### PRINC-012 — Reversibilidade é dimensão obrigatória

> **DEVE** todo ADR classificar a decisão como *two-way door*, *one-way door* ou *hybrid*, e quantificar custo de reversão (engineer-weeks, $, customer impact, opportunity cost).
> **DEVE** decisões *one-way* merecer escrutínio proporcionalmente maior (mais revisores, mais alternativas, análise de sensibilidade).

#### PRINC-013 — Observability é parte da spec, não *afterthought*

> **DEVE** toda *capability* especificar: métricas emitidas, logs estruturados, traces, alertas, runbooks relacionados, dashboards.
> **NÃO DEVE** uma *capability* ser marcada como implementada se não tem *observability* compatível em produção.

#### PRINC-014 — Segurança e compliance são parte da spec, não *afterthought*

> **DEVE** toda *capability* que toca dados de *tenant* especificar: classificação de dados (público/interno/confidencial/restrito), controles de acesso, *audit logging*, *retention*, implicações de regulações aplicáveis (GDPR/LGPD/HIPAA).

#### PRINC-015 — Specs são produto

> **DEVE** toda spec ser escrita com o mesmo rigor de código de produção: revisada, versionada, testada (via CI *lint*, *link checker*, *trace checker*), distribuída com disciplina de *release*.
> Specs **NÃO SÃO** rascunhos — são artefatos de produção.

#### PRINC-016 — Formal methods para invariants críticos

> **DEVERIA** todo invariante classificado como CRITICAL (§17.2) ter especificação formal via TLA+ ou equivalente, verificável por *model checker*.
> **PODE** invariants de severidade menor ser especificados em prosa RFC 2119, desde que método de verificação seja declarado.

Invariants são a proteção do produto. Invariants críticos em prosa são sinal de risco de *silent violation*.

#### PRINC-017 — Domain language é consistente (Ubiquitous Language)

> **DEVE** todo termo de domínio (ex: *tenant*, *digest*, *CAS*, *action*) ter definição única em `§40 Glossário`.
> **NÃO DEVE** o mesmo conceito receber nomes diferentes em áreas diferentes do código ou spec.
> **NÃO DEVE** o mesmo nome referir conceitos diferentes.

Ubiquitous Language (DDD) é propriedade do produto, não apenas das specs.

#### PRINC-018 — API-first e contract-first

> **DEVE** toda integração entre componentes ter contrato explícito (protobuf, OpenAPI, schema) versionado, **antes** da implementação.
> **DEVE** toda API pública ter política de *deprecation* documentada (§28 Feature Lifecycle).
> **DEVE** toda API pública ter testes de contrato (*consumer-driven contract tests* quando aplicável).

### 2.3 Segurança e Privacidade (PRINC-019 a PRINC-024)

#### PRINC-019 — Secure by Default

> **DEVE** toda configuração padrão ser a configuração **mais segura possível**.
> **NÃO DEVE** o usuário precisar ativar segurança manualmente; ativar features menos seguras requer ação explícita.

Exemplos: TLS obrigatório, auth obrigatória, *rate limiting* ativo por padrão, *audit log* ativo por padrão.

#### PRINC-020 — Least Privilege

> **DEVE** todo *principal* (user, service, token, process) ter o **menor conjunto de privilégios** necessários para sua função.
> **NÃO DEVE** privilégio amplo ser concedido por conveniência.
> **DEVE** expansão de privilégio ter *audit trail*.

#### PRINC-021 — Zero Trust

> **NÃO DEVE** existir "perímetro confiável" onde autenticação/autorização é pulada.
> **DEVE** toda chamada entre componentes, mesmo internos, autenticar e autorizar.
> **DEVE** todo dado em trânsito ser criptografado (TLS 1.3+ ou equivalente).
> **DEVE** todo dado em repouso em storage persistente ser criptografado.

#### PRINC-022 — Privacy by Design

> **DEVE** toda *capability* que processa dados pessoais seguir os 7 princípios de Cavoukian (§18):
> 1. Proativo, não reativo
> 2. Privacidade como *default*
> 3. Privacidade embutida no *design*
> 4. Funcionalidade total — positive-sum
> 5. Segurança *end-to-end*
> 6. Transparência
> 7. Respeito ao usuário

#### PRINC-023 — Data minimization

> **DEVE** o produto coletar, armazenar e processar **apenas os dados estritamente necessários** para a função declarada.
> **NÃO DEVE** dado pessoal ser armazenado "caso a gente precise depois".
> **DEVE** *retention policy* existir para toda categoria de dado, com exclusão automatizada.

#### PRINC-024 — Fail safe, not fail open

> **DEVE** todo componente, ao encontrar estado inválido ou falha não recuperável, transicionar para **estado seguro** (deny-by-default) e não para **estado permissivo**.
> Exemplos: *auth* indisponível → **nega** (não permite bypass); *quota service* indisponível → **aplica quota pessimista** (não permite uso ilimitado).

### 2.4 Entrega e Operação (PRINC-025 a PRINC-030)

#### PRINC-025 — Idempotência por padrão

> **DEVE** toda operação de mutação (POST/PUT/DELETE em HTTP, RPC mutativos) ser idempotente ou explicitamente marcada como *non-idempotent* em sua spec.
> **DEVE** *retries* ser seguros.
> **DEVE** *idempotency keys* ser suportadas em operações críticas de negócio (ex: billing).

#### PRINC-026 — Automation over documentation

> **DEVE**, sempre que possível, uma regra ser **automatizada** (CI check, *lint*, *fitness function*) em vez de apenas **documentada**.
> Documentação sem automação é frágil a drift.
> Se não pode ser automatizado, **DEVE** haver *review checklist* explícito que trate o item.

#### PRINC-027 — Progressive delivery obrigatório

> **DEVE** toda mudança que afeta usuários em produção ser entregue via estratégia progressiva: *canary*, *blue-green*, ou *rolling* com *auto-rollback* em degradação.
> **NÃO DEVE** haver *big-bang deploy* de mudança com impacto em produção.

#### PRINC-028 — Cultura *blameless*

> **DEVE** todo post-mortem focar em falhas de **sistema, processo e automação**, não em pessoas.
> **NÃO DEVE** ação corretiva de post-mortem responsabilizar indivíduo por erro não-malicioso.

#### PRINC-029 — Medir antes de otimizar

> **NÃO DEVE** haver otimização de performance sem *baseline* mensurado + hipótese + métrica de sucesso.
> **DEVE** toda otimização ser validada por *benchmark before/after* reproduzível.

#### PRINC-030 — Consumer-first API design

> **DEVE** toda API pública ser projetada do ponto de vista do *consumer*, com *user journey* de integração documentada.
> **DEVE** a primeira validação de uma API nova ser "um consumidor ficcional consegue completar o *journey* sem bater em *paper cut*?".

#### PRINC-031 — Evidence-driven gates

> **DEVE** toda caixa binária de *Completeness Criteria*, *Definition of Done* e *Quality Standards* ter **evidence artifact** registrado (link para log, screenshot, relatório, test output, dashboard, PR diff).
> **NÃO DEVE** uma caixa ser marcada `✅` com base em "confiança" ou "intuição". Checkbox sem *evidence* é **teatro**, não rigor.
>
> Evidence pode ser: URL pra CI log, arquivo gerado, extrato de log/métrica, gravação de dry-run, ata de revisão humana com nome e data.

#### PRINC-032 — Role + Automation tagging

> **DEVE** toda caixa de gate ter:
>
> - **Role tag** (🔧 ENG, 🔒 SEC, 🔏 PRIV, 📊 SRE, 🎨 PROD, ✓ QA, 🏛 ARCH, 💰 FIN, 📜 LEGAL) identificando quem valida.
> - **Automation tag** (🤖 automatizado, 👤 manual, 🤖👤 híbrido) identificando modo de validação.
>
> Taxonomia explícita previne: (a) ambiguidade sobre responsabilidade, (b) gates fantasma (ninguém valida), (c) over-engineering (tudo manual quando poderia ser CI).

#### PRINC-033 — Production Readiness Review separado de DoD

> **DEVE** toda feature/WI com impacto em produção (customer-facing) passar por **PRR (Production Readiness Review)** separado do DoD do WI.
> **NÃO DEVE** feature atingir GA (100% traffic) sem PRR `APPROVED`.
>
> DoD confirma: "WI está implementado corretamente."
> PRR confirma: "WI está pronto para enfrentar customers em produção" — cobrindo SLO, capacity, load, chaos, DR, observability, runbooks, security, privacy, supply chain, rollback, deployment strategy, support, communication, cost, legal.
>
> São **gates distintos**.

#### PRINC-034 — Time-boxing e escalação obrigatória

> **DEVE** todo WI/ST ter protocolo de escalação explícito quando estoura estimate.
> **NÃO DEVE** time "empurrar" silenciosamente — escalação formal em estouro > 50% de estimate.

### 2.5 Proporcionalidade, Herança e Evidence (PRINC-035 a PRINC-038)

Adicionados no Lote 2 (v0.4.0) para elevar a princípios fundamentais as regras de risk lanes, control inheritance, waivers e evidence taxonomy.

#### PRINC-035 — Trabalho classificado por risco; cerimônia é proporcional à lane

> **DEVE** todo Sprint, Work Item e Sub-task ser classificado em lane `LOW_RISK | STANDARD | HIGH_RISK` (§33.5).
> **DEVE** o conjunto de seções obrigatórias, sign-offs requeridos, gates de qualidade e processos de review ser **proporcional à lane**, conforme matriz §33.5.4.
> **NÃO DEVE** trabalho `LOW_RISK` carregar cerimônia de `HIGH_RISK` ("cerimônia sem rigor" — AP-007); **NÃO DEVE** trabalho `HIGH_RISK` ser rebaixado a `LOW_RISK` pra evitar sign-offs (má-fé).
> **DEVE** forcing factors (§33.5.3) serem enumerados via `lane_forcing_factors` sempre que aplicáveis, forçando `HIGH_RISK`.

#### PRINC-036 — Controles cross-cutting têm fonte canônica; duplicação é drift

> **DEVE** toda *cross-cutting concern* (observability, security, privacy, SLO, failure modes, resilience, data, auth, compliance) ter **uma fonte canônica** declarada em §35.5.1.
> **DEVE** todo artefato de trabalho (Sprint/WI/ST/PRR) que toca essas áreas **referenciar** a fonte canônica via `inherits_from`, **não duplicar**.
> **DEVE** `local_deltas` ser usados apenas quando o artefato legitimamente diverge/estende a fonte — com `rationale` escrito.
> **NÃO DEVE** o mesmo controle ser descrito em prosa independente em múltiplos artefatos downstream; isso garante drift.

#### PRINC-037 — Exceção formal requer expiração + compensating control obrigatórios

> **DEVE** toda exceção formal a um gate do framework seguir o processo de **waiver** (§35.6), via documento `specs/_waivers/WAIVER-YYYYMMDD-NNN-*.md`.
> **DEVE** todo waiver ter: `expires_at` (data ISO), `compensating_control`, `rationale` ≥ 10 chars, `revalidation_trigger`, `gates_waived` (lista explícita de IDs).
> **NÃO DEVE** existir "aceitar risco sem proteção por N meses" disfarçado de waiver — isso é ADR de risk acceptance (caminho diferente, §15).
> **NÃO DEVE** waiver ser renovado ≥ 2 vezes sem escalação ao Aprovador Final pra decisão binária: consertar causa raiz ou promover a constraint permanente via ADR.

#### PRINC-038 — Evidence é tipada; "dashboard URL" sozinho não é evidence

> **DEVE** toda caixa binária de Completeness/DoD/Quality marcada como ✅ ter referência a **pelo menos 1 evidence artifact** tipado conforme §35.7.1 (EVT-001 a EVT-024).
> **DEVE** evidence ser **imutável** por pelo menos UMA das: assinatura criptográfica, hash-addressed, plataforma imutável (git, CI run), ou timestamp RFC 3161.
> **NÃO DEVE** `EVT-014 DASHBOARD_URL` isolado ser aceito como evidence — precisa acompanhar `EVT-013 DASHBOARD_SNAPSHOT` pra auditabilidade temporal.
> **NÃO DEVE** a mesma evidence ser reusada pra múltiplos gates não-relacionados ("um CI log vale pra 10 gates distintos" é AP-EVID-007).

---

## 3. Working Backwards

### 3.1 Origem

Metodologia originada na Amazon (Jeff Bezos): antes de construir, escreve-se o *press release* do produto lançado como se ele já existisse. A partir daí, deriva-se o FAQ interno/externo, personas, JTBDs, e só então pensa-se em arquitetura e execução.

### 3.2 Por que aplicamos

1. **Força clareza do outcome** — se o *press release* é confuso, o produto é confuso.
2. **Descobre gaps de valor cedo** — se o FAQ não responde por que alguém pagaria, o produto não tem caso de uso.
3. **Alinha stakeholders** — todos leem o mesmo documento canônico.
4. **Previne sobre-engenharia** — especificamos o que entrega valor, não o que é tecnicamente interessante.
5. **Antecipa objeções chatas** — FAQ inclui "perguntas desconfortáveis" (custo, *lock-in*, falha de concorrente).

### 3.3 Como aplicamos

| Ordem | Artefato | Responde |
|---|---|---|
| 1 | PR/FAQ (Nível 1) | O que é o produto? Por que existe? Para quem? |
| 2 | Personas (Nível 1) | Quem especificamente? Que características? |
| 3 | JTBDs (Nível 1) | Que *job* a persona contrata o produto para fazer? |
| 4 | Success Metrics (Nível 1) | Como sabemos que funcionou? |
| 5 | Capabilities (Nível 2) | O que exatamente o produto faz? |
| 6 | Invariants + NFRs + Constraints (Nível 2) | Sob quais restrições? Com quais garantias? |
| 7 | User Journeys (Nível 2) | Como persona e produto interagem passo a passo? |
| 8 | Arquitetura + ADRs (Nível 3) | Como é construído? Quais foram as decisões? |
| 9 | Protocols + Data Model + Failure Modes (Nível 3) | Quais protocolos? Qual modelo de dados? Como falha? |
| 10 | Security Model (Nível 3) | Quais ameaças? Quais controles? |
| 11 | Sprint Contracts (Nível 4) | Em que ordem construímos? Sob que contrato? |
| 12 | Quality Framework (Nível 5) | Como garantimos rigor durante a construção? |

### 3.4 Regras operacionais

- **REG-WB-001**: PR/FAQ **DEVE** ser o primeiro artefato de conteúdo produzido (após este framework).
- **REG-WB-002**: PR/FAQ **DEVE** assumir produto lançado em sua forma completa (não MVP).
- **REG-WB-003**: Nenhum artefato de nível N **PODE** ser iniciado antes de nível N-1 relevante estar *frozen*.
- **REG-WB-004**: Se durante escrita de nível N percebe-se *gap* em nível < N, **DEVE** *thaw* do nível com *gap*, correção, re-*freeze*, e só então continuar.
- **REG-WB-005**: Se PR/FAQ revela que produto está mal-formulado, **DEVE** parar e reconsiderar visão antes de seguir.

### 3.5 Checklist de qualidade do PR/FAQ

- [ ] Um *outsider* consegue entender o produto lendo apenas o *press release* (3 parágrafos).
- [ ] O FAQ antecipa e responde **no mínimo 30 perguntas** cobrindo: valor, diferenciação, concorrentes, preço, *lock-in*, privacidade, segurança, *uptime*, *support*, *deprecation*.
- [ ] FAQ inclui pelo menos **5 perguntas "chatas" de stakeholders críticos** (CFO, Security, Legal, Ops, Customer Success).
- [ ] Zero uso de "TBD", "talvez", "provavelmente" no documento.

---

# Parte II — Estrutura

## 4. Hierarquia dos 5 Níveis

### 4.1 Visão geral

```
┌──────────────────────────────────────────────┐
│  Nível 1 — Visão (Produto)                   │
│  • PR/FAQ  • Personas  • JTBDs  • Metrics    │
└─────────────────┬────────────────────────────┘
                  │ derives
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 2 — Especificação Funcional            │
│  • Capabilities  • Quality Attrs (NFRs)       │
│  • Invariants    • Constraints                │
│  • User Journeys                              │
└─────────────────┬────────────────────────────┘
                  │ derives
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 3 — Arquitetura                        │
│  • ADRs  • PDRs  • ODRs                       │
│  • C4 (Context/Container/Component/Code)      │
│  • Data Model  • Protocols  • Failure Modes   │
│  • Security Model  • Privacy Model            │
│  • Observability Model  • SLO Catalog         │
└─────────────────┬────────────────────────────┘
                  │ derives
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 4 — Sprint Contracts (invioláveis)     │
│  • S-00, S-01, ..., S-NN                      │
│  cada um: intent, DoR, WI, DoD, gates,        │
│  invariants, anti-scope, sign-off             │
└─────────────────┬────────────────────────────┘
                  │ informs ∧ is informed by
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 5 — Framework de Qualidade             │
│  • DoR padrão  • DoD padrão                   │
│  • Quality Gates  • Completeness Criteria     │
│  • Test Strategy  • Fitness Functions         │
│  • Runbooks templates                         │
└──────────────────────────────────────────────┘
```

Nível 5 é **transversal**: informa Níveis 2, 3, 4, mas é escrito por último (após Nível 3 *frozen*) para refletir a realidade do produto.

### 4.2 Nível 1 — Visão (Produto)

| Arquivo | Conteúdo | Tamanho esperado |
|---|---|---|
| `01_vision/prfaq.md` | Press Release + Internal FAQ + External FAQ | 2.000–4.000 linhas |
| `01_vision/personas.md` | Tipos formais de usuários (estilo Lamport) do produto completo | 500–1.200 linhas |
| `01_vision/jobs_to_be_done.md` | Matriz JTBD por persona, com prioridade, frequência, circumstance | 600–1.500 linhas |
| `01_vision/success_metrics.md` | North Star + *guardrail metrics* + *counter metrics* | 400–800 linhas |

**Critérios de *freeze*:**

- [ ] PR/FAQ descreve **produto completo** (todos protocolos, tiers, interfaces, compliance).
- [ ] Cada persona com pelo menos **3 JTBDs** mapeados.
- [ ] Toda métrica de sucesso com: definição operacional, *baseline*, alvo, janela, *guardrail* inverso.
- [ ] FAQ inclui perguntas "chatas" de ≥ 5 stakeholder types.
- [ ] Zero `TBD`, `provavelmente`, `talvez`.

### 4.3 Nível 2 — Especificação Funcional

| Arquivo | Conteúdo |
|---|---|
| `02_product/capabilities.md` | Capacidades CAP-001…CAP-N (intent, inputs, outputs, pre/poscondições, acceptance, verificação, observability) |
| `02_product/quality_attributes.md` | NFRs numerados (taxonomia ISO/IEC 25010 §14) |
| `02_product/invariants.md` | Safety + Liveness INV-001…INV-N (com enunciado formal quando crítico) |
| `02_product/constraints.md` | Anti-scope explícito com *rationale* |
| `02_product/user_journeys/` | Uma jornada por arquivo; máquinas de estado formais |

**Critérios de *freeze*:**

- [ ] Toda CAP mapeada para ≥ 1 persona e ≥ 1 JTBD de Nível 1.
- [ ] Toda NFR com: categoria ISO 25010, métrica, *baseline*, alvo, janela, consequência de violação.
- [ ] Todo invariante com: enunciado formal (prosa precisa ou TLA+ se CRITICAL), método de verificação, *blast radius*.
- [ ] *Constraints* com *rationale* e rastreamento (ADR / futuro sprint / constraint externa).
- [ ] *User journeys* incluem *happy path*, *error paths* principais, estados terminais.

### 4.4 Nível 3 — Arquitetura

| Arquivo | Conteúdo |
|---|---|
| `03_architecture/adrs/ADR-XXXX-*.md` | Uma decisão arquitetural técnica por arquivo |
| `03_architecture/pdrs/PDR-XXXX-*.md` | Product Decision Records (§15) |
| `03_architecture/odrs/ODR-XXXX-*.md` | Operational Decision Records (§15) |
| `03_architecture/c4_context.md` | C4 nível 1: sistema + atores externos |
| `03_architecture/c4_containers.md` | C4 nível 2: containers (Workers, Containers Rust, R2, DB, KV, DO) |
| `03_architecture/c4_components.md` | C4 nível 3: módulos internos por container |
| `03_architecture/data_model.md` | Schemas + invariants de dados + classificação (§21) |
| `03_architecture/protocols/*.md` | Uma spec por protocolo (REAPI, npm, PyPI, Cargo, OCI, HF, etc.) |
| `03_architecture/failure_modes.md` | FMEA cobrindo (componente, modo) → impacto, detecção, mitigação |
| `03_architecture/security_model.md` | Threat model STRIDE, *trust boundaries*, controles (§19) |
| `03_architecture/privacy_model.md` | LINDDUN threats, data flows, controles (§18) |
| `03_architecture/observability_model.md` | Métricas, logs, traces, events, dashboards, alertas |
| `03_architecture/slo_catalog.md` | SLIs + SLOs + error budgets + burn rate alerts (§22) |
| `03_architecture/resilience_patterns.md` | Catálogo de patterns aplicados (§23) |

**Critérios de *freeze*:**

- [ ] Toda CAP mapeada para ≥ 1 componente em `c4_components.md`.
- [ ] Toda decisão "não-óbvia" tem ADR/PDR/ODR dedicado.
- [ ] Todo ADR com ≥ 2 alternativas, reversibilidade classificada, decision matrix quantitativa.
- [ ] `failure_modes.md` cobre cada par (componente, modo) relevante.
- [ ] `security_model.md` inclui STRIDE completo para cada *trust boundary*.
- [ ] `privacy_model.md` inclui LINDDUN para cada fluxo de dado pessoal.
- [ ] `slo_catalog.md` define SLI/SLO/error budget para cada CAP *user-facing*.

### 4.5 Nível 4 — Sprint Contracts

| Arquivo | Conteúdo |
|---|---|
| `04_sprints/_template.md` | Template canônico (versão *frozen*) |
| `04_sprints/S00_{nome}.md` | Sprint 0: foundation |
| `04_sprints/S01_{nome}.md` | Sprint 1 |
| `...` | ... |

**Critérios de *freeze* (por sprint):**

Ver template em `_templates/sprint_contract.md`. Resumo: 23 seções totais (§0–§22), DoR/DoD binários, invariants operacionais, traceability matrix completa.

### 4.6 Nível 5 — Framework de Qualidade

| Arquivo | Conteúdo |
|---|---|
| `05_quality/definition_of_ready.md` | DoR padrão reusável por sprints |
| `05_quality/definition_of_done.md` | DoD padrão reusável por sprints |
| `05_quality/completeness_criteria.md` | Critérios por tipo de artefato |
| `05_quality/quality_gates.md` | *Gates* por categoria (perf, security, observability, docs, compliance) |
| `05_quality/test_strategy.md` | Taxonomia de testes, *coverage targets*, *mutation*, chaos |
| `05_quality/fitness_functions.md` | Fitness functions automatizadas (§32) |
| `05_quality/runbook_template.md` | Template canônico de runbook |
| `05_quality/incident_response.md` | Processo de incident response + post-mortem (§31) |

### 4.7 Artefatos transversais (fora dos 5 níveis)

Além dos 5 níveis hierárquicos, `specs/` contém 5 diretórios transversais que suportam a infraestrutura do framework:

| Diretório | Propósito | Incluído em CI validation? |
|---|---|---|
| `specs/_schemas/` | JSON Schemas canônicos (ex: `front_matter.schema.json`) usados pra validar YAML dos outros docs. Ver §7.11.6. | ❌ (é o validador, não o validado) |
| `specs/_templates/` | Templates canônicos (sprint, WI, ST, ADR, PRR, waiver) com placeholders. Instâncias reais vão pros níveis 3/4/5 conforme `type`. | 🟡 valida YAML parseability, não schema completo (§7.11.6) |
| `specs/_waivers/` | Waivers ativos e históricos (§35.6). Formato `WAIVER-YYYYMMDD-NNN-slug.md`. | ✅ valida contra schema com `type: waiver` |
| `specs/_audits/` | Relatórios de auditoria gerados por revisores externos (GPT, humanos). Sem front matter obrigatório. | ❌ (não é spec normativo) |
| `specs/_archive/` | Specs históricas preservadas para rastreamento mas não mais canônicas. | ❌ |

**Regra:** os 5 níveis hierárquicos são **prescritivos** (descrevem o produto); os 5 diretórios transversais são **operacionais** (suportam o processo). Ambos coexistem em `specs/`, mas servem papéis diferentes.

---

## 5. Rastreabilidade Bidirecional

### 5.1 Cadeia de derivação canônica

```
Persona ──► JTBD ──► CAP ──► { NFR, INV, Constraint } ──► ADR ──► C4 Componente
                                                                       │
                                                                       ▼
                                                     Sprint WI ──► Código + Testes
                                                                       │
                                                                       ▼
                                        Observability (métrica, log, trace, alert, dashboard)
```

### 5.2 Matriz de rastreabilidade obrigatória

Cada sprint mantém matriz:

| CAP | JTBD | INV | NFR | ADR | Componente C4 | WI | Código | Testes | Métricas | Alertas | Runbook |
|---|---|---|---|---|---|---|---|---|---|---|---|

### 5.3 Tooling de validação (CI)

Repositório **DEVE** incluir script que valida:

- [ ] Toda CAP referenciada em ≥ 1 sprint.
- [ ] Toda INV verificada em ≥ 1 teste (unit, property, integration, ou chaos).
- [ ] Toda NFR verificada em ≥ 1 benchmark ou test de carga.
- [ ] Todo ADR `doc_status: FROZEN` referenciado em ≥ 1 spec ou componente C4.
- [ ] Toda *trace annotation* em código (`// trace: CAP-XXX`) aponta para artefato existente.
- [ ] Todo referência `ADR-XXXX`, `CAP-XXX`, `INV-XXX` em qualquer doc aponta para ID existente.

Script: `scripts/trace_check.py` ou `scripts/trace_check.rs` (a ser implementado no Sprint S-00).

---

## 6. Numeração e Identificadores Estáveis

### 6.1 Formatos canônicos

| Tipo de artefato | Formato | Zero-pad | Agrupamento opcional |
|---|---|---|---|
| Persona | `PERSONA-XX` | 2 dígitos | — |
| Job To Be Done | `JTBD-XXX` | 3 dígitos | — |
| Capability | `CAP-XXX` | 3 dígitos | `CAP-{SUBSISTEMA}-XXX` permitido |
| Non-Functional Req | `NFR-XXX` | 3 dígitos | — |
| Invariant | `INV-XXX` | 3 dígitos | — |
| Constraint | `CNS-XXX` | 3 dígitos | — |
| Architecture Decision | `ADR-XXXX` | 4 dígitos | — |
| Product Decision | `PDR-XXXX` | 4 dígitos | — |
| Operational Decision | `ODR-XXXX` | 4 dígitos | — |
| Sprint | `S-XX` | 2 dígitos | — |
| Work Item | `WI-SXX-NNN` | sprint parent + 3 dígitos | — |
| Risk (sprint-local) | `R-XXX` | 3 dígitos | — |
| Failure Mode | `FM-XXX` | 3 dígitos | — |
| Success Metric | `METRIC-XXX` | 3 dígitos | — |
| Principle (framework) | `PRINC-XXX` | 3 dígitos | — |
| Anti-pattern | `AP-XXX` | 3 dígitos | — |
| Process Rule | `REG-XXX-XXX` | agrupado + 3 | ex: `REG-WB-001` |
| Sprint Invariant | `SPRINT-INV-XXX` | 3 dígitos | — |
| SLI/SLO | `SLI-XXX`, `SLO-XXX` | 3 dígitos | — |
| Runbook | `RB-XXX` | 3 dígitos | — |
| Incident | `INC-YYYYMMDD-NNN` | data + seq | — |
| Fitness Function | `FF-XXX` | 3 dígitos | — |
| Evidence Type | `EVT-XXX` | 3 dígitos | ver §35.7 — enum fechado definido no framework |
| Waiver | `WAIVER-YYYYMMDD-NNN` | data + seq | ver §35.6 — formato `WAIVER-<created YYYYMMDD>-<seq do dia>` |
| Risk Lane Forcing Factor | `FF-HR-NNN` | 3 dígitos | ver §33.5.3 — enum fechado no framework |
| Risk Lane | `LOW_RISK \| STANDARD \| HIGH_RISK` | enum literal | ver §33.5 — não é ID sequencial mas sim enum |

### 6.2 Regras absolutas

- **REG-NUM-001**: **NUNCA** renumerar.
- **REG-NUM-002**: **NUNCA** reusar ID descontinuado.
- **REG-NUM-003**: Atribuição é sequencial. Novo ID = `max + 1` dentro do namespace.
- **REG-NUM-004**: Reservar ID antes de escrever (evita colisão em trabalho paralelo) via `docs/next_ids.md` com *append-only* list.
- **REG-NUM-005**: Placeholders (`CAP-TBD-xxx`, `ADR-????-xxx`) são **proibidos** em documento não-`DRAFT`.

### 6.3 Ciclo de status por ID

Todo artefato identificado segue o `doc_status` canônico (§7.1):

```
DRAFT ──► REVIEW ──► FROZEN ──► (DEPRECATED | SUPERSEDED)
                       │
                       └──► THAWED ──► REVIEW ──► FROZEN (nova versão)
```

Para artefatos de **trabalho** (Sprint, WI, ST, PRR), existe eixo adicional `work_status` independente — ver §7.2.

### 6.4 Resolução de conflitos em numeração paralela

Quando múltiplos autores podem reservar ID simultaneamente:

1. Cada autor cria *draft* com ID reservado via PR.
2. CI valida ausência de colisão; em colisão, último PR recebe `max + 1` automaticamente.
3. Reserva expira se *draft* não avançar em 14 dias.

---

## 7. Ciclo de Vida — Separação entre `doc_status` e `work_status`

> **PRINCÍPIO FUNDACIONAL:** Todo artefato tem **dois eixos de estado independentes**:
>
> 1. **`doc_status`** — estado do *documento em si* (está escrito? revisado? aprovado? imutável?). Aplica-se a **todos** os artefatos (framework, ADR, PDR, ODR, Sprint Contract, WI, ST, PRR, specs de Níveis 1–5, runbooks, etc.).
> 2. **`work_status`** — estado da *execução do trabalho* descrito no documento (pendente? em andamento? bloqueado? concluído?). Aplica-se **apenas aos artefatos de trabalho** (Sprint, WI, ST, PRR).
>
> Essa separação existe porque um documento de sprint pode estar `doc_status=FROZEN` (contrato assinado, imutável) enquanto o `work_status=IN_PROGRESS` (execução em andamento). Misturar os dois eixos — como a v0.2 fazia — gera contradições irresolvíveis ("DONE equivale a FROZEN?").

### 7.1 Ciclo de vida documental (`doc_status`)

Aplica-se a **todo e qualquer** documento do sistema de spec. Este é o eixo canônico.

| `doc_status` | Semântica | Pode ser referenciado por downstream? | Imutável? |
|---|---|---|---|
| `DRAFT` | Em escrita ativa | ❌ | ❌ |
| `REVIEW` | Submetido a revisão formal | ❌ | ❌ (só autor + revisores mudam) |
| `FROZEN` | Aprovado e imutável | ✅ | ✅ |
| `THAWED` | Reaberto temporariamente; downstream em auditoria | ❌ | ❌ |
| `SUPERSEDED` | Substituído por versão nova | ❌ (referencie o *superseder*) | ✅ (histórico) |
| `DEPRECATED` | Obsoleto, não substituído | ⚠️ com aviso | ✅ |

### 7.2 Ciclo de vida de trabalho (`work_status`)

Aplica-se **apenas aos artefatos de trabalho**. Cada tipo tem seu próprio conjunto de estados, mas todos compartilham estrutura: iniciam em algum estado "pré-execução", transicionam para "em execução", terminam em "concluído" ou "abortado".

#### 7.2.1 Sprint Contract (`work_status`)

| Estado | Semântica |
|---|---|
| `PROPOSED` | Sprint contract criado mas DoR não verificado |
| `READY` | DoR 100% ✅, pronto para execução |
| `IN_PROGRESS` | Execução iniciada |
| `REVIEWING` | DoD em verificação (dry run) |
| `COMPLETE` | DoD 100% ✅ |
| `SEALED` | Sign-off completo + retrospectiva feita; sprint imutável |
| `FAILED` | Não cumpriu DoD no prazo + não foi estendido; aprendizado registrado |

#### 7.2.2 Work Item (`work_status`)

| Estado | Semântica |
|---|---|
| `PROPOSED` | WI criado dentro de um sprint proposto |
| `READY` | DoR do WI ✅; pode ser atribuído |
| `DOING` | Assignee executando |
| `REVIEWING` | PR aberto em review |
| `BLOCKED` | Dependência upstream ou bloqueio externo impedindo progresso |
| `DONE` | DoD do WI 100% ✅, PR mergeado |
| `CANCELED` | WI descartado (obsoleto ou desnecessário) |
| `ROLLED_BACK` | WI foi `DONE` mas precisou ser revertido; aprendizado em post-mortem |

#### 7.2.3 Sub-task (`work_status`)

| Estado | Semântica |
|---|---|
| `TODO` | Pendente |
| `DOING` | Em execução |
| `REVIEW` | PR em revisão |
| `BLOCKED` | Dependência impedindo progresso |
| `DONE` | Completeness criteria 100% ✅ + PR mergeado no branch do WI pai |
| `CANCELED` | ST descartada |

#### 7.2.4 Production Readiness Review (`work_status`)

| Estado | Semântica |
|---|---|
| `NOT_STARTED` | PRR documento criado mas nenhum review realizado |
| `IN_REVIEW` | Reviewers avaliando gates |
| `CONDITIONALLY_APPROVED` | Aprovado com caveats (ver §8 de PRR). Deploy permitido apenas em canary ≤ 10% até resolução dos caveats com expiration-date obrigatória |
| `APPROVED` | Aprovado pleno. Pode promover até GA (100%) |
| `REJECTED` | Reprovado. Não pode promover em nenhuma porcentagem |

### 7.3 Mapeamento entre tipos de artefato e eixos de estado

| Tipo de artefato | `doc_status`? | `work_status`? | Notas |
|---|---|---|---|
| Framework (este doc) | ✅ | ❌ | Documento normativo, sem "trabalho executável" |
| Nível 1 — Visão (PR/FAQ, Personas, JTBD, Metrics) | ✅ | ❌ | Documentos descritivos |
| Nível 2 — Capabilities, NFRs, Invariants, Constraints, User Journeys | ✅ | ❌ | Documentos descritivos |
| Nível 3 — ADRs, PDRs, ODRs, C4, Data Model, Protocols, Failure Modes, Security/Privacy Models, SLO Catalog | ✅ | ❌ | Documentos decisórios/descritivos. ADR/PDR/ODR usam `doc_status=FROZEN` em vez do termo histórico "ACCEPTED" (ver §7.4 abaixo) |
| Nível 4 — Sprint Contract | ✅ | ✅ | Duplo eixo obrigatório |
| Nível 4 — Work Item | ✅ | ✅ | Duplo eixo obrigatório |
| Nível 4 — Sub-task | ✅ | ✅ | Duplo eixo obrigatório |
| Nível 4 — Production Readiness Review | ✅ | ✅ | Duplo eixo obrigatório |
| Nível 5 — Quality Framework (DoR, DoD, Quality Gates, Test Strategy, Fitness Functions, Runbooks) | ✅ | ❌ | Documentos normativos |

### 7.4 ADR / PDR / ODR — reconciliação com terminologia histórica

A literatura clássica de ADRs (Michael Nygard) usa `Status: PROPOSED → ACCEPTED → DEPRECATED/SUPERSEDED`. Esses termos mapeiam 1:1 para o nosso `doc_status` canônico:

| Termo histórico ADR | `doc_status` canônico CoreLink |
|---|---|
| `PROPOSED` | `DRAFT` ou `REVIEW` (conforme o ADR esteja em escrita ou submetido a revisão) |
| `ACCEPTED` | `FROZEN` |
| `DEPRECATED` | `DEPRECATED` |
| `SUPERSEDED` | `SUPERSEDED` |

**Regra:** ADRs **DEVEM** usar `doc_status` canônico no cabeçalho. O termo "ACCEPTED" **PODE** aparecer em prosa ou change log histórico, desde que o cabeçalho mostre `doc_status: FROZEN`. Templates atuais devem ser atualizados.

### 7.5 Transições de `doc_status`

```
     ┌──────────────────────────────┐
     ▼                              │
DRAFT ──► REVIEW ──► FROZEN ──► THAWED ──► REVIEW ──► FROZEN (nova versão)
              │                 │
              │                 └──► SUPERSEDED (quando substituído)
              │                 │
              │                 └──► DEPRECATED (obsoleto sem substituto)
              │
              └──► DRAFT (se rejeitado)
```

### 7.6 Transições de `work_status` — regra geral

`work_status` é monotonicamente progressivo com possibilidade de regressão controlada:

- **Sprint:** `PROPOSED → READY → IN_PROGRESS → REVIEWING → COMPLETE → SEALED`. Pode ir para `FAILED` de qualquer estado ativo.
- **WI:** `PROPOSED → READY → DOING → REVIEWING → DONE`. Pode entrar/sair `BLOCKED` a qualquer momento em `DOING/REVIEWING`. Pode ir `CANCELED` de qualquer estado não-final. Pode ir `ROLLED_BACK` a partir de `DONE`.
- **ST:** `TODO → DOING → REVIEW → DONE`. Pode entrar/sair `BLOCKED`. Pode ir `CANCELED` de qualquer estado não-final.
- **PRR:** `NOT_STARTED → IN_REVIEW → (CONDITIONALLY_APPROVED | APPROVED | REJECTED)`. `CONDITIONALLY_APPROVED` expira numa data definida e vira `IN_REVIEW` automaticamente se caveats não resolvidos.

### 7.7 Invariante cruzada: `doc_status` × `work_status`

> **INV-LIFECYCLE-001:** Um artefato de trabalho **NÃO PODE** ter `work_status` ∈ {`IN_PROGRESS`, `DOING`, `REVIEWING`, `DONE`, `COMPLETE`, `SEALED`, `APPROVED`, `CONDITIONALLY_APPROVED`} enquanto seu `doc_status` ≠ `FROZEN`.
>
> Justificativa: execução só começa sobre contrato imutável. Contrato ainda em revisão (`doc_status=DRAFT|REVIEW`) não é contrato, é rascunho.
>
> Verificação: CI `trace-check` valida a combinação em todo PR que altera estado.

### 7.8 Autorização de transições (`doc_status`)

| Transição | Autorizado |
|---|---|
| `DRAFT → REVIEW` | Autor |
| `REVIEW → FROZEN` | Aprovador Final + ≥ N revisores requeridos ✅ (N definido por tipo de doc) |
| `REVIEW → DRAFT` | Qualquer revisor com *dissent* `MUST_FIX` |
| `FROZEN → THAWED` | Aprovador Final, após justificativa escrita |
| `THAWED → REVIEW` | Autor |
| `FROZEN → SUPERSEDED` | Aprovador Final (ao aprovar *superseder*) |
| `FROZEN → DEPRECATED` | Aprovador Final, após ADR/PDR justificando |

### 7.9 Autorização de transições (`work_status`)

| Transição | Autorizado |
|---|---|
| `PROPOSED → READY` (Sprint/WI) | Sprint Owner + Tech Lead, após DoR 100% ✅ |
| `READY → IN_PROGRESS/DOING` | Sprint Owner (Sprint) ou WI Owner (WI) |
| `* → BLOCKED` | Qualquer assignee, com razão registrada |
| `BLOCKED → (estado anterior)` | Assignee, após bloqueio resolvido |
| `DOING/REVIEWING → DONE` (WI) | WI Owner, após DoD 100% ✅ + sign-off |
| `COMPLETE → SEALED` (Sprint) | Aprovador Final, após retrospectiva |
| `* → FAILED` (Sprint) | Aprovador Final, com learnings registrados |
| `* → CANCELED` | Sprint Owner / WI Owner, com justificativa |
| `DONE → ROLLED_BACK` (WI) | Aprovador Final, após post-mortem de incident vinculado |

### 7.10 Obrigações de *thaw* (`FROZEN → THAWED`)

Ao executar `doc_status: FROZEN → THAWED`, **obrigatoriamente**:

1. **Criar issue/ticket** documentando: razão, escopo de mudança, *rollback plan* se mudança quebrar algo.
2. **Listar downstream dependentes** (ferramenta: *trace checker*).
3. **Marcar todos downstream `doc_status: FROZEN`** como `AUDIT_PENDING` (metadata, não `doc_status`).
4. **Re-validar consistência** antes de re-*freeze*.
5. **Comunicar stakeholders impactados** via canal definido.
6. **Incrementar versão** apropriadamente (major se semântica mudou).
7. **Pausar sprints em `work_status: IN_PROGRESS`** cujos WIs dependam do documento sob *thaw* (enforcement de INV-LIFECYCLE-001).

### 7.11 Marcação obrigatória em documento

> **REGRA INVIOLÁVEL:** todo documento **DEVE** começar com YAML front matter **real** (delimitado por `---` no topo absoluto do arquivo, **antes** do H1) que parseia com `yaml.safe_load`. Acompanhado de bloco humano-legível **após** o H1 para leitura rápida.

#### 7.11.1 Campos obrigatórios para TODOS os documentos

| Campo | Tipo | Obrigatório | Valores / Observação |
|---|---|---|---|
| `id` | string | ✅ | ID canônico (PRINC-006) |
| `type` | string | ✅ | `framework \| adr \| pdr \| odr \| capability \| nfr \| invariant \| personas \| jtbd \| prfaq \| success_metrics \| constraints \| user_journey \| c4 \| protocol \| failure_modes \| security_model \| privacy_model \| observability_model \| slo_catalog \| resilience_patterns \| compliance_matrix \| data_model \| sprint \| work_item \| sub_task \| prr \| quality_definition \| runbook \| fitness_function \| incident` |
| `doc_status` | enum | ✅ | `DRAFT \| REVIEW \| FROZEN \| THAWED \| SUPERSEDED \| DEPRECATED` |
| `audit_status` | enum | ✅ | `ACTIVE \| AUDIT_PENDING \| AUDITED` |
| `version` | semver string | ✅ | `X.Y.Z` |
| `created` | ISO date | ✅ | `YYYY-MM-DD` |
| `updated` | ISO date | ✅ | `YYYY-MM-DD` |
| `owner` | string | ✅ | Nome |
| `final_approver` | string | ✅ | Nome |
| `reviewers` | lista de `{role, name}` | ✅ | Pode ser `[]` se ainda não designado |
| `supersedes` | string, lista de strings, ou null | ✅ | ID(s) do(s) doc(s) substituído(s). Lista para consolidação N→1 (ex: WI split reverso). |
| `superseded_by` | string, lista de strings, ou null | ✅ | ID(s) do(s) doc(s) que substitui(em) este. Lista para split 1→N (ex: REG-WI-SPLIT-001). |
| `tags` | lista de strings | ✅ | Pode ser `[]` |
| `inherits_from` | lista de strings | opcional; ✅ quando aplicável (ver §35.5) | IDs de fontes canônicas das quais este doc herda controles (ex: `["OBSERVABILITY-MODEL", "SLO-CATALOG"]`). Ver §35.5 Control Inheritance. |
| `local_deltas` | lista de strings | opcional | Descrições curtas de onde este doc diverge/estende as fontes herdadas. Ver §35.5.3. |

#### 7.11.2 Campos adicionais para artefatos de TRABALHO (Nível 4)

Docs com `type ∈ {sprint, work_item, sub_task, prr}` **DEVEM** adicionar:

| Campo | Tipo | Obrigatório | Valores |
|---|---|---|---|
| `work_status` | enum | ✅ em todos os tipos de trabalho | Específico por tipo — ver §7.2 |
| `lane` | enum | ✅ em `sprint`, `work_item`, `sub_task`; **N/A** em `prr` | `LOW_RISK | STANDARD | HIGH_RISK`. Para `sub_task`, apenas `LOW_RISK | STANDARD` (REG-LANE-004). Ver §33.5. |
| `lane_forcing_factors` | lista de `FF-HR-NNN` | ✅ se `lane=HIGH_RISK`; **proibido** se outra lane | Ex: `["FF-HR-002", "FF-HR-005"]`. Ver §33.5.3 para enum. |
| `parent` | string | ✅ para `work_item` (→ sprint) e `sub_task` (→ WI); **N/A** para `sprint` e `prr` | ID do parent hierárquico. PRR usa `feature_wi` em vez de `parent` (relação lateral, não hierárquica). |
| `feature_wi` | string | ✅ apenas para `prr`; **proibido** nos demais | ID do WI que a PRR certifica pra produção. |
| `capabilities` | lista de strings | ✅ apenas para `prr`; **proibido** nos demais | Lista de IDs de capabilities certificadas por esta PRR (ex: `["CAP-XXX", "CAP-YYY"]`). |
| `prod_target_date` | ISO date | ✅ apenas para `prr`; **proibido** nos demais | Data alvo de GA em produção (`YYYY-MM-DD`). |
| `assignee` | string | ✅ em `work_item` e `sub_task`; **N/A** em `sprint` e `prr` | Nome do assignee principal |

#### 7.11.3 Template de front matter (TODOS os docs)

```yaml
---
id: "<ID canônico>"
type: "<type>"
doc_status: "<estado>"          # ver §7.1
audit_status: "<estado>"        # ver §40.9
version: "X.Y.Z"
created: "YYYY-MM-DD"
updated: "YYYY-MM-DD"
owner: "<nome>"
final_approver: "<nome>"
reviewers:
  - role: "<role>"
    name: "<nome>"
supersedes: null                 # "<ID>" (escalar) | ["<ID1>","<ID2>"] (lista N→1) | null
superseded_by: null              # "<ID>" (escalar) | ["<ID1>","<ID2>"] (lista 1→N, ex: WI split) | null
tags: []
---
```

#### 7.11.4 Template adicional para Nível 4

```yaml
# adicionar aos campos §7.11.3, conforme o type:

work_status: "<estado>"          # OBRIGATÓRIO em sprint/work_item/sub_task/prr — ver §7.2

# Apenas work_item (parent=sprint) e sub_task (parent=WI):
parent: "<ID>"

# Apenas work_item e sub_task:
assignee: "<nome>"

# Apenas prr (relação lateral, não hierárquica — substitui `parent`):
feature_wi: "<WI-SXX-NNN>"
capabilities:
  - "<CAP-XXX>"
prod_target_date: "YYYY-MM-DD"
```

#### 7.11.5 Bloco humano-legível (após H1)

```markdown
> **doc_status:** <estado>
> **work_status:** <estado>              # se aplicável
> **audit_status:** <estado>
> **Versão:** X.Y.Z
> **Última atualização:** YYYY-MM-DD
> **Owner:** <nome>
> **Aprovador Final:** <nome>
> **Revisores:** <lista>
> **Supersedes:** <ID | —>
> **Superseded By:** <ID | —>
```

#### 7.11.6 Validação automática

CI **DEVE** rodar `scripts/validate_specs.py`, que aplica duas camadas de validação:

1. **Parseabilidade YAML** em todos os docs de `specs/` (exceto `_audits/`, `_archive/`, `_schemas/`): front matter **DEVE** parsear com `yaml.safe_load`.
2. **JSON Schema** (`specs/_schemas/front_matter.schema.json`, draft 2020-12) em todos os docs **exceto** `_templates/` (templates têm placeholders intencionais).

Campos validados pelo schema:

- Enums: `doc_status`, `work_status`, `audit_status`, `type`, `lane`
- Formato: `version` (semver `X.Y.Z`), `created`/`updated`/`prod_target_date`/`expires_at` (ISO `YYYY-MM-DD`)
- Padrões de ID: `feature_wi` (`WI-SXX-NNN`), `capabilities` (`CAP-XXX`), `parent`, `predecessor_sprints`
- Cross-field rules: PRR **DEVE** ter `feature_wi`+`capabilities`+`prod_target_date` e **NÃO PODE** ter `parent`/`assignee`; Waiver **DEVE** ter `gates_waived`+`compensating_control`+`expires_at`+`revalidation_trigger`+`rationale`; Sprint **NÃO PODE** ter `parent`; etc.
- Tipos união: `supersedes` / `superseded_by` aceitam `null | string | lista de strings`
- Estruturas: `reviewers` é lista de `{role, name}`

Schema canônico em `specs/_schemas/front_matter.schema.json`.

Executar manualmente:

```bash
# Instalar dependências (primeira vez):
python3 -m venv .venv-specs
source .venv-specs/bin/activate
pip install jsonschema pyyaml

# Validar:
python3 scripts/validate_specs.py           # silencioso se OK
python3 scripts/validate_specs.py -v        # verbose (lista cada arquivo)
python3 scripts/validate_specs.py --strict  # aplica schema também aos templates (falha esperada)
```

Falha em CI = **merge bloqueado**.

### 7.12 SLA de auditoria pós-*thaw*

| Tipo de mudança | SLA para re-*freeze* downstream auditado |
|---|---|
| Patch (tipográfico) | ≤ 24h |
| Minor (adição não-breaking) | ≤ 7 dias |
| Major (breaking, semântica muda) | ≤ 30 dias; sprints dependentes pausam até conclusão (ver §7.10.7) |

---

## 8. Relações Entre Artefatos

### 8.1 Taxonomia de relações

| Relação | Semântica | Exemplo |
|---|---|---|
| `derives_from` | B é derivado conceitualmente de A | CAP derives_from JTBD |
| `refines` | B é versão mais específica de A | c4_components refines c4_containers |
| `implements` | B é implementação de A | Código implements CAP |
| `verifies` | B prova/testa A | Teste verifies INV |
| `constrains` | B restringe escolha em A | NFR constrains ADR |
| `supersedes` | B substitui A (A vira `SUPERSEDED`) | ADR-0007 supersedes ADR-0002 |
| `depends_on` | B não funciona sem A | Sprint depends_on ADR |
| `impacts` | Mudança em A afeta B | ADR impacts Sprint |
| `mentions` | Referência informativa sem dependência | Doc mentions ADR |

### 8.2 Regras de navegação

- **REG-REL-001**: Toda relação **DEVE** ser bidirecional (A relaciona com B ↔ B referencia A de volta).
- **REG-REL-002**: *Trace checker* **DEVE** validar bidirecionalidade.
- **REG-REL-003**: Remover relação requer *thaw* de ambos os documentos.

### 8.3 Visualização

Grafo de relações **DEVE** ser gerável automaticamente via script. Formato: Mermaid `graph` ou GraphViz.

---

# Parte III — Linguagem e Vocabulário

## 9. RFC 2119 — Vocabulário Normativo

### 9.1 Palavras canônicas

Usadas em **MAIÚSCULAS** quando em contexto normativo.

| EN | pt-BR | Semântica |
|---|---|---|
| MUST | **DEVE** | Obrigatório absoluto |
| MUST NOT | **NÃO DEVE** | Proibido absoluto |
| REQUIRED | **OBRIGATÓRIO** | Sinônimo de MUST |
| SHALL | **SERÁ** | Sinônimo de MUST (formal) |
| SHOULD | **DEVERIA** | Fortemente recomendado; exceções requerem justificativa registrada |
| SHOULD NOT | **NÃO DEVERIA** | Fortemente desencorajado |
| RECOMMENDED | **RECOMENDADO** | Sinônimo de SHOULD |
| MAY | **PODE** | Opcional; implementadores escolhem |
| OPTIONAL | **OPCIONAL** | Sinônimo de MAY |

### 9.2 Exemplos

✅ "O servidor **DEVE** rejeitar *requests* com tokens expirados retornando HTTP 401 com *body* JSON contendo `{ \"code\": \"token_expired\" }`."

✅ "O cliente **PODE** implementar *retry* com *exponential backoff* com *jitter* conforme RFC 7234 §5.6."

✅ "O servidor **DEVERIA** comprimir *responses* com `Content-Length` ≥ 1 KiB usando *zstd* ou *gzip*, respeitando o header `Accept-Encoding` do cliente."

❌ "O servidor pode retornar erro 500 em casos de falha." — "pode" ambíguo (permissão? capacidade? RFC 2119 MAY?). Reescrever.

### 9.3 Palavras proibidas em spec

| Expressão | Motivo | Substituir por |
|---|---|---|
| "deveria ser rápido" | não mensurável | "**DEVE** servir P99 ≤ 50 ms sob carga ≤ NFR-XXX" |
| "idealmente" | *wishful* | "**DEVE**" ou "**DEVERIA**" (escolher) |
| "talvez" | ambíguo | remover; tomar decisão |
| "provavelmente" | ambíguo | remover; quantificar |
| "etc." | incompleto | enumerar exaustivamente |
| "e outros" | incompleto | enumerar exaustivamente |
| "TBD" | *placeholder* | decidir ou mover para issue com ID |
| "basicamente" | *filler* | remover |
| "simplesmente" | *filler* | remover |
| "óbvio/obviamente" | presunção | explicar |
| "trivialmente" | presunção | explicar ou remover |
| "algumas" (ex: "algumas vezes falha") | quantidade inexata | quantificar |
| "muitos" | quantidade inexata | quantificar |
| "geralmente" | condição ambígua | condicionar explicitamente |

---

## 10. Política de Linguagem

### 10.1 Regras

- **REG-LANG-001**: Prosa corrida, seções, explicações conceituais: **pt-BR**.
- **REG-LANG-002**: Termos-da-arte da indústria: **EN** sem tradução. Exemplos: *hash*, *digest*, *idempotent*, *throughput*, *observability*, *tenant*, *content-addressable*, REAPI, gRPC, *blast radius*, *one-way door*.
- **REG-LANG-003**: Nomes de componentes, classes, serviços, variáveis, funções: **EN** (code tokens).
- **REG-LANG-004**: Mensagens de erro e logs emitidos pelo sistema: **EN** (padrão de indústria para infra; clientes são devs globais).
- **REG-LANG-005**: Palavras RFC 2119 em **MAIÚSCULAS** em seu equivalente pt-BR ou EN, consistente **dentro do mesmo documento**.
- **REG-LANG-006**: Nomes próprios e marcas preservam capitalização oficial (Cloudflare, Rust, Bazel, gRPC).
- **REG-LANG-007**: Expressões idiomáticas em EN preservadas quando não têm equivalente conciso em pt-BR (*blast radius*, *rabbit hole*, *foot gun*).

### 10.2 Exemplos

✅ "O *tenant* isolamento é invariante **INV-042**. O servidor **DEVE** validar o *bearer token* e extrair `tenant_id` da *claim* `org_id` antes de acessar qualquer *blob* no R2."

❌ "O inquilino isolamento é invariante INV-042. O servidor deveria validar o token e pegar o id do inquilino da declaração `org_id`." — traduzir "tenant", "token", "claim" empobrece; "deveria" ambíguo.

---

## 11. Writing Style Guide

### 11.1 Regras de estrutura

- **REG-STYLE-001**: Todo documento **DEVE** ter sumário se >300 linhas.
- **REG-STYLE-002**: Seções **DEVEM** ter numeração hierárquica.
- **REG-STYLE-003**: Cabeçalhos **DEVEM** ser declarativos (ex: "Tratamento de falhas", não "Como tratar falhas?").
- **REG-STYLE-004**: Parágrafos **DEVEM** ter ≤ 5 frases.
- **REG-STYLE-005**: Frases **DEVEM** ter ≤ 30 palavras em média (aceitável ≤ 40 em casos específicos).
- **REG-STYLE-006**: Usar *bullet lists* quando há ≥ 3 itens paralelos.
- **REG-STYLE-007**: Usar tabelas quando há comparação ou enumeração com atributos múltiplos.
- **REG-STYLE-008**: Código inline em `backticks`. Blocos ≥ 3 linhas em code fences com linguagem especificada.

### 11.2 Voz e tom

- **REG-STYLE-009**: Voz **ativa**, não passiva. ✅ "O servidor valida." ❌ "A validação é feita."
- **REG-STYLE-010**: Tempo **presente do indicativo** para descrever comportamento do sistema. ✅ "O servidor retorna 401." ❌ "O servidor retornará 401."
- **REG-STYLE-011**: **Segunda pessoa** ("você") aceitável em docs de uso/runbook. **Terceira pessoa** para specs arquiteturais.
- **REG-STYLE-012**: Sem humor, sem *snark*, sem emojis (exceto ✅/❌/⚠️/🟢/🟡/🔴 em checklists, status, severidade).

### 11.3 Clareza e precisão

- **REG-STYLE-013**: Primeira menção a termo técnico: **definir ou linkar glossário**.
- **REG-STYLE-014**: Acrônimos: expandir na primeira ocorrência (CAS = *Content-Addressable Storage*).
- **REG-STYLE-015**: Magic numbers: sempre justificar (ex: "chunks de 2 MiB — escolha via ADR-0015").
- **REG-STYLE-016**: Unidades: usar notação SI (`KiB`, `MiB`, `GiB` para base 2; `KB`, `MB`, `GB` para base 10 — seguir RFC 3092/IEC 80000).
- **REG-STYLE-017**: Datas: ISO 8601 (`2026-04-24`).
- **REG-STYLE-018**: Percentuais: sempre com base explícita ("p99 ≤ 50 ms" não "p99 é rápido").

### 11.4 Diagramas na prosa

- **REG-STYLE-019**: Todo diagrama **DEVE** ter: título, legenda, descrição textual equivalente.
- **REG-STYLE-020**: Diagramas **DEVEM** ser fonte-texto (Mermaid, PlantUML) para *diff*-ability.

### 11.5 Linter (vale)

- Lista de termos proibidos (§9.3) aplicada via *vale*.
- Lista de termos-que-precisam-de-capitalização (Rust, Cloudflare, gRPC) aplicada via *vale*.
- Regras de estilo de prosa em `.vale.ini`.

---

## 12. Ubiquitous Language (DDD)

### 12.1 Princípio

Seguimos *Domain-Driven Design* (Evans): **todo termo de domínio tem uma e apenas uma definição**, usada de forma consistente em specs, código, UI, logs, métricas, conversas.

### 12.2 Regras

- **REG-DDD-001**: Todo termo de domínio **DEVE** aparecer no Glossário (§40).
- **REG-DDD-002**: O mesmo conceito **NÃO DEVE** ter nomes diferentes em áreas diferentes.
  - ❌ "organization" em código, "tenant" em doc, "workspace" em UI.
  - ✅ Escolher **um**: *tenant*. Usar em todo lugar.
- **REG-DDD-003**: O mesmo nome **NÃO DEVE** referir conceitos diferentes.
  - ❌ "blob" referindo tanto ao *content-addressable object* quanto ao *raw byte stream*.
- **REG-DDD-004**: Quando evoluímos vocabulário, fazemos *rename* **completo** (código, docs, UI, logs) via ADR explícito.
- **REG-DDD-005**: *Bounded contexts* diferentes **PODEM** usar o mesmo termo com significados diferentes, **DESDE QUE** o *bounded context* esteja explícito no contexto da frase.

### 12.3 Exemplos canônicos do CoreLink

| Conceito | Termo canônico | Termos proibidos |
|---|---|---|
| Entidade isolada (usuário ou org) | **tenant** | organization, account, workspace, customer (exceto em billing) |
| Hash identificador de *blob* | **digest** | hash, checksum, id |
| Bloco de conteúdo em CAS | **blob** | object, file, chunk (chunk tem significado distinto) |
| Subdivisão Merkle de blob grande | **chunk** | piece, segment, part |
| Árvore Merkle representando blob | **manifest** | tree, index |
| Função de hash | **digest function** | hash function, checksum algorithm |

---

# Parte IV — Rigor Formal

## 13. Formal Methods e TLA+

### 13.1 Quando usar formal methods

**PRINC-016** estabelece: invariants CRITICAL **DEVERIAM** ter spec formal.

Exemplos típicos no CoreLink:

- **INV-TenantIsolation**: "Em todo estado do sistema, nenhum *blob* de tenant A é acessível por principal de tenant B."
- **INV-CASIdempotency**: "Upload do mesmo *blob* N vezes resulta em uma única representação em storage."
- **INV-AuditLogImmutability**: "Registros de *audit log*, uma vez escritos, não podem ser alterados."
- **INV-QuotaEnforcement**: "Em nenhum estado, tenant consome mais que sua *quota* configurada."

### 13.2 Ferramenta canônica: TLA+

Leslie Lamport's TLA+ para *model checking* de invariants e *liveness properties*.

- Arquivos: `.tla` (specs) + `.cfg` (model config).
- Localização: `specs/03_architecture/formal/INV-XXX-name.tla`.
- *Model checker*: TLC (incluso com TLA Toolbox).
- Execução: integrada em CI para invariants CRITICAL.

### 13.3 Template mínimo TLA+

```tla
---- MODULE INV_TenantIsolation ----
EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS Tenants, Blobs

VARIABLES ownership, access_attempts

TypeOK ==
  /\ ownership \in [Blobs -> Tenants]
  /\ access_attempts \subseteq [principal: Tenants, blob: Blobs, granted: BOOLEAN]

TenantIsolation ==
  \A att \in access_attempts:
    att.granted => (att.principal = ownership[att.blob])

Init == ...
Next == ...
Spec == Init /\ [][Next]_<<ownership, access_attempts>>

INVARIANT TypeOK
INVARIANT TenantIsolation
====
```

### 13.4 Quando NÃO usar formal methods

- Invariants SEVERITY = MEDIUM/LOW: prosa RFC 2119 + property test é suficiente.
- Invariants que mudam frequentemente: custo de manter spec formal > benefício.
- Invariants testáveis exaustivamente: *property-based testing* pode ser suficiente.

### 13.5 Integração com testes

Toda TLA+ spec **DEVE** ter:

- Property test correspondente em Rust (`proptest`) para validação em runtime.
- Referência cruzada no invariant declaration: `§13-file: INV_TenantIsolation.tla`.

---

## 14. Quality Standards ISO/IEC 25010

### 14.1 Taxonomia oficial

NFRs **DEVEM** ser categorizados conforme ISO/IEC 25010 (Product Quality Model):

| Characteristic | Sub-characteristics |
|---|---|
| **Functional Suitability** | Completeness, Correctness, Appropriateness |
| **Performance Efficiency** | Time behaviour, Resource utilization, Capacity |
| **Compatibility** | Co-existence, Interoperability |
| **Usability** | Appropriateness recognizability, Learnability, Operability, User error protection, User interface aesthetics, Accessibility |
| **Reliability** | Maturity, Availability, Fault tolerance, Recoverability |
| **Security** | Confidentiality, Integrity, Non-repudiation, Accountability, Authenticity |
| **Maintainability** | Modularity, Reusability, Analysability, Modifiability, Testability |
| **Portability** | Adaptability, Installability, Replaceability |

### 14.2 Template de NFR

```markdown
## NFR-XXX: {nome}

- **ISO 25010 category:** {characteristic} / {sub-characteristic}
- **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
- **audit_status:** ACTIVE | AUDIT_PENDING | AUDITED
- **Capabilities afetadas:** CAP-XXX, CAP-YYY
- **Invariants relacionados:** INV-XXX
- **SLI relacionado (se aplicável):** SLI-XXX

### Enunciado normativo
O sistema **DEVE/DEVERIA** {comportamento mensurável}.

### Métrica
{definição operacional da métrica}

### Baseline atual
{valor medido atualmente, ou "N/A (greenfield)"}

### Alvo
{valor alvo com janela temporal}

### Janela de medição
{ex: rolling 30 dias}

### Consequência de violação
- Severidade: CRITICAL | HIGH | MEDIUM | LOW
- Ação automática se violado: {ex: paginar on-call}

### Método de verificação
- Benchmark: {arquivo/função}
- Load test: {arquivo/função}
- Observação em produção: {dashboard, alerta}
```

---

## 15. Types of Decision Records

### 15.1 Taxonomia

Não toda decisão é arquitetural. Diferenciamos:

| Tipo | Prefixo | Escopo | Exemplo |
|---|---|---|---|
| **Architecture Decision Record** | `ADR-XXXX` | Técnica / arquitetural | Escolha de Rust, R2 como storage, REAPI como protocolo |
| **Product Decision Record** | `PDR-XXXX` | Produto / estratégia | Pricing, tiers, o que entra em qual tier, política de *deprecation* |
| **Operational Decision Record** | `ODR-XXXX` | Operações / processo | SLO target, janela de manutenção, política de *on-call* |

### 15.2 Quando criar um DR

> **DEVE** ser registrado como DR (ADR/PDR/ODR) qualquer decisão que:
>
> - Afeta invariants, NFRs ou trade-offs importantes.
> - Introduz *lock-in* significativo.
> - Tem *blast radius* além de um módulo.
> - Custaria >1 semana de engenheiro para reverter.
> - Afeta experiência de cliente visivelmente.
> - Tem implicação legal/regulatória.

### 15.3 Quando NÃO criar DR

- Escolhas locais de implementação (nome de variável, refactor interno).
- Decisões que o código torna óbvias e que qualquer engenheiro tomaria igual.

### 15.4 Template

Ver `_templates/adr.md` (serve para ADR, PDR, ODR com adaptações mínimas).

---

# Parte V — Preocupações Transversais (Cross-Cutting)

## 16. Cross-Cutting Concerns — Visão Geral

### 16.1 Conceito

Certas preocupações atravessam **múltiplos componentes e múltiplas capabilities**. São ortogonais ao mapeamento funcional. Tratá-las como *afterthought* ou como capability comum leva a inconsistências e *gaps* de cobertura.

### 16.2 Lista canônica de cross-cutting concerns do CoreLink

| ID | Concern | Seção |
|---|---|---|
| CC-SEC | Segurança | §17 |
| CC-PRIV | Privacidade | §18 |
| CC-THREAT | Threat Modeling | §19 |
| CC-COMP | Compliance | §20 |
| CC-DATA | Data Governance | §21 |
| CC-SRE | Service Level Methodology | §22 |
| CC-RESIL | Reliability & Resilience | §23 |
| CC-PERF | Performance | §24 |
| CC-A11Y | Accessibility & i18n | §25 |
| CC-SUPPLY | Supply Chain Security | §26 |
| CC-AI | AI/LLM Governance | §27 |

### 16.3 Regra de aplicação

- **REG-CC-001**: Toda capability **DEVE** ser avaliada contra cada CC relevante.
- **REG-CC-002**: Ausência de impacto em CC **DEVE** ser explicitamente registrada ("CC-PRIV: not applicable — no PII handled"), não omitida.

---

## 17. Segurança

### 17.1 Princípios de segurança (adicionais a PRINC-019–021)

- **Defense in Depth**: múltiplas camadas de controle independentes.
- **Fail Secure**: falhas levam a negação, não a permissão (PRINC-024).
- **Separation of Duties**: operações críticas requerem múltiplos principals.
- **Explicit Trust**: todo trust boundary é documentado e auditado.
- **Secure Defaults**: configuração padrão = mais segura (PRINC-019).
- **Least Functionality**: habilitar apenas o necessário.

### 17.2 Frameworks de referência adotados

- **OWASP ASVS 4.0** — *Application Security Verification Standard*.
- **OWASP Top 10** — cobertura obrigatória.
- **NIST Cybersecurity Framework** — maturidade em *Identify/Protect/Detect/Respond/Recover*.
- **CIS Benchmarks** — configurações seguras para componentes (Rust, Linux, Postgres, Docker).
- **SLSA** — *Supply-chain Levels for Software Artifacts* (§26).

### 17.3 Controles mínimos obrigatórios

| Controle | Escopo | Verificação |
|---|---|---|
| TLS 1.3+ em todo *endpoint* | exposto externamente | SAST + DAST |
| AuthN em todo *endpoint* | interno ou externo | integration test |
| AuthZ baseada em *tenant* | toda operação que toca dados | property test INV-TenantIsolation |
| *Rate limiting* por *tenant* | todo *endpoint* custoso | load test |
| *Audit log* imutável | operações sensíveis | integration test + test de imutabilidade |
| *Input validation* | toda fronteira externa | fuzz test |
| *Secrets* em *secrets manager* | todo credencial | `git-secrets` + `trufflehog` CI |
| Dependências com vuln scan | todo crate/lib | `cargo audit` CI |
| Containers com *base image* scaneada | Dockerfile | `trivy` CI |

### 17.4 Classificação de dados (ver §21)

Toda *capability* que processa dados **DEVE** classificar:

- **Público**: exposto livremente.
- **Interno**: uso interno HuGR, sem *PII*.
- **Confidencial**: *tenant data*, sem *PII* específico.
- **Restrito**: *PII*, credenciais, dados regulados (HIPAA/GDPR/LGPD sensitive categories).

### 17.5 Referência ao Security Model

Detalhes de *trust boundaries*, controles específicos, *threat model* vivem em `03_architecture/security_model.md`, frozen em Nível 3.

---

## 18. Privacidade

### 18.1 Os 7 Princípios de Cavoukian (Privacy by Design)

**PRINC-022** reforça:

1. **Proativo, não reativo**: antecipar privacy issues, não reagir a violações.
2. **Privacidade como *default***: máximo de privacidade sem ação do usuário.
3. **Privacidade embutida no *design***: não *bolt-on*.
4. **Funcionalidade total — positive-sum**: privacidade + utilidade; não trade-off.
5. **Segurança *end-to-end***: ciclo de vida completo do dado.
6. **Transparência**: usuário sabe o que é coletado, onde vai, quem acessa.
7. **Respeito ao usuário**: *user-centric*, direitos respeitados (acesso, retificação, exclusão, portabilidade).

### 18.2 Direitos do titular de dados (GDPR / LGPD)

| Direito | Implementação obrigatória |
|---|---|
| Acesso | API que retorna todos dados do *tenant* estruturados |
| Retificação | UI/API para corrigir dados próprios |
| Exclusão ("direito ao esquecimento") | API + processo automatizado, com SLA declarado (ex: ≤ 30 dias) |
| Portabilidade | Export em formato aberto (JSON, CSV) |
| Oposição | Opt-out de processamentos específicos |
| Revogação de consentimento | Interface e API |

### 18.3 Data Processing Agreement (DPA)

- **DEVE** existir DPA padrão com clientes (GDPR Art. 28).
- **DEVE** existir lista de *sub-processors* pública e atualizada.

### 18.4 Referência ao Privacy Model

Detalhes de *data flows*, classificação, LINDDUN threats, controles: `03_architecture/privacy_model.md`, frozen em Nível 3.

---

## 19. Threat Modeling

### 19.1 Metodologias adotadas

| Metodologia | Aplicação |
|---|---|
| **STRIDE** (Microsoft) | Security threats em cada componente e *trust boundary* |
| **LINDDUN** | Privacy threats em cada *data flow* com dado pessoal |
| **Attack Trees** | Ameaças complexas multi-passo |
| **PASTA** | Análise de risco quando *blast radius* justificar |

### 19.2 STRIDE — categorias

| Letra | Categoria | Violação do princípio |
|---|---|---|
| S | Spoofing | Authenticity |
| T | Tampering | Integrity |
| R | Repudiation | Non-repudiation |
| I | Information Disclosure | Confidentiality |
| D | Denial of Service | Availability |
| E | Elevation of Privilege | Authorization |

**REG-TM-001**: Todo componente em `c4_components.md` **DEVE** ter análise STRIDE (6 categorias × componente), com mitigação declarada para cada ameaça identificada (ou aceitação de risco registrada).

### 19.3 LINDDUN — categorias (privacy)

| Letra | Categoria |
|---|---|
| L | Linkability |
| I | Identifiability |
| N | Non-repudiation (privacy sense) |
| D | Detectability |
| D | Disclosure of information |
| U | Unawareness |
| N | Non-compliance |

**REG-TM-002**: Todo *data flow* com PII **DEVE** ter análise LINDDUN.

### 19.4 Cadência

- Threat model atualizado a cada *freeze* de `c4_components.md` ou quando capability nova afeta *trust boundary*.
- Red team review (§38) semestral do threat model.

---

## 20. Compliance e Regulatório

### 20.1 Regulações e certificações relevantes

| Framework | Aplicável? | Gate para |
|---|---|---|
| **GDPR** (EU) | Sim (usuários EU) | Launch em EU |
| **LGPD** (BR) | Sim (mercado-alvo) | Launch em BR |
| **CCPA** (Califórnia) | Sim | Launch em US |
| **HIPAA** (US Health) | Condicional | Tier que aceita BAA |
| **SOC 2 Type II** | Planejado | Enterprise tier |
| **ISO 27001** | Planejado | Enterprise tier |
| **PCI-DSS** | Indireto (Stripe PCI) | Scope limitado via SAQ-A |

### 20.2 Matriz de controles

- **DEVE** existir `03_architecture/compliance_matrix.md` mapeando cada controle requerido × implementação × evidência.
- **DEVE** ser atualizada em toda nova *capability* que afeta compliance.

### 20.3 Audit trail

- **DEVE** toda operação sensível emitir *audit event* imutável.
- **DEVE** *audit log* ser retido pelo prazo regulatório mínimo aplicável (ex: 7 anos para certos registros HIPAA).

---

## 21. Data Governance

### 21.1 Classificação de dados

| Classe | Exemplos | Controles mínimos |
|---|---|---|
| **Público** | Documentação pública, press releases | Nenhum |
| **Interno** | Métricas agregadas não-identificadas | Acesso interno HuGR |
| **Confidencial** | Tenant data (builds, pacotes, metadata) | AuthN/Z + encryption at rest + audit |
| **Restrito** | PII, credenciais, dados regulados | Confidencial + MFA + dedicated retention + separation of duties |

### 21.2 Data lifecycle

```
Collect ──► Process ──► Store ──► (optionally) Share ──► Retain ──► Delete
```

Cada estágio **DEVE** ter controle de classe.

### 21.3 Data contracts

- **DEVE** existir contrato explícito entre produtor e consumidor de dados (schema + semântica + SLA de frescor).
- **DEVE** mudanças em data contracts seguir *semver* + *expand-contract migration*.

### 21.4 Schema evolution

- **DEVE** toda migration ser *additive-first* (adicionar antes de remover).
- **DEVE** existir janela de compatibilidade declarada para cada mudança breaking (mínimo 90 dias para clientes externos).

### 21.5 Data retention matrix

Exemplo parcial:

| Categoria | Retention | Deleção automática |
|---|---|---|
| CAS blob *tenant* ativo | Indefinida (enquanto *tenant* ativo) | N |
| CAS blob após cancelamento | 30 dias | S |
| Audit log | 2 anos | S |
| Access logs (HTTP) | 90 dias | S |
| Traces | 14 dias | S |
| Usage events (billing) | 7 anos | S (após export fiscal) |

---

## 22. Service Level Methodology — SRE

### 22.1 Conceitos (Google SRE Book)

| Conceito | Definição |
|---|---|
| **SLI** (Service Level Indicator) | Métrica quantitativa de aspecto do serviço |
| **SLO** (Service Level Objective) | Alvo ou range de SLI, com janela temporal |
| **SLA** (Service Level Agreement) | SLO contratual com cliente, com penalidade |
| **Error Budget** | `1 - SLO` — quanto de "falha" é aceitável no período |

### 22.2 Os Four Golden Signals (Google SRE)

Métricas mínimas obrigatórias para todo serviço:

1. **Latency** — tempo de processamento de *requests* bem-sucedidas.
2. **Traffic** — demanda no sistema (req/s, B/s).
3. **Errors** — taxa de falhas.
4. **Saturation** — quão cheio o sistema está (CPU, memory, queue depth).

**REG-SLO-001**: Todo serviço **DEVE** expor métricas dos *four golden signals*.

### 22.3 SLO Catalog

`03_architecture/slo_catalog.md` contém:

| SLI-ID | Definition | SLO | Window | Error Budget | Burn Rate Alerts |
|---|---|---|---|---|---|
| SLI-001 | `rate(req_success) / rate(req_total)` | ≥ 99.9% | rolling 30d | 43.2m/mês | 2% in 1h, 5% in 6h |

### 22.4 Error Budget Policy

**REG-SLO-002**: Quando *error budget* se esgota, **DEVE** haver *feature freeze* até reestabelecer baseline. Política operacional, não sugestão.

### 22.5 Four Golden Signals aplicados ao CoreLink

| Signal | SLI principal |
|---|---|
| Latency | P99 de `CAS::BatchReadBlobs` |
| Traffic | GB/s de bandwidth servido |
| Errors | Taxa de 5xx + falha de autenticação legítima |
| Saturation | Utilização CPU do Container + fila de requests pendentes |

---

## 23. Reliability & Resilience Patterns

### 23.1 Patterns obrigatórios

| Pattern | Quando aplicar | Referência |
|---|---|---|
| **Timeout** | Toda chamada externa | Release It! (Nygard) |
| **Retry with exponential backoff + jitter** | Retries idempotentes | AWS Architecture Blog |
| **Circuit Breaker** | Chamadas a deps externos instáveis | Nygard |
| **Bulkhead** | Isolar recursos entre classes de trabalho | Nygard |
| **Rate limiting** | Toda API pública | PRINC-019 |
| **Graceful degradation** | Features não-críticas podem cair sem derrubar sistema | — |
| **Back-pressure** | Quando consumer é mais lento que producer | Reactive Streams |
| **Load shedding** | Sob saturação, rejeitar antes de colapso | SRE Book |

### 23.2 Patterns proibidos

| Anti-pattern | Por que |
|---|---|
| **Infinite retry** | Amplifica falhas transitórias em catastrófico |
| **Unbounded queues** | Mascara falha até explodir |
| **Shared mutable state sem lock** | Race conditions |
| **Silent failures** | Deve sempre emitir métrica/log |

### 23.3 Chaos Engineering

- **DEVE** sprint que introduz nova failure mode incluir chaos test.
- **DEVE** chaos tests rodar em staging com frequência mínima mensal.
- Ferramentas: `chaos-mesh`, toolkit próprio, fault injection via `tower::layer`.

---

## 24. Performance e Capacity Planning

### 24.1 Metodologia

- **Baseline** medido antes de otimização (PRINC-029).
- **Load profile** declarado: RPS esperado, distribuição de request size, padrão temporal (steady, diurnal, burst).
- **Stress test** até 3× o pico esperado.
- **Soak test** ≥ 24h para detectar leaks.

### 24.2 Capacity planning

- **DEVE** existir modelo de capacidade por tier (Free/Solo/Team/Business/Enterprise).
- **DEVE** alertas ativos antes de 70% capacidade planejada.

### 24.3 Performance budgets

| Tipo de endpoint | Budget de latência | Budget de memória |
|---|---|---|
| Metadata lookup (KV) | P99 ≤ 10ms | N/A |
| CAS FindMissingBlobs | P99 ≤ 50ms | ≤ 16MiB per request |
| CAS BatchReadBlobs (small) | P99 ≤ 100ms | ≤ 64MiB per request |
| ByteStream Read (large) | Throughput ≥ 100 MiB/s | streaming, bounded |

---

## 25. Accessibility e Internationalization

### 25.1 Accessibility (a11y)

- Aplicável a Dashboard, docs, landing page.
- **DEVE** seguir WCAG 2.1 AA no mínimo, AAA onde viável.
- **DEVE** testes automatizados (axe-core) em CI do frontend.
- **DEVE** inclusive testes manuais com leitor de tela em releases menores.

### 25.2 Internationalization (i18n)

- **DEVE** Dashboard suportar pt-BR, EN desde day 1.
- **DEVE** mensagens de erro HTTP/gRPC serem em EN (padrão de indústria).
- **PODE** CLI localizar mensagens por `LANG`.
- **DEVE** datas em ISO 8601; números em formato local via biblioteca.

### 25.3 Localization (l10n)

- Tradução gerenciada via ferramenta (ex: Crowdin, Lokalise).
- *Context* fornecido a tradutores.
- Re-translation obrigatória a cada UI change.

---

## 26. Supply Chain Security

### 26.1 Framework: SLSA

Adotamos **SLSA** (*Supply-chain Levels for Software Artifacts*). Alvo: SLSA Level 3 para builds de produção até GA.

### 26.2 Controles obrigatórios

| Controle | Implementação |
|---|---|
| **SBOM** para cada release | Ferramenta: `cargo sbom` ou equivalente; formato SPDX |
| **Signed artifacts** | `cosign` assina binários e images |
| **Reproducible builds** | Build isolado, sem dependências mutáveis |
| **Pinned dependencies** | `Cargo.lock` comitado; versões exatas |
| **Dep vulnerability scan** | `cargo audit` + `cargo deny` em CI |
| **License compliance** | `cargo deny` com lista permitida de licenças |
| **No direct untrusted deps** | PR que adiciona dep passa por revisão explícita |
| **Provenance** | Build attestations via GitHub OIDC + Sigstore |

### 26.3 Incident de supply chain

- Runbook específico para dependência comprometida.
- Capacidade de *rollback* + rebuild limpo em ≤ 4h.

---

## 27. AI/LLM Governance

### 27.1 Motivação

HuGR opera produtos com IA (Donna, Forge). CoreLink **pode** integrar com ferramentas IA (ex: análise de dependências sugerida por LLM, detecção de vulnerabilidade assistida). Governança desde day 1.

### 27.2 Princípios

- **PRINC-031 (futuro, candidato)** — AI features são explicáveis ou marcadas como *black box* com *fallback*.
- **PRINC-032 (futuro, candidato)** — AI não toma decisões irreversíveis sobre *tenant data*.
- **PRINC-033 (futuro, candidato)** — Modelos e prompts são versionados como código.

### 27.3 Políticas

- **Prompt injection defense**: inputs controlados, output sanitizado.
- **PII handling**: dados de *tenant* **NÃO DEVEM** ser enviados a LLMs externos sem *opt-in* explícito.
- **Model versioning**: toda AI feature declara versão exata do modelo e prompt usado.
- **Eval harness**: AI features têm *eval suite* rodando em CI.

---

# Parte VI — Engenharia e Entrega

## 28. Feature Lifecycle

### 28.1 Estados de uma feature

```
PROPOSED ──► ALPHA ──► BETA ──► GA ──► DEPRECATED ──► REMOVED
                 │       │
                 └───┴──► (pode voltar a PROPOSED se kill)
```

| Estado | Semântica | SLA | Visibilidade |
|---|---|---|---|
| **PROPOSED** | Em spec, não implementada | N/A | interna |
| **ALPHA** | Implementada, quebra tolerada | no SLA | opt-in (flag) |
| **BETA** | Estável o suficiente para testar externamente | SLO reduzido | opt-in público |
| **GA** | Production-ready, SLA completo | SLO full | default-available |
| **DEPRECATED** | Ainda funciona, mas sem evolução; fim anunciado | SLO mantido até end-of-life | aviso ativo |
| **REMOVED** | Desligada | N/A | 404 / erro |

### 28.2 Política de *deprecation*

- **DEVE** *deprecation* ser anunciada com ≥ 12 meses de antecedência para features GA.
- **DEVE** existir migration path documentado.
- **DEVE** telemetria ativa de uso residual durante período de *deprecation*.

### 28.3 Breaking changes

- **NÃO DEVE** breaking changes em APIs GA sem bump de versão major + período de coexistência.
- **DEVE** haver política de *semver* para APIs públicas.

---

## 29. Progressive Delivery

### 29.1 Estratégias (PRINC-027)

| Estratégia | Quando usar |
|---|---|
| **Canary** | Rollout incremental, % progressiva (1% → 10% → 50% → 100%) com *auto-rollback* |
| **Blue-green** | Swap de versão inteira; rollback instantâneo via DNS |
| **Rolling** | Gradual replacement de instâncias |
| **Feature flags** | Decouple deploy de release; ativar por *tenant*, *region*, % |
| **Shadow traffic** | Novo sistema recebe tráfego paralelo sem afetar resposta; compara outputs |

### 29.2 Regras

- **REG-PD-001**: Toda mudança em API de produção **DEVE** usar *canary* ou *blue-green*.
- **REG-PD-002**: Toda mudança de schema **DEVE** seguir *expand-contract* (§21.4).
- **REG-PD-003**: Feature flags **DEVEM** ter owner, *kill date*, e ser removidas após *kill date*.

### 29.3 Taxonomia de feature flags

| Tipo | Propósito | Vida típica |
|---|---|---|
| **Release flag** | Decouple deploy/release | dias-semanas |
| **Experiment flag** | A/B test | semanas |
| **Permission flag** | Liberar para subset de clientes | longa |
| **Ops flag** | Kill switch, tuning | indefinida |

---

## 30. Test Strategy Philosophy

### 30.1 Taxonomia de testes

```
         ▲ rare/slow/expensive
         │
         │   ┌───────────────┐
         │   │  Chaos / DR   │
         │   ├───────────────┤
         │   │  E2E          │
         │   ├───────────────┤
         │   │  Integration  │
         │   ├───────────────┤
         │   │  Contract     │
         │   ├───────────────┤
         │   │  Property     │
         │   ├───────────────┤
         │   │  Unit         │
         │   └───────────────┘
         │
         ▼ frequent/fast/cheap
```

### 30.2 Formato de shape: Trophy Testing

Adotamos **Testing Trophy** (Kent C. Dodds, adaptado para backend):

- **Base sólida de Unit + Property** para lógica.
- **Grande centro de Integration + Contract** para confiança em boundaries.
- **E2E + Chaos** mais raros mas essenciais para *critical user journeys*.

### 30.3 Acceptance criteria em Gherkin

**REG-TEST-001**: *Acceptance criteria* em Work Items **DEVEM** usar sintaxe Given-When-Then:

```gherkin
Given {contexto/estado inicial}
When {ação}
Then {resultado observável}
And {resultado adicional}
```

### 30.4 Mutation testing

- Obrigatório para paths críticos (auth, tenant isolation, billing).
- Target: kill rate ≥ 75%.
- Ferramenta: `cargo-mutants`.

### 30.5 Property-based testing

- Obrigatório para todo invariant declarado.
- Iterations ≥ 1000 por teste.
- Ferramenta: `proptest`.

### 30.6 Contract testing

- Obrigatório para toda integração com spec formal (REAPI, OCI, npm).
- Ferramenta: test suites oficiais dos protocolos.

---

## 31. Incident Response e Post-Mortem

### 31.1 Severidade

| Severidade | Impacto |
|---|---|
| **SEV-1** | Outage total, data loss, security breach confirmado |
| **SEV-2** | Degradação grave, features críticas indisponíveis |
| **SEV-3** | Degradação parcial, SLO em risco mas não violado |
| **SEV-4** | Issue menor, sem impacto imediato em clientes |

### 31.2 Response

- **DEVE** SEV-1/2 ter *Incident Commander* dedicado.
- **DEVE** comunicação pública (status page) para SEV-1/2 em ≤ 30 min.
- **DEVE** MTTA (Mean Time To Acknowledge), MTTD (Detect), MTTR (Resolve) medidos e trackados.

### 31.3 Post-Mortem

- **DEVE** post-mortem obrigatório para SEV-1/2; opcional SEV-3.
- **DEVE** post-mortem *blameless* (PRINC-028).
- **DEVE** conter: *timeline*, *impact*, *root cause*, *contributing factors*, *action items* com owner e prazo.
- **DEVE** *action items* entrarem em backlog priorizado.

### 31.4 Runbooks

- **DEVE** cada *alert* ter *runbook* linkado.
- **DEVE** *runbook* ser testável via *dry-run* (simulação).
- **DEVE** *runbooks* serem revisados trimestralmente.

---

## 32. Architecture Fitness Functions

### 32.1 Conceito

*Fitness functions* (Neal Ford et al.) são **checks automatizados** que validam propriedades arquiteturais continuamente.

### 32.2 Exemplos para CoreLink

| FF-ID | Propriedade | Verificação |
|---|---|---|
| FF-001 | Tenant isolation | Property test com 2 tenants fictícios, 10k iterations |
| FF-002 | Módulos respeitam camadas (UI → App → Domain → Infra) | `cargo-modules` + custom check |
| FF-003 | Nenhuma dep proibida (ex: `openssl` direto, usar `rustls`) | `cargo deny` |
| FF-004 | Binary size ≤ X MB | CI check pós-build |
| FF-005 | Cold start ≤ Y ms | Benchmark em CI |
| FF-006 | Nenhuma função >100 linhas | Linter |
| FF-007 | Nenhum arquivo >1000 linhas | Linter |

### 32.3 Cadência

- **DEVEM** fitness functions rodar em CI em cada PR.
- **DEVEM** quebras bloquear merge.

---

## 33. Technical Debt Management

### 33.1 Registro

- **DEVE** toda dívida técnica consciente ser registrada em `specs/technical_debt.md`.
- **DEVE** cada item ter: contexto, custo de pagamento estimado, custo de carregar, prazo proposto.

### 33.2 Classificação

| Tipo | Exemplo |
|---|---|
| **Deliberate, prudent** | "Fizemos X rápido para lançar; sabemos que precisa Y depois" |
| **Deliberate, reckless** | "Não temos tempo para design" — evitar |
| **Inadvertent, prudent** | "Agora sabemos como deveríamos ter feito" |
| **Inadvertent, reckless** | "Nem sabíamos" — evitar via educação |

### 33.3 Sprint budget para dívida

- **DEVE** sprint alocar ≥ 10% de capacidade para dívida técnica prioritária.
- **NÃO DEVE** dívida ser ignorada por >3 sprints consecutivos.

---

## 33.5 Risk Lanes — Classificação de Trabalho por Risco

> **Propósito:** nem toda unidade de trabalho carrega o mesmo risco nem merece o mesmo peso de gates. Impor o contrato completo (33 seções, 12 sign-offs, chaos, PRR, cost analysis) a uma correção tipográfica é **cerimônia sem rigor** (AP-007) e treina o time a burlar o processo. Impor o contrato mínimo a uma mudança que toca `tenant isolation` é **rigor superficial** (AP-019 adjacente).
>
> Este framework define **três lanes** que classificam WI, ST e Sprint por risco, com matriz explícita de gates obrigatórios por lane.

### 33.5.1 As três lanes

| Lane | Semântica | Exemplos típicos |
|---|---|---|
| `LOW_RISK` | Blast radius local; reversível trivialmente; zero customer impact direto | Correção de typo, update de docstring, refactor interno de função, teste adicional, rename de variável local, pagamento de dívida pequena |
| `STANDARD` | Blast radius subsystem/system; reversível (two-way ou hybrid); customer impact indireto ou limitado | Nova feature de produto, mudança de infra, otimização de performance, nova métrica, nova capability user-facing sem breaking change |
| `HIGH_RISK` | Blast radius product/organization; one-way door (ou hybrid com janela curta); customer impact material, regulatório ou inter-tenant | Mudança em `tenant isolation`, breaking change em API pública, migration destrutiva, novo controle SOC 2/HIPAA, feature que processa PII nova categoria, mudança de auth model, one-way vendor lock-in, GC/retention policy |

### 33.5.2 Critérios de classificação (scoring)

Calcula-se score a partir de 5 dimensões. Valor final determina lane **base** (antes de forcing factors §33.5.3).

| Dimensão | Valores |
|---|---|
| **Blast radius** (§5.3) | local=0, subsystem=1, system=2, product=3, organization=4 |
| **Reversibility** (§5.4) | two-way=0, hybrid=1, one-way=3 |
| **Customer tiers afetados** | nenhum=0, 1 tier=1, multi-tier=2, todos os tiers=3 |
| **Compliance triggers** (§5.6) | +1 por regulação aplicável (GDPR, LGPD, HIPAA, SOC 2, PCI) |
| **Cross-tenant risk** | não=0, sim=3 |

| Score total | Lane base |
|---|---|
| 0–2 | `LOW_RISK` |
| 3–7 | `STANDARD` |
| 8+ | `HIGH_RISK` |

### 33.5.3 Forcing factors (forçam `HIGH_RISK` mesmo com score baixo)

Qualquer item abaixo **OBRIGATORIAMENTE** classifica o trabalho como `HIGH_RISK`, independente do score:

- **FF-HR-001**: Reversibility = `one-way door` com custo de reversão > 5 engineer-weeks
- **FF-HR-002**: Toca `tenant isolation` (INV-TenantIsolation)
- **FF-HR-003**: Introduz ou altera processamento de PII / PHI / dados regulados por HIPAA, GDPR sensitive categories, LGPD dados sensíveis
- **FF-HR-004**: Breaking change em API pública sem período de coexistência ≥ 90 dias
- **FF-HR-005**: Altera controle de segurança declarado em `security_model.md` (auth, authz, audit, encryption)
- **FF-HR-006**: Altera retention policy ou garbage collection de dados de customer
- **FF-HR-007**: Muda SLO declarado em `slo_catalog.md` pra pior (relaxa)
- **FF-HR-008**: Introduz dependência em vendor com lock-in ≥ 12 meses pra migrar
- **FF-HR-009**: Muda contrato com customer (ToS, SLA, DPA)
- **FF-HR-010**: Primeiro WI/Sprint a tocar categoria regulatória nova pro produto
- **FF-HR-011**: Altera algoritmo de garbage collection, refcount, ou invariante de reachability (INV-GC-*) com blast radius em integridade de dados (adicionado v0.5.0 via ADR-0012; correlaciona com FM-300, FM-303, FM-404)

Forcing factors **DEVEM** ser listados explicitamente no YAML de qualquer WI/Sprint classificado `HIGH_RISK`, usando campo `lane_forcing_factors: ["FF-HR-001", ...]`.

### 33.5.4 Matriz de gates por lane

Esta é a **fonte canônica** de quais seções de WI, ST e PRR são obrigatórias por lane. Templates reproduzem apenas o necessário; divergências entre framework e template são resolvidas a favor do framework.

#### 33.5.4.1 Sections obrigatórias em Work Item (§0–§32 ver `_templates/work_item.md`)

| Seção do WI | `LOW_RISK` | `STANDARD` | `HIGH_RISK` |
|---|---|---|---|
| §0 Identificação | ✅ | ✅ | ✅ |
| §1 Intent | ✅ | ✅ | ✅ |
| §2 Narrative | 🟡 (prosa curta, ≤ 100 palavras) | ✅ (300–500) | ✅ (300–500 + risk justification) |
| §3 Customer Impact & Journey | ⛔ N/A | ✅ | ✅ |
| §4 Capability Mapping / Trace | ✅ | ✅ | ✅ |
| §5 Tipo e Classificação | ✅ (+ `lane: LOW_RISK`) | ✅ | ✅ (+ `lane_forcing_factors`) |
| §6 Escopo | ✅ | ✅ | ✅ |
| §7 Anti-Scope | 🟡 (≥ 1 item) | ✅ | ✅ |
| §8 Acceptance Criteria (Gherkin) | ✅ (≥ 1 AC) | ✅ | ✅ |
| §9 Design Decisions | 🟡 (inline, sem ADR) | ✅ | ✅ (≥ 1 ADR se decisão arquitetural) |
| §10 Completeness Criteria SOTA | 10.1+10.2+10.3+10.11 obrigatórias; demais N/A | ✅ todas aplicáveis | ✅ TODAS |
| §11 Definition of Done | ✅ | ✅ | ✅ |
| §12 Invariants | 🟡 (apenas operacionais do WI) | ✅ | ✅ (+ formal spec se toca INV crítico) |
| §13 Artifacts Produced | ✅ | ✅ | ✅ |
| §14 Quality Standards SOTA | 14.1+14.2 | ✅ | ✅ TODAS (inclui 14.9 sustainability) |
| §15 Chaos Experiments | ⛔ N/A | 🟡 (se toca I/O) | ✅ obrigatório |
| §16 Production Readiness Review | ⛔ N/A | 🟡 (se customer-facing) | ✅ obrigatório |
| §17 Sub-tasks | ✅ (se decomposto) | ✅ | ✅ |
| §18 Dependencies | ✅ | ✅ | ✅ |
| §19 Effort Estimate | ✅ (size T-shirt) | ✅ (PERT) | ✅ (PERT + histórico) |
| §20 Time-boxing & Escalation | ✅ | ✅ | ✅ |
| §21 Observability Plan | 🟡 (se emite nova métrica/log/trace) | ✅ | ✅ |
| §22 Cost Analysis | ⛔ N/A | ✅ | ✅ (+ TCO 12m) |
| §23 API / Contract Impact | ⛔ N/A | 🟡 (se toca API) | ✅ obrigatório |
| §24 Post-mortem Hooks | ⛔ N/A | ✅ | ✅ |
| §25 Rollback / Recovery | 🟡 (declarar reversibilidade) | ✅ | ✅ (+ teste de rollback) |
| §26 Security & Privacy | ⛔ N/A | 🟡 (STRIDE mini) | ✅ STRIDE + LINDDUN completos |
| §27 Knowledge Transfer | ⛔ N/A | ✅ | ✅ + onboarding test |
| §28 Risk Register | 🟡 (riscos óbvios) | ✅ | ✅ + detectability + exposure + residual |
| §29 Review Checkpoints | ✅ (code review) | ✅ (design + code + pre-merge) | ✅ + adversarial review |
| §30 Sign-off | ver §33.5.4.3 | ver §33.5.4.3 | ver §33.5.4.3 |
| §31 Change Log | ✅ | ✅ | ✅ |
| §32 Apêndice Anti-patterns | 🟡 (se explícito) | ✅ | ✅ |

**Legenda:** ✅ obrigatório · 🟡 condicional (com critério explicitado) · ⛔ N/A (seção pode ser omitida ou marcada como "N/A: LOW_RISK lane")

#### 33.5.4.2 Sections obrigatórias em Sub-task (§0–§18)

`LOW_RISK` ST obrigatórias: §0, §1, §2, §3, §6, §7 (subáreas 7.1, 7.2, 7.3, 7.7), §8, §11, §17, §18.

`STANDARD` ST obrigatórias: todas §0–§18 aplicáveis.

`HIGH_RISK` ST: sempre promovida a WI próprio (REG-WI-002c abaixo). ST **NÃO PODE** carregar classificação `HIGH_RISK` — se surgir, decompor.

#### 33.5.4.3 Matriz de sign-offs por lane

| Papel (§30) | `LOW_RISK` | `STANDARD` | `HIGH_RISK` |
|---|---|---|---|
| WI Owner | ✅ | ✅ | ✅ |
| Code Reviewer | ✅ | ✅ | ✅ |
| Security Reviewer | ⛔ | 🟡 se §5.6 indicar | ✅ obrigatório |
| Privacy Reviewer | ⛔ | 🟡 se PII | ✅ obrigatório se PII/PHI |
| SRE Reviewer | ⛔ | ✅ | ✅ |
| QA Reviewer | ⛔ (tests passam no CI) | ✅ | ✅ |
| Product Reviewer | ⛔ | ✅ | ✅ |
| Architect | ⛔ | 🟡 se toca C4 component | ✅ obrigatório |
| Cost Owner | ⛔ | 🟡 se custo > R$500/mês | ✅ obrigatório |
| Legal | ⛔ | ⛔ | 🟡 se muda contrato/DPA |
| Sprint Owner | ✅ | ✅ | ✅ |
| Aprovador Final | ✅ | ✅ | ✅ |
| **Total mínimo** | **3** | **5–8** | **10–12** |

### 33.5.5 Registro da lane

- **REG-LANE-001**: Todo Sprint, WI e ST **DEVE** ter campo YAML `lane: LOW_RISK | STANDARD | HIGH_RISK` no front matter.
- **REG-LANE-002**: Schema `front_matter.schema.json` valida o enum.
- **REG-LANE-003**: `HIGH_RISK` **DEVE** ser acompanhado de campo `lane_forcing_factors` listando IDs de FF-HR-* aplicáveis.
- **REG-LANE-004**: Sub-task **NÃO PODE** ter `lane: HIGH_RISK` — alta severidade exige WI dedicado.

### 33.5.6 Classificação e promoção/demoção

- **REG-LANE-010**: Lane inicial é decidida pelo WI Owner no momento de criação, usando scoring §33.5.2 + forcing factors §33.5.3. Revisão pelo Sprint Owner.
- **REG-LANE-011**: Demotion (ex: `HIGH_RISK → STANDARD`) durante execução **DEVE** ser aprovada pelo Aprovador Final + justificada por escrito em §32 (ou apêndice de dissent). Não é decisão unilateral do owner.
- **REG-LANE-012**: Promotion (ex: `LOW_RISK → STANDARD`) é sempre permitida unilateralmente; ausência de promoção quando um forcing factor surge é violação de processo (tratada como incidente SEV-3).
- **REG-LANE-013**: Mudança de lane **DEVE** disparar re-verificação dos gates adicionais da nova lane antes de prosseguir.

### 33.5.7 Ferramenta de classificação (suggested)

Repositório **DEVERIA** incluir script `scripts/classify_lane.py` que:

1. Lê front matter do WI.
2. Extrai blast_radius, reversibility, customer_tiers_affected, compliance_triggers, cross_tenant_risk de campos explícitos (ou prompts).
3. Calcula score.
4. Verifica forcing factors.
5. Emite sugestão de lane + justificativa.

Não-bloqueante em CI (humano decide), mas reduz atrito de classificação.

### 33.5.8 Auditoria trimestral

- **DEVE** Aprovador Final revisar trimestralmente distribuição de lanes concluídas.
- **Red flags a investigar:**
  - Predominância de `LOW_RISK` (>70%): possível subclassificação crônica.
  - Predominância de `HIGH_RISK` (>25%): possível superclassificação (carga desnecessária).
  - `STANDARD` próximo de `HIGH_RISK` repetidamente: threshold de scoring pode estar mal calibrado; considerar recalibrar §33.5.2.

---

# Parte VII — Processo

## 34. Templates

### 34.1 Sprint Contract

Ver `_templates/sprint_contract.md` — template canônico com **23 seções totais (§0–§22)** governando um sprint inteiro. §6 do sprint contract **NÃO** inlina WIs; apenas indexa e referencia os arquivos individuais de WI.

### 34.2 ADR / PDR / ODR

Ver `_templates/adr.md` — template com **16 seções totais (§0–§15)**, aplicável aos 3 tipos (ADR / PDR / ODR) com adaptações mínimas.

### 34.3 Work Item (template v2.0)

Ver `_templates/work_item.md` — template canônico com **33 seções totais (§0–§32)** governando cada WI individualmente, incluindo evidence-driven gates (PRINC-031), role + automation tagging (PRINC-032), PRR hook (PRINC-033) e escalation protocol (PRINC-034). Estrutura hierárquica:

```
Sprint (S-XX)
  └── Work Item (WI-SXX-NNN)               ← template work_item.md
        ├── Sub-task (ST-001)              ← template subtask.md
        ├── Sub-task (ST-002)
        └── Sub-task (ST-003)
```

Seções principais (33 totais, §0–§32):

| Seção | Propósito |
|---|---|
| 0. Identificação | metadata, parent, assignee, reviewers, PR, branch, tier/região |
| 1. Intent | uma frase declarativa testável |
| 2. **Narrative** | prosa 300–500 palavras (inspiração Amazon 6-pager) |
| 3. **Customer Impact & Journey** | personas, touchpoints, métricas customer-visible, comms |
| 4. Capability Mapping / Trace | CAP/INV/NFR/ADR rastreados com evidence |
| 5. Tipo e Classificação | tipo + prioridade + blast radius + reversibilidade + experiment flag + compliance triggers |
| 6–7. Escopo / Anti-scope | detalhado + componentes C4 + arquivos |
| 8. Acceptance Criteria | Gherkin + AC Coverage Matrix (happy/error/boundary/property) |
| 9. Design Decisions | locais + ADR triggers + trade-offs |
| 10. **Completeness Criteria (evidence-driven)** | **11 subáreas binárias com evidence/role/automation** |
| 11. Definition of Done | snapshot + gates terminais |
| 12. Invariants | safety + liveness + produto + novos |
| 13. Artifacts Produced | código, testes, docs, infra, observability, ADRs, runbooks, KB |
| 14. **Quality Standards (SOTA)** | test, code, perf, security, observability, docs, reproducibility, a11y/i18n, sustainability |
| 15. **Chaos Experiments** | FMs endereçados + experimentos planejados + cadência |
| 16. **Production Readiness Review (PRR)** | hook pro PRR doc separado |
| 17. Sub-tasks | tabela-sumário + progresso agregado + DAG |
| 18. Dependencies | upstream/downstream/cross-sprint/external |
| 19. Effort Estimate | XS–XL + rationale + PERT + histórico |
| 20. **Time-boxing & Escalation Protocol** | burndown + triggers + decisões + log |
| 21. Observability Plan | métricas/logs/traces/dashboards/alertas/runbooks |
| 22. **Cost Analysis** | infra delta + ops delta + dev delta + TCO 12m |
| 23. **API / Contract Impact** | breaking classification + versioning + migration + SDK + notification |
| 24. **Post-mortem Hooks** | incidents relacionados + lições + preventive measures |
| 25. Rollback / Recovery | strategy + procedure + migration reversibility + feature flags |
| 26. Security & Privacy | STRIDE + LINDDUN + controles + data classification |
| 27. **Knowledge Transfer & Handoff** | audience + assets + onboarding test + ownership |
| 28. Risk Register | P×I + mitigação + contingência |
| 29. Review Checkpoints | design / mid / code / pre-merge / PRR |
| 30. Sign-off | 12 papéis (ENG, SEC, PRIV, SRE, QA, PROD, ARCH, FIN, LEGAL, sprint owner, approver) |
| 31. Change Log | versionado |
| 32. Apêndice: Anti-patterns | APs evitados neste WI (cross-ref ao catálogo §39) |

### 34.4 Sub-task (template v2.0)

Ver `_templates/subtask.md` — template canônico com **19 seções totais (§0–§18)** para unidades atômicas de trabalho, com mesmo rigor evidence-driven + role/automation tagging do WI, proporcional ao escopo atômico.

Princípio de decomposição:

> **REG-WI-002**: Algo é sub-task (não *step*) IFF pode ser trabalhada em paralelo com outra sub-task **e** tem *acceptance* verificável independentemente. Sequencial-indivisível = *step* dentro de sub-task, não sub-task separada.

Seções da sub-task (19 totais, §0–§18):

| Seção | Propósito |
|---|---|
| 0. Identificação | ST-ID, WI pai, sprint, assignee, reviewers, branch, PR |
| 1. Intent | uma frase testável |
| 2. Trace | WI/CAP/INV/NFR/ADR referenciados com evidence |
| 3. Acceptance Criteria (Gherkin) + coverage matrix | obrigatório |
| 4. Escopo / Anti-scope | explícitos + componentes tocados |
| 5. **Design Notes** | abordagem + decisões locais + trigger pra ADR |
| 6. Artifacts Produced | código, testes, docs, observability |
| 7. **Completeness Criteria (evidence-driven)** | **7 subáreas: código, testes, docs, observability, security, performance, processo — com evidence/role/automation** |
| 8. Definition of Done | snapshot §7 + PR mergeado + gates terminais |
| 9. Invariants | preserva + operacionais + novos propostos |
| 10. **Quality Standards (SOTA)** | test, code, performance, security, observability, reproducibility |
| 11. Dependencies | upstream/downstream |
| 12. **Effort Estimate & Time-boxing** | XS–XL + burndown + triggers de escalação |
| 13. Observability Contributed | métricas/logs/traces |
| 14. **Security Notes** | fronteira? STRIDE mini se aplicável |
| 15. Rollback / Recovery | strategy + procedure |
| 16. Risk Register | P×I |
| 17. Sign-off | assignee, code reviewer, security/SRE/QA se aplicável, WI owner |
| 18. Change Log | versionado |

### 34.4.1 Production Readiness Review (PRR)

Ver `_templates/production_readiness_review.md` — template canônico para o gate **"pronto pra produção"** (PRINC-033), separado do DoD de WI.

Cobre **23 seções totais (§0–§22)**: SLO/SLI/Error Budget, Capacity Planning, Load Testing, Chaos Engineering, Disaster Recovery, Observability, Runbooks, Security, Privacy, Supply Chain, Rollback, Deployment Strategy, Customer Support, Communication Plan, Cost/Finance, Legal/Contract.

Status possíveis: `NOT_STARTED | IN_REVIEW | CONDITIONALLY_APPROVED | APPROVED | REJECTED`.

### 34.5 Regras invioláveis de decomposição

| Regra | Enunciado |
|---|---|
| **REG-DECOMP-001** | Todo WI **DEVE** usar template `work_item.md` v2.0 (33 seções totais §0–§32 preenchidas) |
| **REG-DECOMP-002** | Toda ST **DEVE** usar template `subtask.md` v2.0 (19 seções totais §0–§18 preenchidas) |
| **REG-DECOMP-002b** | Toda feature com impacto em produção **DEVE** passar por PRR (`production_readiness_review.md`) antes de GA |
| **REG-DECOMP-002c** | Toda caixa binária em WI/ST/PRR **DEVE** ter evidence artifact (PRINC-031) |
| **REG-DECOMP-002d** | Toda caixa em WI/ST/PRR **DEVE** ter role + automation tag (PRINC-032) |
| **REG-DECOMP-003** | WI de tamanho XL **DEVE** ser decomposto em sub-tasks ou sub-WIs |
| **REG-DECOMP-004** | Nenhum WI transiciona para `DONE` enquanto qualquer sub-task não for `DONE` |
| **REG-DECOMP-005** | Todo WI e toda ST **DEVEM** ter Completeness Criteria, DoD, Invariants, Quality Standards — sem exceção |
| **REG-DECOMP-006** | Se durante execução de ST descobre-se necessidade de decomposição adicional, **DEVE** parar, decompor, atualizar ST pai, só então continuar |
| **REG-DECOMP-007** | Renomeação / movimentação de WI/ST **DEVE** preservar ID (PRINC-006) |

### 34.6 Capability (esqueleto inline — será movido para `_templates/capability.md` no Nível 2)

```markdown
## CAP-XXX: {Título}

- **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
- **audit_status:** ACTIVE | AUDIT_PENDING | AUDITED
- **Introduzida em:** versão X
- **Depende de:** CAP-YYY
- **Personas:** PERSONA-XX
- **JTBDs atendidos:** JTBD-XXX
- **NFRs aplicáveis:** NFR-XXX
- **Invariantes relacionados:** INV-XXX
- **Cross-cutting concerns aplicáveis:** CC-SEC, CC-PRIV, CC-SRE (ver §16)

### Intent
{uma frase}

### Descrição
{parágrafo}

### Entradas
{tipo estruturado}

### Saídas
{tipo estruturado}

### Precondições / Poscondições
{lista}

### Acceptance Criteria (Gherkin)
Given {contexto}
When {ação}
Then {resultado}

### Método de verificação
- Teste principal: {arquivo}
- Property test: {arquivo}
- Load test: {arquivo}

### Observabilidade
- Métricas: {lista}
- Logs: {lista}
- Traces: {lista}
- Alertas: {lista}
- Runbooks: {lista}

### Lifecycle
{PROPOSED | ALPHA | BETA | GA | DEPRECATED | REMOVED} — ver §28

### Cross-cutting concerns
- CC-SEC: {impacto + controles}
- CC-PRIV: {impacto + controles} ou "N/A"
- CC-SRE: {SLI/SLO linkado}
```

### 34.8 Invariant (esqueleto inline — será movido para `_templates/invariant.md` no Nível 2)

```markdown
## INV-XXX: {Nome}

- **Tipo:** Safety | Liveness
- **Severidade:** CRITICAL | HIGH | MEDIUM | LOW
- **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
- **audit_status:** ACTIVE | AUDIT_PENDING | AUDITED

### Enunciado formal
{∀ x . P(x) → Q(x) ou TLA+ referenciado se CRITICAL}

### Enunciado em prosa
{1-2 sentenças precisas}

### Escopo
{onde deve valer: componentes, fluxos, janelas temporais}

### Método de verificação
- TLA+: {arquivo.tla} (se CRITICAL)
- Property test: {arquivo}
- Integration test: {arquivo}
- Runtime assertion: {local, se aplicável}

### Consequência de violação
- Severidade: CRITICAL | HIGH | MEDIUM | LOW
- Blast radius: {escopo}
- Ação automática se detectado: {ex: feature freeze + page on-call}
- Mitigação manual: {procedimento}
```

### 34.9 Runbook (esqueleto inline — será movido para `_templates/runbook.md` no Nível 5)

```markdown
# Runbook RB-XXX: {Nome}

- **Versão:** X.Y.Z
- **Última atualização:** YYYY-MM-DD
- **Owner:** Nome
- **Alert(s) disparador(es):** Alert-XXX
- **Severity esperada:** SEV-1 | 2 | 3 | 4

## Sintomas
{o que on-call vê}

## Impacto
{impacto no cliente}

## Primeira resposta (Triage ≤ 5 min)
1. {passo}
2. {passo}

## Diagnóstico
{queries, dashboards, comandos}

## Mitigação
{passos reversíveis}

## Escalation
Quando escalar: {condições}
Para quem: {papel}

## Pós-incident
- [ ] Métricas confirmadas em baseline
- [ ] Post-mortem agendado (se SEV-1/2)
- [ ] Lições entram em backlog

## Links
- Dashboard: {url}
- Runbooks relacionados: {lista}
```

---

## 35. Diagramas

### 35.1 Notação obrigatória

| Tipo | Notação |
|---|---|
| Arquitetura C4 | Simon Brown C4 Model em Mermaid ou Structurizr |
| Sequence | Mermaid `sequenceDiagram` |
| State machine | Mermaid `stateDiagram-v2` |
| ER (data model) | Mermaid `erDiagram` |
| Grafo de dependências | Mermaid `graph` |
| Threat model | DFD (Data Flow Diagram) com *trust boundaries* |
| Attack tree | Mermaid `graph` top-down |

### 35.2 Regras

- **REG-DIAG-001**: Diagramas **DEVEM** ser texto-fonte (Mermaid, PlantUML, DOT), não imagens binárias.
- **REG-DIAG-002**: Diagrama **DEVE** ter descrição textual equivalente para acessibilidade.
- **REG-DIAG-003**: Diagrama **NÃO DEVE** introduzir informação ausente em prosa estruturada.
- **REG-DIAG-004**: Diagrama **DEVE** ter título e legenda.

---

## 35.5 Control Inheritance — Canonical Source per Domain

> **Problema:** Observability, Security, Rollback, Cost, Compliance e outros controles cross-cutting aparecem repetidos em Sprint, WI, ST e PRR. Cada autor descreve diferente. Drift garantido. Manutenção dobrada.
>
> **Solução:** cada domínio tem **uma fonte canônica** em Nível 3 (arquitetura). Artefatos downstream **herdam** via referência, não duplicam. Local deltas são permitidos mas **explícitos e enumerados**.

### 35.5.1 Catálogo de fontes canônicas

| Domínio (cross-cutting concern) | Fonte canônica | Status planejado |
|---|---|---|
| Observability Model | `specs/03_architecture/observability_model.md` | Nível 3 |
| Security Model | `specs/03_architecture/security_model.md` | Nível 3 |
| Privacy Model | `specs/03_architecture/privacy_model.md` | Nível 3 |
| Threat Model (STRIDE+LINDDUN) | incluído em `security_model.md` + `privacy_model.md` | Nível 3 |
| SLO Catalog | `specs/03_architecture/slo_catalog.md` | Nível 3 |
| Failure Modes (FMEA) | `specs/03_architecture/failure_modes.md` | Nível 3 |
| Resilience Patterns | `specs/03_architecture/resilience_patterns.md` | Nível 3 |
| Data Model | `specs/03_architecture/data_model.md` | Nível 3 |
| Storage Semantics Matrix | `specs/03_architecture/storage_semantics_matrix.md` | Nível 3 (Lote 4) |
| Auth Model | `specs/03_architecture/auth_model.md` | Nível 3 (Lote 4) |
| Compliance Matrix | `specs/03_architecture/compliance_matrix.md` | Nível 3 |
| Remote Cache Product Profile (CAS/AC/GC/dedup/protocol) | `specs/03_architecture/remote_cache_product_profile.md` | Nível 3 (promovido v0.5.0 via ADR-0013) |
| Invariant Registry (INV-XXX catálogo canônico) | `specs/03_architecture/invariant_registry.md` | Nível 3 (criado v0.5.0) |
| BYOK / Key Management | `specs/03_architecture/key_management.md` | Nível 3 (criado v0.5.0) |
| Incident Response Process | `specs/05_quality/incident_response.md` | Nível 5 |
| Runbook Library | `specs/05_quality/runbooks/` | Nível 5 |
| Test Strategy | `specs/05_quality/test_strategy.md` | Nível 5 |
| Quality Gates | `specs/05_quality/quality_gates.md` | Nível 5 |

### 35.5.2 Semântica de herança

- **REG-INHERIT-001**: Artefatos de Nível 4 (Sprint, WI, ST, PRR) **DEVEM** declarar em YAML front matter a lista `inherits_from: [<ID1>, <ID2>, ...]` listando fontes canônicas aplicáveis.
- **REG-INHERIT-002**: Ao herdar de uma fonte canônica, o artefato assume **todos** os controles, métricas, SLOs, alertas e runbooks definidos nela, sem precisar reenumerar.
- **REG-INHERIT-003**: Violação de controle herdado é violação de contrato — tratada como incidente de severidade proporcional à criticalidade da fonte.
- **REG-INHERIT-004**: Atualização em fonte canônica propaga imediatamente para todos os downstream que a herdam (sem necessidade de *thaw* individual), **desde que a atualização seja `minor` ou `patch`** (§41.2). Bump `major` da fonte canônica **DEVE** thaw todos os downstream com `audit_status: AUDIT_PENDING` até auditoria.

### 35.5.3 Local deltas (overrides / extensões)

Quando um artefato downstream precisa **diferir** do controle herdado (override) ou **adicionar** algo não coberto (extensão), declara em:

- Campo YAML `local_deltas: [<descrição curta>, ...]`
- Seção textual dedicada no corpo do artefato (ex: WI §21.7 "Local Observability Deltas")

Regras:

- **REG-INHERIT-010**: Todo `local_delta` **DEVE** ter `rationale` textual explicando por que o controle canônico é insuficiente ou inaplicável.
- **REG-INHERIT-011**: Override (weaker) requer aprovação do owner da fonte canônica + compensating control. Trata-se como waiver (ver §35.6 quando existir — Lote 2.4).
- **REG-INHERIT-012**: Extensão (stronger ou paralelo) é permitida unilateralmente, mas registrada.
- **REG-INHERIT-013**: Se um `local_delta` aparece repetidamente em múltiplos artefatos downstream do mesmo domínio, é sinal de que a fonte canônica está incompleta → propor update.

### 35.5.4 Validação CI

Script `scripts/validate_specs.py` **DEVE** (em Lote 2.4+) validar:

- [ ] Todo `inherits_from` aponta para ID existente com `doc_status: FROZEN`.
- [ ] Fonte canônica listada em §35.5.1 existe (pelo menos como esqueleto DRAFT).
- [ ] `local_deltas` não contém lista vazia nem strings vazias.
- [ ] Para cada `local_delta`, há seção correspondente no corpo do artefato com rationale.

### 35.5.5 Impacto nos templates

Templates são **indicações** de estrutura. Seções duplicadas entre Sprint/WI/ST/PRR que agora são **herdáveis** devem:

- Marcar topo da seção com: `> **Herdado de:** <ID fonte canônica>. Ver §35.5 do framework.`
- Listar apenas `local_deltas` específicos deste artefato.
- Não duplicar tabela completa da fonte canônica.

Exemplos concretos (a serem aplicados em Lote 3 — over-engineering reduction):

- WI §21 Observability Plan → herda de `observability_model.md`, declara apenas novas métricas/logs/traces/dashboards criados **por este WI**.
- WI §25 Rollback / Recovery → herda de `failure_modes.md` + `resilience_patterns.md`, declara apenas local rollback procedure específico.
- WI §26 Security & Privacy → herda de `security_model.md` + `privacy_model.md`, declara apenas new trust boundaries ou data flows.
- Sprint §14 Observability Plan → herda de `observability_model.md`, sumariza mudanças agregadas do sprint.
- PRR §4 SLO / SLI / Error Budget → herda de `slo_catalog.md`, confirma apenas SLOs relevantes.

### 35.5.6 Pre-condição: fontes canônicas precisam existir

Nível 4 (sprints, WIs, STs, PRRs) só escala herança quando Nível 3 existe. Enquanto Nível 3 não é escrito (atualmente zero docs em `03_architecture/`), seções dos templates **continuam preenchidas inline** com nota:

```
> **Herdado de:** `specs/03_architecture/observability_model.md` (PENDENTE — escrever no Lote 4).
> Enquanto fonte canônica não existe, preencher inline.
```

Após fonte canônica criada e `doc_status: FROZEN`, downstream migra para herança real.

### 35.5.7 Precedência entre herança e waiver

Quando um artefato `inherits_from` uma fonte canônica que declara controle C1, e ao mesmo tempo existe um waiver `W1` cobrindo C1 pra este artefato:

- **REG-INHERIT-020**: Waiver tem **precedência explícita** sobre herança pro controle dispensado. Ou seja, C1 fica suspenso conforme `W1` enquanto `W1` estiver `doc_status: FROZEN` + `expires_at` futuro.
- **REG-INHERIT-021**: Quando `W1` expira ou é revogado, C1 volta imediatamente a valer via herança — sem necessidade de *thaw* do artefato downstream (a herança é viva).
- **REG-INHERIT-022**: Ao criar um waiver cobrindo controle herdado, é **obrigatório** listar o ID da fonte canônica no `rationale` do waiver, demonstrando que o controle é herdado (não inventado localmente).
- **REG-INHERIT-023**: Local delta sobre um controle herdado **não** é equivalente a waiver. Delta é divergência documentada; waiver é dispensa temporária. Ambos podem coexistir.

---

## 35.6 Waivers — Exceções Formais com Expiração Obrigatória

> **Problema:** em execução real, haverá casos em que atender a 100% dos gates do framework é **tecnicamente impossível, financeiramente inviável, temporalmente inviável** ou **incompatível com um constraint contratual/regulatório externo**. Sem mecanismo formal, o time cria "N/A oportunistas" ou marca ✅ de fé — o processo se degrada.
>
> **Solução:** Waiver — exceção **formal, temporária, com compensating control, prazo duro e plano de resolução**.

### 35.6.1 Princípios invioláveis do waiver

- **REG-WAIVER-001**: Todo waiver **DEVE** ter `expires_at` (data ISO) — após ela, waiver invalida automaticamente e CI bloqueia merges/deploys dependentes.
- **REG-WAIVER-002**: Todo waiver **DEVE** ter `compensating_control` — proteção alternativa operacional durante a vigência. "Aceitar risco sem proteção" não é waiver; é ADR de risk acceptance (caminho diferente).
- **REG-WAIVER-003**: Todo waiver **DEVE** ter `rationale` ≥ 10 caracteres — sem justificativa escrita, waiver é inválido.
- **REG-WAIVER-004**: Todo waiver **DEVE** ter `revalidation_trigger` — evento/condição que força re-review antes de `expires_at`.
- **REG-WAIVER-005**: Todo waiver **DEVE** ter `gates_waived` — lista explícita de gates do framework sendo dispensados (IDs tipo `REG-XXX-NNN`), não descrições em prosa.
- **REG-WAIVER-006**: Duração máxima recomendada: **6 meses**. Waivers ≥ 6 meses exigem escalação + justificativa adicional + sign-off do Aprovador Final.
- **REG-WAIVER-007**: Waiver **NÃO PODE** ser renovado indefinidamente. Após 2 renovações consecutivas sem resolução da causa raiz, **escalação obrigatória** ao Aprovador Final pra decisão binária: (a) consertar a causa raiz, ou (b) aceitar formalmente como constraint permanente via ADR (mata o waiver, abre trade-off permanente).

### 35.6.2 Campos obrigatórios no YAML front matter

Schema JSON valida automaticamente:

| Campo | Tipo | Obrigatório |
|---|---|---|
| `type` | literal "waiver" | ✅ |
| `gates_waived` | lista de strings (IDs de gates) | ✅ (mínimo 1) |
| `rationale` | string ≥ 10 chars | ✅ |
| `compensating_control` | string | ✅ |
| `expires_at` | ISO date | ✅ |
| `revalidation_trigger` | string | ✅ |

Proibidos em `type: waiver`: `work_status`, `parent`, `assignee`, `feature_wi`, `capabilities`, `prod_target_date`.

### 35.6.3 Localização e numeração

- **REG-WAIVER-010**: Waivers vivem em `specs/_waivers/WAIVER-<YYYYMMDD>-<NNN>-<slug>.md`.
- **REG-WAIVER-011**: ID formato `WAIVER-YYYYMMDD-NNN` — data de criação + sequencial do dia.
- **REG-WAIVER-012**: Diretório `specs/_waivers/` é escopo canônico; CI `validate_specs.py` valida contra schema.

### 35.6.4 Relação com PRR `CONDITIONALLY_APPROVED`

PRR em `CONDITIONALLY_APPROVED` (§16 do PRR template) também usa caveats com expiração. Relação:

- **Caveat de PRR**: exceção **específica a UM PRR**, vigente até resolução. Não é waiver formal — é registro no próprio PRR.
- **Waiver formal**: aplicável a **múltiplos artefatos ou global**. Documento separado em `specs/_waivers/`.

**Regra:** quando um caveat de PRR se repete em ≥ 2 PRRs diferentes (mesmo controle dispensado), **DEVE** ser promovido para waiver formal. Caveat individual pontual permanece no PRR.

### 35.6.5 Template canônico

Ver `_templates/waiver.md` — 11 seções obrigatórias:

| Seção | Conteúdo |
|---|---|
| §0 Identificação | metadata, gates dispensados, escopo, expiração |
| §1 Executive Summary | 3–5 sentenças acessíveis |
| §2 Gate(s) dispensado(s) | IDs exatos + referência do framework |
| §3 Rationale | prosa detalhada + alternativas consideradas + evidência |
| §4 Compensating Control | descrição + diferenças vs controle canônico + owner + testes |
| §5 Impact Assessment | risco residual + usuários afetados + compliance impact |
| §6 Escopo | temporal, por artefato, geográfico, por tier |
| §7 Conditions for Revocation | auto-triggers (CI) + manual triggers + process |
| §8 Review Checkpoints | T+25%, T+50%, T+75%, T+90% (go/no-go renovação) |
| §9 Plano de Resolução | causa raiz + ações + critério de sucesso + fallback |
| §10 Sign-off | Owner + Gate Owner + Security/Privacy/Compliance/Legal conforme + Aprovador Final |
| §11 Change Log | versionado |

### 35.6.6 Enforcement automático de expiração

Script `scripts/check_waivers.py` (a ser criado em Lote 2.5 ou junto do CI pipeline):

1. Lê todos os waivers em `specs/_waivers/` com `doc_status: FROZEN`.
2. Calcula `days_until_expiry = expires_at - today`.
3. Emite warnings em CI:
   - `days_until_expiry ≤ 30`: ⚠️ warn (planejar resolução)
   - `days_until_expiry ≤ 7`: 🟡 alert (escalação próxima)
   - `days_until_expiry ≤ 0`: 🔴 **bloqueio de merge/deploy** pra PRs que dependem do waiver (identificado via `inherits_from` ou referência textual).
4. Incidente automático SEV-3 aberto se waiver expirar em produção ativa.

### 35.6.7 Auditoria de waivers

- **DEVE** Aprovador Final revisar trimestralmente inventário de waivers ativos.
- **Red flags:**
  - > 10 waivers ativos simultaneamente em produção: sinal de framework mal calibrado.
  - Waivers com renovações ≥ 2 consecutivas: plano de resolução falhou, escalação obrigatória.
  - Mesmo gate dispensado em ≥ 3 waivers diferentes: framework impossível de cumprir, revisar o gate.

### 35.6.8 Anti-padrões de waiver

- ❌ **AP-WAIVER-001**: Waiver sem compensating control ("aceitamos o risco por 6 meses"). → ADR de risk acceptance, não waiver.
- ❌ **AP-WAIVER-002**: Waiver sem `expires_at` ou com expiração "indefinida". → Bloqueado pelo schema.
- ❌ **AP-WAIVER-003**: Waiver renovado em silêncio via bump de versão sem re-approval. → Renovação **DEVE** passar por sign-off de novo.
- ❌ **AP-WAIVER-004**: Waiver cobrindo dezenas de gates ("this WI waives all security gates"). → Waivers granulares; agrupamento excessivo é abuso.
- ❌ **AP-WAIVER-005**: Waiver como "N/A oportunista" em checklist de WI/ST pra evitar trabalho. → Se é N/A legítimo, marca N/A com rationale no próprio artefato; waiver é para dispensa de gate que **aplicava**.

---

## 35.7 Evidence Taxonomy — Tipos Formais de Prova para Gates

> **Problema:** PRINC-031 exige evidence artifact pra toda caixa binária. Mas "evidence" sem taxonomia vira loteria: um reviewer aceita URL de dashboard, outro exige snapshot imutável, outro aceita screenshot sem timestamp. Sem regra, o rigor é performativo.
>
> **Solução:** taxonomia formal de **46 tipos de evidence** (24 originais + 22 expandidos no Lote 5.1 pra cobrir compliance, supply chain e privacy), cada um com regras específicas de formato, armazenamento, imutabilidade, retenção e acesso. Aliases humanos legíveis em §35.7.1.1.

### 35.7.1 Taxonomia canônica

Cada tipo tem ID `EVT-XXX`. Ao preencher coluna `Evidence` em WI/ST/PRR, reviewer **DEVE** identificar qual tipo foi fornecido (por prefixo, link tipado, ou referência estruturada).

| ID | Tipo | Conteúdo | Formato aceito | Armazenamento | Imutabilidade | Retenção mínima |
|---|---|---|---|---|---|---|
| **EVT-001** | **CI_LOG** | Log de execução de pipeline CI | URL GitHub Actions / GitLab CI run | GitHub Actions / GitLab | ✅ (via commit SHA + run ID) | 90 dias (política default CI); 1 ano pra runs de release |
| **EVT-002** | **TEST_OUTPUT** | Output de test runner (cargo test, pytest, jest) | Arquivo `.txt`/`.xml`/`.json` linkado a CI run | Artifacts do CI run (EVT-001) | ✅ | Herda de EVT-001 |
| **EVT-003** | **COVERAGE_REPORT** | Relatório de cobertura de testes | HTML/XML/JSON (llvm-cov, lcov, coverage.py) | Artifacts CI + badges repo | ✅ | 1 ano |
| **EVT-004** | **BENCH_REPORT** | Relatório de benchmark | JSON output de `criterion` + comparativo p50/p99 vs baseline | Artifacts CI + `benches/history/` | ✅ | 1 ano |
| **EVT-005** | **SAST_SCAN_REPORT** | Static Application Security Testing | SARIF JSON (CodeQL, semgrep, bandit) | GitHub Security tab + artifacts | ✅ | 1 ano |
| **EVT-006** | **DAST_SCAN_REPORT** | Dynamic AST (endpoints HTTP) | OWASP ZAP report (HTML/JSON) | Artifacts CI | ✅ | 1 ano |
| **EVT-007** | **DEPENDENCY_SCAN_REPORT** | Vuln scan de dependências | `cargo audit` output / GitHub Dependabot / Snyk JSON | Artifacts CI + GitHub Security | ✅ | 1 ano |
| **EVT-008** | **FUZZ_REPORT** | Output de fuzzing | cargo-fuzz output + corpus + stats | Artifacts CI | ✅ | 6 meses |
| **EVT-009** | **MUTATION_REPORT** | Mutation testing kill rate | cargo-mutants output JSON | Artifacts CI | ✅ | 6 meses |
| **EVT-010** | **SBOM** | Software Bill of Materials | **CycloneDX 1.5+ JSON** (preferido no ecossistema Rust via `cargo-cyclonedx`) ou SPDX 2.3+ JSON/YAML; ambos aceitos (ADR-0014) | Artifacts de release + registry | ✅ (assinado com cosign) | 7 anos (supply chain) |
| **EVT-011** | **BINARY_SIGNATURE** | Assinatura de binário/container | cosign verification output + attestation | Registry (R2, GHCR) + Sigstore public log | ✅ (crypto) | Permanente |
| **EVT-012** | **SCREENSHOT** | Captura de tela | PNG com metadata EXIF (timestamp, device) | R2 bucket `evidence-screenshots/` com naming `YYYYMMDD-<hash>` | ✅ (hash-addressed) | 1 ano |
| **EVT-013** | **DASHBOARD_SNAPSHOT** | Screenshot de dashboard Grafana/etc com timestamp | PNG + URL + timestamp ISO + query snapshot | R2 + Grafana snapshot URL (imutável) | ✅ (via snapshot API do Grafana) | 1 ano |
| **EVT-014** | **DASHBOARD_URL** | Link para dashboard ao vivo (NÃO-imutável) | URL | URL externa | ❌ **dados mudam** | N/A |
| **EVT-015** | **PR_APPROVAL** | Aprovação formal em PR | URL de PR review com "APPROVED" + timestamp | GitHub/GitLab | ✅ | Permanente (git history) |
| **EVT-016** | **HUMAN_SIGNOFF** | Aprovação humana nomeada | Nome + papel + data + rationale + assinatura (se papel legal/compliance) | Corpo do artefato §30 (sign-off section) | ✅ via git commit | Permanente |
| **EVT-017** | **RUNBOOK_EXECUTION** | Log de dry-run de runbook | Gravação de sessão (asciinema, vídeo) + timestamps + output | R2 `evidence-runbooks/` | ✅ | 1 ano |
| **EVT-018** | **MIGRATION_APPLIED** | Comprovação de migration aplicada | Migration ID + apply timestamp + schema diff antes/depois | DB migration table + commit git | ✅ | Permanente (db history) |
| **EVT-019** | **INCIDENT_LINK** | Link pra incident post-mortem | Spec doc `specs/incidents/INC-YYYYMMDD-NNN.md` | Repositório git | ✅ | Permanente |
| **EVT-020** | **EXTERNAL_VENDOR_CONFIRMATION** | Comunicação formal com vendor | Email (PDF com headers completos) / support ticket screenshot | R2 `evidence-vendor/` | ✅ (assinado com timestamp) | 7 anos |
| **EVT-021** | **AUDIT_REPORT** | Relatório de auditoria externa | PDF assinado do auditor | R2 `evidence-audits/` | ✅ (crypto) | 7 anos |
| **EVT-022** | **TLA_MODEL_CHECK** | Output de TLC model checker | TLC output JSON + seed + spec | Repositório git + artifacts CI | ✅ | Permanente |
| **EVT-023** | **CHAOS_EXPERIMENT_REPORT** | Resultado de chaos experiment | Report chaos-mesh/custom + métricas observadas | R2 + artifacts | ✅ | 1 ano |
| **EVT-024** | **LOAD_TEST_REPORT** | Output de load test | wrk/oha/k6 output JSON + dashboard snapshot | Artifacts CI | ✅ | 1 ano |
| **EVT-025** | **PENTEST_REPORT** | Relatório de pentest externo ou interno (red team) | PDF assinado do vendor/time + findings catalogados + retest evidence | R2 `evidence-pentest/` | ✅ (crypto + vendor sig) | 7 anos |
| **EVT-026** | **SCHEMA_VALIDATION** | Output de validador de schema (JSON Schema / Protobuf / SQL DDL) | Log do validator + schema file hash + dataset | Artifacts CI + repo | ✅ | 1 ano |
| **EVT-027** | **CLIENT_CONFORMANCE_TEST** | Output de test suite que verifica cliente externo (ex: CLI do cliente validando hash BLAKE3 server-side) | JSON/XML do runner + version do client testado | Artifacts CI | ✅ | 1 ano |
| **EVT-028** | **CONFIG_SNAPSHOT** | Estado de configuração (DO singleton, flags, IAM policy) em momento específico | YAML/JSON + timestamp + commit hash + environment | R2 `evidence-config/` | ✅ (hash + timestamp) | 1 ano |
| **EVT-029** | **ADR_DECISION** | Referência a ADR aprovado com decisão específica | ID ADR-XXXX + URL do doc em `doc_status: FROZEN` | Repositório git | ✅ (git history) | Permanente |
| **EVT-030** | **BUG_BOUNTY_REPORT** | Relatório de submissão HackerOne/Bugcrowd | PDF do ticket + triagem + fix evidence | R2 `evidence-bounty/` + plataforma | ✅ | 7 anos |
| **EVT-031** | **MONITORING_REPORT** | Relatório consolidado de uptime/synthetic canary | JSON do provider (StatusGator, Better Uptime, etc) + intervalo | R2 + provider | ✅ (provider-signed) | 1 ano |
| **EVT-032** | **POLICY_SIGN** | Política corporativa assinada (Code of Conduct, Security Policy) | PDF assinado digitalmente + lista de assinantes + data | R2 `evidence-policies/` | ✅ (crypto sig) | 7 anos (compliance) |
| **EVT-033** | **TRAINING_RECORD** | Comprovante de treinamento (security awareness, privacy) | LMS export + pass grade + certificate ID | R2 `evidence-training/` | ✅ | 3 anos |
| **EVT-034** | **AUDIT_PLAN** | Plano de auditoria interna/externa (SOC 2, ISO 27001) | Doc assinado + escopo + cronograma + auditor | R2 `evidence-audits/` | ✅ | 7 anos |
| **EVT-035** | **ACCESS_REVIEW** | Review trimestral de acessos/PATs/roles | CSV/JSON export + assinatura review owner + ações tomadas | R2 + ticket tracker | ✅ | 3 anos |
| **EVT-036** | **NETWORK_DIAGRAM** | Diagrama formal de rede/topologia com versionamento | SVG/PNG + DSL (e.g. D2, Mermaid) committed + version ID | Repositório git | ✅ (git history) | Permanente |
| **EVT-037** | **TLS_SCAN_REPORT** | Output de SSL Labs / testssl.sh | JSON/HTML report + target + timestamp + grade | Artifacts CI | ✅ | 1 ano |
| **EVT-038** | **DEPLOY_LOG** | Log de deploy em produção (CF deploy, progressive rollout) | CF API response + version hash + timestamp + rollout ID | R2 `evidence-deploys/` | ✅ | 2 anos |
| **EVT-039** | **WAIVER_ACTIVE** | Referência a waiver ativo cobrindo o gate | ID `WAIVER-YYYYMMDD-NNN` com `doc_status ≠ DEPRECATED` e `expires_at > now` | Repositório git + `_waivers/` | ✅ | Permanente (git) |
| **EVT-040** | **VENDOR_REVIEW** | Due diligence de sub-processador (Cloudflare, Neon, Stripe) | PDF de SOC 2 report + DPA + scope letter | R2 `evidence-vendors/` | ✅ (vendor-signed) | 7 anos |
| **EVT-041** | **DR_DRILL** | Disaster Recovery drill execution | Runbook completion log + RTO/RPO measured + lessons learned | R2 `evidence-dr/` | ✅ | 3 anos |
| **EVT-042** | **ERASURE_TEST** | Teste E2E de DSR erasure (fake user lifecycle) | Test log + verification que 0 records remain + timestamp | Artifacts CI + R2 | ✅ | 3 anos |
| **EVT-043** | **DATA_CLASSIFICATION_DOC** | Documento canônico de classificação de dado (LGPD/GDPR categories) | Doc versionado em `doc_status: FROZEN` + tabela dado × classificação | Repositório git | ✅ (git history) | Permanente |
| **EVT-044** | **LEGAL_REVIEW** | Review formal do Legal sobre policy/contract/base legal | Email assinado ou ticket tracker + decisão + rationale | R2 `evidence-legal/` | ✅ | 7 anos |
| **EVT-045** | **DPIA** | Data Protection Impact Assessment | Doc versionado seguindo GDPR Art. 35 + sign-off DPO | Repositório git + R2 | ✅ | 7 anos |
| **EVT-046** | **LIA** | Legitimate Interest Assessment | Doc LIA template preenchido + balanceamento explícito + review anual | Repositório git | ✅ | 3 anos |

### 35.7.1.1 Aliases canônicos (legibilidade humana)

Para docs que preferem nomes mnemônicos a IDs numéricos, a seguinte tabela de aliases é **aceita** mas **DEVE** vir seguida do ID canônico entre parênteses na primeira menção do artefato:

| Alias humano | EVT canônico |
|---|---|
| UNIT_TEST_PASS | EVT-002 |
| INTEGRATION_TEST_PASS | EVT-002 |
| E2E_TEST_PASS | EVT-002 |
| FUZZ_OUTPUT | EVT-008 |
| SAST_SCAN | EVT-005 |
| DAST_SCAN | EVT-006 |
| SLSA_PROVENANCE | EVT-011 |
| RUNBOOK_VALIDATION | EVT-017 |
| CHAOS_REPORT | EVT-023 |
| LOAD_TEST | EVT-024 |
| SCAN_REPORT | EVT-037 (TLS) ou EVT-005 (SAST) ou EVT-006 (DAST) — disambiguar no contexto |
| AUDIT_LOG | EVT-001 (se CI) ou EVT-019 (se post-mortem) ou EVT-028 (se config) |

Qualquer token EVT-<NOME_LIVRE> que não conste nem em §35.7.1 nem em §35.7.1.1 falha validação no `scripts/validate_evidence.py` (a implementar).

### 35.7.2 Regras de aceitação

- **REG-EVID-001**: Toda caixa binária que **PASSA** (✅) em Completeness/DoD/Quality **DEVE** ter referência a pelo menos **1 evidence artifact** do tipo apropriado.
- **REG-EVID-002**: `EVT-014 DASHBOARD_URL` **NÃO É suficiente como evidence isolada** — sempre combinar com `EVT-013 DASHBOARD_SNAPSHOT` pra auditabilidade temporal.
- **REG-EVID-003**: Evidence **DEVE** ser referenciada com formato estruturado:
  - Preferível: `[EVT-XXX]: <URL ou path>`
  - Aceitável: descrição textual com link, desde que tipo EVT-XXX esteja claro
- **REG-EVID-004**: Evidence com retenção expirada invalida a verificação; se o gate é re-auditado após expiração, evidence **DEVE** ser re-coletada.
- **REG-EVID-005**: Evidence imutável é aquela que atende pelo menos UMA de:
  - Assinada criptograficamente (cosign, GPG)
  - Hash-addressed (conteúdo referenciado por hash SHA-256+)
  - Imutável por plataforma (git commit, GitHub run ID, Sigstore log)
  - Timestamped por fonte confiável (RFC 3161 timestamping)

### 35.7.3 Mapping gate → tipos de evidence apropriados

Exemplos de pareamento esperado (não-exaustivo):

| Gate típico (WI §10) | Tipos de evidence aceitáveis |
|---|---|
| 10.1.7 `cargo clippy` clean | EVT-001 (CI log) |
| 10.2.1 Todos AC testados e verdes | EVT-002 (test output) |
| 10.2.5 Property test ≥ 1000 iter | EVT-002 + EVT-009 (mutation opcional) |
| 10.4.1 Métricas emitindo | EVT-013 (dashboard snapshot) + EVT-014 (URL) |
| 10.5.6 Dependencies scanned | EVT-007 |
| 10.5.7 SAST clean | EVT-005 |
| 10.5.8 DAST clean (se HTTP) | EVT-006 |
| 10.7.1 Benchmark adicionado | EVT-004 |
| 12 Invariant preservado | EVT-002 (property test) ou EVT-022 (TLA+ se CRITICAL) |
| 15 Chaos experiment executado | EVT-023 |
| 17 Rollback testado em staging | EVT-017 (runbook execution) |
| 21 Sign-off humano | EVT-015 (PR approval) + EVT-016 (human signoff) |

### 35.7.4 Armazenamento de evidence

Bucket R2 `corelink-specs-evidence/` com estrutura:

```
evidence-screenshots/
  YYYY/MM/DD/<artifact_id>_<hash>.png

evidence-snapshots/
  grafana/<dashboard_id>/<timestamp>_<hash>.png

evidence-runbooks/
  <runbook_id>/<YYYY-MM-DD>_<dry_run_id>.cast   # asciinema

evidence-vendor/
  YYYY/<vendor>/<ticket_id>_<hash>.pdf

evidence-audits/
  YYYY/<auditor>/<report_id>.pdf.sig
```

- **REG-EVID-010**: Bucket **DEVE** ter versioning ativado (imutabilidade via S3 Object Lock ou equivalente).
- **REG-EVID-011**: Acesso ao bucket **DEVE** ser read-only pra usuários não-admin (prevent modification).
- **REG-EVID-012**: Retention policy **DEVE** ser configurada por prefixo do bucket (aplicar tabela §35.7.1).

### 35.7.5 Validação em CI

Script `scripts/validate_evidence.py` (planejado):

1. Parseia todos os WI/ST/PRR com `work_status: COMPLETE | SEALED | DONE | APPROVED | CONDITIONALLY_APPROVED`.
2. Para cada caixa ✅ em Completeness/DoD/Quality, extrai evidence referenciada.
3. Valida:
   - Formato de referência (tipo EVT-XXX identificado)
   - URL acessível (se URL)
   - Timestamp dentro da janela de retenção
   - Se EVT-013, confirma snapshot real (não só URL EVT-014)
4. Falha CI se caixa ✅ sem evidence válida.

### 35.7.6 Anti-padrões de evidence

- ❌ **AP-EVID-001**: "evidence: PR review" sem link específico. Use EVT-015 com URL exato.
- ❌ **AP-EVID-002**: "evidence: roda no CI" sem citar run ID. Use EVT-001 com URL.
- ❌ **AP-EVID-003**: Screenshot sem timestamp visível. Use EVT-012 com EXIF ou sobrepor timestamp.
- ❌ **AP-EVID-004**: "evidence: Grafana dashboard" (só URL ativo). Use EVT-013 com snapshot imutável.
- ❌ **AP-EVID-005**: "evidence: conversa no Slack". Slack não é imutável nem auditável. Promover a EVT-016 (human signoff formal).
- ❌ **AP-EVID-006**: "evidence: test coverage" sem número específico. Use EVT-003 com relatório que mostra percentual.
- ❌ **AP-EVID-007**: Evidence reutilizada em gates não-relacionados ("mesmo CI log vale pra 10 gates distintos"). Cada gate **DEVE** ter evidence específica.

---

## 36. Processo de Revisão e Sign-off

### 36.1 Papéis

| Papel | Responsabilidade |
|---|---|
| **Autor** | Escreve, responde a comentários, itera |
| **Revisor Técnico** | Rigor técnico, rastreabilidade, testabilidade |
| **Revisor de Produto** | Alinhamento com visão, JTBDs, personas |
| **Revisor de Segurança** | Threat model, controles, compliance |
| **Revisor de Privacidade** | Privacy by Design, LINDDUN, direitos do titular |
| **Revisor de Operações** | Observability, SLOs, runbooks, reversibilidade |
| **Revisor de Compliance** | GDPR/LGPD/HIPAA/SOC 2 aplicáveis |
| **Aprovador Final** | Autoriza `REVIEW → FROZEN` |

### 36.2 SLAs de revisão

| Tipo de documento | SLA padrão |
|---|---|
| ADR/PDR/ODR | 48h |
| Capability | 72h |
| Sprint Contract | 96h |
| PR/FAQ | 1 semana |
| Quality Framework (Nível 5) | 2 semanas |
| Framework 00 (este doc) | 2 semanas |

### 36.3 Critério de aprovação

Documento **PODE** ser *frozen* se:

- [ ] Todos revisores requeridos deram ✅.
- [ ] Todos comentários `MUST_FIX` resolvidos.
- [ ] Aprovador Final assinou.
- [ ] Critérios de *freeze* específicos do nível satisfeitos.
- [ ] CI verde (lint, trace check, link check).

### 36.4 Classificação de comentários

| Tipo | Impacto |
|---|---|
| `MUST_FIX` | Bloqueia *freeze* |
| `SHOULD_FIX` | Não bloqueia; registra dívida |
| `NIT` | Melhoria estilística opcional |
| `QUESTION` | Pede esclarecimento |
| `PRAISE` | Reconhece ponto forte |

### 36.5 Dissent protocol

Em caso de *dissent* irreconciliável:

1. Discussão escrita ≥ 24h no documento.
2. Se não resolvido, escalação ao Aprovador Final.
3. Aprovador decide com justificativa registrada no *change log*.
4. *Dissent* minoritário preservado em apêndice `## Apêndice: Dissents`.

---

## 37. Tooling e CI

### 37.1 Linters obrigatórios

| Ferramenta | Propósito |
|---|---|
| `markdownlint` | Sintaxe Markdown |
| `vale` | *Prose linting*: banned words, style, termos capitalizados |
| `lychee` ou `markdown-link-check` | Links válidos |
| `trace-check` (custom) | Rastreabilidade CAP↔WI↔teste↔código |
| `frozen-check` (custom) | Docs FROZEN não mudaram sem *thaw* |
| `id-check` (custom) | Numeração consistente, sem duplicatas |
| `diagram-check` (custom) | Mermaid parseia sem erro |

### 37.2 CI obrigatório em PRs de specs

- [ ] markdownlint: zero erros
- [ ] vale: zero `error` (warnings aceitos)
- [ ] link checker: 100% válidos
- [ ] trace-check: toda referência aponta para ID existente
- [ ] frozen-check: docs FROZEN imutáveis sem label `thaw`
- [ ] id-check: nenhum ID duplicado
- [ ] diagram-check: Mermaid válido

### 37.3 PR discipline

- **REG-PR-001**: PR que altera doc FROZEN **DEVE** ter label `thaw` + link justificativa.
- **REG-PR-002**: PR que cria doc novo **DEVE** declarar IDs atribuídos, parent upstream, status inicial (`DRAFT`).
- **REG-PR-003**: PR que altera numeração **DEVE** ser rejeitado (PRINC-006 violado).
- **REG-PR-004**: PR descrição **DEVE** explicar *por que*, não apenas *o quê*.

---

## 38. Red Team e Adversarial Review

### 38.1 Propósito

Specs podem estar **bem escritas mas erradas**. Adversarial review simula:

- Ataque de segurança (red team).
- Questionamento de produto (contrarian review).
- Simulação de suporte (customer frustration).
- Simulação de engenheiro novo (onboarding).

### 38.2 Cadência

| Tipo | Frequência |
|---|---|
| Security red team | Trimestral |
| Product contrarian review | Antes de launch major |
| Customer frustration review | Antes de *freeze* de PR/FAQ |
| New engineer onboarding review | Trimestral |

### 38.3 Output

- Red team produz relatório com *findings* classificados por severidade.
- *Findings* viram ADRs/PDRs se requerem decisão estratégica.
- Specs atualizadas conforme necessário (com *thaw* formal).

---

# Parte VIII — Referência

## 39. Anti-padrões

### AP-001 — Spec como "whiteboard ao vivo"

❌ Specs mudando toda semana sem versionamento, sem *freeze*, sem *change log*.
✅ Specs versionadas, com *freeze*, auditoria de mudança.

### AP-002 — "Decidimos durante a implementação"

❌ Deixar decisões arquiteturais para o sprint.
✅ Decidir em ADR (Nível 3) **antes** do sprint.

### AP-003 — Capability vaga

❌ "O sistema deve permitir o usuário gerenciar cache."
✅ "CAP-042: O sistema **DEVE** expor `DELETE /v1/tenants/{id}/cache` que deleta *blobs* do *tenant* de forma idempotente, retornando 204 em ≤ 30s p99, emitindo `tenant.cache.purged`."

### AP-004 — Teste como documentação

❌ "O teste de integração é a spec."
✅ Spec é documento prosa + RFC 2119 + invariants. Testes verificam spec mas não a substituem.

### AP-005 — "Isso é detalhe de implementação"

❌ Esconder decisões importantes sob "detalhe".
✅ Se altera invariants, NFRs ou trade-offs: **decisão arquitetural**, merece ADR.

### AP-006 — Sprint que nunca termina

❌ Sprint com "últimos 5% pendentes" que se estende.
✅ Sprint tem *exit criteria* binários. Não cumpriu? Sprint **não passa**. Decomponha ou declare falha.

### AP-007 — Cerimônia sem rigor

❌ Muito documento com pouco conteúdo verificável.
✅ Cada seção com propósito. Se não afeta decisão, teste ou comportamento, não pertence.

### AP-008 — Premature spec optimization

❌ Detalhar *ad nauseam* áreas que não serão construídas em 6+ meses.
✅ Granularidade proporcional à proximidade de execução, mas conjunto completo garantindo nada essencial fique fora.

### AP-009 — Falta de anti-scope

❌ Documento lista o que faz, não o que **não** faz.
✅ Anti-scope explícito previne *scope creep* e *feature bloat*.

### AP-010 — "Security depois"

❌ "Vamos lançar, depois endurecemos."
✅ Segurança é fundação (PRINC-014, PRINC-019). *Retrofit* de segurança custa 10–100× mais.

### AP-011 — "Observability no próximo sprint"

❌ Código em produção sem métricas/logs/traces.
✅ PRINC-013: observability é parte da spec de capability.

### AP-012 — ADR sem alternativas

❌ ADR que declara decisão mas não enumera alternativas.
✅ PRINC-011: ≥ 2 alternativas registradas, com pros/cons/razão de rejeição.

### AP-013 — One-way door sem consciência

❌ Tomar decisão irreversível sem reconhecer que é irreversível.
✅ PRINC-012: toda decisão classifica reversibilidade explicitamente.

### AP-014 — Métrica de vaidade

❌ "Temos 10.000 usuários" sem contexto (ativos? pagantes? DAU?).
✅ Métricas de sucesso têm definição operacional clara + *guardrail metric* inverso.

### AP-015 — Feature flag eterna

❌ Feature flag implementada, nunca removida, acumulando débito.
✅ REG-PD-003: toda flag tem *kill date* e owner.

### AP-016 — Runbook não-testado

❌ Runbook escrito mas nunca executado.
✅ Runbooks testados em *dry-run* trimestralmente; chaos testing dispara cenários.

### AP-017 — Silent failure

❌ Exception engolida com `_ = result` ou `catch { }`.
✅ Toda falha emite métrica/log/trace. Silêncio é bug.

### AP-018 — Premature abstraction

❌ Criar trait/interface genérica para 1 implementação "porque vai crescer".
✅ YAGNI. Abstração aparece quando 3ª implementação motiva.

### AP-019 — Shared mutable state escondido

❌ `static mut`, `lazy_static` com `Mutex`, `Arc<Mutex<Arc<Mutex<...>>>>`.
✅ Estado explícito, owned, preferir *actor model* ou *message passing*.

### AP-020 — God doc

❌ Um doc de 10.000 linhas que "cobre tudo".
✅ Documentos focados, linkados, com escopo definido. Este framework é limite (e justificado por ser *meta*).

---

## 40. Glossário

> Termos canônicos do CoreLink. Uso **consistente** em specs, código, logs, UI, docs (Ubiquitous Language — PRINC-017).

### 40.1 Produto

| Termo | Definição |
|---|---|
| **CoreLink** | Nome do produto: *shared content-addressable cache for developers*. |
| **Tenant** | Entidade isolada (usuário individual ou organização) que consome CoreLink. |
| **Principal** | Entidade autenticada agindo em nome de um *tenant* (user, service, token). |
| **Workspace** | Sinônimo proibido de *tenant*. Não usar. |
| **Organization** | Sinônimo proibido de *tenant* em contexto CoreLink. |

### 40.2 Storage / CAS

| Termo | Definição |
|---|---|
| **CAS** | *Content-Addressable Storage*: armazenamento indexado por *digest* do conteúdo. |
| **AC** | *Action Cache*: mapeamento de *action digest* → *result digest* (REAPI). |
| **Digest** | Hash criptográfico identificador de um *blob* (tipicamente BLAKE3 ou SHA-256). |
| **Digest function** | Função de hash: BLAKE3, SHA-256. |
| **Blob** | Objeto armazenado em CAS, identificado por *digest*. |
| **Manifest** | *Blob* especial contendo metadata de *blob* decomposto em *chunks* Merkle. |
| **Chunk** | Bloco de tamanho fixo (ex: 2 MiB) resultante de decomposição de *blob* grande. |
| **Dedup** | *Deduplication*: reaproveitamento de *blob/chunk* idêntico entre referências. |

### 40.3 Protocolos

| Termo | Definição |
|---|---|
| **REAPI** | *Remote Execution API* v2, spec Bazel. |
| **Bazel HTTP cache** | Protocolo HTTP alternativo do Bazel. |
| **sccache** | Cache de compilador Mozilla. |
| **Turborepo Remote Cache** | Cache HTTP Vercel para Turborepo. |
| **OCI Distribution** | Spec Docker/container image distribution. |
| **GOPROXY** | Protocolo Go modules proxy. |

### 40.4 Processo de spec

| Termo | Definição |
|---|---|
| **Sprint Contract** | Documento formal inviolável de Nível 4. |
| **DoR** | *Definition of Ready*: critérios de entrada de sprint. |
| **DoD** | *Definition of Done*: critérios de saída de sprint. |
| **Frozen** | Estado imutável de documento. |
| **Thaw** | Reabertura controlada de documento *frozen*. |
| **Supersedes** | Relação de substituição (ADR-B supersedes ADR-A). |

### 40.5 Invariants

| Termo | Definição |
|---|---|
| **Safety invariant** | "Nada ruim acontece": ∀ estado alcançável, P(estado) é verdade. |
| **Liveness invariant** | "Algo bom eventualmente acontece": ∃ momento futuro em que P será verdade. |
| **Blast radius** | Escopo de impacto de violação de invariante ou falha. |

### 40.6 Reversibility / Decision

| Termo | Definição |
|---|---|
| **One-way door** | Decisão custosa ou impossível de reverter (Bezos framework). |
| **Two-way door** | Decisão facilmente reversível. |
| **Hybrid door** | Reversível dentro de janela T; após T, one-way. |
| **ADR / PDR / ODR** | Architecture / Product / Operational Decision Record. |

### 40.7 SRE / Observability

| Termo | Definição |
|---|---|
| **SLI** | Service Level Indicator (métrica). |
| **SLO** | Service Level Objective (alvo). |
| **SLA** | Service Level Agreement (contratual). |
| **Error Budget** | `1 - SLO` tolerável em janela. |
| **Four Golden Signals** | Latency, Traffic, Errors, Saturation. |
| **MTTA** | Mean Time To Acknowledge. |
| **MTTD** | Mean Time To Detect. |
| **MTTR** | Mean Time To Resolve. |
| **MTBF** | Mean Time Between Failures. |

### 40.8 Segurança

| Termo | Definição |
|---|---|
| **Trust boundary** | Linha onde nível de confiança muda (ex: internet ↔ datacenter). |
| **STRIDE** | Framework threat modeling (Microsoft). |
| **LINDDUN** | Framework privacy threat modeling. |
| **SLSA** | *Supply-chain Levels for Software Artifacts*. |
| **SBOM** | *Software Bill of Materials*. |
| **PII** | *Personally Identifiable Information*. |

### 40.9 Misc

| Termo | Definição |
|---|---|
| **Idempotent** | Operação que, executada N vezes, tem mesmo efeito que 1 vez. |
| **Ubiquitous Language** | Vocabulário único e consistente (DDD, Evans). |
| **Fitness function** | Check automatizado de propriedade arquitetural (Neal Ford). |
| **Blameless post-mortem** | Análise pós-incident focada em sistema, não pessoas. |
| **PWI** (Partial Work Item) | WI re-escopado durante escalação (§20 do work_item template) para entregar um subconjunto viável do escopo original. Features retiradas movem para WI(s) novo(s) com IDs próprios. Ver REG-WI-SPLIT-001. |
| **audit_status** | Metadata adicional de qualquer doc/artefato com estados `ACTIVE`, `AUDIT_PENDING`, `AUDITED`. Setado `AUDIT_PENDING` quando um upstream `FROZEN` é *thawed* e afeta este doc (§7.10). Voltando a `ACTIVE` após auditoria. |

### 40.10 Vocabulário do Lote 2 (Risk Lanes / Inheritance / Waivers / Evidence)

| Termo | Definição |
|---|---|
| **Lane** (`LOW_RISK`, `STANDARD`, `HIGH_RISK`) | Classificação de risco de uma unidade de trabalho (Sprint/WI/ST) que determina proporção de cerimônia, sign-offs e gates obrigatórios. Ver §33.5. |
| **Forcing factor** (`FF-HR-NNN`) | Gatilho que força classificação `HIGH_RISK` independente de scoring (§33.5.3). Ex: FF-HR-002 = toca tenant isolation. |
| **Control inheritance** | Mecanismo pelo qual artefato downstream (Sprint/WI/ST/PRR) herda controles de uma fonte canônica via campo `inherits_from`, sem duplicar (§35.5). |
| **Canonical source** | Documento autoritativo único para uma *cross-cutting concern* (ex: `observability_model.md` para observability). Vive em Nível 3 (arquitetura) ou Nível 5 (qualidade). Ver §35.5.1. |
| **inherits_from** | Campo YAML listando IDs de fontes canônicas das quais o documento herda controles. |
| **local_deltas** | Campo YAML listando seções onde o documento legitimamente diverge ou estende os controles herdados, com rationale obrigatório. |
| **Waiver** (`WAIVER-YYYYMMDD-NNN`) | Exceção formal e temporária a um ou mais gates do framework. Exige `expires_at`, `compensating_control`, `rationale`, `revalidation_trigger`. Ver §35.6. |
| **Compensating control** | Proteção alternativa vigente durante o período de um waiver, substituindo (parcial ou totalmente) o controle dispensado. |
| **Revalidation trigger** | Evento/condição que força re-review de um waiver antes de `expires_at`. Ex: "vendor lança feature X", "audit finding Y". |
| **Gates_waived** | Lista de IDs de gates do framework (formato `REG-XXX-NNN`) dispensados por um waiver específico. Não aceita prosa genérica. |
| **Evidence taxonomy** | Conjunto de 24 tipos formais (`EVT-001` a `EVT-024`) que classificam evidências aceitas para gates. Cada tipo tem regras de formato, armazenamento, imutabilidade e retenção. Ver §35.7. |
| **Evidence type** (`EVT-XXX`) | ID canônico de um tipo de evidência. Exemplos: `EVT-001 CI_LOG`, `EVT-013 DASHBOARD_SNAPSHOT`, `EVT-022 TLA_MODEL_CHECK`. |
| **Immutable evidence** | Evidência que atende ≥ 1 critério: assinatura criptográfica, hash-addressed, plataforma imutável (git/CI run), timestamp RFC 3161. `DASHBOARD_URL` isolado **não é** immutable. |

---

## 41. Meta-regras (Evolução deste Framework)

### 41.1 Como este framework evolui

Este documento segue as regras que define:

- Tem *status*, versão, *change log*, *sign-off*.
- Mudanças passam por `DRAFT → REVIEW → FROZEN`.
- *Bump* de **major** requer auditoria de todos documentos downstream.
- *Bump* de **minor** sem auditoria obrigatória, mas com *change log*.
- *Patch* para correções tipográficas e esclarecimentos que **não** mudam semântica.

### 41.2 Semver para specs

| Bump | Critério |
|---|---|
| Major (X.0.0) | Mudança em princípio fundamental, regra inviolável, template obrigatório, ou remoção de capacidade |
| Minor (X.Y.0) | Adição de princípio, regra, seção, template ou refinamento não-quebrador |
| Patch (X.Y.Z) | Correção tipográfica, clarificação sem mudança semântica |

### 41.3 Exceções

Em casos em que regra deste framework impede resolução de problema real:

1. Proposição formal (PR com label `framework-change`).
2. Revisão obrigatória pelo Aprovador Final.
3. Aceitação **apenas se** alternativas foram exauridas.
4. Documentação em `## 41.4 Exceções Históricas`.

### 41.4 Exceções Históricas

*Nenhuma no momento.*

### 41.5 Revisão periódica

- **Trimestral**: revisão leve por Aprovador Final + 1 engenheiro — caça drift, desatualizações.
- **Anual**: revisão profunda com adversarial review (§38).

---

## 42. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Versão inicial (15 princípios, estrutura básica) |
| 0.2.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | **Revisão SOTA**: expandiu 15→30 princípios; adicionou Partes III (linguagem), IV (rigor formal), V (cross-cutting concerns), VI (engenharia e entrega); 24 seções novas; formalizou TLA+, ISO/IEC 25010, SRE, STRIDE, LINDDUN, SLSA, Cavoukian, DDD Ubiquitous Language, Feature Lifecycle, Progressive Delivery, Resilience Patterns, Fitness Functions, Technical Debt, AI Governance; 11 anti-padrões adicionais (9→20) |
| 0.3.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | **Lote 1 — audit remediation (GPT)**: (a) separou formalmente `doc_status` de `work_status` (§7 reescrita: §7.1 doc_status, §7.2 work_status por tipo, §7.3 mapping matrix, §7.4 ADR terminology reconciliation, §7.7 INV-LIFECYCLE-001, §7.11 YAML front matter obrigatório). (b) Corrigiu drift de section counts (WI 33 seções §0–§32, ST 19 §0–§18, Sprint 23 §0–§22, PRR 23 §0–§22, ADR 16 §0–§15). (c) Corrigiu refs quebradas ao glossário (§15 → §40). (d) Corrigiu WI split ID violation — REG-WI-SPLIT-001 força IDs sequenciais novos, proíbe sufixos `.a/.b`. (e) Alinhou SLSA gate: framework L3 até GA, PRR agora exige L2 para canary ≤ 10%, L3 para rollout ≥ 50%. (f) Sprint contract: adicionou `FAILED` state ao header. (g) Todos templates (sprint, WI, ST, ADR, PRR) agora têm YAML front matter + human-readable block com `doc_status` + `work_status` separados. (h) PRR gate desambiguado: `CONDITIONALLY_APPROVED` permite apenas canary ≤ 10%; caveats obrigam `expires_at`; expiração força auto-revert para `IN_REVIEW`. |
| 0.3.1 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | **Lote 1-bis — correção pós-review GPT**: (a) YAML front matter real no topo absoluto dos 6 docs (sem code fence, parseável por `yaml.safe_load`), valida em CI. (b) Campos schema expandidos: `audit_status` (ACTIVE/AUDIT_PENDING/AUDITED), `type` com enum completo, `reviewers` como lista de `{role,name}`. (c) Eliminadas 10+ referências residuais a `Status`/`ACCEPTED`/`SEALED` no corpo dos 5 templates — agora todos usam `doc_status`/`work_status` consistentemente. (d) §34 count drift residual corrigido (32→33 totais, 18→19 totais). (e) Regra WI split unificada: original usa `superseded_by`, sucessores usam `supersedes`; referência corrigida §22→§31. (f) PWI formalmente definido no glossário §40.9. (g) PRR hook em WI §16 agora lista todos 5 work_status incluindo `REJECTED`; adiciona regra inviolável bloqueando WI DONE sem PRR OK. (h) §7.11 totalmente reescrita com tabelas de campos obrigatórios, templates canônicos e script de validação CI. |
| 0.3.2 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | **Lote 1-ter — cleanup final pós second GPT pass**: (a) CI script escope corrigido — exclui `_audits/` e `_archive/`, previne falso negativo em arquivos de review. (b) Schema `supersedes`/`superseded_by` agora aceita `string | lista de strings | null` — permite split 1→N e consolidação N→1 (REG-WI-SPLIT-001). (c) Schema §7.11.2 reconciliado: PRR usa `feature_wi` em vez de `parent` (relação lateral, não hierárquica); `feature_wi` adicionado como campo obrigatório apenas pra `type=prr`. (d) Coluna `Status` genérica renomeada pra `Estado requerido` em tabelas de trace (WI §4, ST §2, ADR §12.1, WI §18.1, sprint §5.1) e pra `Atendido?` em tabelas de checklist (sprint §5.x). (e) Drift `14 seções` → `19 seções §0–§18` em WI §17. (f) Framework cleanup: 5 ocorrências residuais de `ACCEPTED`/`PROPOSED` fora dos templates corrigidas (§5.3, §6.3, §14.2 template NFR, §34.6 template CAP, §34.8 template INV). (g) Duplicata §34.6/§34.7 removida. Validação YAML re-confirmada em 6 docs. |
| 0.3.3 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | **Lote 1-quater — propagação parcial dos fixes de §7.11**: (a) §7.11.3 snippet canônico agora mostra `supersedes`/`superseded_by` aceitando escalar OU lista (match tabela §7.11.1, não §7.11.2 como o registro anterior dizia). (b) §7.11.4 Level 4 template agora inclui `feature_wi`, `capabilities`, `prod_target_date` apenas pra `type: prr` (template real de PRR). **ATENÇÃO:** schema table §7.11.2 **ainda não** declarava `capabilities` e `prod_target_date` neste ponto — corrigido em v0.3.4. (c) 3 resíduos finais de `\| Status \|` corrigidos: sprint §13.1 dependencies → `Atendida?`; ST §11.1 dependencies → `Atendida?`; WI §4 JTBD row valor `FROZEN` cru → `doc_status: FROZEN`. Backlog parcialmente zerado; drift schema↔snippet resolvido em v0.3.4. |
| 0.3.4 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | **Lote 1-quinquies — fechamento schema §7.11.2 + correção retroativa de auditabilidade**: (a) Schema §7.11.2 adiciona `capabilities` (lista de strings, obrigatório apenas pra `type: prr`) e `prod_target_date` (ISO date, obrigatório apenas pra `prr`). (b) `work_status` e `assignee` ganharam aplicabilidade explícita por type. (c) Entrada v0.3.3 do change log **corrigida retroativamente**: trocada "tabela §7.11.2" por "tabela §7.11.1" (localização correta dos campos `supersedes`/`superseded_by`); marcada a propagação schema↔snippet como parcial em v0.3.3, completa em v0.3.4. Honestidade de auditabilidade preservada. Backlog de Lote 1 **finalmente zerado** (exceto os 2 itens reservados explicitamente pra Lotes 5 e 6). |
| **0.4.2** | **2026-04-24** | **Gustavo (via Claude Opus 4.7)** | **Lote 3 — Aplicação de inheritance + lane annotations nos templates.** Atende audit v1 §6 (Redundâncias) e §4 (Over-engineering) sem remover conteúdo: adiciona **annotations explícitas** em seções duplicadas/condicionais pra marcar (a) `🔗 Herdado de: <fonte canônica>` apontando pros docs pendentes do Lote 4, e (b) `Lane applicability: ...` referenciando matriz §33.5.4.1. Seções anotadas: **work_item.md** §3 (Customer Impact — lane), §15 (Chaos — lane+FMEA herança), §16 (PRR hook — lane), §21 (Observability — herança+lane), §22 (Cost — lane+ADR herança), §23 (API Impact — lane+ADR), §25 (Rollback — FM+Resilience herança), §26 (Security & Privacy — security_model+privacy_model herança), §27 (Knowledge Transfer — lane). **sprint_contract.md** §14 (Observability — herança, rollup), §15 (Rollback — FM+Resilience herança, delta do sprint), §16 (Security & Compliance — 3 fontes canônicas, delta). **production_readiness_review.md** §4 (SLO — slo_catalog herança), §9 (Observability — herança), §11 (Security & Compliance — herança), §12 (Privacy — privacy_model herança). Resultado: quando fontes canônicas do Lote 4 forem criadas, essas seções vão se reduzir pra deltas locais. Hoje viram "preencher inline enquanto canonical source não existe" — trajetória explícita registrada. Bump patch 0.4.1 → 0.4.2. |

| **0.4.1** | **2026-04-24** | **Gustavo (via Claude Opus 4.7)** | **Lote 2-bis — integração pós self-audit.** Fecha os 4 findings CRITICAL + 6 HIGH do self-audit `2026-04-24-self-audit-lote2.md` (commit 59844d6). **(F-01)** TOC atualizado com §33.5, §35.5, §35.6, §35.7 (4 seções órfãs integradas à navegação; duplicata §35 removida). **(F-02)** Schema declara `lane_forcing_factors` com `pattern: ^FF-HR-\\d{3}$` + regra cross-field `if lane=HIGH_RISK then required: [lane_forcing_factors]`. **(F-03)** `lane` agora obrigatório no schema pra `type ∈ {sprint, work_item, sub_task}`; sub_task restrito a `[LOW_RISK, STANDARD]` (REG-LANE-004). **(F-04)** §6.1 Formatos canônicos adiciona `EVT-XXX`, `WAIVER-YYYYMMDD-NNN`, `FF-HR-NNN` + linha enum `Risk Lane`. **(F-05)** §40.10 novo subsection "Vocabulário do Lote 2" com 13 termos (lane, forcing factor, control inheritance, canonical source, inherits_from, local_deltas, waiver, compensating control, revalidation trigger, gates_waived, evidence taxonomy, EVT type, immutable evidence). **(F-06)** §7.11.1 adiciona `inherits_from` e `local_deltas`; §7.11.2 adiciona `lane` e `lane_forcing_factors`. **(F-07)** 4 novos princípios fundamentais em §2.5 (PRINC-035 a PRINC-038) elevando risk lanes, inheritance, waivers e evidence taxonomy a invioláveis. **(F-08)** §4.7 novo "Artefatos transversais" documentando `_schemas/`, `_templates/`, `_waivers/`, `_audits/`, `_archive/`. **(F-09)** Sign-off matrix do template WI §30 alinhada com framework §33.5.4.3 (coluna Aplicabilidade por lane + totais mínimos). **(F-14)** §35.5.7 nova "Precedência herança vs waiver" (REG-INHERIT-020 a 023) — waiver tem precedência explícita sobre controle herdado enquanto ativo; volta automático ao expirar. **(F-15)** Schema `reviewers[].role` agora tem enum fechado de 21 papéis canônicos (previne drift "security" vs "Security" vs "sec"). Bump patch 0.4.0 → 0.4.1. |

| **0.4.0** | **2026-04-24** | **Gustavo (via Claude Opus 4.7)** | **Lote 2 — Framework SOTA com machine-readable real, risk lanes, control inheritance, waivers, evidence taxonomy.** 5 sub-lotes commitados incrementalmente: **(2.1)** JSON Schema real em `specs/_schemas/front_matter.schema.json` (draft 2020-12), enums validados, cross-field rules via `allOf`+`if`/`then`, schema para 35 types; script `scripts/validate_specs.py` com 2 camadas (YAML parseability em tudo; JSON Schema exceto templates). **(2.2)** Risk lanes: §33.5 novo com 3 lanes (LOW_RISK/STANDARD/HIGH_RISK), scoring de 5 dimensões, 10 forcing factors (FF-HR-*), matriz de gates por lane pra WI (33 seções categorizadas ✅/🟡/⛔), matriz de sign-offs (3/5-8/10-12 roles), 4 regras REG-LANE + auditoria trimestral. Reduz cerimônia sem reduzir rigor. **(2.3)** Control inheritance: §35.5 com catálogo de 15 fontes canônicas (observability_model, security_model, privacy_model, slo_catalog, failure_modes, etc em Nível 3), campo YAML `inherits_from`+`local_deltas`, 8 regras REG-INHERIT, mata duplicação em WI/Sprint/ST/PRR. **(2.4)** Waivers: §35.6 + `_templates/waiver.md` (11 seções: rationale, compensating_control, expires_at obrigatório, revalidation_trigger, review checkpoints T+25/50/75/90%, plano de resolução), 7 regras REG-WAIVER, enforcement automático via expiração, 5 anti-patterns catalogados (AP-WAIVER-*). Diretório `specs/_waivers/` criado. **(2.5)** Evidence taxonomy: §35.7 com **24 tipos formais de evidence** (EVT-001 a EVT-024: CI_LOG, TEST_OUTPUT, COVERAGE_REPORT, BENCH_REPORT, SAST/DAST_SCAN, FUZZ_REPORT, MUTATION_REPORT, SBOM, BINARY_SIGNATURE, SCREENSHOT, DASHBOARD_SNAPSHOT/URL, PR_APPROVAL, HUMAN_SIGNOFF, RUNBOOK_EXECUTION, MIGRATION_APPLIED, INCIDENT_LINK, EXTERNAL_VENDOR_CONFIRMATION, AUDIT_REPORT, TLA_MODEL_CHECK, CHAOS_EXPERIMENT_REPORT, LOAD_TEST_REPORT), cada um com formato/armazenamento/imutabilidade/retenção. Regra crítica: EVT-014 DASHBOARD_URL sozinho não é evidence válida (precisa EVT-013 SNAPSHOT). Mapping gate→tipos apropriados. Estrutura de bucket R2 `corelink-specs-evidence/`. 7 anti-patterns (AP-EVID-*). Preparação pra `scripts/validate_evidence.py`. **Resolve audit v1 Gaps 3, 4, 5 + reduz redundâncias (§6 do audit).** |

---

## 43. Revisão

### 43.1 Revisores requeridos para *freeze*

- [ ] **Gustavo Schneiter** (Aprovador Final) — ____________ YYYY-MM-DD
- [ ] **Revisor Técnico** *(a nomear)* — ____________ YYYY-MM-DD
- [ ] **Revisor de Segurança** *(a nomear)* — ____________ YYYY-MM-DD
- [ ] **Revisor de Produto** *(a nomear)* — ____________ YYYY-MM-DD

### 43.2 Comentários de revisão

*(a preencher durante revisão)*

---

**Fim do Framework 00 — Spec de como especificamos o CoreLink.**
