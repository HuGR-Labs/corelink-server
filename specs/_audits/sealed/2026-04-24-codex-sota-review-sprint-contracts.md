---
id: AUDIT-SPRINT-CONTRACTS-SOTA
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [GPT via codex CLI — SOTA parceiro]
supersedes: null
superseded_by: null
tags: [audit, sota, sprint-contracts, elevation]
---

# Codex SOTA Review — Sprint Contracts

## Escopo

- 22 docs auditados: `specs/04_sprints/_sprint_creation_contract.md` + `S00..S20/_spec_contract.md`.
- Base de comparação: `specs/00_framework.md`, especialmente `§33.5`, `§35.5`, `§35.7`.
- Referências canônicas usadas para validação: `specs/03_architecture/*`, `specs/tla/*`, `specs/05_quality/runbooks/*`.
- Auditado no estado atual do working tree. Havia mudanças locais não commitadas em `S02`, `S07`, `S08` e `S09`; o relatório reflete esse estado.

## Findings globais

- O framework/meta-contract exigem `inherits_from` mínimo por lane, DoD com `EVT-XXX`, timeline com `start/mid/end + buffer` e PERT em STANDARD/HIGH_RISK (`specs/04_sprints/_sprint_creation_contract.md:149-150`, `:178`, `:187-189`). A série atual viola os quatro itens de forma sistemática.
- `121/155` checkboxes de DoD nos 21 sprint contracts não trazem evidence tipada. Os piores casos são `S20 (10/10 sem EVT)`, `S09 (10/11)`, `S02 (8/8)`, `S04 (7/7)`, `S06 (7/7)`.
- `15/20` sprints `STANDARD`/`HIGH_RISK` ainda não trazem PERT real em `§12`, apesar da exigência explícita do meta-contract (`specs/04_sprints/_sprint_creation_contract.md:189`).
- O pacote `S07/S08/S09` já foi elevado para `v1.1.0` e hoje é o trecho mais SOTA da série. A suspeita “S07–S20 estão mais compactas” ficou parcialmente falsa: o buraco real começa em `S10..S19`, com `S14`/`S20` um pouco acima da média, mas ainda abaixo da régua que o próprio framework pede para `HIGH_RISK`.
- A proporcionalidade por lane está quebrada em termos de detalhe. Média de linhas: `STANDARD = 162.6`, `HIGH_RISK = 143.1`. Em termos práticos, `S09` (STANDARD, 310 linhas) está mais completo que `S12` (HIGH_RISK, 126), `S14` (HIGH_RISK, 141) e `S10` (HIGH_RISK, 136).
- `python3 scripts/validate_references.py --json` hoje acusa 10 referências pendentes relevantes nos sprint contracts: novos `INV-*` fora do registry, `SLO-AVAIL` genérico e runbooks inexistentes em `S07/S08/S09`.
- O meta-contract que deveria ser base rígida ainda está `doc_status: DRAFT` e “staffing-blocked” (`specs/04_sprints/_sprint_creation_contract.md:4`, `:19-22`). Como baseline de governança, isso ainda está abaixo da barra que ele impõe aos downstreams.

### Padrão de expansão necessária por sprint

