---
id: AUDIT-SPRINT-CONTRACTS-SOTA-R2
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.2.0
created: 2026-04-24
reviewers: [GPT via codex CLI — SOTA parceiro round 2]
supersedes: AUDIT-SPRINT-CONTRACTS-SOTA
superseded_by: null
tags: [audit, sota, sprint-contracts, round-2, remediation-validation]
---

# Codex SOTA Review Round 2 — Sprint Contracts pos-Lote 9.1+9.2

## Escopo
- Re-audit pos-remediacao do commit `d137d1e`.
- 21 spec contracts (`S-00..S-20`) + meta-contract + canonical sources + runbooks/ADRs criados no lote.
- Validacao manual + leitura dirigida + `python3 scripts/validate_references.py --json`.
- `python3 scripts/validate_specs.py` nao rodou nesta maquina: o script aborta por dependencia ausente (`jsonschema`), logo a validacao de schema segue pendente.

## Resumo executivo
- O Lote 9.1+9.2 fechou a maior parte do backlog estrutural do Round 1: lanes foram corrigidas em `S-08/S-09/S-13/S-19`, `STANDARD-PLUS` saiu de `S-09`, `S-20` foi alinhado para 14 canonical sources, os runbooks/ADRs faltantes foram criados e o `validate_references.py` agora retorna zero dangling references.
- A melhora mais objetiva esta em `EVT-XXX`: o baseline saiu de `34/155` itens DoD tipados no Round 1 para `115/229` agora. Cobertura total subiu de `21.9%` para `50.2%`, mas ainda restam `114` checkboxes sem evidence tipada.
- O principal residual mudou de natureza: nao e mais "falta massa documental"; agora e "massa documental ainda inconsistente". Os maiores pontos sao: meta-contract ainda `DRAFT`, cluster `S-00..S-06` ainda em padrao v1.0.0 compacto, normalizacao HIGH_RISK incompleta em `S-08/S-09`, drift de invariantes/TLA+ em `S-11/S-14`, e `S-20` ainda parcialmente acoplado a launch despite the stated split.

## Validacao Round 1 remediation

| Finding R1 | Status | Notes |
|---|---|---|
| Lane `S-08` `STANDARD -> HIGH_RISK` | ⚠️ | `lane` corrigido em `specs/04_sprints/S08/_spec_contract.md:27`, mas a normalizacao ficou incompleta: `tags` ainda carregam `standard` em `:14`, `inherits_from` nao inclui `INVARIANT-REGISTRY` em `:45-53`, e o contract nao tem gate explicito de PRR/sign-off HIGH_RISK (`:83-96`, `:159-164`). |
| Lane `S-09` `STANDARD -> HIGH_RISK` | ⚠️ | `lane` corrigido em `specs/04_sprints/S09/_spec_contract.md:25`, mas `tags` ainda ficam em `standard` em `:14`, `inherits_from` segue sem `INVARIANT-REGISTRY` em `:51-60`, e o PRR fica subdimensionado em 7 sign-offs (`:139`, `:240`). |
| Lane `S-13` `STANDARD -> HIGH_RISK` | ✅ | Upgrade consolidado em `specs/04_sprints/S13/_spec_contract.md:25-27`, com `INVARIANT-REGISTRY` em `:45-53`. |
| Lane `S-19` `STANDARD -> HIGH_RISK` | ✅ | Upgrade consolidado em `specs/04_sprints/S19/_spec_contract.md:25-27`, `FF-HR-009` em `:26`, e rationale explicito em `:37-45`. |
| `S-09` pseudo-lane `STANDARD-PLUS` removida | ✅ | Nao ha mais ocorrencias de `STANDARD-PLUS`; `S-09` agora declara apenas `HIGH_RISK` em `specs/04_sprints/S09/_spec_contract.md:25`. |
| `S-12` DoD binario para reproducible build | ⚠️ | Melhorou no DoD (`specs/04_sprints/S12/_spec_contract.md:142`), mas a completude ainda reabre o `OR` em `:153` e o requirement-base segue aceitando `<100%` com documentacao em `:119`. |
| `S-20` "10 canonical sources" -> `14` | ✅ | Corrigido em `specs/04_sprints/S20/_spec_contract.md:29`, `:33`, `:38`, `:68`. |
| `S-12` SBOM alinhado a CycloneDX 1.5+ | ✅ | Alinhado em `specs/04_sprints/S12/_spec_contract.md:17`, `:29`, `:60`, `:81`, coerente com `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:44-48`. |
| `S-20` SBOM alinhado a CycloneDX 1.5+ | ⚠️ | O alinhamento conceitual foi feito (`specs/04_sprints/S20/_spec_contract.md:39`), mas o wording residual `Full SBOM v1.0 (CycloneDX 1.5+)` ainda aparece em `:138` e `:184`. |
| Propagacao `S-07 -> S-10/S-14` | ✅ | `S-10` agora lista `S-07` como hard blocker em `specs/04_sprints/S10/_spec_contract.md:183-185`; `S-14` cobre `S-07` via range `S-01..S-10 SEALED` em `specs/04_sprints/S14/_spec_contract.md:185`. |
| Propagacao `S-08 -> S-10/S-14` | ✅ | `S-10` agora lista `S-08` em `specs/04_sprints/S10/_spec_contract.md:184-185`; `S-14` cobre `S-08` no range `S-01..S-10 SEALED` em `specs/04_sprints/S14/_spec_contract.md:185`. |
| Novos INVs propagados ao registry `§3.12` | ✅ | `INV-DEDUP-CONSISTENCY`, `INV-RATE-LIMIT-PROPORTIONALITY`, `INV-OBS-*`, `INV-BYOK-*`, `INV-ONBOARD-*` estao em `specs/03_architecture/invariant_registry.md:162-178`. |
| Runbook stubs criados | ✅ | Os stubs/novos runbooks existem em `specs/05_quality/runbooks/` incluindo `RB-FM-059`, `RB-FM-105`, `RB-FM-151`, `RB-FM-153`, `RB-FM-157`, `RB-FM-201`, `RB-FM-250`, `RB-FM-305`, `RB-FM-SIGNUP-FAILED`, `RB-OBS-CARDINALITY-001`, `RB-BILLING-001`, `RB-BILLING-002`, `RB-DSR-ERASURE-INCOMPLETE`, `RB-DATA-RESIDENCY-LEAK`. |
| ADR-0015 criada | ✅ | `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:1-57`. |
| ADR-0016 criada | ✅ | `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md:1-66`. |
| ADR-0017 criada | ✅ | `specs/03_architecture/adrs/ADR-0017-grafana-cloud-vs-self-hosted-observability.md:1-77`. |
| Meta-contract promovido acima de `DRAFT` | ❌ | Ainda `DRAFT`/`staffing-blocked` em `specs/04_sprints/_sprint_creation_contract.md:4`, `:19-22`. |
| Secao "Alternativas consideradas" nas ADRs | ✅ | Todas as ADRs visiveis em `specs/03_architecture/adrs/` possuem a secao, em PT ou EN: `ADR-0012:42`, `ADR-0013:49`, `ADR-0014:50`, `ADR-0015:48`, `ADR-0016:56`, `ADR-0017:57`. |
| PERT em `S-00..S-06` | ❌ | O cluster legado continua sem tabela PERT real; ha apenas listas/estimativas planas em `S-00:108-124`, `S-01:121-137`, `S-02:110-123`, `S-03:113-128`, `S-04:111-124`, `S-05:107-120`, `S-06:113-127`. |
| Cost regression gate | ⚠️ | O meta-contract ja exige perf regression gate em `specs/04_sprints/_sprint_creation_contract.md:239`, e `S-14` adicionou um gate de custo explicito em `specs/04_sprints/S14/_spec_contract.md:168`, mas nao houve propagacao sistemica para os outros contracts relevantes. |
| Anti-scope robusto em sprint LOW_RISK (`S-18`) | ✅ | O anti-scope foi endurecido com gate cross-functional explicito em `specs/04_sprints/S18/_spec_contract.md:37`, `:43`, `:180-189`. |
| Ownership clash `S-04 CAP-AC-004` vs `S-07 CAP-EVICT-002` | ❌ | Continua sem fronteira formal ou override: `specs/04_sprints/S04/_spec_contract.md:59` vs `specs/04_sprints/S07/_spec_contract.md:64`. |
| Ownership clash `S-07 CAP-EVICT-003` vs `S-08 CAP-QUOTA-001/002` | ❌ | Continua sem decomposicao de ownership: `specs/04_sprints/S07/_spec_contract.md:65` vs `specs/04_sprints/S08/_spec_contract.md:61-62`. |

