---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-signup-flow
manifest: tests/e2e-signup-flow/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-signup-flow-source-1177dad2
---

# e2e-signup-flow — blast radius

Source/static map of one test harness. Dependency arrows, fixture data flow,
and change impact are separate claims. No row proves test execution or runtime.

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) ·
[Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

The package composes Rust APIs and in-memory fakes for signup, DPA, tier
selection, and a local CAS stub. Its declared Stripe adapter edge has an
ignored test path; provider setup and credential handling remain `UNKNOWN`.
This map covers source declarations, local fixture semantics, textual peers,
and broad workspace selectors only.

<a id="b02"></a>
## B02 — Método e populações

| População | Fonte / método | Resultado estático | Limite |
|---|---|---|---|
| Package e targets | pinned `Cargo.toml`, Git tree | 1 library, 6 `[[test]]`, `publish=false` | declaração não é execução |
| Direct Cargo edges | package manifest | 5 first-party + 9 third-party/dev declarations | sem `cargo metadata` ou resolução |
| Cargo inverso | literal `e2e-signup-flow` em `Cargo.toml` | só membership em `Cargo.toml` raiz | não certifica grafo inverso resolvido |
| Código consumidor fora do package | literal package/crate name em fontes | `e2e-billing-flow`, `e2e-dsr`, `e2e-pilot-onboarding` | referências não são dependências/imports |
| Documentos/config fora do package | busca literal rastreada | checklist, inventário, planos, auditorias e `deny.toml` enumerados em B03/B06 | alguns arquivos só foram identificados pelo path/match; conteúdo operacional não foi lido |
| Build/teste amplo | `cas_foundation.yml`, `workspace-lint.yml` | seleção genérica `--workspace --all-targets` | nenhuma execução CI observada |

O manifesto e o root workspace-membership foram comparados com o source pin
`1177dad2ca2a9f21c29b5a118aa7944b77147798`: diff escopado vazio. Não foi
executado `cargo tree`; as setas abaixo são `SOURCE`, não `RESOLVED`.

<a id="b03"></a>
## B03 — Relações atômicas diretas e locais

Cada ficha tem um contrato/condição/falha distintos. `impacto` nomeia a direção
da propagação de alteração; não inverte nem substitui fluxo de dados.

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006)

[REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012)

[REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018)