| Sprint | Lane | Nível atual | Nível SOTA target | gap_score (1-10) |
|---|---|---|---|---|
| META | META | forte em regras, fraco em autoridade | FROZEN + enforceable | 5 |
| S00 | STANDARD | médio | strong planning contract | 6 |
| S01 | HIGH_RISK | forte | strong + evidence-complete | 4 |
| S02 | HIGH_RISK | médio | high-risk SOTA | 7 |
| S03 | HIGH_RISK | médio | high-risk SOTA | 6 |
| S04 | HIGH_RISK | médio | high-risk SOTA | 6 |
| S05 | HIGH_RISK | médio | high-risk SOTA | 7 |
| S06 | HIGH_RISK | médio | high-risk SOTA | 7 |
| S07 | STANDARD | SOTA-ish v1.1 | polish + registry cleanup | 3 |
| S08 | STANDARD | SOTA-ish v1.1 | fix lane/refs + evidence | 4 |
| S09 | STANDARD | SOTA-ish v1.1 | fix lane/refs + evidence | 3 |
| S10 | HIGH_RISK | compacto | high-risk SOTA | 7 |
| S11 | HIGH_RISK | compacto | high-risk SOTA | 7 |
| S12 | HIGH_RISK | compacto | high-risk SOTA | 8 |
| S13 | STANDARD | compacto | security-sensitive STANDARD/HIGH_RISK quality | 8 |
| S14 | HIGH_RISK | médio, scope overstuffed | decomposed high-risk SOTA | 8 |
| S15 | STANDARD | compacto | product-integration SOTA | 8 |
| S16 | STANDARD | compacto | UX/compliance SOTA | 7 |
| S17 | STANDARD | compacto | ops-rhythm SOTA | 8 |
| S18 | LOW_RISK | compacto | publish-safe LOW_RISK+ | 6 |
| S19 | STANDARD | compacto | contract/onboarding SOTA | 8 |
| S20 | HIGH_RISK | médio, gates irreais | GA gate SOTA | 8 |

## Gaps específicos por sprint

### META

- `specs/04_sprints/_sprint_creation_contract.md:4`, `:19-22` mantém o contrato-base em `DRAFT`/`staffing-blocked`. Antes de cobrar conformidade dura de 21 downstreams, esse doc deveria estar `REVIEW` ou `FROZEN`.
- `specs/04_sprints/_sprint_creation_contract.md:149-150`, `:178`, `:187-189` definem quatro obrigações universais que os contracts downstreams ainda não cumprem em massa: herança mínima por lane, EVT tipado, timeline com datas reais e PERT.

### S-00

- `specs/04_sprints/S00/_spec_contract.md:44-49` viola o mínimo STANDARD do meta-contract ao não herdar `OBSERVABILITY-MODEL` e `FAILURE-MODES` (`specs/04_sprints/_sprint_creation_contract.md:149`).
- `specs/04_sprints/S00/_spec_contract.md:69-72` deixa 3 itens DoD sem `EVT-XXX`, apesar do requisito universal em `specs/04_sprints/_sprint_creation_contract.md:178`.
- `specs/04_sprints/S00/_spec_contract.md:86-87` introduz `INV-DATA-CLASSIFICATION` e `INV-SCOPE-DISCIPLINE` fora do `invariant_registry.md`. Se são invariants globais, precisam entrar no registry; se são locais, não deveriam usar prefixo `INV-*`.
- `specs/04_sprints/S00/_spec_contract.md:122-124` tem start/end/buffer, mas falta `mid-check`, que o meta-contract pede em `specs/04_sprints/_sprint_creation_contract.md:187`.

### S-01

- `specs/04_sprints/_sealed/S01/_spec_contract.md:125-131` usa horas simples, não PERT, apesar de `HIGH_RISK` exigir estimativa mais rica (`specs/04_sprints/_sprint_creation_contract.md:189`).
- `specs/04_sprints/_sealed/S01/_spec_contract.md:147-152` ainda usa risk register de 4 colunas. Para um sprint que toca `INV-TENANT-ISOLATION`, a versão S07/S08/S09 já mostra a régua correta: `Prob/Det/Impacto/Exposure/Mitigação/Residual`.

### S-02

- `specs/04_sprints/S02/_spec_contract.md:73-80` está com `8/8` itens DoD sem `EVT-XXX`. Para um sprint `HIGH_RISK` que fecha o read path e side-channel timing, isso é um gap grave.
- `specs/04_sprints/S02/_spec_contract.md:112-119` traz WIs sem qualquer estimativa, em desacordo com o meta-contract.
- `specs/04_sprints/S02/_spec_contract.md:127-129` pede `PRR approved`, mas `§6` não tipa evidence nem cita adversarial review. O gate existe como frase, não como contrato auditável.
- `specs/04_sprints/S02/_spec_contract.md:133-137` mantém risk register minimalista, sem mitigação formal nem residual.