**Remediation effectiveness (tracked R1 subset):** `16/23` major R1 items fechados, `3/23` parcialmente fechados, `4/23` ainda abertos. Em percentual: `69.6%` fully closed, `82.6%` at least partially addressed.

**Cross-reference validator:** `python3 scripts/validate_references.py --json` retornou `0` dangling references e apenas orphans catalogados. Esse era um gap estrutural real no Round 1 e agora esta efetivamente fechado.

## Findings residuais (carry-forward R1)

- **CF-01 — Meta-contract continua abaixo da autoridade que ele proprio exige.** `specs/04_sprints/_sprint_creation_contract.md:4`, `:19-22` mantem o baseline em `DRAFT` e `staffing-blocked`, embora o documento ja esteja atuando como fonte normativa para 21 downstreams. Promover para `REVIEW` ja e razoavel; `FROZEN` ainda depende de reviewer population e schema validation executavel.
- **CF-02 — `S-00..S-06` ainda nao fizeram a travessia minima para o padrao v1.1.0.** Todos seguem `version: "1.0.0"` em `specs/04_sprints/S00/_spec_contract.md:6` ate `S06:6`, continuam sem PERT explicito e preservam risk registers simplificados.
- **CF-03 — O gap de rigor entre o cluster legado e o cluster v1.1.0 continua material para promotion planning.** Cobertura `EVT-XXX` em `S-00..S-06` ficou em `16/52 = 30.8%`, contra `99/177 = 55.9%` em `S-07..S-20`; promover sprints tardias sem uplift minimo das fundacoes documentais aumenta o risco de gates desiguais.
- **CF-04 — Cost regression gate ainda nao virou norma transversal.** O meta-contract fala apenas em perf regression genica em `specs/04_sprints/_sprint_creation_contract.md:239`; somente `S-14` trouxe gate economico explicito em `specs/04_sprints/S14/_spec_contract.md:168`.
- **CF-05 — Clash de ownership em TTL/eviction segue aberto.** `S-04` entrega `CAP-AC-004` "AC TTL management" em `specs/04_sprints/S04/_spec_contract.md:59`, enquanto `S-07` reatribui a semantica para `CAP-EVICT-002` em `specs/04_sprints/S07/_spec_contract.md:64` sem contrato de override.
- **CF-06 — Clash de ownership em quota segue aberto.** `S-07` toma quota enforcement em `specs/04_sprints/S07/_spec_contract.md:65`, mas `S-08` tambem assume storage/bandwidth quota em `specs/04_sprints/S08/_spec_contract.md:61-62`. Ainda falta a fronteira "quota de storage via eviction" vs "quota/rate-limit behavioral".
- **CF-07 — O encaminhamento de compressao de `S-07` para `S-11` ainda esta errado.** `specs/04_sprints/S07/_spec_contract.md:130` continua deferindo compressao para `S-11`, mas `S-11` e sprint de privacy/DSR, nao de storage/perf.
- **CF-08 — A remediacao do binarismo em `S-12` ficou pela metade.** O DoD melhorou em `specs/04_sprints/S12/_spec_contract.md:142`, mas o requirement `R-S12-14` em `:119` e a completeness clause `10.s12.4` em `:153` continuam aceitando o branch "100% bit-identical OU documented sources", o que ainda conflita com o espirito binario do meta-contract (`specs/04_sprints/_sprint_creation_contract.md:177-179`).

## Novos findings R2

