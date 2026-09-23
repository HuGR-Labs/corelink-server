---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-meta-fuzz
manifest: crates/corelink-meta/fuzz/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: "S"
state: draft
evidence_set: corelink-meta-fuzz-structural-normalization-20260921
---

# corelink-meta-fuzz — manual de manutenção

[Preparação](#m01) · [Escolher procedimento](#m02) · [Procedimentos](#m03) ·
[Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003)

**Baseline:** source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; independente em `crates/corelink-meta/fuzz/Cargo.toml` com targets `commit_put_roundtrip` e `audit_idempotency`. Antes da revisão, confira HEAD, diff e manifesto. O workflow declara cargo-fuzz/nightly e limites de 60s (PR) e 3600s (matriz raiz); execução e corpus continuam desconhecidos. Use caches Cargo temporários em diagnóstico autorizado.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Precisa de autorização extra? |
|---|---|---|---|---|
| Conferir manifesto, deps, features, schema e wiring | [PROC-003](#proc-003) | READ_ONLY | Ler fonte/metadata e gravar apenas saída em diretório temporário | Não; não instalar, buildar ou iniciar workflow |
| Alteração do input/oráculo commit_put | [PROC-001](#proc-001) | LOCAL_ISOLATED | Executar apenas o target em ambiente descartável | Não para execução local isolada; runner compartilhado exige owner |
| Alteração do retry/payload audit | [PROC-002](#proc-002) | LOCAL_ISOLATED | Executar apenas o target em ambiente descartável | Não para execução local isolada; runner compartilhado exige owner |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Smoke de commit_put_roundtrip

**Gatilho / resultado esperado:** mudou target ou semântica exercitada; run termina sem finding e preserva log.
**Modo / ambiente / permissão:** LOCAL_ISOLATED, checkout no source commit indicado, cargo-fuzz 0.13.1 e nightly disponíveis; sem production credentials.
**Entradas e validação:** diretório de trabalho crates/corelink-meta, conforme workflow; target confirmado no manifesto.
**Pré-condições:** confira status e salve corpus/artifacts preexistentes; use target/cache isolados.

1. Execute o comando PR declarado: cargo fuzz run commit_put_roundtrip -- -max_total_time=60.
2. Guarde stdout, stderr e exit code; se houver finding, preserve o artifact antes de triagem.

**Falhas e parada:** finding, erro de build, hang ou artefato ausente após falha bloqueiam; não declare PASS.
**Recuperação:** o fake não persiste dados. Preserve input reproduzível e limpe somente arquivos comprovadamente criados nesta execução.

**Estado por PROC:** review_status=BLOCKED; execution_status=REVIEWED_NOT_EXECUTED; mode=LOCAL_ISOLATED; result=NOT_EXECUTED; required_for_acceptance=true.

**Ambiente / evidência:** checkout local no commit fixado; evidência de revisão é manifesto/target/workflow. Evidência de execução: nenhuma.

**Limitações:** campanha não executada; sem corpus/run record e sem cobertura D1.

**Evidência:** workflow corelink-meta.yml declara a forma do comando; não foi executado nesta autoria.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Smoke de audit_idempotency

**Gatilho / resultado esperado:** mudou target/oráculo de conflito; run termina sem finding e preserva log.
**Modo / ambiente / permissão:** LOCAL_ISOLATED, checkout pinado, nightly e cargo-fuzz 0.13.1; sem dados persistentes.
**Entradas e validação:** diretório crates/corelink-meta; confirme nome declarado audit_idempotency.
**Pré-condições:** registre status e proteja corpus/artifacts existentes.

1. Execute o comando PR declarado: cargo fuzz run audit_idempotency -- -max_total_time=60.
2. Guarde stdout, stderr e exit code; em finding, mantenha o artifact e o input exato.

**Falhas e parada:** conflito inesperado, outro erro, hang ou finding não resolvido bloqueia o aceite.
**Recuperação:** preserve o reproducer; descarte apenas saídas novas de uma execução isolada após conferir o diff.

**Estado por PROC:** review_status=BLOCKED; execution_status=REVIEWED_NOT_EXECUTED; mode=LOCAL_ISOLATED; result=NOT_EXECUTED; required_for_acceptance=true.

**Ambiente / evidência:** checkout local no commit fixado; evidência de revisão é manifesto/target/workflow. Evidência de execução: nenhuma.

**Limitações:** campanha não executada; sem corpus/run record e sem cobertura D1.

**Evidência:** workflow corelink-meta.yml declara a forma do comando; não foi executado nesta autoria.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Censo READ_ONLY de manifesto, deps, workflow e schema

**Gatilho / resultado esperado:** mudou manifesto, dependência, feature, schema ou wiring; produzir metadata e inventário reconciliável sem build ou fuzz.
**Modo / ambiente / permissão:** READ_ONLY; checkout pinado; apenas leitura do repositório; `CARGO_HOME` e `CARGO_TARGET_DIR` em diretórios temporários task-owned.
**Pré-condições:** confirme source pin, manifesto, workflows e diretório temporário; não use credenciais ou cache compartilhado.

1. Crie diretório temporário e exporte `CARGO_HOME="$tmp/cargo-home"` e `CARGO_TARGET_DIR="$tmp/target"`; não grave no checkout.
2. Execute `cargo metadata --manifest-path crates/corelink-meta/fuzz/Cargo.toml --locked --offline --format-version 1` e guarde saída temporária; se offline falhar, pare e registre BLOCKED.
3. Leia manifesto, deps/features, `corelink-meta.yml`, `nightly.yml` e `migrations/d1` com `rg`; confira targets, PR 60s, schedule comentado e matriz raiz 3600s.
4. Compare fatos com REL-001/002/006/007/008, API/INV e checker; remova somente o diretório temporário criado.

**Predicado:** metadata/fonte e relações reconciliam; nenhum build, fuzz, migration, provider ou workflow iniciou. **Falha/parada:** pin divergente, cache compartilhado ou schedule conflitante bloqueia; não faça correção funcional.
**Estado por PROC:** review_status=REVIEWED_NOT_EXECUTED; execution_status=NOT_EXECUTED; mode=READ_ONLY; result=NOT_EXECUTED; required_for_acceptance=true.
**Evidência:** comando, pin, metadata, busca de wiring, diff de relações e estado temporário; ausência de run não é PASS operacional.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Tipo de mudança | Package / target / features | Comando ou PROC | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| Input/oracle do round-trip | corelink-meta-fuzz / commit_put_roundtrip / sem feature declarada | PROC-001; cargo fuzz run commit_put_roundtrip -- -max_total_time=60 | Sem crash/finding no orçamento; estado assertado conforme API-001 | LOCAL_ISOLATED; não executado nesta autoria |
| Retry/idempotência | corelink-meta-fuzz / audit_idempotency / sem feature declarada | PROC-002; cargo fuzz run audit_idempotency -- -max_total_time=60 | Mesmo payload aceita; payload divergente dá conflito esperado | LOCAL_ISOLATED; não executado nesta autoria |
| API do fake usada pelos dois targets | corelink-meta-fuzz / ambos os bins / sem feature declarada | PROC-001 e PROC-002 | Ambos preservam seus oráculos; isso não certifica adapter D1 | LOCAL_ISOLATED; execução futura |
| Manifesto/deps/features/schema/wiring | manifest, providers, `migrations/d1`, `corelink-meta.yml`, `nightly.yml` | PROC-003 + checker v1.3 + `git diff --check` | Metadata, anchors, RELs, targets e triggers reconciliam; checker passa; nenhum schedule duplicado é afirmado | READ_ONLY; sem Cargo build, fuzz ou workflow |

**Negativos obrigatórios:** input curto retorna; size zero não é exercitado pelo target round-trip; erro de commit_put no primeiro target retorna sem classificar; ambos usam fake. Trate essas lacunas como limites, não cobertura.

**Gates globais compartilhados:** PR smoke ativo, nightly per-crate dormente e matriz raiz ativa no commit fixado; status real e enforcement de merge não coletados.

**Não executar indiscriminadamente:** campanhas de 3600s, todos os targets, suite de produção ou deploy.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| Código do harness | Sim, em Git | Só muda código/configuração do package | Reverter patch após salvar o finding e restaurar target/oráculo | Diff e source pin rechecados; repetir PROC aplicável |
| Estado do fake | Sim ao encerrar processo | BTreeMap em memória criado por callback | Encerrar o processo; não é necessário rollback de D1 | Nenhuma row/outbox persistente foi chamada pelo target |
| Corpus/artifacts | Condicional | Execução pode produzir corpus ou crash artifacts; não há corpus rastreado observado no pin | Preservar reproducer; identificar arquivos novos antes de limpar | git status/diff e input do finding continuam disponíveis |
| Workflow/cache/artifact remoto | Não por Git revert | Per-crate schedule está dormente; matriz raiz ativa pode usar runner e fazer upload em falha por 14 dias; cache/corpus não foi observado | Não limpar runner/cache nem apagar artifact sem operador autorizado | Conferir run/artifact no provedor; isso não foi observado |
| Schema/dados D1 ou R2 | Fora deste package | Targets não chamam esses sistemas | Encaminhar ao owner do adapter/serviço; usar o runbook daquele owner | Nenhuma recuperação produtiva é definida por este manual |

**Sem estado próprio:** o package e o fake não mantêm estado durável.
**Limites conhecidos:** reverter harness não recupera runner, cache ou artifact remoto; e não reverte mudança feita por outra crate em storage.

<a id="m06"></a>
## M06 — Escalonamento e registro de manutenção

| Condição | Responsável / rota verificada | Evidência mínima | Ação vedada |
|---|---|---|---|
| Finding no target | Reviewer de código independente ainda não designado; CODEOWNERS catch-all solicita review a @gmhelmold | Commit, target, command, exit code e artifact | Apagar input ou chamar finding de bug de produção sem triagem |
| Problema de MetaStore/D1 | Package/provider corelink-meta; rota humana não verificada | API/INV e reprodução limitada ao fake | Alterar migration/adapter a partir deste manual |
| Runner/cache/artifact remoto | Operador do workflow não identificado | Workflow e URL/run ID obtidos do canal autorizado | Modificar runner compartilhado ou excluir artifact |

CODEOWNERS no commit fonte declara que a regra solicita review e não é merge gate; não presume reviewer independente. Registre baseline, resultado real e hashes; encaminhe os quatro documentos para revisão fria sem preencher aprovação em nome do reviewer.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
