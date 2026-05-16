---
id: AUDIT-OPUS-INDEPENDENT-R3
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-25
reviewers: [Opus 4.7 1M context — Independent SOTA reviewer R3]
supersedes: AUDIT-OPUS-INDEPENDENT-R2
superseded_by: null
tags: [audit, sota, sprint-contracts, round-3, opus, independent, lote-9.4-validation, pre-implementation-gate]
---

# Opus Independent SOTA Review R3 — pós-Lote 9.4

## Disclaimer

Auditoria independente do codex r3 (paralelo; não li seu output). Foco em ângulos não-óbvios + validation R2 findings + pre-implementation gate check. Não duplico findings óbvios já remediados em Lote 9.4 (e.g., INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE no registry, ADR-0018/0019/0020 criados, error_taxonomy criado). Procuro **regressões introduzidas pelo Lote 9.4 + ângulos novos pós-elevation**.

## Methodology R3

1. Validação direta de C-01..C-05 + H-01..H-10 R2 com base no estado pós-Lote 9.4.
2. Cross-doc consistency check entre os artifacts novos (ADRs 0018/0019/0020, error_taxonomy.md, PLANNED-specs.md) e os 21 spec contracts.
3. Quality probe nos sprints elevados S-00..S-06 (real SOTA vs template-fill).
4. Pre-implementation gate read de S-02 (iminent) + S-06 (segundo HIGH_RISK na fila).
5. Strategic recommendations Lote 9.5 — itens que devem preceder S-03/S-04 implementation.

---

## Validation R2 critical findings (C-01..C-05)

### C-01 INV-KEY-OVERLAP triple-value — **CLOSED via ADR-0018 + tabela §3.2.1**

Verificado: `key_management.md §3.2.1` agora tem tabela canonical com 5 asset classes (PAT 24h / Audit 24h / TDK 7d / BYOK 7d / Ed25519 30d) + hard upper bound 30d. ADR-0018 explica decision rationale com NIST SP 800-57 Pt 1 Rev 5 §5.3 + Table 4 references. S-13:88, S-13:132, S-13:150 e S-14 referenciam ADR-0018 explicitamente. **Quality assessment: REAL SOTA** — não é template-fill; rationale é grounded em standard, alternatives considered, trade-off explicit.

**Resíduo R3**: a tabela em `key_management.md §3.2.1` precisa property test mapping per asset (`tla/key_lifecycle.tla` PLANNED em S-13). Sem property test, o "obrigatório por sprint que toca rotation" (texto §3.2.1) é só prosa. Tracked em §4.2 PLANNED matrix mas owner = S-13 sprint impl time, não Lote 9.4. **Status**: CLOSED com follow-up tracked.

### C-02 INV-KEY-* not in registry — **CLOSED via §3.13 (domain KEY)**

`invariant_registry.md §3.13` agora tem entry para INV-KEY-NO-SKIP, INV-KEY-OVERLAP, INV-KEY-AUDIT — todas com severity HIGH, enforcement, cross-ref para `key_management §3.2.1` + ADR-0018, e TLA+ status (PLANNED `key_lifecycle.tla` para 2/3, coberto por `audit_immutability.tla` para INV-KEY-AUDIT).

**Resíduo R3 minor**: §1 Regras de naming continua listando `<DOMAIN> ∈ {TENANT, CAS, AC, GC, DATA, AUDIT, CONF, AVAIL, BILLING, SUPPLY}` — KEY foi adicionado em §3.13 mas não atualizado em §1. Lint quebraria se rodasse; recomendado atualizar `<DOMAIN>` enum.

### C-03 S-04 ↔ S-07 TTL clash — **CLOSED via ADR-0019**

ADR-0019 documenta migration plan + per-tier table. S-04:65, S-04:103, S-04:131, S-04:150, S-04:220 referenciam ADR-0019 in-context. S-04 CAP-AC-004 caption: "Default value supersedido por S-07 CAP-EVICT-002 per-tier table (ADR-0019)".

**Resíduo R3**: ADR-0019:73 menciona "S-04 _spec_contract.md: adicionar `local_deltas: ["CAP-AC-004 TTL default semantics overridden by S-07 CAP-EVICT-002 (ADR-0019)"]`" — porém **nenhum sprint contract** usa o campo `local_deltas:` no YAML (verificado via grep). É reference por prose, não machine-readable. Validator não tem oracle. Minor; tracked como new finding M-N3-04 abaixo.

### C-04 S-20 timing impossível — **PARTIALLY ADDRESSED**

S-20 manteve duração 4 semanas + buffer 10d (não foi decomposto em S-20a/S-20b como sugerido). **Resíduo significativo**:
- §6.1 DoD line 128 diz "30d sustained staging…concurrent observation period; documented post-S-17 chaos automation 4-week period". Texto novo é progressão.
- §13 Timeline D+25 diz "WI-007 SEALED (30d staging sustained gate)" — i.e., 30d obs deve **estar feito** at D+25 do sprint, não fechar no D+30.
- Engineering/Launch separation foi materializada (CAP-LAUNCH-001 separated, WI-S20-008 não bloqueia engineering gate).

**Issue R3 persistente**: para WI-S20-007 finalizar o "30d staging sustained" no D+25, observation period precisa ter começado em D-5 (5 dias antes do sprint). Spec não afirma isso explicitamente. Reading literal: gate é unsatisfiable se observation começa D+0. Sugestão R3 reforçada: §13 deve dizer "**observation period start date = (S-19 SEAL date) + 0**, não (S-20 start)". Senão o sprint vai slipar 30 dias com alta probabilidade.

**Status**: PARTIALLY CLOSED — engineering/launch separação OK; timing aritmética ainda frágil.

### C-05 S-12 binary DoD — **PARTIALLY CLOSED**

R-S12-14 (line 119) reescrita: "fontes de non-determinism documentadas em `docs/build/reproducible.md` + ADR-0015 são pré-requisito (não fallback)". Boa.

10.s12.4 (line 153) ajustado: "AND non-determinism sources documented" — agora é AND, não OR. **Bom**.

