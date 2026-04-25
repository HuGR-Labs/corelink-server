---
id: AUDIT-SPRINT-CONTRACTS-SOTA-R3
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.3.0
created: 2026-04-25
reviewers: [GPT via codex CLI — SOTA parceiro round 3]
supersedes: AUDIT-SPRINT-CONTRACTS-SOTA-R2
superseded_by: null
tags: [audit, sota, sprint-contracts, round-3, lote-9.4-validation, pre-implementation-gate]
---

# Codex SOTA Review Round 3 — pós-Lote 9.4 (Pre-Implementation Gate)

## Escopo
- Re-audit pós-remediação Lote 9.4 (commits `88cb3bf`, `f55f654`, `e23925f`, `f1c8382`).
- Validar claims de fechamento dos findings do Codex R2 (`CF-01..08`, `R2-01..25`) e dos críticos do Opus R2 (`C-01..C-05`).
- Verificar se o uplift `S-00..S-06` é estrutural ou cosmético.
- Reavaliar `error_taxonomy.md`, matriz de obrigações TLA+, runbooks expandidos e promoção do meta-contract para `REVIEW`.
- Fazer pre-implementation gate de `S-02` e registrar novos findings R3.

## Validação Lote 9.4 remediation

| Finding origem | Status R3 | File:line evidence |
|---|---|---|
| Opus C-01/C-02 `INV-KEY-*` | ✅ | `specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:17-41`; `specs/03_architecture/key_management.md:97-115`; `specs/03_architecture/invariant_registry.md:181-193` |
| Codex CF-05 / CF-06 ownership TTL/quota | ✅ | `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:30-52`; `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:28-50`; `specs/04_sprints/S04/_spec_contract.md:65-78`; `specs/04_sprints/S07/_spec_contract.md:64-65`; `specs/04_sprints/S08/_spec_contract.md:62-64` |
| Cluster `S-00..S-06` v1.1 | ✅ | `version: "1.1.0"` em `S00..S06`; exemplos: `specs/04_sprints/S00/_spec_contract.md:6`, `S02:6`, `S06:6`; PERT explícito em `S02:197-208`, `S03:177-190`, `S06:207-219`; risk register 6 colunas em `S00:143-149`, `S02:232-244`, `S06:230-244` |
| EVT uplift `S-00..S-06` | ✅ | Recontagem atual do DoD: `83/86` itens com `EVT-*` (`96.5%`), versus `16/52` no R2; evidência representativa em `S00:69-76`, `S02:117-129`, `S06:126-139` |
| Runbooks críticos antes em stub | ✅ | `specs/05_quality/runbooks/RB-BILLING-001.md:21-136`; `RB-BILLING-002.md:21-139`; `RB-DSR-ERASURE-INCOMPLETE.md:21-191`; `RB-DATA-RESIDENCY-LEAK.md:21-198`; `RB-FM-105-region-replication-diverge.md:21-142`; `RB-FM-SIGNUP-FAILED.md:21-174` |
| TLA+ obligation matrix expandida | ⚠️ | `specs/03_architecture/invariant_registry.md:197-247`; `specs/tla/PLANNED-specs.md:17-24`, `:350-399` |
| `error_taxonomy.md` adicionado | ⚠️ | `specs/03_architecture/error_taxonomy.md:17-25`, `:75-165`, `:239-257` |
| Meta-contract `DRAFT -> REVIEW` | ✅ | `specs/04_sprints/_sprint_creation_contract.md:4-6`, `:19-22` |
| Cross-ref validator | ✅ | `python3 scripts/validate_references.py --json` retornou `dangling: {}` |
| Schema validator executável | ❌ | `python3 scripts/validate_specs.py` ainda aborta com `ERRO: jsonschema não instalado` |

## Coverage check categorical

> Convenção deste relatório: referências curtas como `S13:88-92` significam `specs/04_sprints/S13/_spec_contract.md:88-92`.

### Opus R2 critical (5 findings)

