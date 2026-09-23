---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-user-journeys
manifest: tests/e2e-user-journeys/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: H
state: draft
evidence_set: source-inspection-1177dad2
---

# e2e-user-journeys — manual de manutenção

[Preparação](#m01) · [Escolher procedimento](#m02) · [Procedimentos](#m03) ·
[Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Confirme a baseline antes de ler o package. Comandos de journey não fazem parte da revisão documental.

| Campo | Referência segura |
|---|---|
| Checkout | Baseline de integração `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540`; confirme `git rev-parse HEAD`. |
| Source | Pin explícito `1177dad2ca2a9f21c29b5a118aa7944b77147798`; compare package, manifest e workspace. Não use `main` local divergente. |
| Target / features | Um bin `e2e-user-journeys`; nenhuma feature própria. Cargo só para mudança futura de código. |
| Env / fixture | Não leia secrets. Slots ausentes podem GATED; valide host, tenant throwaway, tokens e flags antes de qualquer run autorizado. |
| Runner remoto | `scripts/e2e-real-client/provision-and-run-suite.sh` usa endpoint remoto e provisiona usuários. |
| Recursos | HTTP timeout 60s; quota e burst hard são opt-in; cleanup varia por journey. |

Na revisão documental não habilite Cargo, provider, GitHub, deploy, DB/storage ou provisionamento. Não presuma idempotência nem remoção automática de artefatos.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Precisa de autorização extra? |
|---|---|---|---|---|
| Verificar identidade, rota, gate ou consumer textual | [PROC-001](#proc-001) | READ_ONLY | Leitura local apenas | Não. |
| Alterar somente ownership docs ou seus links | [PROC-002](#proc-002) | LOCAL_ISOLATED | Arquivos do escopo e checks estáticos | Revisão independente antes de integrar. |
| Alterar código e validar unidade/bin local | [PROC-003](#proc-003) | LOCAL_ISOLATED | Build/test local, sem rede pretendida | Aprovação de mudança; Cargo não é permitido nesta autoria. |
| Chamar API, CLI ou wrapper provisionador | [PROC-004](#proc-004) | AUTHORIZED_OPERATION | Efeitos em ambiente descartável explicitamente autorizado | Sim; endpoints, fixture e cleanup confirmados. |
| DSR, quota cap, webhook de billing ou convite | [PROC-005](#proc-005) | AUTHORIZED_OPERATION | Mutação delimitada em recurso descartável | Sim; owner/ambiente/operação específicos. |
| Alterar dependência, membership ou `Cargo.lock` | [PROC-006](#proc-006) | READ_ONLY / LOCAL_ISOLATED | Census do nó lock e revisão de diff; regeneração somente em operação autorizada | Sim para regenerar/atualizar lock ou resolver dependências. |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Confirmar fonte e fronteira

**Gatilho:** dúvida sobre fronteira; baseline e caminho coincidem.

**Modo:** READ_ONLY, checkout local; sem Cargo, HTTP ou credenciais.

**Entradas:** source pin `1177dad2`; root e package.

**Pré-condições:** checkout identificado; sem mudança alheia.

1. Compare manifest e package com o source pin.
2. Leia `journeys/mod.rs`, `main.rs` e o módulo afetado.
3. Busque referências fora de Cargo, scripts e Worker.
4. Reproduza o census do dispatcher e dos implementadores:

   ```bash
   dispatches="$(rg -c '^[[:space:]]*out\.extend\([[:alnum:]_]+::run\(cfg, client\)\);$' tests/e2e-user-journeys/src/journeys/mod.rs)"
   runs="$(rg -l '^pub fn run\(cfg: &Config, client: &Client\)' tests/e2e-user-journeys/src/journeys/*.rs | wc -l | tr -d ' ')"
   expected='identity cas concurrency ac bazel turbo adapters oci oci_public_isolation dashboard pat_lifecycle billing quota dsr introspect audit security edge abuse shared_cache runners runner_purchase team'
   actual="$(sed -n 's/^[[:space:]]*out\.extend(\([[:alnum:]_]*\)::run(cfg, client));/\1/p' tests/e2e-user-journeys/src/journeys/mod.rs | paste -sd' ' -)"
   test "$dispatches" = 23 && test "$runs" = 23 && test "$actual" = "$expected"
   ```

5. O `test` rejeita ordem, duplicata ou módulo ausente; registre drift e preserve unknowns.

**Saída:** dispatches e implementadores contam `23` em ordem. O 22 do plano é histórico e corrigido; execução desconhecida.

**Falha/parada:** source indisponível, hash divergente ou owner externo desconhecido.

**Recuperação:** nenhuma; escolha pin autorizado.

**Certificação:** `REVIEWED`, `EXECUTED_LOCAL`, `PASS`; `required_for_acceptance=true`.

**Evidência:** hashes Git e diff source-scoped; sem Cargo/runtime.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Validar artefatos de ownership

**Gatilho / resultado esperado:** edição de um dos quatro documentos; quatro checks H e diff-check devem passar.

**Modo / ambiente / permissão:** LOCAL_ISOLATED; Python e candidate v1.3 fornecido pelo lead; sem rede.

**Entradas e validação:** checkout na baseline; `OWNERSHIP_V13_ROOT` aponta para a extração candidata aprovada, nunca gravada como política permanente.

**Pré-condições:** quatro caminhos exatos e frontmatter H.

1. Execute `python3 "$OWNERSHIP_V13_ROOT/tools/check_docs.py" .claude/skills/own-e2e-user-journeys/SKILL.md --kind skill --profile H --root .`.
2. Repita para REFERENCE (`--kind reference`), BLAST_RADIUS (`--kind blast_radius`) e MAINTENANCE (`--kind maintenance`).
3. Execute `git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- .claude/skills/own-e2e-user-journeys/SKILL.md docs/ownership/crates/e2e-user-journeys/REFERENCE.md docs/ownership/crates/e2e-user-journeys/BLAST_RADIUS.md docs/ownership/crates/e2e-user-journeys/MAINTENANCE.md`.

**Saída esperada:** cada checker `IMPLEMENTED_CHECKS_PASS`, sem erros, e diff-check vazio.

**Falhas e parada:** link/limite/schema falha; corrija o documento e rerode os quatro; não bypass.

**Recuperação:** reverta somente a edição inválida; preserve demais mudanças de autor.

**Certificação:** review `REVIEWED`; execução `EXECUTED_LOCAL`; resultado `PASS`; `required_for_acceptance=true` para entrega estática.

**Evidência:** JSON de cada checker e diff visível; são checks estruturais, não cold review nem aprovação independente.

[Índice de procedimentos](#m02)

<a id="proc-006"></a>

### PROC-006 — Reconciliar dependências e Cargo.lock

**Gatilho / resultado esperado:** mudança no manifesto, dependências, membership ou lock; package e lock permanecem coerentes sem edge first-party.

**Modo / ambiente / permissão:** READ_ONLY para census/diff; LOCAL_ISOLATED para editar; sem `cargo update`, rede, build ou execução nesta revisão.

**Entradas:** `tests/e2e-user-journeys/Cargo.toml`, `Cargo.toml` raiz, `Cargo.lock`, source pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`.

**Passos:**

1. Leia o package e o membro root; confirme `reqwest`, `serde_json`, `uuid`, `sha2`, `blake3`, `hex` e zero `path =` para `crates/`.
2. Extraia o nó exato: `rg -n -A10 -B2 '^name = "e2e-user-journeys"$' Cargo.lock`.
3. Compare manifest/root/lock com `git diff "$SOURCE_PIN" -- Cargo.toml tests/e2e-user-journeys/Cargo.toml Cargo.lock`; classifique qualquer mudança por package, root ou transitiva.
4. Se o nó precisar ser regenerado, pare e encaminhe para o owner do workspace/Cargo; depois exija diff do lock, SBOM e revisão independente.

**Predicado:** o nó lock corresponde ao manifesto e não cria dependência first-party não declarada.

Presença no lock não prova build, deploy ou runtime.

**Falha/parada:** lock ausente, múltiplos nós, checksum inesperado, mudança transitiva não explicada ou source pin divergente; não use `cargo tree --target all` como prova de artefato.

**Recuperação:** preserve o lock anterior até haver uma regeneração autorizada e revisão; não remova entradas manualmente.

**Certificação:** `REVIEWED`; execução `REVIEWED_NOT_EXECUTED`; resultado `NOT_EXECUTED`; evidência é o diff e o trecho do nó.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Validar alteração Rust local

**Gatilho / resultado esperado:** mudança local de runner, harness, persona ou journey; compile/test declarado passa no target único.

**Modo / ambiente / permissão:** LOCAL_ISOLATED; ambiente sem endpoint acessível e sem secrets; esta autoria não está autorizada a executar Cargo.

**Entradas e validação:** `Cargo.toml`, API/INV/REL afetados, `default` (nenhuma feature própria).

**Pré-condições:** diff apenas local, fixtures unitárias identificadas e efeito HTTP comprovadamente não ativado.

1. Para código sem comportamento externo, selecionar `cargo test -p e2e-user-journeys --bin e2e-user-journeys`.
2. Para mudança que só exige compilação, selecionar `cargo check -p e2e-user-journeys --bin e2e-user-journeys`.
3. Preservar target, features e saída integral; não substituir por `--all-features`.

**Saída esperada:** target bin passa; resultado não certifica rede, endpoint, provider ou deployment.

**Falhas e parada:** teste invoca endpoint, env inseguro ou efeito não isolado; pare antes de Cargo.

**Recuperação:** reverta diff de código local; nenhuma recuperação de estado remoto é presumida.

**Certificação:** review `REVIEWED`; execução `REVIEWED_NOT_EXECUTED`; resultado `NOT_EXECUTED` nesta tarefa documental; `required_for_acceptance=false` porque nenhum Rust foi alterado nesta entrega.

**Evidência:** este manual registra comando futuro, sem saída de build/test nesta autoria.

[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Executar jornada HTTP ou CLI

**Gatilho / resultado esperado:** validar contrato remoto autorizado; cada linha satisfaz assert e piso/teto.

**Modo / ambiente / permissão:** AUTHORIZED_OPERATION; tenant descartável, endpoints explícitos, PATs temporários e tools; nunca produção/customer sem autorização.

**Entradas:** endpoint/tenant e valores do provisionamento aprovado; conferir vars sem imprimir tokens. Localhost não garante isolamento.

**Pré-condições:** recovery dos writes, CLI PATH, fixture e quota conhecidos; nenhum dado de cliente.

1. Confirme `journeys::all`: 23 módulos, ordem fixa, sem `--journey` ou seletor. `cargo run -p e2e-user-journeys` roda todos; `RUN_SLOW` apenas gates caminhos lentos.
2. Habilite só opt-ins necessários (`QUOTA`, `DSR`, billing ou abuse), com fixtures autorizados; registre host redigido, tenant, vars, duração e cada PASS/FAIL/GATED.
3. Para seleção real, use harness/patch local revisado sem mascarar resultados. Tenant/dado inesperado exige parada, sem rerun.

**Saída esperada:** predicados de resposta observados e GATED explicados; um GREEN só respeita piso/teto configurados.

**Falhas e parada:** FAIL de auth/isolation, HTTP inesperado, timeout após mutação ou endpoint de produção; interrompa e reconcilie com owner.

**Recuperação:** API/CLI não possuem rollback uniforme; use cleanup documentado do serviço apenas quando aprovado.

**Certificação:** review `REVIEWED`; execução `BLOCKED_FOR_OPERATION`; resultado `NOT_EXECUTED`; `required_for_acceptance=false` nesta autoria documental.

**Evidência:** saída da execução e estado final do tenant, guardados sem PAT/session/secret.

[Índice de procedimentos](#m02)

<a id="proc-005"></a>

### PROC-005 — Isolar operação mutável

**Gatilho / resultado esperado:** teste DSR, drive de quota, webhook Stripe, convite ou wrapper real-client solicitado.

**Modo / ambiente / permissão:** AUTHORIZED_OPERATION, tenant/user throwaway e aprovação do owner operacional; revisão documental não concede essa autorização.

**Entradas e validação:** flags e fixtures exatas no código. `provision-and-run-suite.sh` pode chamar Clerk, criar users/keys, disparar DSR e executar suite contra endpoint remoto.

**Pré-condições:** consequência e recuperação não financeira/operacional documentadas; confirmar se email, cobrança, exclusão ou storage serão atingidos.

1. Confirme owner via OKF, host, tenant de teste, credenciais temporárias e limite da ação.
2. Inspecione a função antes de habilitar qualquer flag; faça uma execução delimitada.
3. Releia o estado externo com a interface autorizada e guarde evidência redigida.

**Saída esperada:** mutação confinada ao fixture e cleanup/recovery verificado.

**Falhas e parada:** recurso fora do tenant, cobrança, invite enviado inesperadamente ou DSR ambíguo; pare e escale pela rota real.

**Recuperação:** apagar user pode ser assíncrono; billing/convite/dados não têm undo geral. Git revert não compensa operação já feita.

**Certificação:** review `REVIEWED`; execução `BLOCKED_FOR_OPERATION`; resultado `NOT_EXECUTED`; `required_for_acceptance=false` para autoria documental.

**Evidência:** nenhum recurso/segredo de produção nesta entrega; operador autorizado registra antes/depois e cleanup.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Tipo de mudança | Package / target / features | Comando ou PROC | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| Identidade/workspace/source | package `e2e-user-journeys`; bin único; default/no feature | PROC-001; Git-scoped source comparison | Manifest, tree e membership iguais ao pin | Somente checkout; sem Cargo. |
| Dispatch composition | mesmo bin; 23 `run`; default/no feature | PROC-001; `rg` census de `out.extend` + `pub fn run` | Ambos os contadores são 23 e ordem source é preservada | Checkout local; nenhuma jornada executada. |
| Manifest/lock/dependency | package + root + `Cargo.lock` | PROC-006; `rg` lock-node e diff source-scoped | Nó lock corresponde a seis deps externas; zero first-party path edge | Somente Git/texto; regeneração não executada. |
| SBOM membership | workspace CycloneDX; sem target próprio | REL-027; revisar workflow/artefato SBOM | Package aparece quando a agregação resolve o workspace | Release/manual workflow; não observado nesta autoria. |
| Mutants quality lane | `.cargo/mutants.toml` + `.github/workflows/mutation-pr.yml`, `mutation-nightly.yml`, `nightly.yml` | REL-028; revisar `exclude_globs`, `--in-diff`, matrix e `--workspace` | Este package é excluído; não alegar kill-rate | Config/workflow lidos; nenhum lane executado. |
| Skill H | Docs / sem Rust target ou feature | PROC-002; `python3 "$OWNERSHIP_V13_ROOT/tools/check_docs.py" .claude/skills/own-e2e-user-journeys/SKILL.md --kind skill --profile H --root .` | `IMPLEMENTED_CHECKS_PASS` | Python local; variável aponta à extração v1.3 aprovada. |
| Reference H | Docs / sem Rust target ou feature | PROC-002; `python3 "$OWNERSHIP_V13_ROOT/tools/check_docs.py" docs/ownership/crates/e2e-user-journeys/REFERENCE.md --kind reference --profile H --root .` | `IMPLEMENTED_CHECKS_PASS` | Python local; mesma extração. |
| Blast H | Docs / sem Rust target ou feature | PROC-002; `python3 "$OWNERSHIP_V13_ROOT/tools/check_docs.py" docs/ownership/crates/e2e-user-journeys/BLAST_RADIUS.md --kind blast_radius --profile H --root .` | `IMPLEMENTED_CHECKS_PASS` | Python local; mesma extração. |
| Maintenance H | Docs / sem Rust target ou feature | PROC-002; `python3 "$OWNERSHIP_V13_ROOT/tools/check_docs.py" docs/ownership/crates/e2e-user-journeys/MAINTENANCE.md --kind maintenance --profile H --root .` | `IMPLEMENTED_CHECKS_PASS` | Python local; mesma extração. |
| Whitespace/scope | Docs / sem Rust target ou feature | PROC-002; `git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- .claude/skills/own-e2e-user-journeys/SKILL.md docs/ownership/crates/e2e-user-journeys/REFERENCE.md docs/ownership/crates/e2e-user-journeys/BLAST_RADIUS.md docs/ownership/crates/e2e-user-journeys/MAINTENANCE.md` | Sem saída, exit 0 | Local Git diff. |
| Lógica de runner/gate | `e2e-user-journeys` / bin / default sem feature | PROC-003; `cargo test -p e2e-user-journeys --bin e2e-user-journeys` | Casos ship-verdict preservam fail/floor/ceiling | Futuro ambiente isolado; não executado nesta autoria. |
| Harness/target compilation | `e2e-user-journeys` / bin / default sem feature | PROC-003; `cargo check -p e2e-user-journeys --bin e2e-user-journeys` | bin compila; não afirma alcance/runtime | Sem API acessível; não executado. |
| HTTP/OCI/protocol journey | `e2e-user-journeys` / bin / default sem feature | PROC-004; `cargo run -p e2e-user-journeys` | Resposta/corpo/hash/tenant assertados; piso/teto cumpridos | Endpoint + tenant descartável, PAT; coleta linhas e estado. Não rodar aqui. |
| DSR/quota/billing/team/provider | `e2e-user-journeys` / bin / default sem feature | PROC-005; comando de journey só após autorização | Mudança confinada e recuperação do fixture observada | Recursos dedicados; nenhum execução nesta autoria. |

**Negativos obrigatórios:** PAT/tenant ausente vira GATED; piso de zero PASS e teto excedido dão RED; hash errado não deve gravar; read-only, revoked, expired e cross-tenant devem negar.

**Gates globais compartilhados:** scripts Python B126/B270 consomem texto; wrapper real-client é operação separada. Workflows foram encontrados, mas nenhum invoca este bin por nome; mutation config exclui o package.

**Axiomas preservados:** (1) `[package].name` identifica; (2) target/test declarado não é execução; (3) dep/import não prova reachability runtime; (4) fake não prova provider; (5) modo anunciado no manifesto não autoriza operação. Nenhuma mutação DB/storage/customer pertence à autoria.

**Não executar indiscriminadamente:** `--all-features`, journey remoto, bursts, quota/DSR/billing e scripts de provisionamento.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| Documentos/código local | Sim | Sem efeito remoto | Reverter somente patch da tarefa | `git diff` limpo no path. |
| Resultado/env de runner | Parcial | Variáveis e saída local podem vazar credencial | Encerrar processo, limpar env temporário e redigir logs | Confirmar nenhum valor de token persistido. |
| CAS/AC/adapters/Turbo/OCI | Condicional | Writes, tags, cache ou eventos podem persistir | Usar tenant/repo descartável; cleanup pelo serviço autorizado se existir | Releitura do recurso e owner confirma ausência ou retenção conhecida. |
| PAT/key/team invite | Não uniformemente | Key revoke/convite pode emitir evento/email | Teste dedicado; use fluxo API autorizado para revogar quando aplicável | Confirmar key revogada/convite esperado; não há rollback do email. |
| Quota/billing | Condicional | Drive consome espaço/cap; webhook pode alterar subscription | Tenant Stripe/quota de teste; não use tenant customer | Ler billing/uso final e fazer reconciliação com owner. |
| DSR/user delete | Não | Solicitação/exclusão pode ser assíncrona/irreversível | Somente identidade descartável; wrapper usa cleanup trap mas não certifica conclusão | Verificar tombstone/DSR pelo caminho autorizado; não assumir restauração. |
| Runner/CLI temp files | Sim parcial | Processo pode gravar cache/config remoto e diretório temporário | Limpar apenas temp criado pela tarefa; preservar logs redigidos | Confirmar temp removido; remote state verificado separadamente. |

**Sem estado próprio:** o bin não declara banco/storage local próprio, mas envia escritas a serviços externos. Não há composição root recuperável pelo package; recovery fica no owner da API/provider e deve ser encontrado no OKF.
**Compatibilidade e limites:** mudança do format de output/exit pode afetar `provision-and-run-suite.sh`; protocolos e APIs exigem owner externo. Git revert não apaga objetos, desfaz billing, reverte invite/email nem restaura usuário deletado.

<a id="m06"></a>
## M06 — Escalonamento e registro de manutenção

| Condição | Responsável / rota verificada | Evidência mínima | Ação vedada |
|---|---|---|---|
| Semântica ou owner da rota desconhecido | Conceito/owner encontrado pelo [OKF](../../../internal/okf-wiki/concept-manifest.yaml); se ausente, registrar desconhecido | Path, símbolo, REL e resposta redigida | Atribuir owner pela semelhança do nome. |
| Drift de source pin, contagem ou manifest | Lead da wave para census/integration | Hashs Git e diff package/root manifest | Usar local `main` divergente ou atualizar source por rede. |
| Auth/tenant isolation, unexpected 2xx, cross-tenant data | Owner resolvido no OKF; parar operação | Request sem token, host redigido, status/corpo sem dado sensível | Repetir em prod ou consultar DB/storage. |
| Provider, billing, DSR ou convite mutável | Owner operacional conforme OKF e autorização explicitada | Fixture, flags, antes/depois e cleanup | Continuar se cleanup/recovery for incerto. |
| Static verifier quebra após mudança de adapter | Maintainer do script consumidor | Script/path afetado e diff | Alterar verificador silenciosamente ou alegar runtime. |

Após a tarefa: registrar baseline, modo, resultado real, unknowns e risco residual; limpar somente temporários criados nesta tarefa; reconciliar documentos/backlog com lead; enviar hashes alterados para cold review independente. Nenhum status aqui aprova ou autoriza integração.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
