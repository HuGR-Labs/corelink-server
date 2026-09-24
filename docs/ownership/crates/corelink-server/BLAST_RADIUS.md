---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-server
manifest: crates/corelink-container/Cargo.toml
source_commit: 91630baebe3ae7abe686cd4e06a5621ecdc4ab73
profile: H
state: draft
evidence_set: server-main-readback-20260923-91630
---

# corelink-server — blast radius

Source status: this document is pinned to immutable `91630ba`. The bounded
readback records the current package tree and durable-classification test,
focused workflow, and operator recovery-contract delta; see [readback](../../evidence/revision-1.4/SERVER-MAIN-READBACK-20260923-91630.md).
This does not certify workflow execution, resolution, build selection, or runtime.

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

O server compõe HTTP nativo e seleciona adapters para dados, cobrança e KMS.
Os riscos são gate ausente, fallback indevido, mount incorreto e provider errado.
As relações abaixo são estáticas; runtime, deploy e I/O remoto não foram observados.

<a id="b02"></a>
## B02 — Método

| População | Fonte | Resultado | Limite |
|---|---|---|---|
| targets/features | manifesto e fontes fixadas | lib, 2 bins, 13 targets de teste, 14 arquivos Rust de teste e 7 features | não prova entrega |
| composição e inventário source | pin `91630ba`, `git ls-tree -r`; `routes/build.rs`, manifesto e imports first-party | 401 `.rs` em `src/`, 14 em `tests/`; 15 merge roots incondicionais, 6 routers condicionais, 5 layers e 31 dependências first-party declaradas | censo cobre as fronteiras do composition root; não enumera todos os paths HTTP internos |
| router roots incondicionais | `routes/build.rs:421-437` | todos os 15 merge sites, um por REL-008 e REL-057–070 | módulos e contratos internos continuam pertencendo aos respectivos handlers |
| router roots condicionais | signup `:444-445`; adapters `:487-612` | signup e cargo/brew/npm/oci/pip; cada mount tem relação separada REL-009 e REL-072–076 | segredo/env, recursos e reachability em execução não observados |
| layers instaladas | `routes/build.rs:629-716` | residency, failover, rate limit, OTel condicional e timing; REL-011 e REL-077–081 | ordem e wiring source não provam tráfego/runtime |
| source pin / drift | evidência readback campaign snapshot → `origin/main` 91630ba | runtime helper/DSR classification extraction e teste de ACK perdido identificados; 401 fontes Rust em `src/` e 14 em `tests/` | comparação limitada aos caminhos documentados; não valida outros documentos/linhagens |
| graph | inspeção estática de manifestos/workspace | nenhum package consumidor direto visível | grafo Cargo resolvido não executado |
| deps first-party | manifesto + busca estática de símbolos | 31 entradas declaradas; 175 arquivos com usos | relação semântica por dependência aberta |
| adapters | source e OKF | R2, D1, Stripe, BYOK, GC | sem credencial/operação |

<a id="b03"></a>
## B03 — Relações diretas

