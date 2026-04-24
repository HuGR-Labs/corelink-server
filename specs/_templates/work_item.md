# Work Item — WI-SXX-NNN: {{Título}}

> **Template Version:** 1.0.0
> **Status:** PROPOSED | READY | DOING | REVIEWING | DONE | BLOCKED | CANCELED
> **Última atualização:** YYYY-MM-DD
> **Owner (assignee):** {{Nome}}

> **CONTRATO INVIOLÁVEL:** Um Work Item só transiciona para `DONE` quando **TODAS** as caixas de **Completeness Criteria (§9)**, **Definition of Done (§10)** e **Quality Standards (§12)** estão ✅. Não existe "parcial". Sub-tasks (§13) seguem o mesmo contrato em escala proporcional — ver `_templates/subtask.md`.

---

## Sumário

0. [Identificação](#0-identificação)
1. [Intent](#1-intent)
2. [Capability Mapping / Trace](#2-capability-mapping--trace)
3. [Tipo e Classificação](#3-tipo-e-classificação)
4. [Escopo](#4-escopo)
5. [Anti-Scope](#5-anti-scope)
6. [Acceptance Criteria (Gherkin)](#6-acceptance-criteria-gherkin)
7. [Design Decisions](#7-design-decisions)
8. [Artifacts Produced](#8-artifacts-produced)
9. [Completeness Criteria (SOTA)](#9-completeness-criteria-sota)
10. [Definition of Done](#10-definition-of-done)
11. [Invariants (Durante Execução)](#11-invariants-durante-execução)
12. [Quality Standards (SOTA)](#12-quality-standards-sota)
13. [Sub-tasks](#13-sub-tasks)
14. [Dependencies](#14-dependencies)
15. [Effort Estimate](#15-effort-estimate)
16. [Observability Plan](#16-observability-plan)
17. [Rollback / Recovery](#17-rollback--recovery)
18. [Security & Privacy Considerations](#18-security--privacy-considerations)
19. [Risk Register](#19-risk-register)
20. [Review Checkpoints](#20-review-checkpoints)
21. [Sign-off](#21-sign-off)
22. [Change Log](#22-change-log)

---

## 0. Identificação

| Field | Value |
|---|---|
| **WI ID** | WI-SXX-NNN |
| **Título** | {{título crisp}} |
| **Versão** | 1.0.0 |
| **Status** | PROPOSED |
| **Sprint pai** | S-XX |
| **WI pai (se sub-WI)** | WI-SXX-MMM |
| **WIs filhos (se decomposto)** | WI-SXX-AAA, WI-SXX-BBB |
| **Sub-tasks** | ver §13 |
| **Owner (assignee)** | {{Nome}} |
| **Reviewer(s) designado(s)** | {{Nome, Nome}} |
| **Data criação** | YYYY-MM-DD |
| **Data alvo (target)** | YYYY-MM-DD |
| **Data real DONE** | YYYY-MM-DD |
| **Branch** | `feat/WI-SXX-NNN-{{slug}}` |
| **PR** | {{URL}} |
| **Tags** | {{subsistema, área}} |

---

## 1. Intent

> *(UMA frase declarativa. O que este WI entrega, verificável objetivamente.)*

**Exemplo:**
> Implementar o endpoint `CAS::FindMissingBlobs` do REAPI v2 com lookup em R2 e cache de metadata em KV, servindo requests de tenant autenticado em P99 ≤ 50ms para batches de ≤ 1000 digests.

---

## 2. Capability Mapping / Trace

Rastreabilidade ascendente obrigatória.

| Artefato upstream | Status | Relação com este WI |
|---|---|---|
| **CAP-XXX**: {{nome}} | FROZEN | delivers FULL/PARTIAL |
| **CAP-YYY**: {{nome}} | FROZEN | delivers PARTIAL (subset A) |
| **INV-AAA**: {{nome}} | ACTIVE | must preserve |
| **INV-BBB**: {{nome}} | ACTIVE | new property added |
| **NFR-CCC**: {{nome}} | FROZEN | must meet threshold |
| **ADR-XXXX**: {{decisão}} | ACCEPTED | implements |
| **User Journey UJ-YYY** | FROZEN | partial implementation |

---

## 3. Tipo e Classificação

### 3.1 Tipo

Marcar **um** primário, até 2 secundários:

- [ ] `feature` — nova capability user-facing
- [ ] `infra` — infraestrutura, sem impacto user-facing direto
- [ ] `refactor` — melhoria interna, comportamento inalterado
- [ ] `docs` — documentação / spec / runbook
- [ ] `security` — controle de segurança, vuln fix, hardening
- [ ] `privacy` — controle de privacidade, GDPR/LGPD
- [ ] `perf` — otimização de performance
- [ ] `ops` — observability, deploy, runbook, ferramentação
- [ ] `test` — expansão de cobertura, tipos de teste
- [ ] `ux` — experiência, ergonomia, DX
- [ ] `compliance` — controle regulatório
- [ ] `debt` — pagamento de dívida técnica registrada

### 3.2 Prioridade

- [ ] P0 — bloqueante (sprint não entrega sem este)
- [ ] P1 — crítico (alto custo de não fazer)
- [ ] P2 — importante (valor substancial)
- [ ] P3 — desejável (nice-to-have)

### 3.3 Blast radius

- [ ] **Local** — afeta apenas função/módulo
- [ ] **Subsystem** — afeta um componente C4
- [ ] **System** — afeta múltiplos componentes
- [ ] **Product** — afeta usuários ou API pública
- [ ] **Organization** — afeta processos externos (Legal, Finance, Support)

### 3.4 Reversibilidade

- [ ] Two-way door (fácil reverter, <1d)
- [ ] Hybrid (reversível em janela T, depois torna-se one-way)
- [ ] One-way door (custosa/impossível reverter)

**Justificativa da classificação:** {{prosa}}

---

## 4. Escopo

### 4.1 Em escopo (exaustivo)

Descrição precisa do que este WI **faz**:

- {{item}}
- {{item}}
- {{item}}

### 4.2 Componentes afetados

| Componente | Mudança |
|---|---|
| `src/reapi/cas.rs` | nova função `find_missing_blobs` |
| `src/storage/r2.rs` | nova função `head_object_batch` |
| `src/storage/local_cache.rs` | cache de resultado por 60s |

### 4.3 Arquivos novos

- {{path}}: {{propósito}}

### 4.4 Arquivos modificados

- {{path}}: {{mudança}}

### 4.5 Arquivos deletados

- {{path}}: {{razão}}

---

## 5. Anti-Scope

Itens explicitamente **NÃO** cobertos por este WI. Cada item rastreado.

| Item | Razão | Onde será coberto |
|---|---|---|
| {{feature adjacente}} | Fora da intent §1 | WI-SXX-YYY |
| {{otimização Z}} | Requer benchmark first | backlog issue #123 |
| {{decisão W}} | Rejeitada | ADR-XXXX |

---

## 6. Acceptance Criteria (Gherkin)

Toda condição de aceitação **DEVE** estar em sintaxe Given-When-Then, mapeada a teste executável.

### AC-001: {{descrição curta}}

```gherkin
Given {{estado inicial verificável}}
When {{ação executada}}
Then {{resultado observável}}
And {{condição adicional}}
```

**Mapeado em teste:** `tests/integration/wi_sxx_nnn_test.rs::test_ac_001`

### AC-002: {{...}}

```gherkin
Given ...
When ...
Then ...
```

**Mapeado em teste:** `tests/...`

### AC-003 a AC-N

*(preencher todos os AC necessários; cada um mapeado a teste)*

---

## 7. Design Decisions

### 7.1 Decisões locais (não justificam ADR próprio)

| # | Decisão | Rationale curto | Alternativas consideradas |
|---|---|---|---|
| 1 | {{decisão}} | {{por quê}} | {{alternativa descartada}} |

### 7.2 Decisões que justificam ADR

Se durante este WI surgir decisão arquitetural não-óbvia, **DEVE** ser extraída para ADR separado **antes** de continuar implementação.

- [ ] Nenhuma decisão merece ADR neste WI.
- [ ] ADR(s) propostos: ADR-XXXX, ADR-YYYY (links para drafts).

### 7.3 Trade-offs explícitos

| Em troca de... | Aceitamos... |
|---|---|
| {{benefício}} | {{custo}} |

---

## 8. Artifacts Produced

### 8.1 Código

| Artefato | Localização | Propósito |
|---|---|---|
| Função `find_missing_blobs` | `src/reapi/cas.rs` | Handler do RPC REAPI |
| Struct `MissingBlobsResult` | `src/reapi/cas.rs` | Return type tipado |

### 8.2 Testes

| Arquivo | Tipo | Cobre |
|---|---|---|
| `tests/unit/cas_find_missing_test.rs` | unit | lógica pura |
| `tests/integration/cas_r2_test.rs` | integration | integração R2 |
| `tests/props/cas_idempotency.rs` | property-based | INV-CASIdempotency |
| `tests/contract/reapi_conformance_test.rs` | contract | REAPI v2 spec |

### 8.3 Documentação

| Arquivo | Tipo |
|---|---|
| Rustdoc em `src/reapi/cas.rs` | API docs |
| `docs/reapi/find_missing_blobs.md` | guia de uso |

### 8.4 Infrastructure

| Item | Tipo |
|---|---|
| {{item}} | migration / config / deploy |

### 8.5 Observability

| Recurso | Tipo |
|---|---|
| `corelink_cas_find_missing_total` | counter |
| `corelink_cas_find_missing_duration_seconds` | histogram |
| `CoreLinkCasLatencyHigh` | alerta |
| Dashboard `CoreLink CAS Operations` (painel) | dashboard |

### 8.6 ADRs / PDRs / ODRs escritos

- {{ID ou "nenhum"}}

### 8.7 Runbooks

- `runbooks/rb-cas-high-latency.md` — atualizado

---

## 9. Completeness Criteria (SOTA)

> Rigor SOTA. Caixas binárias. Sem "quase".

### 9.1 Code Completeness

- [ ] Toda função pública tem Rustdoc com `# Arguments`, `# Returns`, `# Errors`
- [ ] Toda struct/enum pública tem Rustdoc
- [ ] Todo módulo novo tem `//! module-level doc comment`
- [ ] Nenhum `unwrap()`/`expect()`/`panic!()` reachable por input externo
- [ ] Nenhum `todo!()`/`unimplemented!()`
- [ ] Nenhum TODO/FIXME órfão (todos linkam issue aberta)
- [ ] Linters clean: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Format clean: `cargo fmt --check`
- [ ] Nenhum `#[allow(...)]` sem comentário justificativo
- [ ] Nenhuma dep nova adicionada sem revisão explícita em §7.1
- [ ] `cargo udeps` clean (sem deps não usadas introduzidas)
- [ ] `cargo deny check` clean (licenças, CVEs, bans)

### 9.2 Test Completeness

- [ ] Todos AC em §6 têm teste correspondente e verde
- [ ] Happy path testado
- [ ] Cada error path testado
- [ ] Cada boundary condition testada (empty, zero, max, off-by-one)
- [ ] Cada INV em §11 tem property test
- [ ] Cada integração externa tem contract test
- [ ] Determinismo verificado: 100 runs consecutivos passam
- [ ] Isolamento verificado: teste roda em paralelo sem colisão
- [ ] Nomes descritivos seguindo `test_{unidade}_{condicao}_{resultado_esperado}`

### 9.3 Documentation Completeness

- [ ] README atualizado se impacto user-facing
- [ ] Rustdoc gerada sem warnings: `cargo doc --no-deps`
- [ ] Architecture diagrams (C4) atualizados se arquitetura mudou
- [ ] Sequence diagram criado se novo fluxo crítico
- [ ] Glossário atualizado se novos termos
- [ ] CHANGELOG.md com entrada

### 9.4 Observability Completeness

- [ ] Métricas planejadas em §16 sendo emitidas
- [ ] Cardinality budget respeitado (verificado por script)
- [ ] Logs estruturados com `trace_id`, `tenant_id`, outros campos obrigatórios
- [ ] Traces span: cada chamada externa tem span próprio
- [ ] Dashboard atualizado
- [ ] Alertas configurados com runbook linkado

### 9.5 Security Completeness

- [ ] AuthN obrigatória em endpoints novos (test cobre path sem auth)
- [ ] AuthZ / tenant isolation enforced (property test)
- [ ] Input validation em fronteira externa
- [ ] Error messages sanitized (não vazam internals)
- [ ] Secrets em secrets manager (zero hardcoded)
- [ ] Dependências scanned (`cargo audit`)
- [ ] Se novo endpoint HTTP: headers seguros (HSTS, CSP)
- [ ] Threat model atualizado se novo trust boundary

### 9.6 Privacy Completeness

- [ ] Se toca PII: data classification aplicada
- [ ] Se toca PII: retention configurada
- [ ] Se toca PII: audit log emitido
- [ ] LINDDUN analysis atualizada se novo data flow pessoal
- [ ] Direito de exclusão testado em E2E se aplicável

### 9.7 Performance Completeness

- [ ] Benchmark adicionado se hot path
- [ ] Performance budgets §12.3 verificados
- [ ] Soak test OK se dependência longa (≥ 1h exec)
- [ ] Profiling executado se suspeita de regressão

### 9.8 Reliability Completeness

- [ ] Timeout configurado em toda chamada externa
- [ ] Retry com exponential backoff + jitter onde aplicável
- [ ] Circuit breaker onde aplicável (dep externa instável)
- [ ] Graceful degradation verificada em cenário de falha externa
- [ ] Idempotência verificada se mutativo (test de duplo-write)

### 9.9 Compliance Completeness

- [ ] Se afeta dados regulados: review registrada
- [ ] Audit trail para operação sensível
- [ ] Retention policy correta

### 9.10 Process Completeness

- [ ] PR aberto com descrição explicando *por quê*
- [ ] CI verde no PR
- [ ] Code review aprovada por ≥ 1 outro engenheiro
- [ ] Sub-tasks (§13) todos `DONE`
- [ ] Nenhum blocker registrado em §19 está aberto
- [ ] Sign-off completo em §21

---

## 10. Definition of Done

Este WI está `DONE` quando **TODAS** as categorias §9 (Completeness Criteria) estão 100% ✅. Nada além, nada menos.

**Regra inviolável:**

- ❌ "Fica pro próximo WI" — decompor ou re-scopear, não aceitar pendência.
- ❌ "95% das caixas" — não é DONE, é `REVIEWING` até 100%.
- ❌ "Depois consertamos" — se consertar depois, é outro WI com trace.

### 10.1 DoD Snapshot

- [ ] Todas §9.1 a §9.10 ✅
- [ ] Sub-tasks §13 todas ✅
- [ ] Invariants §11 preservados (verificados)
- [ ] Quality Standards §12 atingidos (medidos)
- [ ] Sign-off §21 completo

---

## 11. Invariants (Durante Execução)

Propriedades que **DEVEM** valer em todo estado intermediário e final.

### 11.1 Safety (nada ruim acontece)

| ID | Invariant | Verificação |
|---|---|---|
| WI-INV-001 | `main` branch sempre compilando (nenhum commit quebra build) | CI pre-merge |
| WI-INV-002 | Nenhuma regressão em testes pre-existentes | Full regression suite pre-merge |
| WI-INV-003 | Nenhum vazamento de `tenant_id` em cross-tenant | Property test INV-TenantIsolation |
| WI-INV-004 | Nenhum secret commitado | git-secrets pre-commit |

### 11.2 Liveness (algo bom eventualmente acontece)

| ID | Invariant | Verificação |
|---|---|---|
| WI-INV-005 | Toda request termina dentro de timeout declarado | Test com carga + timeout |
| WI-INV-006 | Cache eventualmente converge após write (stale ≤ TTL) | Integration test |

### 11.3 Invariants do produto que este WI deve preservar

| ID | Invariant | Como preservamos |
|---|---|---|
| INV-TenantIsolation | Nenhum cross-tenant leak | Property test, 10k iterations, 2 tenants |
| INV-CASIdempotency | Upload duplicado = 1 blob | Integration test |
| INV-AuditLogImmutability | Audit log só append | Schema constraint + test |

### 11.4 Novos invariants introduzidos por este WI

> *(Se este WI introduz invariants, listar aqui. Cada invariant novo DEVE ir para `02_product/invariants.md` com ID permanente via thaw.)*

- {{ID proposto}}: {{enunciado}} — motion para adicionar ao catálogo global.

---

## 12. Quality Standards (SOTA)

### 12.1 Test Coverage

| Métrica | Threshold | Como medir |
|---|---|---|
| Line coverage (código novo) | ≥ 85% | `cargo llvm-cov` |
| Branch coverage (código novo) | ≥ 80% | `cargo llvm-cov --branch` |
| Mutation kill rate (paths críticos) | ≥ 75% | `cargo-mutants` |
| Property test iterations | ≥ 1000 | `proptest` config |

### 12.2 Code Quality

| Métrica | Threshold |
|---|---|
| Complexidade ciclomática por função | ≤ 15 |
| Linhas por função | ≤ 100 |
| Linhas por arquivo | ≤ 1000 |
| Depth de nested blocks | ≤ 4 |
| Número de argumentos por função | ≤ 7 |
| Lint warnings | 0 |
| Format diff | 0 |

### 12.3 Performance Budgets

| Métrica | Budget |
|---|---|
| Latência p50 | ≤ NFR-XXX.p50 |
| Latência p99 | ≤ NFR-XXX.p99 |
| Memory footprint por request | ≤ {{X}} MiB |
| Alocações por request | ≤ {{N}} |
| Throughput mínimo sustentado | ≥ {{RPS}} |

### 12.4 Security Gates

- [ ] SAST (CodeQL/semgrep): zero High/Critical
- [ ] `cargo audit`: zero vuln aberta
- [ ] `cargo deny check`: clean (license, bans, CVE)
- [ ] Fuzzing em parsers novos: sem panic em {{N}} horas
- [ ] Pen-testable endpoints: documentados e marcados

### 12.5 Observability Gates

- [ ] Toda métrica declarada em §16 emitindo em staging
- [ ] Toda log com campos obrigatórios (`trace_id`, `tenant_id`, `component`)
- [ ] Traces completos (sem "orphan span")
- [ ] Cardinality budget: estimativa calculada e documentada

### 12.6 Documentation Gates

- [ ] Rustdoc 100% public items
- [ ] Architecture diagrams refletem código
- [ ] Runbook testado em dry-run
- [ ] Glossário atualizado se termos novos

### 12.7 Accessibility & i18n Gates (se afeta UI)

- [ ] WCAG 2.1 AA: `axe-core` clean
- [ ] Strings extraíveis para tradução (i18n)
- [ ] Sem hardcoded pt-BR/EN em UI

### 12.8 Reproducibility Gates

- [ ] Build reprodutível: `Cargo.lock` commitado
- [ ] Ambiente dev: `make dev-setup` em máquina limpa em ≤ 30 min
- [ ] Testes reproduzíveis: mesma seed → mesmo resultado (para `proptest`)

---

## 13. Sub-tasks

> Sub-task = unidade de trabalho verificável independentemente, decomposta deste WI.
>
> **Regra:** um item é sub-task (não apenas step) IFF pode ser trabalhada em paralelo por outra pessoa e tem *acceptance* independente.
>
> Cada sub-task segue o template simplificado em `_templates/subtask.md`.

### Estrutura mínima por sub-task (inline aqui)

```markdown
### ST-NNN: {{título}}

- **Status:** TODO | DOING | REVIEW | DONE | BLOCKED
- **Assignee:** {{Nome}}
- **Dependencies:** ST-MMM (internal), {{externa}}

#### Intent
{{uma frase}}

#### Acceptance
Given / When / Then (mínimo 1)

#### Completeness (SOTA, proporcional ao escopo)
- [ ] Código escrito conforme design
- [ ] Testes unitários escritos e verdes
- [ ] Testes de integração se cruzar boundary
- [ ] Rustdoc em APIs novas
- [ ] Métricas/logs/traces se aplicável
- [ ] Lint + format clean
- [ ] PR aberto, CI verde, reviewed

#### DoD
- [ ] Todas caixas de Completeness ✅
- [ ] PR mergeado na branch do WI

#### Invariants (durante ST)
- Nenhuma regressão em testes existentes
- `main` do repo permanece verde

#### Quality
- Coverage do código novo ≥ 85%
- Sem clippy warnings
- Sem TODO órfão
```

### Lista de sub-tasks deste WI

> *(preencher todas as ST com a estrutura acima ou referenciar arquivos separados em `_templates/subtask.md` instanciados)*

#### ST-001: {{primeira sub-task}}

*(instanciar conforme estrutura)*

#### ST-002: {{...}}

*(instanciar)*

---

## 14. Dependencies

### 14.1 Upstream (bloqueia este WI)

| Dependência | Tipo | Status |
|---|---|---|
| WI-SXX-YYY `DONE` | interno sprint | ❌ |
| ADR-XXXX `ACCEPTED` | decisão | ✅ |
| Spec `02_product/capabilities.md` `FROZEN` | spec | ✅ |
| Vendor X API vY disponível | externa | ✅ |

### 14.2 Downstream (este WI desbloqueia)

| Dependente | O que libera |
|---|---|
| WI-SXX-ZZZ | pode começar após este `DONE` |

### 14.3 Cross-sprint dependencies

| Sprint / WI externo | Relação |
|---|---|
| S-YY | consome artefato produzido aqui |

---

## 15. Effort Estimate

### 15.1 Size T-shirt

- [ ] XS — ≤ 2h
- [ ] S — ≤ 1 dia
- [ ] M — ≤ 3 dias
- [ ] L — ≤ 5 dias
- [ ] XL — > 5 dias → **DEVE** decompor em sub-tasks ou sub-WIs

### 15.2 Rationale

> *(Por que este tamanho? Em que se baseia?)*

### 15.3 Optimistic / Realistic / Pessimistic

| | Otimista | Realista | Pessimista |
|---|---|---|---|
| Dias úteis | {{X}} | {{Y}} | {{Z}} |

### 15.4 Uncertainty factors

- {{fator que pode aumentar esforço}}
- {{fator que pode diminuir}}

---

## 16. Observability Plan

### 16.1 Métricas novas

| Nome | Tipo | Unidade | Labels | Cardinality budget | Alerta |
|---|---|---|---|---|---|
| `corelink_xxx_total` | counter | requests | `tenant_id, method, status` | ≤ 10k | rate ↗ X% |
| `corelink_xxx_duration_seconds` | histogram | s | `tenant_id, method` | ≤ 5k | p99 > T |

### 16.2 Logs novos

| Evento | Nível | Campos obrigatórios | Sampling |
|---|---|---|---|
| `xxx.success` | INFO | `trace_id, tenant_id, ...` | 1.0 |
| `xxx.error` | ERROR | `trace_id, tenant_id, error_kind` | 1.0 |

### 16.3 Traces

| Span | Atributos | Parent |
|---|---|---|
| `cas.find_missing_blobs` | `tenant_id, batch_size` | request root |
| `r2.head_object_batch` | `bucket, key_count` | `cas.find_missing_blobs` |

### 16.4 Dashboards modificados

| Dashboard | Mudança |
|---|---|
| `CoreLink CAS Operations` | +painel "find_missing latency p99" |

### 16.5 Alertas configurados

| Alerta | Condição | Severity | Runbook |
|---|---|---|---|
| `CoreLinkCasFindMissingLatencyHigh` | p99 > 100ms por 5m | warning | `runbooks/rb-cas-high-latency.md` |

---

## 17. Rollback / Recovery

### 17.1 Strategy

Este WI é reversível? (Ver §3.4.)

- [ ] Two-way: rollback via `git revert + redeploy` em ≤ 10 min
- [ ] Hybrid: reversível até {{evento/data}}; depois one-way
- [ ] One-way: não reversível; ver §17.3

### 17.2 Rollback procedure (two-way / hybrid)

1. `git revert <commit>`
2. Deploy via `wrangler deploy --rollback`
3. Verificar métricas voltaram ao baseline
4. Comunicar em canal {{X}} se impacto externo

### 17.3 Se one-way, justificativa e mitigação

- **Razão:** {{por que não reversível}}
- **Mitigação de risco:** {{o que fizemos para reduzir chance de precisar reverter}}

### 17.4 Data migration reversibility

- [ ] Migration é `additive-only` (reversível por simples revert)
- [ ] Migration é `expand-contract` (reversível durante fase expand)
- [ ] Migration é destrutiva (drop column); procedure de restore documentado

### 17.5 Feature flags

| Flag | Default inicial | Default final | Remove after |
|---|---|---|---|
| `feature.xxx` | false | true | YYYY-MM-DD |

---

## 18. Security & Privacy Considerations

### 18.1 Novo trust boundary?

- [ ] Sim — threat model atualizado
- [ ] Não

### 18.2 Dados processados

| Categoria | Classificação | Controles |
|---|---|---|
| Digest (hash) | Interno | nenhum especial |
| Blob content | Confidencial | encryption at rest, tenant isolation |

### 18.3 STRIDE mini-análise

| Ameaça | Aplicável? | Mitigação |
|---|---|---|
| Spoofing | Sim / Não / N/A | {{controle}} |
| Tampering | ... | ... |
| Repudiation | ... | ... |
| Information Disclosure | ... | ... |
| DoS | ... | ... |
| Elevation of Privilege | ... | ... |

### 18.4 LINDDUN mini-análise (se PII)

{{aplicar se aplicável}}

### 18.5 Controles específicos deste WI

- {{controle}}
- {{controle}}

---

## 19. Risk Register

| ID | Risco | Prob | Imp | Score | Mitigação | Contingência | Owner |
|---|---|---|---|---|---|---|---|
| WR-001 | {{risco}} | L\|M\|H | L\|M\|H | 1–9 | {{ação}} | {{plano}} | {{Nome}} |

### Risk score actions

| Score | Ação |
|---|---|
| 1–2 | Aceitar |
| 3–4 | Mitigar |
| 6–9 | Mitigar + contingência + revisar semanalmente |

---

## 20. Review Checkpoints

### 20.1 Design review (antes de `DOING`)

- [ ] Design discutido em PR descrição ou doc separado
- [ ] Aprovado por Tech Lead

### 20.2 Code review (durante `REVIEW`)

- [ ] ≥ 1 aprovação de outro engenheiro
- [ ] Todos comentários `MUST_FIX` resolvidos
- [ ] CI verde

### 20.3 Pre-merge review

- [ ] Completeness Criteria §9 100% ✅
- [ ] Quality Standards §12 atendidos
- [ ] Invariants §11 preservados

---

## 21. Sign-off

| Papel | Nome | Critério | Assinatura | Data |
|---|---|---|---|---|
| **WI Owner** | {{Nome}} | WI completo conforme contrato | _________________ | YYYY-MM-DD |
| **Code Reviewer** | {{Nome}} | §9.1 (Code) + §9.2 (Test) ✅ | _________________ | YYYY-MM-DD |
| **Security Reviewer** (se aplicável) | {{Nome}} | §9.5 + §18 ✅ | _________________ | YYYY-MM-DD |
| **Ops Reviewer** (se aplicável) | {{Nome}} | §9.4 + §16 ✅ | _________________ | YYYY-MM-DD |
| **Sprint Owner** | {{Nome}} | WI pode fechar dentro de S-XX | _________________ | YYYY-MM-DD |

---

## 22. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | WI criado em PROPOSED |
| 1.0.1 | YYYY-MM-DD | {{Nome}} | AC refinado após design review |
| 1.1.0 | YYYY-MM-DD | {{Nome}} | Sub-task ST-004 adicionada |

---

**Fim de WI-SXX-NNN.**