- **R2-01 — `S-08` virou HIGH_RISK so no campo `lane`; a normalizacao meta ainda nao fechou.** Falta `INVARIANT-REGISTRY` em `inherits_from` (`specs/04_sprints/S08/_spec_contract.md:45-53`), `tags` ainda dizem `standard` (`:14`) e nao ha gate explicito de PRR/sign-off apesar do proprio contract declarar `10-12 sign-offs` em `:38`.
- **R2-02 — `S-09` repete o mesmo problema de normalizacao HIGH_RISK.** `lane` esta correto em `specs/04_sprints/S09/_spec_contract.md:25`, mas `tags` continuam em `standard` (`:14`), `inherits_from` nao traz `INVARIANT-REGISTRY` (`:51-60`) e o sprint fecha com apenas 7 sign-offs (`:139`, `:240`) contra o proprio `10-12` de `:44`.
- **R2-03 — `S-12` ainda subestima a carga de approvers da sua propria lane.** O contract declara `HIGH_RISK (10–12 sign-offs)` em `specs/04_sprints/S12/_spec_contract.md:39`, mas o PRR DoD lista 8 papeis em `:146`.
- **R2-04 — `S-13` tambem fecha abaixo do seu budget de sign-offs.** `specs/04_sprints/S13/_spec_contract.md:39` define `HIGH_RISK (10–12 sign-offs)`, enquanto o PRR de `:133` enumera 9 papeis.
- **R2-05 — `S-11` reabre invariantes ja canonicamente existentes e muda severidade sem sincronizar o registry.** `specs/04_sprints/S11/_spec_contract.md:182-183` marca `INV-DATA-ERASURE-COMPLETE` e `INV-DATA-RESIDENCY` como `CRITICAL — novo`, mas o registry ja os define em `specs/03_architecture/invariant_registry.md:110` e `:154`, ambos `HIGH`. Isso e severity drift normativo.
- **R2-06 — `invariant_registry §3.12` ficou semanticamente stale.** O heading ainda diz "S-07..S-12 (Lote 9.1)" em `specs/03_architecture/invariant_registry.md:156-158`, mas a tabela vai ate `S-19` em `:172-178`.
- **R2-07 — A TLA+ coverage matrix do registry nao acompanha os novos invariantes CRITICAL.** `specs/03_architecture/invariant_registry.md:184-196` ainda cobre apenas o quarteto historico, mas `S-11` e `S-14` agora carregam invariantes CRITICAL adicionais em `specs/04_sprints/S11/_spec_contract.md:182-183` e `specs/04_sprints/S14/_spec_contract.md:155-156`. Pelo proprio registry, isso deveria puxar matriz e backlog TLA+.
- **R2-08 — `S-20` ainda carrega o titulo pre-remediacao de "72h Staging".** O H1 em `specs/04_sprints/S20/_spec_contract.md:17` fala `72h Staging`, enquanto o objetivo/requirements/DoD ja falam `30d sustained staging` em `:33`, `:107-110`, `:128`.
- **R2-09 — A separacao Engineering Gate vs Launch Orchestration em `S-20` continua acoplada por contagem de WI.** O DoD binary abre com `WIs SEALED: 8/8` em `specs/04_sprints/S20/_spec_contract.md:125`, mas `WI-S20-008` e launch prep marketing em `:226`. Logo, o engineering gate ainda depende de um WI de launch.
- **R2-10 — `S-20` ainda deixa launch dentro da propria Definition of Done e sem `EVT-XXX`.** As linhas `specs/04_sprints/S20/_spec_contract.md:144-148` continuam no corpo do DoD, embora `:119` diga que launch nao bloqueia o engineering gate.
- **R2-11 — A baseline de runbooks em `S-20` e `S-17` esta desatualizada.** `specs/04_sprints/S20/_spec_contract.md:135` e `:185`, alem de `specs/04_sprints/S17/_spec_contract.md:197`, ainda falam em `26 runbooks`, mas o repo hoje contem `40` arquivos em `specs/05_quality/runbooks/`.
- **R2-12 — `S-20` manteve wording residual de SBOM antiga apesar do alinhamento conceitual.** `specs/04_sprints/S20/_spec_contract.md:138` e `:184` ainda dizem `Full SBOM v1.0 (CycloneDX 1.5+)`, o que reintroduz ruidao de versao.
- **R2-13 — `S-20` duplica o gate TLA+ e mistura ambiente real com pre-GA.** O DoD ja exige `TLA+ all 4 specs` em `specs/04_sprints/S20/_spec_contract.md:137`, repete o mesmo em `:140`, e ainda fala em `Zero SEV-1 in prod in 30d prior to GA` em `:139`, embora o proprio texto trate esse periodo como staging/prod-like.
- **R2-14 — `S-18` ainda publica claims de compliance/security dependentes de `S-11`, mas nao declara essa dependencia.** O escopo de `specs/04_sprints/S18/_spec_contract.md:107-113` depende de DPA template, sub-processors e privacy notice, mas as dependencies de `:193-200` listam `S-12` e `S-16`, nao `S-11`.
- **R2-15 — `S-14 -> S-19` nao ficou bidirecional.** `specs/04_sprints/S14/_spec_contract.md:198` diz que `S-19` consome enterprise onboarding com BYOK + DPA flow, mas `specs/04_sprints/S19/_spec_contract.md:190-201` nao menciona `S-14` nem como soft blocker.
- **R2-16 — `S-10 -> S-13` nao ficou bidirecional.** `specs/04_sprints/S10/_spec_contract.md:196` diz que `S-13` consome usage dashboard, mas `specs/04_sprints/S13/_spec_contract.md:181-195` nao lista `S-10`.
- **R2-17 — Os runbooks mais criticos criados no lote ainda estao em nivel de stub e pedem expansao antes de PRR.** Os casos mais sensiveis sao `RB-FM-105` (`specs/05_quality/runbooks/RB-FM-105-region-replication-diverge.md:14-44`), `RB-BILLING-001` (`specs/05_quality/runbooks/RB-BILLING-001.md:14-46`), `RB-BILLING-002` (`specs/05_quality/runbooks/RB-BILLING-002.md:14-40`), `RB-FM-SIGNUP-FAILED` (`specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md:14-44`), `RB-DSR-ERASURE-INCOMPLETE` (`specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md:14-46`) e `RB-DATA-RESIDENCY-LEAK` (`specs/05_quality/runbooks/RB-DATA-RESIDENCY-LEAK.md:14-45`).
- **R2-18 — Schema validation continua bloqueada por tooling, nao por conteudo validado.** `python3 scripts/validate_specs.py` aborta com `ERRO: jsonschema nao instalado`; o proprio script depende desse import em `scripts/validate_specs.py:33-38`. Hoje nao ha evidncia de schema pass/fail pos-Lote 9.1+9.2.
- **R2-19 — As novas invariantes nao receberam back-links consistentes nos canonical sources de dominio.** O registry cita `INV-OBS-CARDINALITY-BUDGET` em `specs/03_architecture/invariant_registry.md:164`, mas `observability_model.md` traz o budget sem o ID em `specs/03_architecture/observability_model.md:419-429`; `INV-BYOK-CRYPTO-SOVEREIGNTY` existe no registry em `invariant_registry.md:174`, mas `key_management.md` so descreve o kill switch sem o ID em `specs/03_architecture/key_management.md:117-123`; `INV-CONSENT-PROOF-VERIFIABLE` aparece no registry em `invariant_registry.md:168`, mas `privacy_model.md` lista os campos de proof sem nomear a invariante em `specs/03_architecture/privacy_model.md:255-257`.
- **R2-20 — `S-09` ainda tem cobertura EVT muito baixa justamente nos itens mais nucleares.** `10/11` itens DoD seguem sem evidence tipada, inclusive metricas, logs, tracing, audit, dashboards e synthetic canary (`specs/04_sprints/S09/_spec_contract.md:129-137`, `:139`).
- **R2-21 — `S-10` tambem continua quase sem EVT nos gates centrais de billing.** `10/11` itens DoD seguem sem `EVT-XXX`, inclusive E2E, chaos Stripe outage, replay, idempotencia, TLA e SOC walkthrough (`specs/04_sprints/S10/_spec_contract.md:122-130`, `:132`).
- **R2-22 — `S-11` ainda carrega o maior backlog de evidence entre as v1.1.** `12/14` itens DoD seguem sem EVT, exatamente nas provas de erasure, residency, legal templates, controles privacy e TLA (`specs/04_sprints/S11/_spec_contract.md:147-159`).
- **R2-23 — `S-12` segue com as evidencias mais importantes sem tipo.** `10/12` itens DoD seguem sem EVT, inclusive SLSA, SBOM, `cargo-audit`, `cargo-deny`, Dependabot, Dependency-Track e PRR (`specs/04_sprints/S12/_spec_contract.md:135-146`).
- **R2-24 — `S-20` ainda e o maior acumulador absoluto de gaps EVT entre os contracts v1.1.** Restam `9` linhas sem `EVT-XXX`, incluindo `WIs SEALED`, `30d sustained staging`, `Zero SEV-1`, `All TLA+ specs verdes` e todas as linhas de launch orchestration (`specs/04_sprints/S20/_spec_contract.md:125`, `:128`, `:139-148`).
- **R2-25 — `S-08` e `S-09` ainda deixam toolchain/proof incompletos para observabilidade/rate-limiting.** `S-09` referencia `specs/_schemas/log_event.schema.json` em `specs/04_sprints/S09/_spec_contract.md:95` e `cardinality_check.py` em `:130`, `:167`, `:212`, mas o checkout so expoe `specs/_schemas/front_matter.schema.json` e nenhum `cardinality_check.py`. Como entregavel futuro isso e aceitavel, mas como contrato atual a trilha de evidence ainda e hipotetica.