### S-03

- `specs/04_sprints/_sealed/S03/_spec_contract.md:44-53` não herda `SLO-CATALOG` nem `RESILIENCE-PATTERNS`, embora o sprint imponha alvo de revogação global em `≤ 60s` (`:32`, `:68`, `:85`). O meta-contract obriga essas fontes quando a sprint toca reliability/SLO (`specs/04_sprints/_sprint_creation_contract.md:156`).
- `specs/04_sprints/_sealed/S03/_spec_contract.md:76-81` deixa 4 itens DoD sem evidence tipada.
- `specs/04_sprints/_sealed/S03/_spec_contract.md:115-124` continua sem PERT.

### S-04

- `specs/04_sprints/_sealed/S04/_spec_contract.md:67` define `CTRL-AC-002` como HKDF tenant key, enquanto `:118` abre “HKDF + Ed25519 opt” no WI. Falta decisão arquitetural explícita; hoje o contract aceita duas primitivas incompatíveis sem ADR.
- `specs/04_sprints/_sealed/S04/_spec_contract.md:73-79` tem `7/7` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S04/_spec_contract.md:111-120` não traz PERT.
- `specs/04_sprints/_sealed/S04/_spec_contract.md:59` já entrega `CAP-AC-004` (TTL management), mas `S07` reabre ownership desse tópico em `CAP-EVICT-002` (`specs/04_sprints/_sealed/S07/_spec_contract.md:64`) sem declarar override explícito.

### S-05

- `specs/04_sprints/_sealed/S05/_spec_contract.md:55` promete blobs de até `5 TiB`, mas `:94` limita multipart a `≤ 50` parts por blob. Sem separar “chunk size” de “multipart part size”, o contrato fica matematicamente inconsistente.
- `specs/04_sprints/_sealed/S05/_spec_contract.md:72-77` tem `6/6` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S05/_spec_contract.md:109-116` continua sem PERT.

### S-06