**Persistent issue**: DoD §6 line 142 mantém "2 runners produzem binário com diff ≤ 5% bytes (release builds); fontes de non-determinism documentadas". `≤ 5% bytes` ainda é soft-target em DoD (não criterion observable em métrica). Codex R1 + Opus R2 ambos flaggaram esse específico — line 119 R-S12-14 foi clarified mas line 142 DoD checkbox manteve a wording soft. Provavelmente apenas oversight; trivial fix.

**Status**: PARTIALLY CLOSED — 10.s12.4 binary; line 142 DoD ainda tem soft `≤ 5% bytes` checkbox.

---

## Validation R2 high findings (H-01..H-10)

### H-01 TLA+ coverage gap — **CLOSED via §4.2 PLANNED + PLANNED-specs.md**

`invariant_registry.md §4.2` lista 10 invariants em status `PLANNED` com sprint owner explícito. `specs/tla/PLANNED-specs.md` (450+ linhas) tem blueprint detalhado para 5 specs (billing_atomicity, dsr_erasure_atomicity, byok_sovereignty, onboarding_atomicity, key_lifecycle) com state machines, adversarial actions, formal invariants em pseudo-TLA+, CI integration plan. **Quality**: este é um dos artifacts mais SOTA da Lote 9.4 — comparable a PRD-level planning para formal verification.

**Resíduo R3**: §4.4 CI obligation gate menciona `scripts/check_tla_obligations.py` "criar pós-Lote 9.4". Esse script é blocking dependency para gate enforcement; sem script, "obrigação" é honor system. New finding H-N3-01 abaixo.

### H-02 Cardinality explosion S-08 + S-09 + S-14 — **CLOSED via S-14:88**

S-14 R-S14-3 reescrita: "NÃO via métrica labeled por `tenant_id` — proibido por INV-OBS-CARDINALITY-BUDGET S-09. Strategy: (a) métrica agregada `corelink.cas.get.bytes_total{tenant_tier, region}` para budget-safe live monitoring; (b) offline daily job em audit log R2 (S-09 audit bucket) computa top-1% per tenant via batch query — escapa cardinality budget porque é offline. Replication worker copia para sibling region async; lag p99 ≤ 60s. **Lote 9.4 Opus H-02 fix.**"

**Quality assessment: SOTA real** — solução tecnicamente sólida (live aggregated + offline batch para per-tenant top-1%). **Status**: CLOSED.

### H-03 Customer-facing UX — **CLOSED via error_taxonomy.md**

`specs/03_architecture/error_taxonomy.md` (262 linhas) cobre 56 errors em 10 domains. Schema canonical inclui error_code, http_status, retryable, retry_strategy, sdk_exception (3 langs), customer_message (3 locales), next_action, canonical_source. Quality bar é alto: ZERO `next_action: "internal error"` allowed; cardinality budget bounded; "no PII em error" principle. CI gate `scripts/check_error_taxonomy.py` planned Lote 9.5. **Status**: CLOSED para sprint surface coverage; gate scripts são Lote 9.5 dependency.

### H-04 Cumulative observation gates 90+ days — **NOT ADDRESSED**

Strategic-rec S-2 Opus R2 sugeriu criar `specs/04_sprints/timing-gates-cumulative.md` calendarizando observation gates. **Não foi criado**. Sprint S-20 internamente endereçou seu próprio gate (concurrent S-17 chaos), mas **não há documento canonical** que reconcile S-06 (30d post-sprint observation), S-09 (72h synthetic), S-17 (4w chaos), S-20 (30d staging) cumulative. Roadmap real de observation gates é invisível. **Status: OPEN — escala como new finding H-N3-02**.

### H-05 Consent revocation symmetry — **CLOSED via R-S11-8**

S-11 R-S11-8 reescrita: "DELETE /v1/consent/<purpose> revoga **com proof simétrico (Lote 9.4 Opus H-05)** — consent_revocation table espelha schema do consent_ledger 6-field…GDPR Art. 7 alignment: revoke é 'as easy as giving consent' + cryptographically attested. Verify endpoint…". **Status**: CLOSED.

**Resíduo minor**: registry §3.12 ainda só tem `INV-CONSENT-PROOF-VERIFIABLE` (covering grant), não `INV-CONSENT-REVOCATION-PROOF-VERIFIABLE` (sugerido em R2 H-05). Verify endpoint cobre prática mas registry não tem invariante separado. Minor.

### H-06 LGPD Art. 20 abuse appeal — **CLOSED via S-08:104**

10.s08.3 com 3 sub-items (a/b/c) mandatory: self-service score endpoint + appeal endpoint roteia ≤ 24h human reviewer + audit log. LGPD Art. 20 + GDPR Art. 22 cited. error_taxonomy.md `COR_ABUSE_DETECTED` aponta para `/v1/admin/abuse_appeal`. **Status**: CLOSED com strong execution.

### H-07 ADR-0016 FFI cold-start — **NOT EXPLICITLY ADDRESSED**

ADR-0016 stub não foi expandido para cobrir tabela `language → cold_start_p99 → bundle_size → install_size` que sugeri. Verificar: o opus R2 sugestão era expansion de `ADR-0016-ffi-wrappers-vs-native-http.md` que é stub 66 lines. Lote 9.4 não tocou esse ADR. **Status: OPEN — minor, S-15-time issue, mas worth flag para Lote 9.5**.

### H-08 Dual-approval collusion-rotation — **CLOSED via R-S13-6 + INV update**

S-13 R-S13-6 expandido: "**Lote 9.4 Opus H-08 collusion-rotation defense**: além de `caller ≠ approver`, enforcement adicional: nas últimas 3 destructive ops em janela de 24h, deve haver ≥ 3 distinct approvers (anti-collusion-rotation pattern AC-2(7) NIST SP 800-53). Property test cobre cenário de A→approve B / B→approve A loop em sequence." 

Registry §3.12 INV-ADMIN-DUAL-APPROVAL nota "Lote 9.4 Opus H-08 strengthening". error_taxonomy `COR_ADMIN_DUAL_APPROVAL_COLLUSION` referencia AC-2(7).

**Quality assessment: SOTA real** — addressed at all 3 layers (spec contract, invariant registry, error taxonomy). **Status**: CLOSED.

### H-09 Cost regression gate adoption — **PARTIALLY CLOSED**

§14.10 universal Lote 9.4 criado em meta-contract. Aplicado em sprints **S-01, S-02, S-03, S-04, S-05, S-06, S-14** (7 sprints). 