| ID | Status R3 | Notes |
|---|---|---|
| C-01 `INV-KEY-OVERLAP` triple-value | ✅ | Resolvido por ADR per-asset e propagação ao canonical + registry: `ADR-0018:17-41`, `key_management.md:101-115`, `invariant_registry.md:185-193`, `S13:88-92`, `S14:150-167`. |
| C-02 `INV-KEY-*` fora do registry | ✅ | Domain `KEY` agora existe em `invariant_registry.md:181-193`. |
| C-03 TTL `S-04` vs `S-07` contraditório | ✅ | A contradição semântica foi explicitamente resolvida: `ADR-0019:30-52`, `S04:65-78`, `S07:64-65`. Residual novo R3: migration plan ainda está só na ADR. |
| C-04 S-20 timing crunch impossível | ❌ | O split engineering/launch ajudou, mas a aritmética segue impossível: `S20:220-239` ainda encaixa `30d staging`, `30d lighthouse`, `2w pentest + 1w retest` em `D+30`. |
| C-05 `S-12` DoD não-binário | ✅ | O branch `OU documented sources` saiu do gate principal; agora o contrato exige `AND ADR-0015 ratificado AND documentação` em `S12:142-153`. Residual R3: claim de reproducibility continua abaixo de `100% bit-identical`, mas o critical do Opus foi endereçado. |

### Codex R2 carry-forward (CF-01..CF-08)

| ID | Status R3 | Notes |
|---|---|---|
| CF-01 meta-contract abaixo da própria autoridade | ✅ | `DRAFT` saiu; agora está `REVIEW` com blockers explícitos em `_sprint_creation_contract.md:19-22`. |
| CF-02 `S-00..S-06` fora de v1.1 | ✅ | Todos os 21 `_spec_contract.md` estão em `1.1.0`; o cluster legado foi elevado em `S00..S06`. |
| CF-03 gap estrutural entre clusters | ✅ | Gap caiu materialmente: `S-00..S-06` agora têm PERT, risk register expandido e `83/86` itens DoD com `EVT-*`. |
| CF-04 cost regression gate não transversal | ✅ | Gate universal em `_sprint_creation_contract.md:245`; propagado explicitamente em `S02:129`, `S03:108`, `S04:95`, `S06:137`, `S14:168`. |
| CF-05 clash TTL/eviction | ✅ | Fechado semanticamente por `ADR-0019` + patches em `S04`/`S07`. |
| CF-06 clash quota ownership | ✅ | Fechado semanticamente por `ADR-0020` + boundary explícita em `S07:65` e `S08:62`. |
| CF-07 compressão de `S-07` deferida para `S-11` | ❌ | Continua errado em `specs/04_sprints/S07/_spec_contract.md:130`. |
| CF-08 binarismo de `S-12` pela metade | ⚠️ | O `OR` crítico caiu, mas o target segue `diff <= 5%` em `S12:119`, `:142-153`; melhoria real, não fechamento total do ideal SOTA. |

### Codex R2 (R2-01..R2-25)