- `specs/04_sprints/_sealed/S06/_spec_contract.md:81`, `:86`, `:131-133` exigem simulação/estabilidade de `30d`, mas `§13` continua em `4 semanas` (`:125-127`) sem reservar explicitamente esse burn-in. O gate está fora do orçamento da sprint.
- `specs/04_sprints/_sealed/S06/_spec_contract.md:75-81` tem `7/7` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S06/_spec_contract.md:115-123` não usa PERT.
- `specs/04_sprints/_sealed/S06/_spec_contract.md:137-142` ainda usa risk register de 3 colunas num sprint `FF-HR-011`.

### S-07

- `specs/04_sprints/_sealed/S07/_spec_contract.md:114`, `:121` introduzem `INV-DEDUP-CONSISTENCY`, mas ele ainda não existe em `specs/03_architecture/invariant_registry.md`; o validator aponta dangling.
- `specs/04_sprints/_sealed/S07/_spec_contract.md:78`, `:91`, `:163` referenciam `RB-FM-305` e `RB-FM-059`, mas esses runbooks não existem em `specs/05_quality/runbooks/`.
- `specs/04_sprints/_sealed/S07/_spec_contract.md:130` diz que compressão foi “defer pra S-11”, mas `S-11` é privacy/DSR, não storage optimization (`specs/04_sprints/S11/_spec_contract.md:17`, `:28`).
- `specs/04_sprints/_sealed/S07/_spec_contract.md:138` declara que bloqueia `S-10` e `S-14`, mas isso não está espelhado nos contracts downstream.

### S-08

- `specs/04_sprints/S08/_spec_contract.md:27-39` mantém a sprint em `STANDARD`, embora implemente `CTRL-RATE-001` e `CTRL-QUOTA-001` (`specs/03_architecture/security_model.md:300-301`), o que bate com `FF-HR-005` do framework (`specs/00_framework.md:2049`).
- `specs/04_sprints/S08/_spec_contract.md:86` usa `SLO-AVAIL` genérico, que não existe no `slo_catalog.md`; o catálogo usa IDs específicos como `SLO-AVAIL-CAS-GET` (`specs/03_architecture/slo_catalog.md:130`).
- `specs/04_sprints/S08/_spec_contract.md:90` exige `RB-FM-250`, mas esse runbook não existe.
- `specs/04_sprints/S08/_spec_contract.md:112` cria `INV-RATE-LIMIT-PROPORTIONALITY` fora do registry.
- `specs/04_sprints/S08/_spec_contract.md:73-79` cria semântica via `X-Rate-Limit-Type`, enquanto `:92`, `:120`, `:183-184` empurram padrão IETF. Falta mapping canônico entre header custom e padrão.
- `specs/04_sprints/S08/_spec_contract.md:136` diz que bloqueia `S-10` e `S-14`; isso não foi refletido nos contracts desses sprints.

### S-09

- `specs/04_sprints/_sealed/S09/_spec_contract.md:26`, `:45`, `:138`, `:239` inventam a pseudo-lane `STANDARD-PLUS`. O framework só conhece `LOW_RISK | STANDARD | HIGH_RISK` (`specs/00_framework.md:965`, `:2015-2057`).
- `specs/04_sprints/_sealed/S09/_spec_contract.md:46`, `:56-58`, `:94-116` processa PII, logs e audit chain, mas ainda fica em `STANDARD`. Pelo framework, isso está muito mais próximo de `FF-HR-003`/`FF-HR-005` que de um STANDARD puro.
- `specs/04_sprints/_sealed/S09/_spec_contract.md:128-138` deixa `10/11` itens DoD sem `EVT-XXX`, mesmo sendo o sprint com maior ambição de governança.
- `specs/04_sprints/_sealed/S09/_spec_contract.md:155`, `:157` misturam controles (`CTRL-PRIV-001`, `CTRL-AUDIT-001`) com invariants em `§8`.
- `specs/04_sprints/_sealed/S09/_spec_contract.md:161-162` cria `INV-OBS-CARDINALITY-BUDGET` e `INV-OBS-AUDIT-CHAIN-INTEGRITY`; o validator também acusou `INV-OBS-CARDINALITY` por referência interna não definida. Nada disso está no registry ainda.
- `specs/04_sprints/_sealed/S09/_spec_contract.md:137`, `:217`, `:248` exigem `RB-FM-153` e `RB-FM-201`, mas esses runbooks não existem. Pior: `FM-201` em `failure_modes.md` é “config change causa rate-limit drop”, não “cardinality explosion” (`specs/03_architecture/failure_modes.md:164`).
- `specs/04_sprints/_sealed/S09/_spec_contract.md:133` fala em screenshots de dashboard, mas não tipa `EVT-013`; o framework proíbe aceitar URL de dashboard como evidence isolada (`specs/00_framework.md:458`, `:2717-2744`).

### S-10

- `specs/04_sprints/S10/_spec_contract.md:39-46` está `HIGH_RISK` mas não herda `INVARIANT-REGISTRY`, exigido pelo meta-contract para essa lane (`specs/04_sprints/_sprint_creation_contract.md:150`).
- `specs/04_sprints/S10/_spec_contract.md:78`, `:120`, `:124` exigem `30d` de frescor/drift, mas a sprint continua orçada em `3 semanas`.
- `specs/04_sprints/S10/_spec_contract.md:69-74` deixa `6/6` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/S10/_spec_contract.md:102-104` não lista `S-07` nem `S-08` como blockers, embora ambos digam explicitamente que bloqueiam billing (`specs/04_sprints/_sealed/S07/_spec_contract.md:138`, `specs/04_sprints/S08/_spec_contract.md:136`).

### S-11

