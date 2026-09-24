---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-user-journeys
manifest: tests/e2e-user-journeys/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: H
state: draft
evidence_set: source-inspection-1177dad2
---

# e2e-user-journeys — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) ·
[Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Leitura rápida e escopo

O raio é sobretudo externo: o bin envia HTTP e pode executar `corelink`/`bazel`; o real-client wrapper pode provisionar contas antes de chamá-lo. Source local prova chamadas construídas, não deploy nem efeitos ocorridos.

O source pin contém um bin e 23 funções `run` despachadas. O valor 22 em WAVE_015_PLAN é um censo histórico corrigido: `journeys/mod.rs` e o censo independente dos implementadores têm 23 itens. Isso não prova que as jornadas rodaram. Cada REL registra superfície, ativação, direções, efeito, falha e limite.

**Builds avaliados:** nenhum; pacote/target/features só inspecionados no manifesto.
**Ambientes não observados:** localhost, produção, APIs de terceiros, CI e qualquer tenant/provider.
Não confundir dependência declarada, chamada traçada e runtime observado.

<a id="b02"></a>
## B02 — Inventário e método

| População | Fontes / método | Seleção / revisão | Limite do levantamento |
|---|---|---|---|
| Cargo | Package manifest, root workspace e source pin 1177dad2; diff contra ab7137cd | Um package, um `[[bin]]`, seis deps externas, zero first-party | Sem `cargo metadata` nem grafo resolvido. |
| Jornadas | `journeys/mod.rs`, busca literal `^pub fn run` e source files | 23 dispatches/funções; módulos OCI filhos e `adapters_auth.rs` são auxiliares | Não prova que todas executam em uma configuração. |
| HTTP/processos | Chamadas `reqwest`, `Command::new`, builders e env no source | Relações externas abaixo | Não enviamos request nem executamos subprocesso. |
| Reverse/outside Cargo | Busca literal `e2e-user-journeys` nos paths rastreados fora do package | Wrapper, dois verificadores Python, env scanner, quatro refs Worker e referências textuais | Texto não prova chamada runtime; busca literal não encontra vínculo dinâmico. |
| CI e provider | Busca em `.github/workflows/`, scripts e configs | SBOM, `mutation-pr`, `mutation-nightly`, nightly workspace e wrapper existem; nenhum lane chama este bin por nome | Não é censo semântico nem coleta remota. |
| Workspace root | `Cargo.toml:367-372` e comentário de composição | Membro explícito e regra zero `corelink-*` path-dep | Mudança de membership afeta resolução; não prova build/deploy. |
| Lockfile | `Cargo.lock` bloco `[[package]] name = "e2e-user-journeys"` | Nó com seis dependências externas e sem first-party edge | Lock stale ou alteração de versão exige reconciliação; não rodado. |
| SBOM | `.github/workflows/sbom-consolidated.yml`, `scripts/sbom-aggregate.sh` | Agregação CycloneDX resolve workspace e publica artefato consolidado | Inclusão no SBOM é supply-chain/build evidence, não runtime reachability. |
| Mutants/quality | `.cargo/mutants.toml`; `.github/workflows/mutation-pr.yml:70-189`; `.github/workflows/mutation-nightly.yml:91-290`; `.github/workflows/nightly.yml:448-490` | `exclude_globs = ["tests/e2e-user-journeys/**"]`; PR uses `--in-diff`; nightly matrix names product crates; workspace sweep is global | This bin is explicitly excluded from mutants; no package result is implied. |
| Operação/paths adicionais | `scripts/e2e-real-client/stripe-test-env.sh`, `README.md`, `JOURNEY-MATRIX.md`, `specs/_audits/2026-05-28-e2e-user-journey-suite-seal.md`, `reports/b326-loc-cap-baseline.txt` | Fixtures, contratos, auditoria e inventário textual | São evidências/documentação; Stripe fixture é REL-032, não prova runtime. |

Invariante falsificável: novo `[[bin]]`, primeira dependência `corelink-*`, dispatch, subprocesso ou consumer literal fora do package invalida o escopo desta ficha.

<a id="b03"></a>
## B03 — Registro de relações diretas

| ID | Tipo / direção | Superfície | Ativação | Owner do contrato |
|---|---|---|---|---|
| [REL-001](#rel-001) | HTTP; consumer → provider | Native CAS | Tenant/PAT e journey | Route owner via OKF |
| [REL-002](#rel-002) | HTTP; consumer → provider | Native AC | Tenant/PAT e journey | Route owner via OKF |
| [REL-003](#rel-003) | HTTP; consumer → provider | Bazel REAPI | Instance/PAT | Route owner via OKF |
| [REL-004](#rel-004) | HTTP; consumer → provider | Turbo v8 | Team/tenant/PAT | Route owner via OKF |
| [REL-005](#rel-005) | HTTP; consumer → provider | Cargo/sccache | Tenant/PAT | Adapter owner via OKF |
| [REL-006](#rel-006) | HTTP; consumer → provider | npm registry | Tenant/PAT or anonymous fetch | Adapter owner via OKF |
| [REL-007](#rel-007) | HTTP; consumer → provider | pip registry | Tenant/PAT or anonymous fetch | Adapter owner via OKF |
| [REL-008](#rel-008) | HTTP; consumer → provider | brew + shared cache | Tenant/hash/PAT | Adapter owner via OKF |
| [REL-009](#rel-009) | HTTP; consumer → provider | OCI token and registry | OCI host/PAT | OCI owner via OKF |
| [REL-010](#rel-010) | HTTP; consumer → provider | Identity `/v1/users/me` | Tenant/PAT | Customer/API owner via OKF |
| [REL-011](#rel-011) | HTTP; consumer → provider | PAT/key lifecycle | Admin PAT | Key owner via OKF |
| [REL-012](#rel-012) | HTTP; consumer → provider | Team/member invite | Admin PAT/email fixture | Team owner via OKF |
| [REL-013](#rel-013) | HTTP; consumer → provider | Billing/tier/checkout | Clerk session or tier fixture | Billing owner via OKF |
| [REL-014](#rel-014) | HTTP; consumer → provider | Quota cap drive | Dedicated tenant + opt-in | Quota owner via OKF |
| [REL-015](#rel-015) | HTTP; consumer → provider | DSR/tombstone | Throwaway tenant/hash/session | DSR owner via OKF |
| [REL-016](#rel-016) | HTTP; consumer → provider | Audit export | Admin PAT | Audit owner via OKF |
| [REL-017](#rel-017) | HTTP; consumer → provider | Auth introspection | Internal key/endpoint | Auth owner via OKF |
| [REL-018](#rel-018) | Process; consumer → provider | `corelink` CLI | Slow journey/PATH | CLI owner unresolved here |
| [REL-019](#rel-019) | Process; consumer → provider | `bazel` CLI | Slow journey/PATH | CLI owner unresolved here |
| [REL-020](#rel-020) | Script; consumer → provider | Real-client provisioner | Operator invocation | Operational owner unresolved here |
| [REL-021](#rel-021) | Static input; consumer → provider | B126 verifier | Explicit Python invocation | Script maintainer |
| [REL-022](#rel-022) | Static input; consumer → provider | B270 verifier | Explicit Python invocation | Script maintainer |
| [REL-023](#rel-023) | Source reference; no runtime flow | Worker source/tests | Human navigation | Worker owner separate |
| [REL-024](#rel-024) | Static input; consumer → provider | Secrets env namespace check | Explicit Python invocation | Script maintainer |
| [REL-025](#rel-025) | Workspace declaration; root → package | Root Cargo membership and package policy | Cargo workspace resolution | Workspace/Cargo owner |
| [REL-026](#rel-026) | Lock graph; root → package | `Cargo.lock` package node and registry dependencies | Locked resolution/build | Workspace/Cargo owner |
| [REL-027](#rel-027) | Supply-chain; workspace → SBOM | CycloneDX aggregation and release artifact | Release tag or manual dispatch | Supply-chain owner |
| [REL-028](#rel-028) | Quality lane; config/CI → mutation population | `.cargo/mutants.toml` exclusion plus PR/nightly lanes | PR diff or scheduled mutation lane | CI/testing owner |
| [REL-029](#rel-029) | Static contract; docs → package | README, journey matrix and sealed audit | Human/operator navigation | Test-suite maintainer |
| [REL-030](#rel-030) | Inventory; report → package | LOC/cap report and census references | Campaign audit | Ownership lead |
| [REL-031](#rel-031) | Architecture route; package → OKF | Canonical concept manifest entry | Ownership/route lookup | OKF owner |
| [REL-032](#rel-032) | Static fixture; script → journey | Stripe test environment | Explicit billing test setup | E2E/billing operator |
| [REL-033](#rel-033) | HTTP; consumer → provider | Customer dashboard reads | Admin/PAT and tenant | Customer/API owner via OKF |
| [REL-034](#rel-034) | HTTP; consumer → signup-worker | Stripe webhook simulation | Stripe test opt-in + endpoint/secret | Billing/signup-worker owner |

<a id="rel-001"></a>
### REL-001 — Native CAS

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-cas-001`.
**Dependência / fluxo / impacto:** sem dep Cargo interna; request/body consumer→API e bytes/status API→consumer; rota/auth/hash afetam resultado.

**Superfície:** `journeys/cas.rs`, `concurrency.rs`, `edge.rs`, `security.rs`; `/v1/cas/*` via builders em `harness.rs`.
**Ativação:** dispatch, tenant e PAT adequados.
**Contrato:** put/get/list/batch, isolamento e endereço BLAKE3 segundo asserts source.

**Estado / efeito:** writes podem persistir no tenant; storage pertence ao serviço externo.
**Falha / contenção:** hash/status/corpo divergente dá FAIL; sem rollback uniforme. Valide em tenant descartável; route owner e evidência via OKF/source. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Native AC

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-ac-002`.
**Dependência / fluxo / impacto:** sem dep interna; request de action digest/body consumer→API, resultado JSON/status API→consumer; contrato de chave ou isolamento altera asserts.

**Superfície:** `journeys/ac.rs` e probes relacionadas em `security.rs`; `/v1/ac/*`.
**Ativação:** dispatch, tenant/PAT; ação pode escrever refs.
**Contrato:** update, lookup, listagem e separação tenant são verificados no source.

**Estado / efeito:** refs remotas podem persistir; o harness não lê store.
**Falha / contenção:** cross-tenant ou body inesperado interrompe; validar em tenant throwaway. Owner e fonte da rota pelo OKF; nenhuma chamada observada. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Bazel REAPI HTTP

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-bazel-reapi-003`.
**Dependência / fluxo / impacto:** sem dep interna; requests de blobs/instance consumer→API, resposta/hash API→journey; tamanho e auth mudam resultado.

**Superfície:** `journeys/bazel.rs`, `edge.rs`; `/bazel/v2/*` por `url_bazel_*`.
**Ativação:** journey e PAT/instance; CLI Bazel é fronteira separada em REL-019.
**Contrato:** upload/read/findMissing conforme builders e asserts, sem prova do cliente externo.

**Estado / efeito:** uploads podem persistir sob instance remoto.
**Falha / contenção:** status/hash incorreto é FAIL; use instance descartável. Owner via OKF; request e provider não observados. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Turbo v8

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-turbo-004`.
**Dependência / fluxo / impacto:** sem dep interna; artifact/status/events são enviados consumer→API e respostas retornam; team/hash/shape alteram asserts.

**Superfície:** `journeys/turbo.rs`; `/v8/artifacts/*` e status/events via builders.
**Ativação:** dispatch e tenant/team/PAT.
**Contrato:** write/read, auth, eventos e isolamento conforme source.

**Estado / efeito:** artifact/evento pode persistir no endpoint externo.
**Falha / contenção:** body/status incorreto dá FAIL; usar tenant e artifact descartáveis. Owner via OKF; nenhum endpoint chamado. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Cargo/sccache

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-cargo-adapter-005`.
**Dependência / fluxo / impacto:** sem dep first-party; cache key e bytes seguem HTTP consumer→adapter, resultado/headers voltam; mudança de route/auth atinge journey.

**Superfície:** segmento `/cargo/{tenant}/{key}` em `journeys/adapters.rs`.
**Ativação:** chamada da jornada com tenant/PAT.
**Contrato:** round-trip e negação de auth são assertados; não é execução de Cargo CLI neste arquivo.

**Estado / efeito:** GET/PUT podem gravar ou popular cache remoto.
**Falha / contenção:** mismatch interrompe; use tenant de teste. Route owner via OKF; sem observação de runtime. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — npm registry

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-npm-adapter-006`.
**Dependência / fluxo / impacto:** sem dep interna; nome/metadata/artefato via HTTP e body/status de volta; alteração de auth ou shape afeta assert.

**Superfície:** `/npm/{tenant}/...` em `journeys/adapters.rs`.
**Ativação:** journey tenant-scoped e probes anônimos.
**Contrato:** fetch público/negação e integridade conforme funções locais.

**Estado / efeito:** GET pode alcançar cache/logs externos.
**Falha / contenção:** content/auth fora da asserção é FAIL; não infira instalação npm. Owner pelo OKF; nenhuma request ocorreu. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — pip registry

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-pip-adapter-007`.
**Dependência / fluxo / impacto:** sem dep interna; path/index/download segue consumer→API e bytes voltam; contrato de pacote afeta asserts.

**Superfície:** `/pip/{tenant}/...` em `journeys/adapters.rs`.
**Ativação:** probe auth/PAT ou fetch permitido.
**Contrato:** leitura de índice/artefato e deny shapes são observados somente em source.

**Estado / efeito:** requests podem gerar logs/cache externos; sem write de pacote afirmado.
**Falha / contenção:** corpo/status imprevisto dá FAIL; validar host de teste. Owner via OKF; provider não observado. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — brew e cache público

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-brew-public-cache-008`.
**Dependência / fluxo / impacto:** sem dep interna; pedido bottle/hash e resposta bytes percorrem HTTP; compartilhamento cross-tenant pode expor dado se boundary mudar.

**Superfície:** `journeys/adapters.rs`, `shared_cache.rs`; `/brew/*`, namespace `_public`.
**Ativação:** tenant/PAT, hash/path de fixture; alguns probes usam GET.
**Contrato:** bytes/hash e public/private tenant boundary são asserções locais.

**Estado / efeito:** GET pode popular cache; artefato público pode ser compartilhado.
**Falha / contenção:** bytes ou visibilidade inesperados: parar; evitar artifact privado. Owner pelo OKF; execução não ocorreu. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — OCI token e registry

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-oci-009`.
**Dependência / fluxo / impacto:** sem dep first-party; Basic PAT→token endpoint, bearer→registry e resposta→journey; mudança afeta pull/push asserts.

**Superfície:** `journeys/oci.rs`, `oci_public_isolation.rs`, submódulos `oci/{early,late}/**`; `/token`, `/v2/*`.
**Ativação:** PAT, OCI host opcional, prewarm em `journeys::all`.
**Contrato:** challenge, token e operações OCI são fonte/assert local, não provider proof.

**Estado / efeito:** upload/tag pode persistir; cleanup geral não demonstrado.
**Falha / contenção:** challenge/token/body fora do esperado dá FAIL; use registry descartável. Host e owner pelo OKF; nada foi executado. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Identity `/v1/users/me`

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-customer-read-010`.
**Dependência / fluxo / impacto:** sem dep interna; PAT/query→API e JSON/status→journey; mudança de tenant/auth/shape altera asserts.

**Superfície:** `identity.rs`; health e `/v1/users/me`.
**Ativação:** endpoint e, para dados protegidos, tenant/PAT.
**Contrato:** identidade e auth status conforme asserções de `identity.rs`; dashboard é REL-033.

**Estado / efeito:** leitura; provider pode registrar request.
**Falha / contenção:** resposta ausente ou tenant diferente interrompe; sem consulta interna. Owner de rota via OKF; endpoint não observado. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — Customer dashboard reads

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-dashboard-033`.
**Dependência / fluxo / impacto:** sem dep interna; overview/usage/billing JSON e status retornam ao journey; mudança de auth, tenant ou shape altera asserts.

**Superfície:** `dashboard.rs`; `/v1/customer/overview`, `/v1/customer/usage`, `/v1/customer/billing`.
**Ativação:** dispatch, tenant e PAT/admin conforme o journey.
**Contrato:** leitura de conta; não inclui identity, key lifecycle, audit ou team.

**Estado / efeito:** leitura remota, com logs do provider possíveis. **Falha / contenção:** resposta ou tenant inesperado dá FAIL; valide tenant descartável e resolva owner pelo OKF. Nenhuma chamada foi observada. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — PAT/key lifecycle

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-key-lifecycle-011`.
**Dependência / fluxo / impacto:** sem dep interna; pedidos admin de key/list/revoke e respostas retornam; mutação pode invalidar PAT.

**Superfície:** `pat_lifecycle.rs`, listagem de keys em `dashboard.rs`, fixture em `audit.rs`; `/v1/customer/keys/*`.
**Ativação:** admin/write PAT e dados de teste.
**Contrato:** criação, uso, revoke/deny conforme funções locais.

**Estado / efeito:** keys são recurso remoto; credential revogada deixa de autenticar.
**Falha / contenção:** token/tenant imprevisto: parar; usar throwaway key/tenant. Key owner via OKF; nenhuma mutação observada. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Team invite e membro

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-team-012`.
**Dependência / fluxo / impacto:** sem dep interna; invite email/admin request→API e status/member response→journey; invite pode notificar terceiro.

**Superfície:** `team.rs` e fixture em `audit.rs`; `/v1/customer/team*`.
**Ativação:** admin PAT + email/member fixture; alguns caminhos ficam GATED sem fixture.
**Contrato:** convite/status, membership e deny scope como source assertions.

**Estado / efeito:** membership/invite remoto; e-mail pode ser irreversível.
**Falha / contenção:** endereço/tenant incorreto interrompe antes de repetir; usar email controlado e tenant descartável. Owner pelo OKF; não executado. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Billing, tier e checkout

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-billing-013`.
**Dependência / fluxo / impacto:** sem dep interna; sessão/tier fixture→API e resposta volta; mudança de auth, URL ou shape altera asserts.

**Superfície:** `billing.rs`, `runner_purchase.rs`; portal, tier-select e checkout.
**Ativação:** Clerk session ou fixture de tier; webhook é REL-034.
**Contrato:** deny sem auth e URL/session shape conforme source.

**Estado / efeito:** checkout pode criar sessão remota. **Falha / contenção:** 2xx sem shape esperado é FAIL; use fixture test e aprovação. Billing owner via OKF; nenhuma chamada ocorreu. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Quota cap

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-quota-014`.
**Dependência / fluxo / impacto:** sem dep interna; cap-drive envia writes até limite e consome bytes; resultado de quota volta como status.

**Superfície:** `quota.rs` e quota probes em `abuse.rs`; native CAS API.
**Ativação:** tenant/token dedicado e `CORELINK_E2E_QUOTA_TEST=1` ou legacy slow flag em caminhos específicos.
**Contrato:** cap deve negar sem erro inadequado conforme predicados locais.

**Estado / efeito:** pode encher quota e gravar objetos.
**Falha / contenção:** risco de custo/espaço; parar sem tenant near-limit descartável e recuperação aprovada. Owner pelo OKF; não executado. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — DSR e tombstone

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-dsr-015`.
**Dependência / fluxo / impacto:** sem dep interna; request/hash/tenant vai à API, leitura de gone/status volta; exclusão errada afeta dados.

**Superfície:** `dsr.rs`; customer delete/request, read-410 e internal-auth probes.
**Ativação:** tenant/hash/session/opt-in conforme função.
**Contrato:** source separa customer surface e transport interno; harness não acessa store.

**Estado / efeito:** solicitação/exclusão pode ser irreversível ou assíncrona.
**Falha / contenção:** somente recurso throwaway e operação autorizada; pare em ambiguidade. DSR owner via OKF; não executado. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Audit export

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-audit-016`.
**Dependência / fluxo / impacto:** sem dep interna; admin export request→API, rows/hash chain→journey; resposta/shape errada altera veredito.

**Superfície:** `audit.rs`, leitura `/v1/customer/audit`; gera fixtures por APIs keys/team em REL-011/012.
**Ativação:** admin PAT e limite/paginação.
**Contrato:** export e rederive local da cadeia segundo asserts; sem acesso DB.

**Estado / efeito:** leitura após fixture writes; rows remotas persistem conforme serviço.
**Falha / contenção:** cadeia inválida interrompe; fixtures em tenant isolado. Owner via OKF; execução não observada. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Auth introspection

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-introspect-017`.
**Dependência / fluxo / impacto:** sem dep interna; request de introspect + key interno→endpoint, resultado auth/tenant→journey; erro pode mascarar alcance.

**Superfície:** `introspect.rs`, `runners.rs`, probes em `abuse.rs`; `/internal/v1/auth/introspect`.
**Ativação:** `CORELINK_E2E_INTROSPECT_KEY` ou probe de negação.
**Contrato:** shape/token validity e deny sem key; fonte diz rota pode não ser pública.

**Estado / efeito:** leitura de auth; sem mudança própria afirmada.
**Falha / contenção:** não inferir reachability; sucesso inesperado sem key interrompe. Owner pelo OKF; não chamado. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Processo corelink

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-corelink-cli-018`.
**Dependência / fluxo / impacto:** sem dep Cargo CLI; args/env/files consumer→`corelink`, exit/stdout→journey; CLI diferente altera resultado.

**Superfície:** chamada `Command::new("corelink")` em `journeys/bazel.rs`.
**Ativação:** jornada lenta, bin em PATH e config.
**Contrato:** somente caminho processual declarado; não prova cliente publicado.

**Estado / efeito:** pode escrever config/cache e chamar API externa.
**Falha / contenção:** ausência pode GATED; erro é FAIL; usar dir temporário dedicado e host fixture. Owner CLI não resolvido; não executado. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Processo bazel

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-bazel-cli-019`.
**Dependência / fluxo / impacto:** sem dep Cargo; workspace/config→`bazel`, exit/stdout/cache result→journey; CLI flags/config impactam sucesso.

**Superfície:** chamadas `Command::new("bazel")` em `journeys/bazel.rs`.
**Ativação:** slow journey, PATH e fixture de projeto.
**Contrato:** comportamento do processo local declarado, distinto das requests REAPI de REL-003.

**Estado / efeito:** pode criar cache/arquivos e falar com endpoint.
**Falha / contenção:** não limpar arquivo preexistente; remover só temp criado. CLI owner externo; processo não executado. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — Provisionador real-client

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-provisioner-020`.
**Dependência / fluxo / impacto:** script chama suite via Cargo; passa endpoint/tokens e consome exit/status; package change afeta runbook.

**Superfície:** `scripts/e2e-real-client/provision-and-run-suite.sh`.
**Ativação:** invocação por operador; bootstrap Clerk/CoreLink e EXIT cleanup.
**Contrato:** cria usuários/PATs e chama bin; não é alvo Cargo do package.

**Estado / efeito:** pode criar users/keys, invites, DSR e writes remotos.
**Falha / contenção:** nunca usar como check local; cleanup pode ser assíncrono. Operador e autorização próprios; fonte lida, script não executado. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Verificador B126

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-b126-verifier-021`.
**Dependência / fluxo / impacto:** sem import runtime; script lê arquivos Rust como texto e retorna resultado; rename/shape quebra verificador.

**Superfície:** `scripts/verify_b126_t3_refactor.py` lista `adapters.rs` e `adapters_auth.rs`.
**Ativação:** execução explícita de Python.
**Contrato:** expectativa estática do script, não API.

**Estado / efeito:** leitura/escrita de arquivos locais conforme script.
**Falha / contenção:** revisar contrato do verificador com seu maintainer; não foi executado nesta tarefa. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Verificador B270

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-b270-verifier-022`.
**Dependência / fluxo / impacto:** sem import runtime; script lê/refere source e resultado vai ao operador; mudanças de adapters podem divergir.

**Superfície:** `scripts/verify_b270_b281_bundle_repairs.py` inclui adapter auth na validação.
**Ativação:** execução explícita de Python.
**Contrato:** check estático; nenhum efeito em API/provider.

**Estado / efeito:** resultado local.
**Falha / contenção:** coordenar mudança de expectativa com maintainer do script. Script não executado. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Referências textuais Worker

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-worker-source-reference-023`.
**Dependência / fluxo / impacto:** sem import ou dado runtime; comentários citam linhas da suite; mover linhas pode tornar link textual stale.

**Superfície:** `worker/src/index_public_health.ts`, `route_match.ts`, `worker/tests/index_part3_tests_{1,2}.ts`.
**Ativação:** navegação/revisão humana, não request.
**Contrato:** referência textual sem garantia de execução.

**Estado / efeito:** nenhum fluxo runtime demonstrado.
**Falha / contenção:** corrigir referência com maintainer Worker; stale line não prova falha API. Arquivos lidos; Worker não foi executado. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Secrets matrix namespace

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-env-matrix-024`.
**Dependência / fluxo / impacto:** sem import Cargo ou runtime; scanner mantém prefixo `CORELINK_E2E_`; mudança de env altera classificação/cobertura.

**Superfície:** `scripts/validate_secrets_matrix.py`; allowlist da família de vars de teste.
**Ativação:** execução explícita do scanner; não chama a suite nem lê secret value.
**Contrato:** nomes `CORELINK_E2E_*` são config de teste segundo regra/comentário do scanner.

**Estado / efeito:** classificação estática local apenas.
**Falha / contenção:** prefixo ausente pode omitir nome de env; coordenar com maintainer. Scanner não executado nesta tarefa. [Relation index](#b03)


<a id="rel-025"></a>
### REL-025 — Workspace membership and root policy

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-workspace-025`.
**Superfície:** `Cargo.toml:367-372`, package `tests/e2e-user-journeys/Cargo.toml`.
**Ativação:** Cargo workspace resolution, package selection or root policy review.
**Contract/effect:** root membership makes the package discoverable; the root comment and manifest forbid first-party path dependencies. Removing membership or adding a `corelink-*` path edge changes the build population and ownership census.
**Failure/validation:** inspect exact root/member blocks and re-run package census; no compile or runtime proof. Root Cargo owner coordinates global changes. [Relation index](#b03)


<a id="rel-026"></a>
### REL-026 — Cargo.lock package node

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-lock-026`.
**Superfície:** `Cargo.lock` `[[package]] name = "e2e-user-journeys"` block.
**Ativação:** dependency/version or lockfile change.
**Contract/effect:** the pinned node lists `blake3`, `hex`, `reqwest`, `serde_json`, `sha2 0.11.0` and `uuid`; no first-party package edge is recorded. A stale or regenerated node changes reproducibility and SBOM inputs.
**Failure/validation:** compare the node with the manifest and inspect a scoped diff; use the authorized lockfile procedure, never infer resolution from `cargo tree --target all`. [Relation index](#b03)


<a id="rel-027"></a>
### REL-027 — Consolidated CycloneDX SBOM

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-sbom-027`.
**Superfície:** `.github/workflows/sbom-consolidated.yml`, `scripts/sbom-aggregate.sh`.
**Ativação:** `v*` tag or manual workflow dispatch.
**Contract/effect:** cargo-cyclonedx resolves workspace members and the aggregate publishes `target/sbom/corelink-workspace.cdx.json`; the package may appear as a component/dependency node.
**Failure/validation:** missing/stale package component is a supply-chain defect; inspect generated CycloneDX evidence and workflow logs. SBOM inclusion does not prove deploy or request reachability. [Relation index](#b03)


<a id="rel-028"></a>
### REL-028 — Workspace cargo-mutants lane

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-mutants-028`.

**Superfície:** `.cargo/mutants.toml`; `.github/workflows/mutation-pr.yml:70-189` (`cargo mutants --in-diff`); `.github/workflows/mutation-nightly.yml:91-290` (matrix); `.github/workflows/nightly.yml:448-490` (`--workspace`).

**Ativação:** PR Rust diff or scheduled lane. `exclude_globs = ["tests/e2e-user-journeys/**"]` excludes this package; the nightly matrix names product crates, while workspace sweep is global.

**Contract/effect:** these lanes govern mutation population, not journey execution; `.cargo/mutants.toml` is the decisive exclusion.

**Failure/validation:** not counted as a direct package consumer and no kill rate is claimed. Validate config/workflow diff with CI owner; no mutation run was observed. [Relation index](#b03)


<a id="rel-029"></a>
### REL-029 — README, matrix and sealed audit contract

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-docs-029`.
**Superfície:** package `README.md`, `JOURNEY-MATRIX.md`, `specs/_audits/2026-05-28-e2e-user-journey-suite-seal.md`.
**Ativação:** operator navigation, journey planning or documentation review.
**Contract/effect:** README states black-box/command constraints; matrix states persona/surface/path coverage; audit records the original seal. They guide the binary but do not call it at runtime.
**Failure/validation:** stale route, target or journey count can misroute an owner; reconcile each document to source and preserve contradictions rather than treating prose as execution evidence. [Relation index](#b03)


<a id="rel-030"></a>
### REL-030 — LOC and ownership census reports

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-reports-030`.
**Superfície:** `reports/b326-loc-cap-baseline.txt`, `docs/ownership/CARGO_CENSUS.md`, `docs/ownership/WAVE_015_PLAN.md`.
**Ativação:** campaign capacity, identity or drift audit.
**Contract/effect:** reports enumerate source paths and package identity; WAVE_015_PLAN retains a historical 22-module count, corrected by the pinned source census of 23 dispatches and 23 implementers. Reports do not alter or execute the package.
**Failure/validation:** rerun the source-scoped census and preserve the historical plan value as such; static count does not establish execution or runtime completeness. [Relation index](#b03)


<a id="rel-031"></a>
### REL-031 — Canonical OKF route

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-okf-031`.
**Superfície:** `docs/internal/okf-wiki/concept-manifest.yaml` entry `tests/e2e-user-journeys`.
**Ativação:** contract/owner routing or escalation lookup.
**Contract/effect:** OKF classifies the surface as a test harness outside architecture concepts; it supplies the canonical route for external API/owner questions and does not transfer implementation ownership.
**Failure/validation:** absent or contradictory entry is an explicit unknown; do not infer a route owner from package or directory names. [Relation index](#b03)


<a id="rel-032"></a>
### REL-032 — Stripe test environment fixture

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-stripe-env-032`.
**Superfície:** `scripts/e2e-real-client/stripe-test-env.sh` e variáveis `CORELINK_E2E_STRIPE_*` consumidas por `billing.rs`.
**Ativação:** operador habilita Stripe test; não é executado automaticamente pelo bin.
**Contrato/efeito:** o script fornece fixtures, endpoint e flags ao ambiente; não prova checkout, cobrança ou webhook.
**Falha/validação:** variável ausente deve GATED; valor/tenant incorreto pode atingir serviço indevido. Revisar nomes e logs redigidos; script não foi executado. [Relation index](#b03)


<a id="rel-034"></a>
### REL-034 — Stripe webhook via signup-worker

**Identidade:** `repo:1232040291:boundary:e2e-user-journeys-stripe-webhook-034`.
**Dependência / fluxo / impacto:** billing journey envia evento assinado ao `SIGNUP_WORKER_ENDPOINT`; resposta/status e eventual mudança de tier retornam ao journey.
**Superfície:** `billing.rs`, `harness.rs::url_stripe_webhook`, `scripts/e2e-real-client/provision-and-run-suite.sh`.
**Ativação:** `CORELINK_E2E_STRIPE_WEBHOOK_TEST`, endpoint, secret e IDs de teste.
**Contrato/efeito:** assinatura, status e fixture billing são verificados; pode mutar estado externo de teste.
**Falha/validação:** 2xx sem efeito esperado ou endpoint não dedicado é parada; owner signup-worker/billing autoriza e reconcilia. Não executado. [Relation index](#b03)


<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho por RELs | Condição | Efeito causal | Contenção / validação |
|---|---|---|---|---|
| Veredito do bin | REL-001..019, REL-033..034 → `JourneyResult` → INV-001 | Jornada executada | Contrato remoto inválido produz FAIL/RED; GATED não certifica | Exigir piso e interpretar GATED por pré-condição. |
| APIs remotas | REL-001..017, REL-033/034 | Host/credenciais fornecidos | Writes podem persistir fora do processo | Endpoint/tenant descartável e autorização; sem rollback geral. |
| Processo local | REL-018/019 | Slow path/PATH válido | CLI pode chamar serviço e deixar arquivos | Isolar dir e ambiente; exit 0 não prova deploy. |
| Wrapper provisionado | REL-020 → bin → REL-001..017, REL-032/034 | Invocação autorizada | Recursos podem ser criados; DSR é assíncrono | Não rerun após timeout sem reconciliação. |
| Checks e referências | REL-021..024 | Script/revisão textual | Fonte/env alterada pode quebrar check ou link | Coordenar com owner do verifier/Worker. |
| Workspace e supply chain | REL-025/026 → REL-027; REL-028 é global e excluído do census direto; REL-029..031 | Membership/lock/release/auditoria | Mudança de root/lock pode alterar resolução, SBOM ou navegação | Revisar diffs e artefatos; não alegar runtime. |
| Billing test path | REL-032 → REL-013/034 → provider | Stripe test e webhook opt-in | Fixture ou webhook pode mudar estado billing externo | Endpoint/tenant/secret de teste e reconciliação autorizada. |

**Cobertura:** relações terminam em fronteira HTTP/processo ou consumer textual. Não há caminho witness de handler a banco/storage/provider.

**Caminhos alternativos materiais:** OCI usa host opcional; Clerk session e webhook signup-worker são atores distintos do PAT API.

**Não alcance comprovado:** nenhum Cargo graph, workflow, request ou backend foi exercitado; busca literal não exclui vínculo dinâmico.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | API / INV / REL afetados | Consumidores / estado | Validação necessária | Coordenação / recuperação |
|---|---|---|---|---|
| CAS/AC route, hash ou auth | API-003, INV-003, REL-001/002/014 | Requests e writes de tenant | M04; local unit; HTTP só em fixture | Owner OKF; revert não remove dados. |
| Registry/protocol URL/assert | API-003, REL-003..009 | Blobs, events, tags ou cache | M04; validar bytes/hash/auth | Owner de rota via OKF; recuperar por API autorizada. |
| Identity | API-002/003, REL-010 | Acesso e identidade | M04; deny + estado final | Tenant throwaway; coordenar owner. |
| Dashboard/PAT/team/audit | API-002/003, REL-011/012/016/033 | Acesso, token, email e leitura administrativa | M04; deny + estado final | Tenant/email throwaway; coordenar owner. |
| Billing/tier/checkout | REL-013 | Sessão, plano e checkout remotos | M04 com fixture dedicada | Parar sem recovery/owner; Git não desfaz operação. |
| Stripe webhook/test fixture | REL-032/034 | Estado billing externo e secret/endpoint | M04 com endpoint e tenant de teste | Autorizar signup-worker; não repetir após timeout. |
| DSR/quota | REL-014/015 | Dados e cap remotos | M04 com fixture dedicada | Parar sem recovery/owner; Git não desfaz operação. |
| Introspect/runner gates | INV-002, REL-017 | Acesso interno | Verificar credencial e deny | Não expor internal key; owner OKF. |
| CLI invocation | REL-018/019 | Env, cache, files e API | PATH/config e ambiente isolado | Limpar só temp novo; coordenar CLI owner. |
| Output/provisioner contract | API-001, REL-020 | Wrapper bootstrap/cleanup | Atualizar/verificar wrapper via owner | Não executar provisionamento em revisão. |
| Source layout/static references | REL-021..024 | Python checks, env scanner e Worker comments | Census direto/inverso | Coordenar mantenedor, sem inferir runtime. |
| Workspace/lock/SBOM/quality | REL-025..028 | Membership, resolução, supply-chain ou população de mutants muda | PROC-001/006; ler diffs e artefatos, sem converter presença em runtime proof. |
| Documentação/census/OKF | REL-029..031 | Rota, contagem ou owner pode ficar stale | Reconciliar ao source pin e ao OKF; preservar unknowns. |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| Manifestos/targets próprios | 1 package / 1 bin | 1 / 1 | 0 | Resolução Cargo não feita. |
| Declarações first-party | 0 | 0 | 0 | Dependências transitivas não resolvidas. |
| `run` dispatch/functions | 23 | 23 | 0 | Source: 23 chamadas `out.extend` e 23 implementadores `pub fn run`; o 22 histórico em WAVE_015_PLAN foi corrigido por este censo. Não prova execução/completude runtime. |
| Fronteiras HTTP | 17 | 17 | 0 | Owner de rota não reidentificado nesta autoria. |
| Processos/scripts | 3 | 3 | 0 | Comportamento local/live não executado. |
| Reverse static consumers | 10 code/config/root/lock/SBOM/quality/docs paths | 10 | Docs/provisioner/comment/CI não runtime | Consumers indiretos fora de literal census. |
| Workflows/supply chain | REL-027 SBOM; REL-028 `.cargo/mutants.toml` + `.github/workflows/mutation-pr.yml`, `mutation-nightly.yml`, `nightly.yml` | 1 direto / 1 excluído | 0 package-specific executions | `exclude_globs` exclui este package; nenhum kill-rate. |

**Censo de paths (8+):** `Cargo.toml` raiz; `tests/e2e-user-journeys/Cargo.toml`; `Cargo.lock`; `.github/workflows/sbom-consolidated.yml`; `scripts/sbom-aggregate.sh`; `.github/workflows/nightly.yml`; `scripts/e2e-real-client/provision-and-run-suite.sh`; `scripts/e2e-real-client/stripe-test-env.sh`; `scripts/verify_b126_t3_refactor.py`; `scripts/verify_b270_b281_bundle_repairs.py`; `scripts/validate_secrets_matrix.py`; quatro referências em `worker/`; README/matriz/auditoria/LOC. Cada path tem relação ou exclusão explícita acima.

**Exclusões enumeradas:** `journeys/oci/{early,late}/**` e `adapters_auth.rs` não têm função `run` própria; são helpers em REL-009/005. REL-028 é workflow global sem seleção deste bin e fica fora do census direto. Changelog, ADR, comentário em `scripts/provision-cf-corelink-prod.sh` e referências Worker são fontes/texto, não runtime relations. SBOM não prova runtime. Sem first-party Cargo edge declarado.

**Reconciliação de contagem:** em `1177dad2`, `journeys/mod.rs` declara 23 dispatches `out.extend(<module>::run(cfg, client))`; `rg '^pub fn run\(cfg: &Config, client: &Client\)' tests/e2e-user-journeys/src/journeys/*.rs` encontra 23 implementadores e os nomes/ordem reconciliam com o dispatcher.

O valor 22 em WAVE_015_PLAN é um claim de planejamento histórico corrigido por esta evidência; o plano não foi editado. Este é um censo estático, não prova seleção Cargo, execução, êxito ou completude runtime.

**O que não foi observado:** consumers dinâmicos, API/provider real, owners individuais, resultados, cleanup, workflows ou storage. Igualdade de contagem não prova completude.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
