---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-hash
manifest: crates/corelink-hash/Cargo.toml
source_commit: cacc44fc43ee2f481e933419ba88b9a19ac6c8e8
profile: H
state: draft
evidence_set: hash-pilot-source-20260919
---

# corelink-hash — manual de manutenção

Procedimentos candidatos para checkout isolado. Há evidência histórica delimitada em baseline anterior; ela não é promovida para `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8` sem nova execução.

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Use somente checkout isolado e limpo na baseline `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`. Não reutilize resultados executados na baseline anterior como prova deste pin.
Não resete a branch ativa do operador. Não carregue .env.local nem segredos para testar esta library.
Toolchain deve corresponder a rust-toolchain.toml; obtenha HOST do rustc efetivamente utilizado.
Defina diretório de build próprio, até quatro jobs e preserve logs fora do checkout.
Falha de cache offline é bloqueio de ambiente; não atualizar Cargo.lock para obter verde.

Fontes dos testes/gates: [S07](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/blob_store_contract.rs) [S08](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/mutation_kills.rs) [S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs) [S12](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L1-L155).

<a id="m02"></a>
## M02 — Escolher procedimento

| Situação | Procedimento | Modo | Saída |
|---|---|---|---|
| Verificar baseline e preparar diretório isolado | [PROC-001](#proc-001) | LOCAL_ISOLATED | Evidência delimitada; sem produção |
| Executar contratos e regressões locais | [PROC-002](#proc-002) | LOCAL_ISOLATED | Evidência delimitada; sem produção |
| Planejar mudança de API ou consumer | [PROC-003](#proc-003) | LOCAL_ISOLATED | Evidência delimitada; sem produção |
| Validar tempo e desempenho em release | [PROC-004](#proc-004) | LOCAL_ISOLATED | Evidência delimitada; sem produção |
| Verificar compatibilidade wasm e lint | [PROC-005](#proc-005) | LOCAL_ISOLATED | Evidência delimitada; sem produção |
| Recuperar mudança que afete bytes persistidos | [PROC-006](#proc-006) | LOCAL_ISOLATED | Evidência delimitada; sem produção |

<a id="m03"></a>
## M03 — Procedimentos

Cold review continua pendente. PROC-002 teve execução local source-equivalent
em 2026-09-22 (29 pass, 2 release-only ignorados, 0 fail), registrada em
[`HASH-CARGO-TEST-20260922.md`](../../evidence/revision-1.4/HASH-CARGO-TEST-20260922.md).
Como o manifesto do pin atual só difere no URL do repositório, isso reduz a
incerteza de contratos, mas não certifica o pin exato. PROC-004 e PROC-005
continuam `REVIEWED_NOT_EXECUTED`; PROC-001/003/006 não estão certificados
integralmente. Os seis continuam necessários para certificar este pacote.

<a id="proc-001"></a>
### PROC-001 — Verificar baseline e preparar diretório isolado
**Gatilho/resultado:** antes de qualquer tarefa; contexto e diretório de build identificados.  
**Ambiente/permissão:** clone/worktree já criado e autorizado; sem reset ou deploy.  
**Entradas:** SHA desta edição; ferramenta rustc/cargo correspondente ao arquivo do repo.
1. Execute o bloco no checkout; qualquer comando com erro interrompe o procedimento.
```sh
set -eu
EXPECTED=cacc44fc43ee2f481e933419ba88b9a19ac6c8e8
test "$(git rev-parse HEAD)" = "$EXPECTED"
test -z "$(git status --porcelain)"
rustc -Vv
cargo -V
HOST=$(rustc -vV | sed -n 's/^host: //p')
test -n "$HOST"
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/corelink-hash-owner.XXXXXX")
export HOST SCRATCH CARGO_BUILD_JOBS=4
export CARGO_TARGET_DIR="$SCRATCH/target"
```
2. Confira versões com rust-toolchain.toml; carregue OKF por `python3 scripts/okf_context.py --file crates/corelink-hash/src/digest.rs --full`.
**Parada:** branch divergente, árvore suja ou versão não confirmada.  
**Recuperação:** não editar o checkout; reter/identificar somente o SCRATCH desta execução.  
**Certificação:** pré-condições verificadas no checkout limpo; carregamento OKF não executado; required_for_acceptance=true.  
**Registro:** comando/ambiente/output/SHA/data reais, nunca substituídos por este texto.  
[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Executar contratos e regressões locais
**Gatilho/resultado:** alteração de parser, envelope ou erro; suites específicas verdes, sem testes ocultados.  
**Pré-condições:** PROC-001; dependências já disponíveis; nenhum serviço real configurado.
1. Rode cada comando, preservando saída e exit status; não use pipeline que esconda falha.
```sh
cargo test --locked --offline -p corelink-hash --target "$HOST" --test prop_hash
cargo test --locked --offline -p corelink-hash --target "$HOST" --test mutation_kills
cargo test --locked --offline -p corelink-hash --target "$HOST" --test blob_store_contract
cargo test --locked --offline -p corelink-hash --target "$HOST" --doc
```
2. Confira testes selecionados e ignorados: timing/perf ficam para PROC-004. 3. Reconfira `git status --porcelain`; preserve eventuais arquivos de regressão, não os apague para obter limpeza. **Parada:** erro, suite vazia, lockfile modificado ou requisito ignorado inesperadamente. **Recuperação:** reverter somente a mudança própria por commit; corrigir causa sem relaxar assertivas. **Execução:** `EXECUTED_LOCAL/PASS` em checkout source-equivalent em 2026-09-22: 29 passaram, dois release-only ficaram ignorados e o doctest passou; evidência no arquivo de revisão. **Certificação:** não certifica

consumers nem produção; required_for_acceptance=true. **Registro:** comando/ambiente/output/SHA/data reais, nunca substituídos por este texto. [Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Planejar mudança de API ou consumer
**Gatilho/resultado:** mudar assinatura, parser, representação, constante ou reexport; conjunto de afetados explícito.  
**Pré-condições:** PROC-001; B06 conciliado antes de declarar cobertura final.
1. Registre APIs/RELs e consumidores conhecidos; escolha artifact package, target e features reais.
2. Consulte inversas declaradas separadas de runtime:
```sh
cargo tree --locked --offline --workspace --invert corelink-hash --target "$HOST" --edges normal,build
```
3. Revise aliases crypto/client-verify e relações por dados; inclua dev edges numa consulta separada quando a mudança afeta testes. 4. Execute PROC-002, checks dos consumers e testes de compatibilidade escolhidos. **Parada:** grafo parcial, consumer desconhecido ou mudança de dados sem recuperação. **Recuperação:** ajuste contrato e versão de forma coordenada; não publicar resultado só porque hash compila. **Evidência exigida:** comando completo, seleção, consumers e motivo de cada gate. **Certificação:** inversa Cargo

e aliases foram inspecionados, mas B06 não está reconciliada; required_for_acceptance=true. **Registro:** comando/ambiente/output/SHA/data reais, nunca substituídos por este texto. [Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Validar tempo e desempenho em release
**Gatilho/resultado:** alterar hashing/comparação ou dependências; gates temporais específicos executados.  
**Pré-condições:** PROC-001/002; host sem carga concorrente relevante; registrar CPU/toolchain/carga.
1. Execute os dois testes, sem reduzir limiares:
```sh
cargo test --locked --offline --release -p corelink-hash --target "$HOST" --test prop_hash constant_time_variance -- --exact --nocapture
cargo test --locked --offline --release -p corelink-hash --target "$HOST" --test prop_hash perf_regression_5mib_under_50ms -- --exact --nocapture
```
2. Registre medianas/delta e duração total, inclusive resultados vermelhos. **Predicados existentes:** delta relativo de medianas <5%; dez hashes de 5 MiB em <50 ms após warmup. **Parada:** ruído não caracterizado ou falha; não repetir seletivamente até passar. **Recuperação:** diagnosticar fonte versus ambiente, reaplicar baseline como controle. **Limite:** não certifica toda a requisição, criptografia universal ou p99 em wasm. **Execução atual:** no checkout source-equivalent de 2026-09-22, CT passou com delta `0.019%` e perf passou em `21.792027 ms` para 10 × 5 MiB; evidência em [`HASH-RELEASE-GATES-20260922.md`](../../evidence/revision-1.4/HASH-RELEASE-GATES-20260922.md). O resultado histórico vermelho (78,16 ms) permanece registrado e deve ser considerado na análise de ambiente. **Certificação:** resultado local atual PASS, mas não certifica produção/wasm nem substitui cold review; required_for_acceptance=true. **Registro:** comando/ambiente/output/SHA/data reais, nunca substituídos por este texto. [Índice de procedimentos](#m02)

<a id="proc-005"></a>

### PROC-005 — Verificar compatibilidade wasm e lint
**Gatilho/resultado:** mudança de source/dependência; compilar alvo wasm e lintar seleção nativa.  
**Pré-condições:** PROC-001/002; target wasm já instalado na toolchain aprovada.
1. Execute:
```sh
cargo check --locked --offline -p corelink-hash --target wasm32-unknown-unknown
cargo clippy --locked --offline -p corelink-hash --target "$HOST" --all-targets -- -D warnings
```
2. Registre seleção e warnings; conferir também consumers wasm materialmente afetados pelo PROC-003.
**Parada:** target ausente, dependência incompatível ou lock modificado; não instalar/update silenciosamente.  
**Recuperação:** corrigir causa no escopo da alteração; manter a fronteira do alvo explícita.  
**Limite:** cargo check não executa Cloudflare; este procedimento não faz deploy.  
**Execução:** `EXECUTED_LOCAL/PASS` em 2026-09-22 no checkout source-equivalent:
`cargo clippy --locked --offline -p corelink-hash -- -D warnings` e
`cargo check --locked --offline -p corelink-hash --target wasm32-unknown-unknown`;
ver [`HASH-LOCAL-QUALITY-20260922.md`](../../evidence/revision-1.4/HASH-LOCAL-QUALITY-20260922.md).
Isso não prova bindings Cloudflare, deploy ou runtime.
**Certificação:** não comprova deploy Cloudflare ou consumers wasm; required_for_acceptance=true.  
**Registro:** comando/ambiente/output/SHA/data reais, nunca substituídos por este texto.  
[Índice de procedimentos](#m02)

<a id="proc-006"></a>

### PROC-006 — Recuperar mudança que afete bytes persistidos
**Gatilho/resultado:** algoritmo, largura, case, as_bytes ou writer mudou; compatibilidade demonstrada antes de release.  
**Pré-condições:** REL-014/015 identificadas; separar a API dos bytes de `Digest` (implementada por `corelink-hash`) do framing do envelope persistido (definido pelo package `corelink-server`/adapter); registrar a coordenação dos dois owners. Aprovador formal entre packages não está identificado. Nenhuma operação produtiva.
1. Guarde fixtures sintéticas da versão anterior: chave, payload, envelope e identidade da versão.
2. Teste leitura pela versão nova e pela antiga quando essa compatibilidade for prometida.
3. Exercite estados Verified/Legacy/Corrupt sem modificar dados reais; compare accounting esperado do consumer.
4. Defina rollback de binário versus roll-forward de dados; registre ponto irreversível.
**Parada:** ausência de leitor N/N-1, formato ambíguo, falta de coordenação entre os owners do tipo e do envelope ou tentativa de atribuir aprovação sem autoridade verificada.
**Recuperação:** não presumir que git revert desfaz objetos gravados; encaminhar migração/compatibilidade do envelope ao owner `corelink-server`/adapter e manter coordenada qualquer mudança no contrato de `Digest`.
**Evidência exigida:** fixtures, testes reais escolhidos, estados finais e decisão aprovada. Não há comando único seguro de migração nesta library.  
**Certificação:** autoria conferida; execução não realizada; required_for_acceptance=true.  
**Registro:** comando/ambiente/output/SHA/data reais, nunca substituídos por este texto.  
[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Mudança | Seleção | Procedimento | Resultado exigido | Estado atual |
|---|---|---|---|---|
| Parser/formatação | hash / HOST / default | PROC-002 | Vetores, uppercase, length/offsets, roundtrip | `EXECUTED_LOCAL` histórico na baseline anterior; não executado em `cacc44` |
| Envelope/erros | hash / HOST / default | PROC-002 | Match/mismatch/empty/redaction; fake writer | `EXECUTED_LOCAL` histórico; não executado em `cacc44` |
| Hash/ct_eq | hash / HOST / release | PROC-004 | Gates e controle de carga preservados | `EXECUTED_LOCAL` histórico: CT passou; perf falhou; PROC-004 bloqueado; não executado em `cacc44` |
| Dependência/feature | hash + affected consumers / alvo explícito | PROC-003/005 | Clippy, compile e contrato por consumer | `EXECUTED_LOCAL` histórico: clippy/wasm passaram; consumers incompletos; não executado em `cacc44` |
| Largura/as_bytes/chave | versões N/N-1, fixtures locais | PROC-006 | Estado persistido compatível | Não executado |
| Limite HTTP | quatro consumers Bazel + adjacentes | PROC-003 | Payload abaixo/no/acima e budget | Não executado |

Não adicionar deploy ou publish como teste documental. O manifesto permite publicação, mas isso requer release autorizado.
Gates globais seguem CLAUDE.md e pre-merge-gate-check.sh; não repetir toda a suíte global por crate.
Não executar todas as features, todos os ignored ou testes live indiscriminadamente.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversibilidade | Risco | Ação | Prova |
|---|---|---|---|---|
| Código ainda local | Normalmente reversível | Apagar trabalho de outra tarefa | Reverter só commit próprio | Diff/árvore/PROC-002 |
| Chave de conteúdo | Condicional | Algoritmo/case gera outro endereço | Plano de namespace no worker | Leitura de fixture anterior |
| Envelope persistido | Condicional | Digest width/algoritmo altera decode | Plano no server | Verified/Legacy/Corrupt |
| Admissão HTTP | Configuração/binário | Objetos já aceitos não somem | Recuperar limite e validar dados | Testes de rota e leitura |
| IO em andamento | Não presumir | Erro pode coexistir com efeito | Conferir adapter e ownership | Estado durável, não só status |

A library não possui banco, bucket, clock, migration ou transação próprios. A recuperação de efeitos externos é do consumer que os criou.

<a id="m06"></a>
## M06 — Escalonamento e registro

| Condição | Rota | Evidência | Não fazer |
|---|---|---|---|
| Política de compatibilidade indecisa | Issue/PR no repo; owner do consumer | API/REL e fixture sintética | Escolher política financeira/storage por conveniência |
| Ambiente incapaz de executar | Integração CO-COMMON | Comando, erro, target e baseline | Registrar PASS |
| Documento contradiz código | Owner do conceito/contrato na issue | Ambos os trechos e escopo | Copiar promessa não demonstrada |
| Mudança pronta para revisão | Cold reviewer independente, ainda não atribuído | Hashes finais e fontes | Autoaprovar |

Após execução, reconciliar documentação/backlog e limpar somente temporários da tarefa após preservar evidências.
Não registrar dados de clientes, segredos ou chaves R2 privadas em logs.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