- `specs/04_sprints/S11/_spec_contract.md:40-48` está `HIGH_RISK` mas não herda `INVARIANT-REGISTRY`.
- `specs/04_sprints/S11/_spec_contract.md:28` promete “self-service API + UI”, mas a UI de consent/DSR está alocada em `S-16` (`specs/04_sprints/_sealed/S16/_spec_contract.md:27`, `:48-49`). Falta separar backend de frontend ou declarar dependência.
- `specs/04_sprints/S11/_spec_contract.md:73` usa SLA de `30d`, enquanto `§13` continua em `3 semanas` (`:122-124`).
- `specs/04_sprints/S11/_spec_contract.md:72-78` deixa `6/7` itens DoD sem `EVT-XXX`.

### S-12

- `specs/04_sprints/_sealed/S12/_spec_contract.md:69` coloca “bit-identical **ou** documentação das fontes de non-determinism pending”. Isso quebra o princípio de DoD binário do meta-contract (`specs/04_sprints/_sprint_creation_contract.md:177`).
- `specs/04_sprints/_sealed/S12/_spec_contract.md:62-70`, `:112-115` não citam PRR nem adversarial review, embora o sprint seja `HIGH_RISK`.
- `specs/04_sprints/_sealed/S12/_spec_contract.md:64-70` deixa `6/7` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S12/_spec_contract.md:99-106` continua sem PERT.

### S-13

- `specs/04_sprints/S13/_spec_contract.md:31`, `:48-49`, `:71`, `:76` implementa `CTRL-CRED-003`, `CTRL-AUTH-010`, `CTRL-AUDIT-003`, mas ainda se classifica como `STANDARD`. Pelo framework, isso tem cheiro claro de `FF-HR-005`.
- `specs/04_sprints/S13/_spec_contract.md:76-77` usa controles e key invariants locais em `§8`, não invariants canônicas.
- `specs/04_sprints/S13/_spec_contract.md:62-67` deixa `6/6` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/S13/_spec_contract.md:98-102` não traz PERT.
- `specs/04_sprints/S13/_spec_contract.md:65`, `specs/03_architecture/key_management.md:97-99` mostram outro gap: o sprint cita `INV-KEY-OVERLAP`, mas o DoD não prova overlap de 24h, re-wrap progress nem rollback safe.

### S-14

- `specs/04_sprints/S14/_spec_contract.md:41-50` está `HIGH_RISK` mas não herda `INVARIANT-REGISTRY`.
- `specs/04_sprints/S14/_spec_contract.md:24` omite `FF-HR-009`, embora `:84`, `:122`, `:130` falem de `DPA amendment`, `DPA signed` e contrato enterprise. O framework diz que DPA/SLA customer-facing forçam `HIGH_RISK` com `FF-HR-009` (`specs/00_framework.md:2053`).
- `specs/04_sprints/S14/_spec_contract.md:28`, `:55-62`, `:115-122`, `:126` tentam enfiar 4 regiões + failover + 4 providers BYOK + signed erasure attestation + DPA em 8 WIs/4 semanas, sem PERT. Scope grossamente comprimido.
- `specs/04_sprints/S14/_spec_contract.md:75-80` deixa `6/6` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/S14/_spec_contract.md:107-109` não espelha o bloqueio vindo de `S-07`/`S-08`.
- `specs/04_sprints/S14/_spec_contract.md:73-80`, `:128-130` não citam PRR nem adversarial review, só um pentest BYOK no promotion gate.

### S-15

- `specs/04_sprints/_sealed/S15/_spec_contract.md:35-40` viola o mínimo STANDARD ao não herdar `FAILURE-MODES`.
- `specs/04_sprints/_sealed/S15/_spec_contract.md:71-74` usa `§8 Invariants` para dois statements soltos, sem invariants registradas.
- `specs/04_sprints/_sealed/S15/_spec_contract.md:60-64` deixa `5/5` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S15/_spec_contract.md:94-98` não traz PERT.
- `specs/04_sprints/_sealed/S15/_spec_contract.md:54` define wrappers `pyO3/cgo/WASM`, mas o contract não registra trade-off nem fallback caso a estratégia multi-FFI fique mais cara que SDK nativo/HTTP thin client.

### S-16