| ID | Status R3 | Notes |
|---|---|---|
| R2-01 `S-08` HIGH_RISK normalizado só no campo `lane` | ✅ | `tags` e `inherits_from` corrigidos em `S08:14`, `:46-54`; PRR adequado em `:98`. |
| R2-02 `S-09` mesmo problema de normalização | ✅ | `tags`, `inherits_from` e sign-off foram alinhados em `S09:14`, `:52-60`, `:140`. |
| R2-03 `S-12` abaixo do budget de approvers | ✅ | `S12:146` agora enumera 11 papéis. |
| R2-04 `S-13` abaixo do budget de approvers | ✅ | `S13:133` agora enumera 11 papéis. |
| R2-05 `S-11` severity drift em invariants | ✅ | Correção explícita em `S11:187-190`. |
| R2-06 `invariant_registry §3.12` stale | ✅ | Heading e escopo atualizados em `invariant_registry.md:156-180`. |
| R2-07 matriz TLA+ não acompanhava novas invariants | ✅ | Há matrix planejada em `invariant_registry.md:215-247`. Residual novo R3: scope/file names ainda inconsistentes. |
| R2-08 título antigo de `S-20` | ✅ | H1 corrigido em `S20:17`. |
| R2-09 engineering gate vs launch coupling por contagem de WI | ✅ | Agora `WI-S20-008` não bloqueia gate em `S20:125`. |
| R2-10 launch ainda dentro do DoD | ✅ | Continua documentado, mas virou `§6.2 Launch Orchestration` explicitamente non-blocking em `S20:142-148`. |
| R2-11 baseline de runbooks desatualizada | ⚠️ | S-20 foi para `40`, mas `S17:197` ainda fala `26`; além disso o repo hoje tem 42 arquivos em `specs/05_quality/runbooks/`. |
| R2-12 wording residual de SBOM antiga em `S-20` | ✅ | Restou só nota histórica em `S20:39`; DoD e quality standards já apontam CycloneDX 1.5+ em `:138`, `:184`. |
| R2-13 gate TLA+/prod-vs-staging duplicado em `S-20` | ❌ | Persistem duplicações/contradições em `S20:137-140`, `:186-187`. |
| R2-14 `S-18` sem dependência de `S-11` | ❌ | Continua sem `S-11` em `S18:193-200`, embora o escopo público cite DPA/privacy material. |
| R2-15 `S-14 -> S-19` não bidirecional | ✅ | `S19:202` agora traz `S-14` como hard blocker. |
| R2-16 `S-10 -> S-13` não bidirecional | ✅ | `S13:192-196` já explicita o outbound de billing/admin plane. |
| R2-17 runbooks críticos ainda em stub | ✅ | Expansão robusta validada nos seis runbooks críticos. |
| R2-18 schema validation ainda bloqueada por tooling | ❌ | Continua aberta; `validate_specs.py` ainda falha por falta de `jsonschema`. |
| R2-19 back-links das novas invariants faltando nos canonical sources | ❌ | KEY foi corrigido, mas `INV-CONSENT-PROOF-VERIFIABLE`, `INV-OBS-CARDINALITY-BUDGET`, `INV-BYOK-CRYPTO-SOVEREIGNTY` seguem praticamente só no registry. |
| R2-20 `S-09` com EVT baixo | ❌ | Continua `1/11` no DoD de `S09:130-140`. |
| R2-21 `S-10` com EVT baixo | ❌ | Continua `1/11` no DoD de `S10:122-132`. |
| R2-22 `S-11` com EVT baixo | ❌ | Continua `2/14` no DoD de `S11:152-165`. |
| R2-23 `S-12` com EVT baixo | ❌ | Continua `2/12` no DoD de `S12:135-146`. |
| R2-24 `S-20` maior backlog EVT | ❌ | Continua `12/21`; faltam EVTs justamente nos gates centrais `S20:125-148`. |
| R2-25 toolchain/proof de `S-08/S-09` hipotético | ❌ | `S09` ainda depende de `specs/_schemas/log_event.schema.json` e `cardinality_check.py` em `S09:96-99`, `:131`, `:168`; os arquivos não existem no checkout. |

## Quality elevation / canonical checks

### `S-00..S-06` v1.1: real ou cosmético?

É real, não cosmético.

- O uplift não ficou só em front matter: o cluster ganhou PERT explícito (`S02:197-208`, `S03:177-190`, `S06:207-219`), risk register 6 colunas (`S00:143-149`, `S02:232-244`, `S06:230-244`), gates adversariais e `EVT-*` tipado.
- Recontagem atual do DoD no cluster: `83/86` itens com `EVT-*` (`96.5%`). No Round 2 o mesmo cluster estava em `16/52` (`30.8%`).
- O residual que sobrou já não é "v1.0 compacto"; é governança e integração cruzada: reviewers humanos ausentes, `validate_specs.py` indisponível e alguns acoplamentos a sprints futuras.

### `error_taxonomy.md`: cobre a customer-facing surface adequadamente?

Ainda não.

- O doc melhora muito a direção, mas não implementa o próprio schema: `retry_strategy`, `canonical_source`, `introduced_in_sprint` e mensagens multilocale aparecem em `error_taxonomy.md:55-71`, mas as tabelas reais em `:79-165` só carregam coluna única de mensagem e não materializam esses campos.
- Há conflito direto com `observability_model.md:215-229`, que define a enum fechada de `error_code` como `AUTH_*`, `TENANT_*`, `CAS_*`, etc., enquanto a taxonomia nova usa `COR_*` (`error_taxonomy.md:55-71`).
- O catálogo atual tem 40 códigos explícitos (`error_taxonomy.md:79-165`), não a superfície de 56 alegada no estado do lote.
- Faltam erros de onboarding/customer lifecycle apesar de `S-19` ter fluxo customer-facing rico (`S19:83-104`, `:132-138`) e hoje não existir nenhum domínio `COR_ONBOARD_*`.

