# ADR-XXXX: {{Título declarativo e conciso}}

> **Template Version:** 1.1.0

```yaml
---
id: ADR-XXXX
type: adr
doc_status: DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
version: 1.0.0
created: YYYY-MM-DD
updated: YYYY-MM-DD
owner: {{Nome}}
final_approver: {{Nome}}
reviewers:
  - {{Nome}}
supersedes: null
superseded_by: null
tags: [{{área}}, {{tecnologia}}]
---
```

> **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
> **Versão:** 1.0.0
> **Última atualização:** YYYY-MM-DD
> **Owner:** {{Nome}}
> **Aprovador Final:** {{Nome}}
> **Revisores:** {{Nome1, Nome2}}
> **Supersedes:** {{ADR-YYYY | nenhum}}
> **Superseded By:** {{ADR-ZZZZ | nenhum}}
>
> *Nota histórica: literatura clássica de ADR usa "PROPOSED → ACCEPTED → DEPRECATED → SUPERSEDED". Mapping canônico CoreLink: `PROPOSED`=`DRAFT/REVIEW`, `ACCEPTED`=`FROZEN`, `DEPRECATED`=`DEPRECATED`, `SUPERSEDED`=`SUPERSEDED`. Ver `00_framework.md §7.4`.*

---

## 0. Metadata

| Field | Value |
|---|---|
| **ID** | ADR-XXXX |
| **Título** | {{título}} |
| **Versão do ADR** | 1.0.0 |
| **Status** | PROPOSED |
| **Data Proposed** | YYYY-MM-DD |
| **Data Accepted** | YYYY-MM-DD |
| **Data Deprecated / Superseded** | YYYY-MM-DD (se aplicável) |
| **Autores** | {{Nome <email>}} |
| **Deciders (quem autoriza)** | {{Nome, Nome}} |
| **Stakeholders consultados** | {{Nome, Nome, Nome}} |
| **Supersedes (ADR que este substitui)** | ADR-YYYY (se aplicável) |
| **Superseded By** | ADR-ZZZZ (se este foi substituído) |
| **ADRs relacionados (não supersede)** | ADR-AAAA, ADR-BBBB |
| **Capabilities afetadas** | CAP-XXX, CAP-YYY |
| **Invariantes afetados** | INV-XXX, INV-YYY |
| **NFRs afetados** | NFR-XXX |
| **Tags** | {{área}}, {{tecnologia}}, {{subsistema}}, {{maturidade}} |
| **Reversibilidade** | ONE_WAY_DOOR \| TWO_WAY_DOOR \| HYBRID |
| **Escopo de impacto** | LOCAL \| SUBSYSTEM \| SYSTEM \| PRODUCT \| ORGANIZATION |

---

## Sumário

