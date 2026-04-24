# 00 — Specification Framework

> **Status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador final:** Gustavo Schneiter
> **Revisores:** *(a definir)*
>
> Este é o **meta-documento** do sistema de especificação do **HuGR CoreLink**. Define *como* especificamos o produto — regras, hierarquia, vocabulário, templates, disciplina. Nenhum outro documento de spec pode existir sem antes este estar *frozen*.

---

## Sumário

0. [Meta](#0-meta)
1. [Propósito](#1-propósito)
2. [Princípios Fundamentais (Invariantes da Spec)](#2-princípios-fundamentais-invariantes-da-spec)
3. [Working Backwards — A Disciplina](#3-working-backwards--a-disciplina)
4. [Hierarquia dos 5 Níveis](#4-hierarquia-dos-5-níveis)
5. [Rastreabilidade (Traceability)](#5-rastreabilidade-traceability)
6. [Numeração e Identificadores Estáveis](#6-numeração-e-identificadores-estáveis)
7. [Frozen Flag System — Ciclo de Vida de Documentos](#7-frozen-flag-system--ciclo-de-vida-de-documentos)
8. [RFC 2119 — Vocabulário Normativo](#8-rfc-2119--vocabulário-normativo)
9. [Política de Linguagem (pt-BR + EN)](#9-política-de-linguagem-pt-br--en)
10. [Templates](#10-templates)
11. [Diagramas](#11-diagramas)
12. [Processo de Revisão e Sign-off](#12-processo-de-revisão-e-sign-off)
13. [Tooling e CI](#13-tooling-e-ci)
14. [Anti-padrões](#14-anti-padrões)
15. [Glossário](#15-glossário)
16. [Meta-regras (Evolução deste Framework)](#16-meta-regras-evolução-deste-framework)
17. [Change Log](#17-change-log)
18. [Revisão](#18-revisão)

---

## 0. Meta

### 0.1 Propósito deste documento

Este framework existe para garantir que **todo artefato de especificação do CoreLink** tenha:

- **Rigor formal** — zero ambiguidade, toda afirmação verificável.
- **Rastreabilidade total** — cada linha de código pode ser rastreada até uma *capability*, uma *invariant*, um *sprint contract*.
- **Qualidade SOTA** — *state-of-the-art* em cada dimensão: clareza, completude, testabilidade, auditabilidade.
- **Disciplina de processo** — *working backwards*, hierarquia inviolável, *frozen flags*, revisões explícitas.

### 0.2 Público-alvo

| Audiência | Uso primário |
|---|---|
| Fundador / Product Owner | Definir visão, aprovar *freezes*, resolver trade-offs |
| Engenheiros | Implementar sprints sob contrato inviolável |
| Revisores de segurança | Auditar invariants, *threat models*, ADRs |
| Revisores de operações | Validar *observability*, *runbooks*, SLOs |
| Stakeholders externos | Compreender decisões arquiteturais e roadmap |
| Claude (eu) | Produzir specs consistentes entre sessões |

### 0.3 O que este documento **não** cobre

- O produto em si (coberto pelos Níveis 1–5).
- Implementação de código (coberto pelos sprints).
- Marketing / GTM (coberto por documentos fora do escopo de specs técnicas).

---

## 1. Propósito

O CoreLink é um produto de alta complexidade técnica e alta aposta operacional: serve artefatos de múltiplos *tenants*, precisa de latência baixa, *tenant isolation* inviolável, e opera em um espaço competitivo SOTA (NativeLink, BuildBuddy, JFrog Artifactory, Sonatype Nexus).

Um produto dessa natureza **não pode ser construído com spec solta**. Cada decisão precisa ser:

1. **Explícita** — está escrita, não no *head canon* de alguém.
2. **Justificada** — tem *rationale* rastreável.
3. **Auditável** — qualquer pessoa qualificada pode validar.
4. **Reversível ou consciente de sua irreversibilidade** — *one-way doors* vs *two-way doors* são identificados.

Este framework é a **infraestrutura de rigor** que permite isso.

---

## 2. Princípios Fundamentais (Invariantes da Spec)

As regras a seguir são **invariantes** do processo de especificação. Violação de qualquer uma delas invalida o documento em questão.

### PRINC-001 — Specs descrevem produto completo, não MVP

> **DEVE** todo artefato de spec descrever o produto em sua **forma final completa**.
> **NÃO DEVE** nenhum artefato conter formulações do tipo "no MVP" ou "inicialmente".
> MVP não é conceito de spec; é conceito de *sprint*.

O **destino** (produto completo) é descrito nas specs e é **imutável** exceto por evolução explícita.
O **caminho** (ordem de entrega) vive nos sprints (Nível 4) e é faseado.

### PRINC-002 — Working backwards é inviolável

> **DEVE** toda spec ser produzida na ordem: Nível 1 → 2 → 3 → 4 → 5.
> **NÃO DEVE** um documento de nível N referenciar artefatos ainda não *frozen* de nível N ou inferior.

Corolário: não escrevemos sprint (Nível 4) enquanto *capabilities* (Nível 2) estão em *draft*.

### PRINC-003 — Zero ambiguidade (RFC 2119)

> **DEVE** toda afirmação normativa usar o vocabulário de RFC 2119 (§8).
> **NÃO DEVE** haver palavras ambíguas como "provavelmente", "deveria ser rápido", "idealmente".

Se não pode ser afirmado com RFC 2119, não é spec — é *wishful thinking*.

### PRINC-004 — Toda afirmação é verificável

> **DEVE** toda afirmação normativa ter um método de verificação associado (teste, métrica, revisão humana explícita).
> **NÃO DEVE** existir afirmação que não possa ser validada.

Exemplos:

- ❌ "O sistema deve ser rápido." (não verificável)
- ✅ "O sistema **DEVE** servir P99 de `CAS::FindMissingBlobs` em ≤ 50 ms para requests com ≤ 1000 digests." (verificável via métrica `corelink_cas_find_missing_duration_seconds`)

### PRINC-005 — Rastreabilidade bidirecional

> **DEVE** todo artefato de nível N referenciar os artefatos de nível N-1 que o originam.
> **DEVE** existir, a qualquer momento, capacidade de navegar:
>
> - **Top-down**: visão → *capabilities* → invariants/NFRs → ADRs → sprints → *work items* → código → testes.
> - **Bottom-up**: uma linha de código ↔ trace reverso até uma *capability*.

Um documento que quebra rastreabilidade **NÃO PODE** ser *frozen*.

### PRINC-006 — Numeração estável

> **NÃO DEVE** nenhum identificador (CAP-XXX, INV-XXX, ADR-XXXX, S-XX, NFR-XXX, WI-SXX-XXX, JTBD-XXX, PERSONA-XXX) ser **renumerado** após criação.
> Quando um artefato é descontinuado, seu status vira `DEPRECATED` ou `SUPERSEDED`, mas o identificador permanece reservado para sempre.

### PRINC-007 — Frozen é serio

> **DEVE** um documento *frozen* permanecer imutável exceto por processo de *thaw* explícito (§7).
> **DEVE** qualquer *thaw* disparar auditoria de downstream dependentes.

### PRINC-008 — Diff é explicação

> **DEVE** toda mudança em documento não-*draft* ser acompanhada de entrada em *Change Log* com: data, autor, mudança, razão.
> **NÃO DEVE** haver mudança "silenciosa" em documentos *frozen* ou *accepted*.

### PRINC-009 — Quality gates são binários

> **DEVE** todo *gate* de qualidade ter resultado binário (✅ / ❌).
> **NÃO DEVE** existir "parcialmente atendido" em *exit criteria*.
>
> Se um *gate* parece exigir granularidade de "70% atendido", ele foi mal desenhado e deve ser **decomposto** em múltiplos *gates* binários.

### PRINC-010 — Anti-scope é tão importante quanto scope

> **DEVE** todo artefato relevante explicitar o que ele **NÃO** cobre.
> Ambiguidade sobre anti-scope é causa raiz de *scope creep* e de *sprints* que nunca terminam.

### PRINC-011 — Alternativas consideradas são registradas

> **DEVE** todo ADR registrar as alternativas consideradas, com pró/con e razão de rejeição.
> **DEVE** haver pelo menos **duas alternativas** (mesmo que uma seja "status quo / não fazer nada").

O exercício de alternativas é o que separa **decisão** de **default inconsciente**.

### PRINC-012 — Reversibilidade é dimensão obrigatória

> **DEVE** todo ADR classificar a decisão como *two-way door*, *one-way door*, ou *hybrid*, e quantificar custo de reversão.

Decisões *one-way* merecem escrutínio proporcionalmente maior.

### PRINC-013 — Observabilidade é parte da spec, não *afterthought*

> **DEVE** toda *capability* especificar suas métricas, logs, traces e alertas.
> **NÃO DEVE** uma *capability* ser marcada como implementada se não tem observability compatível.

### PRINC-014 — Segurança e compliance são parte da spec, não *afterthought*

> **DEVE** toda *capability* que toca dados de *tenant* especificar: classificação de dados, controles de acesso, audit logging, *retention*, implicações de GDPR/LGPD.

### PRINC-015 — Specs são produto

> **DEVE** toda spec ser escrita com o mesmo rigor de código de produção: revisada, versionada, testada (via CI *lint* e *link checker*), distribuída com disciplina de *release*.
> Specs **NÃO SÃO** rascunhos — são artefatos de produção.

---

## 3. Working Backwards — A Disciplina

### 3.1 Origem

Metodologia pioneira na Amazon: antes de construir, **escreve-se o press release do produto lançado** como se ele já existisse. A partir daí, deriva-se o FAQ interno/externo, e só então começa-se a pensar em arquitetura e execução.

### 3.2 Por que aplicamos

1. **Força clareza do outcome** — se o press release é confuso, o produto é confuso.
2. **Descobre gaps de valor cedo** — se o FAQ não responde por que alguém pagaria, o produto não tem caso de uso.
3. **Alinha stakeholders** — todo mundo lê o mesmo documento canônico.
4. **Previne sobre-engenharia** — especificamos o que entrega valor, não o que é tecnicamente interessante.

### 3.3 Como aplicamos

| Ordem | Artefato | Resposta que ele dá |
|---|---|---|
| 1 | PR/FAQ (Nível 1) | "Que produto é este? Por que existe? Quem usa?" |
| 2 | Personas, JTBD (Nível 1) | "Quem especificamente? Que *job* ele contrata o produto pra fazer?" |
| 3 | Success Metrics (Nível 1) | "Como sabemos que funcionou?" |
| 4 | Capabilities (Nível 2) | "O que exatamente o produto faz?" |
| 5 | Quality Attributes + Invariants (Nível 2) | "Sob quais restrições?" |
| 6 | Arquitetura + ADRs (Nível 3) | "Como é construído?" |
| 7 | Sprint Contracts (Nível 4) | "Em que ordem entregamos?" |
| 8 | Quality Framework (Nível 5) | "Como garantimos o rigor durante a construção?" |

### 3.4 Regras operacionais da disciplina

- **REG-WB-001**: PR/FAQ **DEVE** ser o primeiro artefato de conteúdo produzido (após este framework).
- **REG-WB-002**: PR/FAQ **DEVE** assumir produto lançado em sua forma completa (não MVP).
- **REG-WB-003**: Nenhum artefato de Nível N **PODE** ser iniciado antes de Nível N-1 estar *frozen*.
- **REG-WB-004**: Se ao escrever Nível N percebemos gap em Nível N-1, **DEVEMOS** *thaw* N-1, corrigir, re-*freeze*, e então continuar N.

---

## 4. Hierarquia dos 5 Níveis

### 4.1 Visão geral

```
┌──────────────────────────────────────────────┐
│  Nível 1 — Visão (Produto)                   │
│  • PR/FAQ  • Personas  • JTBD  • Metrics     │
└─────────────────┬────────────────────────────┘
                  │ deriva
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 2 — Especificação Funcional            │
│  • Capabilities  • Quality Attrs (NFRs)      │
│  • Invariants    • Constraints               │
│  • User Journeys                              │
└─────────────────┬────────────────────────────┘
                  │ deriva
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 3 — Arquitetura                        │
│  • ADRs  • C4 (Context/Container/Component)  │
│  • Data Model  • Protocols  • Failure Modes  │
└─────────────────┬────────────────────────────┘
                  │ deriva
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 4 — Sprint Contracts (inviolável)     │
│  • S-00, S-01, ..., S-NN                      │
│  cada um: intent, DoR, WI, DoD, gates,       │
│  invariants, anti-scope, sign-off            │
└─────────────────┬────────────────────────────┘
                  │ informa
                  ▼
┌──────────────────────────────────────────────┐
│  Nível 5 — Framework de Qualidade             │
│  • DoR padrão  • DoD padrão  • Quality Gates │
│  • Test Strategy  • Completeness Criteria    │
└──────────────────────────────────────────────┘
```

Nível 5 é transversal: **informa** Níveis 2, 3 e 4, mas é escrito por último (após Nível 3 *frozen*) para refletir a realidade do produto.

### 4.2 Nível 1 — Visão (Produto)

#### Artefatos

| Arquivo | Conteúdo | Tamanho esperado |
|---|---|---|
| `01_vision/prfaq.md` | Press Release + Internal FAQ + External FAQ | 2.000–4.000 linhas |
| `01_vision/personas.md` | Tipos de usuários do produto completo, em formato formal | 500–1.200 linhas |
| `01_vision/jobs_to_be_done.md` | Matriz JTBD por persona, com prioridade e frequência | 600–1.500 linhas |
| `01_vision/success_metrics.md` | North Star + *guardrail metrics* + *counter metrics* | 400–800 linhas |

#### Propósito agregado

Responde: **O que é o produto, para quem, por que, como medimos sucesso.**

#### Critérios de *freeze*

- [ ] PR/FAQ cobre **produto completo** (todos os protocolos, tiers, interfaces).
- [ ] Cada persona tem pelo menos 3 *Jobs To Be Done* mapeados.
- [ ] Toda métrica de sucesso tem: definição operacional, *baseline*, alvo, janela de medição.
- [ ] FAQ inclui perguntas "chatas" de stakeholders críticos (security, finance, ops, legal).
- [ ] Nenhuma seção contém "TBD", "provavelmente", "considerando".

### 4.3 Nível 2 — Especificação Funcional

#### Artefatos

| Arquivo | Conteúdo |
|---|---|
| `02_product/capabilities.md` | Capacidades numeradas CAP-001…CAP-N, cada uma com: intent, entradas, saídas, precondições, poscondições, acceptance, verificação |
| `02_product/quality_attributes.md` | NFRs (SLOs, limites, *budgets*) numerados NFR-001…NFR-N |
| `02_product/invariants.md` | Propriedades *safety* + *liveness* numeradas INV-001…INV-N |
| `02_product/constraints.md` | Anti-scope explícito (o que **NÃO** fazemos, e por quê) |
| `02_product/user_journeys/` | Uma jornada por arquivo; máquinas de estado formais |

#### Propósito agregado

Responde: **O que exatamente o produto faz, sob quais restrições, com quais garantias.**

#### Critérios de *freeze*

- [ ] Toda *capability* mapeada para pelo menos uma persona em Nível 1.
- [ ] Toda NFR tem: métrica, *baseline*, alvo, janela, consequência de violação.
- [ ] Todo invariante tem: formalização, método de verificação, *blast radius* se violado.
- [ ] *Constraints* contém anti-scope explícito com *rationale* pra cada item.
- [ ] *User journeys* incluem: *happy path*, *error paths* principais, estados terminais.

### 4.4 Nível 3 — Arquitetura

#### Artefatos

| Arquivo | Conteúdo |
|---|---|
| `03_architecture/adrs/ADR-XXXX-*.md` | Uma decisão arquitetural por arquivo |
| `03_architecture/c4_context.md` | C4 nível 1: sistema e atores externos |
| `03_architecture/c4_containers.md` | C4 nível 2: Workers, Containers, R2, DB, KV, DO |
| `03_architecture/c4_components.md` | C4 nível 3: módulos internos por container |
| `03_architecture/data_model.md` | Schemas + invariantes de dados |
| `03_architecture/protocols/*.md` | Uma spec por protocolo (REAPI, npm, PyPI, Cargo, OCI…) |
| `03_architecture/failure_modes.md` | FMEA (Failure Mode and Effects Analysis) |
| `03_architecture/security_model.md` | Threat model, *trust boundaries*, controles |

#### Propósito agregado

Responde: **Como o produto é construído — decisões, estrutura, protocolos, modos de falha, segurança.**

#### Critérios de *freeze*

- [ ] Todo CAP de Nível 2 mapeia para pelo menos um componente de `c4_components.md`.
- [ ] Toda decisão "não óbvia" tem ADR dedicado.
- [ ] Todo ADR registra pelo menos 2 alternativas consideradas.
- [ ] Todo ADR classifica reversibilidade (*one-way*, *two-way*, *hybrid*).
- [ ] `failure_modes.md` cobre cada par (componente, modo) com mitigação e detecção.
- [ ] `security_model.md` inclui *threat model* STRIDE completo.

### 4.5 Nível 4 — Sprint Contracts (invioláveis)

#### Artefatos

| Arquivo | Conteúdo |
|---|---|
| `04_sprints/_template.md` | Template canônico (versão frozen) |
| `04_sprints/S00_{nome}.md` | Sprint 0: *foundation* |
| `04_sprints/S01_{nome}.md` | Sprint 1 |
| `04_sprints/...` | ... |

#### Propósito agregado

Responde: **Em que ordem entregamos o produto completo, sob que contrato rigoroso.**

#### Critérios de *freeze* (por sprint)

Definidos formalmente no template (§10.1). Resumo:

- [ ] Todas 20 seções obrigatórias preenchidas.
- [ ] *Entry Criteria* binários e verificáveis.
- [ ] *Exit Criteria* cobrem: funcional, qualidade, performance, segurança, observability, docs, ops, compliance.
- [ ] *Anti-scope* explícito.
- [ ] *Traceability matrix* completa.
- [ ] Todos *sign-offs* atribuídos.

### 4.6 Nível 5 — Framework de Qualidade

#### Artefatos

| Arquivo | Conteúdo |
|---|---|
| `05_quality/definition_of_ready.md` | DoR padrão reusável |
| `05_quality/definition_of_done.md` | DoD padrão reusável |
| `05_quality/completeness_criteria.md` | Critérios por tipo de artefato |
| `05_quality/quality_gates.md` | *Gates* por categoria |
| `05_quality/test_strategy.md` | Camadas de teste, *coverage targets*, *mutation* |

#### Propósito agregado

Responde: **O que significa "feito" e "de qualidade" no CoreLink.**

---

## 5. Rastreabilidade (Traceability)

### 5.1 Cadeia de derivação

```
Persona ──► JTBD ──► CAP ──► { NFR, INV } ──► ADR ──► Componente C4
                                                            │
                                                            ▼
                                         Sprint WI ──► Código + Testes
                                                            │
                                                            ▼
                                              Observability (métrica, log, trace)
```

### 5.2 Matriz de rastreabilidade (obrigatória por sprint)

Cada sprint mantém tabela:

| CAP | INV | NFR | ADR | WI | Código | Testes | Métricas/Alertas |
|---|---|---|---|---|---|---|---|

### 5.3 Ferramenta de validação (CI)

O repositório **DEVE** incluir *script* que valida:

- Todo CAP referenciado em algum sprint.
- Toda INV referenciada em pelo menos um teste.
- Todo ADR *accepted* referenciado em pelo menos uma spec ou componente.
- Toda *code path* com anotação `// trace: CAP-XXX`.

---

## 6. Numeração e Identificadores Estáveis

### 6.1 Formatos

| Tipo | Formato | Regras |
|---|---|---|
| Persona | `PERSONA-XX` | 2 dígitos, começa em 01 |
| Job To Be Done | `JTBD-XXX` | 3 dígitos |
| Capability | `CAP-XXX` | 3 dígitos, agrupado por subsistema via prefixo opcional (ex: `CAP-CAS-001`) |
| Non-Functional Req | `NFR-XXX` | 3 dígitos |
| Invariant | `INV-XXX` | 3 dígitos |
| ADR | `ADR-XXXX` | 4 dígitos, zero-padded |
| Sprint | `S-XX` | 2 dígitos |
| Work Item (in sprint) | `WI-SXX-NNN` | inclui sprint parent |
| Risk | `R-XXX` | 3 dígitos (no sprint) |
| Failure Mode | `FM-XXX` | 3 dígitos |
| Success Metric | `METRIC-XXX` | 3 dígitos |

### 6.2 Regras absolutas

- **NUNCA** renumerar.
- **NUNCA** reusar ID descontinuado.
- Atribuição sequencial. Novo ID é sempre `max + 1`.
- *Placeholders* (`CAP-TBD-temperature`) são **proibidos** em documento não-*draft*.

### 6.3 Ciclo de status por artefato identificado

```
PROPOSED ──► ACCEPTED ──► (DEPRECATED) ──► (SUPERSEDED_BY another ID)
```

---

## 7. Frozen Flag System — Ciclo de Vida de Documentos

### 7.1 Estados válidos

| Estado | Semântica | Pode ser referenciado por downstream? |
|---|---|---|
| `DRAFT` | Em escrita ativa | ❌ |
| `REVIEW` | Submetido a revisão | ❌ |
| `FROZEN` | Aprovado e imutável | ✅ |
| `THAWED` | Frozen anterior reaberto para mudança; downstream auditado | ❌ (enquanto nesse estado) |
| `SUPERSEDED` | Substituído por versão nova | ❌ (referencie o superseder) |

### 7.2 Transições

```
DRAFT ──► REVIEW ──► FROZEN
             │
             └──► DRAFT (se rejeitado)

FROZEN ──► THAWED ──► REVIEW ──► FROZEN (novo versão)
FROZEN ──► SUPERSEDED (quando substituído)
```

### 7.3 Quem pode executar transições

| Transição | Autorizado |
|---|---|
| `DRAFT → REVIEW` | Autor do documento |
| `REVIEW → FROZEN` | Aprovador final (Gustavo) + revisores requeridos |
| `REVIEW → DRAFT` | Qualquer revisor com *dissent* |
| `FROZEN → THAWED` | Aprovador final (após justificativa escrita) |
| `THAWED → REVIEW` | Autor do documento |
| `FROZEN → SUPERSEDED` | Aprovador final (ao aprovar o superseder) |

### 7.4 Obrigações de *thaw*

Ao executar `FROZEN → THAWED`, obrigatoriamente:

1. Criar issue/ticket documentando razão.
2. Listar todos os downstream dependentes.
3. Marcar todos os downstream *frozen* como "audit-pending".
4. Re-validar consistência antes de re-*freeze*.

### 7.5 Marcação no documento

Todo documento **DEVE** começar com bloco:

```markdown
> **Status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED
> **Versão:** X.Y.Z
> **Última atualização:** YYYY-MM-DD
> **Owner:** Nome
> **Aprovador final:** Nome
> **Revisores:** Nome1, Nome2
```

---

## 8. RFC 2119 — Vocabulário Normativo

### 8.1 Palavras canônicas (EN + pt-BR equivalente)

Sempre em **MAIÚSCULAS** quando com significado normativo.

| EN | pt-BR | Semântica |
|---|---|---|
| MUST | **DEVE** | Obrigatório absoluto |
| MUST NOT | **NÃO DEVE** | Proibido absoluto |
| REQUIRED | **OBRIGATÓRIO** | Sinônimo de MUST |
| SHALL | **SERÁ** | Sinônimo de MUST (jurídico/especificação formal) |
| SHOULD | **DEVERIA** | Fortemente recomendado; exceções requerem justificativa |
| SHOULD NOT | **NÃO DEVERIA** | Fortemente desencorajado |
| RECOMMENDED | **RECOMENDADO** | Sinônimo de SHOULD |
| MAY | **PODE** | Opcional; implementadores escolhem |
| OPTIONAL | **OPCIONAL** | Sinônimo de MAY |

### 8.2 Exemplos aplicados

- ✅ "O servidor **DEVE** rejeitar requests com tokens expirados retornando HTTP 401."
- ✅ "O cliente **PODE** implementar *retry* com *exponential backoff*."
- ❌ "O servidor pode retornar erro 500 em casos de falha." — "pode" aqui é ambíguo (RFC 2119 ou permissão casual?). Reescreva.

### 8.3 Palavras proibidas em spec

| Palavra/Expressão | Motivo | Substituir por |
|---|---|---|
| "deveria ser rápido" | não mensurável | "DEVE servir P99 ≤ 50 ms" |
| "idealmente" | *wishful* | "DEVE" ou "DEVERIA" (escolher) |
| "talvez" | ambíguo | remover; tomar decisão |
| "provavelmente" | ambíguo | remover; quantificar |
| "etc." | incompleto | enumerar exaustivamente |
| "e outros" | incompleto | enumerar exaustivamente |
| "TBD" | placeholder | decidir ou mover para issue com ID |
| "basicamente" | *filler* | remover |
| "simplesmente" | *filler* | remover |

---

## 9. Política de Linguagem (pt-BR + EN)

### 9.1 Regras

- **REG-LANG-001**: Prosa corrida, seções, explicações conceituais: **pt-BR**.
- **REG-LANG-002**: Termos-da-arte da indústria: **EN** sem tradução. Exemplos: *hash*, *digest*, *idempotent*, *throughput*, *observability*, *tenant*, *content-addressable*, REAPI, gRPC.
- **REG-LANG-003**: Nomes de componentes, classes, serviços: **EN** (code tokens).
- **REG-LANG-004**: Mensagens de erro e logs emitidos pelo sistema: **EN** (padrão de indústria para infra).
- **REG-LANG-005**: Palavras RFC 2119 em MAIÚSCULAS em seu equivalente pt-BR ou EN, consistente **dentro do mesmo documento**.

### 9.2 Exemplos

✅ "O *tenant* isolamento é invariante **INV-042**. O servidor **DEVE** validar o *bearer token* e extrair `tenant_id` da *claim* `org_id` antes de acessar qualquer *blob* no R2."

❌ "O inquilino isolamento é invariante INV-042. O servidor deveria validar o token e pegar o id do inquilino da declaração `org_id`." — traduzir "tenant" e "token" empobrece; "deveria" é ambíguo.

---

## 10. Templates

### 10.1 Sprint Contract — ver `_templates/sprint_contract.md`

Template canônico com **20 seções obrigatórias** que definem o contrato inviolável. Nenhum sprint pode ser aceito sem preenchimento completo.

Resumo das seções:

1. Metadata
2. Executive Summary
3. Intent
4. Capability Mapping
5. Entry Criteria (DoR)
6. Work Items (decomposição atômica)
7. Exit Criteria (DoD) — multidimensional
8. Invariants (safety + liveness)
9. Test Strategy
10. Risk Register
11. Artifacts Produced
12. Anti-Scope
13. Dependencies
14. Observability Plan
15. Rollback Plan
16. Completeness Checklist (SOTA)
17. Traceability Matrix
18. Review Checkpoints
19. Sign-off
20. Retrospective + Change Log

### 10.2 Architecture Decision Record — ver `_templates/adr.md`

Template canônico com **15 seções** cobrindo: contexto, decisão, alternativas (com matriz quantitativa), consequências, reversibilidade, implementação, validação, compliance, custo, dependências, referências, glossário, change log.

### 10.3 Capability — a ser detalhado no Nível 2 *freeze*

Esqueleto mínimo:

```markdown
## CAP-XXX: {Título}

- **Status:** PROPOSED | ACCEPTED | DEPRECATED
- **Introduzido em:** versão X
- **Depende de:** CAP-YYY, CAP-ZZZ
- **Personas:** PERSONA-XX, PERSONA-YY
- **JTBDs atendidos:** JTBD-XXX
- **NFRs aplicáveis:** NFR-XXX
- **Invariantes relacionados:** INV-XXX

### Intent
{uma frase}

### Descrição
{parágrafo}

### Entradas
{especificação tipada}

### Saídas
{especificação tipada}

### Precondições
{lista}

### Poscondições
{lista}

### Critérios de Aceite
- Given ... When ... Then ...

### Método de Verificação
{teste, métrica, revisão humana}

### Observabilidade
- Métricas: {lista}
- Logs: {lista}
- Traces: {lista}
```

### 10.4 Invariant — a ser detalhado no Nível 2 *freeze*

```markdown
## INV-XXX: {Nome}

- **Tipo:** Safety | Liveness
- **Status:** ACTIVE | DEPRECATED

### Enunciado formal
{∀ x . P(x) → Q(x) ou equivalente linguagem-natural precisa}

### Enunciado em prosa
{1-2 sentenças claras}

### Escopo
{onde o invariante deve valer: componentes, fluxos, janelas temporais}

### Método de verificação
- Property-based test: {arquivo, função}
- Integration test: {arquivo, função}
- Runtime assertion: {se aplicável}

### Consequência de violação
- Severidade: CRITICAL | HIGH | MEDIUM | LOW
- Blast radius: {escopo}
- Mitigação: {procedimento}
```

---

## 11. Diagramas

### 11.1 Notação obrigatória

| Tipo de diagrama | Notação |
|---|---|
| Arquitetura (níveis C4) | **C4 Model** (Simon Brown) em Mermaid ou Structurizr |
| Sequência / fluxo de request | **Mermaid `sequenceDiagram`** |
| Máquina de estado | **Mermaid `stateDiagram-v2`** |
| Modelo de dados | **Mermaid `erDiagram`** |
| Grafo de dependências | **Mermaid `graph`** |
| Threat model | **DFD** (Data Flow Diagram) ou STRIDE table |

### 11.2 Regras

- **REG-DIAG-001**: Diagramas **DEVEM** ser texto-fonte (não imagens binárias) para versionamento e diff.
- **REG-DIAG-002**: Diagrama **DEVE** ser acompanhado de legenda textual equivalente para acessibilidade.
- **REG-DIAG-003**: Diagrama **NÃO DEVE** introduzir informação que não esteja também em prosa estruturada no mesmo documento.

---

## 12. Processo de Revisão e Sign-off

### 12.1 Papéis de revisão

| Papel | Responsabilidade |
|---|---|
| **Autor** | Escreve o documento; responde a comentários |
| **Revisor Técnico** | Valida rigor técnico, rastreabilidade, testabilidade |
| **Revisor de Produto** | Valida alinhamento com visão, JTBDs, personas |
| **Revisor de Segurança** | Valida *threat model*, controles, compliance |
| **Revisor de Operações** | Valida observability, runbooks, reversibilidade |
| **Aprovador Final** | Autoriza `REVIEW → FROZEN` |

### 12.2 SLAs de revisão

| Tipo de documento | SLA padrão |
|---|---|
| ADR | 48 horas |
| Capability | 72 horas |
| Sprint Contract | 96 horas |
| PR/FAQ | 1 semana |
| Quality Framework (Nível 5) | 2 semanas |

### 12.3 Critério de aprovação

Um documento **PODE** ser *frozen* se:

- [ ] Todos revisores requeridos deram ✅.
- [ ] Todos comentários `MUST_FIX` resolvidos.
- [ ] Aprovador final assinou explicitamente.
- [ ] Critérios de *freeze* específicos do nível foram satisfeitos.

Comentários classificados em:

- `MUST_FIX` — bloqueia *freeze*.
- `SHOULD_FIX` — não bloqueia mas registra dívida.
- `NIT` — melhoria estilística opcional.
- `QUESTION` — pede esclarecimento.
- `PRAISE` — reconhece ponto forte (não bloqueia).

### 12.4 Dissent protocol

Se revisor emite *dissent* irreconciliável:

1. Discussão escrita de no mínimo 24h no documento.
2. Se não resolvido, escalação ao Aprovador Final.
3. Aprovador Final decide, com justificativa escrita no *change log*.
4. *Dissent* minoritário é preservado como seção no próprio documento (`### Apêndice: Dissent`).

---

## 13. Tooling e CI

### 13.1 Linters obrigatórios

| Ferramenta | Propósito |
|---|---|
| `markdownlint` | Sintaxe markdown consistente |
| `vale` | *Prose linting* (estilo, banned words) |
| `lychee` ou `markdown-link-check` | Verificação de links quebrados |
| Custom trace checker (Python/Rust) | Valida rastreabilidade CAP↔WI↔teste↔código |

### 13.2 CI mandatório (GitHub Actions)

Sobre cada PR em `specs/`:

- [ ] Markdownlint: zero erros.
- [ ] Vale: zero `error` (warnings permitidos).
- [ ] Link checker: 100% links válidos.
- [ ] Trace checker: toda referência a CAP-XXX, INV-XXX, ADR-XXXX, S-XX existe.
- [ ] Frozen-check: documentos `FROZEN` não mudaram sem *thaw* registrado.
- [ ] Status-check: todo documento tem bloco de metadata válido.

### 13.3 PR discipline

- **REG-PR-001**: PR que altera doc `FROZEN` **DEVE** ter label `thaw` e link para justificativa.
- **REG-PR-002**: PR que cria doc novo **DEVE** incluir: IDs atribuídos, pai (upstream), status inicial (`DRAFT`).
- **REG-PR-003**: PR que altera numeração **DEVE** ser rejeitado (violação de PRINC-006).

---

## 14. Anti-padrões

### AP-001 — Spec como "whiteboard ao vivo"

❌ Specs que mudam toda semana sem versionamento, sem *freeze*, sem *change log*.

✅ Specs versionadas, com *freeze*, com auditoria de mudança.

### AP-002 — "Vamos decidir isso durante a implementação"

❌ Deixar decisões arquiteturais para o sprint.

✅ Decidir em ADR (Nível 3) **antes** do sprint começar.

### AP-003 — Capability vaga

❌ "O sistema deve permitir o usuário gerenciar cache."

✅ "CAP-042: O sistema **DEVE** expor `DELETE /v1/tenants/{id}/cache` que deleta todos *blobs* do *tenant* de forma idempotente, retornando 204 em ≤ 30s p99, e emitindo evento `tenant.cache.purged` para o event bus."

### AP-004 — Teste como documentação

❌ Afirmar que o teste de integração "é" a spec.

✅ Spec é documento prosa + RFC 2119 + invariantes. Testes **verificam** a spec mas não a substituem.

### AP-005 — "Isso é detalhe de implementação"

❌ Esconder decisões arquiteturais importantes sob o rótulo de "detalhe".

✅ Se altera invariantes, NFRs ou trade-offs relevantes, é **decisão arquitetural** e merece ADR.

### AP-006 — Sprint que nunca termina

❌ Sprint com "últimos 5% pendentes" que se estende indefinidamente.

✅ Sprint tem *exit criteria* binários. Não cumpriu? Sprint **não passa**. Decomponha ou assuma falha e aprenda.

### AP-007 — Cerimônia sem rigor

❌ Muito documento com pouco conteúdo verificável.

✅ Cada seção tem propósito. Se a seção não afeta alguma decisão, teste ou comportamento, ela não pertence ao documento.

### AP-008 — Premature optimization da spec

❌ Detalhar *ad nauseam* áreas que não serão construídas em 6+ meses.

✅ Granularidade proporcional à proximidade de execução, **mas** com conjunto completo garantindo que nada essencial fique fora.

### AP-009 — Falta de anti-scope

❌ Documento lista o que faz mas não o que **não** faz.

✅ Anti-scope explícito previne *scope creep* e *feature bloat*.

---

## 15. Glossário

Termos canônicos do CoreLink. Uso consistente em todas as specs.

| Termo | Definição |
|---|---|
| **CoreLink** | Nome do produto: shared content-addressable cache for developers. |
| **Tenant** | Entidade isolada (usuário individual ou organização) que consome CoreLink. |
| **CAS** | *Content-Addressable Storage*: armazenamento indexado por hash do conteúdo. |
| **AC** | *Action Cache*: mapeamento de hash de ação → hash de resultado (REAPI). |
| **Digest** | Hash criptográfico identificador de um *blob* (tipicamente SHA-256 ou BLAKE3). |
| **Manifest** | *Blob* especial contendo metadata de um *blob* decomposto em chunks Merkle. |
| **Chunk** | Bloco de tamanho fixo (ex: 2 MiB) resultante da decomposição de *blob* grande. |
| **Dedup** | *Deduplication*: reaproveitamento de *blob/chunk* idêntico entre múltiplas referências. |
| **REAPI** | *Remote Execution API* v2, spec do Bazel: [github.com/bazelbuild/remote-apis](https://github.com/bazelbuild/remote-apis). |
| **Sprint Contract** | Documento formal e inviolável de Nível 4 que governa um sprint. |
| **DoR** | *Definition of Ready*: critérios de entrada de sprint. |
| **DoD** | *Definition of Done*: critérios de saída de sprint. |
| **Invariant (Safety)** | Propriedade "nada ruim acontece": ∀ estado, P(estado) é verdade. |
| **Invariant (Liveness)** | Propriedade "algo bom eventualmente acontece": ∃ momento futuro em que P(estado) será verdade. |
| **One-way door** | Decisão custosa ou impossível de reverter (Bezos framework). |
| **Two-way door** | Decisão facilmente reversível. |
| **FMEA** | *Failure Mode and Effects Analysis*. |
| **SLO** | *Service Level Objective*. |
| **SLI** | *Service Level Indicator*. |
| **Trace** | (1) *Distributed trace* em observability; (2) referência bidirecional entre artefatos de spec. |
| **Freeze** | Transição de documento para estado imutável `FROZEN`. |
| **Thaw** | Reabertura de documento `FROZEN` para edição (processo controlado). |

---

## 16. Meta-regras (Evolução deste Framework)

### 16.1 Como este framework evolui

Este próprio documento segue as regras que define:

- Tem *status*, *versão*, *change log*, *sign-off*.
- Mudanças passam por `DRAFT → REVIEW → FROZEN`.
- Bumping de **versão maior** (X.Y.Z, X) requer auditoria de todos os documentos downstream.
- Bumping de **versão menor** (X.Y) permitida sem auditoria obrigatória, mas com *change log*.
- **Patch** (X.Y.Z) para correções tipográficas e esclarecimentos que **NÃO** mudam semântica.

### 16.2 Semver para specs

| Bump | Critério |
|---|---|
| Major (X.0.0) | Mudança em princípio fundamental, regra inviolável ou template obrigatório |
| Minor (X.Y.0) | Adição de regra, template, seção, ou refinamento sem quebrar downstream |
| Patch (X.Y.Z) | Correção tipográfica, clarificação que não muda semântica |

### 16.3 Exceções

Em casos excepcionais em que regra deste framework impede resolução de problema real, o processo é:

1. Proposição formal de mudança (PR com label `framework-change`).
2. Revisão obrigatória pelo Aprovador Final.
3. Aceitação **apenas se** alternativas foram exauridas.
4. Documentação em `### 16.4 Exceções Históricas` abaixo.

### 16.4 Exceções Históricas

*Nenhuma no momento.*

---

## 17. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo Schneiter (via Claude Opus 4.7) | Versão inicial do framework |

---

## 18. Revisão

### Revisores requeridos para *freeze* deste documento

- [ ] **Gustavo Schneiter** (Aprovador Final) — ____________ YYYY-MM-DD

### Comentários de revisão

*(a ser preenchido durante revisão)*

---

**Fim do Framework 00.**
