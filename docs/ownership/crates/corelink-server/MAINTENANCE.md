---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-server
manifest: crates/corelink-container/Cargo.toml
source_commit: 91630baebe3ae7abe686cd4e06a5621ecdc4ab73
profile: H
state: draft
evidence_set: server-main-readback-20260923-91630
---

# corelink-server — manual de manutenção

**Status do source:** procedures abaixo usam o pin imutável `91630ba`. O
readback de 2026-09-23 registrou que a árvore do package preserva o teste de
ACK perdido, o workflow focado e o contrato operador; veja
[evidência](../../evidence/revision-1.4/SERVER-MAIN-READBACK-20260923-91630.md).
Isto continua sendo leitura SOURCE, sem execução de teste, build, workflow ou runtime.

[Preparação](#m01) · [Escolher](#m02) · [Procedimentos](#m03) · [Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Confirme SHA, target, binário e features antes de agir.
Use checkout limpo, modo locked/offline e target directory temporário.
Não revele segredos nem execute D1, R2, Stripe, KMS, GC, Worker ou deploy por este manual.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito | Autorização |
|---|---|---|---|---|
| package, target, feature | [PROC-001](#proc-001) | READ_ONLY | leitura | não |
| boot, health, mount | [PROC-002](#proc-002) | LOCAL_ISOLATED | teste local | não |
| storage/fallback | [PROC-003](#proc-003) | LOCAL_ISOLATED | env sintético | backend real |
| BYOK/CF | [PROC-004](#proc-004) | LOCAL_ISOLATED | compile matrix | KMS/CF |
| operação de dados | [PROC-005](#proc-005) | AUTHORIZED_OPERATION | nenhuma padrão | sim |
| staging billing / ACK perdido | [PROC-006](#proc-006) | LOCAL_ISOLATED | teste SQLite loopback | não |

<a id="m03"></a>
## M03 — Procedimentos

Registro canônico por procedimento (PROC-001 teve seleção local parcial; demais procedimentos não foram executados nesta campanha):

| PROC-ID | review_status | execution_status | required_for_acceptance | environment | result |
|---|---|---|---|---|---|
| PROC-001 | `REVIEWED` | `REVIEWED_NOT_EXECUTED` | `true` | checkout local isolado; sem rede | package/targets/features observados em `b47f3cf`; source pin divergente, procedimento incompleto |
| PROC-002 | `REVIEWED` | `REVIEWED_NOT_EXECUTED` | `true` | target nativo local; env sintético | boot/router ainda não observados em execução |
| PROC-003 | `REVIEWED` | `REVIEWED_NOT_EXECUTED` | `true` | harness local; sem endpoint remoto | fallback/gate ainda não observados em execução |
| PROC-004 | `REVIEWED` | `REVIEWED_NOT_EXECUTED` | `true` | target/feature local isolado | matriz ainda não observada em execução |
| PROC-005 | `REVIEWED` | `BLOCKED_FOR_OPERATION` | `false` | ambiente remoto autorizado | operação não autorizada nesta campanha |
| PROC-006 | `REVIEWED` | `REVIEWED_NOT_EXECUTED` | `true` | teste isolado SQLite loopback; sem D1/segredo | classificação por retry descrita no source, sem execução |

`review_evidence` é a leitura deste manual contra o source pin; `execution_evidence` aponta
para a evidência parcial de PROC-001 abaixo, é `none` para PROC-002..004 e `not_applicable`
para PROC-005. Nenhum desses estados certifica runtime ou produção.

<a id="proc-001"></a>
### PROC-001 — Confirmar seleção
**Gatilho / resultado esperado:** identificar package, binário, target e feature corretos.
**Modo / ambiente / permissão:** READ_ONLY no checkout local.
**Entradas e validação:** SHA e seleção pretendida.
**Pré-condições:** árvore de trabalho identificada.
1. Rode git rev-parse HEAD e git status --short.
2. Leia o manifesto do container.
3. Rode cargo metadata locked/offline sem dependências.
4. Liste bins, tests e features antes de escolher comando.
**Falhas e parada:** SHA, package, target ou feature ambíguos.
**Recuperação:** nenhuma escrita ocorreu; corrija seleção.
**review_status:** `REVIEWED`.
**execution_status:** `REVIEWED_NOT_EXECUTED`.
**required_for_acceptance:** `true`.
**result:** package/targets/features observados no checkout `b47f3cf`; source pin desta manutenção diverge, então PROC-001 permanece incompleto.
**review_evidence:** source pin, manifesto e este procedimento.
**execution_evidence:** [LOCAL-DIAGNOSTICS-READBACK-20260923.md](../../evidence/revision-1.4/LOCAL-DIAGNOSTICS-READBACK-20260923.md), comando metadata offline; pin divergente.
**limitations:** metadata, target e estado do checkout precisam ser capturados no ambiente indicado.
[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Validar boot e router local
**Gatilho / resultado esperado:** mudança de boot, health, mount ou body limit.
**Modo / ambiente / permissão:** LOCAL_ISOLATED, sem segredo e backend remoto.
**Entradas e validação:** target nativo e variáveis sintéticas mínimas.
**Pré-condições:** identificar relação e rota.
1. Rode testes de router ou boot do módulo afetado.
2. Verifique health com fake ou stub.
3. Teste secret ausente e confirme rota não montada.
4. Preserve output e mounts observados.
**Falhas e parada:** listener tenta backend real ou requer chave.
**Recuperação:** descarte env temporário; restaure código e rerode negativo.
**review_status:** `REVIEWED`.
**execution_status:** `REVIEWED_NOT_EXECUTED`.
**required_for_acceptance:** `true`.
**result:** boot/router ainda não observados em execução.
**review_evidence:** main/routes e relações REL-001/003/008.
**execution_evidence:** `none`.
**limitations:** somente env sintético; não prova backend, edge ou runtime remoto.
[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Diagnosticar seleção de storage
**Gatilho / resultado esperado:** backing ou mount inesperado.
**Modo / ambiente / permissão:** LOCAL_ISOLATED com env sintético.
**Entradas e validação:** nomes de env, nunca valores.
**Pré-condições:** ler API-002 e REL-002.
1. Inspecione StorageEnv e sinal de produção independente.
2. Teste ausência completa no harness.
3. Confirme health sem exposição de credencial.
4. Para PUT Cargo, confira `STORAGE_QUOTA_HEADER` inválido/ausente e o escopo task-local sem consultar D1 real.
5. Pare antes de endpoint, token, bucket ou D1 real.
**Falhas e parada:** backend real ou resultado contraditório.
**Recuperação:** limpe apenas env temporário e escale storage.
**review_status:** `REVIEWED`.
**execution_status:** `REVIEWED_NOT_EXECUTED`.
**required_for_acceptance:** `true`.
**result:** fallback/gate ainda não observados em execução.
**review_evidence:** `StorageEnv`, API-002 e REL-002.
**execution_evidence:** `none`.
**limitations:** nenhum endpoint, token, bucket ou D1 real pode ser usado.
[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Validar feature BYOK ou CF
**Gatilho / resultado esperado:** mudança em feature de provider ou binder.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; compilação não acessa KMS.
**Entradas e validação:** uma feature BYOK real ou nenhuma.
**Pré-condições:** target e matriz definidos.
1. Confirme que não há duas features BYOK reais.
2. Compile apenas seleção autorizada.
3. Trate stub nativo e binder wasm como caminhos distintos.
4. Não construa provider com credencial.
**Falhas e parada:** feature ambígua, target ausente ou necessidade de credencial.
**Recuperação:** volte à seleção anterior e registre matriz.
**review_status:** `REVIEWED`.
**execution_status:** `REVIEWED_NOT_EXECUTED`.
**required_for_acceptance:** `true`.
**result:** matriz ainda não observada em execução.
**review_evidence:** manifesto, orquestrador BYOK e API-004.
**execution_evidence:** `none`.
**limitations:** não prova binder wasm, KMS ou provider real.
[Índice de procedimentos](#m02)

<a id="proc-005"></a>

### PROC-005 — Operação remota autorizada
**Gatilho / resultado esperado:** GC, Stripe, D1, R2, KMS ou Cloudflare reais.
**Modo / ambiente / permissão:** AUTHORIZED_OPERATION.
**Entradas e validação:** ticket, owner, escopo, backup e credencial aprovada.
**Pré-condições:** rollback ou roll-forward e janela definidos.
1. Validar escopo e identidade com owner.
2. Executar dry-run se existir.
3. Executar somente comando autorizado.
4. Verificar estado remoto e recuperação.
**Falhas e parada:** escopo amplo, escrita incerta, segredo exposto ou rollback ausente.
**Recuperação:** seguir plano do owner; revert não repara dado remoto.
**review_status:** `REVIEWED`.
**execution_status:** `BLOCKED_FOR_OPERATION`.
**required_for_acceptance:** `false`, com justificativa: operação remota está fora do escopo documental desta campanha.
**result:** operação não autorizada nesta campanha.
**review_evidence:** fronteiras R2/D1/Stripe/KMS/GC e M05.
**execution_evidence:** `not_applicable`.
**limitations:** exige ticket, owner, escopo, credencial e janela aprovados; não há certificação produtiva.
[Índice de procedimentos](#m02)


<a id="proc-006"></a>
### PROC-006 — Validar classificação de staging após ACK perdido
**Gatilho / resultado esperado:** mudanças em staging/fingerprint ou 503; após ACK perdido pós-commit, retry igual retorna `Deduped` e mantém uma linha.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; SQLite loopback, sem D1 remoto, segredo ou consumer.
**Entradas e validação:** `corelink-server` lib e teste nomeado; confirme SHA/workflow.
**Pré-condições:** checkout isolado; toolchain e deps disponíveis.
1. Leia `REL-019`, `REL-071` e o contrato operador.
2. Verifique commit SQLite antes de descartar resposta.
3. Rode `cargo test --locked -p corelink-server --lib routes::billing_ingest::tests_durable_classification::durable_classification_matrix -- --exact --nocapture`.
4. Exija erro, retry `Deduped` e uma linha vencedora.
5. Registre SHA, comando, status e output; não infira D1/runtime.
**Falhas e parada:** acesso remoto, assertion divergente ou fixture sem commit antes da perda.
**Recuperação:** preserve output, não settle, escale billing/consumer; não altere a coordenada.
**review_status:** `REVIEWED`.
**execution_status:** `REVIEWED_NOT_EXECUTED`.
**required_for_acceptance:** `true`.
**result:** teste lido em `91630ba`; execução pendente.
**review_evidence:** source do teste, workflow `issue-1635-durable-classification.yml`, `REL-019` e contrato operador.
**execution_evidence:** `none`.
**limitations:** fixture SQLite comprova somente comportamento local do adapter; workflow não foi observado em execução nem prova D1/runtime/consumer privado.
[Índice de procedimentos](#m02)

<a id="m04"></a>
## M04 — Matriz de testes e validação

| Mudança | package/target/features | Comando exato (planejado) | Predicado | Ambiente | PROC / execution_status |
|---|---|---|---|---|---|
| seleção | `corelink-server` / host / seleção do manifesto | `git rev-parse HEAD && git status --short; cargo metadata --locked --offline --no-deps --format-version=1` | SHA, package, bins e features concordam | checkout local sem rede | PROC-001 / `REVIEWED_NOT_EXECUTED` |
| boot/router | `corelink-server` / host / default | `cargo test --locked --offline -p corelink-server --test <selected_router_test>` | health e mounts esperados; rota ausente sem gate | env sintético, backend fake/stub | PROC-002 / `REVIEWED_NOT_EXECUTED` |
| storage | `corelink-server` / host / default | `cargo test --locked --offline -p corelink-server --test <selected_storage_test>` | fallback e gate coerentes; nenhum endpoint remoto | harness local sem credenciais | PROC-003 / `REVIEWED_NOT_EXECUTED` |
| cap Cargo | `corelink-server` / host / Cargo route | `cargo test --locked --offline -p corelink-server --test <selected_cargo_test>` | cap Worker encaminhado, resolver não chamado no PUT, ausência fail-closed | harness local sem rede | PROC-003 / `REVIEWED_NOT_EXECUTED` |
| billing staging | `corelink-server` / lib / teste de classificação durável | `cargo test --locked -p corelink-server --lib routes::billing_ingest::tests_durable_classification::durable_classification_matrix -- --exact --nocapture` | ACK descartado após commit produz erro incerto; retry `Deduped`; uma linha vencedora | SQLite loopback, sem D1/consumer remoto | PROC-006 / `REVIEWED_NOT_EXECUTED` |
| feature | `corelink-server` / host / `--no-default-features --features byok-aws-real` | `cargo check --locked --offline -p corelink-server --no-default-features --features byok-aws-real` | seleção única sem ambiguidade; sem chamada KMS | target local isolado | PROC-004 / `REVIEWED_NOT_EXECUTED` |
| remoto | `corelink-server` / target e feature do ticket | `<authorized_command_from_ticket> --dry-run` e depois comando aprovado | escopo, estado e recuperação conferidos pelo owner | ambiente remoto autorizado | PROC-005 / `BLOCKED_FOR_OPERATION` |

`<selected_router_test>` e `<selected_storage_test>` significam o nome de um target de
teste existente, obtido do manifesto e conferido antes do comando; não são evidência de que
um teste foi executado. O comando remoto só existe após ticket, owner, escopo e autorização.

Não execute all-features, ignored, live ou deploy indiscriminadamente.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Tipo | Reversível | Compatibilidade | Ação segura / compensação | Prova / parada |
|---|---|---|---|---|---|
| boot/router | código/config | normalmente sim | comparar binário N/N-1 antes do rollout | reverter somente commit próprio; roll-forward se dados já aceitos | diff e PROC-002; parar se o leitor N-1 divergir |
| mount e wire HTTP | contrato/wire | condicional | manter resposta e headers N/N-1 ou declarar quebra | fechar mount novo; compensar clientes somente com owner do contrato | fixtures/consumer; parar sem leitor N-1 |
| feature/binário | artefato | sim para código, não para efeitos | manter target/features N/N-1 quando prometidos | restaurar seleção/digest; roll-forward quando o artefato antigo não lê estado novo | digest, matriz e PROC-004 |
| R2/D1/Stripe/KMS/GC | dados externos | não só revert | preservar leitura N/N-1 e namespace | não apagar; owner escolhe compensação ou roll-forward e verifica estado remoto | estado durável e PROC-005; parar sem backup/owner |

Revert não recupera schema, objeto, cobrança, chave ou sweep aplicado; a recuperação de
dados e efeitos wire pertence ao composition root/owner que os criou.

<a id="m06"></a>
## M06 — Escalonamento e registro

| Condição | Rota | Evidência | Ação vedada |
|---|---|---|---|
| segredo/backend | storage/ops | nomes e erro redigidos | revelar valor |
| Stripe/cobrança | billing/financeiro | correlação sem PII | retry cego |
| KMS/BYOK | segurança | feature/target/erro | provider real sem autorização |
| GC/dados | GC owner | escopo/dry-run | sweep amplo |
| prova incompleta | consumer owner | SHA e lacuna | declarar produção |

Registre baseline, comandos, output, mounts e pendências; envie hashes para cold review.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