## EVT-XXX coverage matrix

**Delta vs Round 1:** o Round 1 havia encontrado `121/155` checkboxes DoD sem `EVT-XXX` (`34/155` tipados, `21.9%`). Agora o estado e `114/229` sem `EVT-XXX` (`115/229` tipados, `50.2%`). Em termos absolutos, o backlog caiu pouco (`-7` linhas) apesar de a superficie DoD ter crescido `+74` linhas; em termos relativos, houve melhora real de `+28.3 pp`.

| Sprint | DoD items | Com EVT | Sem EVT | Coverage % |
|---|---:|---:|---:|---:|
| S-00 | 6 | 3 | 3 | 50.0% |
| S-01 | 12 | 11 | 1 | 91.7% |
| S-02 | 8 | 0 | 8 | 0.0% |
| S-03 | 6 | 2 | 4 | 33.3% |
| S-04 | 7 | 0 | 7 | 0.0% |
| S-05 | 6 | 0 | 6 | 0.0% |
| S-06 | 7 | 0 | 7 | 0.0% |
| S-07 | 10 | 7 | 3 | 70.0% |
| S-08 | 10 | 5 | 5 | 50.0% |
| S-09 | 11 | 1 | 10 | 9.1% |
| S-10 | 11 | 1 | 10 | 9.1% |
| S-11 | 14 | 2 | 12 | 14.3% |
| S-12 | 12 | 2 | 10 | 16.7% |
| S-13 | 12 | 8 | 4 | 66.7% |
| S-14 | 15 | 12 | 3 | 80.0% |
| S-15 | 11 | 8 | 3 | 72.7% |
| S-16 | 13 | 11 | 2 | 84.6% |
| S-17 | 11 | 9 | 2 | 81.8% |
| S-18 | 13 | 11 | 2 | 84.6% |
| S-19 | 13 | 10 | 3 | 76.9% |
| S-20 | 21 | 12 | 9 | 57.1% |

### Missing EVT lines (all current gaps)

### S-00
- `specs/04_sprints/S00/_spec_contract.md:69` — Success metrics tem thresholds numericos (sem "best effort").
- `specs/04_sprints/S00/_spec_contract.md:70` — Roadmap revisado por: Tech Lead, SRE Lead, Security Lead, Finance/Cost Owner. Dates realistas dada capacity.
- `specs/04_sprints/S00/_spec_contract.md:72` — Todo sprint skeleton tem: objetivo, lane, forcing factors (se HIGH), inherits_from >= minimo, WIs antecipados count.

### S-01
- `specs/04_sprints/S01/_spec_contract.md:76` — 7 WIs SEALED.

### S-02
- `specs/04_sprints/S02/_spec_contract.md:73` — 6 WIs SEALED.
- `specs/04_sprints/S02/_spec_contract.md:74` — Property test: nenhum read retorna blob fora de namespace do tenant.
- `specs/04_sprints/S02/_spec_contract.md:75` — TLA+ `tenant_isolation.tla` + `cas_integrity.tla` verdes em CI.
- `specs/04_sprints/S02/_spec_contract.md:76` — Load test read 50k QPS x 10 min em staging.
- `specs/04_sprints/S02/_spec_contract.md:77` — Client verify habilitado por default em SDK; opt-out documentado.
- `specs/04_sprints/S02/_spec_contract.md:78` — Side-channel test: medir latencia 404 vs 403 -> p99 diff < 5ms.
- `specs/04_sprints/S02/_spec_contract.md:79` — Runbook `RB-FM-253` dry-run.
- `specs/04_sprints/S02/_spec_contract.md:80` — SBOM + signed release.

### S-03
- `specs/04_sprints/S03/_spec_contract.md:76` — 8 WIs SEALED.
- `specs/04_sprints/S03/_spec_contract.md:79` — SSO flow E2E: signup -> tenant created -> PAT emitted -> use em CAS write -> revoke -> falha.
- `specs/04_sprints/S03/_spec_contract.md:80` — MFA WebAuthn testado com 3 devices diferentes (YubiKey, platform authenticator).
- `specs/04_sprints/S03/_spec_contract.md:81` — LGPD DSR support: export PAT list + revoke em erasure pipeline.

### S-04
- `specs/04_sprints/S04/_spec_contract.md:73` — 6 WIs SEALED.
- `specs/04_sprints/S04/_spec_contract.md:74` — E2E: Bazel build com `--remote_cache=corelink://...` -> cache hit reduz tempo em > 50% em rebuilds.
- `specs/04_sprints/S04/_spec_contract.md:75` — Property test: AC entry de Tenant A nunca retornada a Tenant B.
- `specs/04_sprints/S04/_spec_contract.md:76` — TLA+ invariantes verdes.
- `specs/04_sprints/S04/_spec_contract.md:77` — Chaos: TTL expiry sob load; stale read handling.
- `specs/04_sprints/S04/_spec_contract.md:78` — SLO-AVAIL-AC e SLO-LAT-AC-HIT sustained 72h staging.
- `specs/04_sprints/S04/_spec_contract.md:79` — PRR + adversarial review.

