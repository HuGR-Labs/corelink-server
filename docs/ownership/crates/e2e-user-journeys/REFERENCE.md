---
schema: corelink-ownership/1.1
document: reference
package: e2e-user-journeys
manifest: tests/e2e-user-journeys/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: H
state: draft
evidence_set: source-inspection-1177dad2
---

# e2e-user-journeys — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Este package é um bin de journeys que consome contratos HTTP e registra PASS/FAIL/GATED. As descrições de manifesto não provam deploy, chamada HTTP nem comportamento de provider; código-fonte prova apenas que há chamadas construídas.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `e2e-user-journeys` / `tests/e2e-user-journeys/Cargo.toml` |
| Targets | Um bin explícito `e2e-user-journeys`, `src/main.rs`; sem library, test target ou feature declarados. |
| Papel | Harness binário e suite de assertivas externas, sem implementação de API. |
| Identidade Cargo | `edition=2021`, `rust-version=1.78`, `license=UNLICENSED`, `publish=false`; membro de workspace no `Cargo.toml` raiz. |
| Dependências diretas | `reqwest`, `serde_json`, `uuid`, `sha2`, `blake3`, `hex`; zero dependência Cargo de primeiro partido. |
| Contexto operacional | `tests/e2e-user-journeys/README.md` define a regra black-box e execução; `JOURNEY-MATRIX.md` define personas, superfícies e caminhos; nenhum dos dois implementa o serviço. |
| Composição compartilhada | Membro declarado em `Cargo.toml` raiz; nó `e2e-user-journeys` em `Cargo.lock`; composição runtime em `src/journeys/mod.rs`. |
| Perfil | H: o source pin contém 23 funções `run` despachadas em `journeys/mod.rs`; o valor 22 em WAVE_015_PLAN é um censo histórico corrigido pelos 23 dispatches e 23 implementadores contados neste pin. O número documentado é 23 (acima do limiar candidato H de 20); contagem estática não prova execução ou completude runtime. |
| Execução observada | Nenhuma chamada de API, execução de bin, build ou resultado de journey foi feita nesta autoria. |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Dono da implementação | Dono do contrato | Operação / escalonamento |
|---|---|---|---|
| Runner, harness, personas e asserts | Este package | Contratos de comportamento declarados nas fichas API | Mudança local; validação futura via M04. |
| Rotas HTTP consumidas | Serviço público indicado pelos URL builders e fonte citada no harness | Owner canônico conforme OKF; não reidentificado nesta inspeção | Consulte [OKF](../../../internal/okf-wiki/concept-manifest.yaml); pare se a rota não resolver o owner. |
| CLI `corelink` e `bazel` | Binários externos ao package | CLI/protocolo respectivos, owner não declarado pelo manifesto | Modo lento em `journeys/bazel.rs`; requer ferramentas e ambiente dedicados. |
| Provisionamento real | `scripts/e2e-real-client/provision-and-run-suite.sh` | API Clerk/CoreLink e operador do ambiente | Operação independente, capaz de criar e apagar usuários; não é parte do package. |

