---
id: AUDIT-CODEX-R4-S01-WI-REVIEW
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-25
reviewers: [GPT via codex CLI - R4 S-01 WI focused review]
supersedes: null
superseded_by: null
tags: [audit, sota, lote-10.1, s-01, work-items]
---

# Codex R4 - Lote 10.1 S-01 WI Review

## Veredito Geral
SOTA score este lote: 6.3/10

Resumo: a macro-estrutura esta boa, mas o lote nao sustenta o bar "SOTA sem excecao". Os 6 WIs novos todos possuem as 32 secoes, mas so o WI-S01-002 chega perto do template `WI-S01-001`. Cinco dos seis falham o requisito explicito de narrativa `HIGH_RISK >= 300 palavras`; quatro deles degradam secoes obrigatorias (`Security & Privacy`, `Sign-off`, `PRR`, `Observability`, `Anti-patterns`) para placeholders; e dois WIs carregam erros tecnicos reais de protocolo/schema que impedem classificacao como production-ready.

Per-WI assessment summary:

| WI | Grade | SOTA bar | Nota curta |
|---|---|---|---|
| WI-S01-002 | B | close, but below SOTA | melhor do lote; perde pontos por claims tecnicos/perf/cost exagerados |
| WI-S01-003 | C+ | below SOTA | scope creep, metrics/error drift, security/sign-off superficiais |
| WI-S01-004 | C | below SOTA | desvia do schema canonico e omite controle `CTRL-META-001` |
| WI-S01-005 | D+ | below SOTA | protocolo REAPI/ByteStream mal especificado e secoes HIGH_RISK placeholder |
| WI-S01-006 | D | below SOTA | narrativa curta, claims estatisticos fracos, caos/seguranca praticamente vazios |
| WI-S01-007 | C- | below SOTA | boa intencao, mas SBOM/cosign/CI details ainda nao estao tecnicamente fechados |

## Per-WI Findings

### WI-S01-002 (BLAKE3)
- Strengths:
  - Narrativa atende o piso HIGH_RISK (~348 palavras) e justifica blast radius/reversibilidade com clareza (`specs/04_sprints/S01/work_items/WI-S01-002-blake3-verify-at-write.md:62-85`).
  - Gherkin e forte, especifico e nao-placeholder, com cenarios de mismatch, timing oracle, throughput, type-enforcement e build wasm (`WI-S01-002-blake3-verify-at-write.md:148-210`).
  - STRIDE/LINDDUN, sign-off e anti-patterns estao em nivel comparavel ao template (`WI-S01-002-blake3-verify-at-write.md:406-487`).
- Gaps:
  - Claims de throughput/SIMD em WASM estao agressivos demais e pouco ancorados no runtime real de Cloudflare Workers; o texto mistura runtime feature-detect nativo com `wasm32-unknown-unknown` como se fossem a mesma coisa (`WI-S01-002-blake3-verify-at-write.md:58,76,209,216`).
  - A conta de custo por byte esta errada em ordem de grandeza e o TCO "`$0`" ignora o proprio custo de CPU que o WI acabou de modelar (`WI-S01-002-blake3-verify-at-write.md:381-384`).
  - O experimento de caos "25 writes concorrentes de 5 MiB" conflita com o proprio budget de memoria do doc (`WI-S01-002-blake3-verify-at-write.md:234-235,305,313-314`).
- Critical issues:
  - Nenhum blocker estrutural, mas os claims de performance/custo precisam ser rebaixados para "target a validar em staging" para o WI subir ao bar SOTA.
- Quality grade: B

### WI-S01-003 (R2 adapter)
- Strengths:
  - Estrutura completa, Gherkin especifico e decisoes de design tem alternativas reais (`specs/04_sprints/S01/work_items/WI-S01-003-r2-adapter-single-blob.md:167-277`).
  - Dependencias principais estao na direcao certa (`WI-S01-003-r2-adapter-single-blob.md:366-379`).