### S-05
- `specs/04_sprints/S05/_spec_contract.md:72` — 6 WIs SEALED.
- `specs/04_sprints/S05/_spec_contract.md:73` — Upload 1 GiB single blob bem-sucedido; hash verify end-to-end.
- `specs/04_sprints/S05/_spec_contract.md:74` — Dedup: mesmo chunk usado em 2 blobs distintos dentro do tenant.
- `specs/04_sprints/S05/_spec_contract.md:75` — Orphan parts: abort testado chaos-style.
- `specs/04_sprints/S05/_spec_contract.md:76` — Property test: manifest verify rejeita arvores invalidas.
- `specs/04_sprints/S05/_spec_contract.md:77` — SLO-LAT-CAS-PUT-MULTIPART definido (novo SLO) + sustained.

### S-06
- `specs/04_sprints/S06/_spec_contract.md:75` — 7 WIs SEALED.
- `specs/04_sprints/S06/_spec_contract.md:76` — TLA+ `gc_correctness.tla` verde; adicionalmente: property test em Rust cobrindo race Mark+UpdateActionResult.
- `specs/04_sprints/S06/_spec_contract.md:77` — Chaos: rodar GC durante write load -> zero falso positivo (no blob deletado com AC ativa).
- `specs/04_sprints/S06/_spec_contract.md:78` — Undelete testado: soft-delete -> undelete dentro do grace.
- `specs/04_sprints/S06/_spec_contract.md:79` — RB-FM-300 dry-run executado em staging.
- `specs/04_sprints/S06/_spec_contract.md:80` — PRR + adversarial review focado em GC correctness.
- `specs/04_sprints/S06/_spec_contract.md:81` — Storage reclaimed measurable: 30d simulation em staging retorna > 0 bytes reclaimed.

### S-07
- `specs/04_sprints/S07/_spec_contract.md:84` — 5 WIs SEALED.
- `specs/04_sprints/S07/_spec_contract.md:85` — Dedup ratio measurable: >= 3 tenants em staging com workloads Docker pulls; `corelink_dedup_ratio{type=chunk}` >= 2.5x sustained 7d (target SOTA: >= 3x).
- `specs/04_sprints/S07/_spec_contract.md:90` — Alerts armados: dedup ratio anomaly detection (SEV-3) + quota 95% breach (SEV-2 per-tenant).

### S-08
- `specs/04_sprints/S08/_spec_contract.md:87` — 6 WIs SEALED.
- `specs/04_sprints/S08/_spec_contract.md:90` — Load test bandwidth quota: tenant consome 100% de bandwidth monthly -> writes rejected with `over_quota` 429 + Retry-After = days-until-month-reset.
- `specs/04_sprints/S08/_spec_contract.md:93` — Alerts armados: per-tenant quota 95% (SEV-3 per-tenant), global circuit breaker trip (SEV-1 oncall), abuse score threshold (SEV-2).
- `specs/04_sprints/S08/_spec_contract.md:94` — Rate limit headers RFC 9331 (RateLimit, RateLimit-Policy) implementados — padrao IETF atualizado.
- `specs/04_sprints/S08/_spec_contract.md:96` — Coverage >= 90%.

### S-09
- `specs/04_sprints/S09/_spec_contract.md:129` — WIs SEALED: 7/7.
- `specs/04_sprints/S09/_spec_contract.md:130` — Metricas: 100% das metricas em `observability_model.md §4.2` emitindo em staging com cardinality budget respeitado. Validador `cardinality_check.py` verde.
- `specs/04_sprints/S09/_spec_contract.md:131` — Logs: schema validation em CI + PII redaction DLP test 0 leaks em 10k fixtures.
- `specs/04_sprints/S09/_spec_contract.md:132` — Tracing: traces W3C-compliant + exemplars funcionando (click em Grafana abre Tempo).
- `specs/04_sprints/S09/_spec_contract.md:133` — Audit: CloudEvents emitidos para todos os 8 subjects + chain verify daily job verde por 7d.
- `specs/04_sprints/S09/_spec_contract.md:134` — Dashboards: 12/12 live em Grafana com data flowing; screenshot per dashboard arquivada em `docs/dashboards/`.
- `specs/04_sprints/S09/_spec_contract.md:135` — Alerts: `promtool test rules` verde para 100% das regras; multi-burn-rate alert dry-run via injecao de SLO breach sintetico confirma fire em < 5min.
- `specs/04_sprints/S09/_spec_contract.md:136` — PagerDuty: SEV-1 + SEV-2 dispatch end-to-end testado (synthetic page) ack < 5min.
- `specs/04_sprints/S09/_spec_contract.md:137` — Synthetic canary: 24/7 de 3 regioes (us-east, eu-west, ap-south) sustentado 72h sem gap.
- `specs/04_sprints/S09/_spec_contract.md:139` — Sign-offs (HIGH_RISK): SRE lead + Privacy officer + Security lead + Engineer responsavel + QA + Product + Compliance officer.

### S-10
- `specs/04_sprints/S10/_spec_contract.md:122` — WIs SEALED: 7/7.
- `specs/04_sprints/S10/_spec_contract.md:123` — E2E: tenant signup -> 30 dias uso simulado -> monthly invoice generated -> Stripe charged (test mode) -> reconciliation green em 3 layers.
- `specs/04_sprints/S10/_spec_contract.md:124` — Chaos test: Stripe outage 1h -> events queued (PAT-QUEUE-EVENTS-001) -> apos Stripe recovery, retry sucede com 0 lost; 0 dup.
- `specs/04_sprints/S10/_spec_contract.md:125` — Drift detection: simular drift artificial 0.5% em counter -> reconciliation worker detecta + dispara SEV-2 em < 24h.
- `specs/04_sprints/S10/_spec_contract.md:126` — Property test: replay 1M events idempotent — sum(counters) identico ao replay; sem dup; sem loss.
- `specs/04_sprints/S10/_spec_contract.md:127` — Idempotency test: 100 retries de mesmo event -> 1 charge no Stripe (test mode).
- `specs/04_sprints/S10/_spec_contract.md:128` — Replay forensic: gerar invoice fake -> run replay endpoint -> output match Stripe invoice byte-a-byte (modulo Stripe metadata).
- `specs/04_sprints/S10/_spec_contract.md:129` — PRR HIGH_RISK: Finance reviewer + Legal (DPA reference) + Security lead + SRE + Product + Compliance officer + Engineer + QA + 2 peers + Privacy officer (PII em invoice).
- `specs/04_sprints/S10/_spec_contract.md:130` — TLA+ spec `billing_atomicity.tla` (state machine event -> counter -> invoice; no-loss, no-dup) verde em CI.
- `specs/04_sprints/S10/_spec_contract.md:132` — SOC 2 walkthrough: Finance + auditor (mock) consegue reconstruir 1 invoice from R2 events em < 30 min.