### TLA+ obligation matrix: scope completo?

Ainda não.

- O registry agora tem uma obligation matrix útil (`invariant_registry.md:197-247`), mas o blueprint ainda não fecha completamente com os próprios contracts.
- `PLANNED-specs.md` chama `INV-BILLING-NO-LOSS` e `INV-BILLING-NO-DUP` de CRITICAL (`PLANNED-specs.md:42-45`), enquanto o registry continua marcando ambos como HIGH (`invariant_registry.md:136-137`).
- `S-14` e o registry pedem `region_residency.tla` (`S14:213`, `invariant_registry.md:225-226`), mas o blueprint detalha `byok_sovereignty.tla` como owner do mesmo espaço (`PLANNED-specs.md:159-220`), deixando naming/scope ambíguos.
- A regra de CI ainda é papel: `invariant_registry.md:245-247` e `PLANNED-specs.md:354-378` descrevem gates que não existem em `scripts/`.

### Meta-contract `REVIEW`: prematuro ou legítimo?

`REVIEW` é legítimo; qualquer coisa acima disso ainda é prematura.

- O próprio meta-contract explicita os blockers restantes em `_sprint_creation_contract.md:19-22`; isso é coerente com o status `REVIEW`.
- O problema é a pilha de governança acima dele: `00_framework.md` segue `DRAFT`, `1.0.0-rc1`, e afirma que nenhum spec deveria existir sem ele frozen (`specs/00_framework.md:4-6`, `:19-30`).
- Sem reviewers humanos nomeados e sem `validate_specs.py` funcional, a promoção para `FROZEN` continua sem base.

## Novos findings R3

