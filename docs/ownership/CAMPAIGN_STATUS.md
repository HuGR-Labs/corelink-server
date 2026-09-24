# CoreLink ownership — checkpoint de continuidade

Atualizado em 2026-09-22. Este arquivo é um checkpoint de trabalho, não o
standard congelado, uma aprovação ou evidência de runtime.

## Último readback verificado (supersede tabelas históricas abaixo)

### Estado material da branch de campanha (2026-09-22)

- A branch de campanha está versionada e sem alterações não commitadas neste
  checkpoint.
- Working tree: limpo; não há builds, worktrees ou caches gerados pela
  campanha pendentes neste checkout.
- O checkout do usuário em `~/Documents/HuGR/corelink-server` permanece fora
  do escopo desta branch e já estava sujo; não foi resetado nem limpo.

### Atualização de hidratação dos 105 drafts (2026-09-22)

- Os 105 drafts do ledger agora têm a seção bounded de hidratação
  `Contexto hidratado — preparação current-main`: 95 packages workspace e 10
  fuzz independentes.
- O ledger foi reconciliado com os hashes dos 105 corpos e a suíte documental
  terminou em 137/137. Isso fecha a lacuna mecânica de contexto inicial, não o
  gate semântico de consumidores, dedup, cold review ou publicação.
- A fonte é o bundle de preparação em `main` observado em
  `0389714d9f5408f744e17227b82d795fff245a32`, com seed source commit
  `37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`. Os próprios seeds declaram
  `deep_semantic_relations_complete=false` e `issue_ready=false`.
- Evidência detalhada:
  [`evidence/revision-1.4/ISSUE-DRAFT-CONTEXT-CENSUS-20260922-HYDRATED.md`](evidence/revision-1.4/ISSUE-DRAFT-CONTEXT-CENSUS-20260922-HYDRATED.md).

### Readback remoto final desta retomada (2026-09-22)

- `origin/main` avançou para `bcaacebcb5752d63cc0d6a8a614836197d77ec52`.
- O delta desde o pin usado na hidratação é um único teste Stripe de 89 linhas;
  não há mudança nos cinco trees-piloto, manifests, lockfile ou bundle de
  preparation. A evidência está em
  [`evidence/revision-1.4/MAIN-READBACK-20260922-BCA.md`](evidence/revision-1.4/MAIN-READBACK-20260922-BCA.md).
- A hidratação continua válida como preparação bounded; billing/server ainda
  precisam incorporar o novo teste na reconciliação de fonte antes de qualquer
  aprovação.

### Readback GitHub final desta retomada (2026-09-22)

- O snapshot autenticado mais recente retornou 267 issues (abertas/fechadas),
  sem títulos ou markers de ownership. Nenhum write foi feito.
- O lote de 12 rechecado permanece em 7 `DISTINCT` e 5 `UNRESOLVED`; a
  ausência de hit exato não foi convertida em decisão positiva. Evidência:
  [`evidence/revision-1.4/GITHUB-DEDUPE-READBACK-20260922-BCA.md`](evidence/revision-1.4/GITHUB-DEDUPE-READBACK-20260922-BCA.md).

### Readback backlog/dedup — lote 3 (2026-09-22)

- Mais 12 pacotes foram reconciliados somente em leitura contra `BACKLOG.md`
  do `origin/main`: 8 `DISTINCT`, 2 `EXPAND`, 2 `REUSE`, 0 novos
  `UNRESOLVED`.
- O saldo sem decisão semântica cai de 55 para 43 rows. `EXPAND`/`REUSE` ainda
  precisam de confirmação no ledger e não autorizam publicação automática.
- Evidência: [`evidence/revision-1.4/BACKLOG-DEDUPE-BATCH3-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-BATCH3-20260922.md).

### Readback backlog/dedup — lote 5 (2026-09-22)

- Mais 12 pacotes: 6 `DISTINCT`, 4 `EXPAND`, 2 `REUSE`, 0 novos
  `UNRESOLVED`; combinado com o lote 4, o saldo cai de 36 para 24.
- `EXPAND`/`REUSE` exigem reconciliação no ledger antes de qualquer emissão.
- Evidência: [`evidence/revision-1.4/BACKLOG-DEDUPE-BATCH5-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-BATCH5-20260922.md).

### Readback backlog/dedup — lote 4 (2026-09-22)

- O lote anterior teve 7 `DISTINCT` e 5 `UNRESOLVED`; portanto, antes do lote
  5 o saldo era 36. Os cinco no-hit continuam sem decisão positiva.
- Evidência: [`evidence/revision-1.4/BACKLOG-DEDUPE-BATCH4-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-BATCH4-20260922.md).

### Readback backlog/dedup — lotes finais 6–7 (2026-09-22)

- Lote 6: 8 `DISTINCT` e 4 `UNRESOLVED`.
- Lote 7: 9 `DISTINCT` e 3 `EXPAND`.
- Restam **4 pacotes** sem decisão semântica: `corelink-tracing`,
  `corelink-wasm`, `corelink-worker-fuzz` e `e2e-replication-failover`.
- Evidências: [`BACKLOG-DEDUPE-BATCH6-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-BATCH6-20260922.md) e [`BACKLOG-DEDUPE-BATCH7-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-BATCH7-20260922.md).

### Fechamento do censo semântico de dedup (2026-09-22)

- Os quatro casos finais (`corelink-tracing`, `corelink-wasm`,
  `corelink-worker-fuzz`, `e2e-replication-failover`) foram classificados
  `DISTINCT` em leitura independente.
- Resultado: **105/105 packages com decisão explícita** (`DISTINCT`, `REUSE` ou
  `EXPAND`); isso fecha o censo de dedup, não o preflight de publicação.
- Evidência: [`evidence/revision-1.4/BACKLOG-DEDUPE-CLOSEOUT-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-CLOSEOUT-20260922.md).
- As decisões foram agora ligadas ao publication ledger por um registro
  machine-readable: 92 `DISTINCT`, 4 `REUSE`, 9 `EXPAND`, 0 `UNRESOLVED`.
  Isso melhora o resume seguro, mas não muda os gates globais ainda
  `PENDING`.
- O readback consolidado confirmou que o ledger continua corretamente em
  `105 BLOCKED`, contrato congelado `0/105`, publicação `0`; o registro de
  dedup não abriu emissão automática. Evidência:
  [`evidence/revision-1.4/PUBLICATION-PREFLIGHT-READBACK-20260922-CONSOLIDATED.md`](evidence/revision-1.4/PUBLICATION-PREFLIGHT-READBACK-20260922-CONSOLIDATED.md).
- O achado histórico de G2 sobre 23 hits sem decisão foi rechecado contra o
  registry 105/105; essa lacuna de bookkeeping foi fechada, mas G2 continua
  bloqueado por contrato não congelado, integração e autoridade independente.
  Evidência: [`evidence/revision-1.4/STANDARD-G2-POST-DEDUPE-READBACK-20260922.md`](evidence/revision-1.4/STANDARD-G2-POST-DEDUPE-READBACK-20260922.md).
- Uma nova cold review independente (R4) confirmou G0 documentalmente, mas
  manteve G1 e G3 bloqueados e G2 apenas parcialmente resolvido; não há freeze
  nem autorização de publicação. Evidência:
  [`evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922-R4.md`](evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922-R4.md).
- A calibração foi rechecada nos bytes atuais; os cinco perfis continuam dentro
  dos caps. O readback corrige a contagem de palavras/bytes do blast de
  `corelink-cf-bindings` sem promover aprovação. Evidência:
  [`evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260922-FINAL.md`](evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260922-FINAL.md).
- A wave de cold review atual cobriu os cinco pilotos nos bytes atuais:
  billing, CF bindings, E2E e server tiveram os quatro artefatos aprovados
  documentalmente; hash manteve SKILL/REFERENCE/MAINTENANCE aprovados e
  BLAST_RADIUS bloqueado por reconciliação de peers. Isso não fecha aceitação
  operacional: os procedimentos locais seguem não executados e não há prova
  de runtime.
  Evidência: [`evidence/revision-1.4/PILOT-COLD-REVIEW-WAVE-20260922.md`](evidence/revision-1.4/PILOT-COLD-REVIEW-WAVE-20260922.md).

Os números históricos de 24/105 estruturados e 81/105 rasos abaixo continuam
preservados como métricas do censo anterior; não devem ser usados para dizer
que os drafts atuais continuam sem hidratação.

### Atualização corrente — após reancoragem Luna (2026-09-22)

- Campaign HEAD: `6e8036229b94fa8ddf6a8b524a5a84489c59bff4`.
- `origin/main` observado: `0389714d9f5408f744e17227b82d795fff245a32`.
- A população continua 107 manifests / 105 identidades elegíveis, sem mudança
  de nome/caminho; nove conteúdos de manifestos exigem refresh de metadata.
- `main` não contém o framework v1.4, skills, registry, schemas, tools, drafts
  ou pilotos desta branch; ele tem um bundle de preparation separado. O server
  pilot também sofreu mudanças materiais de fonte e está stale contra o pin
  histórico. Evidência consolidada:
  [`evidence/revision-1.4/MAIN-REANCHOR-20260922.md`](evidence/revision-1.4/MAIN-REANCHOR-20260922.md).
- As últimas correções Luna fecharam apenas defeitos documentais delimitados:
  hash API-006/REL-015, cf atomicidade/inventário/wiring SOURCE e proveniência
  do server. Os checks estruturais passam; cf recebeu `APPROVE` documental
  independente, hash continua bloqueado por cobertura/peers e o server precisa
  de reancoragem atual.