- Gaps:
  - A narrativa fica abaixo do piso HIGH_RISK por pouco (~299 palavras), o que ja quebra o criterio formal do lote (`WI-S01-003-r2-adapter-single-blob.md:72-95`).
  - O WI invade escopo de leitura com `R2Reader`, cenarios GET e artifact de read path, apesar de S-01 marcar read path como anti-scope e o sprint descrever S01 como write path (`WI-S01-003-r2-adapter-single-blob.md:49-61,212-231,316-320`; `specs/04_sprints/S01/sprint.md:58,66-68`).
  - `Security & Privacy`, `Knowledge Transfer`, `Sign-off` e `Anti-patterns` estao muito abaixo do template `WI-S01-001` em densidade e rigor (`WI-S01-003-r2-adapter-single-blob.md:415-460`; `WI-S01-001-tenant-path-hmac.md:416-449`).
  - Naming de metricas oscila entre dot notation e underscore notation, quebrando o modelo canonico de observabilidade (`WI-S01-003-r2-adapter-single-blob.md:141-143,184,197,217`; `specs/03_architecture/observability_model.md:120-136`).
- Critical issues:
  - O WI inventa `COR_CAS_DUPLICATE_REJECTED`, mas esse error code nao existe no catalog canonico; pior, o proprio doc diz que o duplicate path mapeia para `Ok(())`, entao a taxonomia e a semantica de app entram em conflito (`WI-S01-003-r2-adapter-single-blob.md:147,195-197,260`; `specs/03_architecture/error_taxonomy.md:102-108`).
  - O custo de write conta "egress" no caminho Worker->R2, contradizendo o canonical source que diz zero-egress dentro da stack Cloudflare/CoreLink (`WI-S01-003-r2-adapter-single-blob.md:397-398`; `specs/03_architecture/storage_semantics_matrix.md:80-81`).
- Quality grade: C+

### WI-S01-004 (D1 schema)
- Strengths:
  - O doc enxerga os riscos certos: uniqueness, refcount race, migration drift e tombstone leakage (`specs/04_sprints/S01/work_items/WI-S01-004-d1-schema-blob-meta.md:72-92`).
  - Sub-tasks e acceptance criteria sao concretos e implementaveis (`WI-S01-004-d1-schema-blob-meta.md:142-186,268-283`).
- Gaps:
  - A narrativa tambem fica abaixo do piso HIGH_RISK (~299 palavras) (`WI-S01-004-d1-schema-blob-meta.md:72-92`).
  - `PRR`, `Security & Privacy`, `Knowledge Transfer`, `Sign-off` e `Anti-patterns` regrediram para versoes curtas/placeholder em comparacao ao template (`WI-S01-004-d1-schema-blob-meta.md:264-369`; `WI-S01-001-tenant-path-hmac.md:290-449`).
  - O getter de tombstone no Gherkin incentiva API perigosa ("alive flag check is consumer responsibility"), exatamente o tipo de foot gun que o proprio risk register diz querer evitar (`WI-S01-004-d1-schema-blob-meta.md:175-179,345`).
- Critical issues:
  - O schema proposto diverge do `data_model.md` em pontos fundamentais: `tenant_id` BLOB vs TEXT, `digest` bare hex vs `algo:hex`, `refcount DEFAULT 0` vs `1`, timestamps em segundos vs milissegundos, e falta a coluna `compression` (`WI-S01-004-d1-schema-blob-meta.md:47-57,150,205-207`; `specs/03_architecture/data_model.md:229-239`).
  - O WI tambem omite o controle canonico `CTRL-META-001` de checksum por linha + trigger D1, apesar de este ser exatamente o controle previsto contra metadata tamper/refcount drift (`WI-S01-004-d1-schema-blob-meta.md:47-57,188-211`; `specs/03_architecture/security_model.md:350-358`).
  - O claim de "latencia sub-ms" e o target `p99 <= 5ms` conflitam com a propria matriz de semantica, que coloca D1 em `10-200ms` p99 dependendo de primary/replica; alem disso, `criterion benchmark` nao e o instrumento certo para provar latencia de um banco gerenciado de rede (`WI-S01-004-d1-schema-blob-meta.md:192,217,250`; `specs/03_architecture/storage_semantics_matrix.md:71,77,186-196`).
