---
id: AUDIT-LOTE6-GPT
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [GPT via codex CLI]
supersedes: null
superseded_by: null
tags: [audit, lote6, tla-v2, final]
---

# Audit Lote 6 (GPT — 3ª iteração)

## Escopo

Sub-lotes 6.1-6.7 + fixes `ee5d3e7`, `cb0c9d8`, `7f4187b`, `3253b2f`, `5427c93`, `cb16321`, `362e6af`, `2c976be`, `1f89b6f`.

Review focado em: resolução real dos 25 findings anteriores, regressões/document drift introduzidos no Lote 6, consistência global de IDs/contagens, e qualidade substantiva das 4 specs TLA+.

## Resolution check

### GPT anteriores (G-01 a G-13)

- G-01 (gc atomic mark): partial — o modelo deixou de ser atômico e passou a ter `GCMarkStart` + `GCMarkStep` com interleaving real em `specs/tla/gc_correctness.tla:100-154`, mas a evidência continua incompleta: o `.cfg` só verifica `InvGCReRefProtected` em `specs/tla/gc_correctness.cfg:20-21`, não `INV-GC-001`; além disso o próprio spec admite que ainda não modela resurrection path em `specs/tla/gc_correctness.tla:192-195`.
- G-02 (BitRot flag): ✓ addressed — `BitRot` agora muta `r2_storage[d]` de fato em `specs/tla/cas_integrity.tla:94-103`, e o read path verifica `Hash(body)` real em `specs/tla/cas_integrity.tla:74-88`.
- G-03 (tenant_isolation insuficiente): ✓ addressed — `List` passou a retornar conteúdo explícito em `specs/tla/tenant_isolation.tla:123-132`, existe tentativa adversarial de write cross-tenant em `specs/tla/tenant_isolation.tla:162-177`, e os invariantes foram separados em read/write/enum em `specs/tla/tenant_isolation.tla:196-245`.
- G-04 (validator cego para INV CamelCase): ✓ addressed — o regex de `INV` foi ampliado em `scripts/validate_references.py:47`, e os aliases/domínios legados foram incorporados ao registry em `specs/03_architecture/invariant_registry.md:146-154`.
- G-05 (EVT taxonomy ambígua): partial — `EVT-047/048/049` existem em `specs/00_framework.md:2685-2687` e o downstream principal foi refatorado, mas o framework ainda fala em “24 tipos” em `specs/00_framework.md:456` e `specs/00_framework.md:3133`, e a alias table ainda deixa `AUDIT_LOG` semanticamente ambíguo em `specs/00_framework.md:2705-2706`.
- G-06 (FM -> RB quebrado): ✓ addressed — `specs/03_architecture/failure_modes.md:255-287` agora cataloga 26 runbooks, e `FM-253` aponta para `RB-FM-253` em `specs/03_architecture/failure_modes.md:178`.
- G-07 (consent não prova informed consent): partial — a substância melhorou de forma real em `specs/03_architecture/privacy_model.md:255-260`, mas a trilha normativa ficou incompleta: `specs/03_architecture/compliance_matrix.md:122` ainda mapeia P2.1 só para `CTRL-PRIV-CONSENT-001..004`, e o validador nem reconhece `CTRL-PRIV-CONSENT-*` por causa de `scripts/validate_references.py:44`.
- G-08 (sprint HIGH_RISK inconsistente): partial — a contagem “10-12” foi corrigida em `specs/_templates/sprint_contract.md:872,895`, mas a matriz continua desalinhada com o framework em `specs/00_framework.md:2113-2127`: o template omite `Cost Owner` e `Legal`, e inventa `Compliance Reviewer` / `Adversarial Reviewer` / `External Reviewer` como parte do alinhamento canônico em `specs/_templates/sprint_contract.md:878-891`.
- G-09 (CTRL-KEY-020..022 contraditórios): ✓ addressed — os placeholders foram renumerados para `CTRL-KEY-030..032` em `specs/03_architecture/key_management.md:170-173`, enquanto `CTRL-KEY-020/021` continuam ativos em `specs/03_architecture/key_management.md:250-255`.
- G-10 (“a criar”, TBD, placeholder em caminhos críticos): partial — houve limpeza real, mas ainda há backlog operacional em `specs/03_architecture/privacy_model.md:402` e `specs/05_quality/runbooks/RB-BREACH-NOTIF.md:62,72,78,99-104`.
- G-11 (single-person-heavy / freeze prematuro): partial — continua presente. Os runbooks seguem `DRAFT` com `reviewers: []` e owner=approver, ex. `specs/05_quality/runbooks/RB-FM-007-deserialize-rce.md:4-11`, e os ADRs seguem `FROZEN` com checklist aberto em `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:64`, `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:72-73` e `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:74-77`.
- G-12 (version drift em compliance_matrix): ✓ addressed — front matter e corpo alinham em `0.2.0 / 2026-04-24` em `specs/03_architecture/compliance_matrix.md:6-8` e `specs/03_architecture/compliance_matrix.md:20-22`.
- G-13 (textos obsoletos no mesmo lote): ✓ addressed — `RB-SLO-AVAIL-CP` agora é link real em `specs/03_architecture/slo_catalog.md:126`; o “stub” apontado no finding anterior saiu.