**Não faz:** implementação de rota, leitura direta de D1/R2/KV, acesso de banco para asserção ou mock de sistema sob teste. Isso é regra de fonte/manifesto, não prova de execução isolada.
**Conceitos canônicos:** o OKF continua sendo a rota de arquitetura e ownership externo; `concept-manifest.yaml` marca `tests/e2e-user-journeys` como harness de teste fora de conceitos de arquitetura.
**Absorções e aliases:** nenhum crate interno é declarado como dependência; não há aliases `pub use` no package.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entradas | Papel e dado sob ownership | Natureza | Contratos / evidências |
|---|---|---|---|
| `src/main.rs` | Carrega config, ordena jornadas, agrega status, imprime saída e aplica mínimo de PASS / máximo de GATED. | Runner próprio | API-001, INV-001, FLOW-001. |
| `src/harness.rs` | Env, token map, `JourneyResult`, cliente blocking, URL builders, hashes BLAKE3/SHA-256 e asserts HTTP. | Harness próprio | API-002/003; R06. |
| `src/personas.rs` | Resolve persona para tenant, token e expectativa; ausência de token vira motivo de gate. | Harness próprio | API-002; INV-002. |
| `src/journeys/mod.rs` | `journeys::all` preaquece OCI uma vez e chama os 23 `run` em ordem fixa. | Composição local | API-004/FLOW-001; 23 chamadas `out.extend`. |
| `journeys/{identity,cas,concurrency,ac,security,edge,abuse}.rs` | Identidade, CAS/AC, concorrência, negações, entradas de borda e controles de abuso. | Módulos próprios | API-003; REL-001/005. |
| `journeys/{bazel,turbo,adapters,oci,oci_public_isolation,shared_cache}.rs` | Protocolos Bazel/Turbo, cargo/npm/pip/brew, registry OCI e cache público. | Módulos próprios | REL-002/003/004. |
| `journeys/{dashboard,pat_lifecycle,billing,quota,dsr,team}.rs` | Painéis e operações de conta, billing, quota, exclusão e convites. | Módulos próprios; algumas operações alteram estado remoto | REL-005/006; M05. |
| `journeys/{audit,introspect,runners,runner_purchase}.rs` | Exportação/validação de audit, introspect, runner entitlement e compra. | Módulos próprios | REL-007/008. |
| `journeys/oci/{early,late}/**` e `journeys/adapters_auth.rs` | Submódulos auxiliares de protocolo/auth; não adicionam função `run` à lista raiz. | Helpers próprios | REL-003/004. |

**Inventário:** contagem reproduzida por declarações/calls em `journeys/mod.rs` e `rg '^pub fn run\(' tests/e2e-user-journeys/src/journeys`; arquivos OCI internos são filhos de apoio. Não resolve o grafo Cargo, não prova execução e não reclassifica a contagem planejada.

**Dispatcher observado (ordem do source pin):** `identity → cas → concurrency → ac → bazel → turbo → adapters → oci → oci_public_isolation → dashboard → pat_lifecycle → billing → quota → dsr → introspect → audit → security → edge → abuse → shared_cache → runners → runner_purchase → team`.

Cada chamada é `out.extend(<module>::run(cfg, client))`; `oci::warm_oci(cfg, client)` ocorre antes. É a composição efetiva, não a lista histórica de sete journeys do README.

**Helpers exatos adicionais de `harness.rs`:** `url_health_serving`, `url_health`, `url_users_me`, `url_stripe_webhook`, `b64_decode_std`, `hmac_sha256`, `stripe_signature_header`.

**Builders exatos restantes:** `url_cas_list`, `url_cas_batch`, `url_cas_batch_read`, `url_cas_batch_exists`, `url_ac_list`.

| Grupo de dispatch | Relações de blast correspondentes |
|---|---|
| `identity`, `cas`, `concurrency`, `ac` | REL-001, REL-002, REL-010, REL-014 |
| `bazel`, `turbo`, `adapters`, `oci`, `oci_public_isolation` | REL-003, REL-004, REL-005–009, REL-018–019 |
| `dashboard` | REL-033 |
| `pat_lifecycle`, `billing`, `quota`, `dsr`, `introspect`, `audit` | REL-011–017 |
| `security`, `edge`, `abuse`, `shared_cache`, `runners`, `runner_purchase`, `team` | REL-001–002, REL-008, REL-010–017 |
| root, lock, SBOM, mutants, docs e OKF, Stripe fixture/webhook | REL-025–034 |

<a id="r04"></a>
## R04 — Contratos públicos

Este bin não exporta uma API de library. Os contratos abaixo são as assertivas e regras locais da suite.

**Índice de contratos:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Resultado e veredito do runner

**Símbolos exatos:** `JourneyStatus::{Pass,Fail,Gated}`, `JourneyResult`, `ship_verdict`, `main`.
**Entrada:** contagens PASS/FAIL/GATED, `CORELINK_E2E_MIN_PASS` (default 1) e `CORELINK_E2E_MAX_GATED` opcional.
**Saída / pós-condição:** FAIL, PASS abaixo do piso ou GATED acima do teto configurado produz RED e exit 1; caso contrário GREEN. GATED é registrado, nunca PASS.
**Prova:** `src/main.rs`; a lógica tem testes unitários declarados no mesmo arquivo, não executados nesta autoria.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Configuração e personas