- Quality grade: C

### WI-S01-005 (REAPI handler)
- Strengths:
  - O WI reconhece corretamente que esse handler e a superficie externa mais perigosa do lote (`specs/04_sprints/S01/work_items/WI-S01-005-reapi-batchupdateblobs.md:68-90`).
  - A cadeia de dependencias 001->004->005 esta clara e correta em alto nivel (`WI-S01-005-reapi-batchupdateblobs.md:291-299`).
- Gaps:
  - A narrativa tem so ~258 palavras e falha o requisito HIGH_RISK (`WI-S01-005-reapi-batchupdateblobs.md:68-90`).
  - `Quality Standards`, `Observability`, `Security & Privacy`, `Sign-off` e parte de `PRR` estao em placeholder puro, sem densidade suficiente para production-ready (`WI-S01-005-reapi-batchupdateblobs.md:256-270,309-357`).
  - O WI adiciona um surface REST `POST /v1/cas/<digest>` que nao aparece no sprint scope nem no contrato REAPI canonico do sprint, aumentando superficie sem amarracao suficiente (`WI-S01-005-reapi-batchupdateblobs.md:125`; `specs/04_sprints/S01/sprint.md:46-50`).
  - O rollback "best-effort delete" apos `R2Writer.put` nao discute o caso canonico de orfao `R2 PUT` sem `D1 INSERT`, que a matriz de storage trata explicitamente como inevitavel e dependente de reconciliacao/GC (`WI-S01-005-reapi-batchupdateblobs.md:83,323-329`; `specs/03_architecture/storage_semantics_matrix.md:104-110,246-250,280-282`).
- Critical issues:
  - O doc confunde `BatchUpdateBlobs` com `google::bytestream::WriteRequest`, o que esta tecnicamente errado na camada de protocolo (`WI-S01-005-reapi-batchupdateblobs.md:85`).
  - A tabela/cenarios de status estao incorretos: "code 13 (ABORTED)" e falso, porque `ABORTED` nao e code 13; alem disso, o mapping gRPC/HTTP esta descrito de forma simplificada demais para um WI de conformance (`WI-S01-005-reapi-batchupdateblobs.md:85,162-163,186`).
  - A parte `ByteStream::Write` esta incompleta para conformance real: faltam `resource_name`, `write_offset`, resume semantics, `finish_write`, `committed_size`, e o requisito canonico de `zstd` em ByteStream uploads/downloads (`WI-S01-005-reapi-batchupdateblobs.md:118-120`; `specs/03_architecture/remote_cache_product_profile.md:79,87-89,218-220`).
  - O WI entra em contradicao interna: marca `ByteStream::Write` como in-scope e, ao mesmo tempo, marca "streaming chunked write para single-blob" como anti-scope (`WI-S01-005-reapi-batchupdateblobs.md:118-120,137`).
- Quality grade: D+

### WI-S01-006 (Property tests)
- Strengths:
  - Boa intuicao de bridge entre TLA+ e codigo real; regression DB e helpers sao escolhas corretas (`specs/04_sprints/S01/work_items/WI-S01-006-property-tests-10k.md:89-110,219-225,255-263`).
  - Sub-tasks sao concretos e cobrem os invariants certos (`WI-S01-006-property-tests-10k.md:285-302`).