- `specs/04_sprints/_sealed/S16/_spec_contract.md:36-41` viola o mínimo STANDARD ao não herdar `FAILURE-MODES`.
- `specs/04_sprints/_sealed/S16/_spec_contract.md:45` colide diretamente com `S-19` na ownership de onboarding (`specs/04_sprints/_sealed/S19/_spec_contract.md:27`, `:44-46`).
- `specs/04_sprints/_sealed/S16/_spec_contract.md:77-80` usa controles/plain statements em `§8`, não invariants canônicas.
- `specs/04_sprints/_sealed/S16/_spec_contract.md:65-70` deixa `6/6` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S16/_spec_contract.md:101-106` não traz PERT.

### S-17

- `specs/04_sprints/_sealed/S17/_spec_contract.md:62` exige `4 semanas` de chaos tests, mas `§13` continua em `2.5 semanas` (`:103-105`). O gate não cabe no orçamento.
- `specs/04_sprints/_sealed/S17/_spec_contract.md:74-75` usa PATs em `§8`, não invariants.
- `specs/04_sprints/_sealed/S17/_spec_contract.md:61-65` deixa `4/5` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S17/_spec_contract.md:97-101` não traz PERT.

### S-18

- `specs/04_sprints/_sealed/S18/_spec_contract.md:27`, `:46-47` publica pricing/security/SLA-adjacent claims, mas continua `LOW_RISK`. Se o sprint puder mudar promessas públicas, precisa anti-scope mais duro ou lane superior.
- `specs/04_sprints/_sealed/S18/_spec_contract.md:58-61` deixa `4/4` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S18/_spec_contract.md:80` aponta “Enterprise-only docs (S-19 customer onboarding)”, mas `S-19` não é sprint de docs enterprise; o cross-reference está torto.

### S-19

- `specs/04_sprints/_sealed/S19/_spec_contract.md:36-40` viola o mínimo STANDARD ao não herdar `OBSERVABILITY-MODEL` e `FAILURE-MODES`.
- `specs/04_sprints/_sealed/S19/_spec_contract.md:31` reconhece que toca `DPA`/`Terms`, mas continua `STANDARD`. Pelo framework, isso aciona `FF-HR-009`.
- `specs/04_sprints/_sealed/S19/_spec_contract.md:27`, `:44-46` colide com `S-16` na ownership do fluxo `signup → DPA → billing`.
- `specs/04_sprints/_sealed/S19/_spec_contract.md:56` cria dependência implícita em Slack/CRM, mas `§11` não a declara e o repo não trata esse fluxo em canonical sources.
- `specs/04_sprints/_sealed/S19/_spec_contract.md:60-64` deixa `4/5` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S19/_spec_contract.md:94-98` não traz PERT.

### S-20

- `specs/04_sprints/_sealed/S20/_spec_contract.md:28` fala em “todos os 10 canonical sources verdes”, mas `§3` lista 14 fontes canônicas (`:39-54`). O número está objetivamente errado.
- `specs/04_sprints/_sealed/S20/_spec_contract.md:82`, `:91`, `:104`, `:111` exigem janelas de `30d`, enquanto `§13` segue em `3 semanas` (`:136-138`). O gate é impossível sem pré-bake externo à sprint.
- `specs/04_sprints/_sealed/S20/_spec_contract.md:78-87` deixa `10/10` itens DoD sem `EVT-XXX`.
- `specs/04_sprints/_sealed/S20/_spec_contract.md:109` regride a precisão do supply-chain gate: `S12` fixou `CycloneDX 1.5+` (`specs/04_sprints/_sealed/S12/_spec_contract.md:48`), mas `S20` voltou para “Full SBOM v1.0”.
- `specs/04_sprints/_sealed/S20/_spec_contract.md:74`, `:85`, `:134` mistura GTM/marketing/Product Hunt com gates de readiness técnica. O sprint final precisa separar `GA engineering gate` de `launch orchestration`.

## Cross-contract inconsistencies