**Resíduo crítico**: H-09 R2 listou explicitamente S-07/S-08/S-09/S-10/S-14 como targets. **S-07/S-08/S-09/S-10 NÃO TÊM cost regression gate** em DoD nem em §9 Quality Standards (verificado via grep). S-14 tem 14.s14.8 BYOK overhead < 15% mas não em DoD §6 explicitly.

Pior: S-07 (eviction hot path), S-08 (rate limit hot path), S-09 (observability hot path = cardinality dictates Prom cost), S-10 (billing hot path) são exatamente os sprints que H-09 R2 chamava como pior cost surface. Não foram tocados.

**Status: PARTIALLY OPEN — escala como new finding H-N3-03**.

### H-10 Erasure 10-backend pseudonymization — **CLOSED**

S-11 lista 10 backends (D1 + Neon + R2 mutable + R2 audit + KV + DO + Loki + R2 cold logs + Stripe + Analytics Engine) com erasure-effective/pseudonymized split. PLANNED-specs.md `dsr_erasure_atomicity.tla` modelo bound = 10 backends. **Status**: CLOSED.

---

## Validation summary

| Finding | R2 status | R3 status | Quality |
|---|---|---|---|
| C-01 | CRITICAL | CLOSED | Real SOTA (ADR-0018 grounded NIST) |
| C-02 | CRITICAL | CLOSED | Solid; minor §1 enum gap |
| C-03 | CRITICAL | CLOSED | Solid; missing local_deltas YAML field |
| C-04 | CRITICAL | PARTIAL | Engineering/Launch split OK; timing aritmética frágil |
| C-05 | CRITICAL | PARTIAL | 10.s12.4 fixed; DoD line 142 ainda soft |
| H-01 | HIGH | CLOSED | PLANNED-specs.md = SOTA artifact |
| H-02 | HIGH | CLOSED | Live agg + offline batch elegant |
| H-03 | HIGH | CLOSED | error_taxonomy.md 56 errors |
| H-04 | HIGH | OPEN | timing-gates-cumulative.md not created |
| H-05 | HIGH | CLOSED | Consent revoke symmetric |
| H-06 | HIGH | CLOSED | Strong LGPD/GDPR alignment |
| H-07 | HIGH | OPEN | ADR-0016 stub não expandido |
| H-08 | HIGH | CLOSED | 3-layer fix (spec/registry/taxonomy) |
| H-09 | HIGH | PARTIAL | Aplicado 7 sprints; S-07-10 still gap |
| H-10 | HIGH | CLOSED | 10-backend mapping correto |

**Lote 9.4 effectiveness ≈ 75-80%** — strong execução em ~12/15 findings; 3 OPEN (H-04, H-07, parcial H-09 + C-04 + C-05).

---

## Novos findings R3

### CRITICAL (delay GA / pre-implementation block)

#### C-N3-01: Validator scripts não existem ainda — gate enforcement = honor system

**Sprints/files afetados**: meta-contract, all 21 sprint contracts, error_taxonomy.md, invariant_registry.md.

**Descrição**: Lote 9.4 introduz 3 CI gate scripts conceituais que **não existem em disk**:
1. `scripts/check_tla_obligations.py` (referenciado em `invariant_registry.md §4.4`).
2. `scripts/check_error_taxonomy.py` (referenciado em `error_taxonomy.md §7`).
3. `scripts/check_cost_regression.py` (implícito em §14.10 — não nomeado nem criado).
4. `scripts/validate_specs.py` exists em `scripts/` mas meta-contract:19 confessa "FROZEN ainda depende de…`scripts/validate_specs.py` rodando em CI (jsonschema dep installed)".

Mecanicamente, all 4 são pré-condições para **machine-enforceable** sprint contract validation. Sem eles, "obrigação" é prose policy — mesmo modo que disciplines pre-Lote 9.0 (`AP-019 rigor superficial em scope creep`).

**Impacto**: meta-contract status `REVIEW` (line 4) é **prematuro**. Real status seria `DRAFT (gate script blocked)`. Audit por Compliance officer/Architect rejeita; PR enforcement vai ser inconsistente; Codex r3 vai pegar.

**Sugestão**: Lote 9.5 priority 1 = criar 4 scripts (jsonschema validator + 3 check scripts) com tests. Após scripts verdes em CI, meta-contract pode promover REVIEW→FROZEN.

#### C-N3-02: Meta-contract REVIEW status com 0 reviewers humanos = governança circular

**Sprints/files afetados**: `_sprint_creation_contract.md` line 22 + framework `00_framework.md` line 19.

**Descrição**: meta-contract está em `doc_status: REVIEW` (Lote 9.4) com `Revisores: [Codex GPT (R1+R2 audit), Opus 4.7 (independent R2 audit)]` listed. **Reviewer humano ainda staffing-blocked**.

Problema epistemológico: **AI auditors auditando spec criada com input de AI**. Codex/Opus reviewing the meta-contract não é "review independente" no sentido governance — não há accountability humana, sem stake real, sem skin-in-game. Promoting `DRAFT → REVIEW` based on AI consensus é category-error.

Pior: meta-contract §11 line 263 define transition `DRAFT → REVIEW → FROZEN` e §22 line 22 confessa staffing-blocked humanos. A tabela §7.1 do framework distingue REVIEW como "em review formal" — REVIEW formal demanda human. Status atual viola sua própria taxonomia.

**Impacto**: SOC 2 Type I auditor rejeita governance trail; Compliance officer não pode sign-off PRR-GA-001 (S-20 R-S20-1) com meta-contract não-verificavelmente-revisado; pattern repete-se em key_management.md, invariant_registry.md, etc — todos `staffing-blocked`.

**Sugestão**: ou (a) criar reviewer role explicit "AI-assisted reviewer" como diferente de "human reviewer" no schema YAML; (b) revert meta-contract para `DRAFT (AI-reviewed)` até reviewer humano nomeado; (c) staffing decision documentada — quem é human reviewer? Quando? Com qual budget de tempo? 

Sem esse fix, S-20 GA gate é não-executável: PRR HIGH_RISK requires 10-12 sign-offs incluindo Compliance officer que não pode aprovar artifact governance-broken.