### S-11
- `specs/04_sprints/S11/_spec_contract.md:147` — WIs SEALED: 8/8.
- `specs/04_sprints/S11/_spec_contract.md:148` — E2E erasure: fake user signup -> use product 30d -> DSR erasure request -> 0 records cross-backend em <= 30d (verification job verde).
- `specs/04_sprints/S11/_spec_contract.md:149` — Erasure cross-backend coverage: 7 backends (D1, Neon, R2, KV, DO, Loki, Stripe) com test isolado per backend + integrated.
- `specs/04_sprints/S11/_spec_contract.md:150` — Crypto-erase BYOK: simulate enterprise BYOK + DSR erasure -> key destroyed; data inacessivel (NIST SP 800-88 evidence).
- `specs/04_sprints/S11/_spec_contract.md:151` — Residency: 10k tenant EU + 10k US property test verde; 0 cross-region leaks.
- `specs/04_sprints/S11/_spec_contract.md:153` — Consent proof verifiable: re-compute notice_text_hash em backend -> match record stored.
- `specs/04_sprints/S11/_spec_contract.md:154` — RB-BREACH-NOTIF dry-run executado com Legal + Privacy Officer + Security Lead; time-to-decision <= 4h.
- `specs/04_sprints/S11/_spec_contract.md:155` — Templates legais drafted em 3 jurisdictions (LGPD/GDPR/CCPA) revisados por Legal externo.
- `specs/04_sprints/S11/_spec_contract.md:156` — CTRL-PRIV-030 (erasure pipeline) + CTRL-PRIV-031 (residency) em prod com evidence.
- `specs/04_sprints/S11/_spec_contract.md:157` — DPIA preenchido para 3 features (dedup, telemetry, billing).
- `specs/04_sprints/S11/_spec_contract.md:158` — PRR HIGH_RISK: Privacy Officer + Legal + DPO interim + Security lead + Compliance officer + SRE + Engineer + QA + Product + 2 peers.
- `specs/04_sprints/S11/_spec_contract.md:159` — TLA+ spec `dsr_erasure_atomicity.tla` (verifica cross-backend erasure e atomic ou compensating-rollback) verde em CI.

### S-12
- `specs/04_sprints/S12/_spec_contract.md:135` — WIs SEALED: 7/7.
- `specs/04_sprints/S12/_spec_contract.md:136` — SLSA L3 attestation publicada em Rekor transparency log para todos os releases pos-S-12 (verifiable via `rekor-cli search --rekor_server https://rekor.sigstore.dev`).
- `specs/04_sprints/S12/_spec_contract.md:137` — SBOM disponivel como GitHub release asset + Dependency-Track ingestion verde + NTIA minimum elements check verde.
- `specs/04_sprints/S12/_spec_contract.md:139` — `cargo-audit` zero findings HIGH/CRITICAL em `Cargo.lock` no momento do release.
- `specs/04_sprints/S12/_spec_contract.md:140` — `cargo-deny` policy verde em CI; license allowlist enforced; 0 yanked deps.
- `specs/04_sprints/S12/_spec_contract.md:141` — Dependabot ativo + weekly grouped PRs functioning; auto-merge minor patches working.
- `specs/04_sprints/S12/_spec_contract.md:143` — Dependency-Track integration: SBOM ingerido + 1 CVE simulado triggers Slack alert.
- `specs/04_sprints/S12/_spec_contract.md:144` — RB-FM-156 (dep malicioso scenario) dry-run executado com Security lead + SRE.
- `specs/04_sprints/S12/_spec_contract.md:145` — RB-FM-157 (typosquatting) dry-run executado.
- `specs/04_sprints/S12/_spec_contract.md:146` — PRR HIGH_RISK: Security lead + SRE + Engineer + Compliance officer + Product + QA + 2 peers + AppSec advisor.

### S-13
- `specs/04_sprints/S13/_spec_contract.md:123` — WIs SEALED: 6/6.
- `specs/04_sprints/S13/_spec_contract.md:130` — CTRL-AUDIT-003 (MFA attestation admin ops) enforced + audit chain integrity verified.
- `specs/04_sprints/S13/_spec_contract.md:131` — CTRL-AUTH-010 (MFA fresh <= 30min) enforced; expiry test passes.
- `specs/04_sprints/S13/_spec_contract.md:133` — PRR HIGH_RISK: SRE lead + Security lead + Engineer + QA + Compliance officer + Product + 2 peers + Architect + AppSec.

### S-14
- `specs/04_sprints/S14/_spec_contract.md:117` — WIs SEALED: 9/9.
- `specs/04_sprints/S14/_spec_contract.md:128` — FIPS 140-3 / 140-2 compliance documented per provider em `compliance/byok-fips-matrix.md`.
- `specs/04_sprints/S14/_spec_contract.md:129` — PRR HIGH_RISK: Security lead + Crypto SME + SRE + Privacy officer + Legal + Compliance officer + Engineer + QA + Product + 2 peers + AppSec.

### S-15
- `specs/04_sprints/S15/_spec_contract.md:128` — WIs SEALED: 6/6.
- `specs/04_sprints/S15/_spec_contract.md:137` — PRR STANDARD: Engineer + QA + Product + DevX advisor + Docs lead.
- `specs/04_sprints/S15/_spec_contract.md:138` — Runbook: nenhum novo runbook (CLI is read-mostly + diagnostic; failure paths ja cobertos em S-01..S-04 runbooks).

### S-16
- `specs/04_sprints/S16/_spec_contract.md:130` — WIs SEALED: 7/7.
- `specs/04_sprints/S16/_spec_contract.md:141` — PRR STANDARD: Frontend lead + Privacy officer + Engineer + QA + Product + Designer + a11y advisor.

### S-17
- `specs/04_sprints/S17/_spec_contract.md:132` — WIs SEALED: 6/6.
- `specs/04_sprints/S17/_spec_contract.md:142` — PRR STANDARD: SRE lead + Engineer + Oncall manager + Product + QA + Compliance officer + Privacy officer (post-mortem privacy incidents).

