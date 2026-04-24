---
id: AUDIT-LOTE5-GPT
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [GPT via codex CLI]
supersedes: null
superseded_by: null
tags: [audit, lote5, tla, runbooks]
---

# Audit Lote 5 + runbooks + TLA+ specs (GPT)

## Escopo revisado

Artefatos efetivamente revisados neste lote:

- **5.1** Framework v0.5.0: `FF-HR-011`, promoção de `REMOTE-CACHE-PRODUCT-PROFILE` / `INVARIANT-REGISTRY` / `KEY-MANAGEMENT`, expansão EVT `025..046`, tabela de aliases humanos (`specs/00_framework.md:2041-2057`, `2435-2458`, `2627-2705`).
- **5.2** Fechamento da contradição "TLA+ opcional vs obrigatório" para `INV-TENANT-ISOLATION` (`specs/03_architecture/auth_model.md:399-401`, `specs/03_architecture/storage_semantics_matrix.md:176`).
- **5.3** Fechamentos de contradições de retenção 7y, key format R2 HMAC, SBOM, SLO-AVAIL-CAS-GET e FMEA (`specs/03_architecture/auth_model.md:346-356`, `specs/03_architecture/remote_cache_product_profile.md:235-265`, `specs/03_architecture/slo_catalog.md:78-129`, `specs/03_architecture/failure_modes.md:68-83`).
- **5.4** `invariant_registry.md` + fechamento de CTRLs dangling em `security_model.md`.
- **5.5** `CTRL-PRIV-CONSENT-001..004` e remap de P2.1 (`specs/03_architecture/privacy_model.md:247-256`, `specs/03_architecture/compliance_matrix.md:117-128`).
- **5.6** Templates `sprint_contract.md` e `production_readiness_review.md` com sign-off lane-aware; `compliance_matrix.md` com `inherits_from`.
- **5.7** 8 runbooks FM + `_audits/matrix-stride-ctrl.csv`, `_audits/iso27001-soa.csv`, `_audits/lia-template.md`.
- **5.8** ADR-0012 (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md`).
- **5.9** ADR-0013 (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md`).
- **5.10** `key_management.md` + ADR-0014 (`specs/03_architecture/key_management.md`, `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md`).
- **5.11** `scripts/validate_references.py`.
- **5.12** 12 runbooks novos: `RB-FM-057`, `RB-FM-205`, `RB-FM-253`, `RB-FM-254`, `RB-FM-300`, `RB-FM-302`, `RB-BREACH-NOTIF`, `RB-KEY-COMPROMISE`, `RB-HSM-UNAVAILABLE`, `RB-BYOK-REVOKE`, `RB-GDPR-ERASURE-HOLD`, `RB-SLO-AVAIL-CP`.
- **5.13** 3 specs TLA+: `tenant_isolation.tla`, `gc_correctness.tla`, `cas_integrity.tla`, mais `specs/tla/README.md`.

## Previous findings — resolution check

### Lote 3+4 findings (GPT + Sonnet consolidados)

