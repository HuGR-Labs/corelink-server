---
id: AUDIT-LOTE3-4-GPT
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: ["GPT via codex CLI"]
supersedes: null
superseded_by: null
tags: [audit, lote3, lote4]
---

# Audit Lote 3 + Lote 4 (GPT)

## Escopo revisado

- `specs/_templates/work_item.md`
- `specs/_templates/sprint_contract.md`
- `specs/_templates/subtask.md`
- `specs/_templates/adr.md`
- `specs/_templates/production_readiness_review.md`
- `specs/_templates/waiver.md`
- `specs/03_architecture/storage_semantics_matrix.md`
- `specs/03_architecture/auth_model.md`
- `specs/03_architecture/remote_cache_product_profile.md`
- `specs/03_architecture/security_model.md`
- `specs/03_architecture/observability_model.md`
- `specs/03_architecture/privacy_model.md`
- `specs/03_architecture/failure_modes.md`
- `specs/03_architecture/resilience_patterns.md`
- `specs/03_architecture/slo_catalog.md`
- `specs/03_architecture/compliance_matrix.md`
- `specs/03_architecture/data_model.md`

## Findings

### CRITICAL (bloqueia GA)

- `F-01: Taxonomia de evidence foi quebrada de forma sistêmica.` `00_framework.md:2629-2653` define uma taxonomia fechada em `EVT-001..EVT-024`, mas os canonical sources passaram a usar um segundo dialeto informal (`EVT-UNIT_TEST_PASS`, `EVT-TLA_MODEL_CHECK`, `EVT-PENTEST_REPORT`, `EVT-AUDIT_LOG`, etc.). Exemplos: `security_model.md:232-319,428-453`, `compliance_matrix.md:76-119,250-258`, `privacy_model.md:254-260,415-422`, `observability_model.md:424-431`, `resilience_patterns.md:90,301-309`. Impacto: o sistema de evidence deixa de ser rastreável e validável por máquina; cada doc vira sua própria taxonomia, o que anula a promessa de gates consistentes. Recomendação: normalizar tudo para `EVT-001..024` imediatamente, adicionar uma tabela de aliases humanos se quiser legibilidade, e falhar CI para qualquer token `EVT-*` fora do espaço numérico canônico.

- `F-02: REMOTE-CACHE-PRODUCT-PROFILE virou fonte canônica de facto, mas não existe como fonte canônica de jure.` O doc exige `inherits_from: ["REMOTE-CACHE-PRODUCT-PROFILE"]` em `remote_cache_product_profile.md:30`, mas o catálogo oficial de fontes canônicas em `00_framework.md:2434-2454` não o lista; no schema ele ainda aparece apenas como `type: "protocol"` (`remote_cache_product_profile.md:3`, `front_matter.schema.json:11-38`). Pior: o mesmo doc usa forcing factors incoerentes, como `FF-HR-002` para `INV-CASIdempotency` (`remote_cache_product_profile.md:468`) e inventa `FF-HR-011` (`remote_cache_product_profile.md:471`) apesar de o framework só definir `FF-HR-001..010` e já ter `FF-HR-006` para GC/retention em `00_framework.md:2045-2054`. Impacto: a principal spec de produto do cache remoto não entra corretamente no regime de inheritance, risk lane nem validação. Recomendação: ou promover este doc formalmente no framework/schema/validator como fonte canônica de arquitetura, ou retirar a linguagem normativa de inheritance e tratá-lo como protocolo comum.

- `F-03: O contrato de invariantes CRITICAL está contraditório entre os próprios canonical sources.` `security_model.md:312-313` e `data_model.md:359-362` endurecem a regra para TLA+ obrigatório em invariantes críticos; `remote_cache_product_profile.md:325` faz o mesmo para `INV-GC-001`. Mas `auth_model.md:384-386` diz que `INV-TenantIsolation` é TLA+ "opcional", `storage_semantics_matrix.md:174` repete TLA+ opcional para o mesmo invariant, e `remote_cache_product_profile.md:468-473` aceita invariantes CRITICAL com mero property/integration test. Impacto: o mesmo invariant muda de gate conforme o doc consultado; reviewer diligente e reviewer relaxado chegam a conclusões opostas sem ninguém estar "formalmente errado". Recomendação: criar um registry único de invariantes com severidade e método de verificação obrigatório, referenciar explicitamente o arquivo `.tla/.cfg` por invariant, e fazer CI falhar quando um doc enfraquecer a regra.

### HIGH (fix antes do próximo lote)