### S-18
- `specs/04_sprints/S18/_spec_contract.md:136` — WIs SEALED: 5/5.
- `specs/04_sprints/S18/_spec_contract.md:148` — PRR LOW_RISK: Docs lead + Engineer + Product + Privacy Officer (compliance page) + Finance (pricing) + Legal (terms).

### S-19
- `specs/04_sprints/S19/_spec_contract.md:130` — WIs SEALED: 6/6.
- `specs/04_sprints/S19/_spec_contract.md:141` — PRR HIGH_RISK: Privacy Officer + Legal + Engineer + QA + Product + SRE + Compliance officer + Sales lead + 2 peers + Privacy/UX advisor.
- `specs/04_sprints/S19/_spec_contract.md:142` — Runbook: cria RB-FM-SIGNUP-FAILED stub se signup atomicity falha em prod.

### S-20
- `specs/04_sprints/S20/_spec_contract.md:125` — WIs SEALED: 8/8.
- `specs/04_sprints/S20/_spec_contract.md:128` — 30d sustained staging: zero SEV-1; < 3 SEV-2 not resolved (concurrent observation period; documented post-S-17 chaos automation 4-week period).
- `specs/04_sprints/S20/_spec_contract.md:139` — Zero SEV-1 in prod in 30d prior to GA (production-like staging observation).
- `specs/04_sprints/S20/_spec_contract.md:140` — All TLA+ specs verdes em CI sustained.
- `specs/04_sprints/S20/_spec_contract.md:144` — Marketing ready: press release reviewed por PR + Legal.
- `specs/04_sprints/S20/_spec_contract.md:145` — 5 blog posts published em staging blog.
- `specs/04_sprints/S20/_spec_contract.md:146` — 3 case studies drafted com lighthouse customer testimonials.
- `specs/04_sprints/S20/_spec_contract.md:147` — Product Hunt launch assets prepared.
- `specs/04_sprints/S20/_spec_contract.md:148` — Note: launch orchestration nao bloqueia engineering gate; engineering gate aprova GA-go independentemente.

## Sprint-by-sprint priority gaps

### S-00 (top 3 gaps)
- `specs/04_sprints/S00/_spec_contract.md:108-124` ainda usa estimativa plana, sem PERT, e sem mid-check explicito.
- `specs/04_sprints/S00/_spec_contract.md:69-72` retira `EVT-XXX` justamente dos gates de qualidade do roadmap.
- `specs/04_sprints/S00/_spec_contract.md:137-142` mantem risk table em 4 colunas simples.

### S-01 (top 3 gaps)
- `specs/04_sprints/S01/_spec_contract.md:121-137` continua sem PERT, so com horas agregadas.
- `specs/04_sprints/S01/_spec_contract.md:147-150` ainda usa risk register minimalista.
- `specs/04_sprints/S01/_spec_contract.md:76` ainda nao tipa o fechamento do sprint (`WIs SEALED`).

### S-02 (top 3 gaps)
- `specs/04_sprints/S02/_spec_contract.md:73-80` segue com `0/8` DoD items tipados.
- `specs/04_sprints/S02/_spec_contract.md:110-123` continua sem PERT.
- `specs/04_sprints/S02/_spec_contract.md:133-137` mantem risk table de 3 colunas.

### S-03 (top 3 gaps)
- `specs/04_sprints/S03/_spec_contract.md:76-81` ainda deixa `4/6` DoD items sem EVT.
- `specs/04_sprints/S03/_spec_contract.md:113-128` nao traz PERT.
- `specs/04_sprints/S03/_spec_contract.md:136-143` usa risk table simplificada.

### S-04 (top 3 gaps)
- `specs/04_sprints/S04/_spec_contract.md:73-79` segue com `0/7` DoD items tipados.
- `specs/04_sprints/S04/_spec_contract.md:118` ainda mistura `HKDF + Ed25519 opt` sem ADR/ownership claro.
- `specs/04_sprints/S04/_spec_contract.md:133-140` continua com risk register minimalista.

### S-05 (top 3 gaps)
- `specs/04_sprints/S05/_spec_contract.md:72-77` segue com `0/6` DoD items tipados.
- `specs/04_sprints/S05/_spec_contract.md:107-120` nao traz PERT.
- `specs/04_sprints/S05/_spec_contract.md:129-134` segue em risk table curta e sem residual/detectability.

### S-06 (top 3 gaps)
- `specs/04_sprints/S06/_spec_contract.md:75-81` segue com `0/7` DoD items tipados.
- `specs/04_sprints/S06/_spec_contract.md:113-127` continua sem PERT.
- `specs/04_sprints/S06/_spec_contract.md:131-133` pede 30d staging clean, mas a timeline de `:125-127` nao orca explicitamente isso.

### S-07 (top 3 gaps)
- `specs/04_sprints/S07/_spec_contract.md:84-90` ainda deixa 3 itens DoD sem EVT, incluindo `WIs SEALED`, dedup ratio e alerting.
- `specs/04_sprints/S07/_spec_contract.md:64-65` continua sobrepondo ownership com `S-04` e `S-08`.
- `specs/04_sprints/S07/_spec_contract.md:130` ainda defere compressao para `S-11`, que nao e o sprint correto.

### S-08 (top 3 gaps)
- `specs/04_sprints/S08/_spec_contract.md:45-53` falta `INVARIANT-REGISTRY` apesar de `HIGH_RISK`; `:14` ainda tagga `standard`.
- `specs/04_sprints/S08/_spec_contract.md:83-96` nao tem PRR/sign-off gate de HIGH_RISK e ainda deixa 5 itens sem EVT.
- `specs/04_sprints/S08/_spec_contract.md:75-80` usa `X-Rate-Limit-Type`, enquanto `:94` pede RFC 9331 sem mapear a convivencia normativa.

### S-09 (top 3 gaps)
- `specs/04_sprints/S09/_spec_contract.md:129-137`, `:139` deixam `10/11` itens DoD sem EVT.
- `specs/04_sprints/S09/_spec_contract.md:14`, `:51-60` mostram normalizacao HIGH_RISK incompleta (`tags` stale + sem `INVARIANT-REGISTRY`).
- `specs/04_sprints/S09/_spec_contract.md:139`, `:240` fecham com 7 sign-offs, abaixo do `10-12` prometido em `:44`.

### S-10 (top 3 gaps)
- `specs/04_sprints/S10/_spec_contract.md:122-130`, `:132` deixam `10/11` itens DoD sem EVT.
- `specs/04_sprints/S10/_spec_contract.md:196` marca `S-13` como consumer, mas `S-13` nao o lista nas dependencies.
- `specs/04_sprints/S10/_spec_contract.md:130` promete `billing_atomicity.tla`, mas o registry TLA matrix nao reflete esse backlog.