- Gaps:
  - A narrativa tem ~223 palavras e falha o piso HIGH_RISK por margem larga (`WI-S01-006-property-tests-10k.md:89-110`).
  - `Chaos Experiments` como `N/A` quebra frontalmente o padrao HIGH_RISK do sprint/framework (`WI-S01-006-property-tests-10k.md:277-280`).
  - `Rollback / Recovery` como `N/A`, `Security & Privacy` com uma unica linha e `Sign-off` placeholder deixam o WI longe do template `WI-S01-001` (`WI-S01-006-property-tests-10k.md:337-367`; `WI-S01-001-tenant-path-hmac.md:361-433`).
  - O artifact `.github/workflows/property_tests.yml` conflita com a ownership de CI centralizada em WI-S01-007 (`WI-S01-006-property-tests-10k.md:259-263`; `WI-S01-007-ci-tlc-gate-sbom.md:280-281`).
- Critical issues:
  - O doc faz afirmacoes estatisticas e semanticas que nao se sustentam: "10k cobre 99.9%+ confidence" sem modelo explicito; "0 false positives/negatives" em property tests; e "idempotency" modelada so como hashear o mesmo byte sequence duas vezes, o que prova determinismo do hash, nao semantica end-to-end de write idempotente (`WI-S01-006-property-tests-10k.md:172-183,215-217`).
- Quality grade: D

### WI-S01-007 (CI TLC + SBOM)
- Strengths:
  - O conjunto de gates e correto em principio: TLC, tests, clippy, audit, deny, fuzz, SBOM, signing (`specs/04_sprints/S01/work_items/WI-S01-007-ci-tlc-gate-sbom.md:46-61,109-123`).
  - Anti-scope e risk around `pull_request_target` e long-lived keys mostram boa higiene de supply chain (`WI-S01-007-ci-tlc-gate-sbom.md:146-153,406-413`).
- Gaps:
  - A narrativa fica em ~234 palavras, abaixo do threshold HIGH_RISK (`WI-S01-007-ci-tlc-gate-sbom.md:66-90`).
  - `Observability`, `Security & Privacy`, `Knowledge Transfer` e `Sign-off` estao muito mais rasos que o template do lote (`WI-S01-007-ci-tlc-gate-sbom.md:350-400`; `WI-S01-001-tenant-path-hmac.md:336-449`).
  - O target "100M iter" de fuzz e o budget de runtime/custo aparecem como numeros pouco ancorados, sem mostrar como serao medidos ou controlados (`WI-S01-007-ci-tlc-gate-sbom.md:82,192-197,249,356`).
- Critical issues:
  - O fluxo SBOM/signing nao esta tecnicamente fechado: `cyclonedx-cli generate` nao e um plano convincente de geracao de SBOM Rust por si so, o texto alterna entre "release asset" e "OCI registry" para o artefato assinado, e a mitigacao para outage do Fulcio/Rekor ("cache previous Rekor proof") nao resolve assinatura de artefato novo (`WI-S01-007-ci-tlc-gate-sbom.md:60-61,179-190,356-357,387`).
  - O cenario "clippy strict" pressupoe que `cargo clippy -D warnings` falha em qualquer `unwrap()`, o que nao e verdade sem lint explicito tipo `clippy::unwrap_used`/policy equivalente (`WI-S01-007-ci-tlc-gate-sbom.md:204-208`).
- Quality grade: C-