- Permanecem bloqueados: freeze do standard, autoridade peer/owner, 81 seeds
  rasos, **55 decisões semânticas de dedup** (o lote 2 classificou 7 como
  `DISTINCT` e deixou 5 `UNRESOLVED`), aprovação dos cinco pilotos,
  integração em `main` e publicação (0 issues da campanha).
- O `main` atual oferece um bundle separado de preparação com 105 packets e
  105 seeds estruturados. Ele foi registrado como fonte de hidratação, mas não
  foi incorporado silenciosamente aos drafts da campanha; o pin e a fronteira
  estão em
  [`evidence/revision-1.4/CURRENT-MAIN-SEED-READBACK-20260922.md`](evidence/revision-1.4/CURRENT-MAIN-SEED-READBACK-20260922.md).

- **Atualização final desta retomada (2026-09-22):** `origin/main` foi
  revalidado em `140e16eab6315bfdec1a0e4a9892d8557a071781`. O censo atual
  preserva 105 identidades elegíveis; dez manifests têm drift de conteúdo, sem
  mudança de identidade. As correções Luna bounded-fix atualizaram billing,
  cf-bindings, server e e2e, regeneraram registry/index para 105/105 PASS e
  mantiveram a suíte documental em 137/137. Evidências:
  [`evidence/revision-1.4/CARGO-CENSUS-CURRENT-MAIN-20260922.md`](evidence/revision-1.4/CARGO-CENSUS-CURRENT-MAIN-20260922.md),
  [`evidence/revision-1.4/LUNA-FIX-WAVE-RETURN-CARDS-20260922.md`](evidence/revision-1.4/LUNA-FIX-WAVE-RETURN-CARDS-20260922.md).
- O que permanece bloqueado não foi mascarado: standard ainda sem freeze,
  cold reviews dos bytes corrigidos ainda pendentes, hash sem peers/owner
  reconciliados, 81 seeds rasos, 62 decisões semânticas de backlog pendentes,
  autoridade independente não evidenciada e publicação 0.

- `origin/main` observado no último readback em 2026-09-22:
  `140e16eab6315bfdec1a0e4a9892d8557a071781`.
- O delta desde `0f90d89e` encontrou 5 arquivos de workflow/Buck2/scripts/teste
  de contrato CI, sem mudança em manifests, lockfile ou fontes dos cinco
  pilotos. O registro reproduzível está em
  [`evidence/revision-1.4/MAIN-READBACK-20260922-0D1.md`](evidence/revision-1.4/MAIN-READBACK-20260922-0D1.md).
- O readback remoto mais recente encontrou `main=41c89d72`; o delta de 60
  arquivos adiciona/ajusta migrations D1, `corelink-container`, worker,
  Terraform/Buck2 e CI. Embora não haja mudança de manifest/lockfile, billing e
  server ficam stale nas superfícies de runtime/D1 até novo SOURCE readback.
  Evidência: [`evidence/revision-1.4/MAIN-READBACK-20260922-41C.md`](evidence/revision-1.4/MAIN-READBACK-20260922-41C.md).
- O readback seguinte encontrou `main=122515e8`; o delta de 11 arquivos muda
  `BACKLOG.md`, governança/release CLI e verificadores Buck2/CI, sem mudar
  manifests, lockfile ou fontes dos cinco pilotos. O gate de backlog/dedup fica
  stale até novo readback; evidência em
  [`evidence/revision-1.4/MAIN-READBACK-20260922-122.md`](evidence/revision-1.4/MAIN-READBACK-20260922-122.md).
- O readback seguinte encontrou `main=3445b217`; o delta de quatro arquivos
  adiciona workflow/verificador/evidência de BYOK KMS, sem mudar manifests,
  lockfile ou fontes dos pilotos. O censo lexical do `BACKLOG.md` nesse pin
  permanece 37/68; evidências em
  [`evidence/revision-1.4/MAIN-READBACK-20260922-344.md`](evidence/revision-1.4/MAIN-READBACK-20260922-344.md)
  e [`evidence/revision-1.4/BACKLOG-READBACK-20260922-344.md`](evidence/revision-1.4/BACKLOG-READBACK-20260922-344.md).
- O snapshot GitHub R2 retornou 262 issues (81 abertas, 181 fechadas), sem
  títulos/markers ownership. As 23 linhas com hit direto foram inspecionadas e
  classificadas como `distinct` por escopo (defeito, CI, migração, runtime,
  compliance ou higiene), sem tratar isso como aprovação; evidências em
  [`evidence/revision-1.4/GITHUB-ISSUE-SNAPSHOT-20260922-R2.md`](evidence/revision-1.4/GITHUB-ISSUE-SNAPSHOT-20260922-R2.md)
  e [`evidence/revision-1.4/GITHUB-DEDUPE-DECISIONS-20260922.md`](evidence/revision-1.4/GITHUB-DEDUPE-DECISIONS-20260922.md).
- O preflight de publicação foi atualizado para reconhecer esse resultado
  parcial: 23 decisões `distinct` explícitas, mas 82 pacotes ainda pendentes de
  aliases/backlog; o ledger permanece sem alteração e todos os gates globais
  continuam bloqueados.
