---
id: "ST-NNN-REPLACE"
type: "sub_task"
doc_status: "DRAFT"                      # enum: DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
work_status: "TODO"                      # enum: TODO | DOING | REVIEW | BLOCKED | DONE | CANCELED
audit_status: "ACTIVE"                   # enum: ACTIVE | AUDIT_PENDING | AUDITED
version: "1.0.0"
created: "YYYY-MM-DD"
updated: "YYYY-MM-DD"
owner: "TEMPLATE_WI_OWNER"
assignee: "TEMPLATE_ASSIGNEE"
final_approver: "TEMPLATE_WI_OWNER"
reviewers:
  - role: "eng"
    name: "TEMPLATE_REVIEWER"
parent: "WI-SXX-NNN-REPLACE"
supersedes: null
superseded_by: null
tags: []
---

# Sub-task — ST-NNN: {{Título}}

> **Template Version:** 2.2.0
> **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED (default inicial: `DRAFT`)
> **work_status:** TODO | DOING | REVIEW | BLOCKED | DONE | CANCELED (default inicial: `TODO`)
> **audit_status:** ACTIVE | AUDIT_PENDING | AUDITED (default inicial: `ACTIVE`)
> **Versão:** 1.0.0
> **Última atualização:** YYYY-MM-DD
> **Owner:** {{WI Owner}}
> **Assignee:** {{Nome}}
> **Aprovador Final:** {{WI Owner}}
> **Revisores:** {{ENG: Nome}}
> **WI pai:** WI-SXX-NNN
> **Supersedes:** —
> **Superseded By:** —

> **CONTRATO INVIOLÁVEL:**
>
> 1. ST só vira `DONE` quando **TODAS** caixas de §7 (Completeness), §8 (DoD), §10 (Quality) estão ✅ **COM EVIDENCE**.
> 2. `✅` sem *evidence* = `❌`.
> 3. Toda caixa tem **role** e **modo de validação** (🤖/👤).
> 4. **Regra de existência:** é sub-task (não *step*) IFF trabalhável em paralelo **E** tem *acceptance* independente. Sequencial-indivisível = *step* interno, não ST.

---

## Legenda

| Role | Automation |
|---|---|
| 🔧 ENG, 🔒 SEC, 🔏 PRIV, 📊 SRE, 🎨 PROD, ✓ QA, 🏛 ARCH | 🤖 automatizado / 👤 manual / 🤖👤 híbrido |

---

## Sumário

