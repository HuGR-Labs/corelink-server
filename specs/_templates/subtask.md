# Sub-task — ST-NNN: {{Título}}

> **Template Version:** 1.0.0
> **Status:** TODO | DOING | REVIEW | DONE | BLOCKED
> **Última atualização:** YYYY-MM-DD
> **Assignee:** {{Nome}}

> **CONTRATO INVIOLÁVEL:** Sub-task é unidade atômica de trabalho verificável. Só vira `DONE` com §6 (Completeness), §7 (DoD) e §9 (Quality Standards) 100% ✅.
>
> **Regra de existência:** algo é sub-task (não *step*) se pode ser trabalhada em paralelo com outras sub-tasks **e** tem *acceptance* verificável independentemente. Se é sequencial e indivisível, é passo dentro de uma sub-task maior.

---

## Sumário

0. [Identificação](#0-identificação)
1. [Intent](#1-intent)
2. [Trace](#2-trace)
3. [Acceptance Criteria (Gherkin)](#3-acceptance-criteria-gherkin)
4. [Escopo e Anti-Scope](#4-escopo-e-anti-scope)
5. [Artifacts Produced](#5-artifacts-produced)
6. [Completeness Criteria](#6-completeness-criteria)
7. [Definition of Done](#7-definition-of-done)
8. [Invariants](#8-invariants)
9. [Quality Standards](#9-quality-standards)
10. [Dependencies](#10-dependencies)
11. [Effort Estimate](#11-effort-estimate)
12. [Observability](#12-observability)
13. [Sign-off](#13-sign-off)
14. [Change Log](#14-change-log)

---

## 0. Identificação

| Field | Value |
|---|---|
| **ST ID** | ST-NNN |
| **Título** | {{título crisp}} |
| **WI pai** | WI-SXX-NNN |
| **Sprint** | S-XX |
| **Assignee** | {{Nome}} |
| **Status** | TODO |
| **Data criação** | YYYY-MM-DD |
| **Data alvo** | YYYY-MM-DD |
| **Data DONE** | YYYY-MM-DD |
| **PR** | {{URL}} |

---

## 1. Intent

> *(UMA frase. O que esta sub-task entrega, verificável.)*

**Exemplo:**
> Implementar a função `blake3_digest(bytes: &[u8]) -> Digest` que calcula o BLAKE3 hash de um buffer, retornando `Digest { fn: BLAKE3, bytes: [u8; 32] }`.

---

## 2. Trace

Rastreabilidade ascendente obrigatória.

| Artefato | Status | Relação |
|---|---|---|
| **WI-SXX-NNN**: {{nome}} | DOING | parent |
| **CAP-XXX** (via parent WI) | FROZEN | contribui parcialmente |
| **INV-YYY** (se aplicável) | ACTIVE | preserva ou implementa |
| **NFR-ZZZ** (se aplicável) | FROZEN | cumpre |

---

## 3. Acceptance Criteria (Gherkin)

Cada AC mapeado a teste executável.

### AC-001: {{descrição}}

```gherkin
Given {{estado/input}}
When {{ação}}
Then {{resultado observável}}
```

**Teste:** `tests/{{path}}::test_ac_001`

### AC-002: {{...}}

```gherkin
Given ...
When ...
Then ...
```

**Teste:** `tests/...`

---

## 4. Escopo e Anti-Scope

### 4.1 Em escopo
- {{item preciso}}
- {{item preciso}}

### 4.2 Anti-scope
- {{item que NÃO faz}} → coberto em ST-MMM / WI-SYY-ZZZ / backlog #N

---

## 5. Artifacts Produced

### 5.1 Código

| Arquivo | Mudança |
|---|---|
| `src/{{path}}.rs` | nova função / struct |

### 5.2 Testes

| Arquivo | Tipo |
|---|---|
| `tests/{{path}}.rs` | unit / integration / property |

### 5.3 Docs

| Arquivo | Tipo |
|---|---|
| Rustdoc em funções/tipos públicos | inline |
| {{outro}} (se aplicável) | — |

---

## 6. Completeness Criteria

> Proporcional ao escopo da sub-task. Cada caixa binária.

### 6.1 Código

- [ ] Função(ões) implementada(s) conforme design
- [ ] Rustdoc em items públicos com `# Arguments`, `# Returns`, `# Errors`
- [ ] Sem `unwrap()`/`expect()`/`panic!()` reachable por input externo
- [ ] Sem `TODO`/`FIXME` órfãos
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] Sem novas deps sem justificativa

### 6.2 Testes

- [ ] Todos AC em §3 testados e verdes
- [ ] Happy path coberto
- [ ] Error path(s) coberto(s)
- [ ] Boundary conditions cobertas (empty, max, off-by-one)
- [ ] Property test se invariante aplicável (§8)
- [ ] Determinístico (100 runs consecutivos ✅)
- [ ] Isolamento (roda em paralelo sem shared state)

### 6.3 Documentação

- [ ] Rustdoc gera sem warnings (`cargo doc --no-deps`)
- [ ] Comentários `// SAFETY:` / `// INVARIANT:` onde relevante

### 6.4 Observability (se aplicável)

- [ ] Métricas em §12 emitidas
- [ ] Logs estruturados com campos obrigatórios
- [ ] Traces span criados

### 6.5 Segurança (se aplicável)

- [ ] Input validado na fronteira
- [ ] Sem secret hardcoded
- [ ] Error messages não vazam internals

### 6.6 Processo

- [ ] PR aberto com descrição explicando *por quê*
- [ ] CI verde no PR
- [ ] Code review: ≥ 1 aprovação
- [ ] Todos comentários `MUST_FIX` resolvidos

---

## 7. Definition of Done

Esta ST está `DONE` quando **TODAS** as caixas de §6 estão ✅ **E** PR está mergeado na branch do WI pai.

- [ ] §6 (todas subseções aplicáveis) 100% ✅
- [ ] §9 (Quality Standards) atendido
- [ ] §8 (Invariants) preservados
- [ ] PR mergeado em `feat/WI-SXX-NNN-...`

---

## 8. Invariants

### 8.1 Invariants que esta ST preserva

| ID | Invariant | Verificação nesta ST |
|---|---|---|
| INV-XXX (do produto) | {{enunciado}} | property test + assertion no código |
| WI-INV-YYY (do WI pai) | {{enunciado}} | test |

### 8.2 Invariants operacionais durante trabalho

| ID | Invariant | Verificação |
|---|---|---|
| ST-INV-001 | `main` branch permanece verde | CI pre-merge |
| ST-INV-002 | Nenhuma regressão em testes pre-existentes | Full suite no CI |
| ST-INV-003 | Nenhum secret commitado | git-secrets + pre-commit |

### 8.3 Invariants novos introduzidos

- [ ] Nenhum
- [ ] Proposto: {{enunciado}} → motion para catálogo global

---

## 9. Quality Standards

### 9.1 Test

| Métrica | Threshold |
|---|---|
| Line coverage (código novo) | ≥ 85% |
| Branch coverage (código novo) | ≥ 80% |
| Property test iterations (se aplicável) | ≥ 1000 |
| Mutation kill rate (se crítico) | ≥ 75% |

### 9.2 Code quality

| Métrica | Threshold |
|---|---|
| Complexidade ciclomática | ≤ 15 |
| Linhas por função | ≤ 100 |
| Nested blocks | ≤ 4 |
| Argumentos por função | ≤ 7 |
| Lint warnings | 0 |
| Format diff | 0 |

### 9.3 Performance (se aplicável)

| Métrica | Budget |
|---|---|
| Latência operação | ≤ {{X}}ms |
| Alocações | ≤ {{N}} |

### 9.4 Security (se aplicável)

- [ ] SAST clean para esta mudança
- [ ] Sem vuln introduzida em deps

### 9.5 Observability (se aplicável)

- [ ] Cardinality respeitada
- [ ] Logs com `trace_id` / `tenant_id`

---

## 10. Dependencies

### 10.1 Upstream (bloqueia esta ST)

| Dependência | Status |
|---|---|
| ST-MMM `DONE` | ❌ |
| {{dep externa}} | ✅ |

### 10.2 Downstream (esta ST libera)

| Dependente | Relação |
|---|---|
| ST-PPP | pode começar após esta ST |

---

## 11. Effort Estimate

### 11.1 Size

- [ ] XS — ≤ 2h
- [ ] S — ≤ 4h
- [ ] M — ≤ 1 dia
- [ ] L — ≤ 2 dias
- [ ] XL — > 2 dias → **DEVE** decompor em sub-sub-tasks ou virar WI próprio

### 11.2 Rationale

> *(Por que este tamanho?)*

---

## 12. Observability

### 12.1 Métricas (se aplicável)

| Nome | Tipo | Labels |
|---|---|---|
| {{nome}} | counter / histogram / gauge | {{labels}} |

### 12.2 Logs

- Evento `{{nome}}.success`: INFO, fields `trace_id, tenant_id, ...`
- Evento `{{nome}}.error`: ERROR, fields `trace_id, error_kind, ...`

### 12.3 Traces

- Span `{{nome}}`: attributes `{{atributos}}`

---

## 13. Sign-off

| Papel | Nome | Critério | Assinatura | Data |
|---|---|---|---|---|
| **Assignee** | {{Nome}} | ST completa conforme contrato | _________________ | YYYY-MM-DD |
| **Code Reviewer** | {{Nome}} | §6 + §9 ✅ | _________________ | YYYY-MM-DD |
| **WI Owner** (parent) | {{Nome}} | ST integra ao WI sem regressão | _________________ | YYYY-MM-DD |

---

## 14. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | ST criada em TODO |

---

**Fim de ST-NNN.**