**Símbolos exatos:** `Config::from_env`, `Config::token`, `Config::tenant_or_anon`, `TokenKind`, `Persona::resolve`, `Expectation`.
**Entrada:** variáveis `CORELINK_E2E_*`; endpoint default `http://localhost:8787`, removendo `/` final.
**Saída / pós-condição:** configura endpoint, tenants, flags e personas; token/tenant ausentes retornam motivo de gate ou `_anonymous` para chamadas que o permitirem.
**Compatibilidade / prova:** `src/harness.rs`, `src/personas.rs`; valores de segredo não são parte do contrato documental.

[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — HTTP, rotas e endereçamento

**Símbolos exatos:** `build_client`, `bearer`, `sha256_hex`, `blake3_hex`, `url_cas`, `url_ac`, `url_bazel_cas_read`, `url_bazel_ac`, `url_bazel_cas_write`, `url_bazel_find_missing`, `url_turbo_artifact`, `url_turbo_status`, `url_turbo_events`, `url_cargo`, `url_npm`, `url_pip`, `url_brew`, `url_oci_token`, `url_oci_v2`, `url_customer`, `url_audit_export`, `url_tier_select`, `url_introspect`, `url_stripe_webhook`.

**Assertions exatas:** `unique_blob`, `expect_status`, `expect_denied`, `expect_gate_denied`.
**Entrada / efeito:** cliente blocking com timeout de 60 s e verificação TLS; builders formam endpoints a partir de `Config`; CAS usa BLAKE3, enquanto chaves opacas e OCI usam SHA-256.
**Falhas / prova:** request, status, corpo e hash são assertados pelos journeys; source local não certifica rota implantada. Fonte: `src/harness.rs` e módulos de journey.

[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Composição e despacho de journeys

**Símbolos exatos:** `journeys::all`, `oci::warm_oci`, `identity::run`, `cas::run`, `concurrency::run`, `ac::run`, `bazel::run`, `turbo::run`, `adapters::run`, `oci::run`, `oci_public_isolation::run`, `dashboard::run`, `pat_lifecycle::run`, `billing::run`, `quota::run`, `dsr::run`, `introspect::run`, `audit::run`, `security::run`, `edge::run`, `abuse::run`, `shared_cache::run`, `runners::run`, `runner_purchase::run`, `team::run`.

**Entrada:** `&Config` e `&reqwest::blocking::Client`. **Pós-condição:** OCI é preaquecido uma vez; os 23 vetores `Vec<JourneyResult>` são concatenados na ordem acima, sem filtro silencioso. **Falha:** remover um `out.extend`, trocar a ordem ou engolir um resultado muda o ship gate; o predicado é verificável por census source-scoped (PROC-001), não por execução observada.

[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado / recurso | Chave e owner | Persistência / vida útil | Escrita / leitura / durabilidade |
|---|---|---|---|
| Resultado | Uma execução local; runner | stdout/exit status do processo | Agregado em memória; não persiste resultado canônico. |
| Configuração/persona | Processo + env | Vida do processo; valores podem ser credenciais | Leitura por `Config::from_env`; não imprime tokens por contrato. |
| Objetos, chaves, invites e billing | Tenant/provider externo | Retenção e cleanup variam por API; não resolvidos pelo harness | Writes HTTP reais podem persistir; não há rollback geral no runner. |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — GREEN exige evidência positiva

**Regra:** GREEN exige zero FAIL, PASS >= `CORELINK_E2E_MIN_PASS` (default 1) e, se configurado, GATED <= `CORELINK_E2E_MAX_GATED`.
**Imposição:** precedência de `ship_verdict` em `src/main.rs`; caso contrário, status não-zero.
**Violação / prova:** zero PASS abaixo do piso não pode ser GREEN; `ship_verdict_tests` declara o contraexemplo, mas não foi executado aqui.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Prerequisito ausente não é sucesso

**Regra:** ausência de token, tenant, CLI ou opt-in deve ser uma linha GATED com motivo, não uma jornada silenciosamente omitida.
**Imposição:** `JourneyResult::gated`, `Persona::resolve` e gates de cada módulo.
**Violação / prova:** ausência que reduz uma chamada sem produzir GATED viola a regra; fonte em `harness.rs`, `personas.rs` e `journeys/*.rs`; execução não observada.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Harness não lê estado interno

**Regra:** asserts vêm de superfícies HTTP e subprocessos externos; não de import `corelink-*`, banco ou storage direto.
**Imposição:** manifesto não declara dependências first-party; source usa `reqwest` e `Command` nos caminhos identificados.
**Violação / prova:** novo import ou caminho direto de banco/storage contradiz o escopo; inspecionar manifesto e `src/`; nenhuma compilação executada.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Despacho e veredito

1. `Config::from_env` lê endpoint e slots opcionais.
2. `build_client` cria cliente HTTP blocking.
3. `journeys::all` preaquece OCI uma vez e chama os 23 módulos na ordem do source, cada um via `out.extend(<module>::run(cfg, client))`.
4. Cada jornada retorna PASS, FAIL ou GATED e duração.
5. Runner conta linhas e compara piso/teto configurados.
6. Imprime sumário e encerra com RED/exit 1 nas condições de INV-001.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / fonte | Default efetivo | Quando é lido | Target / condição | Efeito / falha |
|---|---|---|---|---|
| `CORELINK_E2E_ENDPOINT` | `http://localhost:8787` | Boot | Bin único | Base primária; default não prova serviço local. |
| `CORELINK_E2E_OCI_ENDPOINT` | endpoint primário | OCI journey | Bin único | Sobrescreve host OCI quando não vazio. |
| `CORELINK_E2E_TENANT`, `CORELINK_E2E_TENANT_B` | Ausente | Persona/journey | Bin único | Tenant opcional; ausente pode GATED ou `_anonymous`. |
| `CORELINK_E2E_PAT_{RW,RO,ADMIN,REVOKED,EXPIRED,TENANT_B,FREE,SOLO,PRO,ENTERPRISE,PASTDUE,RUNNER,QUOTA,FRESH,TEAM_MEMBER,ACCT}` | Ausente | Resolver persona | Bin único | Slot de token; ausente deve GATED. |
| `CORELINK_E2E_RUN_SLOW`, `CORELINK_E2E_QUOTA_TEST`, `CORELINK_E2E_ABUSE_HARD` | Desligado | Jornada | Bin único | Ativa caminhos lentos, quota ou burst agressivo. |
| `CORELINK_E2E_MIN_PASS`, `CORELINK_E2E_MAX_GATED` | `1`, sem teto | Veredito | Bin único | Piso e teto que determinam RED. |
| `CORELINK_E2E_{PUBLIC_HASH,PUBLIC_BREW_PATH,TOMBSTONED_HASH,RUNNER_TENANT,QUOTA_TENANT,PASTDUE_TENANT,FRESH_TENANT,ACCT_TENANT,TEAM_INVITE_EMAIL,TEAM_MEMBER_TENANT,TEAM_MEMBER_USER_ID,PAT_TENANT_B_ADMIN}` | Ausente | Jornadas específicas | Bin único | Fixtures/recursos externos; ausência condiciona gate. |
| `CORELINK_E2E_{DSR_TEST,DSR_TEST_TENANT,DSR_TEST_SESSION,DSR_SESSION,INTROSPECT_KEY,SIGNUP_WORKER_ENDPOINT,STRIPE_TEST,STRIPE_WEBHOOK_TEST,STRIPE_WEBHOOK_SECRET,STRIPE_SUBSCRIPTION_ID,STRIPE_CUSTOMER_ID,STRIPE_PRICE_ID,CLERK_SESSION}` | Ausente/desligado | DSR, auth ou billing | Bin único | Segredos/opt-ins; podem habilitar chamadas mutáveis. Registre nomes, nunca valores. |

**Matriz suportada:** uma seleção `e2e-user-journeys` sem `required-features`; não existe `[features]`, portanto não há feature própria declarada. O manifesto possui uma entrada `[[bin]]`; não declara `[[test]]` ou `[lib]`.
**Combinações recusadas / stubs:** falha de resolução normalmente vira GATED; módulos específicos podem ser stubs. O gate e o default devem ser revistos no código alterado; nomes de env não provam seu provisionamento.

**Contradição documental reconciliada:** o `README.md` do source pin ainda exemplifica `CORELINK_E2E_TOKEN` e a antiga lista J1–J7. O código atual lê `CORELINK_E2E_PAT_*`, despacha 23 módulos e usa `CORELINK_E2E_MIN_PASS`/`CORELINK_E2E_MAX_GATED`. Para ownership, `main.rs`, `harness.rs` e `journeys/mod.rs` são a autoridade executável; README e matriz são contextos a atualizar, não prova de runtime.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal / erro | Causa no contrato | Estado após falha | Diagnóstico / procedimento |
|---|---|---|---|
| `FAIL` | HTTP, corpo ou estado observado viola assert | Escritas anteriores podem permanecer | PROC-004; interrompa se isolamento ou dados inesperados. |
| `GATED` | Token, flag, fixture ou bin ausente | Chamada condicionada não foi certificada | PROC-003/004; registrar prerequisite, não inferir PASS. |
| Exit 1 | FAIL, piso de PASS ou teto de GATED | Processo falhou; estado remoto pode ter mudado | `main.rs`, INV-001; consultar M05 antes de rerun. |
| Erro de transporte | Timeout/TLS/conexão ou host errado | Resultado normalmente FAIL; sem rollback implícito | Endpoint e saída sem credenciais; nenhum telemetria declarada. |

**Limites:** o bin imprime linhas PASS/FAIL/GATED; não há exporter ou persistência de evidência demonstrada nesta inspeção. Respostas do endpoint são o oracle, não estado interno.

<a id="r08"></a>
## R08 — Verificação e evidências

| API / INV / FLOW | Fonte e revisão | Teste / método | Resultado e limite |
|---|---|---|---|
| Identidade e target | Manifesto no source pin e `Cargo.toml` raiz | SOURCE; comparação por Git | Package, bin e workspace member confirmados; nenhuma resolução Cargo. |
| API-001 / INV-001 | `src/main.rs::ship_verdict` | Testes inline declarados | Não executados; regra de fonte apenas. |
| API-002 / INV-002 | `harness.rs`, `personas.rs` e gates de journeys | Leitura de fonte | Nenhuma credencial ou journey foi exercitada. |
| API-003 / INV-003 | Builders e chamadas HTTP/processo | Leitura de fonte/manifests | Não prova deploy, alcance, resposta ou ausência de consumidor dinâmico. |
| API-004 / FLOW-001 | `journeys/mod.rs`, `main.rs`, arquivos `journeys/*.rs` | Conferência de `warm_oci`, dispatches `out.extend`, implementadores `pub fn run` e ordem | 23 dispatches reconciliam com 23 implementadores; o 22 do plano é histórico e corrigido neste censo; não é prova de execução/runtime. |
| R06 env contract | `README.md`, `main.rs`, `harness.rs`, `journeys/mod.rs` | Reconciliamento source-vs-doc | README legado (`CORELINK_E2E_TOKEN`, J1–J7) não é o contrato atual; PAT map + 23 dispatches são a fonte atual. |

**Desconhecidos e documentação histórica:** source pin 1177dad2 foi comparado a este integration baseline; os caminhos do package e os manifests coincidem. O plano preserva o antigo valor 22, corrigido aqui pelo censo de 23 dispatches e implementadores. O README também mantém a lista legada J1–J7, enquanto o source atual tem PAT map.

Hosts, secrets, grafo Cargo resolvido, APIs implantadas, owners específicos, disponibilidade de ferramentas, estado remoto e resultado de jornadas não foram observados. A contagem estática não prova execução.
**Continuar:** [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