#### C-N3-03: S-02 e S-06 são iminent HIGH_RISK, mas spec_contract.md não tem sprint.md sibling — work_status incoerente

**Sprints/files afetados**: S-02, S-06 (e S-00, S-03, S-04, S-05).

**Descrição**: meta-contract §2 line 70 obriga estrutura:
```
specs/04_sprints/SXX/
├── _spec_contract.md
├── sprint.md          ← sprint contract full
├── work_items/
└── ...
```
Apenas S-01 tem `sprint.md` (verificado via `ls` — S-00, S-02, S-03, S-04, S-05, S-06 todos têm apenas `_spec_contract.md` + `work_items/`).

S-02 está marcado v1.1 SOTA com PERT 6 WIs, side-channel resistance, etc. Mas **spec_contract.md → sprint.md transition** está formalmente faltando. Meta-contract line 73 diz "sprint.md ← sprint contract full (template: _templates/sprint_contract.md)" — sem sprint.md, work_status do sprint é PROPOSED (não READY). Sprint não pode iniciar IN_PROGRESS sem sprint.md.

**Impacto**: User pretende "iniciar S-02 implementation"? S-02 não está READY do ponto de vista do meta-contract — é PROPOSED. WI-S02-001 não pode promover READY até sprint.md exist (transition rule §11 line 254-258). 

**Sugestão**: Lote 9.5 priority 2 = criar sprint.md para S-02 (template-fill from `_spec_contract.md` SOTA v1.1). Mesmo para S-06 antes de S-06 começar. Alternativa: relaxar meta-contract §2 para "sprint.md OR _spec_contract.md OK" — mas isso é regression.

#### C-N3-04: ADR-0019 menciona `local_deltas:` YAML field; nenhum sprint usa esse campo

**Sprints/files afetados**: ADR-0019:73, todos sprints.

**Descrição**: ADR-0019 line 73 diz "S-04 _spec_contract.md: adicionar `local_deltas: ["CAP-AC-004 TTL default semantics overridden by S-07 CAP-EVICT-002 (ADR-0019)"]`". Verificado via `grep "local_deltas" specs/04_sprints/S*/_spec_contract.md`: zero matches.

S-04 trata ADR-0019 via prose (line 65, 78, 103, 131, 150, 220). É fine para humanos mas **machine-readable validator não tem oracle** — script de cross-doc consistency check não pode confirmar "S-04 declarou override".

Pior: schema YAML em meta-contract §3 (line 86-105) não inclui `local_deltas` field. Nenhum schema atualmente captura override semantics.

**Impacto**: Lote 9.5 validator script não vai catch override drift; futuro S-04 vs S-07 silent divergence é re-runnable risk; M-N3-04 escalates se machine-readable.

**Sugestão**: 
1. ADR-0019 atualiza para reflitir realidade (prose-based) OR
2. Schema YAML adiciona `local_deltas: []` field opcional + cada sprint que tem override declara explicitly + jsonschema validator verifica + ADRs apontam para presence em sprint.

---

### HIGH (worth fixing pre-S-04 implementation)

#### H-N3-01: TLA+ obligation gate sem script é honor system

Já capturado em C-N3-01. Reforço HIGH-level: §4.4 obligation gate em registry diz "pre-S-20 GA gate, todo INV CRITICAL com `Status: PLANNED` deve transitar para `GREEN`" + "CI script `scripts/check_tla_obligations.py` (criar pós-Lote 9.4)". Sem script, "todo INV CRITICAL deve transitar GREEN" é honor system. S-10/S-11/S-13/S-14/S-19 sealing **não pode ser bloqueada** automaticamente — só via reviewer human noticing.

**Sugestão**: Lote 9.5 = `scripts/check_tla_obligations.py` antes de S-10 implementation start.

#### H-N3-02: Cumulative observation period gate calendarization missing

Tracked em H-04 R2. Não foi addressed.

S-06 ("30d staging sustained" pos-sprint observation, declarado como "concurrent com S-07/S-08"), S-09 ("Synthetic canary 24/7 sustentado 72h"), S-17 ("4 weeks chaos"), S-20 ("30d sustained staging") cumulativos formam calendar invisible. Sem `specs/04_sprints/timing-gates-cumulative.md`, real GA wall-clock é under-estimated.

**Cálculo R3**: Se S-19 SEAL = T0:
- S-20 30d staging start ideal = T0 (precisa começar antes do S-20 sprint start, senão impossível, ver C-04 R2).
- S-17 4-week chaos automation já terminou T0-X (varia).
- S-06 30d post-sprint observation: começou em S-06 SEAL (presumir T-180+ days?); coincide com prod load? Reading: "concurrent com S-07/S-08" sprints — S-07/S-08 são T-180+, então S-06 obs é **histórico** quando S-20 start. OK.
- S-09 72h synthetic em S-09 = T-90?

Conclusão: cumulative timeline pode resolver-se internamente, **mas só com canonical document**. Sem docs, é "trust me" — não auditable.

**Sugestão**: Lote 9.5 = `specs/04_sprints/timing-gates-cumulative.md` calendarizando 4-5 observation gates + dependencies + resolution.

#### H-N3-03: Cost regression gate ainda gap em S-07/S-08/S-09/S-10

Tracked em H-09 R2. Parcial fix (7 sprints aplicaram). Sprints faltantes:
- **S-07 (eviction hot path)** — eviction worker per-tenant per-blob cost decision; PR > 10% regression é critical surface.
- **S-08 (rate limit + abuse hot path)** — middleware overhead per request; cardinality risk + cost.
- **S-09 (observability hot path)** — cardinality budget = literal Prom cost; gate é o pontocentral; ironicamente missing.
- **S-10 (billing hot path)** — counters write per tenant per hour; Stripe API cost per invoice generation.

S-14 tem `14.s14.8 BYOK adds < 15% overhead` mas **não em DoD §6** explicit.

**Sugestão**: Lote 9.5 = adicionar `Cost regression gate` em DoD §6 + §9 Quality Standards desses 4 sprints. Ou seguir pattern de S-02:129/164 (DoD checkbox + Quality standard + Risk register entry).

#### H-N3-04: S-02 risk register R-S02-Risk-PAT-stub-divergence é placeholder; integration interface S-03↔S-02 não definido

**File**: `specs/04_sprints/S02/_spec_contract.md:240`.