- A busca expandida no `BACKLOG.md@3445b217` encontrou 20 dos 82 pacotes sem
  hit de issue; os contextos foram inspecionados e classificados como
  `distinct` (referências de implementação/CI/teste/deploy, não ownership).
  Restam 62 pacotes para revisão semântica de aliases/backlog. Evidência em
  [`evidence/revision-1.4/BACKLOG-DEDUPE-DECISIONS-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-DECISIONS-20260922.md).
- Os **62 restantes** foram enumerados explicitamente como `UNRESOLVED` após
  busca por nome, manifest, diretório e skill slug; ausência de hit não foi
  convertida em `distinct`. A lista e o próximo evidence-required estão em
  [`evidence/revision-1.4/BACKLOG-DEDUPE-REMAINING-62-20260922.md`](evidence/revision-1.4/BACKLOG-DEDUPE-REMAINING-62-20260922.md).
- Um censo corrigido, sensível às seções em português, confirmou caminhos e
  identidades em 105/105; 24/105 drafts têm a seção estruturada de contexto de
  manifesto e 20/105 têm chaves de dependência explícitas. Os 81 restantes
  precisam de hydration mais profunda; o gate continua `PENDING`. Evidência:
  [`evidence/revision-1.4/ISSUE-DRAFT-CONTEXT-CENSUS-20260922.md`](evidence/revision-1.4/ISSUE-DRAFT-CONTEXT-CENSUS-20260922.md).
- A normalização dos 37 registros de integridade alterou 63 frontmatters e
  fechou o gate mecânico em 105/105 PASS. A classificação e o impacto sobre
  reviews estão em
  [`evidence/revision-1.4/REGISTRY-INTEGRITY-REPAIR-20260922.md`](evidence/revision-1.4/REGISTRY-INTEGRITY-REPAIR-20260922.md).
  A auditoria confirmou que os diffs são apenas frontmatter; as revisões
  substantivas dos corpos não foram descartadas. Readback:
  [`evidence/revision-1.4/METADATA-ONLY-READBACK-20260922.md`](evidence/revision-1.4/METADATA-ONLY-READBACK-20260922.md).
- O gap do ledger de `corelink-hash` foi decomposto em um suplemento explícito
  de REL-020..042, mantendo `peer_review=not_reconciled`; ele não é promovido
  ao ledger canônico até os peers serem confirmados:
  [`evidence/revision-1.4/HASH-RELATION-SUPPLEMENT-20260922.md`](evidence/revision-1.4/HASH-RELATION-SUPPLEMENT-20260922.md).
- O ledger de `corelink-hash` foi então reconciliado mecanicamente para 42/42
  relações, com anchors/chaves/source IDs presentes; todos continuam
  `peer_review=not_reconciled`, portanto o conjunto exige cold review nova e
  permanece sem aprovação. Evidência:
  [`evidence/revision-1.4/HASH-LEDGER-RECONCILIATION-20260922.md`](evidence/revision-1.4/HASH-LEDGER-RECONCILIATION-20260922.md).
- O readback estrutural pós-ledger passou nos quatro artefatos H e confirmou
  42 IDs/chaves/anchors únicos; como `BLAST_RADIUS.md` mudou, a cold review
  anterior não é reutilizável. Evidência:
  [`evidence/revision-1.4/HASH-POST-LEDGER-STRUCTURAL-READBACK-20260922.md`](evidence/revision-1.4/HASH-POST-LEDGER-STRUCTURAL-READBACK-20260922.md).
- O registry/index foram regenerados após a reconciliação do hash: 105/105
  structural PASS, 105/105 integrity PASS, cold review `UNVERIFIED` e
  publicação `0`. Evidência:
  [`evidence/revision-1.4/REGISTRY-READBACK-20260922-344.md`](evidence/revision-1.4/REGISTRY-READBACK-20260922-344.md).
- O censo de chaves peer do hash encontrou matches exatos para REL-025..031 e
  REL-034..036; todos continuam pendentes de confirmação. REL-020..024,
  REL-028, REL-032..033 e REL-037..042 permanecem sem chave peer encontrada.
  Evidência:
  [`evidence/revision-1.4/HASH-PEER-KEY-CENSUS-20260922.md`](evidence/revision-1.4/HASH-PEER-KEY-CENSUS-20260922.md).
- O teste local offline do `corelink-hash` foi executado no checkout
  source-equivalent: 29 pass, 2 ignored release-only, 0 fail. A evidência está
  em [`evidence/revision-1.4/HASH-CARGO-TEST-20260922.md`](evidence/revision-1.4/HASH-CARGO-TEST-20260922.md); os bytes de Reference/Maintenance mudaram para refletir isso e exigem nova cold review.
- Os dois gates release-only também passaram localmente: constant-time delta
  `0.019%` e performance de 10 × 5 MiB em `21.792027 ms`. Evidência:
  [`evidence/revision-1.4/HASH-RELEASE-GATES-20260922.md`](evidence/revision-1.4/HASH-RELEASE-GATES-20260922.md). O histórico anterior vermelho foi preservado; os bytes tocados ainda exigem rereview independente.
- `clippy -D warnings` e `cargo check --target wasm32-unknown-unknown` também
  passaram localmente; evidência em
  [`evidence/revision-1.4/HASH-LOCAL-QUALITY-20260922.md`](evidence/revision-1.4/HASH-LOCAL-QUALITY-20260922.md). Isso fecha apenas qualidade/target local, não deploy ou runtime.
- Durante a verificação final, `main` avançou para `a0eb612c`; o delta adicional
  foi de três arquivos/186 linhas no workflow/teste de classificação de fleet,
  sem drift de Cargo ou dos cinco pilotos. Evidência:
  [`evidence/revision-1.4/MAIN-READBACK-20260922-A0E.md`](evidence/revision-1.4/MAIN-READBACK-20260922-A0E.md).
- A verificação seguinte encontrou `main=791f474d`; o delta desde `a0eb612c`
  foi de 21 arquivos (CI, OpenAPI, backlog, handoff e testes), sem drift dos
  cinco pilotos, manifests ou lockfile. Evidência:
  [`evidence/revision-1.4/MAIN-READBACK-20260922-791.md`](evidence/revision-1.4/MAIN-READBACK-20260922-791.md).
- O registry/index gerado foi reancorado novamente para `0d1e8579`; o readback confirma
  105 packages, 105 PASS, 0 FAIL, cold review `UNVERIFIED` e publicação 0:
  [`evidence/revision-1.4/REGISTRY-READBACK-20260922-0D1.md`](evidence/revision-1.4/REGISTRY-READBACK-20260922-0D1.md).
- `main` avançou novamente para `03d30712`; o delta de 121 arquivos toca
  `corelink-container`, `corelink-billing-stripe-materializer`, Stripe, D1,
  migrações e testes de billing, além de CI. Billing/server/materializer ficam
  stale até novo readback SOURCE. Evidência:
  [`evidence/revision-1.4/MAIN-READBACK-20260922-03D.md`](evidence/revision-1.4/MAIN-READBACK-20260922-03D.md).
- O pin remoto seguinte é `0d1e8579`; seu delta de 5 arquivos é somente
  workflows, Buck2/examples e scripts/teste de contrato CI, sem novo drift dos
  cinco pilotos. O registry foi reancorado para esse pin; evidência:
  [`evidence/revision-1.4/MAIN-READBACK-20260922-0D1.md`](evidence/revision-1.4/MAIN-READBACK-20260922-0D1.md).
- O preflight read-only do ledger de publicação reconcilia 105 itens, todos
  `BLOCKED`, contrato não congelado e seis gates `PENDING`; nenhum item está
  elegível. Evidência:
  [`evidence/revision-1.4/PUBLICATION-PREFLIGHT-20260922.md`](evidence/revision-1.4/PUBLICATION-PREFLIGHT-20260922.md).
- O snapshot autenticado R2 mais recente tem 262 issues (81 abertas, 181
  fechadas), zero markers/títulos ownership e 23 packages com hits nominais;
  as decisões dessas 23 linhas estão em
  [`evidence/revision-1.4/GITHUB-DEDUPE-DECISIONS-20260922.md`](evidence/revision-1.4/GITHUB-DEDUPE-DECISIONS-20260922.md).
  O snapshot anterior de 250 issues e o inventário lexical antigo são
  históricos.
  O censo lexical do `BACKLOG.md` está em
  [`evidence/revision-1.4/BACKLOG-TEXT-CENSUS-20260922.md`](evidence/revision-1.4/BACKLOG-TEXT-CENSUS-20260922.md):
  37 packages têm ocorrência literal e 68 não têm ocorrência direta; isso não
  substitui a decisão semântica por aliases e intenção.
- O delta desde `743317c4` contém workflows, runbooks, scripts e Terraform de
  auditoria e quatro fontes DSR do `corelink-container`; o piloto
  `corelink-server` está stale até novo SOURCE readback. O detalhe reproduzível
  está em [`evidence/revision-1.4/MAIN-READBACK-20260922-FB6.md`](evidence/revision-1.4/MAIN-READBACK-20260922-FB6.md).
- O readback dos quatro arquivos DSR foi feito no pin `fb611330`: a referência
  e o blast do server agora registram `API-006`, `INV-005` e `REL-055` para as
  tabelas runner 0133/0134/0136. Isso é reconciliação SOURCE parcial; o piloto
  continua sem aprovação fria e sem execução Rust/D1.
- O reanchor cumulativo também confirmou drift de metadata do `corelink-hash`
  (`Cargo.toml.repository`) e do predicado do probe quota do billing (`<5 ms`
  → `≤5 ms`); ambos foram registrados nos R08/B06 dos respectivos pilotos,
  sem converter leitura SOURCE em execução ou aprovação.
- A nova cold review do standard está registrada em
  [`evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922.md`](evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922.md): SHA `f0f7c6ac`, veredito `BLOCKED`, sessão `/root/standard_cold_rereview_current`; identidade humana nominal não retornada, sem promoção a `APPROVE`.
- A rerevisão fria independente após o alinhamento do perfil H está em
  [`evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922-R2.md`](evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922-R2.md): o standard continua `BLOCKED`; G0/G1/G2/G3 permanecem abertos.
  O candidato atual (`e9b9c8ae`) inclui a emenda metadata-only documentada em
  [`evidence/revision-1.4/STANDARD-METADATA-SCOPE-AMENDMENT-20260922.md`](evidence/revision-1.4/STANDARD-METADATA-SCOPE-AMENDMENT-20260922.md);
  ele ainda requer cold review independente antes do freeze.
- A revisão fria independente do candidato atual (`e9b9c8ae`) está registrada em
  [`evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922-R3.md`](evidence/revision-1.4/STANDARD-COLD-REVIEW-20260922-R3.md): veredito
  `BLOCKED`. O readback confirma a emenda metadata-only e 420/420 checks
  estruturais, mas G0/G1/G2/G3 continuam abertos; não há freeze ou autorização
  de publicação.
- A suíte documental foi repetida após o reparo do registry e passou em
  `137/137`; a asserção histórica que esperava um registry misto foi alinhada
  ao estado atual `105/105 PASS`, mantendo `cold_review=UNVERIFIED` e publicação
  `0`. Evidência: [`evidence/revision-1.4/DOCUMENTARY-TEST-READBACK-20260922.md`](evidence/revision-1.4/DOCUMENTARY-TEST-READBACK-20260922.md).
- A rerevisão fria independente dos quatro bytes de `corelink-hash` está em
  [`evidence/revision-1.4/HASH-COLD-REVIEW-20260922.md`](evidence/revision-1.4/HASH-COLD-REVIEW-20260922.md): o perfil H foi confirmado; o ledger já está completo em 42/42, mas os quatro artefatos continuam `BLOCKED` por peers/consumers/owners não reconciliados, bytes alterados sem nova cold review e ausência de execução no pin atual.
- A calibração mensurável dos cinco pilotos está em
  [`evidence/revision-1.4/PILOT-CALIBRATION-20260922.md`](evidence/revision-1.4/PILOT-CALIBRATION-20260922.md): os bytes declarados são três perfis H e dois S; hash foi alinhado ao gatilho de 42 relações e billing agora tem census source-backed de 49 declarações de módulos em 47 arquivos não-test, ainda sujeito a confirmação semântica. O detalhamento está em [`evidence/revision-1.4/BILLING-MODULE-CENSUS-20260922.md`](evidence/revision-1.4/BILLING-MODULE-CENSUS-20260922.md). Isso demonstra capacidade documental, não aprovação ou freeze.
- A branch desta campanha mantém os quatro caminhos para **105/105** packages
  provisórios, mas `main` atual não contém esses 336 caminhos; há divergência
  documental material e integração ainda não decidida.
- Nenhuma issue específica da campanha foi publicada: o readback R2 encontrou
  262 issues, zero títulos `[ownership]` e zero markers `corelink-ownership:v1`.
- O standard/framework comum continua candidato e não integrado/congelado.
- O framework candidato agora está integrado na branch (`STANDARD.md`,
  templates, schemas, tools, tests e evidência). O censo reproduzido nessa
  branch passou com 95 workspace + 10 fuzz = 105 elegíveis e classificação
  explícita `archive`; isso ainda não certifica o `main` remoto.
- O censo direto do objeto Git `fb611330` reconciliou 107 manifests: 106
  packages nomeados, 1 manifest virtual e 1 package archive (`corelink-erasure`)
  excluído explicitamente; os 105 packages do registry estão todos presentes.
  Evidência: [`evidence/revision-1.4/CARGO-CENSUS-20260922-FB6.md`](evidence/revision-1.4/CARGO-CENSUS-20260922-FB6.md).
- O último avanço de `main` adicionou runner aggregate/CI/migration D1; as
  relações desses surfaces continuam exigindo reconciliação SOURCE.
- O avanço seguinte alterou billing Stripe materializer, D1 HTTP e
  `corelink-container`; os pilotos de billing/server continuam stale até nova
  reconciliação.
- A reancoragem mais recente (`5006fb2f` → `ba5ecee8`) alterou somente
  `.github/workflows/dco-check.yml` e `.github/workflows/rustfmt.yml`, movendo
  checks obrigatórios para hosted runners. Nenhum manifesto Cargo, fonte dos
  cinco pilotos ou standard/OKF mudou nesse delta; relações de CI continuam
  declaradas como SOURCE/resultado UNKNOWN.
- Desde `ba5ecee8`, `main` adicionou o plano fail-closed de staging/provider,
  alterou `corelink-terraform-drift-consumer` e ajustou a sonda de CI. Esse
  delta não toca os cinco pilotos, mas reabre as relações do package de drift,
  scripts e CI; nenhum resultado de execução foi observado.
- Desde `7cef578c`, `campaign-ci.yml` recebeu o bundle hosted de evidências
  (+177 linhas). Isso reabre somente a superfície de CI/campanha; não altera
  os cinco pilotos nem prova execução, publicação ou reachability.
- Desde `b2b9fc0d`, `main` adicionou lanes hosted de auditoria/staging/worker,
  probes read-only e fixtures Workerd, além de alterar um teste de quota do
  `corelink-billing`. Billing fica stale até novo SOURCE readback; os demais
  deltas são CI/test harness e não provam execução ou runtime.
- Desde `e55e73f`, o remoto mudou 94 arquivos (+3174/-709), incluindo
  `Cargo.toml`, `Cargo.lock`, `corelink-container` (adapter cache/audit),
  `wrangler.toml` e testes. Isso invalida a ponta atual dos pilotos hash/server
  e qualquer relação de composição afetada até novo SOURCE readback.
- A suíte integrada de preparação/contrato passou com **137 testes** e a
  verificação estrutural passou em **420/420 artefatos**, além de `git diff
  --check`; isso é validação documental/tooling, não aprovação fria dos
  artefatos nem evidência de Cargo/runtime. O readback R3 está em
  `evidence/revision-1.4/STRUCTURAL-READBACK-20260922-R3.md`.
- O registry/index gerado reconcilia **105/105** identidades e mostra
  `cold_review: UNVERIFIED` e `publication: NOT_PUBLISHED`; os 105 conjuntos
  agora têm integridade estrutural/cross-artifact PASS após a normalização
  mecânica de frontmatter registrada em
  [`evidence/revision-1.4/REGISTRY-INTEGRITY-REPAIR-20260922.md`](evidence/revision-1.4/REGISTRY-INTEGRITY-REPAIR-20260922.md).
  Os 63 blobs alterados exigem cold review nova; nenhum pacote é promovido.
  A invalidação explícita das revisões anteriores está em
  [`evidence/revision-1.4/STANDARD-REVIEW-INVALIDATION-20260922.md`](evidence/revision-1.4/STANDARD-REVIEW-INVALIDATION-20260922.md).
- Nenhum pacote é declarado aprovado globalmente sem revisão fria nos bytes
  atuais, reconciliação de relações compartilhadas e pin de fonte compatível.

### Bloqueios que permanecem reais

1. Reancorar/revisar os artefatos afetados pelo `main` atual e decidir a
   integração dos 336 caminhos ausentes no ponto canônico.
2. Reancorar e revisar os conjuntos afetados pelo `main` atual, começando pelos
   cinco pilotos e pelos peers com relações compartilhadas; a normalização
   estrutural já não deixa falhas CO-1 abertas.
3. Revisar/congelar o framework integrado, migrar/revalidar os 105 conjuntos e
   só então executar deduplicação/preflight e publicação.
4. Confirmar na cold review que os 47 arquivos-fonte não-test usados como proxy
   de módulos de billing representam módulos semânticos suficientes para o
   perfil H; a mudança de hash para H foi aplicada e a rerevisão confirmou o
   gatilho, mas não os demais gates.

Os resultados novos não são promovidos a `APPROVE` por inferência: checker
estrutural passa não substitui cold review, e cold review não prova Cargo,
runtime, deploy ou reachability.

### Resultados mais recentes registrados

- `corelink-worker-fuzz`: quatro `APPROVE` frios no candidato
  `0837e700`; sem Cargo/fuzz/runtime.
- `corelink-audit-chain-fuzz`: Skill/Reference/Maintenance `APPROVE`, Blast
  `FIX_FIRST` por `REL-001/002` ainda não reconciliadas com os peers.
- `corelink-reapi-fuzz`: quatro `APPROVE` frios; `uuid::Uuid` não declarado
  permanece corretamente `SOURCE_INCOMPATIBLE / build UNKNOWN`.
- `corelink-tenant-isolation`: Reference corrigida em `e8d3b578`; exige nova
  cold review. Blast continua `BLOCKED` por peers/owners.
- `corelink-server`: skill/maintenance documentalmente aprováveis após
  reconciliação; reference/blast ainda `FIX_FIRST` por contagem/censo estático
  e desconhecidos materiais.
- `corelink-cf-bindings`: estrutura corrigida; cold review ainda `FIX_FIRST`/
  `BLOCKED` por contratos atômicos, evidência exata, peers e execução wasm não
  observada.
- `corelink-meta-fuzz`: quatro artefatos `FIX_FIRST`; correção/review não foi
  falsamente convertida em aprovação.
- `corelink-hash`: cold review independente dos bytes finais (`6ec50aa7f`,
  hashes registrados pelo revisor) retornou `FIX_FIRST` nos três artefatos
  documentais e `BLOCKED` na manutenção. Papéis/endpoints foram corrigidos;
  permanecem sem evidência verificável a rota nominal de aprovação, os
  desconhecidos de B06, o ledger REL-020–042 e a execução local exigida.
- `corelink-hash`: rerevisão fria em `ea43e713` confirmou o novo perfil H e os
  quatro hashes atuais; o veredito dos quatro artefatos continua `BLOCKED`.
- `STANDARD.md`: cold review independente do candidato corrigido
  (`f0f7c6ac0fe2276ce84548361ef102f25023034540bc03b772aae0525c07aa21`)
  retornou `BLOCKED`; G0/G1/G2/G3 não podem ser promovidos.
- `STANDARD.md`: rerevisão independente R2 após o ajuste de perfil H também
  retornou `BLOCKED`; os quatro gates G0/G1/G2/G3 seguem sem promoção.
- O histórico 353/420 → 420/420 está registrado em
  `evidence/revision-1.4/STRUCTURAL-READBACK-20260922-R2.md`; o resultado atual
  ainda não substitui revisão fria nem reconciliação de consumidores.

## Âncora

### Readback final desta retomada — 2026-09-23

- `origin/main` foi relido em `5e4339c50907afe6be682d231d836120fc55fd28`.
- O delta desde o snapshot documental `50a5ab3a` toca cinco arquivos do
  `corelink-container`/DSR e material de workflow, backlog, capacidade e
  verificadores; manifests e lockfile não mudaram.
- O snapshot da campanha continua deliberadamente em `50a5ab3a`. O piloto
  `corelink-server` e relações DSR não são promovidos a current-main; o
  readback reproduzível está em
  [`evidence/revision-1.4/MAIN-DRIFT-READBACK-20260923-5E43.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260923-5E43.md).