| ID | Tipo/direção | Superfície | Ativação | Owner |
|---|---|---|---|---|
| [REL-001](#rel-001) | server→Axum | listener/router | boot | server |
| [REL-002](#rel-002) | server→storage | StorageEnv/R2/D1 | env/rota | storage |
| [REL-003](#rel-003) | server→route gate | build/mount | boot | server/handler |
| [REL-004](#rel-004) | feature→BYOK | provider/KMS | compile/boot | BYOK |

| ID | Tipo/direção | Superfície | Ativação | Owner |
|---|---|---|---|---|
| [REL-005](#rel-005) | bin→GC | sweep | execução bin | GC |
| [REL-006](#rel-006) | server→Stripe | webhook | secret/mount | Stripe |
| [REL-007](#rel-007) | server→telemetry | health/logs | boot/request | observability |
| [REL-008](#rel-008) | CAS router→server | merge incondicional | boot | handler CAS/server |

| ID | Tipo/direção | Superfície | Ativação | Owner |
|---|---|---|---|---|
| [REL-009](#rel-009) | env→signup | HMAC pilot route | secret válido | signup |
| [REL-072](#rel-072) | verifier/D1→Cargo router | `/cargo/*` | PAT+D1 | adapter-host/storage |
| [REL-073](#rel-073) | verifier/D1→Brew router | `/brew/*` | PAT+D1 | adapter-host/storage |
| [REL-074](#rel-074) | verifier/D1/KV→npm router | `/npm/*` | PAT+D1+metadata KV | adapter-host/storage |
| [REL-075](#rel-075) | verifier/D1/KV/key→OCI router | `/v2/*` | PAT+D1+manifest KV+token key | adapter-host/storage |
| [REL-076](#rel-076) | verifier/D1/KV→pip router | `/pip/*` | PAT+D1+index KV | adapter-host/storage |
| [REL-011](#rel-011) | layer composition order | data plane stack | router build | server/policy |
| [REL-077](#rel-077) | request→residency guard | data plane | router build | residency |
| [REL-078](#rel-078) | request→failover guard | data plane | regional state | failover |
| [REL-079](#rel-079) | request→rate limiter | data plane | router build | ratelimit |
| [REL-080](#rel-080) | response→OTel exporter | data plane | configured exporter | telemetry |
| [REL-081](#rel-081) | request→origin timing | data plane | router build | server/Worker |
| [REL-012](#rel-012) | build/test→server | proto + integration targets | build/test | package |
| [REL-071](#rel-071) | workflow→durable-classification test | PR paths/manual dispatch | selected test | package/CI |
| [REL-057](#rel-057) | AC router→server | merge incondicional | boot | handler AC/server |
| [REL-058](#rel-058) | admin router→server | merge incondicional | boot | handler admin/server |
| [REL-059](#rel-059) | admin tenant detail→server | merge incondicional | boot | handler admin/server |
| [REL-060](#rel-060) | admin pilot→server | merge incondicional | boot | handler admin/server |
| [REL-061](#rel-061) | BYOK admin→server | merge incondicional | boot | BYOK/server |
| [REL-062](#rel-062) | audit export→server | merge incondicional | boot | audit/server |
| [REL-063](#rel-063) | audit analytics→server | merge incondicional | boot | analytics/server |
| [REL-064](#rel-064) | users→server | merge incondicional | boot | users/server |
| [REL-065](#rel-065) | customer→server | merge incondicional | boot | customer/server |
| [REL-066](#rel-066) | DSR portal→server | merge incondicional | boot | DSR/server |
| [REL-067](#rel-067) | customer runners→server | merge incondicional | boot | runners/server |
| [REL-068](#rel-068) | workspaces→server | merge incondicional | boot | workspaces/server |
| [REL-069](#rel-069) | Bazel v2→server | merge incondicional | boot | Bazel/server |
| [REL-070](#rel-070) | Turbo v8→server | merge incondicional | boot | Turbo/server |
| [REL-055](#rel-055) | server→DSR registry | migrations 0134–0140; 0133/0138 excluded | source/test gate | DSR/privacy |
| [REL-056](#rel-056) | Worker quota header→Cargo PUT | trusted header/task-local cap | Cargo PUT | storage/accounting |

**Índice das dependências first-party declaradas**

| Relação | Package | Relação | Package |
|---|---|---|---|
| [REL-013](#rel-013) | adapter-host | [REL-014](#rel-014) | analytics |
| [REL-015](#rel-015) | audit | [REL-016](#rel-016) | audit-chain |
| [REL-017](#rel-017) | bazel-bridge | [REL-018](#rel-018) | billing |
| [REL-019](#rel-019) | billing-emit | [REL-020](#rel-020) | billing-stripe-materializer |
| [REL-021](#rel-021) | BYOK | [REL-022](#rel-022) | CF bindings |
| [REL-023](#rel-023) | core | [REL-024](#rel-024) | DPA acceptance |
| [REL-025](#rel-025) | DSR | [REL-026](#rel-026) | erasure-attestation |
| [REL-027](#rel-027) | failover-router | [REL-028](#rel-028) | GC |
| [REL-029](#rel-029) | handler AC | [REL-030](#rel-030) | handler admin |
| [REL-031](#rel-031) | handler CAS | [REL-032](#rel-032) | handler CAS erase |
| [REL-033](#rel-033) | handler customer | [REL-034](#rel-034) | hash |
| [REL-035](#rel-035) | PAT | [REL-036](#rel-036) | privacy erasure worker |
| [REL-037](#rel-037) | ratelimit | [REL-038](#rel-038) | SLO |
| [REL-039](#rel-039) | Stripe real | [REL-040](#rel-040) | telemetry |
| [REL-041](#rel-041) | tenant-path | [REL-042](#rel-042) | tier-selection |
| [REL-043](#rel-043) | turbo-bridge |  |  |

**Caminhos semânticos de audit chain:** [REL-044](#rel-044) · [REL-045](#rel-045) · [REL-046](#rel-046).

**Fronteiras compartilhadas com hash:** [REL-047](#rel-047) · [REL-048](#rel-048) · [REL-049](#rel-049) · [REL-050](#rel-050) · [REL-082](#rel-082) · [REL-083](#rel-083) · [REL-084](#rel-084) · [REL-085](#rel-085) · [REL-086](#rel-086) · [REL-087](#rel-087).

**Métodos customer source-only:** [REL-088](#rel-088) · [REL-089](#rel-089) · [REL-090](#rel-090) · [REL-091](#rel-091) · [REL-092](#rel-092) · [REL-093](#rel-093) · [REL-094](#rel-094) · [REL-095](#rel-095) · [REL-096](#rel-096) · [REL-097](#rel-097) · [REL-098](#rel-098).

**Fronteiras tenant-path divididas por consumidor atômico:** [REL-041](#rel-041) · [REL-051](#rel-051) · [REL-052](#rel-052) · [REL-053](#rel-053) · [REL-054](#rel-054).

**Fronteira DSR de classificação de tabelas:** [REL-055](#rel-055).

**Fronteira de cap Cargo encaminhado:** [REL-056](#rel-056).

<a id="rel-001"></a>
### REL-001 — Listener e router HTTP
**Identidade:** `repo:1232040291:boundary:server-axum-boot-001`.
**Dependência / fluxo / impacto:** server→Axum; request→router; boot→plano HTTP.
**Superfície:** `main`, `axum::serve`, `routes::build_with_factory`.
**Ativação:** binário após boot válido.
**Contrato:** listener usa `PORT`; health fica em `/_health`.
**Estado / efeitos:** aceita tráfego e encerra por sinal.
**Falha / propagação:** boot ou bind falho deixa plano indisponível.
**Contenção:** não prova forwarding edge/DO.
**Validação:** boot/router local; pendente.
**Coordenação / fontes:** server; `src/main.rs` conecta listener, health e shutdown a `src/main_runtime.rs`. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Seleção de storage e adapters
**Identidade:** `repo:1232040291:boundary:server-storage-adapters-001`.
**Dependência/superfície:** env→`StorageEnv`→adapters R2/D1; endpoint/chaves R2, account/token CF e database D1 devem ser não vazios.

**Ativação/contrato:** boot rotula health `r2` se o struct existe; CAS, billing e rotas constroem clients/gates separados. Ausência retorna `None`; `Debug`/`Display` redigem credenciais e o struct não prova client ou mount.

**Efeito/falha:** handler real pode endereçar R2/D1; dev/CI pode selecionar memória. R2 recusado deixa CAS 503, não memória; D1 recusado bloqueia mounts dependentes, mas rate usa default.

**Contenção/validação:** health não prova conectividade, recurso ou egress; testar env, redação, seleção e recusa fake. Operação remota autorizada é separada.

**Coordenação/fontes:** storage/server; `main.rs:151-160,705-726`, `storage.rs:64-145`, `routes/build.rs:623-644`. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Montagem condicional de rotas
**Identidade:** `repo:1232040291:boundary:server-route-mount-001`.
**Fluxo/superfície:** factory cria a base; `main` mescla router se `build_state_from_env` retorna `Some`. Health/heartbeat não usam esses gates.

**Ativação/contrato:** `None` omite rota sem fallback. Mint: duas chaves; ingest: chave+D1; archive: erase+R2/D1; scrub: também TDK; webhook: segredo+D1.

**Efeito/falha:** `Some` altera o router e registra a escolha. Configuração incompleta fecha a superfície, não certifica segredo, D1, R2 ou handler.

**Contenção:** não há gate universal; secret, D1, R2, TDK e fallback variam. Headers edge/deploy ficam fora da prova.

**Validação/coordenação/fontes:** testar montagem/ausência por rota com env isolado; coordenar server/handler/owner do segredo; `main.rs:385-755`, `routes/build.rs`. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Provider BYOK
**Identidade:** `repo:1232040291:boundary:server-byok-provider-001`.
**Dependência/superfície:** feature→`Arc<dyn KmsProvider>`→KMS; `byok-{aws,gcp,azure,vault}-real` encaminham à feature homônima de `corelink-byok`.

**Ativação/contrato:** seleção é compile-time; pares de flags disparam `compile_error!`; sem flag há `ActiveProvider::Unavailable`. `make_provider` não possui fallback criptográfico local e o construtor pode requerer região, URL ou credencial.

**Falha/limite:** flag ausente ou construtor falho retorna `BYOKError::Provider`; boot/caminho que o requisita deve falhar fechado, não persistir como BYOK ativo. Feature, log e fonte não provam credencial, chamada KMS, audit sink ou proteção de dados.

**Validação/coordenação/fontes:** revisar feature única/configuração e testar fake isolado; operação KMS é autorizada; owners BYOK/server; `Cargo.toml:36-72`, `byok_orchestrator.rs`. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Binário de GC
**Identidade:** `repo:1232040291:boundary:server-gc-bin-001`.
**Dependência / fluxo / impacto:** bin→GC adapters→D1/R2; sweep→dados.
**Superfície:** `corelink-gc-sweep-production`, `gc_sweep.rs`.
**Ativação:** execução explícita do binário.
**Contrato:** adaptação nativa sobre crate GC pura.
**Estado / efeitos:** pode alterar dados remotos.
**Falha / propagação:** escopo errado é materialmente destrutivo.
**Contenção:** não executar sem autorização.
**Validação:** wiring e dry-run autorizado.
**Coordenação / fontes:** GC; `Cargo.toml`, `src/bin/gc_sweep.rs`. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Webhook Stripe
**Identidade:** `repo:1232040291:boundary:server-stripe-webhook-001`.
**Dependência / fluxo / impacto:** Stripe→webhook→materializer→D1/audit.
**Superfície:** `STRIPE_WEBHOOK_SECRET`, dispatcher e materializer.
**Ativação:** secret e estado de webhook disponíveis.
**Contrato:** rota não monta sem secret; transport é de crates Stripe.
**Estado / efeitos:** pode afetar cobrança e D1.
**Falha / propagação:** signature, idempotência ou storage falham.
**Contenção:** teste local não prova Stripe real.
**Validação:** fake e ambiente autorizado.
**Coordenação / fontes:** Stripe/billing; `src/main.rs:741-871`. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Saúde e logs
**Identidade:** `repo:1232040291:boundary:server-health-tracing-001`.
**Dependência / fluxo / impacto:** boot→tracing; health→DO/operator.
**Superfície:** tracing subscriber e `/_health`.
**Ativação:** boot e probe HTTP.
**Contrato:** health expõe status/backing sem segredo.
**Estado / efeitos:** observabilidade de processo.
**Falha / propagação:** coleta externa pode faltar.
**Contenção:** não prova exportação remota.
**Validação:** handler local; pendente.
**Coordenação / fontes:** server; `src/main.rs` registra backing e conecta rota/shutdown; `src/main_runtime.rs` implementa health e sinal. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Merge do router CAS
**Identidade:** `repo:1232040291:boundary:server-cas-router-merge-001`.

**Tipo/endpoints:** composição interna; `cas::router(cas_state)` → router do server.

**Superfície/ativação:** chamada explícita em `routes/build.rs:422`, executada ao compor o router base.

**Contrato/efeito:** o módulo CAS contribui suas rotas e estado ao router composto; gates e handlers internos pertencem às superfícies CAS e não são enumerados por esta relação.

**Falha/propagação:** erro de wiring altera ou remove a superfície CAS; merge no source não prova bind, edge, autorização efetiva, storage nem request alcançável.

**Validação/coordenação:** verificar composição e gates CAS em teste isolado; coordenar handler-CAS, storage e server. [Relation index](#b03)

<a id="rel-057"></a>
### REL-057 — Merge do router AC
**Identidade:** `repo:1232040291:boundary:server-ac-router-merge-001`.
**Tipo/endpoints:** composição interna; `ac::router(ac_state)` → router do server.
**Superfície/ativação:** chamada em `routes/build.rs:423`, no router base.
**Contrato/efeito:** ac agrega suas rotas/estado; contratos de handler e gates internos ficam fora desta relação.
**Falha/propagação:** wiring ausente altera AC; fonte não prova bind, edge, auth efetiva ou acesso ao backing.
**Validação/coordenação:** teste isolado da composição e gates; owners handler-AC/server. [Relation index](#b03)

<a id="rel-058"></a>
### REL-058 — Merge do router admin
**Identidade:** `repo:1232040291:boundary:server-admin-router-merge-001`.
**Tipo/endpoints:** composição interna; `admin::router(admin_state)` → router do server.
**Superfície/ativação:** `routes/build.rs:424`, no router base.
**Contrato/efeito:** o módulo admin agrega sua superfície; autorização e cada endpoint não são cobertos por esta relação.
**Falha/propagação:** wiring pode expor, alterar ou remover rotas; merge não prova identidade, edge ou request.
**Validação/coordenação:** testar composição e autorização por handler; owners admin/server. [Relation index](#b03)

<a id="rel-059"></a>
### REL-059 — Merge do router admin tenant detail
**Identidade:** `repo:1232040291:boundary:server-admin-tenant-detail-router-merge-001`.
**Tipo/endpoints:** composição interna; `admin_tenant_detail::router(AdminTenantDetailState::from_env())` → router server.
**Superfície/ativação:** `routes/build.rs:425-426`, incondicional no composition root; configuração do state não equivale a gate do merge.
**Contrato/efeito:** contribui handlers de detalhe de tenant; paths e checks internos não inventariados aqui.
**Falha/propagação:** wiring/state incorreto pode mudar exposição ou resposta; runtime não observado.
**Validação/coordenação:** verificar env, auth e endpoints com owner admin/server. [Relation index](#b03)

<a id="rel-060"></a>
### REL-060 — Merge do router admin pilot
**Identidade:** `repo:1232040291:boundary:server-admin-pilot-router-merge-001`.
**Tipo/endpoints:** composição interna; `admin_pilot::router(pilot_admin_state)` → router server.
**Superfície/ativação:** `routes/build.rs:427`, incondicional no router base.
**Contrato/efeito:** adiciona superfície de operações pilot; autorização e endpoints individuais não foram enumerados.
**Falha/propagação:** erro pode retirar ou alterar controles de pilot; source não comprova operador ou tráfego.
**Validação/coordenação:** revisar cada handler e auth com admin-pilot/server owners. [Relation index](#b03)

<a id="rel-061"></a>
### REL-061 — Merge do router BYOK admin
**Identidade:** `repo:1232040291:boundary:server-byok-admin-router-merge-001`.
**Tipo/endpoints:** composição interna; `byok_admin::router(byok_admin_state)` → router server.
**Superfície/ativação:** `routes/build.rs:428`, incondicional no router base; backend e worker têm gates próprios.
**Contrato/efeito:** agrega handlers administrativos BYOK; contrato de provider/KMS é REL-004/021, não transferido pelo merge.
**Falha/propagação:** wiring/auth/storage incorretos podem alterar configuração de chaves/tenants; nenhuma operação BYOK observada.
**Validação/coordenação:** testar auth, ausência de writer e provider isolado; owners BYOK/server. [Relation index](#b03)

<a id="rel-062"></a>
### REL-062 — Merge do router audit export
**Identidade:** `repo:1232040291:boundary:server-audit-export-router-merge-001`.
**Tipo/endpoints:** composição interna; `audit_export::router(audit_export_state)` → router server.
**Superfície/ativação:** `routes/build.rs:429`, incondicional no router base; state recebe PAT gate antes do merge.
**Contrato/efeito:** adiciona endpoints do exporter; contrato HTTP/verificação é detalhado em REL-046.
**Falha/propagação:** merge não prova que gate/exporter/edge estejam ativos; auth ou wiring errado pode expor logs.
**Validação/coordenação:** testar negativas PAT/tenant e contrato REL-046; owners audit/server. [Relation index](#b03)

<a id="rel-063"></a>
### REL-063 — Merge do router audit analytics
**Identidade:** `repo:1232040291:boundary:server-audit-analytics-router-merge-001`.
**Tipo/endpoints:** composição interna; `audit_analytics::router(audit_analytics_state)` → router server.
**Superfície/ativação:** `routes/build.rs:430`, incondicional; state recebe PAT gate e shadow factory.
**Contrato/efeito:** agrega endpoints de analytics; semântica de `corelink-analytics` está em REL-014.
**Falha/propagação:** merge não prova prelude, sink, auth ou exportação; fallback/tenant incorreto altera analytics.
**Validação/coordenação:** testar gate, tenant e fallback; owners analytics/audit/server. [Relation index](#b03)

<a id="rel-064"></a>
### REL-064 — Merge do router users
**Identidade:** `repo:1232040291:boundary:server-users-router-merge-001`.
**Tipo/endpoints:** composição interna; `users::router(users_state)` → router server.
**Superfície/ativação:** `routes/build.rs:431`, incondicional; state leva o PAT possession gate.
**Contrato/efeito:** adiciona handlers de usuário; paths e validações por handler não inventariados nesta relação.
**Falha/propagação:** wiring/gate incorreto pode alterar identidade exposta; merge não comprova auth externa.
**Validação/coordenação:** testar identidade forjada e ausência de PAT válido; owners users/server. [Relation index](#b03)

<a id="rel-065"></a>
### REL-065 — Merge do router customer
**Identidade:** `repo:1232040291:boundary:server-customer-router-merge-001`.
**Tipo/endpoints:** composição interna; `customer::router(customer_state)` → router server.
**Superfície/ativação:** `routes/build.rs:432`, incondicional; state liga PAT gate, deleção e export se disponíveis.
**Contrato/efeito:** agrega customer control plane; conta/deleção/export são gates próprios, sem pressupor backend ativo.
**Falha/propagação:** wiring/tenant/auth incorretos afetam ações e dados do cliente; runtime não observado.
**Validação/coordenação:** testar own-tenant, gates ausentes e export/deletion isolados; owners customer/DSR/server. [Relation index](#b03)

<a id="rel-066"></a>
### REL-066 — Merge do router DSR portal
**Identidade:** `repo:1232040291:boundary:server-dsr-portal-router-merge-001`.
**Tipo/endpoints:** composição interna; `dsr::portal::router(privacy_dsr_state)` → router server.
**Superfície/ativação:** `routes/build.rs:433`, incondicional; state construído por env e recebe PAT gate.
**Contrato/efeito:** agrega intake do portal; ticket/pipeline DSR permanecem contratos próprios.
**Falha/propagação:** merge não prova D1, worker, pipeline ou apagamento; gate ausente pode fechar operação.
**Validação/coordenação:** testar auth, persistência fake e falha fechada; owners DSR/privacy/server. [Relation index](#b03)

<a id="rel-067"></a>
### REL-067 — Merge do router customer runners
**Identidade:** `repo:1232040291:boundary:server-customer-runners-router-merge-001`.
**Tipo/endpoints:** composição interna; `customer_runners::router(customer_runners_state)` → router server.
**Superfície/ativação:** `routes/build.rs:434`, incondicional; state vem de env e recebe PAT gate.
**Contrato/efeito:** agrega handlers de runner do tenant; endpoints/side effects por handler não inventariados aqui.
**Falha/propagação:** erro de tenant/auth/wiring pode cruzar isolamento ou interromper runners; runtime não observado.
**Validação/coordenação:** testar cross-tenant e backend ausente; owners runner/customer/server. [Relation index](#b03)

<a id="rel-068"></a>
### REL-068 — Merge do router workspaces
**Identidade:** `repo:1232040291:boundary:server-workspaces-router-merge-001`.
**Tipo/endpoints:** composição interna; `workspaces::router(workspaces_state)` → router server.
**Superfície/ativação:** `routes/build.rs:435`, incondicional; state vem de env e recebe PAT gate.
**Contrato/efeito:** agrega handlers de workspace; não descreve path, schema ou runtime sem leitura handler-specific.
**Falha/propagação:** wiring, auth ou isolamento incorretos podem afetar estado de workspace; runtime não observado.
**Validação/coordenação:** testar own-tenant, autorização e falha do store; owners workspaces/server. [Relation index](#b03)

<a id="rel-069"></a>
### REL-069 — Merge do router Bazel v2
**Identidade:** `repo:1232040291:boundary:server-bazel-v2-router-merge-001`.
**Tipo/endpoints:** composição interna; `bazel_v2::router(bazel_state)` → router server.
**Superfície/ativação:** `routes/build.rs:436`, incondicional; state recebe gates e portas antes do merge.
**Contrato/efeito:** agrega superfícies Bazel; relações hash e limite HTTP estão separadas em REL-047–049.
**Falha/propagação:** gate/adapter ou limite errado afeta cache; merge não prova Worker, R2 ou cliente.
**Validação/coordenação:** testar gates, limites e errors REAPI; owners Bazel/handler-CAS/storage/server. [Relation index](#b03)

<a id="rel-070"></a>
### REL-070 — Merge do router Turbo v8
**Identidade:** `repo:1232040291:boundary:server-turbo-v8-router-merge-001`.
**Tipo/endpoints:** composição interna; `turbo_v8::router(turbo_state)` → router server.
**Superfície/ativação:** `routes/build.rs:437`, incondicional; state recebe quota, PAT gate, byte accountant e usage meter.
**Contrato/efeito:** agrega protocolo Turbo; armazenamento/envelope são REL-043/047.
**Falha/propagação:** wiring errado muda auth, quota ou backend; merge não prova request, R2 ou cliente real.
**Validação/coordenação:** testar auth/quota/backend e limites; owners turbo-bridge/storage/PAT/server. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Signup pilot condicionado por segredo
**Identidade:** `repo:1232040291:boundary:server-signup-env-gate-001`.
**Dependência / fluxo / impacto:** `SIGNUP_TOKEN_KEY`→estado→router; request→signup.
**Superfície:** `signup::build_state_from_env`, `signup::router`.
**Ativação:** chave hex válida de pelo menos 32 bytes.
**Contrato:** chave ausente ou inválida deixa `/v1/signup/pilot` em 404, não usa chave dev.
**Estado / efeitos:** HMAC e estado do handler somente quando montado.
**Falha / propagação:** configuração inválida desabilita apenas signup.
**Contenção:** não registra nem revela segredo.
**Validação:** teste negativo sem chave; pendente de execução.
**Coordenação / fontes:** signup/server; `src/routes/build.rs:444-445`. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Ordem de composição das layers do data plane
**Identidade:** `repo:1232040291:boundary:server-data-plane-layer-order-001`.
**Tipo/endpoints:** composição; layers adicionadas ao router do data plane → handler final.
**Superfície/ativação:** `routes/build.rs:629-716`; `.layer(...)` instala timing, OTel opcional, rate, failover e residency em ordem definida.
**Contrato/efeito:** resposta atravessa as layers na ordem inversa de instalação; health/internal são montados depois em `main.rs` e ficam fora desta stack.
**Falha/limite:** trocar ordem muda status, headers, contagem e latência; source não demonstra tráfego ou precedência de deploy.
**Validação/coordenação:** testar ordem e casos individuais das REL-077–081; owners server/telemetry/policy. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Build script e alvos de integração
**Identidade:** `repo:1232040291:boundary:server-build-targets-001`.
**Fluxo/impacto:** proto→build script→lib/bin/test; teste→superfície composta.
**Superfície/ativação:** `build.rs`, library, 2 bins e 13 integration targets (14 arquivos Rust de teste, um helper incluído); build selecionado.
**Contrato/efeito:** target delimita validação; geração e teste podem divergir do boot.
**Falha/limite:** target falho bloqueia artefato; `--target all` não prova deploy.
**Validação:** locked/offline com target explícito; não executada nesta campanha.
**Coordenação/fontes:** package; `Cargo.toml`, `build.rs`, metadata. [Relation index](#b03)

<a id="rel-071"></a>
### REL-071 — Workflow focado para classificação durável do staging
**Identidade:** `repo:1232040291:boundary:server-durable-classification-workflow-001`.
**Tipo/endpoints:** build/test; workflow `issue-1635-durable-classification.yml` → teste `durable_classification_matrix` de `corelink-server`.
**Superfície/ativação:** PR nos caminhos listados ou `workflow_dispatch`; Rust 1.91.1; permissão `contents: read`.
**Contrato/efeito:** SQLite loopback comita INSERT, perde um ACK, exige erro incerto, retry `Deduped` e uma linha vencedora; cobre replay/conflitos.
**Falha/propagação:** regressão falha o job acionado. Não prova D1 remoto, deploy, consumer ou produção.
**Validação/coordenação:** comando no workflow e PROC-006; fontes lidas em `91630ba`, sem execução. Coordenar billing-emit, consumer e CI. [Relation index](#b03)

<a id="rel-072"></a>
### REL-072 — Cache privado Cargo
**Identidade:** `repo:1232040291:boundary:server-cargo-cache-mount-001`.
**Tipo/endpoints:** config/runtime-call; PAT verifier + D1 URL map → Cargo router.
**Ativação/superfície:** `routes/build.rs:467-497`; verifier e D1 devem construir. O resolver de tenant cap, quota e per-tenant CAS bridge são injetados.
**Efeito/falha:** mount ausente fecha `/cargo/*`; D1/env incompleto não usa mapa volátil. Escrita pode alterar CAS/contabilidade.
**Limite/validação:** fonte não prova storage, quota ou request alcançável; testar gates e first-write cap em fake. Owners adapter-host/PAT/storage/server. [Relation index](#b03)

<a id="rel-073"></a>
### REL-073 — Cache público Brew
**Identidade:** `repo:1232040291:boundary:server-brew-cache-mount-001`.
**Tipo/endpoints:** config/runtime-call; PAT verifier + D1 URL map → Brew router.
**Ativação/superfície:** `routes/build.rs:500-518`; usa o mapa D1 compartilhado, verifier e resolvedor de cap público.
**Efeito/falha:** builder separado do Cargo e pode falhar por adapter; mount ausente fecha `/brew/*`. Conteúdo público permite dedup cross-tenant.
**Limite/validação:** não inferir endpoint ativo por mapa/verifier construído; testar builder, cap e falha isolada. Owners adapter-host/storage/server. [Relation index](#b03)

<a id="rel-074"></a>
### REL-074 — Cache npm com metadata KV
**Identidade:** `repo:1232040291:boundary:server-npm-cache-mount-001`.
**Tipo/endpoints:** config/runtime-call; D1 map + metadata KV → npm router.
**Ativação/superfície:** `routes/build.rs:520-544`; exige verifier, mapa D1 e `npm_kv_from_env`.
**Efeito/falha:** ausência de metadata KV omite somente npm; o D1 map é compartilhado. PUTs usam resolvedor cap e metadados podem divergir do CAS se contrato mudar.
**Limite/validação:** mount não prova D1/KV/request remoto; testar cada ausência isoladamente e consistência metadata/CAS com fake. Owners adapter-host/storage/server. [Relation index](#b03)

<a id="rel-075"></a>
### REL-075 — Registry OCI com manifesto e token
**Identidade:** `repo:1232040291:boundary:server-oci-cache-mount-001`.
**Tipo/endpoints:** config/runtime-call; D1 map + manifest KV + token key → OCI router.
**Ativação/superfície:** `routes/build.rs:547-590`; requer verifier, D1, manifest KV e chave canônica ou legado.
**Efeito/falha:** falta qualquer requisito omite `/v2/*`; token carrega cap assinado e suspend resolver protege mint/operações.
**Limite/validação:** nenhuma chave, D1, suspensão ou registry observado; testar gates, token, cap e isolamento com fake. Owners adapter-host/storage/server. [Relation index](#b03)

<a id="rel-076"></a>
### REL-076 — Índice PIP
**Identidade:** `repo:1232040291:boundary:server-pip-cache-mount-001`.
**Tipo/endpoints:** config/runtime-call; D1 map + index KV → PIP router.
**Ativação/superfície:** `routes/build.rs:597-609`; montado depois do bloco do verifier/mapa e recebe verifier, quota e resolvedor cap.
**Efeito/falha:** falta do verifier/mapa fecha todos os adapters; index KV é uma fronteira própria e alteração pode quebrar resolução de pacotes.
**Limite/validação:** código não prova índice, storage nem client PIP remoto; testar ausência e lookup/publish com fake. Owners adapter-host/storage/server. [Relation index](#b03)

<a id="rel-077"></a>
### REL-077 — Guard de residência
**Identidade:** `repo:1232040291:boundary:server-residency-layer-001`.
**Tipo/endpoints:** runtime-call/config; request → `residency_guard` → handler.
**Ativação/superfície:** layer instalada em `routes/build.rs:629`; valida a região confiável contra `R2_CAS_REGION`.
**Efeito/falha:** divergência rejeita com 409 antes de I/O; header/região errados bloqueiam tráfego ou afrouxam residência.
**Limite/validação:** source não prova Worker confiável nem tráfego; testar região válida/inválida e ausência. Owner residency/server. [Relation index](#b03)

<a id="rel-078"></a>
### REL-078 — Guard de failover regional
**Identidade:** `repo:1232040291:boundary:server-failover-layer-001`.
**Tipo/endpoints:** runtime-call; request → failover decision → handler/edge hint.
**Ativação/superfície:** `routes/build.rs:641-646`; state depende de região e health probe.
**Efeito/falha:** estado degradado rejeita escrita e pode marcar leitura para sibling; sem probe/region wiring a política muda.
**Limite/validação:** comentário/código não provam reroute edge ou observação regional; testar política isolada e validar com owner regional. [Relation index](#b03)

<a id="rel-079"></a>
### REL-079 — Rate limit por tenant
**Identidade:** `repo:1232040291:boundary:server-ratelimit-layer-001`.
**Tipo/endpoints:** runtime-call; request → token bucket → resposta/handler.
**Ativação/superfície:** `routes/build.rs:653-686`; resolução D1 é opcional, e a layer envolve só este router.
**Efeito/falha:** limite resulta em 429/Retry-After; resolver ausente usa team default, erro interno é fail-open; health/internal entram depois.
**Limite/validação:** não prova D1 nem quota efetiva; testar ordem, fallback e limites com fake. Owners ratelimit/server. [Relation index](#b03)

<a id="rel-080"></a>
### REL-080 — Exportação OTel configurável
**Identidade:** `repo:1232040291:boundary:server-otel-layer-001`.
**Tipo/endpoints:** telemetry; response → OTel layer → exporter.
**Ativação/superfície:** `routes/build.rs:699-704`; instala apenas se `OtelExportState::from_env` retorna estado.
**Efeito/falha:** observa resposta final e envia pontos/spans; export fail-open preserva resposta, mas pode perder telemetria.
**Limite/validação:** configuração não prova egress, vendor ou entrega; testar disabled/invalid/configured e falha de export isolada. Owners telemetry/server. [Relation index](#b03)

<a id="rel-081"></a>
### REL-081 — Timing do origin para Worker
**Identidade:** `repo:1232040291:boundary:server-origin-timing-layer-001`.
**Tipo/endpoints:** telemetry/external-contract; request → `Server-Timing` → Worker.
**Ativação/superfície:** `routes/build.rs:706-716`; instalada incondicionalmente por último.
**Efeito/falha:** mede o tempo do container e publica componente `origin`; mudança de header/nome altera agregação downstream.
**Limite/validação:** fonte não prova parsing pelo Worker nem métricas observadas; testar header e contrato do Worker no repo consumidor. Owners server/Worker/observability. [Relation index](#b03)

### Dependências first-party declaradas

Cada ficha desta seção é uma relação Cargo atômica: o endpoint provider muda em
cada caso. As contagens são busca de símbolo no recorte `src/` + `tests/` do
server; elas não demonstram chamada, target ou runtime. As relações de dependência
são atômicas por package provider; os fluxos runtime selecionados têm registros próprios.

| Relação | Provider | Relação | Provider |
|---|---|---|---|
| [013](#rel-013) | adapter-host | [014](#rel-014) | analytics |
| [015](#rel-015) | audit | [016](#rel-016) | audit-chain |
| [017](#rel-017) | bazel-bridge | [018](#rel-018) | billing |
| [019](#rel-019) | billing-emit | [020](#rel-020) | billing-stripe-materializer |
| [021](#rel-021) | byok | [022](#rel-022) | cf-bindings |
| [023](#rel-023) | core | [024](#rel-024) | dpa-acceptance |
| [025](#rel-025) | dsr | [026](#rel-026) | erasure-attestation |
| [027](#rel-027) | failover-router | [028](#rel-028) | gc |
| [029](#rel-029) | handler-ac | [030](#rel-030) | handler-admin |
| [031](#rel-031) | handler-cas | [032](#rel-032) | handler-cas-erase |
| [033](#rel-033) | handler-customer | [034](#rel-034) | hash |
| [035](#rel-035) | pat | [036](#rel-036) | privacy-erasure-worker |
| [037](#rel-037) | ratelimit | [038](#rel-038) | slo |
| [039](#rel-039) | stripe-real | [040](#rel-040) | telemetry |
| [041](#rel-041) | tenant-path | [042](#rel-042) | tier-selection |
| [043](#rel-043) | turbo-bridge | — | — | [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — dependência adapter-host
**Tipo/endpoints:** dependency; server → `corelink-adapter-host`.

**Superfície/ativação:** server adapta Cargo, Brew, npm, OCI e pip às portas do adapter-host; cada router recebe resolver de tenant/PAT, CAS/KV/audit e configuração própria.

**Efeito/falha:** mudança de porta ou protocolo propaga para APIs de build/registro e storage. Verificação de integridade e SSRF pertencem ao adapter; o server não deve reimplementar nem relaxar esses gates.

**Composição:** mounts dependem de PAT e D1/R2; OCI ainda requer token/manifest stores. Rotas públicas OCI usam superfície distinta e não transferem autorização de namespace privado.

**Limite/validação:** não houve mount, PAT, D1/R2, upstream, token OCI ou request externo observado. Testar contrato de porta e gates negativos por adapter; coordenar owners adapters/PAT/storage; `routes/{cargo,brew,npm,oci,pip}.rs`, `routes/build.rs`. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — dependência analytics
**Tipo/endpoints:** dependency; server → `corelink-analytics`.

**Superfície/ativação:** `Region` liga o prelude opcional das rotas de audit analytics ao `ShadowSinkFactory`; `RedMetricKind` classifica somente CAS PUT e AC GET na camada OTel.

**Efeito/falha:** prelude ausente ou de outro tenant emite aviso/marcador e usa o resolvedor legado; rota sem variante RED fica apenas com span, não métrica RED.

**Propriedade/limite:** analytics possui taxonomias; server possui roteamento, fallback e emissão. Não foram observados Worker, D1, Neon, exportador OTel ou request real.

**Validação/coordenação:** testar prelude ligado a tenant, fallback e mapeamento RED; coordenar analytics/audit-chain/telemetry; `routes/audit_analytics/shadow_factory.rs`, `routes/otel_layer.rs`. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — dependência audit
**Tipo/endpoints:** dependency; server → `corelink-audit`.

**Superfície/ativação:** builders de Cargo, Brew, npm, OCI e pip injetam `AuditEmitter`; todos constroem `InMemoryAuditEmitter` quando seus adapters são montados.

**Efeito/falha:** o sink conserva eventos apenas na memória do processo. Logo, presença do adapter não demonstra persistência, encadeamento nem entrega de auditoria; a falha de um emitter durável não é exercida por este wiring.

**Propriedade/limite:** audit possui o trait e o formato de evento; server escolhe a implementação e a montagem. Não há emitter durável, evento, storage ou tráfego de adapter observado nesta campanha.

**Validação/coordenação:** confirmar cada builder e testar um emitter que falha antes de trocar wiring; coordenar audit/adapter-host/storage; `routes/{cargo,brew,npm,oci,pip}.rs`. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — dependência audit-chain
**Tipo/endpoints:** dependency; server → `corelink-audit-chain`.

**Superfície/ativação:** chain fornece vínculo e verificação para drain e export; o server também seleciona sinks shadow/factory. Os caminhos HTTP materiais estão em REL-044, REL-045 e REL-046.

**Efeito/falha:** drain sela outbox e avança head sob compare-and-set; export verifica prova/manifesto antes de servir. Alterar formato de link/prova quebra verificadores e evidência histórica.

**Propriedade:** audit-chain possui o contrato criptográfico; server é produtor de outbox, composição de rotas e operador do armazenamento. Reexport ou route não transfere ownership do chain.

**Limite/validação:** não há D1, R2, witness, shadow sink, selo ou export observado. Testar vectores chain e falhas de CAS/export isoladas; coordenar owners audit-chain/audit/storage. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — dependência bazel-bridge
**Tipo/endpoints:** dependency; server → `corelink-bazel-bridge`.

**Superfície/ativação:** `BazelAdapter`, `Digest`, `FindMissingHandler` e `WriteCtx` atendem REAPI REST e cache HTTP sob `/bazel/*`; ambos usam os trait objects CAS/AC compartilhados.

**Efeito/falha:** PUT verifica SHA-256 antes do adapter; mismatch mapeia erro de bridge a 422. `findMissingBlobs` usa o mesmo leitor CAS, portanto erro/gate do storage propaga à resposta Bazel.

**Propriedade/limite:** bridge possui o contrato REAPI, digest e erros; server possui extratores, gates, rotas e composição de handlers. Não houve header Worker, R2/D1, cliente Bazel ou quota realmente observado.

**Validação/coordenação:** testar esquemas REST/cache, digest inválido, limite e `findMissing`; coordenar bazel-bridge/CAS/AC/auth; `routes/bazel_v2/{part-00.rs,part-00-01.rs,part-01.rs}`. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — dependência billing
**Tipo/endpoints:** dependency; server → `corelink-billing`.

**Superfície/ativação:** `stripe::real::{WebhookDispatcher, DispatchResponse, WebhookDlqStore}` dá o contrato do `POST /v1/billing/stripe-webhook`, montado somente com `STRIPE_WEBHOOK_SECRET` e cliente D1.

**Efeito/falha:** o shell entrega corpo e `Stripe-Signature` ao dispatcher e mapeia 200/400/401/422/500. Segredo sem D1 deixa a rota fora, em vez de usar uma fila volátil.

**Propriedade/limite:** billing é a fachada pública; `corelink-stripe-real` implementa o dispatcher e REL-020 implementa materialização D1. Server possui mount e adaptadores, não o protocolo Stripe. Nenhum webhook, segredo, D1 ou Stripe foi observado.

**Validação/coordenação:** testar ausência de assinatura, assinatura/mensagem inválida e montagem negativa; coordenar billing/stripe-real/materializer; `webhook.rs`, `main.rs:720-885`, `corelink-billing/src/stripe.rs`. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — dependência billing-emit
**Tipo/endpoints:** dependency; server → `corelink-billing-emit`.

**Superfície/ativação:** `UsageEventKind`, `validate_billing_period` e `USAGE_EVENT_TYPE` normalizam `POST /internal/v1/billing/usage`; mount requer segredo dedicado e `StorageEnv`. O store usa `(tenant_id, request_id)`.

**Efeito/falha:** inválido não persiste; fingerprint igual retorna `Deduped`; divergente/inverificável conflita. Falha retorna 503 sem body: consumer não settle e retry classifica o winner. Limite: 1024 itens. ACK perdido e linha única são INV-007/REL-071.

**Propriedade/limite:** billing-emit possui taxonomia/período/evento; server possui auth, HTTP, hash do payload e store D1. Não foram observados segredo, request de runner, D1, drain nem agregação de cobrança.

**Validação/coordenação:** chave ausente, inválido, duplicata, conflito e falha D1; o teste de ACK perdido em SQLite loopback não foi executado. Coordenar billing-emit/runners/aggregator/storage; `routes/billing_ingest.rs` e contrato operador. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — dependência billing-stripe-materializer
**Tipo/endpoints:** dependency; server → `corelink-billing-stripe-materializer`.
**Superfície/ativação:** com segredo Stripe e D1, dispatcher recebe writer, audit, idempotência e DLQ duráveis por traits do materializador.
**Efeito/falha:** writer D1-HTTP mapeia falha para transiente; dedup ocorre antes da materialização, portanto DLQ durável retém evento que retry não repetirá.
**Limite:** sem D1/Stripe observado. Em nativo, `cf-billing-real` só testemunha build; binder wasm pertence ao boot do Worker.
**Validação/coordenação:** teste isolado dos adapters, rota e operação autorizada; owner materializer; `main.rs:707-870`, `billing_d1_http.rs`. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — dependência BYOK
**Tipo/endpoints:** dependency; server → `corelink-byok`.
**Superfície/ativação:** uma feature `byok-*-real` seleciona AWS, GCP, Azure ou Vault; combinações são erro de compilação. Rotas administrativas requerem writer D1 e autenticação interna.
**Efeito/falha:** sem provider real ativação retorna `501`; sem writer D1, rotas retornam `503`. Scheduler exige feature real, flag e D1/R2 duráveis.
**Limite:** não houve credencial, KMS, D1, scheduler ou ativação observados; configuração/deploy seguem desconhecidos.
**Validação/coordenação:** matriz feature/config e operação autorizada; owner BYOK; `Cargo.toml:36-72,272-285`, `src/routes/byok_admin.rs`, `src/main.rs:900-967`. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — dependência CF bindings
**Tipo/endpoints:** dependency; server → `corelink-cf-bindings`.

**Superfície/ativação:** dependência opcional, ativada apenas por `cf-r2-real`; nenhum símbolo é importado pelo recorte nativo. A feature seleciona `CfR2BucketReal` para o build wasm32 esperado.

**Efeito/falha:** em alvo nativo, o stub devolve `WasmOnly` se uma operação R2 for alcançada; isso detecta wiring inadequado, mas não constitui binding, bucket nem Worker real.

**Propriedade/limite:** bindings possui adaptadores CF e o contrato target-specific; `corelink-clerk-cf` é o composition/runtime root do Worker. Server apenas declara a feature; não houve wasm, Wrangler, recurso CF ou deploy observado.

**Validação/coordenação:** resolver feature/target e exercitar stub em isolamento; coordenar cf-bindings/clerk-cf/adapters-cloud; `Cargo.toml:74-111`, `storage.rs`. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — dependência core
**Tipo/endpoints:** dependency; server → `corelink-core`.

**Superfície/ativação:** adapters npm/pip/OCI e pull-through usam `TenantId` e `Digest`; OCI recebe `SecretWrap` para a chave de token. A montagem desses adapters continua condicionada pelos gates de REL-013.

**Efeito/falha:** tenant não canônico ou digest inválido falha na conversão antes de atingir a porta; trocar o tipo altera identidade, keyspace e contratos de adapters mesmo sem mudar uma rota.

**Propriedade/limite:** core é dono dos tipos apex, não dos adapters nem do armazenamento. `Digest` deste contrato não prova equivalência com `corelink-hash`; nenhuma credencial, D1/R2, upstream ou request foi observado.

**Validação/coordenação:** testar parsing de tenant/digest e redaction de secret nos adapters; coordenar core/adapter-host/PAT/storage; `adapter_oci_kv.rs`, `routes/{npm,oci,pip}.rs`. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — dependência DPA acceptance
**Tipo/endpoints:** dependency; server → `corelink-dpa-acceptance`.

**Superfície/ativação:** `POST /v1/onboarding/dpa-accept` usa locale/jurisdição, hash de IP e `sign_receipt`; só monta com auth, D1, versão DPA e chave `DPA_RECEIPT_SIGNING_KEY`.

**Efeito/falha:** grava a aceitação idempotente por tenant+versão; versão, locale ou hash de aviso inválidos não criam recibo. O registro é pré-requisito de tier-select, que retorna 403 antes de Stripe quando ele falta.

**Propriedade/limite:** DPA acceptance possui primitivas criptográficas e schema; server possui headers verificados, store D1 e rota. Não foram verificados aviso jurídico canônico, chave, Worker, D1, recibo ou checkout real.

**Validação/coordenação:** testar gates de mount, replay, versão/hash e bloqueio de tier; coordenar DPA/tier-selection/legal/storage; `routes/dpa_accept.rs`, `routes/tier_select/part-01.rs`, `main.rs:683-720`. [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — dependência DSR
**Tipo/endpoints:** dependency; server → `corelink-dsr`.

**Superfície/ativação:** portal privacy usa `DsrRequestKind`, `DsrJurisdiction` e `sla_for` para access, portability, rectification, erasure, restriction e objection.

**Efeito/falha:** tipo/jurisdição direcionam deadline e recibo. Variante futura não suportada retorna 400; restriction/objection ficam pendentes para operador, enquanto braços destrutivos exigem MFA e pipeline separado.

**Limite:** package DSR não prova ticket D1, Worker, execução do pipeline ou apagamento. Cabeçalhos de tenant/jurisdição/MFA dependem de cadeia externa ainda não verificada.

**Validação/coordenação:** testar SLA, tipo desconhecido, MFA, persistência e falha do pipeline em isolamento; owners DSR/privacy/PAT; `routes/dsr/portal/*`. [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — dependência erasure-attestation
**Tipo/endpoints:** dependency; server → `corelink-erasure-attestation`.

**Superfície/ativação:** DSR usa `ErasureSigningKey`, signer, payload e `EvidenceBundle` após verificação completa; a mesma infraestrutura assina a exportação de portabilidade e a rota pública usa `Region`.

**Efeito/falha:** região só é aceita com asserção explícita de região única; evidência incompleta, seed inválida ou falha R2/D1 retém a atestação. Isso não desfaz nem certifica o apagamento subjacente.

**Propriedade/limite:** package possui chave, assinatura e verificação; server reúne evidência, escolhe bucket e ordena R2 → chave pública D1 → índice D1. Não houve seed, R2, D1, sweep, certificado ou verificação pública observados.

**Validação/coordenação:** testar evidência incompleta, região ausente, assinatura adulterada e ordem de persistência com fake; coordenar erasure-attestation/DSR/privacy/storage; `routes/dsr/attestation.rs`, `public_attestation.rs`. [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — dependência failover-router
**Tipo/endpoints:** dependency; server → `corelink-failover-router`.
**Superfície/ativação:** `failover_guard` no build; heartbeat interno autenticado atualiza estado.
**Efeito/falha:** degradado rejeita escrita `POST`/`PUT`/`PATCH`/`DELETE` com `503 failover_readonly` e auditoria; `GET`/`HEAD` passam com hint de região irmã.
**Limite:** heartbeat obsoleto degrada; `/_health` público não o atualiza. Container não reroteia; camadas `nrt`/`syd` são inertes.
**Validação/coordenação:** teste de política/rotas e validação regional autorizada; owner failover; `src/routes/build.rs:641-646`, `src/routes/failover.rs`, `src/main.rs:388-394`. [Relation index](#b03)

<a id="rel-028"></a>
### REL-028 — dependência GC
**Tipo/endpoints:** dependency; server → `corelink-gc`.

**Superfície/ativação:** bin `corelink-gc-sweep-production` compõe a máquina de estados GC com adapters D1/R2 do container; não é o self-check dry-run do package GC.

**Efeito/falha:** configuração ausente falha antes de D1/R2. `GC_VALIDATE_ONLY=true` somente valida; modo live exige `GC_LIVE_DELETE_CONFIRM=I_UNDERSTAND`, e o relatório pode alimentar audit outbox.

**Limite:** declaração de binário e código não provam imagem, schedule, credencial, candidato ou deleção. Nenhuma execução foi feita; validação que alcance D1/R2 é operação autorizada.

**Validação/coordenação:** checar configuração local isolada e revisar report; escalar dry-run/live ao owner GC/storage; `Cargo.toml:15-17`, `src/bin/gc_sweep.rs`, `gc_sweep.rs`. [Relation index](#b03)

<a id="rel-029"></a>
### REL-029 — dependência handler AC
**Tipo/endpoints:** dependency; server → `corelink-handler-ac`.

**Superfície/ativação:** GET/PUT/DELETE `/v1/ac/{tenant}/{action_digest}` e listagem usam quatro traits AC; boot compõe quota, backstop PAT e decorator de byte accounting.

**Efeito/falha:** tenant/escopo/PAT precedem storage; digest não canônico é rejeitado. Corpo divergente não sobrescreve entrada e gera 409; auditoria falha fecha em 503.

**Configuração:** `StorageEnv` ausente seleciona in-memory para dev/CI. Credencial presente e handler R2/TDK inválido seleciona handler indisponível 503, sem fallback durável falso; wasm falha em compile-time.

**Limite/validação:** nenhum R2, D1, Worker ou rota montada foi observado. Testar auth, 409, 503, quota e seleção de env; owners handler-AC/storage/PAT; `routes/ac/*`, `routes/build.rs:199-235`. [Relation index](#b03)

<a id="rel-030"></a>
### REL-030 — dependência handler admin
**Tipo/endpoints:** dependency; server → `corelink-handler-admin`.

**Superfície/ativação:** read, mutate e approve usam `AdminReadHandler`, `AdminMutateHandler` e ledger de aprovações; com `StorageEnv`, handler e ledger usam D1, senão in-memory para dev/CI.

**Efeito/falha:** RBAC e aprovação dupla precedem mutação e geram auditoria/SLI em ordem canônica. `SetTenantTier` espelha D1; `RotateAdminToken` retorna erro interno por fluxo ainda não conectado.

**Limite:** a seleção de D1 não prova rota, aprovação, mutation ou estado remoto. Não executar nem simular operação administrativa fora de fake isolado.

**Validação/coordenação:** testar RBAC, autoaprovação, consumo único, D1 fault e operação suportada; owners admin/D1/approval; `routes/admin/*`. [Relation index](#b03)

<a id="rel-031"></a>
### REL-031 — dependência handler CAS
**Tipo/endpoints:** dependency; server → `corelink-handler-cas`.

**Superfície/ativação:** server constrói os traits CAS e compartilha-os entre CAS nativo, Bazel, OCI e adaptadores. Sem credenciais usa memória; R2 recusado com credenciais monta `UnavailableCasHandler` 503.

**Efeito/falha:** escrita/delete recebem accounting somente com D1. Com tombstones, o wrapper devolve read 404, re-PUT 410 e falha de gate 503; sem store em dev/CI passa sem gate.

**Propriedade/limite:** handler-CAS possui traits/semântica; server possui seleção R2/memória, accounting e gate. R2, D1, rotas, tráfego e consumidores compartilhados não foram observados.

**Validação/coordenação:** testar seleções, accounting, tombstone e superfícies que reutilizam traits; coordenar handler-CAS, CAS-erase, storage e adapters; `routes/cas/single_setup.rs`, `routes/build.rs:103-186`. [Relation index](#b03)

<a id="rel-032"></a>
### REL-032 — dependência handler CAS erase
**Tipo/endpoints:** dependency; server → `corelink-handler-cas-erase`.

**Superfície/ativação:** `POST /_internal/cas/{tenant}/{hash}/erase` valida tenant/digest via `prepare_erase`; montagem exige chave ERASE exclusiva, TDK, D1 tombstone e legitimidade DSR.

**Efeito/falha:** após auth e legitimidade, apaga R2 e só então grava lápide; falha entre ambos deixa 404, nunca serve bytes, e retry idempotente pode concluir. Falha de legitimidade é 403/503 e não apaga.

**Limite:** o apagador cobre apenas chave BLAKE3 nativa; apagamento integral DSR cobre o namespace Bazel. Nenhum binding R2/D1, chamada ou recuperação foi observado.

**Validação/coordenação:** teste isolado de auth, legitimidade, R2 e retry; operação autorizada para R2/D1; owners CAS erase/DSR; `main.rs:597-620`, `cas_erase/*`. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — dependência handler customer
**Tipo/identidade/peer:** dependency server→handler-customer; `repo:1232040291:boundary:customer-server-package-001`; [handler REL-036](../corelink-handler-customer/BLAST_RADIUS.md#rel-036). Fingerprint `sha256:0e27a1885fba6732e784e8021f83955895660da221d30fcb81172d11a8aa13b2`.
SHA-256 de `customer-package-v1|<key>|<handler-manifest-blob>|<server-manifest-blob>` (UTF-8, sem newline); blobs nos pins `cca798ff`/`91630ba`: `4aafb7ca54f8bc586a7a846c28a1ba35d4cf3097`/`e9feace6365776c6a4141ff59efa70fe1b36e158`.

**Fluxo/impacto:** requests server→trait; response/erro handler→server; API handler→D1/HTTP. Owner tipado: handler; integração: server. Métodos REL-088–098 separados; `SOURCE-PAIRED / UNAPPROVED`.
**Ativação/efeito:** target server e router `/v1/customer/*`; `StorageEnv` escolhe D1, senão fake. PAT gate, export e apagamento têm gates próprios; ausência pode dar 503.
**Falha/limite:** mudança DTO quebra D1/HTTP; autenticação Worker, mount, D1 e tráfego não observados. **Validação:** testes isolados de gate, tenant, D1 e 503; `routes/customer/*`, `routes/build.rs:325-390`. [Relation index](#b03)

### Métodos customer: vista local do server

Cada relação REL-088–098 usa a chave e o fingerprint SHA-256 da vista
[handler-customer B03](../corelink-handler-customer/BLAST_RADIUS.md#b03).
Fingerprint = SHA-256 de bytes UTF-8 sem newline de
`customer-method-v1|<key>|<Trait::method>(<Request>)->Result<<Response>,CustomerHandlerError>|<H>|<I>|<R>`.
`H=0eeb3f875e452fe4b96dfc42aa2dedcf6a542c3f` é o blob Git de
`handler-customer/src/handler.rs` no pin `cca798ff`;
`OU=61bab5af3938b1565d99e3f904d388870a7e79cb`,
`BK=1788eda7d562547a4941cbd01f425660a2164fe8`,
`TA=a4391bbab92219484ee3b6774ce83e1d61b67092` são os blobs Git de
`customer_d1_{overview_usage,billing_keys,team_audit}.rs`, respectivamente;
`R0=c7504c45c3f515789ba2e1d732f79084cb3ff8f1` e
`R1=16874290a27a4b6f9a879da5475f28bada5052a4` são os blobs de
`routes/customer/part-00-01.rs` e `part-01.rs`, no pin server `91630ba`.

As posições abaixo selecionam o método dentro dos blobs. Dependência é
server→trait provider; request server→implementação e response/erro
implementação→HTTP server; mudança de assinatura/DTO/erro no handler afeta
D1 e caller HTTP. Ativação exige seleção/build do target server, composição
do `CustomerRouteState` e request; D1 depende de `StorageEnv`, com fake como
fallback source-declared. Efeito local é chamada/tradução HTTP indicada
em cada ficha; alteração pode quebrar compilação ou resposta.

O owner do contrato é handler-customer e o da implementação/HTTP é container/server.
Todas são `SOURCE-PAIRED / UNAPPROVED`; nenhum hash prova build, artefato,
identidade de caller, D1, wire compatibility ou tráfego.

<a id="rel-088"></a>
### REL-088 — Customer overview
`repo:1232040291:boundary:customer-overview-method-001`; `CustomerOverviewHandler::overview(OverviewRequest)->Result<OverviewResponse,CustomerHandlerError>`; OU/R0; `sha256:f7decbc6c4b051f505d10a423105339af6c5051f2dd879cf1eabfbf7e7bf9886`. D1 `customer_d1_overview_usage.rs:2`; caller `part-00-01.rs:90`; erro altera snapshot/HTTP. Peer [REL-001](../corelink-handler-customer/BLAST_RADIUS.md#rel-001). [Relation index](#b03)

<a id="rel-089"></a>
### REL-089 — Customer usage
`repo:1232040291:boundary:customer-usage-method-002`; `CustomerUsageHandler::usage(UsageRequest)->Result<UsageResponse,CustomerHandlerError>`; OU/R0; `sha256:d895b69605d9fcc8557071645988e16df7c6eb222dfbfeebd570983688bd0c46`. D1 `customer_d1_overview_usage.rs:73`; caller `part-00-01.rs:146`; erro altera filtro/HTTP. Peer [REL-002](../corelink-handler-customer/BLAST_RADIUS.md#rel-002). [Relation index](#b03)

<a id="rel-090"></a>
### REL-090 — Customer billing
`repo:1232040291:boundary:customer-billing-method-003`; `CustomerBillingHandler::billing(BillingRequest)->Result<BillingResponse,CustomerHandlerError>`; BK/R0; `sha256:12f741069abd0ac1b3789919b7ca7e3a64b0fb88e4f31938c0871442b85e03e5`. D1 `customer_d1_billing_keys.rs:2`; caller `part-00-01.rs:249`; erro altera billing snapshot/HTTP. Peer [REL-003](../corelink-handler-customer/BLAST_RADIUS.md#rel-003). [Relation index](#b03)

<a id="rel-091"></a>
### REL-091 — Customer portal URL
`repo:1232040291:boundary:customer-portal-url-method-004`; `CustomerBillingHandler::portal_url(PortalRequest)->Result<PortalResponse,CustomerHandlerError>`; BK/R0; `sha256:a758a9144dab55af6f5426302106914419cd588ca770fb9f9e8e5c1470f72a94`. D1 `customer_d1_billing_keys.rs:73`; caller `part-00-01.rs:295`; erro altera portal URL/HTTP. Peer [REL-004](../corelink-handler-customer/BLAST_RADIUS.md#rel-004). [Relation index](#b03)

<a id="rel-092"></a>
### REL-092 — Customer keys list
`repo:1232040291:boundary:customer-keys-list-method-005`; `CustomerKeysHandler::list(KeysListRequest)->Result<KeysListResponse,CustomerHandlerError>`; BK/R1; `sha256:43ba2ddaa099737e8408d1b87608006c2392f95a6f5a985c3e38fac8ffe8576d`. D1 `customer_d1_billing_keys.rs:133`; caller `part-01.rs:30`; erro altera listagem de PAT/HTTP. Peer [REL-005](../corelink-handler-customer/BLAST_RADIUS.md#rel-005). [Relation index](#b03)

<a id="rel-093"></a>
### REL-093 — Customer key create
`repo:1232040291:boundary:customer-keys-create-method-006`; `CustomerKeysHandler::create(KeyCreateRequest)->Result<KeyCreateResponse,CustomerHandlerError>`; BK/R1; `sha256:ea610043f09869601737623d6151c06827b4ce55a8693f8efd933e2d446d2e04`. D1 `customer_d1_billing_keys.rs:177`; caller `part-01.rs:183`; erro altera criação/token/HTTP. Peer [REL-006](../corelink-handler-customer/BLAST_RADIUS.md#rel-006). [Relation index](#b03)

<a id="rel-094"></a>
### REL-094 — Customer key revoke
`repo:1232040291:boundary:customer-keys-revoke-method-007`; `CustomerKeysHandler::revoke(KeyRevokeRequest)->Result<KeyRevokeResponse,CustomerHandlerError>`; BK/R1; `sha256:044e4000273446d0b90c04ce53ab0aea126faf541d666848372119edfbb67aab`. D1 `customer_d1_billing_keys.rs:293`; caller `part-01.rs:232`; erro altera revogação/HTTP. Peer [REL-007](../corelink-handler-customer/BLAST_RADIUS.md#rel-007). [Relation index](#b03)

<a id="rel-095"></a>
### REL-095 — Customer team list
`repo:1232040291:boundary:customer-team-list-method-008`; `CustomerTeamHandler::list(TeamListRequest)->Result<TeamListResponse,CustomerHandlerError>`; TA/R1; `sha256:6b8f22a711886744ce6ff33a4e4f12d7fb3ac0155a3caf6f71e27c98821cefbc`. D1 `customer_d1_team_audit.rs:2`; caller `part-01.rs:284`; erro altera listagem de membros/HTTP. Peer [REL-008](../corelink-handler-customer/BLAST_RADIUS.md#rel-008). [Relation index](#b03)

<a id="rel-096"></a>
### REL-096 — Customer team invite
`repo:1232040291:boundary:customer-team-invite-method-009`; `CustomerTeamHandler::invite(TeamInviteRequest)->Result<TeamInviteResponse,CustomerHandlerError>`; TA/R1; `sha256:40cd1f9665256fbbbe07c19057de8b22b1464ef22721b5296509dd28dfcd44e6`. D1 `customer_d1_team_audit.rs:61`; caller `part-01.rs:351`; erro altera convite/HTTP. Peer [REL-009](../corelink-handler-customer/BLAST_RADIUS.md#rel-009). [Relation index](#b03)

<a id="rel-097"></a>
### REL-097 — Customer team remove
`repo:1232040291:boundary:customer-team-remove-method-010`; `CustomerTeamHandler::remove(TeamRemoveRequest)->Result<TeamRemoveResponse,CustomerHandlerError>`; TA/R1; `sha256:4ae18421dea785cdd476501e209a56e8ec11a806817f0847fe33c76e805d3884`. D1 `customer_d1_team_audit.rs:155`; caller `part-01.rs:401`; erro altera remoção de seat/PATs e HTTP. Peer [REL-010](../corelink-handler-customer/BLAST_RADIUS.md#rel-010). [Relation index](#b03)

<a id="rel-098"></a>
### REL-098 — Customer audit query
`repo:1232040291:boundary:customer-audit-query-method-011`; `CustomerAuditHandler::query(AuditQueryRequest)->Result<AuditQueryResponse,CustomerHandlerError>`; TA/R0; `sha256:b9e0fb685cc6d77cbe10dc1f25ec575334179d3065844dc3bc7d162b7229f23a`. D1 `customer_d1_team_audit.rs:243`; caller `part-00-01.rs:209`; erro altera filtros/rows/HTTP. Peer [REL-011](../corelink-handler-customer/BLAST_RADIUS.md#rel-011). [Relation index](#b03)

<a id="rel-034"></a>
### REL-034 — dependência hash
**Tipo/endpoints:** dependency; server → `corelink-hash`.

**Superfície/ativação:** `Digest::compute` forma hashes de cache e transição BYOK; `DIGEST_LEN` e `CACHE_ENTRY_MAX_BYTES` aparecem em envelope R2, CAS e Bazel. As fronteiras materiais são REL-047 a REL-050.

**Efeito/falha:** mudança de algoritmo, formato ou constante altera bytes persistidos, endereçamento, validação e admissão; o hash OCI é outro contrato (`OciDigest`) e não deve ser confundido com este package.

**Propriedade/limite:** hash possui tipo/algoritmo/limites; server possui seus chamadores e composição. O censo local encontrou oito arquivos, mas não prova features, reachability, R2 ou equivalência com re-exports.

**Validação/coordenação:** resolver seleções reais e testar vetores, envelope, limite e BYOK isolados; coordenar hash, storage, CAS, Bazel e BYOK; `adapter_cache.rs`, `byok_control_transition.rs`, REL-047–050. [Relation index](#b03)

<a id="rel-035"></a>
### REL-035 — dependência PAT
**Tipo/endpoints:** dependency; server → `corelink-pat`.

**Superfície/ativação:** mint, HMAC, Argon2id, `PatScopes` e key set são usados pelo verifier dos adapters, customer keys e `/_internal/pat/mint`; verifier requer `PAT_SIGNING_KEY` e `StorageEnv`.

**Efeito/falha:** HMAC rejeita forjado antes de lookup; segredo validado confronta hash D1 sob limites de Argon. Ausência de configuração não monta adapters. Mint usa chave dedicada e não deve logar plaintext.

**Propriedade/limite:** PAT possui formato/criptografia/scopes; server possui D1, gates, cache e rotas. A posição/encaminhamento edge e qualquer confiança em header injetado devem ser reconciliados por caminho, não inferidos da fonte.

**Validação/coordenação:** testar HMAC inválido, rotação, revogação, escopo write e mount negativo com fake; coordenar PAT/adapter-host/auth/storage; `adapter_pat_verifier.rs`, `routes/internal_pat/*`. [Relation index](#b03)

<a id="rel-036"></a>
### REL-036 — dependência privacy erasure worker
**Tipo/endpoints:** dependency; server → `corelink-privacy-erasure-worker`.

**Superfície/ativação:** `CORELINK_ERASE_AUTH_KEY` dedicado (≥32, atual/anterior) monta DSR. Com `StorageEnv`, worker recebe audit, ledger e legitimidade D1, quatro adapters D1/R2 AC/R2 CAS/Stripe e oito `NotApplicable`.

**Efeito/falha:** sem storage, worker placeholder tem legitimidade vazia e rejeita erase. Account-delete não usa placeholder: sem worker configurado recusa. `main` ainda cita `CORELINK_INTERNAL_AUTH_KEY`, conflito de fonte a reconciliar.

**Propriedade/limite:** privacy worker possui orquestração, backend kinds, decisões e ledger traits; server possui auth, adapters e bridges D1/R2. Não houve D1/R2/Stripe, rota ou deleção observados.

**Validação/coordenação:** testar chave, legitimidade, adapter faltante, `NotApplicable` e recusa account-delete; coordenar privacy/DSR/storage/Stripe e reconciliar `main`; `routes/dsr.rs`, `routes/dsr/adapter_*.rs`. [Relation index](#b03)

<a id="rel-037"></a>
### REL-037 — dependência ratelimit
**Tipo/endpoints:** dependency; server → `corelink-ratelimit`.

**Superfície/ativação:** data plane usa token bucket e `BucketKey`; a primeira request pode resolver tier em D1 e aplicar a ladder. Export, analytics, attestation e pull-through possuem gates locais distintos.

**Efeito/falha:** `Deny429` responde com `Retry-After`; futuro braço de negação também fecha em 429. Falha interna do limiter segue aberta e logada; buckets são em processo e resetam no restart.

**Propriedade/limite:** ratelimit possui bucket, taxonomia e ladder; server possui ordem de middleware, resolvedor D1 e sinks. `/_health` e `/_internal/*` ficam fora da camada; não houve D1, tráfego ou 429 real observado.

**Validação/coordenação:** testar tier default/resolvido, 429, erro do limiter, restart e ordem/exclusões; coordenar ratelimit, billing e telemetry; `routes/ratelimit_layer.rs`, `routes/build.rs`. [Relation index](#b03)

<a id="rel-038"></a>
### REL-038 — dependência SLO
**Tipo/endpoints:** dependency; server → `corelink-slo`.

**Superfície/ativação:** `CountingSliObserver` agrega CAS/AC e audit-attempted em janelas e usa `BurnRateCalculator`; a decisão é escrita em `tracing` periodicamente.

**Efeito/falha:** lock envenenado descarta a observação em vez de derrubar o data plane. O aviso de quota próximo ao teto é default-off e, se ligado, usa só `InMemoryPagerDutyDispatcher` fire-and-forget.

**Propriedade/limite:** SLO possui SLIs, janelas e decisão; server possui contadores, cadência e sink. Não há coletor, PagerDuty HTTPS, routing key, métrica remota ou alerta entregue observado.

**Validação/coordenação:** testar janelas, erro/latência, lock e flag de quota com fake; coordenar SLO/telemetry/quota; `sli_aggregate.rs`, `tenant_quota/b126_m2_impl_01_part_02.rs`. [Relation index](#b03)

<a id="rel-039"></a>
### REL-039 — dependência Stripe real
**Tipo/endpoints:** dependency; server → `corelink-stripe-real`.
**Superfície/ativação:** `POST /v1/billing/stripe-webhook` monta só com `STRIPE_WEBHOOK_SECRET` e cliente D1.
**Efeito/falha:** shell HTTP encaminha corpo bruto e `stripe-signature` ao `WebhookDispatcher` canônico; materialização/idempotência pertencem ao caminho downstream.
**Limite:** segredo ou D1 ausente não monta rota. Não houve requisição Stripe, operação D1 nem cutover de produção observado.
**Validação/coordenação:** revisão de rota/dispatcher, fake isolado e operação autorizada; owner Stripe; `src/webhook.rs`, `src/main.rs:734-755`. [Relation index](#b03)

<a id="rel-040"></a>
### REL-040 — dependência telemetry
**Tipo/endpoints:** dependency; server → `corelink-telemetry`.
**Superfície/ativação:** camada OTel somente se `OtelExportState::from_env` retorna exportador configurado.
**Efeito/falha:** após a resposta registra status final e latência como `MetricPoint`/`TraceSpan`; falha de exportação é fail-open para a resposta.
**Limite:** config ausente/inválida não monta camada. Exportadores atuais validam e aceitam batch como stubs deferred-real; não há egress/exportação runtime evidenciado.
**Validação/coordenação:** teste de camada/config e observação autorizada; owner telemetry; `src/routes/build.rs:647-660`, `src/otel_layer.rs`. [Relation index](#b03)

<a id="rel-041"></a>
### REL-041 — dependência tenant-path
**Identidade compartilhada:** `repo:1232040291:boundary:tenant-path-server-r2-kv-prefix-001`; peer: `corelink-tenant-path` BLAST `REL-001`.
**Tipo/endpoints:** dependency; server → `corelink-tenant-path`.

**Fluxo de dados:** `TenantDerivationKey` + tenant UUID → `derive_prefix` → string prefix → R2/KV key. **Impacto:** algoritmo ou largura diferente pode endereçar objetos existentes de forma incompatível; revert simples não migra chaves.

**Superfície/ativação:** `storage/r2_kv.rs:{129,462,467,527,908}`; rota/worker selecionado precisa chamar a superfície.

**Efeito/falha:** TDK secreto e UUID canônico produzem prefixo não previsível; ausência de TDK ou tenant não derivável falha fechado no caminho produtivo, evitando cair em prefixo público.

**Limite/validação:** nenhum bucket, objeto, tenant ou operação BYOK foi observado. Testar vetor de prefixo, UUID inválido e leitura N/N-1; owners tenant-path/storage/BYOK; `storage/r2_kv.rs:115-130`. [Relation index](#b03)

<a id="rel-042"></a>
### REL-042 — dependência tier-selection
**Tipo/endpoints:** dependency; server → `corelink-tier-selection`.

**Superfície/ativação:** `TierKind` alimenta o selector de preços no boot e o materializador de assinatura; a rota onboarding é composição do servidor sobre auth, D1, Stripe e DPA.

**Efeito/falha:** preço configurado mapeia para tier; preço vazio preserva placeholder de compatibilidade, que não corresponde a evento Stripe real. Estado `pending_checkout` não deve conceder tier pago no resolver D1.

**Propriedade:** package tier-selection é dono do tipo/regra; servidor é composição e runtime operator; D1/Stripe/DPA continuam owners de seus contratos e aprovações.

**Limite/validação:** sem secret, D1, Stripe, DPA ou webhook observado. Testar mapa preço→tier, placeholder, ativo versus pendente e gate de mount; `main_boot.rs:62-131,181-210`, `main.rs:665-679,812-825`. [Relation index](#b03)

<a id="rel-043"></a>
### REL-043 — dependência turbo-bridge
**Tipo/endpoints:** dependency; server → `corelink-turbo-bridge`.

**Superfície/ativação:** `TurboArtifactHandler` e ports CAS atendem `/v8/artifacts/*`, `/events` e `/status`; hash é opaco e tamanho de hash/team é validado pela bridge.

**Efeito/falha:** com storage válido, `R2KvStore` implementa as ports; sem storage usa memória. Credencial presente com builder R2 falho seleciona handler 503, sem fallback volátil. Tenant e `teamId` são distintos.

**Propriedade/limite:** bridge possui protocolo, limites e erros; server possui HTTP, gates, locks e backend. A integridade do envelope R2 é a fronteira compartilhada REL-047, não uma garantia da hash opaca Turbo. Não houve R2, PAT, Turbo client ou request observado.

**Validação/coordenação:** testar hash/team inválido, isolamento tenant, seleção de backend e 503; coordenar turbo-bridge/storage/PAT; `routes/turbo_v8/*`, `storage/r2_kv.rs`. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho | Condição | Efeito | Validação |
|---|---|---|---|---|
| edge/DO | REL-001 | forwarding ativo | request chega router | não observado |
| R2/D1 | REL-002/003 | env e rota válidos | dados podem mudar | autorizado |
| Stripe | REL-003/006 | webhook montado | billing/materializer | autorizado |
| KMS | REL-004 | feature/provider | operações BYOK | autorizado |
| dados GC | REL-005 | binário executado | sweep remoto | owner/dry-run |
| tenant-path split | REL-041/051/052/053/054 | selected route/job | R2/KV/CAS/DSR/BYOK/GC key effects | no runtime observed |

| Destino | Caminho | Condição | Efeito | Validação |
|---|---|---|---|---|
| Cargo cache privado | REL-072 | verifier + D1 map construídos | `/cargo/*` monta com cap por tenant | gate e first-write cap em fake |
| Brew cache público | REL-073 | verifier + D1 map construídos | dedup público e resolver de cap | builder, cap e falha isolada |
| npm metadata | REL-074 | verifier + D1 map + metadata KV | `/npm/*` monta isoladamente | ausência por dependência e consistência CAS/KV |
| OCI registry | REL-075 | verifier + D1 + manifest KV + token key | token, cap assinado e suspend gate | matriz de gates e token em fake |
| PIP index | REL-076 | verifier + D1 map + index KV | índice e artefatos do registry | ausência/lookup/publish em fake |
| residency guard | REL-077 | router do data plane | recusa antes de I/O | região válida/inválida |
| regional failover | REL-078 | state/probe regionais | bloqueio de escrita/hint de leitura | política isolada; sem inferir reroute |
| rate limiter | REL-079 | router do data plane | 429 ou team-default quando resolver falta | ordem, fallback e limite |
| OTel exporter | REL-080 | export configurado | export fail-open da resposta | disabled/config/falha fake |
| origin timing | REL-081 | layer externa | header `Server-Timing` consumível pelo Worker | header e contrato consumidor |

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | Relações | Efeito | Validação | Recuperação |
|---|---|---|---|---|
| boot/env | 001/002/007 | disponibilidade/backing | boot/health local | restaurar config |
| mount/auth | 003/006 | exposição de rota | negativo de env/auth | desabilitar mount |
| adapter | 002 | durabilidade/dado | fake + integração autorizada | owner storage |
| feature BYOK | 004 | provider/target | matriz compile | reverter seleção |
| GC | 005 | dado remoto | dry-run autorizado | roll-forward |
| tenant-path algorithm/prefix | 041/051/052/053/054 | key namespace and `_public` boundary | vectors, N/N-1, downstream owners | migration/roll-forward |

| Mudança | Relações | Efeito | Validação | Recuperação |
|---|---|---|---|---|
| router de plano | 008, 057–070/009, 072–076 | contrato HTTP e exposição por root/mount | teste de composição e cada gate condicional | retirar merge/mount |
| residency guard | 077 | status 409 e política regional | região válida/inválida | restaurar guard/config |
| failover guard | 078 | status de escrita e hint de leitura | decisão isolada com probe fake | restaurar política/config |
| rate limit | 079 | 429, tier/fallback, exclusões | caso limite e ordem | restaurar camada/resolver |
| OTel | 080 | pontos/spans, resposta fail-open | exporter fake, config ausente/inválida | desabilitar exporter |
| origin timing | 081 | header externo consumido pelo Worker | parser/contrato do Worker | manter compatibilidade de header |
| build/proto | 012 | bin/library/test | target explícito | corrigir geração | [Relation index](#b03)
| cap de storage Cargo | 056 | PUT reutiliza cap Worker e evita lookup D1 por escrita | testes de escopo e bypass do resolver | preservar fail-closed; reverter apenas código | [Relation index](#b03) |

<a id="rel-044"></a>
### REL-044 — drain da cadeia de auditoria
**Tipo/endpoints:** runtime-call; rota interna → audit chain/D1.
**Superfície/ativação:** POST /_internal/audit/drain; chave dedicada CORELINK_ERASE_AUTH_KEY (≥32) e StorageEnv válidos montam a rota.
**Efeito/falha:** sela outbox e avança head por CAS; ausência de chave/D1 não monta a rota, sem fallback compartilhado.
**Limite/validação:** nenhum D1, mount ou selo foi observado; testar isolamento e operação autorizada; audit_drain/b126_m2_impl_01.rs:513-643, main.rs:471-483. [Relation index](#b03)

<a id="rel-045"></a>
### REL-045 — archive separado da cadeia
**Tipo/endpoints:** runtime-call/data; rota interna → audit chain/D1/R2.
**Superfície/ativação:** POST /_internal/audit/archive exige chave dedicada, D1 e R2.
**Efeito/falha:** arquiva prefixo verificado em R2 antes de marcar archived_at; indisponibilidade R2 não altera o selo D1 já separado.
**Limite/validação:** nenhum bucket, D1, rota ou objeto foi observado; revisar prefixo/falha isoladamente e escalar operação; audit_archive.rs:218-316, main.rs:513-527. [Relation index](#b03)

<a id="rel-046"></a>
### REL-046 — export verificável de auditoria
**Tipo/endpoints:** runtime-call/external-contract; cliente → audit chain.
**Superfície/ativação:** GET /v1/audit/export; tenant e escopo de leitura precedem o exporter, e gate de posse PAT é adicional quando configurado.
**Efeito/falha:** janela/prova inválida falha antes de servir dados; bearer forjado ou falha do verifier é rejeitado.
**Limite/validação:** não houve export, PAT ou dado de cliente observado; testar fake/negações e operação autorizada; audit_export/handler.rs:95-145, state.rs:157-160, routes/build.rs:318-387. [Relation index](#b03)

<a id="rel-047"></a>
### REL-047 — envelope de integridade Turbo em R2
**Identidade:** `repo:1232040291:boundary:hash-container-envelope-001`.

**Tipo/endpoints:** data; storage R2KvStore → corelink-hash.

**Superfície/ativação:** escrita e leitura formam `CLTBINT1 + DIGEST_LEN + payload`; `Digest::as_bytes` é usado no envelope.

**Efeito/falha:** payload verificado retorna hit; envelope truncado ou divergente vira miss. Legacy passa sem verificação, portanto não é prova de integridade retroativa.

**Limite/validação:** largura ou algoritmo muda bytes persistidos e leitura N/N-1; R2 não foi observado. Testar legacy, verificado e corrompido; owners storage/hash; `storage/r2_kv.rs:172-215`. [Relation index](#b03)

<a id="rel-048"></a>
### REL-048 — limite de corpo nas rotas Bazel
**Identidade:** `repo:1232040291:boundary:hash-server-bazel-limit-001`.

**Tipo/endpoints:** config/runtime; router Bazel → corelink-hash.

**Superfície/ativação:** quatro rotas AC/CAS usam `DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)` antes do handler.

**Efeito/falha:** mudar a constante altera admissão e memória do extrator; não verifica digest nem certifica montagem do router.

**Limite/validação:** edge, transporte e tráfego não foram observados. Testar abaixo/no/acima do limite em cada rota; owners Bazel/hash; `routes/bazel_v2/part-00.rs:313-351`. [Relation index](#b03)

<a id="rel-049"></a>
### REL-049 — orçamento de leitura CAS
**Identidade:** `repo:1232040291:boundary:hash-server-cas-read-limit-001`.

**Tipo/endpoints:** config; política de leitura CAS → corelink-hash.

**Superfície/ativação:** `CAS_READ_MAX_OBJECT_BYTES` é alias `u64` de `CACHE_ENTRY_MAX_BYTES` no caminho CAS.

**Efeito/falha:** a constante direciona seleção/admissão de leitura, distinta da verificação de digest. Mudança altera orçamento; não demonstra aplicação em todos os readers.

**Limite/validação:** não houve leitura R2 ou capacidade runtime observada. Testar a política e montagem CAS; owners handler-CAS/hash; `routes/cas/foundation_core.rs:178-187`. [Relation index](#b03)

<a id="rel-050"></a>
### REL-050 — limite de corpo no endpoint CAS
**Identidade:** `repo:1232040291:boundary:hash-server-cas-delete-limit-001`.

**Tipo/endpoints:** config/runtime; router CAS → corelink-hash.

**Superfície/ativação:** a rota única CAS registra GET, PUT e DELETE e aplica `DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)` ao conjunto.

**Efeito/falha:** alterar o máximo muda aceitação e memória para requests com corpo; isso não autentica, verifica integridade ou prova que a rota está montada.

**Limite/validação:** não é uma das quatro rotas Bazel e não houve request observado. Testar abaixo/no/acima e montagem; owners handler-CAS/hash; `routes/cas/single_setup.rs:155-163`. [Relation index](#b03)

<a id="rel-051"></a>
### REL-051 — dependência tenant-path multipart/CAS
**Identidade compartilhada:** `repo:1232040291:boundary:tenant-path-server-multipart-prefix-001`; peer: `corelink-tenant-path` BLAST `REL-002`.
**Tipo/endpoints:** dependency/data; server → tenant-path → multipart/CAS keys.
**Superfície/ativação:** `storage/r2_s3_parts/{ac_handler.rs:20,cas_builder.rs:205,cas_helpers.rs:{59,72,130}}`; CAS erase/scrub paths. `_public` is a special namespace and must not degrade to a tenant prefix.
**Efeito/falha:** prefix or fallback drift can cause collision, isolation, or old-key incompatibility. No bucket/object/runtime operation observed; validate public, tenant, missing-TDK, and N/N-1 paths with storage owners. [Relation index](#b03)

<a id="rel-052"></a>
### REL-052 — dependência tenant-path DSR
**Identidade compartilhada:** `repo:1232040291:boundary:tenant-path-server-dsr-prefix-001`; peer: `corelink-tenant-path` BLAST `REL-003`.
**Tipo/endpoints:** dependency/data; server DSR → tenant-path → AC/CAS/legal-hold erase keys.
**Superfície/ativação:** `routes/dsr/adapter_r2_ac.rs:{216,237}`; `adapter_r2_cas.rs:{186,211}`; `adapter_r2_cas_legalhold.rs:{196,229}`.
**Efeito/falha:** derivation drift can make erase miss or address another namespace. No erase, bucket, object, or tenant runtime observed; validate vectors and authorized N/N-1 recovery. [Relation index](#b03)

<a id="rel-053"></a>
### REL-053 — dependência tenant-path BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:tenant-path-server-byok-prefix-001`; peer: `corelink-tenant-path` BLAST `REL-004`.
**Tipo/endpoints:** dependency/data; server BYOK workers → tenant-path → activation/backfill/purge key paths.
**Superfície/ativação:** `storage/byok_activation_worker.rs:{297,1834,1910,2330}`; `byok_backfill_d1.rs:151`; `byok_purge_io.rs:301`.
**Efeito/falha:** prefix drift can break convergent address compatibility. KMS, D1, provider, and runtime activation were not observed; coordinate BYOK and storage owners. [Relation index](#b03)

<a id="rel-054"></a>
### REL-054 — dependência tenant-path GC
**Identidade compartilhada:** `repo:1232040291:boundary:tenant-path-server-gc-prefix-001`; peer: `corelink-tenant-path` BLAST `REL-005`.
**Tipo/endpoints:** dependency/data; GC sweep → tenant-path → object sweep key.
**Superfície/ativação:** `src/gc_sweep/part-04.rs:7`; only a selected GC binary/job can activate it.
**Efeito/falha:** derivation drift can leave objects undiscovered or target another namespace. No binary selection, sweep, or remote deletion observed; validate dry-run and recovery with GC/storage owners. [Relation index](#b03)

<a id="rel-055"></a>
### REL-055 — registro DSR de tabelas runner
**Identidade:** `repo:1232040291:boundary:server-dsr-runner-classification-001`.
**Tipo/endpoints:** server DSR → registry D1 → migrations 0133–0140.
**Superfície/ativação:** `TENANT_ID_TABLES`, `RETAIN_SET`, `ALL_TENANT_KEYED_TABLES`, `classification::ensure_classification`; ativado pela validação de completude na rota DSR.

**Contrato/efeito:** 0134 `usage_event_staging_conflicts`, 0136 `storage_mutation_liability`, 0137 `runner_checkout_attempts` e 0139 `runner_entitlement_reconcile_fence` são apagadas por tenant. 0135 `runner_period_terms_snapshot` e `runner_aggregate_event_claim` são retidas.

0140 adiciona `authority_is_current` à fence 0139.
0133 `stripe_webhook_event_effects` e 0138 `dsr_dlq_delivery_receipts` não têm `tenant_id`; ficam fora de `ALL_TENANT_KEYED_TABLES`.
**Falha/propagação:** tabela ausente ou em buckets múltiplos deve falhar fechado; erro pode bloquear erasure ou deixar under-erasure, sem apagar a retenção fiscal.
**Boundary/validação:** fontes `routes/dsr/adapter_d1.rs` e `routes/dsr/adapter_d1/classification.rs` no pin `91630ba`, leitura estática. Testes lidos, não executados; D1/runtime não observados.
**Coordenação:** server, DSR, privacy, billing e owners de migrations. [Relation index](#b03)

<a id="rel-056"></a>
### REL-056 — Cap de storage Worker encaminhado ao Cargo PUT
**Identidade:** `repo:1232040291:boundary:server-cargo-storage-cap-forward-001`.
**Tipo/endpoints:** config/data/call; Worker header → parser → `cargo_gate` → task-local → `CargoMoatStore::put`.
**Superfície:** `STORAGE_QUOTA_HEADER`, `CARGO_STORAGE_QUOTA_CAP`, `routes/cargo/*`; PUT após stripping do cliente.
**Contrato:** não-negativo → `Some(cap)`; ausente/inválido → `None`; escopo evita lookup D1; chamada direta usa resolver.
**Efeito/falha:** reduz lookup; header falsificável quebra orçamento; ausência é fail-closed.
**Validação:** dois testes de propagação lidos no pin `91630ba`, não executados; confira `src/byte_accounting/b126_m2_impl_01.rs:22-39` e `src/routes/cargo/tests-00-00.rs:377`.
**Coordenação:** server, Worker/edge, byte-accounting, storage e quota; validar trusted-header sem D1. [Relation index](#b03)

<a id="rel-082"></a>
### REL-082 — Hash de conteúdo da cache de adapter
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-adapter-cache-001`; peer [corelink-hash REL-037](../corelink-hash/BLAST_RADIUS.md#rel-037).

**Endpoints/superfície:** `adapter_cache::canonical_hash_hex(bytes)` → `corelink_hash::Digest::compute(bytes).to_hex()`; o resultado é usado na chave de conteúdo da MoatCache production adapter e alinhado ao digest BLAKE3 usado pelo CAS.

**Impacto/falha:** mudança de algoritmo ou codificação muda identidade de chave e pode produzir miss/divergência; não altera por si só framing do objeto ou prova uma chamada em execução.
**Limite/validação:** os quatro arquivos server usados para REL-082–087 são idênticos entre `b9b3ee8` e `91630ba`; runtime/cache remota UNKNOWN. Comparar chave e CAS com fixtures; owner nominal ou operador não verificado. [Relation index](#b03)

<a id="rel-083"></a>
### REL-083 — Hash de bytes TCS encapsulados BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-byok-tcs-001`; peer [corelink-hash REL-038](../corelink-hash/BLAST_RADIUS.md#rel-038).

**Endpoints/superfície:** `ActivationHashes::new` chama `Digest::compute(&activation.tcs_wrapped).to_hex()` e o fluxo da intent grava o valor em `wrapped_tcs_blake3`.

**Impacto/falha:** algoritmo/codificação diferente muda fingerprint persistido e comparação posterior; não descriptografa nem valida a chave.
**Limite/validação:** runtime BYOK/D1 UNKNOWN. Testar fixture N/N-1; owner nominal e operador não verificados. [Relation index](#b03)

<a id="rel-084"></a>
### REL-084 — Fingerprint da política de ativação BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-byok-policy-001`; peer [corelink-hash REL-039](../corelink-hash/BLAST_RADIUS.md#rel-039).

**Endpoints/superfície:** `ActivationHashes::new` codifica domínio `corelink.byok.activation-policy.v1` e campos de modo/crypto_mode/provider/key/region, depois calcula BLAKE3 hex; server define os campos e framing, hash executa digest.

**Impacto/falha:** mudança de domínio, ordem, bytes ou campos muda fingerprint comparado/persistido.
**Limite/validação:** runtime/D1 UNKNOWN. Revisar golden bytes e leitura N/N-1; owner nominal não identificado. [Relation index](#b03)

<a id="rel-085"></a>
### REL-085 — Fingerprint de requisição BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-byok-request-001`; peer [corelink-hash REL-040](../corelink-hash/BLAST_RADIUS.md#rel-040).

**Endpoints/superfície:** `ActivationHashes::new` codifica domínio `corelink.byok.activation-request.v1` e campos tenant/policy/wrapped-TCS, depois calcula BLAKE3 hex; o request hash participa de lookup e insert da intent.

**Impacto/falha:** mudar campo, ordem, codificação ou separador altera igualdade/idempotência.
**Limite/validação:** runtime/D1 UNKNOWN. Testar mesma requisição e mudança unitária de campo; owner nominal não identificado. [Relation index](#b03)

<a id="rel-086"></a>
### REL-086 — Fingerprint canônico de evento de billing
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-billing-payload-001`; peer [corelink-hash REL-041](../corelink-hash/BLAST_RADIUS.md#rel-041).

**Endpoints/superfície:** `BillingIngest::payload_hash` forma a imagem UTF-8 com oito campos do usage record separados por US (`\x1f`) e aplica `Digest::compute(...).to_hex()`; o fingerprint participa da detecção de conflito por tenant/request.

**Impacto/falha:** mudar algoritmo ou serialização muda classificação de payload igual/divergente; não altera o valor monetário em si.
**Limite/validação:** runtime/D1 UNKNOWN. Testar determinismo, alteração de campo e conflito; owner nominal não identificado. [Relation index](#b03)

<a id="rel-087"></a>
### REL-087 — Verificação BLAKE3 de conteúdo CAS multipart
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-cas-verify-001`; peer [corelink-hash REL-042](../corelink-hash/BLAST_RADIUS.md#rel-042).

**Endpoints/superfície:** `verify_content_hash(DigestAlgo::Blake3, claimed_hash, bytes)` chama `Digest::{compute,from_hex,verify_constant_time}`; erro de parse ou mismatch retorna antes de servir/persistir conforme o caller.

**Impacto/falha:** parser/hash alterado muda admissão de CAS BLAKE3; SHA-256 é caminho separado e `Ok(())` não afirma escrita nova.
**Limite/validação:** helper source privado não prova request/R2. Runtime/R2 UNKNOWN. Validar claims malformed/match/mismatch; owner nominal não identificado. [Relation index](#b03)

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos | Desconhecidos |
|---|---:|---:|---:|---:|
| targets/features | 2 bins; 7 features do manifesto | declarações mapeadas em REL-004/012 | 0 | seleção para artefato/target concreto |
| deps first-party | 31 entradas de manifesto | 31 relações Cargo atômicas por provider (REL-013–043) | 0 | feature/target resolvido e runtime reachability |
| métodos customer | 11 implementações D1 e 11 call sites HTTP | 11 relações source-only REL-088–098, peers handler-customer REL-001–011 | 0 | aprovação bilateral, build, artefato e runtime |
| roots incondicionais | `routes/build.rs:421-437` | 15 merges, 15 relações (REL-008, REL-057–070) | 0 | paths HTTP internos e efeitos particulares continuam nos owners de cada handler |
| routers condicionais | `:444-445`, `:487-612` | 6 merges; signup (REL-009) e cinco adapters (REL-072–076) | 0 | secrets, stores e mounts em runtime |
| layers do data plane | `:629-716` | 5 layers; ordem (REL-011) e efeito de cada layer (REL-077–081) | 0 | tráfego, exporter e edge observados |
| relações semânticas selecionadas | REL-044–056, REL-071–081 | 24 relações por contrato/efeito/ativação | 0 | relações que dependam de deploy, configuração externa ou caminho não selecionado |
| source Rust | 401 em `crates/corelink-container/src/**/*.rs`; 14 em `tests/` | contagem no pin reproduzível; census de fronteiras conferido com builder e manifesto | 0 | contratos internos de cada handler permanecem no documento do package owner respectivo |
| runtime remoto | 0 observado | 0 | 0 | R2/D1/Stripe/KMS/CF, edge, tráfego e deploy |

### Inventário de dependência first-party

O censo abaixo é uma busca estática pelo nome Rust em `src/` e `tests/` do package. Ele identifica superfície de investigação, não chamada, target selecionado, feature ou reachability. `0` significa que o manifesto declara a dependência mas a busca não achou esse símbolo neste recorte; pode ser cfg, macro, feature ou declaração obsoleta.

| Package | Arquivos com símbolo | Package | Arquivos com símbolo |
|---|---:|---|---:|
| adapter-host | 22 | analytics | 14 |
| audit | 5 | audit-chain | 19 |
| bazel-bridge | 5 | billing | 4 |
| billing-emit | 1 | billing-stripe-materializer | 6 |
| byok | 13 | cf-bindings | 0 |
| core | 6 | dpa-acceptance | 1 |
| dsr | 1 | erasure-attestation | 4 |
| failover-router | 3 | gc | 3 |
| handler-ac | 20 | handler-admin | 2 |
| handler-cas | 50 | handler-cas-erase | 2 |
| handler-customer | 3 | hash | 8 |
| pat | 23 | privacy-erasure-worker | 13 |
| ratelimit | 20 | slo | 3 |
| stripe-real | 11 | telemetry | 1 |
| tenant-path | 12 | tier-selection | 7 |
| turbo-bridge | 8 | — | — |

**Bounded census source-backed (pin `91630ba`):** o boundary set inclui 31 dependências first-party,
15 merges incondicionais, seis mounts condicionais e cinco layers. Cada dependência e merge tem ficha;
signup, cada cache adapter e cada layer têm relações distintas. Também inclui os fluxos selecionados
de storage, segurança, auditoria, billing, hash, tenant-path, DSR e workflow (REL-044–056, REL-071–081).

Fontes: [readback imutável](../../evidence/revision-1.4/SERVER-MAIN-READBACK-20260923-91630.md),
`crates/corelink-container/src/routes/build.rs:421-716`, `Cargo.toml` e registros B03. Esse boundary set
completa a composição Cargo/server visível no pin, mas não individualiza cada path HTTP dos 401 arquivos
Rust; contrato, validação e efeito interno ficam com o package owner do handler.

Desconhecidos: feature/target selecionado, tráfego, secrets/config externos, edge/deploy e estado de
recursos remotos. Relações introduzidas fora do pin também não estão cobertas. Source, merge ou contagem
não certificam resolução de build nem reachability runtime.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