- `F-04: O catálogo FM → PAT está incompleto e com drift de naming.` `failure_modes.md` e `slo_catalog.md` referenciam patterns que não existem em `resilience_patterns.md`. Exemplos concretos: `PAT-MEMORY-001` (`failure_modes.md:99`), `PAT-TIMEOUT-002` (`failure_modes.md:101`), `PAT-READ-YOUR-WRITES-001` (`failure_modes.md:112`), `PAT-KV-TTL-001` (`failure_modes.md:114`), `PAT-ONLINE-MIGRATE-001` (`failure_modes.md:116`), `PAT-BACKOFF-001` (`failure_modes.md:140`), `PAT-QUEUE-EVENTS-001` (`failure_modes.md:141`), `PAT-RUNBOOK-DRILL` (`failure_modes.md:154`), `PAT-ABUSE-DETECT-001` (`failure_modes.md:169`), `PAT-GC-HEALTHCHECK-001` (`failure_modes.md:183`), `PAT-TTL-JITTER-001` (`failure_modes.md:191`) e `PAT-AUTHZ-002` (`slo_catalog.md:203`). Há ainda IDs sem versão (`PAT-DUAL-APPROVAL`, `PAT-ROLL-FORWARD`) em `failure_modes.md:153,156`, enquanto o catálogo formal usa `-001`. Impacto: o catálogo de mitigação não fecha; vários FMs "mitigados" apontam para patterns inexistentes. Recomendação: completar `resilience_patterns.md` ou remapear todos os FMs/SLOs para IDs realmente definidos, sem shorthand e sem variações de sufixo.

- `F-05: O SLO catalog viola as próprias regras e ainda mascara um failure mode real.` O doc exige `ADR + baseline ≥ 30 dias + owner on-call assinado` em `slo_catalog.md:33` e classifica SLO sem 30 dias de SLI em prod como red flag em `slo_catalog.md:53`, mas o catálogo não traz baseline em nenhuma entry e várias entradas não têm owner/window/alert/runbook completos (`slo_catalog.md:134-150,178-183`). Mais grave: `slo_catalog.md:71` diz que `429` dentro da quota conta como failure de availability, mas a fórmula de `SLO-AVAIL-CAS-GET` exclui `rate_limited_within_quota` do denominador em `slo_catalog.md:112`, ou seja, esse erro desaparece do SLI em vez de contar como ruim. Impacto: o painel pode ficar verde enquanto o cliente toma rate-limit indevido. Recomendação: padronizar um template obrigatório por SLO, adicionar baseline/evidence explícitos e corrigir as fórmulas para que falhas "within quota" entrem no denominador e saiam do numerador.

- `F-06: O retrofit de inheritance ficou pela metade e a automação não cobre o contrato prometido.` Os templates ainda falam em fontes "PENDENTE Lote 4" mesmo depois de os 11 canonical sources existirem: `sprint_contract.md:593,632,675`, `production_readiness_review.md:195,362,445,490`, `work_item.md:831,1070,1246,1290`. Além disso, `REG-INHERIT-001` e `35.5.4` exigem `inherits_from`/`local_deltas` em YAML e validação CI (`00_framework.md:2481-2488`), mas os templates não trazem esses campos no front matter e `scripts/validate_specs.py:73-108` não checa nada além de front matter/schema. `subtask.md` sequer recebeu marcação de inheritance. Impacto: o Lote 3/4 cria a semântica de herança sem tornar o fluxo operacional; autores continuarão duplicando controles ou herdando de forma não validada. Recomendação: remover linguagem "pendente", adicionar `inherits_from` e `local_deltas` nos templates de Nível 4, e implementar as checagens prometidas no validador.

- `F-07: A matriz de compliance está mapeando consentimento para o controle errado.` `compliance_matrix.md:113` usa `CTRL-PRIV-015` para `P2.1 Consent`, mas `privacy_model.md:243` define `CTRL-PRIV-015` como `Constant-time signup response`, não consentimento. Em `privacy_model.md:346-353`, consentimento aparece como base legal real, mas sem controle próprio correspondente. Impacto: a trilha de compliance para consentimento é falsa; qualquer auditor que siga o ID vai cair num controle anti-enumeration, não numa evidência de opt-in/revocation. Recomendação: introduzir um controle explícito de consent management, remapear `P2.1` e também os pontos de LGPD/GDPR que dependem dele.

### MEDIUM (backlog explícito)

- `F-08: failure_modes.md promete runbook para todo P0/P1, mas deixa oito P1 sem stub.` A regra está em `failure_modes.md:235`, mas os runbooks listados em `failure_modes.md:246-251` cobrem apenas `FM-253`, `FM-254`, `FM-300`, `FM-302`, `FM-057` e `FM-205`. Ficam sem stub ao menos `FM-051`, `FM-054`, `FM-202`, `FM-206`, `FM-303`, `FM-400`, `FM-403` e `FM-404`, todos classificados como `P1` em `failure_modes.md:111,114,154,158,181,197,200,201`. Impacto: o catálogo identifica riscos top-tier sem completar a cadeia alert → runbook → resposta. Recomendação: ou baixar severidade onde for exagero, ou criar os RB stubs antes de tratar o catálogo como completo.

