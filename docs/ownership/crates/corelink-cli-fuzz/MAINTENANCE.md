---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-cli-fuzz
manifest: tools/cli/fuzz/Cargo.toml
source_commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
profile: "S"
state: draft
evidence_set: corelink-cli-fuzz-structural-normalization-20260921
---

# corelink-cli-fuzz — manual de manutenção

[Preparação](#m01) · [Escolher procedimento](#m02) · [Procedimentos](#m03) ·
[Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

**Checkout / fonte:** use um worktree isolado, confirme `git rev-parse HEAD`,
`git show 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018:tools/cli/fuzz/Cargo.toml` e o manifesto exato. Não use o
checkout sujo do usuário. **Pré-condições:** package name, cinco bin paths,
source pin e parent contract devem coincidir; divergência para e escala.

**Ferramentas:** leitura Git, `rg`, `git diff --check` e checker CO-1 são
READ_ONLY. Cargo, Rust, cargo-fuzz, CI, providers e rede são operações separadas;
não foram executadas nesta autoria. Nunca coloque PAT, token ou env secreto em
command line, corpus, log, issue ou artifact.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Precisa de autorização extra? |
|---|---|---|---|---|
| Conferir package, targets, blobs e wiring | [PROC-001](#proc-001) | `READ_ONLY` | nenhuma | não |
| Validar estrutura e links dos quatro docs | [PROC-002](#proc-002) | `READ_ONLY` | nenhum arquivo gerado | não |
| Exercitar target em ambiente temporário | [PROC-003](#proc-003) | `LOCAL_ISOLATED` | target/corpus temporário | sim, toolchain/recursos |
| Preservar e reproduzir crash/leak/timeout | [PROC-004](#proc-004) | `LOCAL_ISOLATED` | cópia de artifact local | sim se sair do isolamento |
| Alterar dependência, target ou parser contract | [PROC-005](#proc-005) | `READ_ONLY` → revisão | código/lockfile sob mudança aprovada | revisão do parent/CI |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — recensear manifesto e relações

**Gatilho / predicado:** mudança ou revisão deve mostrar os cinco bins e o path parent correto.
**Modo / ambiente / permissão:** `READ_ONLY`, worktree isolado, sem credenciais.
**Entradas:** pin `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`, manifesto, `tools/cli/src/lib.rs`, targets e workflows.
**Pré-condições:** HEAD e caminhos confirmados.

1. Leia package name, `[workspace]`, deps, lints e cada `[[bin]]`.
2. Compare imports dos cinco targets com API-001–006.
3. Busque `corelink-cli`, nomes dos targets, script e workflows fora de Cargo.
4. Registre cada relação em B03, inclusive referências stale ou ausentes.

**Falhas/parada:** divergência de package, source ou target; não complete por inferência.
**Recuperação:** preservar saída textual e reancorar em commit confirmado.
**Certificação:** `reviewed-not-executed`; não é prova de resolve/runtime.
**Evidência:** comandos, HEAD, blobs, paths e resultado da busca.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — validar os quatro artefatos

**Gatilho / predicado:** antes de commit ou revisão independente, os docs devem passar CO-1 no perfil S.
**Modo / ambiente / permissão:** `READ_ONLY`, Python local somente; não executa Cargo.
**Entradas:** quatro paths e checker versionado em `docs/ownership/tools/check_docs.py`.
**Pré-condições:** frontmatter e anchors finalizados.

1. No root do checkout, execute exatamente:

   ```sh
   python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-cli-fuzz/SKILL.md
   python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-cli-fuzz/REFERENCE.md
   python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-cli-fuzz/BLAST_RADIUS.md
   python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-cli-fuzz/MAINTENANCE.md
   git diff --check 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018 -- .claude/skills/own-corelink-cli-fuzz docs/ownership/crates/corelink-cli-fuzz
   ```

2. Confirme que cada checker retorna exit `0` e `IMPLEMENTED_CHECKS_PASS`; esse resultado é somente gate estrutural.
3. Registre linhas, palavras, bytes, saída completa dos quatro comandos e o SHA do commit; qualquer byte posterior invalida a revisão correspondente.

**Falhas/parada:** placeholder, anchor ausente, limite ou diff error bloqueia handoff.
**Recuperação:** corrigir somente bytes do package e repetir todos os checks afetados.
**Certificação:** `executed_local` para checker, nunca `APPROVE`.
**Evidência:** saída completa, versão do checker e hashes; cold review ainda obrigatório.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — smoke fuzz isolado

**Gatilho / predicado:** owner autorizado precisa observar que um target inicia e respeita seu limite sem afirmar cobertura.
**Modo / ambiente / permissão:** `LOCAL_ISOLATED`; CARGO_HOME/TARGET_DIR temporários; sem secrets/rede.
**Entradas:** target escolhido, seed não sensível, `FUZZ_DURATION` curto.
**Pré-condições:** resolver locked e parent API reconciliada.

1. Execute em `bash`, sem secrets, com diretórios privados e Cargo isolado. Escolha `<target>` entre os cinco nomes registrados em R06.

   ```sh
   set -euo pipefail
   umask 077
   RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/corelink-cli-fuzz.XXXXXX")"
   export CARGO_HOME="$RUN_ROOT/cargo"
   export CARGO_TARGET_DIR="$RUN_ROOT/target"
   CORPUS="$RUN_ROOT/corpus"
   ARTIFACTS="$RUN_ROOT/artifacts"
   mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$CORPUS" "$ARTIFACTS"
   cd tools/cli/fuzz
   set +e
   cargo +nightly fuzz run <target> "$CORPUS" -- -artifact_prefix="$ARTIFACTS/" -max_total_time=60 2>&1 \
     | sed -E 's/(corelink_(pat|ci|ro)_[^[:space:]]+)/[REDACTED_PAT]/g; s/(CORELINK_PAT=)[^[:space:]]+/\1[REDACTED]/g' \
     > "$RUN_ROOT/output.redacted"
   status=${PIPESTATUS[0]}
   set -e
   test ! -e "$RUN_ROOT/output.raw"
   printf 'exit_status=%s\n' "$status" >> "$RUN_ROOT/output.redacted"
   shasum -a 256 "$RUN_ROOT/output.redacted"
   ```

2. Persista somente `output.redacted`, o status, target, comando e hashes; não grave o stream bruto. Se houver crash/leak/timeout, pare e use PROC-004.
3. Remova o diretório temporário apenas após capturar evidência redigida; não minimize por cima do original.

**Falhas/parada:** build failure, dependency fetch, OOM, secret exposure ou runner não isolado.
**Recuperação:** remover só temporários da tarefa; preservar artifact fora do diretório efêmero.
**Certificação:** `not-executed` neste candidato; uma futura execução precisa do ambiente.
**Evidência:** run ID local, commit, toolchain, duração, exit status e artifact hash.

[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — preservar e reproduzir falha

**Gatilho / predicado:** target produces crash, leak, timeout or non-zero assertion.
**Modo / ambiente / permissão:** `LOCAL_ISOLATED`; sem provider, segredo ou produção.
**Entradas:** input copy, target, commit, command and redacted output.
**Pré-condições:** original hash before minimization.

1. Se target/path não estiverem identificados, pare. Copie o input, registre o
   hash e só depois minimize:

   ```sh
   set -euo pipefail
   umask 077
   RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/corelink-cli-fuzz-replay.XXXXXX")"
   export CARGO_HOME="$RUN_ROOT/cargo"
   export CARGO_TARGET_DIR="$RUN_ROOT/target"
   CORPUS="$RUN_ROOT/corpus"
   ARTIFACTS="$RUN_ROOT/artifacts"
   mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$CORPUS" "$ARTIFACTS"
   cd tools/cli/fuzz
   TARGET=secret_redaction_check
   INPUT=/absolute/path/to/artifacts/crash-<id>
   test -r "$INPUT"
   INPUT_COPY="$CORPUS/replay-input"
   cp "$INPUT" "$INPUT_COPY"
   shasum -a 256 "$INPUT_COPY"
   set +e
   cargo +nightly fuzz run "$TARGET" "$CORPUS" -- -artifact_prefix="$ARTIFACTS/" -runs=1 2>&1 \
     | sed -E 's/(corelink_(pat|ci|ro)_[^[:space:]]+)/[REDACTED_PAT]/g; s/(CORELINK_PAT=)[^[:space:]]+/\1[REDACTED]/g' \
     > "$RUN_ROOT/output.redacted"
   status=${PIPESTATUS[0]}
   set -e
   test ! -e "$RUN_ROOT/output.raw"
   printf 'exit_status=%s\n' "$status" >> "$RUN_ROOT/output.redacted"
   shasum -a 256 "$INPUT_COPY" "$RUN_ROOT/output.redacted"
   test "$status" -ne 0
   ```

2. Preserve o log redigido; o predicado é reproduzir a classe conhecida, não só iniciar.
3. Classifique a causa como harness, parent API, dependency ou ambiente.
4. Minimize somente após preservar original, hash, target, commit e comando.
5. Se não reproduzir, registre `NOT_REPRODUCED`; escale ambiente/toolchain.

**Falhas/parada:** input perdido, output com segredo, não reprodução ou produção.
**Recuperação:** manter original, remover cópias sensíveis e escalar.
**Certificação:** `reviewed-not-executed` aqui; sem run nesta autoria.
**Evidência:** hashes original/minimizado, target, command, environment e verdict.

[Índice de procedimentos](#m02)

<a id="proc-005"></a>

### PROC-005 — mudar dependência, target ou contrato

**Gatilho / predicado:** alteração deve preservar package identity e atualizar todas as inversas.
**Modo / ambiente / permissão:** `READ_ONLY` para planejamento; mudança requer revisão do parent/CI.
**Entradas:** diff do manifesto/source, API/REL/INV afetados e recuperação.
**Pré-condições:** confirmar source pin e contrato `corelink-cli`.

1. Liste novo/removido target, dep, feature, lockfile e script/workflow atingidos.
2. Atualize REFERENCE/BLAST/MANUAL e o peer relation key, se aplicável.
3. Planeje `cargo metadata --locked --offline` e target checks; não executar aqui.
4. Se input/contract mudou, preserve corpus e marque compatibilidade como desconhecida.
5. Solicite cold review novo para cada byte alterado.

**Falhas/parada:** dep não resolvida, target sem owner, lockfile stale ou inversa não conciliada.
**Recuperação:** roll-forward coordenado ou reverta apenas a mudança de código; Git revert não recupera corpus removido.
**Certificação:** planejamento `reviewed-not-executed`; aprovação não implícita.
**Evidência:** diff, relação, comando planejado, owner e decisão de compatibilidade.

[Índice de procedimentos](#m02)

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Tipo de mudança | Package / target / features | Comando ou PROC | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| manifesto/targets | `corelink-cli-fuzz`, cinco bins | PROC-001 + CO-1 | identity/paths concordam | estático; sem Cargo |
| docs/anchors | quatro artefatos, profile S | PROC-002 | quatro checks PASS e diff limpo | executável local; registrar saída |
| parser/config | `config_toml`, `cli_input` | PROC-003 + API-001/002 | sem crash no limite; errors structured; `cli_input` é key/value, não argv | não executado neste commit |
| auth/redaction | `auth_resolution`, `secret_redaction_check` | PROC-003/004 | determinismo e zero count nos `ConfigError` exercitados; não cobre stderr geral | não executado neste commit |
| JSON | `json_deserialize` | PROC-003 | parse/serialize local não aborta para corpus; não prova formatter/schema de saída | não executado neste commit |
| dep/contract | package + parent | PROC-005, metadata locked | resolução e inversas reconciliadas | não executado neste commit |

**Negativos obrigatórios:** bytes não UTF-8, TOML/JSON inválido, key inexistente,
PAT malformado, erro com substring PAT-shaped, divergência de parser e input que
causa timeout. **Gates:** CO-1 estrutural e revisão independente; nenhum green de
workflow substitui execução específica. Não executar `--all-features`, deploy,
provider, ou o script global enquanto REL-009 não for corrigida/justificada.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| target source/manifest | sim no Git, condicional no corpus | bytes e input original preservados | roll-forward/revert coordenado | diff limpo e target revalidado |
| `fuzz_api` contract | sim no código, não no consumidor já compilado | parent/targets podem estar em versões distintas | compatibilidade coordenada com `corelink-cli` | compile + target evidence futura |
| lockfile/dependency | condicional | registry/cache/toolchain podem mudar | atualizar sob revisão; não apagar lock | locked metadata e diff |
| corpus/crash artifact | não se apagado sem cópia | input pode ser única reprodução | hash/cópia antes de minimizar | reprodução no mesmo target/commit |
| CI/script wiring | sim no texto; não cria evidência retroativa | run ausente ou path stale | corrigir registry e registrar novo run | run ID e artifact redigido |

**Sem estado próprio:** o package não grava dados de produção; corpus/artifacts de
uma execução podem ser estado operacional local/CI. `git revert` não restaura um
artifact apagado, não invalida um release já publicado e não prova compatibilidade
de input/wire. Mudança de parser pode aceitar/rejeitar corpus existente sem alterar
o runtime da CLI.

<a id="m06"></a>
## M06 — Escalonamento e registro de manutenção

| Condição | Responsável / rota verificada | Evidência mínima | Ação vedada |
|---|---|---|---|
| API parent changed | owner `corelink-cli` (nome de pessoa não verificado) | symbol, diff, affected targets | aprovar harness sozinho |
| script/workflow path stale | maintainer CI a identificar | workflow/script path, run result | chamar declaração de execução |
| crash/leak/timeout | package + security/CLI owner a atribuir | original hash, target, redacted output | apagar única reprodução |
| dependency/lock failure | package/CI maintainer | lock diff, resolver output, environment | buscar provider ou mutar shared Cargo |
| PAT/secret exposure | security incident route a verificar | apenas fingerprint/redacted metadata | copiar segredo para logs/issues |
| production/release request | operador autorizado não verificado nesta fonte | approval, scope, rollback | executar por este manual |

Após a tarefa: registre baseline, resultado e hashes; remova somente temporários
criados por ela; reconcilie relações e backlog; encaminhe qualquer byte alterado
para nova cold review. O estado mínimo por procedimento é `executed_local`,
`reviewed-not-executed` ou `blocked-for-authorized-operation`; este candidato
marca PROC-001/002 como executed local apenas para leitura/checker e PROC-003–005
como reviewed-not-executed.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