## Cross-WI Consistency
- Estrutura: os 6 WIs novos possuem as 32 secoes, mas so `WI-S01-002` realmente sustenta profundidade HIGH_RISK; `WI-S01-003/004/005/006/007` falham o piso de narrativa >= 300 palavras.
- Dependencias: a espinha dorsal `WI-S01-002 -> WI-S01-003 -> WI-S01-005` esta correta, e `WI-S01-005` depender de todos os prereqs faz sentido (`WI-S01-002:339-354`, `WI-S01-003:366-379`, `WI-S01-005:291-299`). Ainda assim, ha over/under-specification: `WI-S01-004` nao deveria depender fortemente de `WI-S01-001`; `WI-S01-003` invade read path; `WI-S01-006` assume ownership de workflow CI que `WI-S01-007` tambem assume.
- Estimativas: o contrato de sprint estima ~206h PERT (`specs/04_sprints/S01/_spec_contract.md:131-141`), mas a soma atual dos PERTs dos WIs e ~186.9h; considerando so os 6 novos, o lote fecha ~144.9h vs ~164h esperados. O maior drift esta em `WI-S01-005` (32h vs 48h do contrato) e `WI-S01-007` (28.5h vs 16h do contrato).
- Estimativas internas: em todos os 6 novos WIs, a soma dos sub-tasks e maior que o `O` da secao 19, e o `**Total**`/`PERT-weighted` da secao 17 conflita com a conta formal da secao 19 (`WI-S01-002:337,358-361`; `WI-S01-003:364,383-384`; `WI-S01-004:283,298`; `WI-S01-005:289,303`; `WI-S01-006:302,314`; `WI-S01-007:329,344`).
- Naming de metricas: ha drift serio entre sprint e WIs, e entre WIs entre si: `corelink.cas.*` vs `corelink_cas_*` vs `corelink.storage.r2.*`; labels `tenant_tier` vs `tenant_id` vs canonical `plan`; e `_bucket` aparece/ some de forma inconsistente (`specs/04_sprints/S01/sprint.md:179-183`; `WI-S01-002:57,124-125,164,176,372-373`; `WI-S01-003:141-143,184,197,217`; `WI-S01-004:252,307`; `specs/03_architecture/observability_model.md:120-136`).
- Error taxonomy: referencias validas existem para `COR_CAS_DIGEST_MISMATCH`, `COR_AUTH_SCOPE_INSUFFICIENT`, `COR_CAS_BLOB_TOO_LARGE` e `COR_SERVICE_DEGRADED`; `COR_CAS_DUPLICATE_REJECTED` nao existe no catalog (`specs/03_architecture/error_taxonomy.md:102-108,122-127,200-202`).
- Scope names: o sprint usa `cas-w`, o WI-S01-005 usa `cache:w/cache:r`, e o `auth_model.md` canonico define `cache-w/cache-r/cache-rw`; hoje ha tres dialetos para o mesmo conceito (`specs/04_sprints/S01/sprint.md:113`; `WI-S01-005:149,168`; `specs/03_architecture/auth_model.md:176-179,227-228`).
- Digest format: os WIs novos migraram silenciosamente de `digest = algo:hex` do `data_model.md` para bare hex 64-char/newtype binario, sem decisao explicita de compatibilidade (`specs/03_architecture/data_model.md:94,231`; `WI-S01-002:55,69,115`; `WI-S01-003:67,123`; `WI-S01-004:51`).

## Technical Accuracy Issues
- BLAKE3 throughput e SIMD: a semantica geral de BLAKE3 esta correta, mas os numeros e o mecanismo de "runtime SIMD detect" em WASM/Workers estao over-specified para um WI e precisam virar alvo empirico, nao afirmacao de design (`WI-S01-002:58,76,209,216`).
- R2 `If-None-Match: *`: a semantica de 412 em caso de objeto existente esta plausivel; o problema nao e o conceito, e sim o fechamento incompleto da semantica de app ao redor dele: codigo de erro invalido, custo errado e risco mal mitigado (`WI-S01-003:147,195-197,397-398,429`).
- D1/refcount: `UPDATE refcount = refcount + 1` atomico faz sentido em SQLite/D1, mas o WI ignora a regra canonica de Sessions API para fluxos write+read e, mais grave, descola do schema canonico (`specs/03_architecture/storage_semantics_matrix.md:186-196`; `WI-S01-004:47-57,84,170-173`).
- REAPI v2: o WI-S01-005 esta tecnicamente incorreto ao confundir `BatchUpdateBlobs` com `WriteRequest`, errar code 13/ABORTED, e nao especificar o contrato resumable de `ByteStream::Write` (`WI-S01-005:85,118-120,162,186`).
- Supply chain tooling: `cargo-audit`/`cargo-deny` como gate fazem sentido; o que nao esta maduro e o plano de SBOM/signing em `WI-S01-007` (`WI-S01-007:60-61,182-190,387`).
- Mann-Whitney U: nao e tema de S-01; nas referencias de S-02, o metodo e apropriado para latencia nao-normal e nao identifiquei erro conceitual relevante aqui (`specs/04_sprints/S02/_spec_contract.md:122,136,269`; `specs/04_sprints/S02/work_items/WI-S02-004-constant-time-middleware.md:104-105,246-248`).