1. [Executive Summary](#1-executive-summary)
2. [Context](#2-context)
3. [Decision](#3-decision)
4. [Alternatives Considered](#4-alternatives-considered)
5. [Decision Matrix (quantitativo)](#5-decision-matrix-quantitativo)
6. [Consequences](#6-consequences)
7. [Reversibility Assessment](#7-reversibility-assessment)
8. [Implementation Notes](#8-implementation-notes)
9. [Validation Plan](#9-validation-plan)
10. [Compliance & Regulatory Implications](#10-compliance--regulatory-implications)
11. [Cost Implications](#11-cost-implications)
12. [Dependencies](#12-dependencies)
13. [References](#13-references)
14. [Glossary](#14-glossary)
15. [Change Log](#15-change-log)

---

## 1. Executive Summary

> *(3 a 5 sentenças. Pode ser lido isolado. Responde: o quê foi decidido, por quê, e qual consequência mais importante.)*

**Exemplo:**
> Adotamos Rust como linguagem de implementação do servidor CoreLink devido à necessidade de memory-safety sem GC e performance em hot-path de serialização/hashing. Alternativas consideradas: Go, Zig, C++. Rust foi escolhido pela combinação de maturidade do ecossistema gRPC (tonic), garantias de safety, e alinhamento com concorrentes SOTA (NativeLink é Rust). Principal consequência negativa: learning curve maior que Go; mitigada pela disponibilidade de tooling AI e documentação extensa.

---

## 2. Context

### 2.1 Problem Statement

> *(Formulação crisp do problema. O que estamos tentando resolver?)*

### 2.2 Current State

> *(Como as coisas funcionam hoje. Se greenfield, escrever "N/A (greenfield)".)*

### 2.3 Forces at Play

> *(Concerns/pressões competitivas que moldam a decisão. Lamport-style: enumerar explicitamente.)*

- **Force 1:** {{descrição}}. Pressão: {{quantificação se possível}}.
- **Force 2:** {{descrição}}. Pressão: {{quantificação}}.
- **Force 3:** {{descrição}}. Pressão: {{quantificação}}.

### 2.4 Constraints (Hard)

> *(Restrições não-negociáveis. O motivo de imovibilidade explicitado.)*

- **C1:** {{constraint}}. Motivo de imovibilidade: {{razão}}.
- **C2:** {{constraint}}. Motivo: {{razão}}.

### 2.5 Assumptions

> *(Premissas que fundamentam a decisão. Cada uma marcada como validada ou a validar.)*

- **A1:** {{premissa}}. Validação: ✅ {{evidência/link}} \| ⏳ a validar por {{método}}.
- **A2:** {{premissa}}. Validação: ✅ \| ⏳.

### 2.6 Out of Scope

> *(O que este ADR explicitamente NÃO aborda, para evitar scope creep do debate.)*

- {{fora do escopo}}
- {{fora do escopo}}

### 2.7 Stakeholders afetados

| Stakeholder | Interesse | Consultado? |
|---|---|---|
| {{Time X}} | {{impacto}} | ✅ / ❌ |

---

## 3. Decision

### 3.1 Statement

> *(Declarativa, assertiva. "We will...". Sem hedging.)*

### 3.2 Rationale (resumo)

> *(Por que esta opção, em 3-5 sentenças. Detalhes em §4 e §5.)*

### 3.3 Scope da Decisão

> *(A decisão vale para: todo o sistema? subsistema X? até quando? até qual escala?)*

---

## 4. Alternatives Considered

> Mínimo 2 alternativas (uma pode ser "status quo"). Profundidade proporcional à proximidade de seleção.

### Alternative A: {{nome}}

#### A.1 Descrição
> {{O que é, breve mas preciso.}}

#### A.2 Pros
- {{pro}}
- {{pro}}
- {{pro}}

#### A.3 Cons
- {{con}}
- {{con}}
- {{con}}

#### A.4 Por que rejeitada
> {{A razão decisiva. Fact-based, não "achamos que".}}

#### A.5 Evidência consultada
- {{link benchmark / paper / case study / documento interno}}
- {{link}}

#### A.6 Condições sob as quais A seria reconsiderada
> {{Triggers para review. Ex: "se custo operacional de Rust ficar >3× Go, reconsiderar."}}

---

### Alternative B: {{nome}}

> *(mesma estrutura que A)*

---

### Alternative C: {{nome}}

> *(mesma estrutura que A)*

---

### Status Quo (se aplicável)

#### Descrição
> {{O que teria acontecido se não tomássemos decisão.}}

#### Por que não é aceitável
> {{Razões explícitas.}}

---

### Alternativas descartadas por triagem inicial

> *(Opções menos sérias; 1-2 linhas cada mostrando que foram consideradas e por que foram eliminadas rapidamente.)*

- **{{Opção}}**: descartada porque {{razão breve}}.
- **{{Opção}}**: descartada porque {{razão breve}}.

---

## 5. Decision Matrix (quantitativo)

> Comparação ponderada das alternativas da shortlist (decisão + A + B + C, etc.).

### 5.1 Critérios e Pesos

Pesos somam 1.0. Justificativa de pesos documentada.

| Critério | Peso | Justificativa |
|---|---|---|
| Performance | 0.25 | CoreLink é latency-sensitive |
| Ecosystem maturity | 0.20 | Estamos construindo sobre libs do ecossistema |
| Operational burden | 0.15 | Time pequeno |
| Hiring pool | 0.10 | Crescimento futuro |
| Cost | 0.10 | Margem apertada |
| Reversibility | 0.10 | Queremos manter otimalidade |
| Strategic alignment | 0.10 | Brand/marketing halo |

### 5.2 Pontuação

Escala 1–10 (10 = excelente).

| Critério | Peso | {{Selecionado}} | {{Alt A}} | {{Alt B}} | {{Alt C}} |
|---|---|---|---|---|---|
| Performance | 0.25 | 9 | 7 | 8 | 6 |
| Ecosystem maturity | 0.20 | 8 | 9 | 5 | 7 |
| Operational burden | 0.15 | 7 | 8 | 6 | 8 |
| Hiring pool | 0.10 | 6 | 9 | 5 | 8 |
| Cost | 0.10 | 7 | 8 | 6 | 7 |
| Reversibility | 0.10 | 6 | 7 | 5 | 8 |
| Strategic alignment | 0.10 | 9 | 6 | 7 | 5 |
| **Weighted Total** | 1.00 | **{{X.XX}}** | {{X.XX}} | {{X.XX}} | {{X.XX}} |

### 5.3 Justificativa de cada score

> *(Para decisões grandes. Cada célula da tabela merece uma frase explicando a nota.)*

#### {{Selecionado}} — Performance = 9
> {{justificativa com referência}}

*(repetir para células que mereçam contexto)*

### 5.4 Análise de sensibilidade

> *(Como o ranking muda se variarmos pesos? Quais combinações de pesos fariam a decisão mudar?)*

- Se Performance peso ↓ de 0.25 para 0.15, ranking: {{...}}
- Se Cost peso ↑ de 0.10 para 0.30, ranking: {{...}}
- **Decisão robusta?** {{sim/não, justificativa}}

---

## 6. Consequences

### 6.1 Positivas

- {{consequência}}
- {{consequência}}

### 6.2 Negativas

- {{consequência}}
- {{consequência}}

### 6.3 Neutras / Trade-offs assumidos

- {{trade-off}}: aceitamos {{X}} em troca de {{Y}}.

### 6.4 Riscos introduzidos

| ID | Risco | Probabilidade | Impacto | Score (P×I) | Mitigação | Contingência |
|---|---|---|---|---|---|---|
| R1 | {{risco}} | L\|M\|H | L\|M\|H | 1–9 | {{mitigação}} | {{contingência}} |

### 6.5 Dívida técnica/operacional assumida

- **Dívida X:** {{descrição}}. Plano de pagamento: {{quando/como}}.
- **Dívida Y:** {{descrição}}. Plano: {{quando/como}}.

### 6.6 Impacto em CAPs / INVs / NFRs

| ID | Impacto |
|---|---|
| CAP-XXX | Habilita / Altera / Bloqueia |
| INV-YYY | Altera método de verificação |
| NFR-ZZZ | Threshold muda de X para Y |

---

## 7. Reversibility Assessment

### 7.1 Door Type

- [ ] **Two-way door** — facilmente reversível, baixo custo de mudar de ideia.
- [ ] **One-way door** — custo alto ou impossível reverter (Bezos framework).
- [ ] **Hybrid** — reversível dentro de janela T; após T, torna-se one-way.

**Janela de reversibilidade (se hybrid):** {{duração}}

### 7.2 Cost of Reversal

> Quantificação explícita:

| Dimensão | Custo estimado |
|---|---|
| Engineer-weeks | {{N}} |
| Financeiro (infra, migrations, retrabalho) | {{R$ / US$}} |
| Customer impact (downtime, breaking changes) | {{descrição}} |
| Opportunity cost (o que deixa de ser feito) | {{descrição}} |

### 7.3 Procedimento de reversão

> Passos ordenados para reverter a decisão, se necessário:

1. {{passo}}
2. {{passo}}
3. {{passo}}

Se não-reversível, declarar: **"Esta decisão não é reversível após {{evento/prazo}}; razão: {{explicação}}."**

### 7.4 Point of No Return (se aplicável)

> *(Evento/momento após o qual reversão deixa de ser viável.)*

- **Evento trigger:** {{descrição}}
- **Data prevista:** {{YYYY-MM-DD}}
- **Flag de alerta:** {{sinal que deveria nos fazer reverter ANTES do PNR}}

---

## 8. Implementation Notes

### 8.1 Escopo da mudança

- **Componentes afetados:** {{lista}}
- **Componentes novos:** {{lista}}
- **Componentes modificados:** {{lista}}
- **Componentes deprecated:** {{lista}}

### 8.2 Migration Path (se substituindo algo existente)

1. **Fase 1 — Expand:** {{o que acontece}}
2. **Fase 2 — Migrate:** {{o que acontece}}
3. **Fase 3 — Contract:** {{o que acontece}}

### 8.3 Rollout faseado

| Fase | Escopo | Trigger pra próxima fase |
|---|---|---|
| Fase 1 | {{escopo limitado}} | {{métrica/evento}} |
| Fase 2 | {{expansão}} | {{métrica/evento}} |
| Fase 3 | {{GA}} | — |

### 8.4 Feature flags (se aplicável)

| Flag | Default inicial | Default final | Remoção planejada |
|---|---|---|---|
| `feature.xxx` | false | true | Após 2 meses estável |

### 8.5 Breaking changes introduzidas

> *(Para usuários externos / integrações.)*

- {{breaking change}} — mitigação: {{...}}

### 8.6 Training / knowledge transfer

> *(Se decisão muda stack/processo, como time internaliza.)*

- {{ação}}: {{responsável}}, {{prazo}}.

---

## 9. Validation Plan

### 9.1 Success Criteria

> Como sabemos que esta decisão foi **correta**? Métricas específicas.

| Métrica | Baseline | Alvo | Janela | Como medimos |
|---|---|---|---|---|
| {{métrica}} | {{valor atual}} | {{valor desejado}} | {{30d / 90d}} | {{fonte}} |

### 9.2 Failure Signals

> Que sinais indicariam que erramos?

- **Sinal 1:** {{sintoma mensurável}}. Ação: {{o que fazer}}.
- **Sinal 2:** {{sintoma}}. Ação: {{o que fazer}}.

### 9.3 Review Cadence

- **Revisão formal programada:** {{YYYY-MM-DD, ex: 6 meses pós-accepted}}
- **Revisão automática por trigger:** {{sinal}}

### 9.4 Review Triggers (eventos que forçam revisão imediata)

- {{evento}} (ex: "vendor aumenta preço >20%")
- {{evento}} (ex: "surgir alternativa com delta de perf >30%")
- {{evento}} (ex: "violação grave de invariante relacionada")

### 9.5 Pré-condição de aceitação (gating)

> *(Se decisão só é oficialmente aceita após validação em ambiente piloto.)*

- [ ] Piloto executado em {{escopo}} por {{duração}}
- [ ] Métricas de §9.1 atingiram alvo
- [ ] Sem incidentes bloqueantes

---

## 10. Compliance & Regulatory Implications

### 10.1 Data Protection

| Regulação | Aplicável? | Impacto desta decisão | Ação requerida |
|---|---|---|---|
| GDPR (EU) | sim/não | {{impacto}} | {{ação}} |
| LGPD (BR) | sim/não | {{impacto}} | {{ação}} |
| HIPAA (US health) | sim/não/N-A | {{impacto}} | {{ação}} |
| CCPA (CA) | sim/não | {{impacto}} | {{ação}} |

### 10.2 Auditabilidade

> Esta decisão introduz novo audit trail? Muda retenção de logs? Afeta imutabilidade?

- {{consequência}}

### 10.3 Certificações afetadas

| Certificação | Aplicável | Impacto |
|---|---|---|
| SOC 2 Type II | {{sim/trabalhando para/não}} | {{impacto}} |
| ISO 27001 | {{sim/não}} | {{impacto}} |

### 10.4 Revisões requeridas

- [ ] **Security Review** (se relevante) — {{Nome, YYYY-MM-DD}}
- [ ] **Legal Review** (se relevante) — {{Nome, YYYY-MM-DD}}
- [ ] **Privacy Review** (se nova categoria de dado) — {{Nome, YYYY-MM-DD}}

---

## 11. Cost Implications

### 11.1 Financeiro — Infraestrutura

| Item de custo | Delta mensal | Delta anual | Notas |
|---|---|---|---|
| {{item}} | {{+/- R$X}} | {{+/- R$Y}} | {{base de cálculo}} |

### 11.2 Operacional

| Item | Impacto |
|---|---|
| Headcount | {{delta FTE}} |
| Training | {{horas}} |
| Tooling licenses | {{delta}} |
| On-call burden | {{delta}} |

### 11.3 Desenvolvimento

| Métrica | Baseline | Pós-decisão | Delta |
|---|---|---|---|
| Velocity (story points/sprint) | {{X}} | {{Y}} | {{±%}} |
| Cycle time | {{X}} | {{Y}} | {{±%}} |
| Complexity delta (estimativa) | — | {{aumento/redução}} | — |

### 11.4 Opportunity Cost

> O que deixamos de fazer por escolher isto?

- {{projeto/feature alternativa}}
- {{projeto/feature alternativa}}

### 11.5 TCO (Total Cost of Ownership) a 3 anos

> Agregação quando decisão é estratégica.

| Ano | Custo total estimado |
|---|---|
| Ano 1 | {{R$}} |
| Ano 2 | {{R$}} |
| Ano 3 | {{R$}} |

---

## 12. Dependencies

### 12.1 Upstream (depende de)

| Dependência | Tipo | Status |
|---|---|---|
| ADR-YYYY | decisão | ACCEPTED |
| CAP-XXX | capability | FROZEN |
| Vendor X API vX.Y | externa | disponível |

### 12.2 Downstream (habilita / bloqueia)

| Dependente | Relação |
|---|---|
| ADR-ZZZZ | este é pré-requisito |
| Sprint S-XX | este sprint requer esta decisão aceita |
| CAP-YYY | implementação desbloqueada |

### 12.3 External dependencies

| Dependência | SLA/Disponibilidade | Vendor |
|---|---|---|
| {{API/serviço}} | {{SLA}} | {{vendor}} |

---

## 13. References

### 13.1 Standards / Specs

- {{link}} — {{o que é e por que relevante}}
- {{link}}

### 13.2 Benchmarks / Papers acadêmicos

- {{link}} — {{o que mostra}}

### 13.3 Case Studies

- {{link}} — {{quem aplicou, resultado}}

### 13.4 Prior art (similar decisions elsewhere)

- {{link}} — {{contexto}}

### 13.5 Internal context

- {{link de thread, memo, meeting notes, ticket}}
- {{link de outras specs relacionadas}}

### 13.6 Tooling / Reference implementations

- {{link}} — {{papel na decisão}}

---

## 14. Glossary

> Termos específicos deste ADR que podem não estar no glossário master (`00_framework.md §40`).

| Termo | Definição |
|---|---|
| {{termo}} | {{definição precisa}} |

---

## 15. Change Log

| Versão | Data | Autor | Estado | Mudança |
|---|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | PROPOSED | Versão inicial |
| 1.0.1 | YYYY-MM-DD | {{Nome}} | PROPOSED | Incorpora feedback de revisor X |
| 1.1.0 | YYYY-MM-DD | {{Nome}} | ACCEPTED | Status após sign-off do Aprovador Final |
| 2.0.0 | YYYY-MM-DD | {{Nome}} | DEPRECATED | Substituída por ADR-ZZZZ |

---

## Apêndice A: Dissents (se houver)

> *(Discordâncias minoritárias registradas para a história.)*

### A.1 Dissent de {{Nome}}

> {{Posição oposta, com rationale}}

**Resolução pelo Aprovador Final:** {{como foi decidido}}

---

**Fim de ADR-XXXX.**