- **R3-01 — `S-02` introduz dependency inversion com sprints futuras.** O DoD exige integração SDK em 3 linguagens "S-15 ownership" e SBOM via `S-12`, mesmo `S-15` sendo outbound e `S-12` estando à frente no roadmap. Isso conflita com o anti-pattern "sprint que depende de sprint futura". Evidence: `specs/04_sprints/S02/_spec_contract.md:121`, `:124`, `:194`, `specs/04_sprints/_sprint_creation_contract.md:298`.
- **R3-02 — `S-02` ainda não está liberado para implementation start pelo próprio gating declarado.** O contract exige `S-00` e `S-01` SEALED como hard blockers, mas `S-01` ainda está `work_status: READY` e `doc_status: DRAFT` em `specs/04_sprints/S01/sprint.md:4-7`, e `S-00` nem possui `sprint.md` no diretório. Evidence: `specs/04_sprints/S02/_spec_contract.md:181-184`.
- **R3-03 — `S-20` continua aritmeticamente impossível.** O sprint tenta fechar `30d sustained staging`, `30d lighthouse observation` e `2w pentest + 1w retest` dentro de uma janela `D+30`, com `WI-S20-007` e `WI-S20-004` ditos completos em `D+25`. Evidence: `specs/04_sprints/S20/_spec_contract.md:220-239`.
- **R3-04 — `S-20` continua subespecificando o gate formal methods pré-GA.** O registry exige que todo INV CRITICAL planejado esteja `GREEN` antes do gate S-20, mas o contract ainda fala em "all 4 specs" históricas. Evidence: `specs/03_architecture/invariant_registry.md:215-247`, `specs/04_sprints/S20/_spec_contract.md:137`, `:140`, `:318`.
- **R3-05 — O blueprint TLA+ reintroduziu severity drift no domínio billing.** `PLANNED-specs.md` trata `INV-BILLING-NO-LOSS` e `INV-BILLING-NO-DUP` como CRITICAL, enquanto o registry canônico ainda os classifica HIGH. Evidence: `specs/tla/PLANNED-specs.md:42-45`, `specs/03_architecture/invariant_registry.md:136-137`.
- **R3-06 — O naming/scope de TLA+ para residency/BYOK está inconsistente.** Registry/S-14 apontam `region_residency.tla`; o blueprint detalha `byok_sovereignty.tla` cobrindo a mesma invariante de residency. Evidence: `specs/03_architecture/invariant_registry.md:225-226`, `specs/04_sprints/S14/_spec_contract.md:213`, `specs/tla/PLANNED-specs.md:159-220`.
- **R3-07 — Os novos CI gates continuam não-enforced.** `check_tla_obligations.py` e `check_error_taxonomy.py` estão planejados em docs, mas não existem em `scripts/`; então o lote criou obrigações sem enforcement real. Evidence: `specs/03_architecture/invariant_registry.md:245-247`, `specs/03_architecture/error_taxonomy.md:239-257`.
- **R3-08 — `error_taxonomy.md` não instancia o próprio schema canônico.** O schema exige `retry_strategy`, `customer_message` multilocale, `canonical_source` e `introduced_in_sprint`, mas o catálogo real não os materializa. Evidence: `specs/03_architecture/error_taxonomy.md:55-71`, `:79-165`.
- **R3-09 — A nova taxonomia de erros conflita com o canonical source anterior de observability.** `observability_model.md` continua definindo a enum fechada de `error_code` em `AUTH_*`, `TENANT_*`, `CAS_*` etc.; a nova taxonomia cria `COR_*` sem mapping canônico. Evidence: `specs/03_architecture/observability_model.md:215-229`, `specs/03_architecture/error_taxonomy.md:55-71`.
- **R3-10 — A customer-facing surface de onboarding ainda não entrou na taxonomia.** `S-19` define signup, DPA versioning, re-acceptance e degrade read-only; a taxonomia não tem domínio `COR_ONBOARD_*` nem equivalentes. Evidence: `specs/04_sprints/S19/_spec_contract.md:83-104`, `:132-138`, `specs/03_architecture/error_taxonomy.md:77-165`.
- **R3-11 — `S-11` expandiu a sweep list para 10 backends, mas DoD e completeness continuam provando só 7.** Isso reabre o risco de H-10 em termos de auditability. Evidence: `specs/04_sprints/S11/_spec_contract.md:99-104`, `:153-154`, `:173`.
- **R3-12 — `S-11` e `S-14` continuam contraditórios sobre Schrems II TIA.** `S-11` ainda trata TIA como anti-scope pós-GA; `S-14` a trata como deliverable pré-GA. Evidence: `specs/04_sprints/S11/_spec_contract.md:205`, `specs/04_sprints/S14/_spec_contract.md:90`, `:126`, `:136`.
- **R3-13 — A baseline de runbooks está em três estados ao mesmo tempo.** `S-17` ainda exporta "26 runbooks", `S-20` fala "40", e o repo hoje contém 42 arquivos em `specs/05_quality/runbooks/`. Evidence: `specs/04_sprints/S17/_spec_contract.md:197`, `specs/04_sprints/S20/_spec_contract.md:135`, `:185`, `:281`.
- **R3-14 — `S-15` ainda tem inconsistência interna simples mas corrosiva: 8 checks prometidos, 6 listados.** Evidence: `specs/04_sprints/S15/_spec_contract.md:81`, `:85-92`, `:134`.
- **R3-15 — `admin.corelink.dev` continua órfão.** `S-16` o exclui explicitamente, mas nenhuma sprint assume ownership desse painel operacional interno. Evidence: `specs/04_sprints/S16/_spec_contract.md:177`, `:194`.
- **R3-16 — `S-07` ainda empurra compressão para o sprint errado.** O texto continua deferindo para `S-11` ou ADR, apesar de `S-11` ser privacy/DSR. Evidence: `specs/04_sprints/S07/_spec_contract.md:130`.
- **R3-17 — `S-09` continua com trilha de prova hipotética.** O contract depende de `specs/_schemas/log_event.schema.json` e `cardinality_check.py`, mas esses artefatos não existem no checkout. Evidence: `specs/04_sprints/S09/_spec_contract.md:96-99`, `:131`, `:168`.
- **R3-18 — O stack de governança ainda não está frozen enough para chamar gate pre-implementation "limpo".** Meta-contract está em `REVIEW`, mas framework segue `DRAFT/rc1` e `validate_specs.py` continua indisponível. Evidence: `specs/04_sprints/_sprint_creation_contract.md:19-22`, `specs/00_framework.md:4-6`, `:19-30`.

## Pre-implementation gate check

### S-02 readiness