### Correções documentais e rerevisão — 2026-09-23

- A revisão fria em ondas encontrou omissões concretas, não apenas falhas do
  checker: consumidores inversos, superfícies FFI/packaging, contratos de
  umbrella H, peers compartilhados, contagem de journeys e caminhos do checker.
- Foram aplicadas correções bounded somente em documentação para esses achados;
  nenhum código-fonte, deploy, provider, runtime ou issue GitHub foi alterado.
- A reconciliação do hash agora registra cinco fingerprints peer exatos, mas
  mantém `peer_review=not_reconciled` e os desconhecidos de owner/runtime.
  Evidência: [`HASH-COLD-REVIEW-20260923-PEER-RECONCILIATION.md`](evidence/revision-1.4/HASH-COLD-REVIEW-20260923-PEER-RECONCILIATION.md).
- A varredura final dos 420 artefatos retornou `IMPLEMENTED_CHECKS_PASS` em
  todos os perfis declarados; a suíte documental repetida passou **137/137**.
  Esses resultados ainda não substituem rerevisão independente dos bytes
  alterados, freeze do standard, integração em `main` ou prova de runtime.

O registro terminal dos bloqueios atuais, com owners/evidências necessárias,
está em [`evidence/revision-1.4/TERMINAL-BLOCKERS-20260922.md`](evidence/revision-1.4/TERMINAL-BLOCKERS-20260922.md).

| Campo | Valor |
|---|---|
| Worktree | `/tmp/corelink-ownership-campaign` |
| Branch | `codex/corelink-ownership-campaign` |
| Baseline verificado | `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` |
| Publicação GitHub | nenhuma issue de ownership publicada |
| Execução remota | nenhuma |

O snapshot histórico de 211 issues no repositório redirecionado é preservado
apenas como evidência histórica. O snapshot canônico R2 em `HuGR-dev/corelink-server`
tem 262 issues, nenhum título `[ownership]` e nenhum marker
`corelink-ownership:v1`; isso corrige a ambiguidade entre “há issues no
repositório” e “as 105 ownership issues desta campanha foram emitidas”.

## Estado por piloto

| Package | Artefatos | Cold review | Estado real |
|---|---|---|---|
| `corelink-hash` | quatro documentos commitados | três `FIX_FIRST`/`BLOCKED`; standard `BLOCKED` | rota de aprovação, peers/consumers, procedimentos e drift de main continuam abertos; ledger 42/42 |
| `corelink-billing` | quatro documentos commitados | skill, blast, manutenção e referência (re-review) aprovados independentemente | ainda não publicável: standard/global gates não congelados |
| `corelink-server` | quatro documentos commitados | não solicitada | rascunho H; 31 relações Cargo e 19 relações de composição/semânticas, mas censo por call path segue aberto |
| `corelink-cf-bindings` | quatro documentos commitados | não solicitada | rascunho S; censo de bindings/configuração/deploy e consumidores indiretos segue aberto |
| `e2e-billing-flow` | quatro documentos commitados | não solicitada | rascunho S; grafo normal offline e busca nominal de CI capturados, sem consumidor Cargo/job direto; execução local e CI por matriz/contexto externo pendentes |

## Rollout de autoria

