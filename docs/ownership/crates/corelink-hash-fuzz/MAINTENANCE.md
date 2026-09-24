---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-hash-fuzz
manifest: crates/corelink-hash/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: hash-fuzz-source-20260921
---

# corelink-hash-fuzz — manual de manutenção

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) ·
[Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Use checkout de trabalho isolado e confira package pelo manifesto, não pelo diretório. Compare source pin e integration baseline informados na tarefa; alterações em qualquer um dos quatro paths source exigem recenso. Não carregue .env, credentials, corpus de cliente ou configuração real: estes harnesses são para bytes sintéticos.

Para execução local, requer toolchain nightly e cargo-fuzz compatíveis já disponíveis, cache/dependências preseeded e diretórios Cargo/build isolados por run. Defina FUZZ_CARGO_HOME e FUZZ_TARGET para paths graváveis exclusivos da execução; o Cargo home deve ter cache preseeded. Pare em falta de cache/offline, lockfile alterado, runner desconhecido ou necessidade de instalar ferramenta/rede; não afrouxe limites para obter verde. Gatilhos e duração em YAML não demonstram execução.

Índice: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003).

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Estado |
|---|---|---|---|---|
| Recensear identity, bins e wiring sem executar Cargo | [PROC-001](#proc-001) | LOCAL_ISOLATED | Leitura de fontes Git | REVIEWED_NOT_EXECUTED; cold review pendente |
| Validar um target após mudança aprovada | [PROC-002](#proc-002) | LOCAL_ISOLATED | Build/fuzz em checkout descartável | REVIEWED_NOT_EXECUTED |
| Preservar e reproduzir crash input | [PROC-003](#proc-003) | LOCAL_ISOLATED | Cópia de artifacts sintéticos | REVIEWED_NOT_EXECUTED |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Recensear o source pin

**Gatilho / resultado:** antes de alterar target, dependency ou CI; diff e matches classificados.
**Modo / ambiente / permissão:** LOCAL_ISOLATED no checkout; sem network/provider.
**Entradas e validação:** source pin e integration baseline da tarefa; manifest path canônico.
**Pré-condições:** ambos commits existem localmente; não trocar branch/estado do operador.

1. git status --short e git rev-parse HEAD; registre saída sem limpar árvore.
2. git diff --name-status SOURCE_PIN INTEGRATION_BASE -- crates/corelink-hash/fuzz/Cargo.toml crates/corelink-hash/fuzz/Cargo.lock crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs crates/corelink-hash/fuzz/fuzz_targets/verify_body.rs.
3. Leia manifesto, lock, targets e busque nome/path/target em Cargo.toml, .github, scripts e specs.
4. Registre matches literais relevantes, falsos positivos/históricos e UNKNOWN; não use ausência literal como prova de ausência semântica.

**Falhas e parada:** commit/path ausente, dirty state inexplicado ou fonte divergente; pare e reancore sem reset.
**Recuperação:** nenhuma modificação é feita; peça nova baseline ao lead.
**review_status:** BLOCKED. **execution_status:** REVIEWED_NOT_EXECUTED. **result:** NOT_EXECUTED. **required_for_acceptance:** true para cobertura documental.
**Evidência:** saída completa, commits, comandos e data; autoria não é cold review.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Smoke local isolado de um target

**Gatilho / resultado:** mudança de harness/API; um alvo selecionado completa o limite sem crash.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; checkout descartável, toolchain nightly/cargo-fuzz preinstalados, cache isolado, sem credentials.
**Entradas e validação:** escolher digest_parse ou verify_body; conservar lock original; cada invocação limitada a 60 s.

1. Confirme PROC-001 e dependências offline já disponíveis; se não, pare.
2. No diretório crates/corelink-hash, execute o alvo selecionado, sem juntar saídas que escondam exit status:

~~~sh
CARGO_HOME="$FUZZ_CARGO_HOME" CARGO_TARGET_DIR="$FUZZ_TARGET" CARGO_NET_OFFLINE=true cargo +nightly fuzz run digest_parse -- -max_total_time=60
CARGO_HOME="$FUZZ_CARGO_HOME" CARGO_TARGET_DIR="$FUZZ_TARGET" CARGO_NET_OFFLINE=true cargo +nightly fuzz run verify_body -- -max_total_time=60
~~~

3. Execute uma linha por vez; não rode ambos se a mudança atingir apenas um target.
4. Registre host/toolchain, seleção, duração, status e qualquer artifact; smoke não declara cobertura ampla.

**Falhas e parada:** build/fuzz falha, tenta atualizar lock, precisa de rede ou excede tempo; preserve log/artifact e não repita até verde.
**Recuperação:** corrigir ou reverter só a alteração própria; preservar crash original e comparar com baseline.
**review_status:** BLOCKED. **execution_status:** REVIEWED_NOT_EXECUTED. **result:** NOT_EXECUTED. **required_for_acceptance:** true para mudança de comportamento.
**Evidência:** este comando não foi executado na autoria documental; registre resultados reais quando aplicável.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Preservar e avaliar crash artifact

**Gatilho / resultado:** panic/assertion ou artifact em job; input original preservado e regressão triada.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; sem banco/provider/produção; artifact somente de fuzz sintético.
**Entradas e validação:** manter nome/path/job/target/SHA de origem; se origem for desconhecida, não alimentar o arquivo em outro sistema.

1. Confirme source/job/target e verifique que o arquivo veio deste harness; pare se provenance ou conteúdo for sensível/desconhecido.
2. Copie o artifact original para diretório privado do checkout descartável; registre tamanho e shasum -a 256.
3. Compare o input às pré-condições de digest_parse ou verify_body; associe ao commit e oráculo violado.
4. Use PROC-002 para reprodução bounded após definir seed path suportado pela versão aprovada de cargo-fuzz; não invente flags de minimização.

**Falhas e parada:** input sem proveniência, dado sensível, desaparecimento do original ou efeito fora do checkout; pare e escale.
**Recuperação:** não apague corpus/artifact no runner nem suponha que revert do código remova artifact remoto; peça ao owner do workflow tratamento do artifact.
**review_status:** BLOCKED. **execution_status:** REVIEWED_NOT_EXECUTED. **result:** NOT_EXECUTED. **required_for_acceptance:** true para crash confirmado.
**Evidência:** hash de original/cópia, comando/versão real e estado de reprodução. S14 é snapshot histórico de step state; nenhum crash artifact foi inspecionado.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Tipo de mudança | Package / target / features | Comando ou PROC | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| Identidade, bin ou dependência | corelink-hash-fuzz; ambos; features não declaradas | PROC-001 | Nome do package, paths, 2 targets e 3 deps conciliados; inversas desconhecidas permanecem | Fonte pinned; metadata não executada |
| Parser, filtro UTF-8 ou formato | digest_parse; corelink-hash provider | PROC-001/002 | Fuzz input UTF-8 não crasha no limite selecionado; não UTF-8 retorna cedo | Execução não realizada nesta revisão |
| Split de claim/body ou VerifiedBody | verify_body; corelink-hash, bytes | PROC-001/002 | <32 retorna; match e mismatch seguem oráculos; body length preservado no match | Execução não realizada; timing não medido |
| Lint/unsafe/target declaration | Bin exato, sem --all-features | PROC-001; `cargo clippy --manifest-path crates/corelink-hash/fuzz/Cargo.toml --locked --offline --all-targets -- -D warnings` | Manifest/source e CI declaram seleção; lint separado de fuzz | Nenhum build/Clippy executado |
| PR/nightly wiring e artifacts | Workflows hash/nightly e target exato | PROC-001; workflow run independente | Trigger, condição, duração e falha/artifact rastreados ao job | YAML lido; S14 é snapshot histórico com fuzz steps skipped; sem consulta live ao GitHub |
| Parent hash path-triggered PR gate | `.github/workflows/corelink-hash.yml`; pr-gate/fuzz-smoke | PROC-001; REL-011; conferir `pull_request.paths` incluindo `crates/corelink-hash/**` | Path elegível seleciona o gate parent; fuzz-smoke continua declaração até run observado | S09/YAML; nenhum run/runner verificado |
| Parent hash wasm-build | `.github/workflows/corelink-hash.yml`; wasm-build; `wasm32-unknown-unknown` | PROC-001; REL-012; conferir job/target e `cargo check --package corelink-hash --target wasm32-unknown-unknown` | Compatibilidade de compilação wasm; não runtime Cloudflare | S09/YAML; nenhum check wasm executado |
| Fuzz-nightly cache | `.github/workflows/corelink-hash.yml:233-236`; job `fuzz-nightly` | PROC-001; REL-014; conferir condição, workspace e runner | Cache pode alterar warmness/custo; não prova execução ou cobertura | S09 YAML; nenhum cache/run consultado |
| Fuzz-matrix nightly cache | `.github/workflows/nightly.yml:418-421`; job `fuzz-matrix` | PROC-001; REL-015; conferir condição, workspace, matrix e runner | Cache pode alterar warmness/custo; não prova execução ou cobertura | S10 YAML; nenhum cache/run consultado |
| Crash corpus/reproducer | Target e commit de origem | PROC-003 | Original preservado; causa corrigida; original e minimized reproduzem resultado esperado | Artifact não observado nesta revisão |

**Negativos obrigatórios:** não contar bytes inválidos em UTF-8 como parser coverage; não contar input curto como body coverage; assertions não medem tempo constante; workflow não equivale a run. Não executar --all-features, toda a frota fuzz, deploy ou jobs live para mudança local sem justificativa.

**Resultado desta edição:** somente inspeção estática e quatro checkers documentais; Cargo, Rust, build, teste e fuzz não foram executados.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| Manifesto/targets/fonte | Sim, enquanto local | Mudança própria identificada; código não reverte crashes já distribuídos | Reverter só commit próprio; manter falha corrigida como corpus de regressão autorizado | Diff do package e PROC-002 no target selecionado |
| Cargo.lock | Condicional | Versões do harness/hash podem alterar build/oráculo; lock não prova resolução | Preservar lock anterior e revisar delta; parar se ferramenta tentar atualizar | Comparação do lock e build sob seleção aprovada |
| Corpus local ignorado | Condicional | Pode conter seed/reproducer; tree fonte não tinha corpus rastreado | Cópia byte-a-byte e proveniência antes de minimizar | SHA-256 do original/cópia e reprodução PROC-003 |
| Artifacts de CI | Condicional | nightly.yml declara upload em falha e retenção de 14 dias; S14 não tinha crash artifact | Obter job/artifact por owner autorizado; não presumir que revert o apaga | Identidade do run, SHA, retenção e reprodutor |
| Digest/body persistidos por consumers | Não é estado deste harness | Writer/DB/bucket pertencem a consumers de corelink-hash | Coordenar owner do contrato e consumer real; não migrar/apagar pelo fuzzer | Fixture N/N-1 e plano do adapter em ownership separado |

Este package não possui tabela, bucket, transação ou corpus versionado identificados. git revert pode restaurar código, mas não remove crash artifacts remotos ou efeitos já escritos por um consumer. Compatibilidade de formato/armazenamento pertence a corelink-hash e ao writer/composition root.

<a id="m06"></a>
## M06 — Escalonamento e registro

| Condição | Owner/rota | Evidência mínima | Não fazer |
|---|---|---|---|
| Mudança em Digest/VerifiedBody | corelink-hash; identidade nominal UNKNOWN | REL-001, símbolo, target, input/regressão | Tratar fuzz package como implementação hash |
| Configuração job/artifact | Owner de integração CI não identificado | Workflow path, evento/job, run ID obtido pelo operador | Declarar YAML como prova de execução |
| Crash sem origem ou corpus ambíguo | Reviewer/security owner a designar | Hash, proveniência e escopo sem input sensível | Publicar input ou copiar para sistema externo |
| Documento pronto | Cold reviewer independente a designar | Quatro hashes finais, fontes e achados | Autoaprovar ou promover state |

Registrar baseline, target, comando literal, status, engine/toolchain e artifact hash; atualizar artefatos atingidos e solicitar cold review. Não inserir corpus bruto sensível em issue, log ou relatório.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