### Sonnet anteriores (T-01 a T-12)

- T-01 (TODO.md plaintext): ✓ addressed — `TODO.md` agora usa HMAC prefix explícito em `TODO.md:23`.
- T-02 (INV-AUDIT-APPEND-ONLY CRITICAL sem TLA+): partial — a spec foi criada em `specs/tla/audit_immutability.tla:1-169` com `.cfg` em `specs/tla/audit_immutability.cfg:13-24`, mas o registry ainda diz “sem TLA+ necessário” em `specs/03_architecture/invariant_registry.md:171`, e o modelo em si é fraco demais para sustentar a claim.
- T-03 (6 P1 FMs sem runbook): ✓ addressed — os 6 runbooks novos existem e o catálogo foi atualizado em `specs/03_architecture/failure_modes.md:255-287`.
- T-04 (FM-253 -> RB-FM-303): ✓ addressed — a referência foi corrigida em `specs/03_architecture/failure_modes.md:178`.
- T-05 (refs “a criar” em key_management): ✓ addressed — a seção de runbooks agora aponta para artefatos existentes em `specs/03_architecture/key_management.md:206-210`.
- T-06 (5 tiers sem target formal por-SLO): ✓ addressed — a regra de interpolação foi formalizada em `specs/03_architecture/slo_catalog.md:98-107`.
- T-07 (gc sem resurrection path): partial — continua explícito que o modelo não tem resurrection path em `specs/tla/gc_correctness.tla:192-195`.
- T-08 (tenant isolation com HMAC bounds fracos): partial — a abstração continua fraca: `hmac_prefix` é `1..Cardinality(Tenants)` em `specs/tla/tenant_isolation.tla:67`, e `PathGuess` itera exatamente esse domínio reduzido em `specs/tla/tenant_isolation.tla:185-186`, apesar do comentário falar em 2^-128 em `specs/tla/tenant_isolation.tla:57-60`.
- T-09 (48 CTRL orphans): partial — melhorou, mas não fechou. `validate_references.py --warn-orphans` ainda reporta 31 CTRL orphans; além disso `CTRL-PRIV-CONSENT-*` nem entra na contagem por causa de `scripts/validate_references.py:44`.
- T-10 (placeholder@law-firm.com): partial — o placeholder externo saiu, mas o runbook ainda depende de “Legal (TBD)” e 3 templates inexistentes em `specs/05_quality/runbooks/RB-BREACH-NOTIF.md:62,72,78,99-104`.
- T-11 (PAT-AUTHZ-002 orphan): partial — continua como alias histórico órfão em `specs/03_architecture/resilience_patterns.md:395-396`, e segue aparecendo como orphan no validador.
- T-12 (`hash_fn` VARIABLE em vez de CONSTANT): partial — permanece como variável de estado em `specs/tla/cas_integrity.tla:29-37,44-46`.

## Novos findings

### CRITICAL

- C-01: A claim de cobertura formal do GC continua overstated. `INV-GC-001` é CRITICAL em `specs/03_architecture/invariant_registry.md:99-102`, mas `specs/tla/gc_correctness.cfg:20-21` só model-checka `InvGCReRefProtected`. O README ainda sobredeclara cobertura em `specs/tla/README.md:27-29`, e o spec admite que não modela resurrection path em `specs/tla/gc_correctness.tla:192-195`. Isso não é “verde com gaps conhecidos”; é um buraco direto em `CTRL-FORMAL-001`.
- C-02: `audit_immutability.tla` fecha T-02 no papel, mas ainda é teatro como prova de append-only. As ações adversariais `TryDelete/TryReplace/TryReorder` são no-ops por construção em `specs/tla/audit_immutability.tla:84-109`; `VerifyChain` nem entra em `Next` em `specs/tla/audit_immutability.tla:111-129`; e `InvAuditAppendOnly` duplica basicamente o mesmo check de cadeia de `InvAuditChainIntact` em `specs/tla/audit_immutability.tla:142-151`. Pior: o registry ainda se contradiz sobre a necessidade de TLA+ em `specs/03_architecture/invariant_registry.md:116,160-171`.

### HIGH