- **Bloqueios materiais:**
- Dependency inversion no próprio DoD: `S-02` não pode exigir `S-12` e `S-15` para selar (`S02:121`, `:124`, `:194`, `_sprint_creation_contract.md:298`).
- Hard blocker literal ainda não satisfeito: `S-01` está `READY`, não `SEALED` (`specs/04_sprints/S01/sprint.md:4-7`).
- Governança/tooling ainda incompletos: `00_framework.md` segue `DRAFT/rc1` e `validate_specs.py` não roda.

- **Pré-condições atendidas:**
- O contract em si já está em `v1.1.0`, com PERT, risk register, invariants e DoD auditável (`specs/04_sprints/S02/_spec_contract.md:115-165`, `:197-245`).
- O uplift estrutural do cluster anterior (`S-00..S-06`) foi real; não há mais o argumento de que `S-02` está baseado em spec "compacto".
- `validate_references.py` continua sem dangling refs.

- **Recommendation:** `⚠️ proceed with caveats` para criar `sprint.md` **somente após** micro-lote doc-only corrigindo a dependency inversion e registrando explicitamente que implementation start continua condicionado a `S-01` SEALED. Para **implementation start**, o status atual é `❌ blocked`.

### S-06 GC readiness

- **Bloqueios materiais:** nenhum blocker novo específico de spec; o contrato foi realmente elevado (`S06:126-150`, `:207-244`).
- **Pré-condições atendidas:** PERT, adversarial review, TLA+, runbooks e 30d observation estão explicitados.
- **Residual caveat:** o sprint continua dependente do mesmo gap transversal de governance/tooling (`jsonschema`, reviewers humanos, CI gates planned-only).
- **Recommendation:** `✅ spec-ready`, sem urgência de bloqueio sobre `S-02`.

## Forward-looking (Lote 9.5)

1. **Micro-lote pré-S-02 imediatamente:** corrigir `S-02` dependency inversion (`S-12`/`S-15` no DoD) e alinhar os hard blockers com o estado real de `S-00/S-01`.
2. **Tooling gate real, não declarativo:** instalar `jsonschema` no CI e criar `scripts/check_tla_obligations.py` + `scripts/check_error_taxonomy.py`. Isso é urgente.
3. **Reconciliar a pilha formal:** alinhar severidades/nomes entre `invariant_registry.md`, `PLANNED-specs.md`, `S-14` e `S-20`; atualizar `S-20` para o conjunto completo de obrigações TLA+, não só as 4 históricas.
4. **Consolidar `error_taxonomy.md`:** fazer o catálogo obedecer o próprio schema, publicar mapping canônico para `observability_model.md`, e adicionar domínio onboarding/customer lifecycle.
5. **Higiene cross-doc rápida:** corrigir contagem de runbooks (`26/40/42`), `S-15` "8 checks", `S-07` compressão→`S-11`, e `S-11` 10 backends vs 7 backends.
6. **Reviewer staffing strategy:** nomear primeiro 2 reviewers humanos para `00_framework.md` + `_sprint_creation_contract.md` (Arquitetura/Processo e Security/Privacy). Na sequência, montar bench leve por lane: SRE/ops para HIGH_RISK operacionais; Legal/Privacy para `S-11/S-14/S-19`; DevX/Product para `S-15/S-18`.
7. **Apalache symbolic upgrade timing:** manter como `post-GA Q1` faz sentido. Antes disso, a prioridade é TLC + CI gate funcional + 5 specs planejadas realmente verdes. O melhor piloto pós-GA é `key_lifecycle.tla` ou `onboarding_atomicity.tla`, onde o state space tende a crescer.

## Veredito Round 3

- **SOTA score atual:** `8.6/10` (vs `8.0/10` no R2; vs `6.8/10` no R1).
- **Remediation effectiveness:** `24/38` findings rastreáveis (`Opus C-01..05` + `Codex CF-01..08/R2-01..25`) estão `✅ closed` (`63.2%`); `26/38` estão pelo menos parcialmente tratados (`68.4%`).
- **Pending effort para 10/10:** ~`55h` de trabalho documental/governança/CI gate antes de considerar o programa realmente "implementation-ready".
- **Recommendation:** fazer **micro-Lote 9.5 first** (doc-only + CI/tooling hardening), então criar `S-02/sprint.md`; não iniciar implementation de `S-02` antes de fechar os blockers materiais acima.
