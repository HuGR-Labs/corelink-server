# Self-Audit Lote 2 — Claude Opus 4.7 (modo brutal)

> **Contexto:** Codex/GPT bateu limite de uso às 14:14. Gustavo autorizou eu auditar por conta própria, com rigor GPT-equivalente. Modo: caçar furos, não pintar ✅.
> **Alvo:** Lote 2 completo (v0.4.0, 5 sub-commits bd9b7f8 a d85b596).

## 1. Veredicto global

Lote 2 entrega **substância real** em todos os 5 sub-lotes declarados — 1320 linhas adicionadas, 4 seções novas no framework, 1 schema JSON funcional, 1 template novo, 1 script de CI executável. O problema não é "não entregou"; é "entregou sem **integrar**". Os 5 patches foram focados cada um no seu escopo, mas **nenhum voltou pra atualizar o meta-framework** (TOC, §6.1 registry, §7.11 schema table, glossário §40, hierarquia §4). Resultado: 4 seções novas órfãs no sumário, 3 formatos de ID novos fora do registry, 4+ campos de schema declarados no JSON mas não no framework, ~8 termos canônicos novos fora do glossário. **Score honesto: 5/5 sub-lotes com entrega substancial; 0/5 totalmente integrados. 18 findings reais.**

## 2. Status dos 5 sub-lotes declarados

### 2.1 JSON Schema + validate_specs.py

- **Status: RESOLVIDO (substantivamente)** — schema funcional draft 2020-12, script com 2 camadas (YAML + Schema), valida CI-ready.
- **Gaps (ver §3):** schema não declara `lane_forcing_factors`, referenciado como obrigatório na §33.5.5.

### 2.2 Risk Lanes

- **Status: RESOLVIDO (substantivamente)** — §33.5 completa com 8 subseções, scoring de 5 dimensões, 10 forcing factors, matriz de gates/sign-offs por lane, regras REG-LANE.
- **Gaps:** seção órfã no TOC; campo `lane` em schema não é obrigatório apesar da REG-LANE-001 dizer "DEVE"; `lane_forcing_factors` descrito na prosa mas ausente do schema; sub-task template declara `lane: "LOW_RISK"` default enquanto framework §33.5 não restringe ST.

### 2.3 Control Inheritance

- **Status: RESOLVIDO (substantivamente)** — §35.5 com catálogo de 15 fontes canônicas, semântica de herança, regras de local delta.
- **Gaps:** seção órfã no TOC; campos `inherits_from` e `local_deltas` existem no schema JSON mas NÃO aparecem em §7.11.1 (tabela canônica de campos obrigatórios); hierarchy §4 não menciona onde waivers/inheritance encaixam.

### 2.4 Waivers

- **Status: RESOLVIDO (substantivamente)** — template waiver.md com 11 seções, §35.6 com 8 subseções, regras REG-WAIVER.
- **Gaps:** formato `WAIVER-YYYYMMDD-NNN` fora de §6.1 (Formatos canônicos) = viola PRINC-006; seção órfã no TOC; diretório `_waivers/` fora da hierarquia §4; regra §35.6.4 (caveat PRR → waiver) sem mecanismo operacional de detecção.

### 2.5 Evidence Taxonomy

- **Status: RESOLVIDO (substantivamente)** — 24 tipos EVT-XXX formalizados, mapping gate→evidence, bucket R2 estruturado, 7 anti-patterns.
- **Gaps:** formato `EVT-XXX` fora de §6.1 = viola PRINC-006; seção órfã no TOC; regra EVT-014 DASHBOARD_URL "não suficiente sozinho" não é enforçada por schema/CI; relação com control inheritance (§35.5) não especificada.

## 3. Findings detalhados (18 reais)

### 🔴 CRITICAL (4 findings — bloqueiam green light honesto)

**F-01 — TOC (Sumário) não atualizado.** `specs/00_framework.md:20-60` lista seções 0-43 mas não cita §33.5, §35.5, §35.6, §35.7. Quatro seções novas órfãs, navegação do framework quebrada. Usuário que abre o framework pelo TOC não descobre risk lanes, inheritance, waivers, evidence taxonomy. **Fix:** adicionar entradas ao TOC.

**F-02 — `lane_forcing_factors` falta no schema.** Framework §33.5.5 (linha ~1960) declara REG-LANE-003: "HIGH_RISK **DEVE** ser acompanhado de campo `lane_forcing_factors`". Schema `front_matter.schema.json` NÃO declara o campo nem regra cross-field. WI com `lane: HIGH_RISK` sem forcing_factors passa no CI silenciosamente. **Fix:** adicionar ao schema + rule `if lane=HIGH_RISK then required: [lane_forcing_factors]`.