- `F-01 / S-01 (EVT taxonomy): partial / regression.` O corpus migrou de aliases livres para `EVT-001..046` e adicionou tabela de aliases (`specs/00_framework.md:2637-2684`, `2688-2705`), mas a correção ficou semântica e operacionalmente errada: `EVT-001` (`CI_LOG`) virou coringa para consent, DSR, auditoria de chaves e evidência de compliance (`specs/03_architecture/privacy_model.md:253-255`, `272-278`; `specs/03_architecture/key_management.md:53`, `99`, `224-254`; `specs/03_architecture/compliance_matrix.md:122`, `125`, `171-174`). Pior: a tabela de aliases introduz aliases ambíguos (`SCAN_REPORT`, `AUDIT_LOG`) em vez de eliminá-los (`specs/00_framework.md:2702-2705`). Não há `scripts/validate_evidence.py`.
- `F-02 (REMOTE-CACHE canonical source): partial.` A promoção formal foi feita no framework e ADR (`specs/00_framework.md:2456-2458`, `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:41-47`). O problema é que o próprio doc promovido continua split-brain: usa aliases/nomes de invariantes não canônicos e um forcing factor sem sentido para `INV-CASIdempotency` (`specs/03_architecture/remote_cache_product_profile.md:483-488`).
- `F-03 / S-02 (TLA+ obrigatório para INV-TENANT-ISOLATION): addressed, mas com ressalva forte.` A contradição textual foi removida (`specs/03_architecture/auth_model.md:399-401`, `specs/03_architecture/storage_semantics_matrix.md:176`, `specs/03_architecture/invariant_registry.md:64-65`). O problema agora não é mais "obrigatório vs opcional"; é que o modelo TLA+ ainda não prova a propriedade certa. Isso vira finding novo crítico abaixo.
- `F-04 / S-05 / S-16 (PAT dangling / catálogo incompleto): addressed ✓.` `scripts/validate_references.py` reporta `PAT definitions=51 uses=51` e zero dangling. A família `PAT-AUTHZ-002` agora existe e o catálogo deixa de quebrar em cross-ref.
- `F-05 / S-08 (SLO-AVAIL-CAS-GET e 429 within quota): partial.` A fórmula e o target enterprise foram alinhados (`specs/03_architecture/slo_catalog.md:78`, `84-94`, `123-129`). Mas a regra global "baseline ≥ 30 dias + owner on-call assinado" segue sem materialização por entry; o catálogo continua declarando targets sem anexar baseline/evidence por SLO (`specs/03_architecture/slo_catalog.md:33`, `60`, `94`, `103-170`).
- `F-06 / S-20 (inheritance retrofit / compliance_matrix orphan): partial.` `compliance_matrix.md` agora declara `inherits_from` (`specs/03_architecture/compliance_matrix.md:14`), e o texto "PENDENTE Lote 4" saiu dos templates principais. Mas a correção não fechou o contrato prometido em `00_framework.md:2485-2492`: `sprint_contract.md`, `production_readiness_review.md`, `work_item.md` e `subtask.md` continuam sem `inherits_from` / `local_deltas` no YAML de topo; `scripts/validate_specs.py` ainda faz só YAML + JSON Schema, não semântica de inheritance (`scripts/validate_specs.py:73-108`).
- `F-07 (P2.1 consent mapeado para CTRL errado): partial.` O remap existe e a família `CTRL-PRIV-CONSENT-001..004` foi criada (`specs/03_architecture/privacy_model.md:247-256`, `specs/03_architecture/compliance_matrix.md:122`). O gap novo: o modelo ainda não prova **consentimento informado**; os registros não preservam a versão/hash do notice/texto apresentado ao usuário. Isso é insuficiente para demonstrar Art. 7 GDPR em disputa.
- `F-08 (runbook para todo P0/P1): partial.` Os arquivos foram criados, mas a rastreabilidade ainda não fecha. `failure_modes.md` continua listando só o lote inicial de seis runbooks (`specs/03_architecture/failure_modes.md:246-262`), FM-253 aponta para o runbook errado (`:178` referencia `RB-FM-303`), e o validador local reporta 7 runbooks órfãos (`RB-FM-051`, `054`, `202`, `206`, `400`, `403`, `404`).
- `F-09 (reviewers reais no YAML): partial / theater.` Nos canonical sources novos a omissão virou aviso explícito de staffing block (`specs/03_architecture/invariant_registry.md:11-25`, `specs/03_architecture/key_management.md:11-25`, `specs/03_architecture/slo_catalog.md:11-24`). Isso é mais honesto que antes, mas o YAML continua vazio. Pior: os 3 ADRs estão `FROZEN` com `reviewers: []` (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:1-15`, idem ADR-0013/0014). Em doc canônico DRAFT eu aceito "staffing-blocked"; em ADR `FROZEN`, isso é teatro.
- `F-10 (supporting artifacts ausentes): addressed ✓.` Os três artefatos agora existem: `specs/_audits/matrix-stride-ctrl.csv`, `specs/_audits/iso27001-soa.csv`, `specs/_audits/lia-template.md`.
- `F-11 (clock semantics): addressed ✓.` DSRs agora têm `clock_start` / `clock_stop` explícitos (`specs/03_architecture/privacy_model.md:262-278`) e o pager model também (`specs/03_architecture/observability_model.md:345-356`).
- `F-12 (boundary Fase 1 remote cache vs Fase 2 remote execution): addressed ✓.` Os canonical sources centrais agora marcam explicitamente `execute-action` como Fase 2 / futuro (`specs/03_architecture/slo_catalog.md:35-38`, `specs/03_architecture/security_model.md:37-38`, `specs/03_architecture/observability_model.md:37-38`, `specs/03_architecture/data_model.md:36-37`).
- `F-13 (SOTA gaps enterprise: BYOK / deletion attestation / posture): partial.` `key_management.md` fecha boa parte do gap com BYOK, BYOE roadmap e erasure attestation (`specs/03_architecture/key_management.md:176-191`, `235-248`). O que falta para competir de verdade com os melhores é productização operacional: self-hosted / hybrid / air-gapped posture, DPA/TIA templates reais e pipelines de compliance/supply-chain concluídos.
- `S-03 (audit retention 2y vs 7y): addressed ✓.` `auth_model.md` agora declara hot/warm/cold e retenção total ≥ 7 anos (`specs/03_architecture/auth_model.md:346-356`), e `storage_semantics_matrix.md` alinha hot vs archive (`specs/03_architecture/storage_semantics_matrix.md:159-164`).
- `S-04 (SBOM SPDX vs CycloneDX): partial.` A contradição documental foi resolvida por ADR-0014 e framework (`specs/00_framework.md:2648`, `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:42-48`). Mas a implementação real continua aberta no próprio ADR (`ADR-0014:74-77`).
- `S-06 (R2 key format inconsistente): addressed ✓.` `remote_cache_product_profile.md` e `storage_semantics_matrix.md` agora convergem em HMAC prefix e layout shard-aware (`specs/03_architecture/remote_cache_product_profile.md:235-265`, `specs/03_architecture/storage_semantics_matrix.md:168-173`).
- `S-07 (INV naming inconsistente): partial / regression.` O `invariant_registry.md` existe e é a decisão correta (`specs/03_architecture/invariant_registry.md:28-32`, `72-180`). Mas a propagação falhou: docs ativos ainda usam aliases CamelCase e nomes não registrados (`INV-QuotaEnforcement`, `INV-DigestVerification`, `INV-DataResidency`) em `remote_cache_product_profile.md:483-488`, `storage_semantics_matrix.md:266-270` e `00_framework.md:1255-1258`.
- `S-09 (FM-051 P1 sem regra explícita): addressed ✓.` A regra `S=5 => minimum P1` foi formalizada (`specs/03_architecture/failure_modes.md:76-77`) e FM-051 foi anotado corretamente (`:122`).
- `S-10 (CTRL dangling em STRIDE): addressed ✓.` `security_model.md` ganhou as seções 6.11, 6.12 e 6.13 (`specs/03_architecture/security_model.md:328-358`), e o validador agora fecha `CTRL definitions=87 uses=87`.
- `S-11 (lane-aware sign-off sprint/PRR): partial.` Os dois templates agora têm lane no YAML e texto de sign-off por lane (`specs/_templates/sprint_contract.md:7`, `868-887`; `specs/_templates/production_readiness_review.md:10-11`, `721-742`). Mas o sprint `HIGH_RISK` ainda não bate com o framework: o template pede 8 sign-offs, enquanto o framework fala em 10–12 (`specs/00_framework.md:2113-2127` vs `specs/_templates/sprint_contract.md:870-887`).
- `S-12 (GC re-reference mid-GC não coberto): partial / regression.` O corpus textual adicionou `INV-GC-004` (`specs/03_architecture/invariant_registry.md:99-103`) e o produto documenta `mark_started_at` (`specs/03_architecture/remote_cache_product_profile.md:319-343`). Mas a spec TLA+ não modela a fase de mark não-atômica. O finding foi respondido com um modelo mais fácil que o sistema real.
- `S-13 (tiers Solo/Business fantasmas): addressed ✓.` `slo_catalog.md` agora documenta 5 tiers (`specs/03_architecture/slo_catalog.md:84-92`).
- `S-14 (CTRL-ISO-001 sem evidence/revalidação): addressed ✓.` `security_model.md:260-266` resolve isso.
- `S-17 (tabelas markdown inválidas em compliance_matrix): addressed ✓.` As seções inspecionadas agora têm cabeçalhos e separadores corretos (`specs/03_architecture/compliance_matrix.md:111-128`).
- `S-18 (storage_semantics audit retention inconsistente): addressed ✓.` `specs/03_architecture/storage_semantics_matrix.md:159-164`.
- `S-19 (zero P0 suspeito): partial.` O documento ficou menos ingênuo ao impor `S=5 => P1` e `O mínimo 2` para FMs adversariais (`specs/03_architecture/failure_modes.md:76-83`). Mesmo assim, continua sem nenhum P0. Eu não trataria isso como contradição mecânica hoje, mas o catálogo segue conservador demais para incidentes de classe "cross-tenant read" e "key compromise".

## New findings

### CRITICAL (bloqueia GA)

- `G-01: gc_correctness.tla não modela o GC documentado; ele modela um snapshot atômico que o sistema real não tem.` O algoritmo do produto é explicitamente multi-pass sobre D1 (`remote_cache_product_profile.md:319-333`). Já o modelo TLA+ cola "mark" inteiro em uma única transição `GCMarkStart`, captura `gc_mark_set` atomically e entra direto em `sweep` (`specs/tla/gc_correctness.tla:120-129`). Isso elimina exatamente a janela que o audit anterior queria verificar: interleaving entre scan parcial de mark e `UpdateActionResult`. Resultado: o TLC verde não fecha S-12; ele prova um algoritmo mais forte do que o descrito/implementável sobre D1. `INV-GC-004` fica afirmado, não demonstrado.

- `G-02: cas_integrity.tla simula bit rot com uma flag, não com corrupção de storage.` `BitRot` só adiciona o digest em `corruption_flags` (`specs/tla/cas_integrity.tla:92-99`); `r2_storage` nunca é mutado. O read path então devolve `verify_mismatch` só porque a flag existe (`:78-90`), não porque `hash(body) != digest`. Assim, `InvCASIntegrity` (`:114-116`) nunca é tensionada pelo adversário e a claim do README "bit rot adversarial" (`specs/tla/README.md:27-29`) é falsa. Hoje a spec só mostra que um boolean inventado faz o log dizer "mismatch". Isso não é model checking útil para FM-051.

### HIGH

- `G-03: tenant_isolation.tla não formaliza o invariante prometido; ele só checa allowed-read por digest.` O texto do módulo promete "ler/escrever/enumerar" (`specs/tla/tenant_isolation.tla:11-15`), mas o invariante só olha reads (`:172-181`). `List` não modela conteúdo retornado, só `empty/result` (`:120-130`). `Write` nunca tenta escrever em tenant alheio (`:81-95`). Resultado: o modelo não cobre write isolation, enumeration leakage nem confusão por digest repetido entre tenants. Para uma claim CRITICAL de `INV-TENANT-ISOLATION`, isso é curto demais.

- `G-04: o “zero dangling references” é falso conforto; validate_references.py é cego para INV CamelCase.` O regex de `INV` aceita só `[A-Z0-9_-]` (`scripts/validate_references.py:47`), então tokens como `INV-TenantIsolation`, `INV-CASIdempotency`, `INV-QuotaEnforcement`, `INV-DigestVerification` e `INV-DataResidency` não entram no relatório. É por isso que o script fica verde enquanto `remote_cache_product_profile.md:483-488`, `storage_semantics_matrix.md:266-270` e `00_framework.md:1255-1258` ainda carregam invariantes legados/indefinidos. O validador está verde porque não enxerga o problema.

- `G-05: a correção da EVT taxonomy virou enum fechado com semântica aberta.` O framework agora tem 46 EVTs, mas introduziu aliases ambíguos (`SCAN_REPORT`, `AUDIT_LOG`) em `specs/00_framework.md:2702-2704`. Pior: vários docs usam `EVT-001` como se significasse "audit event", "consent event", "DSR evidence" ou "key operation log", embora `EVT-001` seja `CI_LOG` (`specs/00_framework.md:2639`). Exemplos concretos: `privacy_model.md:253-255`, `272-278`; `key_management.md:53`, `99`, `224-254`; `compliance_matrix.md:122`, `125`, `171-174`. Isso volta a quebrar auditabilidade por máquina, só que agora numericamente.

- `G-06: a biblioteca de runbooks cresceu, mas a trilha FM -> RB continua quebrada.` `failure_modes.md` ainda descreve a situação como "runbooks catalogados inicialmente" e lista só 6 IDs (`specs/03_architecture/failure_modes.md:246-262`). `FM-253` referencia o runbook errado (`:178` aponta para `RB-FM-303`), e o sweep local do `validate_references.py --warn-orphans` reportou 7 runbooks órfãos (`RB-FM-051`, `054`, `202`, `206`, `400`, `403`, `404`). Ou seja: os arquivos existem, mas o catálogo normativo ainda não os reconhece direito.

- `G-07: CTRL-PRIV-CONSENT-001..004 ainda não provam consentimento informado.` O modelo registra `ts`, `principal`, `purpose` e `basis_legal` (`specs/03_architecture/privacy_model.md:253-256`), e a tabela de bases legais existe (`:364-371`). O que falta é o elemento probatório de Art. 7 GDPR: qual texto, versão de notice, locale e checkbox wording foram apresentados ao titular no momento do opt-in. Sem isso, você prova que "houve um evento", não que o consentimento era informado e inequívoco. Para P2.1 isso ainda está curto.

### MEDIUM

- `G-08: sprint_contract.md continua inconsistente com o framework em HIGH_RISK.` O template afirma alinhamento com `§33.5.4.3` (`specs/_templates/sprint_contract.md:868`), mas define HIGH_RISK como "todos os 7 + adversarial" (`:872`), i.e. 8 assinaturas. O framework exige 10–12 (`specs/00_framework.md:2123-2127`). PRR ficou aceitável; sprint não.

- `G-09: key_management.md se contradiz sobre CTRL-KEY-020..022.` Na seção BYOE futura, o doc chama `CTRL-KEY-020..022` de placeholders de Fase 2 (`specs/03_architecture/key_management.md:170-173`). Depois, na seção 8.5, `CTRL-KEY-020` e `CTRL-KEY-021` já aparecem como controles ativos (`:250-255`). Isso é drift interno evitável num doc novo.

- `G-10: ainda existe material "a criar", TBD e placeholder em caminhos operacionais críticos.` Exemplos: `RB-BREACH-NOTIF` depende de três templates inexistentes e um contato jurídico placeholder (`specs/05_quality/runbooks/RB-BREACH-NOTIF.md:62`, `72`, `78`, `99`); `privacy_model.md` ainda diz que `RB-BREACH-NOTIF` está "a criar" (`:425`) e aponta `legal/dpa/v1.md` inexistente (`:398`); `key_management.md` ainda fala que `RB-KEY-COMPROMISE`, `RB-HSM-UNAVAILABLE` e `RB-BYOK-REVOKE` serão criados em Lote 5.7 (`:208-210`); `storage_semantics_matrix.md:176` ainda diz que a spec TLA+ está "a criar". Não é blocker sozinho, mas é sintoma de fechamento pela metade.

- `G-11: status documental e aprovação continuam single-person-heavy demais.` Os 20 runbooks estão `DRAFT` com `reviewers: []` e owner/aprovador iguais (`specs/05_quality/runbooks/*.md`, front matters). Os ADRs 0012/0013/0014 estão `FROZEN` com `reviewers: []` e ainda carregam checklists de implementação em aberto (`ADR-0012:64`, `ADR-0013:72-73`, `ADR-0014:74-77`). Isso não é só staffing shortage; é freeze prematuro.

### LOW / POLISH

- `G-12: há drift de versão/metadados dentro do próprio compliance_matrix.md.` Front matter diz `version: 0.2.0`, `updated: 2026-04-24` (`specs/03_architecture/compliance_matrix.md:6-8`), enquanto o corpo ainda exibe `Versão: 0.1.0`, `Última atualização: 2026-04-23` (`:20-23`).

- `G-13: alguns textos já ficaram obsoletos no mesmo lote que os criou.` Exemplo simples: `slo_catalog.md:115` ainda chama `RB-SLO-AVAIL-CP` de `stub`, embora o runbook exista e tenha corpo real.

## Veredito final

`RED LIGHT`

O Lote 5 melhorou o corpus de forma real. O salto entre "catálogo bonito mas quebrado" e "sistema com registry, key management, runbooks e cross-ref validator" é grande. Se eu comparar com o estado auditado no Lote 3+4, houve progresso substantivo e mensurável.

Mas o lote falha exatamente onde não podia falhar:

- as 3 specs TLA+ CRITICAL não são confiáveis o suficiente para sustentar o discurso "formal proof-backed";
- a EVT taxonomy saiu do caos lexical e entrou em caos semântico;
- o validator de referências dá verde mesmo sem enxergar aliases/nomes legados de invariantes;
- a biblioteca de runbooks existe, mas a rastreabilidade normativa ainda está incompleta.

Em português claro: o projeto ficou muito mais sério. Ainda não ficou "fucking awesome" spec-wise. Ficou **perigosamente perto de parecer pronto** antes de realmente estar.

## Observações meta

- **Pontos fortes reais:** `invariant_registry.md`, `key_management.md`, os 20 runbooks, a expansão controlada do framework e o `validate_references.py` representam trabalho substantivo. A direção é correta.
- **SOTA test contra concorrentes:** CoreLink agora ganha de muitos projetos em rigor declarativo, invariantes explícitos e intenção formal. Ainda perde para players maduros em empacotamento operacional e enterprise posture. BuildBuddy e NativeLink já têm histórias claras de self-hosted/on-prem/hybrid; JFrog já opera com supply-chain/compliance productizado e catálogo de artefatos mais maduro. CoreLink ainda tem DPA/template/legal contact placeholder, pipelines SBOM incompletos e formal models que super-abstract the real system.
- **Waivers:** `specs/_waivers/` está vazio. Não encontrei waiver ativo escondido. O problema aqui não é waiver eterno; é checklist/ADR congelado cedo demais e comentários residuais "a criar".
- **Contadores:** pelo `validate_references.py --warn-orphans`, as **definições** batem com o número alegado: 46 EVT, 87 CTRL, 51 PAT, 64 FM, 23 INV, 11 FF-HR, 13 SLO, 20 RB, 3 ADR. Isso **não** significa consistência total. Há muitos orphans e, no caso de `INV-*`, o número é subcontado porque o validador ignora CamelCase.
- **Limitações deste audit:** não rodei TLC de fato porque o ambiente não tem Java Runtime; não rodei `validate_specs.py` completo porque falta `jsonschema`. O review TLA+ foi conceitual e estrutural, não por execução de model checker.