**Descrição**: S-02 §15 risk row "PAT auth stub diverge de S-03 real" mitigation = "Stub interface frozen no Lote 9.4 contract; S-03 implementação respeita interface; integration test cobre."

Verifiquei: nenhum interface contract entre S-02 stub e S-03 real foi materialized em algum lugar (interface FROZEN não está em data_model.md, auth_model.md, ou em arquivo separate). Mitigation prose-based; sem oracle.

S-02 deps lista S-03 como "Soft blockers" (line 188) mas então S-02 vai ship com stub auth. Quando S-03 ship, interface drift é o exatamente padrão de bug que `inherits_from` vai detectar tarde demais.

**Impacto**: S-02 ship → S-03 ship → integration test breaks → rework. 1-2 weeks delay.

**Sugestão**: 
1. Pre-S-02 implementation: criar `specs/03_architecture/auth_stub_contract.md` (Nível 4) ou estender `auth_model.md §X` com explicit `TenantCtx` interface signature + stub vs real adapter pattern.
2. Add WI-S02-007 = "Define stub interface contract; integration test framework S-02↔S-03 boundary".

#### H-N3-05: Sprint S-00 ainda STANDARD lane mas afeta entire 12-month roadmap (blast radius mismatch)

**File**: `specs/04_sprints/S00/_spec_contract.md:38-39`.

**Descrição**: S-00 v1.1 mantém `Lane: STANDARD` com rationale "trabalho de planejamento não toca invariantes CRITICAL, mas tem blast radius alto (bad plan = meses de rework). Merece rigor STANDARD (5–8 sign-offs)".

Issue R3: framework §33.5.2 lane forcing factors definem HIGH_RISK por blast radius=organization OR reversibility=hybrid OR controle CRITICAL. S-00 explicitly tem "blast radius alto". Mas STANDARD permits 5-8 sign-offs sem Compliance/Privacy/Architect mandatory. PRFAQ pode incluir pricing, customer commitments (FF-HR-009 contract domain) — esses deveriam disparar HIGH_RISK.

V1.0 → v1.1 elevation foi quality bump (PERT, risk register 6 cols, Lote 9.4 references) mas **lane assignment é template-fill** — não foi reconsidered.

**Impacto**: PRFAQ pode incluir GA pricing claims que custom enforcing lateris S-10 billing terms. Anti-pattern AP-019 ("rigor superficial em scope creep") é exatamente o pattern — promoção lane sem real lane re-assignment.

**Sugestão**: revisitar S-00 lane post-PRFAQ draft. Se PRFAQ inclui pricing, FF-HR-009 ativa. Adicionar gate: "PRFAQ content review by Legal" pré-publication. Codex R1 mencionado lightly; precisar elevar.

#### H-N3-06: error_taxonomy.md schema não tem versionamento de error codes

**File**: `specs/03_architecture/error_taxonomy.md:55-72` (schema canônico).

**Descrição**: Schema canônico não inclui `version` field para error_code itself. Backwards-compat: §7 line 243 diz "nunca remover error_code; mark como `deprecated_in_sprint: S-XX` se obsoleto; remoção real após 12 meses (semver major bump)" — mas schema field para `deprecated_in_sprint` não existe na §2 schema.

Mais: error_code semantics podem evoluir (e.g., `COR_BILLING_OVERDUE` HTTP 402→403 se mudar interpretation OAuth-style). Sem schema versioning, customer SDK vai breaking change silently.

**Impacto**: SDK em 3 langs (S-15) vai não-coordinated; customer integration degrades.

**Sugestão**: schema canônico §2 adicionar:
```yaml
error_code: COR_<DOMAIN>_<SHORT>
schema_version: <integer>            # bump quando semantics change
introduced_in_sprint: S-XX           # already present
deprecated_in_sprint: null|S-YY      # add to schema
removed_in_sprint: null|S-ZZ         # add to schema
```

#### H-N3-07: TLA+ PLANNED-specs.md adversarial actions enumeration faltando 1 critical scenario per spec

**File**: `specs/tla/PLANNED-specs.md` per-spec adversarial actions.

**Descrição**: each PLANNED-specs.md adversarial actions é bom mas **incompleto** para SOTA standard:

1. **billing_atomicity.tla §1**: adversarial actions inclui `EventReplay`, `StripeOutage`, `ClockSkew`, `ConcurrentCounterUpdate`. **Faltando**: `RefundConcurrentInvoice` (Stripe webhook race; refund flow inverte sign) — refund/dispute = 30%+ de billing edge cases real.

2. **dsr_erasure_atomicity.tla §2**: faltando `ConsentRevocationDuringErasure` race (subject revoga consent durante erasure pipeline; 24h verification job pega state inconsistente).

3. **byok_sovereignty.tla §3**: faltando `RotationDuringRevoke` (CMK access revoked durante TDK rotation; rotation rollback contradição).

4. **onboarding_atomicity.tla §4**: faltando `DPAReSignDuringSubscriptionMigration` (DPA v1→v2 + plan upgrade race).

5. **key_lifecycle.tla §5**: faltando `EmergencyRotationOverlapBypass` (RB-KEY-COMPROMISE workflow vs INV-KEY-OVERLAP — emergency may need < 24h overlap).

**Impacto**: TLA+ specs sem these adversarial actions vão miss bugs em production. Cada adversarial scenario é ~10-15h additional effort; total gap = 50-75h adicional vs estimativa atual.

**Sugestão**: pré-S-10 implementation, atualizar PLANNED-specs.md com 1 adversarial scenario adicional per spec. Ou aceitar gap + tracked as known limitation.

#### H-N3-08: Sprint v1.1 elevation S-00..S-06 — quality é v1.1 nominal mas missing customer journey § (sprint contract não tem §3 Customer Impact)

**Files**: S-00..S-06 _spec_contract.md.

**Descrição**: meta-contract §4 line 128 obriga "§3 Customer Impact & Journey" para sprints STANDARD/HIGH_RISK. Verificado: S-00 _spec_contract.md não tem §3 explicit; S-01..S-06 _spec_contract.md também não.

Spec contract template é menor que sprint.md (sprint.md tem §3 mandatory). Mas elevation Lote 9.4 a "v1.1 SOTA" implica full SOTA quality — incluindo customer journey thinking (Working Backwards principle do framework §3).

