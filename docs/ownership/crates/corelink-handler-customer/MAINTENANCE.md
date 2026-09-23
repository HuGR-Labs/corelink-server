---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-handler-customer
manifest: crates/corelink-handler-customer/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-customer-structural-normalization-20260921
---

# corelink-handler-customer — manutenção

Procedimentos de fonte e checagem documental usam `READ_ONLY`; inspeção de
estado mutável do fake e Rust tests exigem `LOCAL_ISOLATED` com executor e
evidência separados. Nenhum procedimento autoriza rede, acesso a cliente,
identidade, segredo, billing, backend audit/SLO, Worker, deploy ou operação real.

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Testes](#m04) · [Recuperação](#m05) · [Escalação](#m06).

Procedures: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003).

<a id="m01"></a>
## M01 — Preparação

Modo: `READ_ONLY`. Predicado: HEAD, manifesto e recorte são os desta edição.
Ação: comparar SHA, branch e `crates/corelink-handler-customer/Cargo.toml` antes de
interpretar fonte. Parada: baseline, árvore ou manifesto diverge. Recuperação: não
editar; reconciliar o recorte. Evidência: SHA, branch e caminhos lidos.

<a id="m02"></a>
## M02 — Seleção

| Gatilho | Procedimento e modo | Gate/owner |
|---|---|---|
| Trait, DTO, erro, reexport ou compatibilidade muda | [PROC-001](#proc-001), `READ_ONLY` | B03/B05, owner do contrato e container |
| Tenant guard, audit kind, PAT/team mutation ou fake muda | [PROC-002](#proc-002), `READ_ONLY`; ramo de inspeção mutável `LOCAL_ISOLATED` | B03, owner do fake; D1 real ao container |
| Handoff documental ou revisão de fonte | [PROC-003](#proc-003), `READ_ONLY` | Quatro checkers e diff-check |
| Código Rust mudou e validação isolada está no escopo | [M04](#m04), `LOCAL_ISOLATED` | Executor de código com ambiente isolado e output literal |

<a id="m03"></a>
## M03 — Procedimentos

Índice executável: [PROC-001 — contrato](#proc-001),
[PROC-002 — guard/mutação](#proc-002),
[PROC-003 — checagem documental](#proc-003). Cada PROC distingue revisão de
execução; [M04](#m04) contém os gates Rust que dependem de executor isolado.

<a id="proc-001"></a>
### PROC-001 — Avaliar mudança de trait/DTO
**Objetivo/gatilho:** mudança em grupo handler, DTO, erro ou reexport. **Modo:** `READ_ONLY`; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`; `required_for_acceptance = false` neste handoff documental sem diff Rust; passa a `true` se uma mudança de API/DTO/erro/reexport for submetida à aceitação. **Pré-condições/ambiente:** pin de fonte e diff limitados; sem cliente/segredo. **Permissões/entradas:** leitura do repo; símbolo/consumer afetado.

**Passos:** (1) seguir `lib.rs` até trait e DTO; (2) comparar assinatura/campos/erro; (3) buscar imports D1 e handlers HTTP; (4) mapear N/N-1 e registrar owner de compatibilidade. **Predicado:** mudança ligada a API e REL específico. **Parada/recuperação:** compatibilidade/HTTP não determinado bloqueia conclusão; pedir contrato/fixture ao owner do container. **Evidência de execução:** SHA, símbolos, fontes, buscas e resultado por consumer.
[Procedure index](#m03)

<a id="proc-002"></a>
### PROC-002 — Avaliar guard e mutação auditada
**Gatilho/estado:** mudança de tenant guard, audit, PAT ou team mutation. `review_status = REVIEWED`; `execution_status = REVIEWED_NOT_EXECUTED`; `required_for_acceptance = false` para este handoff só documental, `true` para aceitação de código nesses ramos. Ambiente: pin/diff local, IDs fictícios, sem segredo ou backend real.

**READ_ONLY:** escolher API-001–011, denial REL-012–022 e mutação REL-023–031; ordenar Attempted, mutação e Committed; comparar trait, fake e D1, inclusive PATs na remoção. Saída: efeito e falha por ramo com fonte. Se identidade/durabilidade D1 não for verificável, marcar UNKNOWN e escalar ao container.

**LOCAL_ISOLATED, quando requerido:** fixture descartável com handler/observer em memória e `SelectiveAuditSink: AuditSink` próprio. O sink guarda eventos em `Mutex<Vec<AuditEvent>>`, registra Attempted e falha uma vez só no kind `KeyCreateCommitted`, sem gravá-lo; expõe `snapshot` e `clear_failure`. Usar tenant, nome e timestamp fictícios únicos. `InMemoryAuditSink::inject_failure` falha também em Attempted antes da mutação (`audit.rs:190-265`); não exercita este ramo.

**Asserções:** `create` retorna `AuditFailed`; snapshot antes da leitura contém `KeyCreateAttempted`, sem `KeyCreateCommitted`. Limpar falha na fixture; `CustomerKeysHandler::list` encontra a PAT row do tenant/ID. Para invite, selecionar `TeamInviteCommitted` e conferir membro. Parar em lock envenenado, row ambígua ou backend real. Preservar IDs, branch, erro e snapshots; não repetir mutação automaticamente. Recriar/reseedar fixture; recuperação D1 cabe ao owner.
[Procedure index](#m03)

<a id="proc-003"></a>
### PROC-003 — Validar evidência e limites
**Objetivo/gatilho:** handoff de mudança/documento. **Modo:** `READ_ONLY`; `review_status = REVIEWED`, `execution_status = REVIEWED_NOT_EXECUTED`; `required_for_acceptance = true` para handoff documental. **Pré-condições/ambiente:** quatro artefatos no pin esperado. **Permissões/entradas:** leitura e checker documental.

**Passos:** (1) conferir anchors/IDs; (2) executar os quatro comandos documentais de [M04](#m04); (3) executar `git diff --check`; (4) separar resultados documentais dos Rust gates e owners. **Predicado:** quatro `IMPLEMENTED_CHECKS_PASS` e diff limpo, sem claim operacional. **Parada/recuperação:** falha invalida handoff; corrigir arquivo afetado e repetir. **Evidência de execução:** registrar data UTC, checkout, comando, exit code e stdout/stderr literal neste registro antes de mudar para `EXECUTED_LOCAL`; Rust gates seguem `REVIEWED_NOT_EXECUTED`.

**Estado deste registro:** o resumo anterior HC-PROC003-20260923T1954Z não
continha saída literal, timestamp e exit code por comando, nem a seleção exata
do `git diff --check`. Ele não sustenta `EXECUTED_LOCAL` para este procedimento.
Até anexar evidência por comando aos bytes finais, o gate documental de
aceitação permanece aberto. Gates Cargo, revisão bilateral e runtime também
não foram certificados.
[Procedure index](#m03)

<a id="m04"></a>
## M04 — Testes e gates

Modo: `READ_ONLY` para checkers e `LOCAL_ISOLATED` para Rust. Predicado: estado `Mutex`/`HashMap`, fixture, observação ou caminho
de retorno muda. Ação: verificar o predicado de fonte para sucesso, erro e negação e a
direção para `SliObserver`. Parada: pedido requer métrica entregue, alerta, SLO, dado de
cliente ou comportamento concorrente observado. Recuperação: registrar o limite e obter
evidência do sistema correspondente. Evidência: `handler.rs`, `observer.rs` e B04.

| Seleção | Comando exato, a partir da raiz | Predicado | Estado |
|---|---|---|---|
| Referência | `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-handler-customer/REFERENCE.md` | `IMPLEMENTED_CHECKS_PASS`; somente estrutura | `READ_ONLY`; executar em PROC-003 |
| Blast | `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-handler-customer/BLAST_RADIUS.md` | `IMPLEMENTED_CHECKS_PASS`; somente estrutura | `READ_ONLY`; executar em PROC-003 |
| Manutenção | `python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-handler-customer/MAINTENANCE.md` | `IMPLEMENTED_CHECKS_PASS`; somente estrutura | `READ_ONLY`; executar em PROC-003 |
| Skill | `python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-handler-customer/SKILL.md` | `IMPLEMENTED_CHECKS_PASS`; somente estrutura | `READ_ONLY`; executar em PROC-003 |
| Test target explícito | `cargo test -p corelink-handler-customer --test handler_customer` | Assertions de audit, guard, mutações e SLI passam | `LOCAL_ISOLATED`; REVIEWED_NOT_EXECUTED; executor de código requerido |
| Test target autodetectado | `cargo test -p corelink-handler-customer --test mutation_kills` | Assertions por endpoint passam | `LOCAL_ISOLATED`; REVIEWED_NOT_EXECUTED; executor de código requerido |
| Seleção de release | `cargo test -p corelink-handler-customer --release` | Ambos os targets selecionados passam | `LOCAL_ISOLATED`; REVIEWED_NOT_EXECUTED; executor de código requerido |

Os três comandos Cargo acima selecionam o package `corelink-handler-customer` no
workspace Rust local, sem feature de package (`Cargo.toml` não declara features).
Os dois primeiros selecionam integration test targets nomeados; `--release`
seleciona os testes do package sob o perfil release. Ambiente requerido:
checkout isolado, toolchain Cargo/Rust do workspace e fixture sem dados reais,
segredos, rede ou backend de produção.

`execution_status = REVIEWED_NOT_EXECUTED` e resultado `UNKNOWN` para os três.
São gates de aceitação quando mudança de API/fake dispara PROC-001/002. Até
resultado literal do executor, esses procedimentos e a mudança de código não
têm aprovação de aceitação. Target server, D1 e Worker exigem validação própria
do owner de composição. Um teste local não atesta operação.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

Modo: `READ_ONLY` para análise, `LOCAL_ISOLATED` para fixture. Predicado: import conhecido, adapter ou referência de container muda.
Ação: classificar a relação como import/trait/DTO estático e revisar a fronteira do
package. Parada: a alegação passa de rota declarada a seleção de D1, Worker CF,
target ou request real. Recuperação: encaminhar para owner de container/adapter/runtime
sem transferir ownership. Evidência: manifesto, busca reversa, B03/B06 e
[corelink-server REL-033](../corelink-server/BLAST_RADIUS.md#rel-033).

Para mudança de API, comparar N (novo trait/DTO/erro) com N-1 (consumer e
artefato anterior) nos imports D1 e handlers HTTP; alteração de campo ou
variante não é compatível só por `#[non_exhaustive]`. Exigir decisão do owner
do container sobre rollout/roll-forward antes de afirmar compatibilidade;
reverter código não desfaz dados já escritos. Sem evidência de artifact/version
N-1, registrar `UNKNOWN`, não aprovação de wire compatibility.

### Recuperação de mutação parcial no fake

Se `Committed` falhar, registrar tenant, método, ID e mensagem antes de nova ação.
Em ambiente local isolado, consultar `SelectiveAuditSink::snapshot` na fixture
dedicada de PROC-002 e, após limpar a falha seletiva apenas ali, usar
`CustomerKeysHandler::list` ou
`CustomerTeamHandler::list` para conferir a row.

`create` pode inserir PAT antes de falhar no lock de tokens; o mapa de tokens não
possui inspeção pública. Não repetir cegamente `create`/`invite`: o mesmo ID ou
estado pode já existir.
Recriar um fake limpo e reseedar a fixture, ou reparar a fixture de teste com
evidência de estado. Em D1/backend real, owner de chave/team/audit decide a
reconciliação; `git revert` não compensa escrita já efetuada.

<a id="m06"></a>
## M06 — Escalonamento e registro

Modo: `READ_ONLY`. Predicado: análise ou mudança documental terminou. Ação: registrar
baseline, arquivos, símbolos, relações, comandos documentais, resultados e lacunas.
Parada: checker não equivale a teste, revisão fria, compatibilidade, runtime ou operação.
Recuperação: marcar desconhecido e abrir decisão com fonte, consumer e owner necessário.
Evidência: diff, `git diff --check`, checker e SHA.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
