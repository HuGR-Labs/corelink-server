---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-reapi-fuzz
manifest: crates/corelink-reapi/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: reapi-fuzz-w016-1177dad2
---

# corelink-reapi-fuzz — manual de manutenção

Esta autoria somente inspecionou arquivos e objetos Git locais. Nenhum comando Cargo/Rust, teste, build ou fuzz foi executado. Os procedimentos abaixo são candidatos locais; aprovação e execução permanecem estados separados.

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Matriz](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

**Fonte/base:** pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`; reconcilie divergência antes de executar. Esta autoria partiu de `8cdc02828132b9b6f03a3b57117b8140325f6762`.
**Ambiente:** checkout isolado; toolchain, nightly, cargo-fuzz 0.13.1 e dependências já disponíveis. Workflow declara 3600 s por target em runner macOS; não presuma essa capacidade local.
**Estado:** use cache/target isolados. Execução pode gravar corpus e artifacts; confira e preserve dados existentes. Não os limpe.
**Parada:** fonte divergente, alvo ausente, import sem dependência direta declarada, lock alterado, ferramenta/dependência ausente ou limite sem owner confirmado.

No pin atual, `audit_request_id_total.rs` importa `uuid::Uuid` sem `uuid` no manifesto: estado `SOURCE incompatível / build UNKNOWN`; não execute Cargo para contornar. Não baixe, instale, use credentials ou configure serviço.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Precisa de autorização extra? |
|---|---|---|---|---|
| Conferir pin e selecionar um target sem executar Rust | [PROC-001](#proc-001) | READ_ONLY | Leitura local de Git e manifestos | Não, dentro do checkout autorizado. |
| Fazer smoke fuzz local bounded | [PROC-002](#proc-002) | LOCAL_ISOLATED | CPU, build cache e corpus/artifacts no checkout isolado | Confirmar orçamento local de recurso e posse do checkout. |
| Preservar e triar um crash existente | [PROC-003](#proc-003) | LOCAL_ISOLATED | Cópia/inspeção de artifact local | Não enviar input/log a provider sem autorização. |

<a id="m03"></a>
## M03 — Procedimentos

<!-- PROC record metadata follows corelink-ownership-record/1.1; each remains blocked pending cold review and execution. -->

<a id="proc-001"></a>
### PROC-001 — Confirmar fonte, seleção e ambiente

**Gatilho / resultado:** antes de mexer em target ou fazer fuzz; pin, paths e isolamento identificados. **Modo / ambiente / permissão:** READ_ONLY; checkout já autorizado; sem Cargo/Rust, rede ou provider. **Entradas e pré-condições:** pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`; lista exata do R01; árvore do checkout. 1. Leia `git rev-parse HEAD`, `git status --short`, manifesto e os três `path` dos bins. 2. Compare o pin dos docs aos arquivos efetivos e confira se alvo

escolhido está em `[[bin]]`. 3. Compare imports de crates nos targets com `[dependencies]`; registre `uuid::Uuid` como import não declarado no pin atual. 4. Leia a invocação CI pertinente; ausência de run verificável fica UNKNOWN. **Saída:** baseline, diff, target, import/dependency audit e diretórios corpus/artifacts existentes registrados. **Falhas e parada:** pin divergente não reconciliado, árvore alheia suja, target/path ausente ou import sem dependência direta declarada. **Recuperação:** não faça reset/checkout de branch e

não limpe arquivos; encaminhe a divergência para decisão do responsável verificado. **Registro PROC-001:** `mode=READ_ONLY`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=inspeção estática local`; `result=SOURCE_INCOMPATIBLE`; `review_evidence=fontes no pin e comparação import/dependency em R08`; `execution_evidence=[]`; `limitations=uuid::Uuid não declarado no manifesto; sem validação independente e sem execução Rust`. [Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Executar smoke local bounded de um target

**Gatilho / resultado:** alteração de harness/dependência ou diagnóstico; registrar crash, timeout e artifacts. **Modo / ambiente / permissão:** LOCAL_ISOLATED; worktree/cache exclusivos; nightly e cargo-fuzz pré-existentes; sem provider. **Entradas:** `TARGET` deve ser um dos três bins do manifesto; a execução parte de `crates/corelink-reapi` e aponta ao subpackage `fuzz/`. **Pré-condições:** PROC-001 sem incompatibilidade de imports, orçamento aprovado, ferramentas disponíveis e corpus anotado. No pin atual, audit falha essa pré-condição. 1. No diretório

`crates/corelink-reapi`, defina `CARGO_HOME` e `CARGO_TARGET_DIR` em diretórios temporários exclusivos e já existentes; mantenha o corpus isolado do operador. 2. Execute o comando exato escolhido em [M04](#m04), com `-- -max_total_time=60` para o smoke local. 3. Preserve saída e exit status, target, pin, duração e hashes de artifacts/corpus novos; não apague resultados anteriores. **Parada:** qualquer crash, sanitizer finding, timeout, lockfile modificado, tentativa de buscar dependência ou limite excedido. **Recuperação:** congele input/log; classifique

como finding/ambiente sem rerun seletivo para obter verde; reverta só alteração própria quando necessário e mantenha corpus reproduzível. **Registro PROC-002:** `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=nenhum ambiente de fuzz utilizado nesta autoria`; `result=NOT_EXECUTED`; `review_evidence=manifesto e targets inspecionados no pin`; `execution_evidence=[]`; `limitations=smoke 60s não iguala matrix nightly configurada em 3600s`. [Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Preservar e reproduzir um crash input

**Gatilho / resultado:** target reporta crash, timeout ou sanitizer finding; caso e ambiente preservados para análise, sem declarar causa antes de reproduzir. **Modo / ambiente / permissão:** LOCAL_ISOLATED; cópia em workspace descartável autorizado; sem dados reais ou envio externo. **Entradas e pré-condições:** artefato já existente com target e pin identificados; PROC-001 confirmado; preservar hash do original. 1. Copie o input para diretório exclusivo sem alterar o original e registre SHA-256,

tamanho e origem. 2. Use o procedimento cargo-fuzz de reprodução do mesmo target, pin, toolchain e argumentos do run de origem; registre comando literal e resultado, ou pare se a ferramenta/CLI não estiver confirmada. 3. Reduza/minimize somente uma cópia, conservando input original e relação entre hashes; compare contra a versão anterior apenas em ambiente isolado. **Parada:** input contém dado sensível, origem desconhecida, mudança de ambiente não explicada ou efeito externo.

**Recuperação:** remova apenas temporários criados nesta tarefa depois de preservar evidência aprovada; não apague o corpus compartilhado. Se não reproduzir, reporte “não reproduzido” e ambiente exato. **Registro PROC-003:** `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=nenhum crash triado nesta autoria`; `result=NOT_EXECUTED`; `review_evidence=fontes estáticas do harness e workflow`; `execution_evidence=[]`; `limitations=sem crash input, sem reprodução e sem autorização para provider`. [Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Mudança | Package / target / features | Comando ou PROC | Predicado exigido | Estado nesta autoria |
|---|---|---|---|---|
| Compatibilidade manifesto/imports | `corelink-reapi-fuzz` / todos os bins / features próprias vazias | PROC-001; comparação estática, sem Cargo | Cada `use <crate>` externo tem dependência direta ou é explicitamente reconciliado; `uuid::Uuid` atualmente não tem declaração correspondente. | **SOURCE incompatível / audit build UNKNOWN; não executar esse target até decisão separada.** |
| Decoder/schema REAPI | `corelink-reapi-fuzz` / `proto_decode_batch_update` / features próprias vazias; defaults do pai não resolvidos | `cd crates/corelink-reapi && cargo fuzz run proto_decode_batch_update -- -max_total_time=60` via PROC-002 | Sem crash/timeout no limite; isso não prova domínio de protocolo nem handler. | Não executado. |
| Helper de request-id | `corelink-reapi-fuzz` / `audit_request_id_total` / idem | Após reconciliar `uuid::Uuid`/manifesto, `cd crates/corelink-reapi && cargo fuzz run audit_request_id_total -- -max_total_time=60`; depois teste pai | Smoke completa; suite pai verifica predicados de idempotência escolhidos. | **Bloqueado por incompatibilidade SOURCE; não executar nesta revisão.** |
| Parser Read | `corelink-reapi-fuzz` / `parse_read_resource_name` / idem | `cd crates/corelink-reapi && cargo fuzz run parse_read_resource_name -- -max_total_time=60` via PROC-002; `cargo test --locked --offline -p corelink-reapi --lib parse_read_resource_name` | Smoke completa; testes parser cobrem casos unitários existentes. | Não executado. |
| Mudança em workflow ou novo target | package/target/ferramenta exatamente como manifest e matrix atualizados | PROC-001; validar target listado e command/limite em revisão de CI separada | Seleção é explícita; execução real requer evidência do run. | Não executado/revisado externamente. |

**Gates do pai:** workflows inspecionados configuram testes do package `corelink-reapi`; isso não certifica os bins do package excluído de fuzz. O workflow nightly configura 3600 s para proto e audit; o smoke local de 60 s é menor. `parse_read_resource_name` não foi encontrado na matrix consultada.

O import `uuid::Uuid` sem declaração direta mantém o audit target em `SOURCE incompatível / build UNKNOWN`; o build package-wide também não foi certificado. Não executar Cargo para contornar. Não executar `--all-features`, suites live, workflow, deploy ou fuzz sem escopo e recursos aprovados.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| Código de harness | Sim por mudança própria revisada. | Não altera armazenamento de aplicação. | Reverta apenas o commit próprio ou corrija a closure; preserve finding reproduzível. | Diff e reexecução do target pertinente, depois de autorizada. |
| Corpus local | Condicional. | Corpus pode conter entradas novas/valiosas; ownership/persistência compartilhada não está definida. | Preserve paths/hashes e não sobrescreva/remova corpus preexistente. | Comparação de inventário antes/depois e reproduções. |
| `fuzz/artifacts/` / runner artifact | Condicional à retenção e ao provider. | Workflow declara upload on failure e retenção de 14 dias no nightly; não observada. | Capture artifact por mecanismo autorizado; não presuma disponibilidade permanente. | Identificador/hash do run e arquivo relido, se autorizado. |
| Target name / matrix | Condicional. | Renomear bin pode deixar entrada CI inválida; matrix seleciona 2 de 3 nomes. | Atualize consumidores coordenadamente após localizar owner do workflow. | Diff manifesto/matrix e run do target; não disponível nesta autoria. |
| Schema/API no pai | Recuperar código não restaura artefato/runtime anterior. | Pode quebrar build ou alterar interpretação do input; nenhuma persistência do harness é definida. | Coordene rollback do código e compatibilidade do pai; mantenha corpus como regressão sintética se aprovada. | Compilação/teste/fuzz nos pins anterior e novo. |

**Sem estado de aplicação próprio:** fontes do harness não chamam banco, bucket ou serviço; ainda assim, o pai e as dependências precisam de inspeção em mudanças que alterem esses limites. Nenhuma migração de produção pertence a este package.
**Limites:** reverter código não recupera corpus/artifacts removidos nem cancela jobs; ambos dependem do local/provider de execução, que não foi operado nem consultado.

<a id="m06"></a>
## M06 — Escalonamento e registro

| Condição | Responsável / rota verificada | Evidência mínima | Ação vedada |
|---|---|---|---|
| Contrato pai mudou ou crash aponta para helper/proto | Owner humano não identificado nos arquivos estudados; identificar rota existente antes de delegar. | API/REL, pin, target, input sintético e log sem dados sensíveis. | Atribuir owner inventado ou concluir impacto produtivo a partir do fuzz. |
| Ambiente/cache/toolchain falha | Owner da operação desconhecido; registrar bloqueio e seleção. | Comando, versões, ambiente, saída e lock diff. | Instalar, baixar dependências, editar lock ou rodar em runner compartilhado sem autorização. |
| Resultado de CI solicitado | Status do provider não consultado; obter evidência por canal autorizado. | ID e target do run, SHA de source/workflow e artifact quando aplicável. | Declarar PASS por causa de manifesto ou YAML. |
| Docs prontos | Cold reviewer independente ainda não atribuído. | Quatro hashes finais e fonte pinada. | Autoaprovar ou tratar checker editorial como review. |

Depois de execução autorizada, registre baseline, saída, status e hashes; preserve corpus valioso, limpe só temporários criados pela tarefa e encaminhe revisão dos docs alterados. Não inclua credenciais nem dados de clientes.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