0. [Identificação](#0-identificação)
1. [Intent](#1-intent)
2. [Trace](#2-trace)
3. [Acceptance Criteria (Gherkin)](#3-acceptance-criteria-gherkin)
4. [Escopo e Anti-scope](#4-escopo-e-anti-scope)
5. [Design Notes](#5-design-notes)
6. [Artifacts Produced](#6-artifacts-produced)
7. [Completeness Criteria (evidence-driven)](#7-completeness-criteria-evidence-driven)
8. [Definition of Done](#8-definition-of-done)
9. [Invariants](#9-invariants)
10. [Quality Standards (SOTA)](#10-quality-standards-sota)
11. [Dependencies](#11-dependencies)
12. [Effort Estimate & Time-boxing](#12-effort-estimate--time-boxing)
13. [Observability Contributed](#13-observability-contributed)
14. [Security Notes](#14-security-notes)
15. [Rollback / Recovery](#15-rollback--recovery)
16. [Risk Register](#16-risk-register)
17. [Sign-off](#17-sign-off)
18. [Change Log](#18-change-log)

---

## 0. Identificação

| Field | Value |
|---|---|
| **ST ID** | ST-NNN |
| **Título** | {{título crisp imperativo}} |
| **Versão** | 1.0.0 |
| **doc_status** | `DRAFT` (inicial) |
| **work_status** | `TODO` (inicial) |
| **audit_status** | `ACTIVE` |
| **WI pai** | WI-SXX-NNN |
| **Sprint** | S-XX |
| **Assignee** | {{Nome}} |
| **Reviewers** | {{ENG: Nome; + opcional SEC/SRE/QA}} |
| **Data criação** | YYYY-MM-DD |
| **Data alvo** | YYYY-MM-DD |
| **Data início real** | YYYY-MM-DD |
| **Data DONE** | YYYY-MM-DD |
| **Branch** | `feat/WI-SXX-NNN-{{slug}}` (branch do WI pai) |
| **PR(s)** | {{URL}} |
| **Tipo (herda do WI pai)** | feature/infra/test/docs/security/privacy/perf/ops/ux/compliance/debt |

---

## 1. Intent

> **UMA frase declarativa, testável.**

**Padrão:** `Implementar/Adicionar/Remover {{o quê}} que {{faz o quê verificável}} em {{condição}}.`

**Exemplo bom:**
> Implementar `blake3_digest(bytes: &[u8]) -> [u8; 32]` que calcula hash BLAKE3 de buffer arbitrário em CPU ≤ 500 ns/KiB.

**Exemplo ruim:**
> Mexer no hashing. ❌

---

## 2. Trace

| Artefato upstream | Estado requerido | Relação | Evidence |
|---|---|---|---|
| **WI-SXX-NNN** (parent) | `work_status: DOING` + `doc_status: FROZEN` | part-of | — |
| **CAP-XXX** (via parent) | `doc_status: FROZEN` | contribui parcialmente | — |
| **INV-YYY** (se aplicável) | `doc_status: FROZEN`, `audit_status: ACTIVE` | preserva | test em §10 |
| **NFR-ZZZ** (se aplicável) | `doc_status: FROZEN` | cumpre | bench em §10.3 |
| **ADR-XXXX** (se aplicável) | `doc_status: FROZEN` | implementa | — |

---

## 3. Acceptance Criteria (Gherkin)

Cada AC mapeado a teste executável.

### AC-001: {{descrição curta}}

```gherkin
Given {{estado/input}}
When {{ação}}
Then {{resultado observável}}
```

| Evidence | Teste |
|---|---|
| CI test output | `tests/{{path}}::test_ac_001` |

### AC-002: {{...}}

```gherkin
Given ...
When ...
Then ...
```

| Evidence | Teste |
|---|---|
| — | `tests/...` |

### AC coverage matrix

| AC | Happy | Error | Boundary | Property |
|---|---|---|---|---|
| AC-001 | ✅ | ✅ | ✅ | ✅ (se aplicável) |

### Anti-pattern ❌
> AC em prosa genérica. Sempre Gherkin mapeado a teste nomeado.

---

## 4. Escopo e Anti-scope

### 4.1 Em escopo

- {{item preciso}}
- {{item preciso}}

### 4.2 Anti-scope

| Item | Razão | Onde será coberto |
|---|---|---|
| {{item}} | Fora da intent §1 | ST-MMM / WI-SYY-ZZZ / backlog #N |

### 4.3 Componentes tocados

| Arquivo | Ação | Linhas aproximadas |
|---|---|---|
| `src/storage/digest.rs` | cria | ~80 |
| `tests/props/digest_props.rs` | cria | ~50 |

---

## 5. Design Notes

### 5.1 Abordagem

{{Como implementa em 2-3 frases.}}

### 5.2 Decisões locais

| # | Decisão | Rationale |
|---|---|---|
| 1 | Usar crate `blake3` oficial | upstream maintido + SIMD built-in |
| 2 | Retornar `[u8; 32]` em vez de `Vec<u8>` | evita alocação |

### 5.3 Qualquer decisão aqui merece ADR?

- [ ] Não (decisões são locais e não afetam arquitetura sistêmica)
- [ ] Sim — **PARAR**, escalar ao WI owner, criar ADR antes de continuar.

---

## 6. Artifacts Produced

### 6.1 Código

| Arquivo | Mudança |
|---|---|
| `src/storage/digest.rs` | nova função `blake3_digest` + struct `Digest` |

### 6.2 Testes

| Arquivo | Tipo | Cobre |
|---|---|---|
| `tests/unit/digest_test.rs` | unit | happy + error paths |
| `tests/props/digest_props.rs` | property | determinismo + correção |
| `benches/digest_bench.rs` | bench | throughput BLAKE3 |

### 6.3 Docs

| Arquivo | Tipo |
|---|---|
| Rustdoc em `src/storage/digest.rs` | API |

### 6.4 Observability (se aplicável)

| Item | Tipo |
|---|---|
| — | — (ST de função pura, sem I/O) |

---

## 7. Completeness Criteria (evidence-driven)

> Proporcional ao escopo atômico da ST. Cada caixa com *evidence*.

### 7.1 Código

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.1.1 | Função(ões) implementada(s) conforme §5 | 👤 | 🔧 | PR diff |
| 7.1.2 | Rustdoc em items públicos: `# Arguments`, `# Returns`, `# Errors`, `# Panics` se aplicável | 🤖 | 🔧 | `cargo doc --no-deps` clean |
| 7.1.3 | Sem `unwrap()`/`expect()`/`panic!()` reachable por input externo | 🤖👤 | 🔧 | grep + review |
| 7.1.4 | Sem `todo!()`/`unimplemented!()` | 🤖 | 🔧 | grep |
| 7.1.5 | Sem TODO/FIXME órfão (com issue link) | 👤 | 🔧 | PR review |
| 7.1.6 | `cargo clippy -- -D warnings` clean | 🤖 | 🔧 | CI |
| 7.1.7 | `cargo fmt --check` clean | 🤖 | 🔧 | CI |
| 7.1.8 | Nenhuma nova dep sem justificativa em §5.2 | 👤 | 🔧 | PR review |
| 7.1.9 | Nenhum `#[allow(...)]` sem comentário `// SAFETY:` ou `// ALLOWED:` | 👤 | 🔧 | PR review |

### 7.2 Testes

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.2.1 | Todos AC em §3 testados e verdes | 🤖 | ✓ | CI report |
| 7.2.2 | Happy path coberto | 🤖 | ✓ | coverage |
| 7.2.3 | Cada error path testado | 👤 | ✓ | branch coverage + review |
| 7.2.4 | Boundary conditions cobertas (empty, max, off-by-one) | 👤 | ✓ | test inspection |
| 7.2.5 | Property test se invariante aplicável | 🤖 | ✓ | proptest run log ≥ 1000 iter |
| 7.2.6 | Determinismo: 100 runs seq verdes | 🤖 | ✓ | CI matrix |
| 7.2.7 | Isolamento: paralelo sem colisão | 🤖 | ✓ | `cargo test -- --test-threads=8` |
| 7.2.8 | Nomes seguindo `test_{unidade}_{condição}_{resultado}` | 👤 | ✓ | review |

### 7.3 Documentação

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.3.1 | `cargo doc --no-deps` sem warnings | 🤖 | 🔧 | CI |
| 7.3.2 | Comentários `// SAFETY:` / `// INVARIANT:` onde relevante | 👤 | 🔧 | PR review |
| 7.3.3 | Exemplos em Rustdoc compiláveis (`cargo test --doc`) | 🤖 | 🔧 | CI |

### 7.4 Observability (se ST emite telemetria)

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.4.1 | Métricas em §13 emitindo em dev | 🤖 | 📊 | Grafana query |
| 7.4.2 | Logs estruturados com `trace_id`, `tenant_id` | 👤 | 📊 | sample |
| 7.4.3 | Traces span com atributos corretos | 👤 | 📊 | trace inspect |

### 7.5 Segurança (se ST toca fronteira, I/O externa, ou crypto)

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.5.1 | Input validado na fronteira | 🤖 | 🔒 | fuzz test |
| 7.5.2 | Sem secret hardcoded | 🤖 | 🔒 | trufflehog |
| 7.5.3 | Error messages não vazam internals | 👤 | 🔒 | review |
| 7.5.4 | Se crypto: usa primitivos aprovados (blake3, sha2, ring, rustls) | 👤 | 🔒 | review |

### 7.6 Performance (se hot path)

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.6.1 | Benchmark criado / atualizado | 🤖 | 🔧 | `benches/*.rs` |
| 7.6.2 | Budget §10.3 atingido | 🤖 | 🔧 | bench output |
| 7.6.3 | Alocações dentro do budget | 🤖 | 🔧 | dhat heap profile |

### 7.7 Processo

| # | Check | Auto | Role | Evidence |
|---|---|---|---|---|
| 7.7.1 | PR aberto com descrição explicando *por quê* | 👤 | 🔧 | PR URL |
| 7.7.2 | CI verde no PR | 🤖 | 🔧 | CI status |
| 7.7.3 | Code review: ≥ 1 aprovação | 👤 | 🔧 | PR reviews |
| 7.7.4 | Todos `MUST_FIX` resolvidos | 👤 | 🔧 | PR reviews |
| 7.7.5 | Commits squashable / clean | 👤 | 🔧 | `git log` |

---

## 8. Definition of Done

Esta ST está `DONE` quando **TODAS** as subseções aplicáveis de §7 estão 100% ✅ COM EVIDENCE **E** PR mergeado na branch do WI pai.

### 8.1 Snapshot

| Categoria §7 | Aplicável? | Status |
|---|---|---|
| 7.1 Código | ✅ Sempre | ❌ |
| 7.2 Testes | ✅ Sempre | ❌ |
| 7.3 Documentação | ✅ Sempre | ❌ |
| 7.4 Observability | Condicional | N/A ou ❌ |
| 7.5 Segurança | Condicional | N/A ou ❌ |
| 7.6 Performance | Condicional (hot path) | N/A ou ❌ |
| 7.7 Processo | ✅ Sempre | ❌ |

### 8.2 Gates terminais

- [ ] Todas subseções §7 aplicáveis 100% ✅
- [ ] Invariants §9 preservados (teste ✅)
- [ ] Quality §10 atingido (medido)
- [ ] PR mergeado em branch do WI pai
- [ ] Sign-off §17 completo

---

## 9. Invariants

### 9.1 Invariants que esta ST preserva

| ID | Invariant | Verificação nesta ST |
|---|---|---|
| INV-XXX (do produto) | {{enunciado}} | property test + assertion |
| WI-INV-YYY (do WI pai) | {{enunciado}} | test |

### 9.2 Invariants operacionais durante trabalho

| ID | Invariant | Verificação |
|---|---|---|
| ST-INV-001 | `main` branch permanece verde | CI pre-merge |
| ST-INV-002 | Nenhuma regressão em testes pre-existentes | full suite |
| ST-INV-003 | Nenhum secret commitado | git-secrets + pre-commit |
| ST-INV-004 | Nenhuma breaking change em API pública sem PR label `breaking` | API diff check |

### 9.3 Invariants novos introduzidos

- [ ] Nenhum
- [ ] Proposto: **INV-XXX (draft)**: {{enunciado}}
  - Severidade: CRITICAL | HIGH | MEDIUM | LOW
  - Motion para catálogo global via *thaw* de `02_product/invariants.md`

---

## 10. Quality Standards (SOTA)

### 10.1 Test

| Métrica | Threshold | Evidence |
|---|---|---|
| Line coverage (código novo desta ST) | ≥ 85% | `cargo llvm-cov` |
| Branch coverage (código novo) | ≥ 80% | idem |
| Property test iterations (se aplicável) | ≥ 1000 | proptest log |
| Mutation kill rate (se crítico) | ≥ 75% | `cargo-mutants` |

### 10.2 Code

| Métrica | Threshold |
|---|---|
| Complexidade ciclomática por função | ≤ 15 |
| Linhas por função | ≤ 100 |
| Nested blocks | ≤ 4 |
| Argumentos por função | ≤ 7 |
| Lint warnings | 0 |
| Format diff | 0 |

### 10.3 Performance (se hot path)

| Métrica | Budget | Evidence |
|---|---|---|
| Tempo por operação | ≤ {{X}} ns/µs/ms | bench |
| Alocações por chamada | ≤ {{N}} | dhat |
| Throughput (quando aplicável) | ≥ {{X}} op/s | bench |

### 10.4 Security (se aplicável)

| Gate | Requisito | Evidence |
|---|---|---|
| SAST clean pra esta mudança | zero High/Critical | scan |
| Sem nova vuln em deps | clean | `cargo audit` |
| Fuzz (parsers/decoders) | sem panic em ≥ 1M iter | `cargo fuzz` |

### 10.5 Observability (se aplicável)

| Gate | Requisito |
|---|---|
| Cardinality respeitada | ≤ budget |
| Logs com `trace_id` / `tenant_id` | 100% |
| Spans sem órfão | 100% |

### 10.6 Reproducibility

| Gate | Requisito |
|---|---|
| Build reprodutível | `Cargo.lock` respeitado |
| Tests determinísticos | 100 runs consecutivos iguais (`proptest` mesma seed) |

---

## 11. Dependencies

### 11.1 Upstream (bloqueia esta ST)

| Dependência | Status | Hard blocker? |
|---|---|---|
| ST-MMM `DONE` | ❌ | sim |
| {{dep externa}} | ✅ | sim |

### 11.2 Downstream (esta ST libera)

| Dependente | Relação |
|---|---|
| ST-PPP | pode começar após `DONE` |

---

## 12. Effort Estimate & Time-boxing

### 12.1 Size

- [ ] XS — ≤ 2h
- [ ] S — ≤ 4h
- [ ] M — ≤ 1 dia útil
- [ ] L — ≤ 2 dias úteis
- [ ] XL — > 2 dias úteis → **DEVE** decompor mais ou virar WI próprio

### 12.2 Rationale

> Por que este tamanho?

### 12.3 Burndown e escalação

| Checkpoint | % esperado `DONE` |
|---|---|
| 50% do tempo | 50% |
| 100% do tempo | 100% |

**Triggers de escalação:**
- Estouro > 50% → atualizar WI owner em async update
- Estouro > 100% → escalação formal ao WI owner: re-escopear / decompor / cancelar

---

## 13. Observability Contributed

Se esta ST contribui para telemetria:

### 13.1 Métricas

| Nome | Tipo | Labels |
|---|---|---|
| {{nome}} | counter/histogram/gauge | {{labels}} |

### 13.2 Logs

- `{{evento}}.success`: INFO, `trace_id, tenant_id, ...`
- `{{evento}}.error`: ERROR, `trace_id, error_kind, ...`

### 13.3 Traces

- Span `{{nome}}`: attributes `{{...}}`

---

## 14. Security Notes

### 14.1 Esta ST toca fronteira de segurança?

- [ ] Não — função interna pura, sem I/O externa
- [ ] Sim — completar abaixo

### 14.2 Controles implementados

- {{controle 1}}
- {{controle 2}}

### 14.3 STRIDE mini (se aplicável)

| Ameaça | Aplicável? | Mitigação |
|---|---|---|
| Spoofing | — | — |
| Tampering | — | — |
| Repudiation | — | — |
| Information Disclosure | — | — |
| DoS | — | — |
| Elevation of Privilege | — | — |

---

## 15. Rollback / Recovery

### 15.1 Strategy

- [ ] Two-way: `git revert` resolve (ST é aditiva)
- [ ] Hybrid: reversível até {{condição}}
- [ ] One-way: descreva por que e mitigação

### 15.2 Procedure

{{passos, se não-trivial}}

---

## 16. Risk Register

| ID | Risco | Prob | Imp | Score | Mitigação |
|---|---|---|---|---|---|
| ST-R-001 | {{risco}} | L/M/H | L/M/H | 1–9 | {{ação}} |

---

## 17. Sign-off

| Papel | Nome | Critério | Assinatura | Data |
|---|---|---|---|---|
| 🔧 **Assignee** | {{Nome}} | ST completa conforme contrato | _____________ | YYYY-MM-DD |
| 🔧 **Code Reviewer** | {{Nome}} | §7.1 + §7.2 + §10 ✅ | _____________ | YYYY-MM-DD |
| 🔒 **Security Reviewer** (se §14.1=sim) | {{Nome}} | §7.5 + §14 ✅ | _____________ | YYYY-MM-DD |
| 📊 **SRE Reviewer** (se §7.4 aplicável) | {{Nome}} | §7.4 + §13 ✅ | _____________ | YYYY-MM-DD |
| ✓ **QA Reviewer** (se crítico) | {{Nome}} | §7.2 + §10.1 ✅ | _____________ | YYYY-MM-DD |
| **WI Owner** (parent) | {{Nome}} | ST integra ao WI sem regressão | _____________ | YYYY-MM-DD |

---

## 18. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | ST criada em TODO |

---

**Fim de ST-NNN.**

---

### Notas para autores

- **Proporcionalidade:** ST XS pode ter §7.4/§7.5/§7.6 = N/A; ST L exige todas.
- **Evidence mandatória:** sem link/descrição verificável, checkbox é inválido.
- **Decomposição adicional:** se durante `DOING` surgir complexidade escondida, **PARAR**, re-estimar, decompor ou escalar ao WI owner — não empurrar.