| Marco | Estado |
|---|---|
| Ondas 001–012 | 72 packages integrados após autoria e cold review separados |
| Onda 013 | seis packages integrados, cada um com quatro vereditos independentes `APPROVE`; diff de 24 caminhos limpo |
| Onda 014 | Seis packages authored. `e2e-dsr` (`fee2a49`) recebeu quatro `APPROVE` independentes e foi integrado; `e2e-byok-revoke` (`b95e7c8`) recebeu quatro `APPROVE` nos bytes repinados para `1177` e foi integrado com reprodução dos quatro checkers S. `chaos-campaign` (`34f9538`) aguarda rereview da Reference; `e2e-chaos` (`507aa4b`) teve Reference/Blast aprovados e aguarda reconciliação dos demais bytes; `e2e-failover-router` (`8ab23c0`) aguarda rereview final de Maintenance e segue operacionalmente BLOCKED. `e2e-pilot-onboarding` continua sem aprovação. |
| População provisória | 105 de 105 packages têm os quatro caminhos presentes na branch, contado mecanicamente no checkout; isso não significa aprovação. Os cinco conjuntos recém-integrados (`chaos-campaign`, `corelink-audit-chain-fuzz`, `corelink-meta-fuzz`, `e2e-replication-failover` e `e2e-tenant-isolation`) permanecem drafts com vereditos `FIX_FIRST`/`BLOCKED` ou revisão ainda não concluída. `e2e-failover-router` conta como integrado com gate de Maintenance reaberto; `e2e-dsr` conta como integrado após quatro APPROVE frios; `e2e-signup-flow` (`8a6053a`), `migrate-single-to-multi-region` (`275ead3`) e `e2e-user-journeys` (`0d7e463`) foram integrados após cold APPROVE final e reprodução dos quatro checkers. `corelink-ac-fuzz` (`5f678fe`) foi integrado após quatro APPROVE frios e quatro checkers S reproduzidos. `e2e-resilience` (`66d011f`) tem quatro APPROVE locais e quatro checkers S reproduzidos, mas permanece globalmente BLOCKED até reconciliar fingerprints com `corelink-ratelimit`/`corelink-rate-headers`; a lacuna está explicitamente registrada. `corelink-cli-fuzz` (`929164a`) e `corelink-client-verify-fuzz` (`318b87d`) receberam quatro APPROVE nos bytes finais e quatro checkers S reproduzidos. `e2e-chaos` (`a92e6d3`) recebeu quatro APPROVE frios nos bytes finais e quatro checkers S reproduzidos. `corelink-byok-fuzz` (`f696e64`) recebeu quatro APPROVE locais e quatro checkers S reproduzidos, condicionado à reconciliação com o peer `corelink-byok`. `corelink-worker-fuzz` (`ba81e4b`), `corelink-reapi-fuzz` (`7f4e9c1`), `corelink-tenant-path-fuzz` (`4a964e2`) e `e2e-pilot-onboarding` (`50c8183`) receberam quatro APPROVE documentais nos bytes finais, com o tenant manual ainda BLOCKED por owner/escalation não verificável e o reapi mantendo `SOURCE_INCOMPATIBLE / build UNKNOWN` para um target. `corelink-hash-fuzz` (`9f5639b`) recebeu quatro APPROVE locais e quatro checkers S reproduzidos; os peers parent `REL-037/038` permanecem pendentes e o estado global está explicitamente condicionado a essa coordenação. Candidatos W014/W015 e os cinco drafts finais continuam em review/correção; o denominador agora reconcilia mecanicamente, mas a campanha continua provisória. |
| Campanha | standard, framework comum, pilotos, censo certificado, deduplicação e publicação continuam abertos |

## Commits da campanha

| Commit | Conteúdo |
|---|---|
| `a959a15b6` | piloto `corelink-hash` |
| `76cf1e870` | reconciliação de consumidores de hash |
| `b54b3a0dd` / `8201ac596` | piloto e correção de referência de billing |
| `d48d21083` / `9b19d14de` | piloto server e inventário de 31 deps first-party |
| `5b1bbcfed` | conjunto rascunho do piloto `corelink-cf-bindings` |
| `274b50858` | conjunto rascunho do piloto `e2e-billing-flow` |
| censo corrente | classificação explícita dos 107 manifests rastreados |
| `d19f8d56c` | wiring CF e limite de IDs Wrangler `PLACEHOLDER_*` |
| `6d76a629f` / `8f9ab256c` / `0871789ec` | grafo, orquestração e correção formal de e2e billing |
| `453ea0787` | 31 relações Cargo explícitas do server |
| `04cbb9e6e` | resultado honesto do roteamento OKF dos novos pilotos |
| `7ab947d53` / `a220d057a` / `97e6748e1` | fronteiras semânticas server: CAS/PAT/rate, failover/Stripe/telemetry e BYOK |
| `1b44fc2c9` | seleção do harness e2e pelo smoke noturno do workspace |
| `1ba2d1f35` | bindings, cron e logs declarados do Worker CF |
| `4eb67df03` / `4096ec1a7` | conflito de chave DSR e materialização Stripe/D1 do server |
| 3ab2fe0c6 / 5939941e4 / f9011d944 | índice Cargo, caminhos audit chain e cobertura semântica do server |
| `cd1e6ffdf` / `3a6d4b31e` / `5e234f885` | fronteiras CAS erase, customer e identidades compartilhadas com hash |
| `f0bcdbe00` / `f136988da` / `b53ec76fa` / `c24392552` / `731ac4d6f` | fronteiras AC, DSR, admin, tenant-path e GC do server |
| `7f3824496` / `c4a9e4392` / `8e846fedf` | fronteiras tier-selection, adapter-host e audit-chain do server |
| `db91b16f8` / `3b19bf1fe` / `f0b600ba0` | fronteiras analytics, audit e Bazel do server |
| `6132ec3f2` / `a6423194b` / `761785f57` / `3cbd2229b` | fronteiras billing, billing-ingest, core e DPA do server |
| `840c47400` / `eab2de2c6` / `4f3832b69` | fronteiras de atestação, SLO e Turbo do server |
| `808c9730e` / `8ad07d8b9` / `4e347cbf8` | fronteiras privacy-erasure, CF bindings e PAT do server |
| `0609aa455` / `175fff58b` | fronteiras ratelimit, CAS e hash do server |
| `7d663d9fc` / `8fb10f274` / `884fcf2c1` | seleção de storage, gates de mount e provider BYOK do server |
| `22aec466c`–`5812ec9c8` | onda 013: CLI, data-transfer, OpenAPI e SBOM; seis pacotes, revisões frias por artefato e correções finais |
| `db2b21483` / `7182a8f72` | W014 `e2e-failover-router`; quatro vereditos frios `APPROVE`, correção de atomicidade integrada |
| `4aaa2c5f0`–`527234c0a` | W014 `e2e-dsr`; quatro artefatos, quatro `APPROVE` frios e scope-check do lead; integração documental |

## Evidência atual e limites