**F-03 — Campo `lane` não é obrigatório no schema apesar de REG-LANE-001.** Framework §33.5.5 REG-LANE-001: "Todo Sprint, WI e ST **DEVE** ter campo YAML `lane`". Schema JSON permite omitir. Regra inviolável em prosa, não enforçada pela automação que promete enforçá-la. **Fix:** mover `lane` para `required` em sprint/work_item/sub_task no schema.

**F-04 — §6.1 (Formatos canônicos de ID) não atualizado com 3 formatos novos.** Lote 2 introduziu:
- `EVT-XXX` (§35.7.1)
- `WAIVER-YYYYMMDD-NNN` (§35.6.3 REG-WAIVER-011)
- `FF-HR-NNN` (§33.5.3)

Nenhum entra na tabela §6.1 (PRINC-006 exige numeração estável registrada). REG-NUM-004 manda reservar IDs antes — como reservar se o namespace não existe? **Fix:** adicionar 3 linhas em §6.1.

### 🟠 HIGH (6 findings — degradam qualidade mas não bloqueiam)

**F-05 — Glossário §40 não atualizado com ~10 termos novos.** §40.9 Misc ignora: `lane`, `forcing factor`, `control inheritance`, `inherits_from`, `local_deltas`, `waiver`, `compensating control`, `revalidation trigger`, `evidence taxonomy`, `EVT type`. Viola PRINC-017 (Ubiquitous Language). **Fix:** adicionar ~10 entradas em §40.9 (ou criar §40.10 "Lote 2 vocabulário").

**F-06 — Campos `inherits_from`/`local_deltas` existem no schema mas ausentes de §7.11.1/§7.11.2.** Tabela de campos canônicos obrigatórios lista apenas 13 campos. Schema tem 20+. Quem lê o framework não sabe que `inherits_from` existe. **Fix:** adicionar linhas em §7.11.1 (opcionais, documentar).

**F-07 — Nenhuma nova PRINC criada apesar de mudanças fundacionais.** Lote 2 adiciona ~900 linhas de regras novas de controle (lanes, inheritance, waivers, evidence). Nenhuma entrou como PRINC-035+. Princípios fundamentais existentes (PRINC-001..034) não capturam o espírito das novas regras. Ex: candidatos:
- PRINC-035: "Trabalho é classificado por risco; cerimônia é proporcional à lane."
- PRINC-036: "Controles cross-cutting têm fonte canônica única; duplicação é drift."
- PRINC-037: "Exceção formal (waiver) tem expires_at + compensating control obrigatórios."
- PRINC-038: "Evidence é tipada; 'dashboard URL' sozinho não é evidence."

Sem elevar a princípios, as novas regras ficam como "seções do framework" sem status de inviolável. **Fix:** 3-4 PRINC novas em §2.

**F-08 — Hierarquia §4 não menciona waivers nem evidence bucket.** Nível 1-5 + fontes canônicas de §35.5 formam a estrutura. `specs/_waivers/` e `specs/_schemas/` são diretórios novos, transversais, sem posição no mapa. Hierarchy diz "5 níveis"; na prática são 5 níveis + schemas + waivers + templates + audits. **Fix:** adicionar seção §4.7 "Artefatos transversais" listando `_schemas/`, `_templates/`, `_waivers/`, `_audits/`, `_archive/`.

**F-09 — Sign-off matrix §33.5.4.3 vs template WI §30.** Matriz diz HIGH_RISK = 10-12 papéis, alguns condicionais (🟡). Template WI §30 ainda lista 12 papéis como obrigatórios sem condicional. Cross-check incompleto. Sprint/WI/ST templates precisam ser updated pra refletir matriz de lanes. **Fix:** anotar condicionais no template OU adicionar nota "ver §33.5.4.3 do framework para aplicabilidade por lane". (Lote 3 planejado fazer isso, mas cria drift temporário.)

**F-10 — EVT-014 DASHBOARD_URL "não suficiente" não é enforçada.** Regra crítica em §35.7.2 REG-EVID-002 mas só existe em prosa. Schema/CI não detecta. Reviewer cansado aceita URL isolada. Automation-over-documentation (PRINC-026) violada. **Fix:** `scripts/validate_evidence.py` (planejada §35.7.5) precisa implementar esta regra, OU regex em `scripts/validate_specs.py` pra flag evidence suspeita.

### 🟡 MEDIUM (5 findings — cosméticos/operacionais)

**F-11 — Scripts citados como planejados sem ADR/trace.** §33.5.7 `classify_lane.py`, §35.6.6 `check_waivers.py`, §35.7.5 `validate_evidence.py`. 3 scripts prometidos sem WI/lote associado. Prometer sem rastreamento é dívida silenciosa. **Fix:** adicionar a backlog (Lote 4 ou 5), citar lote no framework.