- H-01: O namespace novo `CTRL-PRIV-CONSENT-*` quebrou a promessa de machine-readability. O validador aceita só `CTRL-[A-Z]+-\d{3}` em `scripts/validate_references.py:44`, enquanto os novos controles vivem em `specs/03_architecture/privacy_model.md:255-260`. Resultado: a contagem “88 CTRLs” não fecha, os controles de consent não são auditáveis pelo tooling, e `specs/03_architecture/compliance_matrix.md:122` ficou stale sem ser detectado.
- H-02: A taxonomia de evidência ainda está internamente inconsistente após a expansão para 49 EVTs. O framework segue dizendo “EVT-001 a EVT-024” em `specs/00_framework.md:456` e `specs/00_framework.md:3133`, enquanto o catálogo real vai até `EVT-049` em `specs/00_framework.md:2639-2687`. A alias table também continua com `AUDIT_LOG` ambíguo e sem incorporar `EVT-047/049` em `specs/00_framework.md:2705-2706`.
- H-03: O template de sprint continua desalinhado com a matriz canônica de sign-off do framework. O template vende “alinhamento com §33.5.4.3” em `specs/_templates/sprint_contract.md:868-895`, mas a matriz real de `HIGH_RISK` em `specs/00_framework.md:2113-2127` exige outra semântica de papéis. Isso é governança incoerente justamente na lane mais sensível.

### MEDIUM

- M-01: A política de sunset de aliases históricos de invariantes não é enforceable. O registry promete remoção após `2026-10-24` em `specs/03_architecture/invariant_registry.md:177-186`, mas o validador hard-whitelista esses aliases indefinidamente em `scripts/validate_references.py:125-130`.
- M-02: A abstração de HMAC em `tenant_isolation.tla` continua materialmente mais fraca do que o texto afirma. O comentário fala em “collision probability modelada como 0” em `specs/tla/tenant_isolation.tla:57-60`, mas a implementação efetiva usa um espaço reduzido a `1..Cardinality(Tenants)` em `specs/tla/tenant_isolation.tla:67`, com guessing no mesmo domínio em `specs/tla/tenant_isolation.tla:185-186`.
- M-03: `cas_integrity.tla` ainda usa `hash_fn` como variável de estado em vez de constante em `specs/tla/cas_integrity.tla:29-37,44-46`. Não quebra G-02, mas segue sendo uma modelagem estruturalmente pior do que o necessário e mantém T-12 aberto.

### LOW

- L-01: `specs/tla/README.md` ficou stale logo após o Lote 6. Ele omite `audit_immutability` do how-to run em `specs/tla/README.md:34-39`, afirma que `gc_correctness` verifica `InvMarkingConsistent` em `specs/tla/README.md:27-29` embora o `.cfg` não faça isso, e a seção de scope em `specs/tla/README.md:45-49` não bate com os bounds atuais dos `.cfg`.
- L-02: O rigor operacional ainda não fechou. Há backlog jurídico explícito em `specs/03_architecture/privacy_model.md:402`, templates de breach ainda inexistentes em `specs/05_quality/runbooks/RB-BREACH-NOTIF.md:62,72,78`, e ADRs `FROZEN` com checklist pendente em `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:64`, `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:72-73` e `specs/03_architecture/adrs/ADR-0014-sbom-format-cyclonedx.md:74-77`.

## Veredito final

RED LIGHT.

O Lote 6 melhorou bastante a superfície documental e fechou vários findings de forma real, especialmente em runbooks, TODO.md, renumeração BYOE, consent payload e multi-pass mark. Mas dois pontos continuam graves demais para aprovar: a cobertura formal do GC continua sobredeclarada, e a nova spec de audit append-only ainda é fraca a ponto de parecer teatro. Somado ao drift de validator/IDs e ao desalinhamento de HIGH_RISK sign-offs, ainda não dá para chamar o fechamento do Lote 6 de confiável.

## Observações meta

- `source .venv-specs/bin/activate && python3 scripts/validate_specs.py`: ✅ passou (`50 total`, `44` com schema completo, `6` YAML-only).
- `source .venv-specs/bin/activate && python3 scripts/validate_references.py`: ✅ sem dangling refs.
- `python3 scripts/validate_references.py --warn-orphans`: ainda reporta orphans relevantes, inclusive `PAT-AUTHZ-002` e `31` CTRLs.
- Consistência de contagens: `49 EVTs`, `51 PATs`, `64 FMs`, `26 INVs`, `11 FF-HRs`, `13 SLOs`, `26 RBs`, `3 ADRs` batem no validador. `CTRL` não bate com o discurso: o validador enxerga `87`, e o corpus ainda contém `CTRL-PRIV-CONSENT-001..006` fora do namespace que o tooling entende.
- TLC: tentei rodar os 4 módulos com `tlc -config ...`, inclusive com `-metadir` em `/tmp`. Todos passaram por parse/semantic phase, mas a execução foi bloqueada pelo ambiente com `java.rmi.server.ExportException: Listen failed on port: 0`; em `cas_integrity` e `tenant_isolation` também houve problema inicial de metadir em `specs/tla/states/...`. Portanto eu não validei localmente nenhum dos “verde” alegados.
- SOTA vs concorrentes: o CoreLink está acima da média de projetos pré-GA em densidade de specs, FMEA, runbooks e trilha de compliance. Ainda não está “fucking awesome” no sentido de benchmark: o gap relevante é que o sistema de garantias formais e machine-readable ainda não é confiável o bastante para sustentar o próprio marketing de rigor.