- O `corelink-server` tem 31 dependências first-party declaradas, agora registradas como relações Cargo distintas; busca estática encontrou 0–50 arquivos com cada símbolo no recorte do package. Isso não reconcilia os call paths semânticos.
- O censo semântico do server confirmou gates e limites de failover, Stripe, telemetry, BYOK, DSR e materialização. Há conflito aberto entre `routes/dsr.rs` (chave dedicada `CORELINK_ERASE_AUTH_KEY`) e mensagem/comentário de `main.rs` (chave interna compartilhada); ele foi registrado, não resolvido por documentação.
- O censo atual também separa o apagamento CAS (chave exclusiva, TDK, D1 e legitimidade DSR), o plano customer (D1/PAT/export/apagamento) e quatro fronteiras hash. As quatro usam identidades compartilhadas já presentes no piloto hash; R2, D1, Worker e requests continuam não observados.
- AC, admin, DSR, tenant-path e GC receberam registros atômicos adicionais. Em especial, GC permanece não executado, o admin não afirma operação D1 e tenant-path não permite tratar mudança de prefixo como rollback de dados.
- Tier-selection, adapter-host e audit-chain também foram separados por papel: tipo/regra versus composição, protocolos externos versus mounts e cadeia criptográfica versus outbox/rotas. O censo do servidor ainda não está completo e não habilita cold review.
- Analytics, audit e Bazel agora têm superfície estática específica: prelude/RED, emitter em memória dos cinco adapters, e bridge REAPI sobre CAS/AC. Nenhum desses vínculos prova Worker, storage remoto, exportação ou tráfego real.
- Billing, ingest, core e DPA também foram separados por contrato: billing é fachada do webhook, ingest é a taxonomia+staging de uso, core fornece tipos apex e DPA bloqueia tier-select antes de Stripe. Segredos, D1, webhook, recibos e checkout seguem não observados.
- Atestação, SLO e Turbo agora distinguem assinatura de evidência, cálculo/log local de alerta e protocolo de cache/seleção de backend. Não houve attestation, PagerDuty, R2, cliente Turbo ou tráfego externo observado.
- Privacy-erasure, CF bindings, PAT, ratelimit, CAS e hash foram refinados por seleções e falhas estáticas: os stubs wasm não são bindings reais; o limiter in-process pode resetar; CAS separa memória, R2 recusado e gates D1; e as relações hash específicas permanecem REL-047–050. Essas constatações não provam deploy, D1/R2, autenticação edge ou requests reais.
- A seleção de storage agora separa a presença de `StorageEnv` da construção de cada client; os gates de rota são por superfície, não um sinal único de produção; e BYOK seleciona no compile-time um único provider, sem fallback criptográfico local. Nenhuma dessas leituras prova conectividade, chave, KMS, mount efetivo ou operação remota.
- A execução deixou de ser serial por piloto: a primeira onda de autoria foi congelada em `WAVE_001_PLAN.md` e foi despachada em seis worktrees isolados para `corelink-ac`, `corelink-adapter-host`, `corelink-analytics`, `corelink-audit`, `corelink-audit-chain` e `corelink-bazel-bridge`. Cada agente possui só os quatro artefatos do seu package; integração, checker reproduzido e cold review continuam com o lead e revisores independentes.
- A onda 001 concluiu autoria, correção factual e revisão fria por package para AC, adapter-host, analytics, audit, audit-chain e Bazel bridge. As revisões acharam e fecharam, entre outros, semântica de resultados vazios/SQL AC, sessão OCI sem vínculo de tenant, forma serializada de `AuthEvent`, flags/testes Neon e o limite entre bridge Bazel e mapper HTTP do container. Os seis conjuntos foram integrados, mas isso não congela o standard nem libera issue/publicação.
- `WAVE_002_PLAN.md` e `ROLLOUT.md` registram uma fila explícita de 105 packages provisórios. A onda 002 concluiu autoria, correções factuais e revisões frias separadas para `corelink-auth`, `corelink-core`, `corelink-crypto`, `corelink-dpa-acceptance`, `corelink-dsr` e `corelink-pat`; os seis conjuntos foram integrados. As revisões corrigiram identidade Cargo versus diretório, limites de consumidores estáticos e a condição completa de falha antes do retorno MFA. Isso é evidência documental estática, não freeze do standard, prova de runtime ou autorização de publicação.
- A onda 003 concluiu autoria, revisões frias, correções e integrações para `corelink-billing-aggregator`, `corelink-billing-emit`, `corelink-billing-reconcile`, `corelink-billing-stripe`, `corelink-billing-stripe-materializer` e `corelink-billing-stripe-traits`. As revisões fixaram transições de UPSERT idempotente, a fórmula exata de drift, importações somente de teste e o censo de re-exports. Os seis worktrees temporários foram removidos depois da integração. O resultado continua sendo evidência documental estática, não certificação de provider, runtime, cobrança ou publicação.
- A onda 004 concluiu autoria, revisões frias, correções e integrações para `corelink-handler-ac`, `corelink-handler-admin`, `corelink-handler-cas`, `corelink-handler-cas-erase`, `corelink-handler-customer` e `corelink-cas`. As revisões corrigiram classes de dependência, ordens condicionais de auditoria/mutação, caminhos idempotentes de revoke, limites de fake temporário e a separação entre dependência declarada e uso fonte. Os seis worktrees temporários foram removidos após integração. Isso permanece evidência documental estática, não prova de HTTP, Worker, R2/D1, armazenamento, identidade ou runtime.
- As ondas 005 e 006 concluíram autoria, revisão fria, correções e integrações para 12 packages de adapters, Clerk, configuração, WASM, BYOK e privacy. As revisões capturaram inventários de testes implícitos versus manifestos, separação entre facades e re-exports, relações pipeline estáticas, módulos locais reais, tipos de resultado idempotente e limites de provider/runtime. Os worktrees temporários dessas ondas foram removidos após integração. OKF foi usado como referência canônica já verificada, sem duplicação de política. Nenhum resultado certifica serviço externo, operação de dados, legalidade, credencial ou runtime.
- As ondas 007 e 008 concluíram autoria, correção, revisão fria por artefato e integração para 12 packages de ciclo de storage, regiões, rate limit, REAPI e observabilidade. As revisões corrigiram, entre outros, relações atômicas, decisões com evidência e parada, procedimentos recuperáveis, autoria em facades híbridas, forma exata de invariantes e âncoras de evidência fonte. Os worktrees temporários foram removidos após integração. Os resultados permanecem documentação SOURCE, não prova de Cargo, serviço, provider, deploy ou runtime.
- As ondas 009 e 010 concluíram autoria, correção, revisão fria por artefato e integração para 12 packages de metadata, operações, replicação, runbooks, runners e adapters externos. As revisões corrigiram inventários de targets, semântica de erros/retorno, pré-condições de relações, cobertura declarada versus selecionada, cardinalidade de relações e limites de adapters. Todos os worktrees temporários foram removidos. Isso continua sendo evidência documental SOURCE e não certifica operação, provider, cobrança, integração externa, deploy ou runtime.
- A onda 011 concluiu autoria, correção, revisão fria por artefato e integração para seis packages de integrações de serviço e composição: Stripe, tier selection, transparency log, Turbo bridge, worker e client verify. As revisões corrigiram direção e atomicidade de relações, contagens de inventário, navegação, evidência-fonte e predicados de manutenção. Os seis worktrees temporários foram removidos. Nenhum artefato comprova pagamento, SDK, auditoria, requisição, armazenamento, deploy ou comportamento em runtime.
- A onda 012 concluiu autoria, correção, revisão fria por artefato e integração para seis packages de schedulers, inquiry, drift e SDKs. As revisões corrigiram relações compostas, direções, predicados de erro, contagens, citações exatas, comandos reproduzíveis e limites entre helpers estáticos e operação. Todos os worktrees temporários foram removidos. Nenhum artefato comprova cron, Statuspage, Terraform, provider, customer request, release ou runtime.
- A onda 013 concluiu autoria, correções e cold review independente dos quatro artefatos para `corelink-cli`, `corelink-dt-cli`, `corelink-dt-reconcile`, `corelink-dt-webhook`, `corelink-openapi` e `sbom-publish`. Foram corrigidas citações, atomicidade das relações, rotas de navegação, limites de valores e claims de crate-root/metadata. Os 24 caminhos passaram o checker estrutural S e `git diff --check`; o checker não certifica completude semântica, runtime ou revisão fria. Os seis worktrees de autoria e os seis de revisão foram removidos depois do selo de integração.
- A onda 014 tem seis conjuntos de quatro documentos produzidos sob baseline-fonte `cb94e251c`. `e2e-dsr` foi integrado apenas após quatro aprovações independentes nos bytes finais (`fee2a49`) e reprodução dos quatro checkers S pelo lead; isso não prova execução do harness ou comportamento runtime. `e2e-failover-router` permanece integrado, mas o veredito atual de Maintenance é `FIX_FIRST`: corrigir a separação entre ledger e estado do router no M04 e o local/conteúdo da ambiguidade de autorização do Override no runbook. A aceitação de operação continua `BLOCKED` por autoridade/staffing não verificados.
- Retomada W014 em 2026-09-21: `e2e-chaos` (`ec88bfe`) aguarda correções de Reference/Blast após achados de callbacks públicos, configuração `PROPTEST_CASES`, migration, mutation workflow, atomicidade e call-site census. `e2e-byok-revoke` (`d72f43d`) está em rereview fria de Reference/Blast depois de novo ajuste das direções explícitas de REL; skill/manutenção não mudaram. `chaos-campaign` (`23cee3c`) tem Skill/Blast aprovados; Reference está em correção para âncoras/traits e Maintenance continua `BLOCKED` até existir rota real de source owner/escalation. `e2e-pilot-onboarding` (`dbed78b`) teve `FIX_FIRST` nos quatro; o author corrige o skill, contratos atômicos, censo SBOM/peer relation e manual de validação. Duas reviews preliminares foram contaminadas/interrompidas; uma review formal limpa encontrou os quatro grupos de achados. Nenhuma outra aprovação foi presumida. Source/manifests dos seis packages continuam sem diferenças em relação ao baseline de autoria `cb94e251c` vs `main` observado `1177dad2c`.
- Auditoria inicial de adoção do contrato v1.3 em `95cb49328`: dos 84 manuais integrados, busca literal encontrou `PROC-###` em 20 e encontrou campos `**Mode:**`/`**Modo:**` com token fora de `READ_ONLY`, `LOCAL_ISOLATED`, `AUTHORIZED_OPERATION` em 31. A busca é apenas estrutural e não classifica tabelas/casos sem esses marcadores; ela não é um veredito de cada arquivo. Depois do freeze do contrato, o lead deve reconciliar os 105 conjuntos, atualizar o que não satisfizer CO-08 e invalidar/reemitir reviews afetadas. Aprovações históricas contra contratos anteriores não serão reutilizadas como aprovação do contrato congelado.
- Auditoria estrutural reproduzível do checker candidato v1.3 em 2026-09-21: 84 diretórios com `REFERENCE.md`, 336 checagens dos quatro caminhos esperados, zero arquivos ausentes, 55 checagens falharam e um `REFERENCE.md` não declara perfil. Os erros observados incluem seções/âncoras/índices antigos, blocos de procedimento e metadados fora do contrato. Este resultado mede apenas a estrutura no checkout corrente; não prova completude semântica nem review, e qualquer perfil faltante foi diagnosticado como S para identificar erros adicionais.
- A execução ampla de `cargo test -p corelink-server --lib` foi iniciada e interrompida por ser desproporcional ao rascunho documental. Não registrar resultado, sucesso ou falha dessa execução.
- `corelink-cf-bindings` foi lido estaticamente: target wasm32 pretendido, stubs nativos `WasmOnly:`, consumidores Cargo `corelink-adapters-cloud`, `corelink-clerk-cf` e `corelink-dsr-statuspage-scheduler`, e reexport por `corelink-adapters-cloud::cf`. O Wrangler de `corelink-clerk-cf` declara KV/D1/R2/DO, cron e logs, mas IDs/vars são `PLACEHOLDER_*`; isso não prova recurso, deploy ou observação.
- Nenhum build wasm, binding Cloudflare, deploy, credencial ou runtime foi executado/observado.
- `e2e-billing-flow` foi lido estaticamente: package de teste `publish=false`, seis cenários de integração e composição in-memory de signup, tier selection, billing-stripe e DSR. Nenhum `cargo test` foi executado nesta etapa; não há evidência de Stripe, DSR ou cobrança em runtime.
- O grafo de `e2e-billing-flow` foi consultado com `cargo tree --locked --offline`, sem compilação: confirma as quatro dependências diretas e não retorna consumidor Cargo normal. Isso não é execução nem prova de CI/runtime.
- A busca nominal não encontrou job de `e2e-billing-flow`, mas `nightly.yml` chama `ci-bounded-workspace-tests.sh`, que declara `--workspace --all-targets`; essa é uma seleção estática do harness, não evidência de execução ou resultado. `e2e-pilot-onboarding` apenas o cita como cobertura complementar.
- O censo atual classifica 107 manifests: 95 packages workspace, 10 fuzz independentes, raiz virtual e um arquivo histórico. A classificação `archive` ainda não existe no contrato compartilhado, portanto a elegibilidade global continua provisória.
- As 20 peças dos cinco pilotos passaram em `check_docs.py` na varredura formal; referências CF/e2e alteradas depois também passaram individualmente. Isso não certifica conteúdo, perfil, links externos, navegação cross-file, cold review ou runtime.
- O framework comum do ZIP (`STANDARD`, schema, templates, gates e checker) ainda não foi integrado ao repo. O checker usado está na extração controlada do ZIP; uma integração revisada é requisito antes de freeze/PR e não pode ser apresentada como existente hoje.
- A revisão do ZIP confirmou um bloqueio específico de integração: `tools/prepare_census.py` só aceita `first_party_independent`, `third_party_vendor` e `test_fixture`, enquanto o censo real contém a exclusão justificada `archive`. A classificação precisa ser acrescentada e testada antes de adotar a ferramenta; não será forçada para vendor ou fixture.