**F-12 — REG-LANE-013 vago.** "Mudança de lane **DEVE** disparar re-verificação dos gates adicionais." Quem dispara? Como? CI? Humano? Processo indefinido. **Fix:** especificar trigger (ex: "PR change to `lane:` field triggers CI job `re-verify-lane-gates`").

**F-13 — Regra `§35.6.4 caveat PRR → waiver` sem mecanismo de detecção.** "Quando caveat de PRR se repete em ≥ 2 PRRs diferentes, **DEVE** ser promovido pra waiver." Sem query, sem CI check, sem alert. Dead rule. **Fix:** script que compara caveats entre PRRs ou anotar como backlog.

**F-14 — Precedência herança vs waiver não definida.** Se WI `inherits_from` observability_model.md E tem waiver ativo sobre regra específica, qual vence? Framework não diz. **Fix:** adicionar §35.5.7 ou §35.6.9 com regra de precedência.

**F-15 — Waiver reviewers sem enum de roles.** Template `waiver.md` tem `role: "gate_owner"`, `role: "security"`, `role: "compliance"` — strings livres. Schema aceita qualquer string. Drift futuro garantido (alguém vai escrever `"Security"` vs `"security"` vs `"sec"`). **Fix:** definir enum de roles no schema OU adicionar ao glossário.

### 🟢 LOW (3 findings — nice-to-have)

**F-16 — Sections ordering `§33.5` cosmetic.** §33.5 sits entre §33 (Tech Debt) e §34 (Templates) sem afetar numeração, mas quebra fluxo visual. Poderia ser §33.5 renomeado pra §33.1 (filho de §33) ou promovido a §34.0. **Fix:** opcional.

**F-17 — Waiver template exemplo "REG-XXX-001" pode ser confundido com placeholder.** Reviewer iniciante pode deixar literal. **Fix:** template com exemplo real (ex: "REG-SEC-003") ou nota explícita "substitua por ID real do framework".

**F-18 — `_schemas/` folder não versiona schema.** `front_matter.schema.json` sem versioning próprio. Se o schema muda breaking, quem usou antes fica desalinhado. **Fix:** considerar `$id` incluindo versão, ou manter changelog separado do schema.

## 4. Regressões vs Lote 1 v0.3.4

Rodei `scripts/validate_specs.py`: passa. Lote 1 core (framework + 5 templates existentes) não quebrou. Zero regressão funcional detectada.

Pequenos *side effects*:
- Mudança em template WI (linhas 7-9 adicionando `lane` e `lane_forcing_factors` comentado) é benigna mas amplia o YAML.
- ADR/PRR templates **não** ganharam campo `lane` — **por design** (ADR não tem lane; PRR não é work unit mas sim gate). OK.

## 5. Gaps residuais vs SOTA (audit v1 backlog)

Do audit v1 original, Lote 2 resolve:

- ✅ Gap 3 (automação promet.): JSON Schema real
- ✅ Gap 4 (sem risk lanes): §33.5
- ✅ Gap 5 (sem inheritance + waiver): §35.5 + §35.6
- ✅ §5 (evidence sem taxonomia): §35.7
- ✅ §6 (redundâncias): inheritance resolve arquiteturalmente — **mas aplicação aos templates é Lote 3**

Ainda abertos (fora do escopo do Lote 2, reservados):

- Gap 1 (Storage Semantics Matrix Cloudflare): **Lote 4**
- Gap 2 (Remote Cache Product Profile): **Lote 4**
- Gap 6 (Compatibility Matrix por protocolo): **Lote 4**
- Gap 7 (Auth Model estruturado): **Lote 4**
- Gap 8 (SLOs user-perceived): **Lote 4**
- Gap 9 (Formal GC Model): **Lote 4**
- `tenant_id` em métricas (audit v1 §5): **Lote 5**
- Framework `DRAFT` self-contradiction: **Lote 6**

Tudo conforme roadmap declarado.

## 6. Over-engineering detectado?

Sim, em partes. Honestidade brutal:

- **§33.5 matriz de gates (33×3 células)**: alta densidade de informação, mas útil. Não é overengineering.
- **§35.6 waiver template 11 seções**: para exceção formal com rastreabilidade legal/compliance, 11 seções é proporcional. Não é over.
- **§35.7 24 tipos de EVT**: alguns com overlap sutil (ex: EVT-005 SAST vs EVT-006 DAST vs EVT-007 DEPENDENCY_SCAN poderiam ser 1 tipo "SCAN" com subtype). Dá pra consolidar. **Minor over.**
- **§33.5.8 auditoria trimestral de lanes** + §35.6.7 auditoria trimestral de waivers + §35.5.4 validação CI de inherits_from: 3 rotinas trimestrais novas. Pra framework de spec com 1 fundador hoje, é pesado. Mas é a natureza do SOTA — aceitável se automação amadurecer.