- `S07` declara que bloqueia `S10` e `S14` (`specs/04_sprints/_sealed/S07/_spec_contract.md:138`), mas `S10` (`:102-104`) e `S14` (`:107-109`) não refletem isso.
- `S08` declara que bloqueia `S10` e `S14` (`specs/04_sprints/S08/_spec_contract.md:136`), mas `S10`/`S14` também não refletem isso.
- Ownership de TTL/eviction ficou duplicada: `S04 CAP-AC-004` (`specs/04_sprints/_sealed/S04/_spec_contract.md:59`) vs `S07 CAP-EVICT-002` (`specs/04_sprints/_sealed/S07/_spec_contract.md:64`).
- Ownership de quota ficou duplicada: `S07 CAP-EVICT-003` (`specs/04_sprints/_sealed/S07/_spec_contract.md:65`) vs `S08 CAP-QUOTA-001/002` (`specs/04_sprints/S08/_spec_contract.md:59-60`).
- `S11` coloca UI no escopo (`specs/04_sprints/S11/_spec_contract.md:28`), mas `S16` já é a sprint de consent/DSR UI (`specs/04_sprints/_sealed/S16/_spec_contract.md:27`, `:48-49`).
- `S16` e `S19` duplicam onboarding: `S16 CAP-UI-001` (`specs/04_sprints/_sealed/S16/_spec_contract.md:45`) vs `S19 CAP-ONBOARD-001..003` (`specs/04_sprints/_sealed/S19/_spec_contract.md:44-46`).
- `S14` já faz `DPA amendment`/`DPA signed` (`specs/04_sprints/S14/_spec_contract.md:84`, `:122`, `:130`) mas não lista `FF-HR-009`.
- `S20` fala em 10 canonical sources (`specs/04_sprints/_sealed/S20/_spec_contract.md:28`), herda 14 (`:39-54`) e o framework lista 18 fontes canônicas possíveis (`specs/00_framework.md:2443-2462`).
- `S12` fixa SBOM em `CycloneDX 1.5+` (`specs/04_sprints/_sealed/S12/_spec_contract.md:48`), enquanto `S20` retrocede para “SBOM v1.0” (`specs/04_sprints/_sealed/S20/_spec_contract.md:109`).
- `S09` cria `STANDARD-PLUS` (`specs/04_sprints/_sealed/S09/_spec_contract.md:26`, `:45`), mas o framework não reconhece lane intermediária.
- `S07/S08/S09` introduzem invariants novas (`INV-DEDUP-CONSISTENCY`, `INV-RATE-LIMIT-PROPORTIONALITY`, `INV-OBS-*`) sem propagação para `invariant_registry.md`.
- `S07/S08/S09` referenciam runbooks inexistentes (`RB-FM-305`, `RB-FM-059`, `RB-FM-250`, `RB-FM-153`, `RB-FM-201`), quebrando a coerência com a biblioteca de 26 runbooks.
- `S07` manda compressão para `S-11` (`specs/04_sprints/_sealed/S07/_spec_contract.md:130`), mas `S-11` é privacy/DSR, não storage/perf.
- `S09` mapeia `RB-FM-201` para “cardinality explosion” (`specs/04_sprints/_sealed/S09/_spec_contract.md:137`), enquanto `FM-201` em `failure_modes.md` é “config change causa rate-limit drop” (`specs/03_architecture/failure_modes.md:164`).

## SOTA enrichments recomendados (universais)

- Benchmarks competitivos mínimos por domínio:
  - Cache: BuildBuddy, NativeLink, bazel-remote, JFrog remote cache.
  - Métricas-alvo: `dedup ratio`, `CAS GET p99`, `AC hit p99`, `GC reclaim latency`, `billing freshness`, `rate-limit overhead`.
- Formal methods além de TLC:
  - `Apalache` para model checking simbólico.
  - `loom` para races de concorrência Rust em rate limit, GC e admin-plane.
  - `cargo-proptest` + seeds persistidos por sprint.
- Perf/testing stack SOTA:
  - `criterion.rs` com baseline versionado.
  - `wrk2`/`k6` open-loop para evitar coordinated omission.
  - `hdrhistogram` em artifacts.
  - `cargo-mutants` para kill-rate em invariants críticas.