S-02 elevation é quality bump em §16 SOTA benchmarks, §15 risk 6 cols, §17 references — mas customer journey ("um Bazel user faz `bazel build`; o que acontece em error_code COR_CAS_BLOB_NOT_FOUND? Como percebe?") não consta.

**Impacto**: Working Backwards é foundation principle; sprints elevados sem customer journey é template-fill em lugar wrong (rigor in benchmarks > rigor in customer-facing).

**Sugestão**: spec_contract.md template-fill (sprint.md ainda manda) é OK por agora. Mas se sprint.md vai ser criado para S-02 (per C-N3-03), MUST include §3. Lote 9.5 ou time of sprint.md creation.

---

### MEDIUM (nice-to-have)

#### M-N3-01: Registry §1 Regras de naming `<DOMAIN>` enum não inclui KEY

`invariant_registry.md:50` define `<DOMAIN> ∈ {TENANT, CAS, AC, GC, DATA, AUDIT, CONF, AVAIL, BILLING, SUPPLY}`. KEY foi adicionado em §3.13 mas enum §1 não atualizou. Lint/validator ainda falha em strict mode. Trivial fix.

#### M-N3-02: §3.12 PRODUCT entries continuam históricos; novos invariantes Lote 9.4 já em §3.12 mistura "sprint-driven" + product-driven em mesma seção

§3.11 PRODUCT (3 invariantes históricos) + §3.12 (Sprint-driven Lote 9.1 + Lote 9.4) usam taxonomia mixed. Pre-Lote 9.5: split em §3.11 Product (legacy) + §3.12 Sprint-driven CRITICAL/HIGH (com sprint_origin column) seria cleaner.

#### M-N3-03: PLANNED-specs.md §7 effort estimation = 70-100h; PERT-style decomposition seria mais grounded

PLANNED-specs.md §7 line 386 dá "12-18h" range per spec. Estimação otimista para someone novo a TLA+ (curve aprendizado real para Apalache/TLC). Pre-implementation pode beneficiar PERT (O/M/P): TLA+ effort em sprint owner que nunca escreveu spec = 30-50h realistic, não 12-18h.

#### M-N3-04: ADR-0019:73 wishful thinking — local_deltas: YAML não existe em schema

Tracked em C-N3-04. Minor recategoriza.

#### M-N3-05: error_taxonomy.md i18n strategy menciona pt-BR/es mas não pt-PT, fr (Quebec)

§1.4 line 47 "3 locales (en/pt-BR/es) at GA". Para enterprise multi-region (S-14), **pt-PT (Portugal)** + **fr (Quebec)** são frequently-needed. Add to roadmap.

#### M-N3-06: S-13 dual-approval property test 10k attempts inclui "collusion-rotation A→B then B→A"; mas spec não inclui timing window

S-13:84 diz "nas últimas 3 destructive ops em janela de 24h, deve haver ≥ 3 distinct approvers". Window 24h é arbitrary — de onde? NIST AC-2(7) não specifica window. Para safety, spec deve documenter justificação ("24h é window típico de incident; reduzir a 1h aumenta operational toil; aumentar a 7d permite slow-collusion").

#### M-N3-07: §14.10 universal cost regression gate menciona `$USD per million ops` mas não estado nominal — flutuação CF Workers pricing pode breakar dois meses depois

§14.10 line 245: "benchmark com per-op cost estimate em $USD por million ops; PR > 10% regression bloqueia merge". CF Workers pricing changes (Workers Paid plan: $5/10M req); $USD baseline floats sem versionamento. Precisar `$USD-per-million-ops at <vendor pricing date>`.

#### M-N3-08: Sprint S-00 §10 anti-scope falta "❌ Customer commitments" mas line 105 lista "❌ Customer commitments (PRFAQ é internal working backwards, não GTM)" — então OK; mas S-00:75 R-S00-3 success_metrics includes "5 tenants pagantes em produção" — isso É commitment

§5 R-S00-3 success metric "5 tenants pagantes em produção" implies commitment. Pode ser rebut argued como "internal target" mas Sales team pode treat as commitment se PRFAQ goes external.

#### M-N3-09: S-14 Schrems II TIA owner clarification — S-11 anti-scope "TIA pós-GA" vs S-14 entrega TIA pré-GA

`S-11:205` e `S-14:90` — anti-scope S-11 line 205 ainda diz "Schrems II TIA…pós-GA se EU tenants materializarem (anti-scope explícito)". S-14:90 entrega `legal/tia-template.md` em S-14 implementation. **Contradição persistente**.

Codex R2 já flaggou (CF-08 sample). Lote 9.4 não closed. Either S-11 anti-scope deve dizer "TIA owned by S-14" OR S-14 anti-scope move TIA delivery.

#### M-N3-10: Admin panel `admin.corelink.humangr.com` orphan ownership

S-16:177 "Admin panel operacional interno (CoreLink ops) — `admin.corelink.humangr.com` subdomain separado, não em S-16". Nenhum sprint cobre `admin.corelink.humangr.com`. Incident response oncall (S-17) precisa de quê pra ack/manage? S-13 admin plane API exists, mas UI não. Tracked em R2 M-09; ainda standing.

#### M-N3-11: PLANNED-specs.md menciona Apalache symbolic check pos-GA Q1, mas não documenta migration path TLC→Apalache

PLANNED-specs.md:398 "Pós-GA Q1 (S-21+ Fase 2), avaliar `Apalache`". OK como roadmap mas TLC→Apalache não é trivial — ações `sequence_of` semantics, `Action`s definitions diferent. Recomendação: PLANNED-specs.md §X "Apalache compatibility checklist" para each spec a priori.

---

## Pre-implementation gate check

### S-02 readiness — **PROCEED COM RESSALVAS BLOQUEANTES**

#### Material blockers

1. **C-N3-03**: S-02 não tem `sprint.md` — meta-contract §2 obriga; transition PROPOSED→READY bloqueado. **Bloqueante.**
2. **C-N3-01**: validator scripts não existem; spec/cross-ref machine-readable não enforced. **Bloqueante para CI gate.**
3. **H-N3-04**: Stub interface contract S-02↔S-03 não definido. **Bloqueante para integration test plan.**
4. **C-N3-02**: meta-contract REVIEW status circular; PRR sign-off Compliance officer não-grounded. **Bloqueante para PRR-S02.**