Verdadeiro overengineering: nenhum crítico. Mas framework cresceu de 2600 pra 3100+ linhas. Próximo lote não deveria adicionar seções novas — deveria **reduzir via inheritance** (Lote 3 é exatamente isso).

## 7. Recomendação: AVANÇAR PARA LOTE 3? (SIM / NÃO / SIM COM CAVEATS)

**SIM COM CAVEATS.**

Lote 2 **entregou substância real** em todos os sub-lotes. Mas 4 findings CRITICAL + 6 HIGH = não pode ser chamado "fechado" sem micro-patch.

**Opção A (honesta):** Lote 2-bis endereçando F-01 a F-10 antes de Lote 3.
- F-01 a F-04: estruturais, obrigatórios (TOC, §6.1, schema hole).
- F-05 a F-10: glossário, PRINC, hierarchy, matrix cross-check, EVT-014 enforcement, campos schema. Importantes mas podem entrar parcialmente com Lote 3.

**Opção B (pragmática):** Combinar correções residuais com Lote 3 (que vai mexer em templates de qualquer jeito). Lote 3 = "apply inheritance to templates + close Lote 2 audit gaps". Um único commit bloco.

**Opção C (GPT posterior):** Quando GPT liberar, rodar review formal dele sobre v0.4.0. Se concordar com meus 18 findings, faz 2-bis. Se discordar de alguns, negociar.

**Minha recomendação sincera: Opção B.** Lote 3 naturalmente mexe nos templates (pra substituir seções duplicadas por `inherits_from`), é o momento ótimo pra também atualizar TOC, §6.1, glossário, PRINC, schema gaps, cross-reference matrices. Evita Lote 2-bis → 2-ter → 2-quater que vimos acontecer no Lote 1. Fecha tudo num lote maior.

## 8. Scorecard

| Sub-lote | Entrega declarada | Gaps HIGH/CRIT | Score |
|---|---|---|---|
| 2.1 JSON Schema | ✅ funcional | F-02, F-03, F-06 | 🟡 substancial mas incompleto |
| 2.2 Risk Lanes | ✅ seções novas | F-01, F-02, F-03, F-07, F-09 | 🟡 substancial mas incompleto |
| 2.3 Control Inheritance | ✅ catálogo | F-01, F-06, F-08, F-14 | 🟡 substancial mas incompleto |
| 2.4 Waivers | ✅ template+seção | F-01, F-04, F-08, F-14, F-15 | 🟡 substancial mas incompleto |
| 2.5 Evidence Taxonomy | ✅ 24 tipos | F-01, F-04, F-07, F-10 | 🟡 substancial mas incompleto |

**Pattern comum em todos os 5:** seção nova + lógica correta + **ausência de propagação pro meta** (TOC/registry/glossário/PRINC/schema).

## 9. Diferença vs audit GPT hipotético

Se GPT tivesse rodado, provável concordância em 14-16/18 findings. Onde eu poderia estar menos severo que GPT:

- F-05 (glossário): GPT costuma catar mais termos.
- F-07 (PRINC): GPT poderia exigir mais princípios novos.
- Possíveis findings que eu não vi: consistência interna entre §33.5.4 WI matrix e ST matrix (talvez ST matrix em §33.5.4.2 seja mais rasa do que deveria).

Recomendação: quando GPT voltar, rodar o review formal sobre v0.4.0. Se confirmar ≥ 80% destes findings, proceder com Opção B (Lote 3 fecha Lote 2 residuals).

---

## Apêndice A: Método de auditoria

Rodei:
1. `git log --oneline 64e366d..HEAD` — verificar 5 commits.
2. `git diff 64e366d..HEAD --stat` — escopo de mudanças.
3. Leitura crítica de: TOC, §4 hierarchy, §6.1 numeração, §7.11 schema, §33.5, §35.5, §35.6, §35.7, §40 glossário.
4. Leitura crítica de: `_schemas/front_matter.schema.json`, `_templates/waiver.md`, `_templates/work_item.md` diff, `scripts/validate_specs.py`.
5. Execução: `python3 scripts/validate_specs.py` (pass), `--strict` (falha esperada em templates).
6. Cross-reference de regras inter-seção (REG-LANE-003 vs schema, REG-WAIVER-005 vs §6.1, etc).

## Apêndice B: Meu viés conhecido

Como revisor = autor, meus blind spots prováveis:
- **Subestimar complexidade**: 18 findings pode subestimar; GPT talvez ache 22-25.
- **Sobrestimar clareza**: algo que pareceu claro pra mim enquanto escrevia pode ser opaco pra leitor novo.
- **Aceitar dívida futura**: eu escrevi "scripts planejados"; revisor externo cobraria prazo.

Por isso a recomendação final sugere validação GPT formal quando liberado.

---

**Fim do self-audit.**