- A reancoragem observou `main` em `1177dad2c` em 2026-09-21, 138 arquivos além da baseline, incluindo cinco manifests e fontes de audit, ops, SBOM, Worker e migrations. O standard/OKF e o manifest raiz não mudaram; o censo continua provisório. Reviews da baseline não aprovam automaticamente a ponta atual. A decisão é preservar a baseline e reconciliar seletivamente os packages afetados antes de freeze/emissão; detalhe em [REANCHOR_DELTA_20260921.md](REANCHOR_DELTA_20260921.md) e [REANCHOR_DELTA_20260920.md](REANCHOR_DELTA_20260920.md). Não houve rebase.
- Reancoragem adicional em 2026-09-21: `git ls-remote origin refs/heads/main` agora resolve `c363612a5db81906b632f64c9ef449a55902777a`. O delta desde `1177dad2` é restrito a `.github/workflows/billing-health-daily.yml`, `README.md`, `scripts/verify_b113_lane_population.py`, `scripts/verify_b139_semgrep.py` e `tests/test_verify_b113_lane_population.py` (111 adições/17 remoções). Nenhum manifesto Cargo, fonte dos candidatos W014/W015/W016 ou standard/OKF aparece nesse delta; ainda assim, o pin novo substitui `1177` para qualquer discovery/freeze futuro. Candidatos já escritos permanecem explicitamente ancorados em `1177` até reconciliação; não houve rebase.
- W015 foi iniciada sem esperar o encerramento de W014: `main` foi relida em `1177dad2ca2a9f21c29b5a118aa7944b77147798`, e comparação estática dos seis packages + manifest raiz não mostrou drift desde esse pin. `e2e-signup-flow` (`8a6053a`), `migrate-single-to-multi-region` (`275ead3`) e `e2e-user-journeys` (`0d7e463`) foram integrados após cold APPROVE nos bytes finais e reprodução dos gates. Os três candidatos restantes continuam em correção/review: replication (`e5f3f83`), resilience (`1499ee5`), tenant-isolation (`0c9a33a` + peer BLAST). O primeiro reviewer de resilience abriu indevidamente um REVIEW-INPUT de outro package; resultado descartado e substituído por sessão fria limpa. Nenhum dos três restantes está aprovado.
- W016 abriu para reduzir a fila sem tocar em fuzz/runtime: o plano enumera e distingue os dez manifests independentes; oito authors foram dispatchados no baseline `8cdc02828132b9b6f03a3b57117b8140325f6762`. Os oito já têm commits: tenant-path (`88a1bc0`), worker (`06c52ef`), meta (`123fe34`), reapi (`41cc43f`), client-verify (`425c19a`), BYOK (`f957668`), audit-chain (`4237318`) e hash (`6ea9bdd`). O lead reproduziu quatro checkers + escopo em BYOK/audit-chain; os demais autores reportaram gates equivalentes. `corelink-ac-fuzz`, `corelink-cli-fuzz`, `corelink-client-verify-fuzz` e `e2e-chaos` foram integrados após quatro APPROVE frios; `corelink-byok-fuzz` foi integrado após quatro APPROVE locais, condicionado ao peer parent; `corelink-hash-fuzz` também foi integrado após quatro APPROVE locais, com peers parent REL-037/038 pendentes; os demais fuzz seguem em review/correção. `main=1177dad2` foi relido e as dez árvores fuzz + manifest raiz não mostraram drift. Perfis S seguem padrão observável; AC não depende de `corelink-ac`, e client-verify ativa `stream`/`ffi` com `unsafe` apenas no harness.
- Read-only `git ls-remote origin refs/heads/main` em 2026-09-21 confirmou `1177dad2ca2a9f21c29b5a118aa7944b77147798`. Comparação da árvore dos cinco pilotos entre `cca798ff5` e esse `main`: somente `crates/corelink-hash/Cargo.toml` difere, pelo campo de URL `repository` (`HuGR-Labs` → `HuGR-dev`); os outros quatro manifestos/fontes-alvo de piloto não diferem. O piloto hash ainda exige atualização do pin/claim e nova revisão afetada.

- Após os refinamentos atuais, a varredura repetiu os 20 artefatos com o perfil declarado em cada blast-radius (Hash/CF/e2e S; Billing/Server H): todos passaram estrutura, limites e links locais. O veredito continua somente IMPLEMENTED_CHECKS_PASS.

- Ajuste de escopo de governança em 2026-09-21: não serão criadas novas camadas de reconciliação bilateral para cada relação estática. A exigência permanece apenas onde há fronteira pública/operacional material; nos demais casos, a lacuna é registrada como `UNRESOLVED`/`BLOCKED` com evidência e sem aprovação implícita. Isso reduz recursão documental sem relaxar atomicidade, rastreabilidade ou revisão fria.

- Higiene de sessão em 2026-09-21: worktrees temporários limpos da campanha foram removidos e os refs/commits foram preservados; o checkout principal do usuário permaneceu intocado. Um worktree W016 que não estava limpo não foi usado como fonte de integração.

- Reconciliação mecânica da população em 2026-09-21: os 105 packages provisórios agora possuem os quatro caminhos esperados. Isso fecha apenas a presença documental; os cinco conjuntos finais continuam sem aprovação global conforme registrado acima.

- A higiene desta retomada removeu o worktree/branch W016 `codex/w016-ac-fuzz`, que havia sido criado pelo campaign lead mas ficou sem author devido ao limite de frota. O package permanece pendente de dispatch; nenhum outro worktree/branch de terceiros foi removido.

- Releitura final desta retomada: `origin/main` avançou de `96fd6779` para
  `a2a03b2b`. O delta ficou restrito a workflows/scripts/teste do bundle CLI e
  OKF; nenhum caminho de fonte, manifesto, migração ou harness dos cinco
  pilotos mudou. Os pins dos pilotos permanecem válidos para os paths
  revisados, mas `a2a03b2b` passa a ser o SHA obrigatório para o próximo
  pre-freeze/integration readback. Evidência:
  [`evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-A2A0.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-A2A0.md).
- Nova releitura encontrou `origin/main=ab10690b`; o delta seguinte também é
  restrito a CI, schemas/scripts de capacity/TLS e testes/fixtures. Nenhum
  caminho dos cinco pilotos mudou. O SHA `ab10690b` substitui `a2a03b2b` como
  pin obrigatório do próximo pre-freeze readback. Evidência:
  [`evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-AB10.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-AB10.md).
- O fetch seguinte encontrou `origin/main=47f4db7f`. O delta agora inclui
  mudança real em `routes/cargo`: o Worker-authenticated storage cap é escopado
  no Cargo PUT e coberto por dois testes fonte. O server foi reancorado,
  REL-056/API-007/INV-006 foram adicionados, e o registry foi regenerado para
  105/105. Essa mudança invalida a cold review anterior do server; novo review
  independente é obrigatório. Evidências:
  [`evidence/revision-1.4/SERVER-CURRENT-MAIN-READBACK-20260922-47F4.md`](evidence/revision-1.4/SERVER-CURRENT-MAIN-READBACK-20260922-47F4.md) e
  [`evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260922-47F4.md`](evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260922-47F4.md).
- A rereview fria independente do server foi concluída depois do ajuste de
  B06: os quatro artefatos atuais estão `APPROVE` documentalmente, incluindo
  API-007/INV-006/REL-056 e a contagem de nove relações semânticas. Isso não
  certifica os procedimentos locais, build, Worker, D1/R2 ou runtime.
  Evidência:
  [`evidence/revision-1.4/SERVER-COLD-REVIEW-20260922-47F4.md`](evidence/revision-1.4/SERVER-COLD-REVIEW-20260922-47F4.md).
- Nova leitura remota encontrou `origin/main=50a5ab3a`. O delta alterou
  accounting/BYOK e CAS no server, além do lockfile do fuzz CLI. A aprovação do
  server em `47f4` continua válida para o pin imutável; apenas claims que
  promovam esses bytes a `main=50a5` precisam de reconciliação nas relações
  afetadas. Billing, CF, E2E e os três artefatos não-blast do hash não precisam
  ser refeitos. Evidência:
  [`evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-50A5.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-50A5.md).
- Após o snapshot, o remoto avançou para `5e0b2144` com nova reorganização de
  `main.rs`/DSR e CI. O snapshot da campanha permanece deliberadamente em
  `50a5ab3a`; esse delta posterior é externo ao fechamento e exige novo ciclo
  antes de qualquer claim current-main.
  Evidência:
  [`evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-5E0B.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260922-5E0B.md).