### S-11 (top 3 gaps)
- `specs/04_sprints/S11/_spec_contract.md:147-159` deixam `12/14` itens DoD sem EVT.
- `specs/04_sprints/S11/_spec_contract.md:182-183` reclassificam invariantes existentes como `CRITICAL — novo`, divergindo do registry em `specs/03_architecture/invariant_registry.md:110`, `:154`.
- `specs/04_sprints/S11/_spec_contract.md:267-275` depende de runbooks que existem, mas `RB-DSR-ERASURE-INCOMPLETE` e `RB-DATA-RESIDENCY-LEAK` ainda estao em nivel stub.

### S-12 (top 3 gaps)
- `specs/04_sprints/S12/_spec_contract.md:135-146` deixam `10/12` itens DoD sem EVT.
- `specs/04_sprints/S12/_spec_contract.md:153` ainda reabre a logica `100% bit-identical OU documented sources`.
- `specs/04_sprints/S12/_spec_contract.md:39`, `:146` fecham o sprint com 8 approvers, abaixo do `10-12` declarado.

### S-13 (top 3 gaps)
- `specs/04_sprints/S13/_spec_contract.md:123`, `:130-133` ainda deixam 4 DoD lines sem EVT.
- `specs/04_sprints/S13/_spec_contract.md:39`, `:133` ficam em 9 sign-offs vs `10-12`.
- `specs/04_sprints/S13/_spec_contract.md:192-195` nao lista `S-10`, embora `S-10:196` o marque como outbound consumer.

### S-14 (top 3 gaps)
- `specs/04_sprints/S14/_spec_contract.md:117`, `:128-129` ainda deixam 3 DoD lines sem EVT.
- `specs/04_sprints/S14/_spec_contract.md:198` aponta `S-19` como consumer, mas `S-19` nao o lista de volta.
- `specs/04_sprints/S14/_spec_contract.md:213` promete `region_residency.tla`, mas o registry TLA matrix ainda nao o espelha.

### S-15 (top 3 gaps)
- `specs/04_sprints/S15/_spec_contract.md:128`, `:137-138` deixam 3 DoD lines sem EVT.
- `specs/04_sprints/S15/_spec_contract.md:190-193` diz que `S-16` e consumer, mas `S-16` nao depende de `S-15`; a outbound relation esta frouxa.
- `specs/04_sprints/S15/_spec_contract.md:234` cita onboarding video como mitigacao, mas video nao aparece nem como deliverable nem como anti-scope em `:168-176`.

### S-16 (top 3 gaps)
- `specs/04_sprints/S16/_spec_contract.md:130`, `:141` deixam 2 DoD lines sem EVT.
- `specs/04_sprints/S16/_spec_contract.md:202-204` nao espelha a dependencia indireta declarada por `S-15:190`.
- `specs/04_sprints/S16/_spec_contract.md:177-184` tem anti-scope bom, mas o scope principal de `:33` ainda e muito largo para um STANDARD.

### S-17 (top 3 gaps)
- `specs/04_sprints/S17/_spec_contract.md:132`, `:142` deixam 2 DoD lines sem EVT.
- `specs/04_sprints/S17/_spec_contract.md:197` ainda propaga a baseline antiga de `26 runbooks`.
- `specs/04_sprints/S17/_spec_contract.md:237-246` tem mitigacoes mais processuais do que control-oriented em varios riscos (`culture`, `handoff`, `game day quality`).

### S-18 (top 3 gaps)
- `specs/04_sprints/S18/_spec_contract.md:136`, `:148` deixam 2 DoD lines sem EVT.
- `specs/04_sprints/S18/_spec_contract.md:107-113`, `:199-200` deixam `S-11` fora das dependencies apesar de usar artefatos de privacy/compliance.
- `specs/04_sprints/S18/_spec_contract.md:241-248` tem mitigacoes corretas, mas ainda genericas para stale-doc drift (`quarterly docs review` e pouco para sprint de launch-facing docs).

### S-19 (top 3 gaps)
- `specs/04_sprints/S19/_spec_contract.md:130`, `:141-142` deixam 3 DoD lines sem EVT.
- `specs/04_sprints/S19/_spec_contract.md:190-201` nao espelha a dependencia enterprise/BYOK que `S-14:198` declara.
- `specs/04_sprints/S19/_spec_contract.md:142` considera `RB-FM-SIGNUP-FAILED` suficiente, mas o runbook ainda esta muito curto para um fluxo HIGH_RISK (`specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md:21-44`).

### S-20 (top 3 gaps)
- `specs/04_sprints/S20/_spec_contract.md:17`, `:33`, `:128` mantem inconsistencia `72h` vs `30d`.
- `specs/04_sprints/S20/_spec_contract.md:125`, `:144-148`, `:226` mostram que o launch soft-gate continua acoplado ao engineering gate por `8/8 WIs`.
- `specs/04_sprints/S20/_spec_contract.md:135`, `:185` ainda ancoram o gate em `26 runbooks`, nao nos `40` presentes no repo.

## Veredito Round 2

- **SOTA score atual:** `8.0/10` (vs `6.8/10` no Round 1).
- **Remediation effectiveness:** `69.6%` dos major findings R1 revalidados estao efetivamente fechados; `82.6%` estao pelo menos parcialmente enderecados.
- **EVT coverage:** `115/229 = 50.2%` tipados; melhora relevante, mas ainda abaixo do nivel "auditable by default" para `S-09..S-12` e `S-20`.
- **Pending effort para 10/10:** `44-56h` de trabalho documental/governance, concentrado em:
- EVT normalization (`S-09..S-12`, `S-20`, legado `S-00..S-06`).
- Promocao do meta-contract para `REVIEW` + reviewer model minimo.
- Resolucao dos ownership clashes (`S-04/S-07`, `S-07/S-08`).
- Limpeza final de `S-20` (72h vs 30d, 26 vs 40, engineering-vs-launch desacoplado de verdade).
- Expansao dos runbook stubs privacy/billing/onboarding/region.
- Atualizacao do registry/TLA matrix para novos invariantes CRITICAL.
- **Recommendation:** executar um **Lote 9.3 docs-only** antes de qualquer freeze adicional. Implementacao de `S-01/S-02` pode seguir, mas eu **nao** promoveria novos contracts para `READY/FROZEN` nem abriria cadeia `S-10+`/GA readiness sem fechar: (1) meta-contract `REVIEW`, (2) EVT backlog critico, (3) ownerships conflitantes, (4) limpeza final de `S-20`.