#### Pre-conditions met

1. ✅ S-02 v1.1 SOTA spec contract com PERT, 6 WIs, side-channel + client verify CRITICAL surfaces.
2. ✅ INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE em registry §3.12.
3. ✅ error_taxonomy.md cobre `COR_CAS_DIGEST_MISMATCH`, `COR_CAS_BLOB_NOT_FOUND`, etc — surface coverage match.
4. ✅ TLA+ tenant_isolation.tla GREEN; cas_integrity.tla GREEN — base verification ready.
5. ✅ Cost regression gate aplicado (DoD line 129 + §14.s02.7 + risk register).
6. ✅ Inherits_from minimum 5 sources (HIGH_RISK requirement).

#### Risk de iniciar implementation sem fix bloqueantes

- **Documentation drift**: implementation diverge silently de spec sem oracle CI.
- **Integration drift**: S-02 stub vs S-03 real interface evolve incompatible.
- **PRR review impossível**: Compliance officer + Privacy officer não-grounded sem reviewer humano.
- **WI quality compromised**: WIs derivados de spec contract puro (sem sprint.md) ficam órfãos.

#### WIs decomposable em 32-section HIGH_RISK pattern

Verificado WI-S01-001 (existing template HIGH_RISK pattern, 50+ lines header). WI-S02-001..006 antecipados em `_spec_contract.md §12` PERT — sub-tasks claros, scope tight, sprint timeline 3 weeks reasonable.

**Decomposable: SIM** mas requer:
1. C-N3-03 fix (sprint.md created) ANTES.
2. WI-S02-007 added: "Define stub interface contract S-02↔S-03 + integration test framework".

#### Recommendation S-02

**BLOCKED** até Lote 9.5 fix bloqueantes (C-N3-01, C-N3-03, H-N3-04, C-N3-02 partial).

Estimate Lote 9.5 effort para unblock S-02: **3-5 dias úteis** (validator scripts ~2d + sprint.md S-02 ~1d + stub interface contract ~1d + reviewer staffing decision ~0.5d).

---

### S-06 GC readiness — **PROCEED COM RESSALVAS NÃO-BLOQUEANTES**

#### gc_correctness.tla adequately verified pra prod?

Verificado `specs/tla/gc_correctness.tla` exists; .cfg exists. §4.1 registry diz "GREEN (Lote 5.13) — mark+sweep+grace+mark_started_at-aware".

**Deep check R3**: spec é written para small bound (Tenant=2, Blobs=4, AcEntries=2 typical TLC bound). Production = 1k tenants × 10M blobs × 100k AC entries. Bound expansion likely vai disclosure novos counterexamples (e.g., GC sob 100 concurrent UpdateActionResult em mesmo tenant). **Status**: spec é GREEN para small bound; **não é verified para production bound**. Industry standard accepts small-bound TLC verification + property test fills gap.

S-06 §6 DoD line 138 "Property test 100k race Mark+UpdateAR" cobre esse gap. **OK** mas não é "TLA+ verified for production scale".

#### 30d post-sprint observation period concurrent factível?

S-06:144 says "30d staging sustained (post-sprint observation period concurrent com S-07/S-08; documented em §13 timeline)". S-06 timeline says "Post-sprint observation: 30d concurrent S-07+ — gate liberation pré-S-20 GA".

**Aritmética R3**: S-06 SEAL = T0. S-07 implementation duration ≈ 3w + 5d buffer = ~25 days. S-07 Sprint SEAL = T+25. S-06 30d obs window = T+0 to T+30. Concurrent é possível **só se S-06 staging environment + S-07 dev environment não conflict** — i.e., S-07 não modifies blob_meta/ac_meta schema during S-06 obs.

Verificado S-07: S-07 R-S07-3 says "atomic D1 UPDATE ou DO singleton batch" para blob_meta last_accessed_at. Esse field exists em S-01 schema. S-07 não cria new column? Verificar: S-07 não reescreve blob_meta schema — só adds eviction worker. Compatível com S-06 obs concurrent.

**Status**: Factível.

#### INV-GC-004 mark-phase-aware re-ref property test plan claro?

S-06:138 "Property test 100k: race Mark + UpdateActionResult interleavings → INV-GC-004 0 violations". S-06:148 "10.s06.6 Mark-phase-aware re-ref protection verified em property test 100k iter (INV-GC-004)".

**Quality assessment**: spec is concrete. Implementation strategy via `proptest`/`quickcheck` with strict `<` vs `>=` comparison + `mark_started_at` strict timestamp typing.

**Status**: Plan claro.

#### Material blockers S-06

Same as S-02:
1. **C-N3-03**: S-06 não tem sprint.md.
2. **C-N3-01**: validator scripts.
3. **C-N3-02**: meta-contract REVIEW circular.
4. Adicional: S-06 has hard blocker "S-04 SEALED". S-04 não start ainda. S-06 está well downstream (T-90+ days). Mais tempo para Lote 9.5+ remediation.

#### Recommendation S-06

**PROCEED CONDICIONAL** — material blockers gerais, mas timeline (S-06 só após S-04 SEALED) dá tempo para fix Lote 9.5 e até Lote 9.6+ se necessário. S-06 specific quality é HIGH; spec contract é production-grade.

---

## Strategic recommendations Lote 9.5+

### S-1: Validator scripts criação (PRIORITY 1 — desbloqueia tudo)

3-4 scripts em `scripts/`:
1. `validate_specs.py` — jsonschema YAML front matter validation.
2. `validate_references.py` — cross-doc reference validation (já mencioned).
3. `check_tla_obligations.py` — verifica `invariant_registry §4.2` PLANNED → GREEN status; bloqueia PR se sprint touches CRITICAL invariant TLA+.
4. `check_error_taxonomy.py` — verifica error code in catalog vs SDK exception in code vs docs i18n vs error_codes referenciados em sprint contracts.

Total: ~10-15h dev work. **Bloqueia meta-contract REVIEW→FROZEN, bloqueia S-02 IN_PROGRESS.**

### S-2: Reviewer staffing humano strategy decision