## Readback final desta retomada — 2026-09-23

- `origin/main` foi relido em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`; o snapshot imutável da campanha continua `50a5ab3a0e3e90bff56beb18fade52f50e3ff005`. A comparação está em [`MAIN-DRIFT-READBACK-20260923-B9B3.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260923-B9B3.md).
- `corelink-server` foi reancorado para o SHA atual em todos os quatro artefatos; os bytes reancorados ainda requerem cold review independente.
- A calibração dos cinco pilotos foi medida nos bytes atuais em [`PILOT-CALIBRATION-READBACK-20260923-CURRENT.md`](evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260923-CURRENT.md).
- A reconciliação hash ampliou o censo para as 13 relações antes sem par e manteve `peer_review=not_reconciled` quando a evidência não fechou identidade, owner ou runtime. O censo e a aprovação limitada estão em [`HASH-PEER-CENSUS-20260923.md`](evidence/revision-1.4/HASH-PEER-CENSUS-20260923.md) e [`HASH-COLD-REVIEW-20260923-PEER-RECONCILIATION.md`](evidence/revision-1.4/HASH-COLD-REVIEW-20260923-PEER-RECONCILIATION.md).
- Reviews frias adicionais encontraram correções documentais necessárias em pacotes restantes; nenhum checker estrutural foi tratado como aprovação semântica.

### Releitura de `main` — 2026-09-23 (`cd74a094`)

- `origin/main` avançou para `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`.
  Permanecem 107 manifests rastreados, sem delta em `Cargo.toml`/`Cargo.lock`,
  framework de ownership ou perfil congelado `docs/internal/okf-wiki/`.
- O delta atual de fonte relevante para ownership é o teste de durabilidade de
  `corelink-server`: a matriz agora simula ACK perdido após commit e retry que
  resulta em `Deduped` com um único vencedor durável. Um workflow focado e o
  contrato operacional de `issue-1635` também foram acrescentados; nenhuma
  execução é inferida.
- Correção explícita: a leitura B9B3 dizia que todo o tree server permanecia
  igual desde `5e4339c`. A comparação fresca encontrou a mudança nesse arquivo
  de teste, logo o server ainda exige SOURCE readback e cold review contra o
  tip atual. O pin `5e4339c` dos quatro artefatos é histórico para esta cobertura.
- Três páginas de conhecimento ADR receberam reparos de proveniência/citações;
  o perfil OKF congelado e os artefatos de ownership não mudaram em `main`.
  Isso não altera decisão de campanha nem aprova os ADRs como fonte dos packages.
- Evidência detalhada: [`MAIN-DRIFT-READBACK-20260923-CD74.md`](evidence/revision-1.4/MAIN-DRIFT-READBACK-20260923-CD74.md).

## Próxima sequência segura

1. Fechar correções e reviews frias dos quatro candidatos ainda não integrados da onda 014; corrigir e fazer nova review fria da manutenção integrada de `e2e-failover-router`; integrar somente bytes verdes.
2. Concluir autoria/review fria W015 e W016 em paralelo; despachar `corelink-ac-fuzz` e `corelink-cli-fuzz` assim que houver capacidade, com review separada.
3. Concluir os cinco pilotos, reconciliar o censo Cargo integral e calibrar limites sem declarar completude por checker.
4. Integrar/revisar o framework comum, incluindo a classificação `archive`, e então congelar o contrato com baseline imutável.
5. Fazer a migração de conformidade dos 105 conjuntos para o contrato congelado; revisar mudanças de bytes e invalidar aprovações dependentes do contrato antigo.
6. Hidratar os issue seeds restantes, reconciliar backlog/issues duplicadas e executar preflight package a package.
7. Publicar apenas packages aprovados; reconciliar readback/ledger e fazer auditoria/higiene final.

## Regras preservadas

- Não publicar issues, não empurrar branch e não alterar `main`.
- Não fabricar cold review, execução, reachability ou runtime.
- Usar o OKF como referência canônica; não copiar nem redefinir sua política.
- Mudança de bytes exige nova review do artefato alterado.

### Readback adicional — 2026-09-23 (`91630ba`)

- O último `origin/main` relido durante esta execução é
  `91630baebe3ae7abe686cd4e06a5621ecdc4ab73` (`docs(auth): repair knowledge
  citation evidence`). O avanço desde `38f43b6` é restrito a páginas de
  conhecimento de autenticação; não altera `corelink-container`, o teste de
  ACK perdido, o workflow focado nem o contrato operacional já reancorados.
- O checkout de campanha continua no snapshot documental `247af6c5`; nenhum
  source Cargo foi alterado pela campanha. Os quatro artefatos de
  `corelink-server` foram reancorados para `91630ba` e mantêm o teste de
  ACK-perdido como `SOURCE`, não como execução observada.
- `corelink-runner-aggregate` foi reancorado ao tip de package verificado;
  `corelink-terraform-drift-consumer`, CLI-fuzz e rate-headers foram
  reconciliados contra as superfícies de source/manifest observadas. As
  revisões frias correspondentes ainda são obrigatórias para os bytes novos.
- Evidência do server: [`SERVER-MAIN-READBACK-20260923-91630.md`](evidence/revision-1.4/SERVER-MAIN-READBACK-20260923-91630.md).
- Readback consolidado desta execução: [`FINAL-AUDIT-READBACK-20260923.md`](evidence/revision-1.4/FINAL-AUDIT-READBACK-20260923.md). Ele registra os 420 checks estruturais, os 137 testes documentais, a ausência de publicação e os blockers de cold review sem promovê-los a aprovação.

### Readback de pilotos corrigidos — 2026-09-23 (`91630ba`)

- `corelink-billing` agora cataloga os símbolos exatos das dez fachadas, sem duplicidade no API-009; `corelink-cf-bindings` separa os contratos públicos por símbolo e torna INV-002/003/004 falsificáveis.
- `corelink-hash` REL-037–042 e `corelink-server` REL-082–087 têm identidades compartilhadas e backlinks bilaterais; REL-039/REL-084 registram também `crypto_mode` no fingerprint de política BYOK.
- Cold review independente scoped final: hash, billing e CF passaram a `APPROVE` nos quatro artefatos documentais; não houve Cargo/runtime. A aprovação não se estende às ondas restantes nem autoriza congelamento/publicação.
- A calibração foi corrigida para os bytes atuais: billing Reference `486 / 3.982 / 39.798` e server Blast `1.127 / 9.894 / 87.621` (linhas/palavras/bytes). Evidência: [`PILOT-CALIBRATION-READBACK-20260923-CURRENT.md`](evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260923-CURRENT.md).
- Registry: 105 identidades, 420 checks estruturais, 0 issues publicados; suite documental: 137 testes `OK`. Permanecem sem fechamento: revisão fria das ondas com `FIX_FIRST/BLOCKED`, censo Cargo certificado, standard congelado e publicação/deduplicação.

### Reparos das ondas seguintes — 2026-09-23

- Group B corrigiu a identidade region↔SLO, os links de probe, as relações separadas de Slack ops/cloud e a semântica de redaction; telemetry e SLO foram conferidos contra as superfícies já reparadas.
- Group A fez um passe bounded em adapter-host/REAPI/meta-fuzz/audit-chain-fuzz/meta/analytics, canonicalizando modos, pins e algumas chaves bilaterais. Ainda faltam contratos públicos e cobertura atômica completos em adapter-host, CLI-fuzz, handler-customer, rate-headers e REAPI.
- Group C acrescentou contratos/invariantes e relações nos pacotes worker, e2e-failover-router e tracing; os dois blast-radius que excediam o limite de parágrafo foram normalizados e agora passam os checkers.
- Diagnóstico local seguro foi executado somente para seleção offline/metadata e inventário de toolchain; nenhum build, teste Rust, fuzz, runtime ou deploy foi alegado. O readback está em [`LOCAL-DIAGNOSTICS-READBACK-20260923.md`](evidence/revision-1.4/LOCAL-DIAGNOSTICS-READBACK-20260923.md), e PROC-001 do server permanece `REVIEWED_NOT_EXECUTED` por pin divergente.
- Após esses reparos: registry 105/105 estrutural `PASS`, 420 checks, 137 testes documentais `OK`, 0 issues publicados. Cold reviews independentes das ondas alteradas ainda são necessárias; a campanha continua sem freeze/publicação.

### Rereview e reconciliação cross-package — 2026-09-23

- Group B rereview encontrou e foi corrigido: direção SLO↔telemetry, peer explícito de region/SLO e redação da condição de `redact_webhook`.
- Group C rereview encontrou e foi corrigido: referências de INV-004 no procedimento de sampling de tracing, B02–B06 no PROC-001 do worker e parágrafos que faziam os blast checkers falharem.
- Group A rereview encontrou e foi corrigido: resumo stale de audit-chain-fuzz, chave bilateral CLI-fuzz↔CLI, identidade handler-CAS↔adapter-host e relação adapter-host↔REAPI. Permanecem pendentes a expansão exata de assinaturas públicas em adapter-host/rate-headers/REAPI e a decomposição completa de alguns fluxos handler-customer.
- Estado atual: registry 105/105 `PASS`, suite 137 `OK`, branch limpo após commit `a329e8038`, 0 issues publicados. Os blockers semânticos remanescentes impedem freeze e publicação; nenhuma aprovação global é alegada.