- `F-09: Os 11 canonical sources estão sem reviewers reais no YAML, apesar de o texto listar papéis.` Exemplos: `auth_model.md:11,24`, `security_model.md:11,24`, `slo_catalog.md:11,24`, e o padrão se repete em todos os demais arquivos de `03_architecture/`. Impacto: os docs que pretendem virar raiz de herança não têm accountable reviewers nem rota clara de freeze/thaw. Recomendação: preencher `reviewers` com nomes reais antes de qualquer promoção para `FROZEN`; se ainda não houver nomes, pelo menos declarar explicitamente que o freeze está bloqueado por review staffing.

- `F-10: Há supporting artifacts citados como existentes que simplesmente não estão no repositório.` `compliance_matrix.md:127` aponta para `_audits/iso27001-soa.csv`, `security_model.md:444` aponta para `_audits/matrix-stride-ctrl.csv`, e `privacy_model.md:357` presume LIAs em `_audits/`; nenhum desses artefatos existe hoje em `specs/_audits/`. Impacto: o leitor recebe a impressão de que a evidência/matriz completa já existe, mas a trilha morre no clique. Recomendação: criar stubs versionados agora ou reescrever esses trechos como TODO explícito, não como referência existente.

- `F-11: SLAs operacionais e de DSR estão sem semântica de relógio.` `privacy_model.md:252-260` define `5 dias úteis`, `15 dias úteis`, `30 dias` e `imediato`, mas não diz se o relógio começa na submissão, na verificação de identidade, na aceitação do ticket ou após sair de legal hold. `observability_model.md:340-345` traz `Response SLA` de `5 min`, `30 min`, `Business day`, etc., sem dizer se mede ack do pager, primeira resposta humana ou mitigação iniciada. Impacto: duas equipes diferentes conseguem "cumprir" o SLA com interpretações incompatíveis. Recomendação: adicionar `clock_start`, `clock_stop`, timezone/calendário útil e regras de pausa/hold.

### LOW / POLISH

- `F-12: O boundary entre Fase 1 (remote cache) e Fase 2 (remote execution) está borrado.` `auth_model.md:97-105` marca `Executor identity` como futuro, mas `security_model.md:408-415`, `slo_catalog.md:142-151`, `observability_model.md:318` e `data_model.md:173` já tratam `execute-action` como superfície corrente e plenamente modelada. Impacto: o corpus mistura contrato atual com roadmap e passa falsa sensação de completude. Recomendação: marcar explicitamente seções "future/phase-2" ou separar remote execution em fonte canônica própria.

- `F-13: Ainda faltam alguns blocos SOTA para competir de verdade com BuildBuddy Enterprise/JFrog/NativeLink.` O corpus menciona BYOK/BYOE de passagem (`privacy_model.md:315`, `security_model.md:149`) mas não os transforma em controle canônico, evidência auditável, política de chave do cliente ou postura comercial clara. Também não há spec formal de attestation de deleção/erasure para clientes enterprise nem postura de self-hosted/hybrid. Impacto: os docs estão bons para "cache cloud sério", mas ainda não fecham a história enterprise/compliance que diferencia concorrentes top-tier. Recomendação: abrir fontes canônicas ou ADRs específicas para CMEK/BYOK, deletion attestations e deployment models.

## Veredito

`RED LIGHT`

O lote tem progresso real, mas ainda não está coeso o bastante para servir como base canônica de herança e gates. Evidence taxonomy, invariantes críticos e rastreabilidade FM/PAT estão inconsistentes exatamente nos pontos onde o framework prometeu rigor mecânico; isso torna o corpus auditável no papel e ambíguo na prática.

## Observações meta

Pontos fortes: o material de domínio ficou muito melhor; `failure_modes.md`, `security_model.md`, `privacy_model.md` e `remote_cache_product_profile.md` têm densidade técnica alta, bons trade-offs explícitos e cobertura temática bem acima do lote anterior.

Pontos que eu não consegui avaliar totalmente: não rodei a validação JSON Schema completa porque `jsonschema` não está instalado no ambiente; fiz checagem manual de parse YAML e de campos base, e os 11 docs de `03_architecture/` passam nessa camada mínima. Também não avaliei runbooks reais, `_waivers/` reais ou evidências externas porque esses artefatos não estão no escopo/repositório atual.