[REL-019](#rel-019) · [REL-020](#rel-020) · [REL-021](#rel-021) · [REL-022](#rel-022) · [REL-023](#rel-023) · [REL-024](#rel-024)

[REL-025](#rel-025) · [REL-026](#rel-026) · [REL-027](#rel-027) · [REL-028](#rel-028) · [REL-029](#rel-029) · [REL-030](#rel-030)

[REL-031](#rel-031) · [REL-032](#rel-032) · [REL-033](#rel-033) · [REL-034](#rel-034) · [REL-035](#rel-035) · [REL-036](#rel-036)

[REL-037](#rel-037)

<a id="rel-001"></a>
### REL-001 — Orchestrator de signup

**Identidade:** `repo:1232040291:boundary:e2e-signup-signup-orchestration-001`.
**Tipo/direções:** dependency `e2e-signup-flow→corelink-signup`; dados request→orchestrator e outcome/audit→harness; impacto API upstream→build/assertions, mudança local→cobertura.
**Superfície/ativação:** `SignupOrchestrator::provision`, outcomes, store e audit fakes; setup/cenários signup.
**Falha/limite:** rejeição ou mudança de variante invalida assertions; nenhuma chamada ao signup worker é demonstrada.
**Validação/coordenação:** alvo happy/replay/property; contrato pertence a corelink-signup. Fonte: manifesto, `helpers.rs`, alvos. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Serviço de aceite DPA

**Identidade:** `repo:1232040291:boundary:e2e-signup-dpa-service-001`.
**Tipo/direções:** dependency `e2e-signup-flow→corelink-dpa-acceptance`; dados contexto/proof→serviço e receipt/audit→harness; impacto API→compilação/assertions.
**Superfície/ativação:** `DpaAcceptanceService::accept`, locale registry e recibo; somente cenário que chama o fake.
**Falha/limite:** locale/proof/receipt incompatível interrompe o fluxo local; conteúdo legal, chave configurada e persistência são desconhecidos.
**Validação/coordenação:** cenários DPA e Free; contrato pertence a corelink-dpa-acceptance. Fonte: manifest, `helpers.rs`, testes. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Gate e seleção de tier

**Identidade:** `repo:1232040291:boundary:e2e-signup-tier-selection-001`.
**Tipo/direções:** dependency `e2e-signup-flow→corelink-tier-selection`; dados `TierTenantCtx`/tier→ledger, receipt/audit→harness; impacto contrato upstream→compilação/assertions.
**Superfície/ativação:** `TierSelectionLedger`, `InMemoryDpaGate`, `InMemoryStripeClient`; cenário de seleção.
**Falha/limite:** gate fechado espera `DpaRequired`; chamada do serviço DPA não atualiza automaticamente este gate no cenário, que o aceita explicitamente.
**Validação/coordenação:** alvo adversarial DPA e happy paths; contrato pertence a corelink-tier-selection. Fonte: manifest, `helpers.rs`, testes. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Cliente Stripe declarado, alvo ignorado

**Identidade:** `repo:1232040291:boundary:e2e-signup-stripe-real-ignored-001`.
**Tipo/direções:** dependency declarada `e2e-signup-flow→corelink-stripe-real`; fluxo externo, ativação/config e sentido de chamada além da declaração `UNKNOWN`; impacto de mudança de API pode alcançar compilação do alvo.
**Superfície/ativação:** dependência normal no manifest e alvo Starter que declara variante ignorada; nenhum provider/config foi inspecionado.
**Falha/limite:** possibilidade de HTTPS/credencial permanece fora desta evidência; não usar `--ignored` nem afirmar operação.
**Validação/coordenação:** bloqueado para execução; contrato pertence a corelink-stripe-real. Fonte: manifest e presença do alvo. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Digest verificado do CAS de teste

**Identidade compartilhada:** `repo:1232040291:boundary:hash-signup-r2-integrity-001` (chave já registrada em corelink-hash REL-025).
**Tipo/direções:** dependency `e2e-signup-flow→corelink-hash`; body→`VerifiedBody`/`Digest`→fake e bytes/digest→harness; mudança no contrato hash→compilação/assertions, mudança local→detecção do harness.
**Superfície/ativação:** `Digest`, `VerifiedBody`, `Digest::compute`, comparação de digest em `r2.rs`.
**Falha/limite:** mismatch produz erro local; não prova compatibilidade ou uso de R2 real.
**Validação/coordenação:** `happy_path_free` e teste local de R2; proprietário é corelink-hash. Fonte: manifest, `r2.rs`, [peer REL-025](../corelink-hash/BLAST_RADIUS.md#rel-025). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Declaração direta de `bytes`

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-bytes-001`.
**Tipo/direções:** dependency `e2e-signup-flow→bytes`; bytes de teste→fake CAS e bytes de leitura→assertion; impacto de API Rust→build/cenário.
**Superfície/ativação:** `Bytes` guarda o corpo local usado por PUT/GET.
**Falha/limite:** mudança de tipo/construtor atinge fixture; não é envio de objeto.
**Validação/coordenação:** `happy_path_free`; source `Cargo.toml`, `happy_path_free.rs`, `r2.rs`. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Declaração direta de `blake3`

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-blake3-001`.
**Tipo/direções:** dependency declarada `e2e-signup-flow→blake3`; uso em fonte Rust não encontrado na busca literal; fluxo efetivo `UNKNOWN`; impacto resolvido não medido.
**Superfície/ativação:** somente entrada em `[dependencies]` foi confirmada.
**Falha/limite:** não inferir uso, versão resolvida ou redundância sem grafo/compilação.
**Validação/coordenação:** revisão estática do manifest; provider/uso a confirmar em outra revisão autorizada. Fonte: manifest. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Derivação de `hex`

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-hex-001`.
**Tipo/direções:** dependency `e2e-signup-flow→hex`; saída SHA-256→hex string→`UserEmailHash`; mudança no formato atinge IDs de fixture/replay.
**Superfície/ativação:** `make_test_tenant`.
**Falha/limite:** string diferente muda request; não identifica usuário real.
**Validação/coordenação:** checar fixtures/consumidores textuais; source `Cargo.toml`, `helpers.rs`. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Derivação com `sha2`

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-sha2-001`.
**Tipo/direções:** dependency `e2e-signup-flow→sha2`; bytes do nome e sufixo fixo→SHA-256; digest→hex em `make_test_tenant`; mudança afeta fixtures.
**Superfície/ativação:** derivação do email hash de teste.
**Falha/limite:** não prova política de hash nem proteção de dado em produção.
**Validação/coordenação:** fixture/property conforme mudança; source `Cargo.toml`, `helpers.rs`. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Erros do fake via `thiserror`

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-thiserror-001`.
**Tipo/direções:** dependency `e2e-signup-flow→thiserror`; variantes de `R2Error`→formatação de erro; mudança no derive/formato→diagnóstico local.
**Superfície/ativação:** enum do `r2.rs`.
**Falha/limite:** não estabelece logging/redação de runtime.
**Validação/coordenação:** conferir casos de erro e consumidores; source manifest e `r2.rs`. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Runtime de teste `tokio`

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-tokio-001`.
**Tipo/direções:** dependency `e2e-signup-flow→tokio`; alvo sync→runtime current-thread→closure do cenário; resultado→assertion.
**Superfície/ativação:** wrapper de cenário happy Free e DPA adversarial.
**Falha/limite:** falha de build/runtime encerra o teste; não prova servidor assíncrono.
**Validação/coordenação:** targets nomeados; feature/default resolvido não observado. Fonte manifest e alvos. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Chave RSA da fixture (`rsa`)

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-rsa-001`.
**Tipo/direções:** dependency `e2e-signup-flow→rsa`; gerador→PEM privado/público→DPA fake; key→verificação receipt; mudança pode afetar setup/receipt.
**Superfície/ativação:** chave 2048-bit e encoding usada por `gen_keys_inner`.
**Falha/limite:** chave aleatória é cacheada por processo; nenhuma chave operacional foi observada.
**Validação/coordenação:** receipt tests, não executados; manifest feature `pem`, `helpers.rs`. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — RNG da chave (`rand`)

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-rand-001`.
**Tipo/direções:** dependency `e2e-signup-flow→rand`; RNG de thread→geração RSA em `gen_keys_inner`; impacto muda fixture criptográfica por processo.
**Superfície/ativação:** geração da chave compartilhada via `OnceLock`.
**Falha/limite:** não torna IDs de tenant aleatórios nem determinísticos; segurança de produção não é alegada.
**Validação/coordenação:** setup/receipt source; manifest e `helpers.rs`. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Casos aleatórios (`proptest`)

**Identidade:** `repo:1232040291:boundary:e2e-signup-cargo-proptest-001`.
**Tipo/direções:** dev dependency `e2e-signup-flow→proptest`; estratégias geradas→orchestrator fake→predicados; mudança altera amplitude/forma de cobertura.
**Superfície/ativação:** alvo `prop_atomic_invariants`; default de source é 1.000 casos, `PROPTEST_CASES` pode substituí-lo.
**Falha/limite:** property verde não prova universo completo ou produção.
**Validação/coordenação:** alvo property e invariantes signup; manifest e teste. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Identificadores derivados da fixture

**Identidade:** `repo:1232040291:boundary:e2e-signup-fixture-identities-001`.
**Tipo/direções:** data `name→TenantBundle→SignupRequest`; efeito local request→ledgers; impacto mudança de separador/hash/prefixo→replay e peers textuais.
**Superfície/ativação:** idempotency key, event/correlation ID e email hash.
**Falha/limite:** não contém identidade de cliente; `gen_keys` não é determinístico entre processos.
**Validação/coordenação:** `make_test_tenant` e idempotency test; `helpers.rs`, e2e-billing textual peer. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Composição de stores fake

**Identidade:** `repo:1232040291:boundary:e2e-signup-memory-ledgers-001`.
**Tipo/direções:** data `setup_test_ledgers→stores/sinks/services in-memory→TestEnv`; dependency edges seguem REL-001–004; impacto mudança da composição→alvos que leem `TestEnv`.
**Superfície/ativação:** constructor setup; relógio fixo e clones dos sinks/stores.
**Falha/limite:** escopo da fixture, não composition root implantado.
**Validação/coordenação:** revisar os seis alvos; fonte `helpers.rs:72–213`. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — DPA service → gate tier por fixture

**Identidade:** `repo:1232040291:boundary:e2e-signup-dpa-gate-handoff-001`.
**Tipo/direções:** data service receipt/aceite→ação explícita `dpa_gate.accept`; gate→ledger; impacto alteração da costura→validade do cenário composto.
**Superfície/ativação:** happy path; gate não é derivado automaticamente do resultado de `dpa.accept` no código visto.
**Falha/limite:** cenário pode passar sem modelar o wiring de produto.
**Validação/coordenação:** rever passo manual e gate adversarial; `happy_path_free.rs`, `helpers.rs`. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Agregação de audit assertions

**Identidade:** `repo:1232040291:boundary:e2e-signup-audit-subsequence-001`.
**Tipo/direções:** data snapshots signup/DPA/tier→lista de eventos→assertion; impacto mudança de variantes/snapshot→sequência esperada do teste.
**Superfície/ativação:** `verify_audit_chain`.
**Falha/limite:** o helper concatena por sink e aceita eventos intermediários/duplicados; não estabelece relógio/ordem temporal entre sinks.
**Validação/coordenação:** assert nas suites e source do helper; `helpers.rs:295–362`. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Estado local do CAS

**Identidade:** `repo:1232040291:boundary:e2e-signup-r2-memory-state-001`.
**Tipo/direções:** data bind→map PAT/tenant, PUT→map objeto/contador, GET→bytes/contador; impacto mudança de chave/lock→resultados do fake.
**Superfície/ativação:** `HashMap` sob um `Mutex`, chave `tenant:digest_hex`.
**Falha/limite:** mapa volátil por instância; lock poisoning é mapeado ao erro conforme cada método; não é CAS persistente.
**Validação/coordenação:** unit/happy tests; `r2.rs:57–165`. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — Crate root → exports do harness

**Identidade:** `repo:1232040291:boundary:e2e-signup-crate-exports-001`.
**Tipo/direções:** reexport root→módulos/helpers/R2; dados fluem segundo APIs específicas; mudança de export→imports dos seis alvos e consumidores Cargo futuros.
**Superfície/ativação:** `pub mod`, `pub use` e constantes de `src/lib.rs`.
**Falha/limite:** caminho público não prova consumidor externo ou estabilidade publicada.
**Validação/coordenação:** comparar exports a call sites; `publish=false`, `src/lib.rs`. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Teste da jornada Free

**Identidade:** `repo:1232040291:boundary:e2e-signup-test-happy-free-001`.
**Tipo/direções:** test target→fixtures e ledgers REL-001–003/005; dados fluem pelo fluxo Free/R2; impacto da falha é perda do alarme local.
**Superfície/ativação:** `happy_path_free`, incluindo PAT não ligado.
**Falha/limite:** resultado ainda não executado; não valida endpoint.
**Validação/coordenação:** alvo `happy_path_free`; owner dos contratos upstream permanece separado. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Teste Starter e alvo Stripe ignorado

**Identidade:** `repo:1232040291:boundary:e2e-signup-test-happy-starter-001`.
**Tipo/direções:** test target→fixture/tier; o subcaminho provider e seus dados são `UNKNOWN`; impacto alteração do alvo→build/assertions.
**Superfície/ativação:** `happy_path_starter_stripe_test_mode`; fonte declara um teste in-memory e uma variante `#[ignore]`.
**Falha/limite:** nenhuma inspeção de configuração/credenciais e nenhuma execução autorizada nesta revisão.
**Validação/coordenação:** alvo estático apenas; revisar limite com corelink-stripe-real antes de qualquer operação. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Negação sem aceite DPA

**Identidade:** `repo:1232040291:boundary:e2e-signup-test-dpa-negative-001`.
**Tipo/direções:** test target→signup/tier fake; dados de tentativa→erros/audit/session snapshot; impacto de falha reduz detecção da precondição.
**Superfície/ativação:** `adversarial_dpa_not_accepted`, Free e Starter.
**Falha/limite:** prova apenas assertion source; sem usuário/provider.
**Validação/coordenação:** alvo nomeado; contrato tier pertence ao upstream. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Negação de assinatura inválida

**Identidade:** `repo:1232040291:boundary:e2e-signup-test-webhook-negative-001`.
**Tipo/direções:** test target→tipos/fake tier; header/payload de fixture→verifier e estado consultado→assertions; impacto verifier API→compile/expectation.
**Superfície/ativação:** `adversarial_webhook_signature_invalid`.
**Falha/limite:** fixture não é webhook recebido; runtime/provider não observado.
**Validação/coordenação:** alvo estático; contrato da assinatura pertence ao crate upstream que define os símbolos. [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — Teste de replay idempotente

**Identidade:** `repo:1232040291:boundary:e2e-signup-test-idempotency-001`.
**Tipo/direções:** test target→signup fake; mesma request→duas invocações→outcomes/commit/audit→assertions; impacto de falha remove detecção do replay local.
**Superfície/ativação:** `idempotency_replay`.
**Falha/limite:** store em memória, não D1/durabilidade.
**Validação/coordenação:** alvo nomeado; corelink-signup mantém contrato de idempotência. [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — Property de invariantes atômicos

**Identidade:** `repo:1232040291:boundary:e2e-signup-test-prop-atomic-001`.
**Tipo/direções:** test target→estratégias proptest→signup fake→predicados; impacto mudança de estratégia/caso→escopo aleatório da detecção.
**Superfície/ativação:** `prop_atomic_invariants`, payload e replay com mesma chave.
**Falha/limite:** faixa/default não certificam concorrência ou transação durable.
**Validação/coordenação:** alvo property; comando futuro e ambiente em M04. [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — Peer textual `e2e-billing-flow`

**Identidade:** `repo:1232040291:boundary:e2e-signup-billing-fixture-reference-001`.
**Tipo/direções:** relação documental, dependency `not-applicable`, flow `not-applicable`; comentários em `harness.rs` citam clock/derivação, mas os valores são definidos no peer; impacto é coordenação de fixtures, não chamada.
**Superfície/ativação:** source comments e `make_test_tenant` do peer.
**Falha/limite:** mudança não propaga dado entre processos ou crate.
**Validação/coordenação:** rever os dois documentos/callers ao alterar a convenção comum; `tests/e2e-billing-flow/src/{harness,lib}.rs`. [Relation index](#b03)

<a id="rel-028"></a>
### REL-028 — Peer textual `e2e-dsr`

**Identidade:** `repo:1232040291:boundary:e2e-signup-dsr-clock-reference-001`.
**Tipo/direções:** relação documental, dependency/flow `not-applicable`; comentário declara que o clock do DSR espelha a fixture; impacto de alteração é revisão de convenção.
**Superfície/ativação:** `tests/e2e-dsr/src/lib.rs` constante `TEST_NOW_MS`.
**Falha/limite:** fonte não prova compartilhamento de objeto nem execução cruzada.
**Validação/coordenação:** conferir as duas declarações; o peer mantém sua fixture. [Relation index](#b03)

<a id="rel-029"></a>
### REL-029 — Peer documental `e2e-pilot-onboarding`

**Identidade:** `repo:1232040291:boundary:e2e-signup-pilot-peer-reference-001`.
**Tipo/direções:** relação documental, dependency/data `not-applicable`; crate root lista este package como harness companheiro; impacto limitado a texto/journey rationale.
**Superfície/ativação:** comment em `tests/e2e-pilot-onboarding/src/lib.rs`.
**Falha/limite:** não há chamada ou aresta Cargo provada.
**Validação/coordenação:** manter descrição de escopos distintos; source do peer. [Relation index](#b03)

<a id="rel-030"></a>
### REL-030 — Documentos de teste e auditorias

**Identidade:** `repo:1232040291:boundary:e2e-signup-static-doc-references-001`.
**Tipo/direções:** referências documentais em `docs/testing/2026-06-23-gapmap-{journeys,quality,surfaces}.md`, `specs/_audits/2026-05-27-proptest-cases-helper-audit.md` e auditorias sealed de 15, 16, 26 e 27 maio; dependency/data `not-applicable`; impacto é texto potencialmente stale.
**Superfície/ativação:** menções rastreadas ao path/package; não equivalem à evidência atual do harness.
**Falha/limite:** documentos históricos não provam execução desta baseline.
**Validação/coordenação:** atualizar somente se a tarefa cobrir esses docs; não copiar alegação histórica como resultado atual. [Relation index](#b03)

<a id="rel-031"></a>
### REL-031 — Política de dependências em `deny.toml`

**Identidade:** `repo:1232040291:boundary:e2e-signup-deny-policy-001`.
**Tipo/direções:** configuração Cargo-deny referencia package como exceção de wrapper para Stripe; dependency direction continua REL-004; impacto de renomear/remover edge exige reconciliar policy.
**Superfície/ativação:** entrada textual em `deny.toml`; nenhum checker de deny foi rodado.
**Falha/limite:** comentário de policy não demonstra autorização ou execução.
**Validação/coordenação:** owner de policy não verificado; `deny.toml` no pin. [Relation index](#b03)

<a id="rel-032"></a>
### REL-032 — Workflow amplo de build e teste

**Identidade:** `repo:1232040291:boundary:e2e-signup-ci-workspace-test-001`.
**Tipo/direções:** build/test selector `cas_foundation.yml→workspace --all-targets`; fluxo é compilação/teste quando job roda; impacto do source pode falhar gate amplo.
**Superfície/ativação:** comando genérico em workflow, selecionado por eventos daquele workflow.
**Falha/limite:** não há job nomeado nem run/result observado; eventos/permissões não reconciliados nesta revisão.
**Validação/coordenação:** consultar owner de CI antes de interpretar status; fonte workflow. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — Lint amplo da workspace

**Identidade:** `repo:1232040291:boundary:e2e-signup-ci-workspace-lint-001`.
**Tipo/direções:** build selector `workspace-lint.yml→clippy workspace/all-targets`; dados não aplicável; impacto é erro de lint/build em alvo selecionado.
**Superfície/ativação:** seleção genérica por workflow; sem execução afirmada.
**Falha/limite:** lint verde não valida assertions nem runtime.
**Validação/coordenação:** owner de CI não verificado; fonte workflow. [Relation index](#b03)

<a id="rel-034"></a>
### REL-034 — Inventário e documentos da campanha W015

**Identidade:** `repo:1232040291:boundary:e2e-signup-ownership-campaign-docs-001`.
**Tipo/direções:** referências em `docs/ownership/CARGO_CENSUS.md`, `ROLLOUT.md`, `WAVE_015_PLAN.md`; dependency/data `not-applicable`; impacto é reconciliação de inventário e relações.
**Superfície/ativação:** menção do manifest/package e do peer REL-025; estes arquivos não pertencem a esta unidade.
**Falha/limite:** documento de campanha não prova composição, resolução ou aprovação deste package.
**Validação/coordenação:** integração serial da campanha; revisar o registro peer sem editar índice global em paralelo. [Relation index](#b03)

<a id="rel-037"></a>
### REL-037 — Referência estática no peer DPA

**Identidade:** `repo:1232040291:boundary:e2e-signup-dpa-static-consumer-reference-001`.
**Tipo/direções:** relação documental nos dois arquivos owner de DPA; dependency/data `not-applicable`; impacto do source/manifest change é revisão do registro de consumidor estático.
**Superfície/ativação:** `corelink-dpa-acceptance/BLAST_RADIUS.md` e `MAINTENANCE.md` nomeiam e2e-signup-flow como referência/consumidor estático.
**Falha/limite:** não prova import, execução ou efeito; essas docs têm baseline própria.
**Validação/coordenação:** reconciliar peer durante review cruzada; contrato DPA mantém owner separado. [Relation index](#b03)

<a id="rel-035"></a>
### REL-035 — Match de checklist fora do escopo lido

**Identidade:** `repo:1232040291:boundary:e2e-signup-checklist-path-match-001`.
**Tipo/direções:** match textual `docs/internal/secrets-checklist.md`; dependency/data/impacto semânticos `UNKNOWN` porque o conteúdo não foi inspecionado.
**Superfície/ativação:** somente caminho e match literal foram observados.
**Falha/limite:** nenhuma afirmação sobre segredo, provider, valor, instrução ou ativação.
**Validação/coordenação:** revisão futura delimita leitura autorizada; esta ficha não permite operar nem ler configuração. [Relation index](#b03)

<a id="rel-036"></a>
### REL-036 — Snapshot de LOC

**Identidade:** `repo:1232040291:boundary:e2e-signup-loc-inventory-001`.
**Tipo/direções:** referência em `reports/b326-loc-cap-baseline.txt`; dependency/data `not-applicable`; impacto de path/renomeação pode tornar o snapshot stale.
**Superfície/ativação:** somente linhas que listam arquivos do package.
**Falha/limite:** não é source of truth para targets atuais nem cobertura.
**Validação/coordenação:** reconciliar pelo manifesto/Git tree; não regenerar o report nesta tarefa. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho testemunha | Condição | Efeito causal | Contenção / estado |
|---|---|---|---|---|
| `corelink-signup` | REL-001 → REL-021/025/026 | source/target compile ou assertion | quebra upstream pode quebrar harness; harness quebrado remove detecção | efeito de produto não inferido |
| `corelink-dpa-acceptance` | REL-002 → REL-017/021/023 | alvo que usa contexto/receipt | API muda setup/assertions | operação de DPA desconhecida |
| `corelink-tier-selection` | REL-003 → REL-017/021–024 | seleção fake | mudança pode alterar receipt/gate assertion | gate de produção não inferido |
| `corelink-hash` | REL-005 → REL-019/021 | caminho CAS do fake | digest/VerifiedBody mudam build ou contrato do cenário | peer existente requer chave compartilhada |
| `corelink-stripe-real` | REL-004 → REL-022/031 | dependência/target declarado | API pode alterar compilação; provider path não revisado | inspeção/execução bloqueada |
| `e2e-billing-flow`, `e2e-dsr` | REL-027/028 | mudanças de convenção fixture | comentários ou valores parelhos podem divergir | sem fluxo/import provado |
| workspace CI | REL-032/033 | workflow seleciona workspace | mudança pode quebrar build/test/lint se job rodar | estado de CI não observado |

As rotas transitivas param em APIs de fakes e assertions locais. Não há fonte
que conecte estas fixtures a route montada, storage durável, provider executado,
deploy ou tenant real. O alvo ignorado tem setup não inspecionado e permanece
desconhecido; não se infere contenção apenas por ser `#[ignore]`.

<a id="b05"></a>
## B05 — Matriz mudança → impacto → validação

| Mudança | Contratos/RELs | Impacto local | Validação futura mínima | Coordenação / recuperação |
|---|---|---|---|---|
| API/outcome de signup | API-001; REL-001, 021, 025, 026 | fixture, replay e property podem não compilar ou afirmar outra semântica | alvo alterado + idempotency/property conforme M04 | owner `corelink-signup`; rever asserts |
| proof/receipt DPA | API-001; INV-003; REL-002, 017, 023 | receipt/context e gate fixture divergem | alvo DPA adversarial e happy Free | owner DPA; não alterar conteúdo legal sem owner |
| tier ou gate | API-001; INV-003; REL-003, 017, 021–023 | recusa/receipt/sessions diferem | targets Free, Starter in-memory e DPA negative; ignored excluído | owner tier; recuperação em código de teste |
| IDs/clock/audit | API-001/002; INV-001/002; REL-015, 018, 027/028 | peers, replay ou audit assertion deixam de coincidir | idempotency/property e peer review | reverter apenas fixture/cobertura |
| CAS/digest/keying | API-003; INV-004; REL-005/019/021 | isolamento, digest ou contadores mudam | `r2.rs` tests e Free target | coordene hash owner se API mudar |
| alvo/property/manifesto | REL-006–014, 021–026, 031–037 | build/test selecionado ou política muda | checks estáticos; depois gate restrito e autorizado | reconciliar root manifest/policy/CI |
| escopo Stripe/credencial | API-004; REL-004/022/031 | novo caminho pode executar provider ou tocar segredo | `BLOCKED`; sem comando nesta manutenção | owner/provider/config `UNKNOWN` |

<a id="b06"></a>
## B06 — Cobertura, exclusões e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| Direct dependencies Cargo | 14 | 14 | 0 | 0 |
| Test targets declarados | 6 | 6 | 0 | 0 |
| Consumidores em manifests | 0 encontrados na busca literal; inverso resolvido não rodado | 0 | 0 | 1 domínio resolvido pendente |
| Referências externas ao package em fontes | 3 peers de harness | 3 | 0 | 0 |
| Seletores de workflow genéricos | 2 | 2 | 0 | 0 |
| Referências documentais/config fora de Cargo | 18: deny, checklist-path, docs de campanha/owners, testes, auditorias e snapshot | 18 | 0 | 0 |

A busca de arquivos fora do package encontrou também `docs/internal/secrets-checklist.md`;
seu conteúdo/config não foi inspecionado por escopo. Os outros 17 matches documentais
são `deny.toml`, `docs/ownership/{CARGO_CENSUS,ROLLOUT,WAVE_015_PLAN}.md`, os dois
documentos DPA, o blast do hash, três `docs/testing/2026-06-23-gapmap-*.md`, seis
auditorias `specs/`, e `reports/b326-loc-cap-baseline.txt`. A busca também encontrou
`Cargo.lock` e root `Cargo.toml`; o lock não foi resolvido com Cargo e o report é
inventário, não consumidor. Não se afirma completude de busca dinâmica, macros,
código gerado ou configuração não lida.

Desconhecidos que impedem aprovação operacional: grafo Cargo inverso/resolvido,
targets/feature efetivos, execução dos testes, conteúdo/provider da variante Stripe,
rota de escalação, revisão independente e qualquer deploy/runtime. Os 37 RELs cobrem
as relações estáticas enumeradas, não certificam ausência de relações ocultas.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-e2e-signup-flow/SKILL.md#s01) · [Início](#b01)
