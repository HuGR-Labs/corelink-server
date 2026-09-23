---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-audit-chain-fuzz
manifest: crates/corelink-audit-chain/fuzz/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: "S"
state: draft
evidence_set: corelink-audit-chain-fuzz-structural-normalization-20260921
---

# corelink-audit-chain-fuzz — manual de manutenção

[Preparação](#m01) · [Escolher procedimento](#m02) · [Procedimentos](#m03) ·
[Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003)

**Checkout / toolchain / target:** package source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`, matching this artifact's front matter; pacote independente em `crates/corelink-audit-chain/fuzz/`; dois targets sem features declaradas. O invocador documenta nightly e `cargo-fuzz 0.13.1`, mas sua disponibilidade local não foi verificada.

**Baseline / pré-condições:** confirme diff, manifesto, target afetado e limites de [R04](REFERENCE.md#r04); pare com fonte divergente ou consumer/owner desconhecido.

**Dependências / fixtures:** entradas são bytes gerados pelo fuzzer; corpus atual é desconhecido. Não use dados de produção.

**Recursos:** selecione duração explícita; o script usa 300s e o workflow 1800s. Rode em checkout descartável, com `CARGO_HOME` e `CARGO_TARGET_DIR` próprios; não altere toolchain compartilhado. A execução pode alterar corpus/artifacts.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Autorização extra? |
|---|---|---|---|---|
| Validar mudança de um harness em ambiente local isolado | [PROC-001](#proc-001) | `LOCAL_ISOLATED` | Executar um target por duração limitada e preservar saída | Sim, se host/toolchain não for descartável |
| Preservar entrada/artifact de crash antes de retomar trabalho | [PROC-002](#proc-002) | `LOCAL_ISOLATED` | Copiar evidência local e registrar hashes | Sim, para artefato fora do checkout autorizado |
| Inspecionar manifesto, dependências, features e wiring sem executar Cargo | [PROC-003](#proc-003) | `READ_ONLY` | Comparar declarações e seletores estáticos | Não; parar se exigir resolução/runner |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Fuzz direcionado com limite de tempo

**Gatilho / resultado esperado:** mudança no harness ou dependência; target selecionado termina no tempo escolhido sem finding novo.

**Modo / ambiente / permissão:** `LOCAL_ISOLATED`; checkout descartável, nightly existente e cargo-fuzz 0.13.1; sem CI/provider.

**Entradas e validação:** target somente `merkle_append` ou `jcs_canonicalize`; confirme pin e árvore do checkout antes de começar.

**Pré-condições:** preserve corpus existente; use `CARGO_HOME` e `CARGO_TARGET_DIR` temporários; confirme armazenamento suficiente.

1. Entre em `crates/corelink-audit-chain` no checkout descartável.
2. Rode somente o comando do target afetado em [M04](#m04), com `-max_total_time=60 -max_len=4096` explícitos.
3. Registre stdout/stderr, status, toolchain, target, duração e novos artifacts.

**Falhas e parada:** build falha, assert/panic, timeout, erro de sanitizer ou saída não atribuível ao target: pare; preserve caso e não reexecute sobre o único corpus.

**Recuperação:** preserve o input/finding conforme [PROC-002](#proc-002); descarte somente checkout descartável depois de confirmar cópia e hashes.

**Campos PROC:** `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false` para esta entrega documental; `environment=checkout descartável, nightly/cargo-fuzz existentes`; `result=NOT_EXECUTED`; `review_evidence=F04,F05`; `execution_evidence=[]`; `limitations=[nenhuma execução ou revisão independente nesta autoria]`.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Preservar corpus e crash artifact

**Gatilho / resultado esperado:** target falha ou a sessão vai ser encerrada; bytes de reprodução ficam identificados e intactos.

**Modo / ambiente / permissão:** `LOCAL_ISOLATED`; leitura e cópia apenas de caminhos autorizados no checkout/artefato recebido.

**Entradas e validação:** caminho relativo, target, tipo de artifact e status do processo; pare se path real sair do root autorizado.

**Pré-condições:** destino de evidência isolado; não remova, edite ou substitua o original.

1. Copie o input/artifact pelo hash para um diretório de evidência nomeado com target e run local.
2. Registre SHA-256 do original e cópia, tamanho, origem e status; compare os hashes.
3. Encaminhe o identificador ao revisor designado antes de reproduzir ou alterar o corpus.

Comandos literais de preservação/hash (ajuste somente `ARTIFACT` e `EVIDENCE_DIR` autorizados):

```sh
mkdir -p "$EVIDENCE_DIR"
cp "$ARTIFACT" "$EVIDENCE_DIR/$(basename "$ARTIFACT")"
shasum -a 256 -- "$ARTIFACT" "$EVIDENCE_DIR/$(basename "$ARTIFACT")"
```

**Falhas e parada:** cópia/hash diverge, path não é legível ou ownership de dados é incerto: interrompa sem apagar nem compartilhar.

**Recuperação:** retome do original preservado; não há rollback seguro para artifact/cache apagado ou expirado.

**Campos PROC:** `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false`; `environment=checkout/artefato autorizado e destino isolado`; `result=NOT_EXECUTED`; `review_evidence=F04,F05`; `execution_evidence=[]`; `limitations=[não executado; retenção externa não observada]`.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Auditoria READ_ONLY de dependências e features

**Gatilho / resultado esperado:** mudança em `Cargo.toml`, target, feature,
workflow ou script; declarações, anchors e seletores ficam reconciliados sem
resolver ou executar o grafo.

**Modo / ambiente / permissão:** `READ_ONLY`; checkout pinado; sem instalação,
rede, runner, escrita de Cargo.lock ou alteração de corpus/artifacts.

**Entradas:** manifesto, targets, features, `scripts/fuzz-all.sh` e workflow;
`CARGO_HOME`/`CARGO_TARGET_DIR` devem ser diretórios temporários mesmo para
comandos que apenas listem metadados.

1. Leia o manifesto e confirme bins/deps/features contra [R06](REFERENCE.md#r06).
2. Compare os dois nomes de target em script/workflow, registrando schedule comentado.
3. Se autorizado, capture apenas saída de `cargo metadata --no-deps --format-version=1`; não use `cargo tree` como prova de runtime.

**Predicado:** nenhuma declaração/selector fica sem anchor; resolução, execução e reachability permanecem `UNKNOWN`.
**Parada/recuperação:** qualquer necessidade de resolver, baixar, compilar ou escrever estado vira `BLOCKED`; não há rollback porque o modo é somente leitura.
**Campos PROC:** `mode=READ_ONLY`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false`; `environment=checkout pinado + CARGO_HOME/CARGO_TARGET_DIR temporários`; `result=NOT_EXECUTED`; `review_evidence=F01,F04,F05`; `execution_evidence=[]`.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Mudança | Package / target / features | Comando ou PROC | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| Propriedade de append/evento | `corelink-audit-chain-fuzz` / `merkle_append` / nenhuma declarada | `CARGO_HOME="$PWD/.cargo-home-merkle" CARGO_TARGET_DIR="$PWD/.target-merkle" cargo +nightly fuzz run merkle_append -- -max_total_time=60 -max_len=4096` em `crates/corelink-audit-chain` | Exit 0 no intervalo; finding/panic reprova; não prova produção | Checkout descartável e estado isolado; não executado |
| Parsing/JCS/idempotência | Mesmo package / `jcs_canonicalize` / nenhuma declarada | `CARGO_HOME="$PWD/.cargo-home-jcs" CARGO_TARGET_DIR="$PWD/.target-jcs" cargo +nightly fuzz run jcs_canonicalize -- -max_total_time=60 -max_len=4096` no mesmo diretório | Exit 0 no intervalo; finding/panic reprova; entradas inválidas podem ser ignoradas pelo alvo | Mesmo ambiente isolado; não executado |
| Mudança comum ao manifesto | Ambos os bins | PROC-003, depois executar os dois comandos acima em sequência | Declarações reconciliadas; ambos completam sem finding no intervalo escolhido | Preservar corpus/artifacts separados por target; não executado |

**Negativos obrigatórios:** dois append errors (`None`) não satisfazem prova de append válido; JSON inválido e primeiro `serde_jcs::Err` são saídas esperadas do alvo, não crashes. **Gates globais:** nenhum foi executado nesta tarefa. Não usar `--all-features`, lista total, workflow dispatch ou perf benches como substituto.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| Harness source / manifest | Sim no diff; condicional após merge | Mudança confinada aos três arquivos do package | Reverter somente patch revisado; não tocar fonte de crates provedoras | Diff/hash volta à versão acordada; nenhuma prova de runtime implícita |
| Corpus local / `crash-*`, `leak-*`, `timeout-*` | Condicional; artifacts não são código | Conteúdo atual desconhecido; invocador escreve corpus/artifacts | Preserve input e hash antes de retry/limpeza; restaure apenas snapshot conhecido | SHA-256 da cópia coincide; evidência ausente não é recuperável por `git revert` |
| Cache e artifacts de workflow | Condicional e externa | Config declara cache por target e upload de falha com retenção de 14 dias; execução/retention atuais desconhecidas | Operador designado preserva artefato antes do prazo; não apagar cache para “resetar” evidência | Leitura posterior do workflow/run e hash do artifact; nenhum readback feito |

**Sem estado de aplicação declarado:** os três arquivos do package não declaram DB/schema/storage de produção; a configuração de corpus/artifacts pertence ao invocador. Não concluir que execução jamais acessa outros recursos.
**Limites conhecidos:** Git reverte source, não recupera corpus/cache expirado, arquivo excluído, log perdido ou resultado de runner não preservado.

<a id="m06"></a>
## M06 — Escalonamento e registro de manutenção

| Condição | Responsável / rota verificada | Evidência mínima | Ação vedada |
|---|---|---|---|
| Mudança/finding em API de `corelink-audit-chain` | Skill da crate existe; responsável humano não verificado | Símbolo, target, input e finding preservado | Declarar bug/garantia de produção sem validação do provedor |
| Mudança de analytics/dep externa | Owner humano desconhecido | Dependência e erro observável | Inventar assignee ou modificar dependência fora do escopo |
| Workflow, runner, cache ou artifact externo | Workflow estático conhecido; operator/escalation desconhecido | Ref/run, target, status e hash se fornecidos | Dispatch, editar schedule/cache ou alegar execução |

Após tarefa, registre baseline, pin, arquivos, comandos e resultado real; preserve somente artifacts da tarefa; encaminhe os quatro hashes finais para cold review independente. A rota humana ainda precisa ser designada.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