## Missing Gaps for Production
- PRR ainda nao esta "production-ready" em nenhum dos 6 novos WIs: as secoes 16 sao majoritariamente ponteiros para docs futuros e nao trazem checklist inline de go/no-go.
- Observability esta incompleta em `WI-S01-005`, `WI-S01-006` e `WI-S01-007`: faltam nomes canonicos de metricas, labels, queries/alerts e ligacao real com `DASH-CAS`/SLOs.
- `WI-S01-005` nao cobre cenarios de erro cruciais para production: `write_offset mismatch`, `resource_name` invalido, resume interrupted upload, `finish_write=false`, `committed_size` inconsistente, falha de delete compensatorio apos `R2 PUT`, e `request_id` na superficie de erro.
- `WI-S01-004` nao cobre metadata integrity completa: checksum por linha, trigger de verify-before-update, sessions consistency e reconciliacao com GC.
- `WI-S01-007` nao transforma os controles de supply chain em comandos/processo executavel suficiente: geracao de SBOM Rust, storage da assinatura, verificacao, e politica de outage ainda estao difusos.
- Customer-facing error messages e `next_action` estao bons em `WI-S01-002`, parciais em `WI-S01-005`, e quase inexistentes nos demais WIs; isso enfraquece a promessa do `error_taxonomy.md`.

## Comparison vs WI-S01-001 Template
- `WI-S01-002` e o unico novo WI no mesmo bairro de qualidade do template; ainda assim, abaixo dele em rigor de claims tecnicos.
- `WI-S01-003` esta um degrau abaixo: boa forma, mas menos densidade em STRIDE/LINDDUN, sign-off e anti-patterns, alem de scope creep.
- `WI-S01-004/005/006/007` estao claramente abaixo do template. O padrao de regressao e repetido:
  - narrativa curta;
  - secoes HIGH_RISK virando placeholders;
  - anti-patterns sem justificativa substantiva;
  - menor aderencia aos canonical sources.
- O template `WI-S01-001` nao e perfeito, mas ele pelo menos oferece tabela de sign-off completa, STRIDE/LINDDUN trabalhados e apendice anti-patterns com racional. Os novos WIs, em sua maioria, perderam exatamente esses atributos.

## Recommendations
1. Corrigir primeiro o `WI-S01-004` para alinhar 100% com `data_model.md` e `security_model.md` antes de qualquer implementacao.
2. Reescrever o `WI-S01-005` com contrato REAPI/ByteStream correto: tipos, status, campos, resume semantics, compression e error envelope.
3. Decidir se `WI-S01-003` fica estritamente write-path em S-01 ou se o escopo do sprint muda; hoje o doc invade S-02 sem admitir isso.
4. Normalizar estimativas, metricas, scopes e error codes em todos os 6 WIs; hoje o lote nao fecha numericamente nem lexicalmente.
5. Elevar `WI-S01-003/004/005/006/007` ao padrao de profundidade de `WI-S01-001/002` nas secoes 16, 21, 26, 30 e 32.
6. Fechar `WI-S01-007` com toolchain executavel e verificavel para SBOM/signing, nao so nomes de ferramentas.

## Final Veredict
- Lote 10.1 SOTA quality: falls short
- Action items para next iteration:
  1. Fixar schema canonico e controles de metadata no WI-S01-004.
  2. Corrigir protocolo/conformance do WI-S01-005.
  3. Remover placeholders HIGH_RISK e reescrever narrativas < 300 palavras.
  4. Unificar metricas/scopes/error codes/digest format com os canonical sources.
  5. Reestimar o lote inteiro e reconciliar com `specs/04_sprints/S01/_spec_contract.md`.
