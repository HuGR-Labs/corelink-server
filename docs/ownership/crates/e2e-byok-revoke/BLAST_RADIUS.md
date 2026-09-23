---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-byok-revoke
manifest: tests/e2e-byok-revoke/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-byok-revoke-static-source-20260921
---

# e2e-byok-revoke — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Impactos](#b05) · [Cobertura](#b06)

<a id="b01"></a>
## B01 — Escopo e leitura rápida

Este é o censo estático do package Cargo e2e-byok-revoke em 1177dad2ca2a9f21c29b5a118aa7944b77147798. Abrange sete declarações diretas, dois peers first-party, oito alvos de teste, membership e workflows workspace-wide. Fakes/helpers são código local; nenhuma operação BYOK real, execução de teste, seleção de target ou efeito de produção foi observado. O pin histórico cb94e251c0f17382565bf863f517945cbb2a84d6 é equivalente para os arquivos do package examinados; lockfile/workflows são lidos no pin atual. [OKF verificado](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) é somente rota.

**Builds avaliados:** nenhum; apenas declarações de manifesto, fonte e workflow foram lidas. **Ambientes não observados:** resolução Cargo por features/target, execução de CI, runtime, provider externo e consumidores fora deste checkout.

<a id="b02"></a>
## B02 — Método e populações

| População | Fontes / método | Resultado estático | Limite |
|---|---|---|---|
| Dependências | package manifest; workspace declarations; source imports | 7 diretas: 2 path first-party, 4 workspace normais, 1 workspace dev | Sem cargo metadata nem grafo resolvido |
| Inversas/test targets | workspace Cargo.toml literal search + 8 [[test]]/paths/imports | 0 package Cargo consumers inversos; separadamente, 8 alvos importam a library própria e corelink_byok | Não prova seleção/compilação nem caller externo |
| Workspace/CI | root Cargo.toml:276; rustfmt.yml, cas_foundation.yml, cargo-deny.yml, workspace-lint.yml, codeql.yml, nightly.yml, coverage.yml, mutation-pr.yml | membro; 11 relações workspace-wide (13 formas Cargo, incluindo 3 em coverage.sh), 1 inventory e 1 mutation diff-scoped | YAML/script declaram chamadas; nenhuma execução foi observada |
| Runtime/dados/contratos | helpers, alvos, detector source, workflow/env e prose references | caminhos locais e fronteiras estão enumerados | Sem tracing, bindings de produção ou repositórios externos |
| Inventário/candidatos fora da relação Cargo | Cargo.lock, .sbom/cyclonedx-rust.json, 10 prose/report/audit paths e guidance Cargo em welcome-first-pr.yml | 13 itens fora dos edges package/test/workflow enumerados; o workflow também contém `run` com `gh api` | lock/SBOM são inventário; texto de Cargo é prosa; API GitHub não é consumer Cargo |

Seleção/fingerprint: checkout de evidência indicado no cabeçalho; inspeção estática de manifesto, manifestos Cargo para inversos, diretórios tests/e2e-byok-revoke, raiz do workspace, workflows build/lint/format/policy/mutation/coverage, coverage.sh, lock/SBOM e buscas literais. `git grep` no baseline, em Cargo.toml fora do manifesto do package, achou somente a entrada de membership na raiz e zero consumers Cargo. cargo metadata/build/test/fetch/workflow não foram executados. Resolução e completude externa ficam UNKNOWN.

<a id="b03"></a>
## B03 — Relações diretas: registro completo

Cada card separa direção de dependência, fluxo de dados e propagação de impacto. Dados usam origem→destino; impacto usa mudança→fronteira afetada. `N/A` traz motivo; `bidirecional` nomeia as duas pernas. Um import/call não implica provider real nem runtime observado. REL-KEY usa o ID 1232040291.