- Supply-chain / build provenance:
  - SLSA v1.0, `in-toto` provenance, Rekor transparency log, `GUAC` para ingestão de SBOM/provenance.
  - `NIST SP 800-218` (SSDF) como referência de processo.
- Security/compliance baseline:
  - `FIPS 140-3` para HSM/KMS/BYOK claims.
  - `NIST SP 800-57` para lifecycle de keys.
  - `NIST SP 800-207` para zero-trust framing.
  - `NIST SP 800-61r2` para incident response/runbooks.
  - `FedRAMP Moderate baseline` como checklist delta para S14/S20.
- Privacy/compliance extra:
  - `ISO/IEC 27701` para extensões de privacy do ISMS.
  - `NIST Privacy Framework 1.0`.
  - explicitar `DPIA/LIA/TIA` por sprint regulatória, não só em S11/S14/S20.
- Observability SOTA:
  - `OpenTelemetry semantic conventions`.
  - `OpenMetrics exemplars`.
  - `Google SRE Workbook` para burn-rate alerting.
  - `Sloth` ou tooling equivalente para alerting as code.
- HTTP/API standards:
  - `RFC 6585` para `429 Too Many Requests`.
  - alinhar o uso de `RateLimit`/`RateLimit-Policy` ao padrão HTTPAPI vigente; evitar citar RFC não relacionado.
  - `CloudEvents v1.0.2` explicitamente nos sprints de audit/billing/privacy.
- Docs/public artifacts:
  - `WCAG 2.2 AA` em vez de 2.1 quando o escopo já é greenfield.
  - `Diátaxis` + `Vale` + `lychee` para lint de docs.
  - screenshots/example sanitization via fixture pipeline.
- Evidence ops:
  - criar bloco obrigatório por gate no formato `[EVT-XXX]: <path|url>` para cada DoD item.
  - impedir `dashboard URL` sem `dashboard snapshot`, conforme framework.
- Cost/FinOps:
  - budget explícito por sprint para Grafana cardinality, Logpush volume, R2 retention, Stripe fees.
  - adicionar “cost regression gate” em S07/S08/S09/S10/S14.
- Alternative-design discipline:
  - exigir mini-seção “Alternativas consideradas” sempre que houver escolha de primitiva, vendor ou protocolo.
  - exemplos críticos: `HKDF vs Ed25519` (S04), `fixed chunking vs FastCDC` (S05), `FFI wrappers vs native SDKs` (S15), `Grafana Cloud vs self-hosted` (S09), `per-region vs federated rate-limit` (S08).

## Priorização de expansão

1. TIER-1 (urgent — high-risk compactas ou com gates inexequíveis): `S10`, `S12`, `S14`, `S20`, `S11`, `S06`.
2. TIER-1 (urgent — lane mismatch/security mismatch): `S08`, `S09`, `S13`, `S19`.
3. TIER-2 (merge conflicts de ownership / dependency graph): `S04↔S07`, `S07↔S08`, `S11↔S16`, `S16↔S19`.
4. TIER-2 (productization/detail debt): `S15`, `S16`, `S17`, `S18`.
5. TIER-3 (polish / governance): `S00`, `S01`, `S02`, `S03`, `META`.

## Veredito global

SOTA score atual: `6.8/10`.

O material tem base forte: framework rico, canonical sources bons e `S07/S08/S09` já mostram a direção certa. O problema é **desigualdade de rigor**. Parte da série ainda opera em `v1.0.0` com DoD pouco tipado, risk register subespecificado, PERT ausente e cronogramas incompatíveis com os próprios gates de 30 dias. Em especial, `HIGH_RISK` não está consistentemente mais detalhado que `STANDARD`.

SOTA target com enrichments: `10/10`.

Effort estimado: `48-64 horas` para elevar os 22 contracts de forma coerente, sendo ~`24-32h` de edição direta, ~`12-16h` de reconciliação cross-contract e ~`12-16h` de validação/normalização de evidence, runbooks e invariant registry.