**Não pode ser deferido novamente.** Opções:
1. **AI-assisted reviewer role**: meta-contract reviewer YAML field aceita "ai-assisted-{model}" + "human" tags. Validates human ≥ 1 nomeado para FROZEN promotion.
2. **External contractor**: hire 1-2 senior architects para 5-10h/week review duty (Lote 9.5 → end of Phase 1).
3. **Document review-by-PR pattern**: each PR has rev-by-2 (1 owner + 1 peer); accumulating across PRs constituting "review" via audit trail.

Decision needed pre-S-02. Without, all `staffing-blocked` artifacts stay DRAFT — meaning **GA gate is não-issuable**.

### S-3: Sprint.md creation para S-02 (e S-06 quando pre-S-04 SEAL)

S-02 sprint.md template-fill from S-02 _spec_contract.md SOTA v1.1; add §3 Customer Impact & Journey; total ~6-10h.

### S-4: Stub interface contract S-02↔S-03

Endereçar H-N3-04. Criar `specs/03_architecture/auth_stub_contract.md` ou estender `auth_model.md §X`. Define `TenantCtx` interface signature; stub vs real adapter pattern. ~3-5h.

### S-5: Cost regression gate aplicação em S-07/S-08/S-09/S-10

H-N3-03 fix: 4 sprints adicionar DoD §6 + §9 Quality Standards + §15 Risk Register entries. Pattern from S-02:129/164/244 já estabilizado. ~2-3h per sprint = 8-12h total.

### S-6: Apalache symbolic check timing decision

PLANNED-specs.md menciona pos-GA Q1. R3 recommendation: **decide agora**:
- TLC apenas em Lote 9.5 (cheap; vai escalar OK em small bounds para algorithmic correctness).
- Apalache evaluation em S-21 (Phase 2 start) — formal POC, não yet GA gate.

### S-7: Timing-gates cumulative document

Endereçar H-N3-02. Criar `specs/04_sprints/timing-gates-cumulative.md` calendarizando 5 observation gates (S-06, S-09, S-17, S-19, S-20) + dependencies + resolution table. ~5h.

### S-8: TLA+ adversarial actions enrichment

H-N3-07 fix: PLANNED-specs.md atualizar com 1 critical adversarial scenario adicional per spec (5 specs × 1 scenario = 5 scenarios). ~3h doc work; +50-75h spec-time effort para sprint owners (transparent in §7 effort estimation).

### S-9: Schema YAML evolução

Adicionar fields:
- `local_deltas: []` (override declarations).
- `deprecated_in_sprint: null|S-XX` (in error_taxonomy).
- `removed_in_sprint: null|S-YY`.

Tied to S-1 validator scripts.

### S-10: Reviewer rota plan + staffing budget

Pos-S-2: documenter who reviews what when. Without, every doc keeps "staffing-blocked" status indefinitely.

---

## Veredito R3

### SOTA score atual

**8.0/10** vs R2 7.4/10 vs R1 codex 6.8/10.

**Driver upgrade**:
- ADRs 0018/0019/0020 = real SOTA decisions (não template).
- error_taxonomy.md 56 errors = canonical artifact strong.
- PLANNED-specs.md 450+ linhas = blueprint TLA+ industry-leading.
- 21 sprint contracts em v1.1; cumulativo Lote 9.4 efforts diluem residual issues.

**Driver attenuation**:
- 4 validator scripts não existem (gate enforcement = honor system).
- meta-contract REVIEW status circular (AI-only review).
- 4 sprint contracts iminentes não têm sprint.md (S-02, S-06 pelo menos, mais S-00/S-03/S-04/S-05).
- Cost regression gate ainda gap em 4 hot-path sprints (S-07/8/9/10).
- C-04 e C-05 são PARTIAL closed (não FULL closed).

### Lote 9.4 remediation effectiveness

**~75-80%**: 12/15 R2 findings closed; 3 OPEN ou PARTIAL. Quality dos closed é HIGH (real SOTA, não template-fill).

### Implementação S-02 pronta?

**NÃO — com 3-5 dias de Lote 9.5 work bloqueante:**
1. Criar 4 validator scripts (~10-15h).
2. Sprint.md S-02 (~6-10h).
3. Stub interface contract S-02↔S-03 (~3-5h).
4. Reviewer staffing decision (~0.5d).
5. (Pós-S-02 unblock): Lote 9.5b para H-N3-03 cost gate + H-N3-02 timing-gates doc + H-N3-07 adversarial enrichment.

### Implementação S-06 pronta?

**SIM com ressalvas não-bloqueantes** — S-06 SEAL date é T+90+ dias; Lote 9.5+ tem tempo para resolution antes.

### Próximo lote prioritário Lote 9.5

**Lote 9.5 — Pre-S-02 Implementation Hardening (3-5 dias)**:

1. **CRITICAL**: validator scripts (`validate_specs.py`, `check_tla_obligations.py`, `check_error_taxonomy.py`, `check_cost_regression.py`) — 10-15h.
2. **CRITICAL**: sprint.md S-02 criação — 6-10h.
3. **HIGH**: stub interface contract S-02↔S-03 — 3-5h.
4. **HIGH**: reviewer staffing decision (formal documentation) — 4h.
5. **MEDIUM**: cost regression gate em S-07/S-08/S-09/S-10 — 8-12h.
6. **MEDIUM**: timing-gates-cumulative.md — 5h.
7. **MEDIUM**: registry §1 KEY domain enum fix — 0.5h.
8. **MEDIUM**: M-N3-04 (S-04 local_deltas decision) — 1h.
9. **LOW**: TLA+ adversarial actions enrichment in PLANNED-specs.md — 3h.

**Total**: ~40-55h ≈ 5-7 dias úteis. Após Lote 9.5, S-02 implementation pode safely começar.

**Lote 9.6 (post-S-02 implementation)**: H-N3-08 customer journey § enrichment para S-00..S-06 spec contracts; M-N3-09 Schrems II TIA cross-sprint reconciliation; M-N3-10 admin.corelink.humangr.com ownership decision.

---

**Fim Opus Independent Round 3 Audit.** Findings independentes — minimal overlap with codex r3 (não li seu output). Foco em validation R2 + Lote 9.4 regression check + pre-implementation gate. Recommendation executável: Lote 9.5 antes de S-02 implementation.