| ID | Tipo / dependência | Superfície / endpoints | Ativação |
|---|---|---|---|
| [REL-001](#rel-001) | build-deploy / n.a. | root workspace → package | membro declarado |
| [REL-002](#rel-002) | dependency / package → corelink-byok | API e imports BYOK | dependência normal |
| [REL-003](#rel-003) | dependency / package → corelink-ops | MultiChannelAlerter | dependência normal |
| [REL-004](#rel-004) | dependency / package → async-trait | herdada workspace | normal |
| [REL-005](#rel-005) | dependency / package → serde_json | herdada workspace | normal |
| [REL-006](#rel-006) | dependency / package → thiserror | herdada workspace | normal |
| [REL-007](#rel-007) | dependency / package → tokio | herdada workspace | normal |
| [REL-008](#rel-008) | dependency / package → proptest | herdada workspace | dev |
| [REL-009](#rel-009) | runtime-call / helper → corelink-ops | CustomerAlertSink → MultiChannelAlerter | chamada local |
| [REL-010–017](#rel-010) | test / target → crate próprio | 8 imports, paths abaixo | compilação do alvo |
| [REL-018–025](#rel-018) | test / target → corelink-byok | 8 imports, paths abaixo | compilação do alvo |
| [REL-026](#rel-026) | build-deploy / workflow → workspace | CAS clippy | workspace-build-test |
| [REL-027](#rel-027) | build-deploy / workflow → workspace | CAS test | workspace-build-test |
| [REL-028](#rel-028) | build-deploy / workflow → workspace | CAS doc | workspace-build-test |
| [REL-029](#rel-029) | build-deploy / workflow → workspace | workspace lint clippy | gatilho declarado |
| [REL-030](#rel-030) | build-deploy / workflow → workspace | CodeQL cargo build | análise declarada |
| [REL-031](#rel-031) | build-deploy / workflow → inventário | CAS SBOM generator | inventory only |
| [REL-032](#rel-032) | reexport / package root → helpers module | root subset | compile-time surface |
| [REL-033](#rel-033) | runtime-call / happy target → BYOK detector method | run_one_cycle | chamada source-local |
| [REL-034](#rel-034) | build-deploy / rustfmt workflow → workspace | cargo fmt --all --check | push/PR/dispatch |
| [REL-035](#rel-035) | build-deploy / CAS workflow → workspace | cargo fmt --all -- --check | dispatch |
| [REL-036](#rel-036) | build-deploy / CAS workflow → workspace policy | cargo deny --all-features check | dispatch |
| [REL-037](#rel-037) | build-deploy / cargo-deny workflow → workspace policy | cargo deny --all-features check | PR/schedule/dispatch |
| [REL-038](#rel-038) | build-deploy / nightly workflow → workspace mutants | cargo mutants --workspace | schedule/dispatch |
| [REL-039](#rel-039) | build-deploy / coverage workflow → workspace coverage | coverage.sh / cargo llvm-cov | dispatch |
| [REL-040](#rel-040) | build-deploy / mutation PR workflow → diff lines | cargo mutants --in-diff | PR Rust paths |

Índice individual: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018) · [REL-019](#rel-019) · [REL-020](#rel-020)

Continuação do índice: [REL-021](#rel-021) · [REL-022](#rel-022) · [REL-023](#rel-023) · [REL-024](#rel-024) · [REL-025](#rel-025) · [REL-026](#rel-026) · [REL-027](#rel-027) · [REL-028](#rel-028) · [REL-029](#rel-029) · [REL-030](#rel-030) · [REL-031](#rel-031) · [REL-032](#rel-032) · [REL-033](#rel-033) · [REL-034](#rel-034) · [REL-035](#rel-035) · [REL-036](#rel-036) · [REL-037](#rel-037) · [REL-038](#rel-038) · [REL-039](#rel-039) · [REL-040](#rel-040)

<a id="rel-001"></a>
### REL-001 — Membro do workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-workspace; root→package.
**Dependência:** N/A (membership, não package edge). **Dados:** root Cargo.toml→grafo de membros. **Impacto:** mudança no workspace root→descoberta/seleção do package.
**Superfície:** Cargo.toml:276, tests/e2e-byok-revoke.
**Ativação:** resolução do workspace root; target/feature efetivo UNKNOWN.

**Contrato:** caminho identifica membro Cargo, não publicação (publish=false).
**Estado/efeito:** somente metadado de build; sem runtime local.
**Falha/propagação:** path ausente/referência inválida afeta descoberta do workspace.
**Contenção:** termina no grafo de workspace; consumidores externos UNKNOWN.
**Validação:** fonte estática; cargo metadata não executado; falsificador: entrada removida.
**Coordenação/evidência:** root Cargo.toml:276; manter identidade do manifesto; SOURCE. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Dependência normal BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-corelink-byok-dep; package→corelink-byok.
**Dependência:** package→corelink-byok. **Dados:** APIs/tipos BYOK→helper e alvos locais. **Impacto:** mudança de contrato BYOK→compile/chamadas do harness.
**Superfície:** manifest path ../../crates/corelink-byok; imports em helper e 8 alvos.
**Ativação:** dependência normal declarada; features/target resolvidos UNKNOWN.

**Contrato:** traits, types e cache consumidos conforme REFERENCE.md#r02; sem claim de execução real.
**Estado/efeito:** build edge; efeito runtime local depende de fake/harness.
**Falha/propagação:** API incompatível impede compile do consumidor selecionado; propagation além dele UNKNOWN.
**Contenção:** para na fronteira do package e nos imports listados.
**Validação:** manifest/source census; sem resolution/build; falsificador: path/dependency removida.
**Coordenação/evidência:** manifest e src/helpers.rs; owner BYOK /crates/corelink-byok/ @gmhelmold; SOURCE. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Dependência normal Ops
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-corelink-ops-dep; package→corelink-ops.
**Dependência:** package→corelink-ops. **Dados:** helper payload/config→API local Ops. **Impacto:** mudança na API Ops→compile/resultado do sink local.
**Superfície:** manifest path ../../crates/corelink-ops; CustomerAlertSink em src/helpers.rs.
**Ativação:** dependência normal; feature/target resolvidos UNKNOWN.

**Contrato:** MultiChannelAlerter e AlerterConfig; configuração local é default/stub.
**Estado/efeito:** somente sink local após chamada; não implica entrega a cliente.
**Falha/propagação:** erro de API pode quebrar build/chamada local; caller externo UNKNOWN.
**Contenção:** fronteira encerra em corelink-ops; nenhum canal real aferido.
**Validação:** manifest/source estáticos; build não executado; falsificador: import/dependência ausente.
**Coordenação/evidência:** helper lines 461–478; CODEOWNERS usa fallback * @gmhelmold; SOURCE. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Dependência de workspace async-trait
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-async-trait; package→workspace crate.
**Dependência:** package→async-trait. **Dados:** tokens async do package→macro→impl gerada local. **Impacto:** incompatibilidade da macro→build dos itens afetados.
**Superfície:** manifest [dependencies].async-trait = { workspace = true }.
**Ativação:** dependência normal herdada; alias/features/resolução UNKNOWN.

**Contrato:** versão/feature advém da declaração root; grafo não coletado.
**Estado/efeito:** compile-time somente conforme declaração.
**Falha/propagação:** resolução ou expansão incompatível pode falhar no alvo; runtime UNKNOWN.
**Contenção:** boundary externa ao package; sem inventário transitivo.
**Validação:** leitura manifest; sem Cargo; falsificador: declaração removida/trocada.
**Coordenação/evidência:** manifest e root workspace; SOURCE; sem owner externo verificado. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Dependência de workspace serde_json
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-serde-json; package→workspace crate.
**Dependência:** package→serde_json. **Dados:** fixtures/valores harness→API/Value JSON. **Impacto:** mudança de API/serialização→compile/valor local do harness.
**Superfície:** manifest [dependencies].serde_json = { workspace = true }.
**Ativação:** dependência normal herdada; features/target/resolução UNKNOWN.

**Contrato:** versão e features vêm da raiz; grafo não foi coletado.
**Estado/efeito:** serialização local em source; nenhum contrato de wire externo declarado aqui.
**Falha/propagação:** API incompatível pode falhar compilation; alcance runtime UNKNOWN.
**Contenção:** encerra na fronteira declarada.
**Validação:** leitura manifest/source; sem Cargo; falsificador: declaração/uso removido.
**Coordenação/evidência:** manifest e usos em helper; SOURCE; owner upstream não verificado. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Dependência de workspace thiserror
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-thiserror; package→workspace crate.
**Dependência:** package→thiserror. **Dados:** atributos/enum local→derive macro→tokens `RevokeError`. **Impacto:** mudança incompatível no derive→build local.
**Superfície:** manifest [dependencies].thiserror = { workspace = true }.
**Ativação:** dependência normal herdada; features/resolução UNKNOWN.

**Contrato:** versão/feature conforme workspace root; resolução não executada.
**Estado/efeito:** compile-time no tipo RevokeError.
**Falha/propagação:** derive incompatível pode impedir build; sem chamada externa provada.
**Contenção:** boundary dependency; transitivos excluídos do domínio.
**Validação:** static manifest/source; sem Cargo; falsificador: declaração/derive removido.
**Coordenação/evidência:** manifest e src/helpers.rs; SOURCE; owner upstream UNKNOWN. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Dependência de workspace tokio
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-tokio; package→workspace crate.
**Dependência:** package→tokio. **Dados:** futuros/chamadas harness→runtime Tokio; resultados→alvos. **Impacto:** API/features Tokio→build/test async local.
**Superfície:** manifest [dependencies].tokio = { workspace = true }.
**Ativação:** dependência normal herdada; features selecionadas UNKNOWN.

**Contrato:** versão/features determinadas no workspace; sem grafo resolvido.
**Estado/efeito:** harness usa concorrência/async local; runtime executado UNKNOWN.
**Falha/propagação:** feature ausente/incompatível pode impedir alvo; efeito runtime não observado.
**Contenção:** fronteira do dependency declarado.
**Validação:** static manifest/source; sem Cargo; falsificador: declaração/import alterado.
**Coordenação/evidência:** manifest e tests; SOURCE; upstream owner não verificado. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Dependência dev proptest
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-proptest; package→workspace crate.
**Dependência:** package (dev)→proptest. **Dados:** estratégia alvo→proptest; casos gerados→assertions do alvo. **Impacto:** API/feature proptest→build/execução do property target.
**Superfície:** manifest [dev-dependencies].proptest = { workspace = true }; prop_fail_closed.rs.
**Ativação:** dev target; features/resolução UNKNOWN.

**Contrato:** versão/features advêm da raiz; PROPTEST_CASES é controle de teste.
**Estado/efeito:** geração aleatória local, não executada nesta autoria.
**Falha/propagação:** resolução incompatível falha test build; não é edge de produção.
**Contenção:** dev dependency; transitivos não recopiados.
**Validação:** source declaration only; nenhum property test; falsificador: dep/target removido.
**Coordenação/evidência:** manifest e tests/prop_fail_closed.rs; SOURCE; upstream owner UNKNOWN. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Helper chama o stub Ops
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ops-alert-call; helper→corelink-ops.
**Dependência:** helper→API MultiChannelAlerter (runtime-call). **Dados:** payload/args helper→Ops; resultado/erro Ops→sink. **Impacto:** erro/contrato Ops→resultado do runner local.
**Superfície:** CustomerAlertSink::{new,alert,alert_recovery} → MultiChannelAlerter.
**Ativação:** métodos do sink local são chamados; alvo/runtime efetivos UNKNOWN.

**Contrato:** usa AlerterConfig::default; assert source-visible exige stub_mode.
**Estado/efeito:** alerta é registrado em vetor local apenas após sucesso interno.
**Falha/propagação:** RevocationError propaga; envio real/customer delivery não demonstrado.
**Contenção:** local stub; não atravessa para canais/provider comprovados.
**Validação:** helper source lines 456–551; sem execução; falsificador: remoção da chamada.
**Coordenação/evidência:** corelink-ops route em REFERENCE.md#r02; SOURCE. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Teste happy importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-happy; alvo→library local.
**Dependência:** happy target→library local. **Dados:** chamadas/inputs target→helpers; bundle/events/helpers→assertions target. **Impacto:** API/fixture library→compile/resultado deste target.
**Superfície:** happy_revoke_flow.rs: helpers::* e setup_byok_env.
**Ativação:** alvo manifestado; seleção normal/resolvida UNKNOWN.

**Contrato:** imports root/helper; detector smoke usa default source separado.
**Estado/efeito:** fixture local somente; sem execução observada.
**Falha/propagação:** rename/API break impede compilação deste alvo.
**Contenção:** termina no alvo; falhas não provam produção.
**Validação:** manifest/import source; teste não rodado; falsificador: import removido.
**Coordenação/evidência:** tests/happy_revoke_flow.rs:33–36; SOURCE. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Teste recovery importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-recovery; alvo→library local.
**Dependência:** recovery target→library local. **Dados:** chamadas/inputs target→helpers; estado/eventos→assertions target. **Impacto:** API/fixture library→compile/resultado deste target.
**Superfície:** recovery_flow.rs: make_wrapped_for, KillSwitchRunner, KmsBehaviour, setup.
**Ativação:** alvo declarado; seleção não coletada.

**Contrato:** recovery encena estados com fixtures do crate.
**Estado/efeito:** memória local; execution UNKNOWN.
**Falha/propagação:** import/signature change pode quebrar compile; não cobre runtime real.
**Contenção:** teste individual e harness.
**Validação:** import census estático; sem Cargo/test; falsificador: import removido.
**Coordenação/evidência:** tests/recovery_flow.rs:24–25; SOURCE. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Teste stampede importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-stampede; alvo→library local.
**Dependência:** stampede target→library local. **Dados:** setup/ações target→helpers; estado/contagens→assertions target. **Impacto:** API/fixture library→compile/resultado deste target.
**Superfície:** adversarial_stampede.rs: helpers::* e setup_byok_env.
**Ativação:** alvo declarado; seleção desconhecida.

**Contrato:** teste exercita cenário de fake conforme assertions source.
**Estado/efeito:** local; não valida produção/cache real.
**Falha/propagação:** import mismatch pode impedir compile; runtime não observado.
**Contenção:** crate test target.
**Validação:** manifest/source only; sem execução; falsificador: import apagado.
**Coordenação/evidência:** tests/adversarial_stampede.rs:23–26; SOURCE. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Teste race importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-race; alvo→library local.
**Dependência:** race target→library local. **Dados:** chamadas/fixtures target→helpers; respostas/estado→assertions target. **Impacto:** API/fixture library→compile/resultado deste target.
**Superfície:** adversarial_race_condition.rs: make_wrapped_for, runner, behavior, setup.
**Ativação:** alvo declarado; seleção UNKNOWN.

**Contrato:** fake/status local; nenhuma prova de corrida em provider real.
**Estado/efeito:** memória de harness; runtime não observado.
**Falha/propagação:** contrato importado quebrado impede teste.
**Contenção:** alvo isolado; não extrapolar ao workspace todo.
**Validação:** static import inspection; sem Cargo/test.
**Coordenação/evidência:** tests/adversarial_race_condition.rs:36–37; SOURCE. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Teste transient importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-transient; alvo→library local.
**Dependência:** transient target→library local. **Dados:** cenários/chamadas target→helpers; status/eventos→assertions target. **Impacto:** API/fixture library→compile/resultado deste target.
**Superfície:** adversarial_transient_api_error.rs: runner, make_wrapped_for, behavior, setup.
**Ativação:** alvo declarado; target resolvido UNKNOWN.

**Contrato:** branch fake/transient conforme source, sem execução.
**Estado/efeito:** local; não confirma política de detector real.
**Falha/propagação:** helper import ou signature break bloqueia alvo.
**Contenção:** harness; production caller UNKNOWN.
**Validação:** static inspection only; falsificador: import alterado.
**Coordenação/evidência:** tests/adversarial_transient_api_error.rs:28–29; SOURCE. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Property target importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-property; alvo→library local.
**Dependência:** property target→library local. **Dados:** cenário gerado/chamadas→helpers; resultados→assertions. **Impacto:** API/fixture library→compile/resultado deste target.
**Superfície:** prop_fail_closed.rs: event/status types, helpers e setup.
**Ativação:** dev test target; número de casos pode ser env override.

**Contrato:** propriedade source-level; nenhum caso executado.
**Estado/efeito:** estado gerado no teste; não prova comportamento universal.
**Falha/propagação:** import/contract break impede build do alvo.
**Contenção:** property test e fixtures; runtime externo UNKNOWN.
**Validação:** source census; sem Cargo/proptest.
**Coordenação/evidência:** tests/prop_fail_closed.rs:39–42; SOURCE. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Matrix target importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-matrix; alvo→library local.
**Dependência:** matrix target→library local. **Dados:** setup/fixtures target→helpers; resultados→assertions target. **Impacto:** export/API library→compile/resultado deste target.
**Superfície:** multi_provider_matrix.rs: helpers e setup.
**Ativação:** alvo declarado; não prova provider matrix executada.

**Contrato:** nomes de provider são labels; provider implementation é fake no inspected body.
**Estado/efeito:** fake local; sem remote operations.
**Falha/propagação:** helper import failure limita alvo.
**Contenção:** target; produção fora do limite.
**Validação:** static imports; sem execução.
**Coordenação/evidência:** tests/multi_provider_matrix.rs:25–28; SOURCE. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Gated target importa harness
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-live-label; alvo→library local.
**Dependência:** gated target→library local. **Dados:** imports/setup target→helpers; resultados locais→assertions. **Impacto:** export/API library→compile/resultado deste target.
**Superfície:** live_provider_gated.rs: helpers/setup.
**Ativação:** alvo contém item normal e quatro #[ignore]; nenhum foi executado aqui.

**Contrato:** corpos inspecionados verificam env mas usam setup/fake; não instanciam provider real.
**Estado/efeito:** fake local; env values unused conforme source.
**Falha/propagação:** import break limita alvo; live behavior UNKNOWN.
**Contenção:** nenhum credential/network/provider operation autorizado.
**Validação:** static source only; falsificador: helper use removido.
**Coordenação/evidência:** tests/live_provider_gated.rs:30–35; SOURCE. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Happy target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-happy; alvo→corelink-byok.
**Dependência:** happy target→corelink-byok. **Dados:** target→BYOK calls/types; statuses/results→target assertions. **Impacto:** BYOK contract change→target compile/semantics.
**Superfície:** revocation imports e Dek, KmsProvider, provider kind.
**Ativação:** alvo declarado; resolved features UNKNOWN.

**Contrato:** testes constroem interfaces/trait objects; detector default smoke é distinta do runner fake.
**Estado/efeito:** source seam somente; sem provider real.
**Falha/propagação:** API change afeta este target; external callers UNKNOWN.
**Contenção:** depende em corelink-byok, sem propagar além do peer.
**Validação:** happy_revoke_flow.rs:28–36,134–135; SOURCE; no test.
**Coordenação/evidência:** BYOK CODEOWNERS route; consumer local enumerado. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Recovery target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-recovery; alvo→corelink-byok.
**Dependência:** recovery target→corelink-byok. **Dados:** target→status/trait calls; responses→target assertions. **Impacto:** BYOK contract change→target compile/semantics.
**Superfície:** revocation API, Dek, KmsProviderKind.
**Ativação:** alvo declarado; seleção e features UNKNOWN.

**Contrato:** fake-driven test source; sem recovery externa provada.
**Estado/efeito:** fixture local; runtime UNKNOWN.
**Falha/propagação:** API break afeta compile/alvo.
**Contenção:** package peer only.
**Validação:** recovery_flow.rs:19–24; SOURCE; not executed.
**Coordenação/evidência:** /crates/corelink-byok/ @gmhelmold route. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — Stampede target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-stampede; alvo→corelink-byok.
**Dependência:** stampede target→corelink-byok. **Dados:** target→DEK/kind API; returned values→target. **Impacto:** BYOK API change→target compile/semantics.
**Superfície:** Dek, KmsProviderKind; comentário também menciona limite cache.
**Ativação:** alvo declarado; feature resolution UNKNOWN.

**Contrato:** comentário sobre limite não é prova de valor ou comportamento.
**Estado/efeito:** local fake/source; sem cache runtime claim.
**Falha/propagação:** import break pode impedir target compile.
**Contenção:** peer API; limite transitive unknown.
**Validação:** adversarial_stampede.rs:22; source only, no execution.
**Coordenação/evidência:** BYOK owner route; falsifier: imports removidos. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Race target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-race; alvo→corelink-byok.
**Dependência:** race target→corelink-byok. **Dados:** target↔BYOK (trait calls/inputs e error/status outputs); bidireção é chamada/retorno. **Impacto:** contract change→target compile/semantics.
**Superfície:** BYOKError, Dek, KmsProvider, KmsProviderKind.
**Ativação:** alvo declarado; seleção UNKNOWN.

**Contrato:** interfaces do peer usadas para fake local; nenhum provider real.
**Estado/efeito:** fake/local only.
**Falha/propagação:** compile/API failure localizado no target.
**Contenção:** corelink-byok boundary; além disso UNKNOWN.
**Validação:** adversarial_race_condition.rs:35–37; SOURCE.
**Coordenação/evidência:** BYOK CODEOWNERS route; sem teste executado. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Transient target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-transient; alvo→corelink-byok.
**Dependência:** transient target→corelink-byok. **Dados:** target→DEK/status API; results→target assertions. **Impacto:** BYOK API change→target compile/semantics.
**Superfície:** Dek, KmsAccessStatus, KmsProviderKind.
**Ativação:** alvo declarado; resolution UNKNOWN.

**Contrato:** fake transient statuses source-level only.
**Estado/efeito:** local fake; sem chamada provider.
**Falha/propagação:** API incompatível pode bloquear alvo.
**Contenção:** peer boundary; detector externo UNKNOWN.
**Validação:** adversarial_transient_api_error.rs:27–29; no test.
**Coordenação/evidência:** BYOK owner route; SOURCE. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Property target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-property; alvo→corelink-byok.
**Dependência:** property target→corelink-byok. **Dados:** generated scenario target→BYOK types; observed results→target property. **Impacto:** BYOK API change→property target compile/semantics.
**Superfície:** revocation event, TenantByokStatus, Dek, KmsAccessStatus, provider kind.
**Ativação:** dev target declarado; PROPTEST_CASES lido pelo source.

**Contrato:** property definition declarada; não provada por execução.
**Estado/efeito:** gerado em processo de teste se executado; não neste trabalho.
**Falha/propagação:** incompatibilidade atinge o alvo.
**Contenção:** dev test / peer API.
**Validação:** prop_fail_closed.rs:37–42; SOURCE.
**Coordenação/evidência:** BYOK owner route; run NOT_EXECUTED. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Matrix target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-matrix; alvo→corelink-byok.
**Dependência:** matrix target→corelink-byok. **Dados:** target→provider/type APIs; values/statuses→target assertions. **Impacto:** BYOK API change→matrix target compile/semantics.
**Superfície:** revocation API, Dek, provider kind, FipsLevel, KmsProvider.
**Ativação:** alvo declarado; source nested imports também examinados.

**Contrato:** provider matrix usa labels/fake; não prova adapters.
**Estado/efeito:** harness local apenas.
**Falha/propagação:** peer API break afeta matrix target.
**Contenção:** target; provider runtime UNKNOWN.
**Validação:** multi_provider_matrix.rs:20–25,82; no execution.
**Coordenação/evidência:** BYOK CODEOWNERS route; SOURCE. [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — Gated target importa BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-test-byok-live; alvo→corelink-byok.
**Dependência:** gated target→corelink-byok. **Dados:** KmsProviderKind/type definition→target (compile-time import only; runtime value UNKNOWN). **Impacto:** API change→target compile; runtime/provider remains UNKNOWN.
**Superfície:** KmsProviderKind em live_provider_gated.rs.
**Ativação:** alvo declarado com 4 ignored bodies; env gate não ativa execução por esta autoria.

**Contrato:** inspected bodies seguem fake setup, não instanciam adapter real.
**Estado/efeito:** nenhum provider/network action executado.
**Falha/propagação:** import/API break afeta alvo; live behavior UNKNOWN.
**Contenção:** no credentials, endpoint or external provider authority.
**Validação:** live_provider_gated.rs:30; static only.
**Coordenação/evidência:** BYOK route; four ignored cases NOT_EXECUTED. [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — CAS foundation clippy workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-cas-clippy; workflow→workspace targets.
**Dependência:** CAS workflow→workspace Cargo targets. **Dados:** workspace source/manifests→clippy; exit status→job. **Impacto:** source/lint change→workspace check result.
**Superfície:** cas_foundation.yml workspace-build-test; cargo clippy --workspace --all-targets -- -D warnings.
**Ativação:** workflow_dispatch per declared trigger; no PR trigger asserted.

**Contrato:** command declaration, not evidence of run/result.
**Estado/efeito:** CI build process; no product runtime state.
**Falha/propagação:** lint error fails job; actual target resolution UNKNOWN.
**Contenção:** workflow job/workspace scope.
**Validação:** YAML read; not executed; falsifier: job command changed.
**Coordenação/evidência:** .github/workflows/cas_foundation.yml:134–162; SOURCE. [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — CAS foundation test workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-cas-test; workflow→workspace tests.
**Dependência:** CAS workflow→workspace tests. **Dados:** source/tests→test process; exit status→job. **Impacto:** workspace test change/failure→CI job result.
**Superfície:** cargo test --workspace --all-targets in CAS workspace job.
**Ativação:** workflow_dispatch; no execution observed.

**Contrato:** declaration only; target/feature resolution/runtime unknown.
**Estado/efeito:** CI process would execute tests if scheduled; not this author.
**Falha/propagação:** test failure would fail job; source result UNKNOWN.
**Contenção:** workspace job; does not prove production behavior.
**Validação:** static workflow line 164; command not run.
**Coordenação/evidência:** .github/workflows/cas_foundation.yml; SOURCE. [Relation index](#b03)

<a id="rel-028"></a>
### REL-028 — CAS foundation doc workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-cas-doc; workflow→workspace docs.
**Dependência:** CAS workflow→workspace docs. **Dados:** Rust docs/source→rustdoc; diagnostics/status→job. **Impacto:** source/doc warning change→CI doc result.
**Superfície:** cargo doc --no-deps --workspace in workspace-build-test.
**Ativação:** workflow_dispatch; no run observed.

**Contrato:** workflow command only; selected package docs UNKNOWN.
**Estado/efeito:** CI artifact/process, not production runtime.
**Falha/propagação:** doc failure job effect; not observed.
**Contenção:** workspace doc job.
**Validação:** YAML line 169 static; no Cargo.
**Coordenação/evidência:** .github/workflows/cas_foundation.yml; SOURCE. [Relation index](#b03)

<a id="rel-029"></a>
### REL-029 — Workspace lint declaration
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-workspace-lint; workflow→workspace targets.
**Dependência:** workspace-lint workflow→workspace targets. **Dados:** source/manifests→clippy; status→job. **Impacto:** lint/source change→workflow result.
**Superfície:** cargo clippy --workspace --all-targets --message-format=short -- -D warnings.
**Ativação:** push main Rust/Cargo paths; workflow_dispatch; PR edits to this workflow.

**Contrato:** YAML trigger and command only; not run evidence.
**Estado/efeito:** lint job; no runtime state.
**Falha/propagação:** clippy failure blocks job; actual package selection UNKNOWN.
**Contenção:** workflow as declared.
**Validação:** .github/workflows/workspace-lint.yml:35–42,58–73; SOURCE.
**Coordenação/evidência:** static workflow; no CI claimed. [Relation index](#b03)

<a id="rel-030"></a>
### REL-030 — CodeQL workspace build declaration
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-codeql; workflow→workspace targets.
**Dependência:** CodeQL workflow→workspace targets. **Dados:** source/build output→CodeQL analysis job; status→workflow. **Impacto:** workspace compile change→build/analysis result.
**Superfície:** cargo build --workspace --all-targets --locked.
**Ativação:** CodeQL workflow events as declared; no run observed.

**Contrato:** static command declaration, not successful build or scanned target evidence.
**Estado/efeito:** analysis build process only.
**Falha/propagação:** build failure can stop analysis; runtime implication none.
**Contenção:** declared workspace build; resolved graph unknown.
**Validação:** .github/workflows/codeql.yml:207–214; not run.
**Coordenação/evidência:** SOURCE; no claim of CodeQL result. [Relation index](#b03)

<a id="rel-031"></a>
### REL-031 — CAS SBOM generation
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-sbom; workflow→package inventory.
**Dependência:** CAS SBOM job→workspace inventory. **Dados:** workspace manifests/lock→SBOM artifact. **Impacto:** manifest/metadata change→inventory content/job result.
**Superfície:** cas_foundation.yml SBOM job; cargo cyclonedx --format json --spec-version 1.5 --all.
**Ativação:** workspace SBOM job follows workspace-build-test declaration.

**Contrato:** all workspace members inventory; not package compilation/test or caller.
**Estado/efeito:** emits SBOM artifacts; no runtime behavior.
**Falha/propagação:** inventory generation/validation failure affects workflow job only.
**Contenção:** ends at inventory artifact; does not execute BYOK operation.
**Validação:** workflow lines 197–233; command not run; SOURCE.
**Coordenação/evidência:** .github/workflows/cas_foundation.yml; distinguish inventory from consumer. [Relation index](#b03)

<a id="rel-032"></a>
### REL-032 — Root public reexport
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-root-helpers; crate root→helpers module.
**Dependência:** crate root→public helpers module. **Dados:** exported names module→root consumers. **Impacto:** export/module change→affected consumer imports/compile.
**Superfície:** pub mod helpers and selective root pub use in src/lib.rs:56–65.
**Ativação:** Rust compilation when crate/test consumer is selected.

**Contrato:** helpers module public; only eight listed symbols reexported at root in REFERENCE API-001.
**Estado/efeito:** name resolution only; no runtime call or data flow.
**Falha/propagação:** remove/rename breaks consumers using affected path; exact external set UNKNOWN.
**Contenção:** source surface; no claim that helpers or peer APIs are root-reexported.
**Validação:** inspect pub module/use lines; no compile; falsifier: export list changes.
**Coordenação/evidência:** src/lib.rs; consumers REL-010–017; SOURCE. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — Happy alvo chama ciclo do detector BYOK
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-happy-run-one-cycle; happy target→método BYOK.
**Dependência:** happy test→BYOK detector method (call edge). **Dados:** local fake inputs→detector; result/error→test. **Impacto:** method/API change→smoke target compile/result; production reachability N/A (test-only edge).
**Superfície:** happy_revoke_flow.rs:129–141 → RevocationDetector::run_one_cycle.
**Ativação:** somente se alvo/teste executar; nenhum foi executado.

**Contrato:** `new` usa EmptyActiveByokKeySource; lista vazia evita checks por chave. Não é reachability de produção.
**Estado/efeito:** wiring local passa fake, store/memória e NoopAlerter; ciclo source-level sem operação BYOK.
**Falha/propagação:** erro de listagem/ciclo retorna ao teste; estado externo UNKNOWN.
**Contenção:** detector/source fake local; scheduler de produção exige source configurada.
**Validação:** source only; falsificador: remover chamada/configuração default; sem execução.
**Coordenação/evidência:** tests/e2e-byok-revoke/tests/happy_revoke_flow.rs:129–141; crates/corelink-byok/src/byok_revocation/detector.rs:162–176,224–236,510–523; SOURCE. [Relation index](#b03)

<a id="rel-034"></a>
### REL-034 — Rustfmt workflow verifica workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-rustfmt; workflow→workspace formatting.
**Dependência:** rustfmt workflow→workspace. **Dados:** workspace files→formatter; exit status→job. **Impacto:** formatting/config change→workspace gate result.
**Superfície:** `cargo fmt --all --check` em rustfmt.yml:104–105.
**Ativação:** push main/PR com paths declarados ou dispatch; execução UNKNOWN.

**Contrato:** comando verifica workspace formatado; não compila nem prova runtime.
**Estado/efeito:** job declarativo, sem estado de produto.
**Falha/propagação:** formato divergente falha job; runner/resultados UNKNOWN.
**Contenção:** gate/workspace; nenhuma ação de release.
**Validação:** YAML estático; falsificador: comando/paths removidos; não executado.
**Coordenação/evidência:** .github/workflows/rustfmt.yml:44–60,70,104–105; SOURCE. [Relation index](#b03)

<a id="rel-035"></a>
### REL-035 — CAS workflow verifica formato workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-cas-fmt; workflow→workspace formatting.
**Dependência:** CAS fmt workflow→workspace. **Dados:** workspace files→formatter; exit status→job. **Impacto:** formatting/source change→CAS job result.
**Superfície:** workspace-build-test `cargo fmt --all -- --check`.
**Ativação:** workflow_dispatch; schedule semanal comentado/parked; execução UNKNOWN.

**Contrato:** declaração de formato para todo workspace; sem build/runtime claim.
**Estado/efeito:** processo CI declarado; sem alteração de produto.
**Falha/propagação:** formato divergente falharia step/job; resultado UNKNOWN.
**Contenção:** job CAS; não executado nesta revisão.
**Validação:** .github/workflows/cas_foundation.yml:92–98,134,157–158; SOURCE.
**Coordenação/evidência:** comando no YAML; gatilho ativo é manual; SOURCE. [Relation index](#b03)

<a id="rel-036"></a>
### REL-036 — CAS workflow verifica política Cargo
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-cas-deny; workflow→workspace policy.
**Dependência:** CAS policy job→workspace dependency metadata. **Dados:** manifest/lock/features→cargo-deny; verdict→job. **Impacto:** metadata/policy change→CAS policy result.
**Superfície:** CAS `cargo deny --all-features check` em cargo-deny job.
**Ativação:** workflow_dispatch; schedule está comentado/parked; resultado UNKNOWN.

**Contrato:** regra de policy gate declarada, não consumo runtime nem resultado aprovado.
**Estado/efeito:** valida dependências na execução futura; sem estado produto local.
**Falha/propagação:** violação/resolução falha job; grafo resolvido UNKNOWN.
**Contenção:** workflow CAS; comando não executado.
**Validação:** .github/workflows/cas_foundation.yml:92–98,175–193; SOURCE.
**Coordenação/evidência:** gate de dependências workspace; SOURCE, sem CI. [Relation index](#b03)

<a id="rel-037"></a>
### REL-037 — Workflow cargo-deny verifica política workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-cargo-deny; workflow→dependency policy.
**Dependência:** cargo-deny workflow→workspace dependency metadata. **Dados:** manifest/lock/features→cargo-deny; verdict→job. **Impacto:** metadata/policy change→standalone gate result.
**Superfície:** `cargo deny --all-features check --hide-inclusion-graph`.
**Ativação:** PR paths declarados, cron `20 6 * * *` e dispatch; sem run observado.

**Contrato:** política de dependências, não caller executável do package.
**Estado/efeito:** gate declarativo; pacote/gráfico efetivo UNKNOWN.
**Falha/propagação:** advisory/license/source/ban ou resolução pode falhar job.
**Contenção:** workflow policy; nada foi instalado ou consultado nesta autoria.
**Validação:** .github/workflows/cargo-deny.yml:20–52,124–132; SOURCE.
**Coordenação/evidência:** check all-features declarado; não executar ou alegar PASS. [Relation index](#b03)

<a id="rel-038"></a>
### REL-038 — Nightly mutants declara escopo workspace
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-nightly-mutants; workflow→workspace targets.
**Dependência:** nightly workflow→workspace mutation targets. **Dados:** workspace source/tests→mutants runs; results→job. **Impacto:** surviving mutant/test change→nightly result.
**Superfície:** `cargo mutants --workspace --no-shuffle --minimum-test-timeout=600`.
**Ativação:** cron `23 4 * * *` ou dispatch; nenhuma execução afirmada.

**Contrato:** alvo de mutations é workspace inteiro, distinto de mutation-nightly por package matrix.
**Estado/efeito:** CI executaria testes por mutants se acionado; nenhum teste aqui.
**Falha/propagação:** survivor/test failure pode falhar job; targets efetivos UNKNOWN.
**Contenção:** nightly workflow; não executar/despachar CI.
**Validação:** .github/workflows/nightly.yml:28–33,443–490; SOURCE.
**Coordenação/evidência:** verificar declarativo somente; falsificador: command/scope mudado. [Relation index](#b03)

<a id="rel-039"></a>
### REL-039 — Coverage workflow chama script workspace-wide
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-coverage; workflow→script→workspace.
**Dependência:** coverage workflow→script→workspace. **Dados:** source/tests→coverage runner→report artifact. **Impacto:** source/tool/config change→report/job result.
**Superfície:** coverage.yml `bash scripts/coverage.sh`; script invoca clean, workspace HTML e summary.
**Ativação:** workflow_dispatch; schedule semanal comentado/parked; sem PR/push.

**Contrato:** defaults `--workspace --no-default-features`; `COV_FLAGS` e `COV_OUT` configuráveis (`target/coverage` default); clean muta dados locais de cobertura.
**Estado/efeito:** gera target/coverage HTML/SUMMARY; não muda estado BYOK nem prova cobertura atingida.
**Falha/propagação:** tool ausente/coverage failure retorna falha do script/job.
**Contenção:** output no workspace efêmero/artefato CI; não executado aqui.
**Validação:** coverage.yml:24–55,68–100; coverage.sh:60–85; SOURCE, sem comando rodado.
**Coordenação/evidência:** script default chama 3 formas Cargo; resultado/targets resolvidos UNKNOWN. [Relation index](#b03)

<a id="rel-040"></a>
### REL-040 — Mutation PR limita execução ao diff
**Identidade:** repo:1232040291:boundary:e2e-byok-revoke-ci-mutation-pr; workflow→diff lines.
**Dependência:** mutation PR workflow→diff-scoped Cargo mutant job. **Dados:** PR diff→mutants; results→gate. **Impacto:** changed source/test mutation outcome→PR check; package targets UNKNOWN.
**Superfície:** `cargo mutants --in-diff pr.diff --test-tool nextest --no-shuffle --output . -j 2`.
**Ativação:** pull_request com paths Rust/Cargo/lock; nenhum run observado.

**Contrato:** PR mutation é diff-scoped, sem `--workspace`; não contar como gate workspace-wide.
**Estado/efeito:** mutation/teste pode criar arquivos `mutants.out`; survivors falham gate; resultado UNKNOWN.
**Falha/propagação:** fetch/base/diff/mutant pode falhar job; workflow também aceita zero mutants em casos declarados.
**Contenção:** PR job e diff; sem efeitos de produto.
**Validação:** mutation-pr.yml:45–54,147–168; SOURCE, sem executar.
**Coordenação/evidência:** package/target resolution não coletada; não disparar workflow. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva e limites

| Caminho testemunha | Condição | Efeito causal estático | Contenção / validação |
|---|---|---|---|
| test target → own library (REL-010–017) → fake helper | alvo for selecionado | mudança API local pode afetar somente build/test target | imports enumerados; compile/test não executados |
| test target → corelink-byok (REL-018–025) → traits/types/cache contract | alvo compilado/rodado | peer API mudança pode quebrar compile; fake behavior depende código local | termina na crate BYOK; grafo transitive/runtime UNKNOWN |
| helper → corelink-ops::MultiChannelAlerter (REL-003/009) | local sink alerta chamado | RevocationError pode propagar após estado/audit local | default stub; sem cliente/canal observado |
| root workspace → CI commands (REL-026–030,034–039) | workflow trigger e runner disponíveis | declara build/test/doc/lint/format/policy/mutation/coverage; nenhuma execução afirmada | YAML/script apenas; seleção efetiva UNKNOWN |
| workspace dependency policy (REL-036/037) → declared lock/manifest scope | policy workflow for acionado | gate pode falhar por policy ou resolução; sem efeito BYOK runtime | sem grafo resolvido nem execução |
| workspace SBOM command (REL-031) → component inventory | job acionado | gera informação de pacote, não caller executável | artefato/run não observado |
| happy test source → detector cycle method (REL-033) | alvo for executado | default empty source produz lista vazia; sem prova de caminho de produção | source only; seleção/runtime UNKNOWN |
| PR mutation workflow → diff (REL-040) | PR tem paths compatíveis e job executa | muta/testa mudanças source; target package/resultados desconhecidos | fonte estática; nenhuma execução |

corelink-byok e corelink-ops são fronteiras first-party, não cópias de suas transitivas. A execução do detector de produção, production composition root, durable store/audit, actual customer alerter, provider adapters e externas não foram ligados a este package pelo censo inspecionado.

<a id="b05"></a>
## B05 — Mudança, impacto e validação

| Mudança | API / INV / REL | Consumidores/estado | Validação necessária | Coordenação/recuperação |
|---|---|---|---|---|
| Manifest/dependency/target | R01,R06; REL-001–008,010–031,036–040 | workspace selection, SBOM, mutation gate e target contracts | reler root/member/import census; Cargo candidate em M04, NOT_EXECUTED | dono local; parar antes de alegar resolution |
| Fake/helper/API/call order | API-001–015, INV-001–004, FLOW-001/002; REL-002/003,009–025,032–033 | cache/status/audit/alert locais podem ficar parciais | seguir source branch e falha; não converter em provider claim | manter estado parcial visível; corrigir docs locais |
| Peer API BYOK ou Ops | REL-002/003,009,018–025,033 | imports, trait call sites e detector smoke-call; callers externos desconhecidos | encaminhar mudança ao peer owner; validar ambos os lados | BYOK rota específica; Ops fallback CODEOWNERS |
| Workflow/Cargo CI declaration | REL-026–031,034–040 | job workspace-wide formatting/build/test/lint/policy/mutation/coverage, SBOM inventory ou PR diff mutation; não run verificado | conferir triggers/command/caminhos; execução por CI fora desta autoria | não disparar CI; registrar UNKNOWN |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Unidade: aresta/dependência ou registro candidato descoberto na população local enumerada; `documentados` e `excluídos` são destinos mutuamente exclusivos. Excluídos são listados abaixo e ficam fora do censo de callers/comandos executáveis. `unknown` conta candidatos não classificados dentro desse conjunto delimitado (não propriedades desconhecidas de um registro).

| População disjunta | Total | Descobertos | Documentados | Excluídos | Unknown |
|---|---:|---:|---:|---:|---:|
| Dependências Cargo diretas (REL-002–008) | 7 | 7 | 7 | 0 | 0 |
| Consumers Cargo inversos deste package | 0 | 0 | 0 | 0 | 0 |
| Test targets → library própria (REL-010–017) | 8 | 8 | 8 | 0 | 0 |
| Test targets → BYOK (REL-018–025) | 8 | 8 | 8 | 0 | 0 |
| Membership do workspace (REL-001) | 1 | 1 | 1 | 0 | 0 |
| Runtime-call/reexport source-level (REL-009,032,033) | 3 | 3 | 3 | 0 | 0 |
| Relações workspace-wide (REL-026–030,034–039) | 11 | 11 | 11 | 0 | 0 |
| Mutation PR diff-scoped (REL-040) | 1 | 1 | 1 | 0 | 0 |
| Relação SBOM/inventory (REL-031) | 1 | 1 | 1 | 0 | 0 |
| Registros candidatos fora das RELs abaixo | 13 | 13 | 0 | 13 | 0 |
| **Total local: 40 REL + 13 exclusões** | **53** | **53** | **40** | **13** | **0** |

**Exclusões enumeradas (13):** 1 package record no Cargo.lock e 1 componente em `.sbom/cyclonedx-rust.json`; 10 referências prose/report/audit (`changelog.d/1494-faq-s11-inverse-direction.md`, `docs/ownership/CARGO_CENSUS.md`, `docs/ownership/ROLLOUT.md`, `docs/ownership/WAVE_014_PLAN.md`, `marketing/sales/FAQ-MASTER.md`, `reports/b326-loc-cap-baseline.txt`, `scripts/published_claims_inventory.json`, `specs/_audits/2026-05-27-proptest-cases-helper-audit.md` e os dois `specs/_audits/sealed/2026-05-26-w35-p2-*absorption.md`); 1 fragmento de guidance Cargo em `.github/workflows/welcome-first-pr.yml`. Esse workflow tem `run:` executável com `gh api`; apenas suas instruções Cargo são prosa, e a chamada GitHub API não é consumer Cargo. Lock/SBOM são inventário, não callers. Mutation-nightly package matrix não é o comando nightly workspace.

Contagem de comandos: os 11 REL workspace-wide correspondem a 13 formas executáveis Cargo, pois `scripts/coverage.sh` declara clean, HTML e report; além deles há 1 comando SBOM inventory e 1 comando de mutation PR diff-scoped. Portanto, 15 formas Cargo no conjunto de workflows coberto, das quais só 13 têm escopo workspace-wide; nenhum comando foi executado.

**Não observado (fora da contagem finita):** seleção/resolução de aliases, features, targets e transitivos; resultados/artefatos CI; reachability/runtime; consumers fora do checkout; providers, credenciais, composição/estado de produção; efetividade operacional de CODEOWNERS. `Unknown=0` significa zero candidatos sem classificação dentro da busca delimitada, não zero consumers externos nem ausência de runtime. Relações `REL-009/032/033` são efeitos source-level documentados, não produção alcançável. Reproduza o censo na revisão fria.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-e2e-byok-revoke/SKILL.md#s01)
