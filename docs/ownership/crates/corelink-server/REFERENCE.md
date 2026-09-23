---
schema: corelink-ownership/1.1
document: reference
package: corelink-server
manifest: crates/corelink-container/Cargo.toml
source_commit: 91630baebe3ae7abe686cd4e06a5621ecdc4ab73
profile: H
state: draft
evidence_set: server-main-readback-20260923-91630
---

# corelink-server — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

`corelink-server` é a raiz nativa de composição: inicia HTTP, monta o router e seleciona adapters.
O package em `crates/corelink-container` possui o binário `corelink-server`, o binário
`corelink-gc-sweep-production` e a library `corelink_server`. O diretório `src/` contém
401 arquivos Rust; `tests/` tem 13 targets de integração e 14 arquivos Rust, incluindo
um helper compartilhado, portanto usa perfil H.

Essa contagem vem do objeto Git `origin/main@91630ba`; o checkout da campanha pode estar
em outra linhagem e não é usado como prova de identidade do source pin.

A inspeção estática dos manifestos não encontrou
um package consumidor direto; o grafo Cargo resolvido não foi executado. Build, target entregue e
runtime de produção permanecem dimensões distintas.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-server` / `crates/corelink-container/Cargo.toml` |
| Targets | library, dois bins, 13 targets de teste, build script; 14 arquivos Rust de teste incluindo helper |
| Papel | composição root e adapter nativo |
| Features | default; quatro BYOK reais; `cf-r2-real`; `cf-billing-real` |
| Runtime | não observado; seleção por env e feature existe no código |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Implementação | Contrato | Operação |
|---|---|---|---|
| boot e listener | server | server | container/operador |
| rotas Axum | server + handlers | server e handler owner | edge/container |
| R2/D1 nativos | adapters server | adapter/storage owner | Cloudflare autorizado |
| Stripe/BYOK | crates externos compostos | crate origem | operador autorizado |

**Não faz:** não prova deploy, segredo, bucket, D1, KMS ou cobrança apenas pelo código.
**OKF:** [container](../../../knowledge/planes/container.md), [request flow](../../../knowledge/planes/request-flow.md) e [credential handling](../../../knowledge/security/credential-handling.md).

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo | Papel | Natureza | Evidência |
|---|---|---|---|
| `main.rs` | boot, env, listener, mounts condicionais e wiring runtime | composição | API-001–003 |
| `main_runtime.rs` | estado do backing, health e shutdown gracioso | runtime local | API-001/003 |
| `main_boot.rs` | configuração de boot/tier | composição | API-002 |
| `main_byok.rs`, `byok_*` | feature-gated KMS e workers | adapter/gate | API-004 |
| `routes.rs`, `routes/build.rs` | declarações e montagem do router | composição | API-003 |
| CAS, AC, admin, admin tenant detail, admin pilot, BYOK admin, audit export, audit analytics, users, customer, DSR portal, customer runners, workspaces, Bazel v2, Turbo v8 | 15 chamadas `.merge(<root>::router(...))` em `routes/build.rs:378-395` | fronteiras de composição separadas em REL-008 e REL-057–070; handler interno não inferido |
| signup/cargo/brew/npm/OCI/pip | planos montados por secret, PAT e D1 | adapter/gate | REL-003/009/010 |
| residency/failover/ratelimit/otel/origin timing | camadas transversais do data plane | política/observabilidade | REL-011 |
| `storage.rs`, `storage/**` | R2 S3, D1 HTTP e fakes | adapter | API-005 |
| `routes/dsr/adapter_d1/classification.rs` | buckets e gate fail-closed do registry DSR | classificação DSR | API-006 |
| `routes/billing_ingest/tests_durable_classification.rs` | replay, conflito e perda de ACK após commit | teste loopback D1/SQLite | INV-007 / REL-071 |
| `bin/gc_sweep.rs`, `gc_sweep.rs` | executor nativo GC | bin/adapter | REL-005 |

No composition root `main.rs`, o router de `routes::build_with_factory_and_byok` recebe
heartbeat interno, `/_health` e limite global de body de 10 MiB (Turbo pode definir limite
local maior). `routes/build.rs` compõe 15 routers e gates de signup e dos adapters Cargo,
Brew, npm, OCI e pip. Residency, failover, rate limit, OTel opcional e origin timing
envolvem o data plane.

`main.rs` monta PAT mint, introspection, billing ingest, DSR, audit drain, CAS-attempt audit,
audit archive, CAS scrub, tenant quota, public attestation, CAS erase, public revocation,
public mirror, tier selection, DPA acceptance, Stripe webhook e DSR anchor por builders de
ambiente. Estado ausente mantém cada mount fechado. Isto descreve call sites; nenhum mount
ou adapter foi observado em runtime.

**Censo limitado de implementação:** `git ls-tree -r` no pin `91630ba` conta 401 arquivos `.rs` em
`crates/corelink-container/src/`; `tests/` contém 14 arquivos `.rs` para 13 targets de integração
(um helper compartilhado). Em `routes/build.rs:378-395`, foram conferidos os 15 pontos de merge
incondicionais, registrados separadamente em REL-008 e REL-057–070.

`routes/build.rs:397-412` tem o merge condicional de signup; `:415-580` contém os cinco adapters
de cache Cargo, Brew, npm, OCI e pip, montados sob gates. `:585-675` instala residency, failover,
rate limit e OTel opcional; origin timing é declarado no mapa, mas não pertence a esse intervalo
de instalação.

Esse censo prova pontos de composição e contagens no source pin, não enumera endpoints de cada
router, todas as funções/handlers nos 401 arquivos, seus call paths nem alcance em artefato/runtime.
O censo handler-a-handler e a reconciliação de consumidores semânticos seguem desconhecidos no
blast-radius.

**Readback de source pin:** [evidência](../../evidence/revision-1.4/SERVER-MAIN-READBACK-20260923-91630.md). O pin anterior `47f4db7` precede a extração de helpers runtime e classificação DSR; o teste focado e o contrato de recuperação estão presentes no pin atual. A execução local e o workflow hospedado não foram verificados.

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007).

<a id="api-001"></a>
### API-001 — Binário HTTP e saúde
**Símbolos exatos:** `main`, `runtime::health_handler`, `runtime::shutdown_signal`, `runtime::record_storage_backing`, `runtime::STORAGE_BACKING`.
**Entrada / pré-condição:** `PORT`, env e configuração válida.
**Saída / efeitos:** listener HTTP; `/_health` retorna status e backing selecionado.
**Erros / compatibilidade:** falha de orçamento impede bind; sinal encerra graciosamente.
**Vínculos e prova:** INV-001; `main.rs` liga backing, rota health e shutdown a `main_runtime.rs`.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Seleção de storage e gates de boot
**Símbolos exatos:** `StorageEnv::from_env`, `validate_runtime_budget`, `should_fatal_on_missing_gate`.
**Entrada / pré-condição:** env de R2/D1 e sinal independente de produção.
**Saída / efeitos:** escolhe R2 ou fake; produção incompleta deve recusar boot.
**Erros / compatibilidade:** fallback local não prova backing durável.
**Vínculos e prova:** INV-001/002; REL-002/003.

[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Composição de rotas
**Símbolos exatos:** `routes::build_with_factory`, `DefaultBodyLimit`, `runtime::health_handler`.
**Entrada / pré-condição:** factory e estados por env.
**Saída / efeitos:** router composto; mounts sensíveis ficam ausentes sem configuração.
**Erros / compatibilidade:** limite global é 10 MiB, com exceção local explícita para Turbo.
**Vínculos e prova:** INV-003; REL-003/008/009/010/011.

[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Seleção BYOK
**Símbolos exatos:** `active_provider`, `make_provider`, `start_byok_background_tasks`.
**Entrada / pré-condição:** exatamente uma feature `byok-*-real` quando provider real é requerido.
**Saída / efeitos:** provider ativo ou indisponível fail-closed.
**Erros / compatibilidade:** múltiplas features são erro de compilação; sem fallback crypto em memória.
**Vínculos e prova:** INV-004; REL-005.

[Índice de contratos](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Adapters nativos de storage
**Símbolos exatos:** `StorageEnv`, `D1HttpClient`, `R2CasHandler`, `R2KvStore`.
**Entrada / pré-condição:** endpoint, credenciais e ids válidos.
**Saída / efeitos:** chamadas R2/D1 através de adapters; fakes em dev/teste.
**Erros / compatibilidade:** ausência de env pode selecionar fake ou desarmar mount conforme rota.
**Vínculos e prova:** INV-002; REL-002/003.

[Índice de contratos](#r04)
[↩](#r01)

<a id="api-006"></a>
### API-006 — Classificação DSR de tabelas tenant-keyed
**Símbolos exatos:** `TENANT_ID_TABLES`, `RETAIN_SET`, `ALL_TENANT_KEYED_TABLES`, `ensure_tenant_keyed_tables_classified`, `classification::ensure_classification`, `classification::classification_count`.
**Entrada / pré-condição:** tabelas tenant-keyed das migrations D1, com `tenant_id` e classe de retenção conhecida.
**Saída / efeitos:** cada tabela recebe exatamente uma disposição; conflitos e checkout são apagados por tenant, termos/claims imutáveis são retidos.
**Erros / compatibilidade:** tabela ausente ou em dois buckets falha fechado; nova migration exige atualização das três registries e dos testes.
**Vínculos e prova:** INV-005; REL-055; `routes/dsr/adapter_d1.rs`, `routes/dsr/adapter_d1/classification.rs` e migrations conferidas no pin `91630ba`.

[Índice de contratos](#r04)
[↩](#r01)

<a id="api-007"></a>
[↩](#r04)
### API-007 — Propagação do cap de storage no Cargo
**Símbolos:** `cargo_gate`, `CARGO_STORAGE_QUOTA_CAP`, `CargoMoatStore::put`, `storage_quota_from_headers`, `STORAGE_QUOTA_HEADER`.
**Pré-condição:** PUT Cargo com header de quota autenticado pelo Worker; cópia do cliente removida antes do encaminhamento.
**Efeito:** o gate escopa o cap; `put` o reutiliza e evita lookup D1 por PUT. Chamadas diretas usam resolver opcional.
**Erros/compatibilidade:** ausente ou inválido vira `None` fail-closed; header não confiável ou ausência ilimitada viola o contrato.
**Prova:** INV-006/REL-056; dois testes de propagação lidos no pin `47f4db7`, não executados.

[Índice de contratos](#r04)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Owner | Vida | Escrita/leitura |
|---|---|---|---|
| `runtime::STORAGE_BACKING` | runtime | processo | set por `main` antes do bind, lido por health |
| router/state | server | processo | construído no boot |
| objetos/metadados | R2/D1 | externo | adapters; não observado |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006) · [INV-007](#inv-007) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Orçamento inválido impede listener
**Regra:** capacidade deve ser validada antes de construir rotas ou bind.
**Imposição:** `validate_runtime_budget` no início de `main`.
**Violação / prova:** ambiente incompatível inicia listener; fonte confirma ordem, teste pendente.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Configuração de produção incompleta não fica silenciosa
**Regra:** sinal independente de produção com gates ausentes deve falhar fechado.
**Imposição:** bloco de gates em `main.rs` após seleção de storage.
**Violação / prova:** fallback em memória aceito como produção; testes de env pendentes.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Mount interno exige estado válido
**Regra:** rota interna não monta sem suas chaves e storage requeridos.
**Imposição:** `build_state_from_env` e `if let Some` no boot.
**Violação / prova:** rota interna acessível sem gate; testes de mount pendentes.

[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — BYOK real tem seleção única e fail-closed
**Regra:** zero provider real é indisponível; mais de um é inválido.
**Imposição:** features e orquestrador BYOK.
**Violação / prova:** provider ambíguo ou fake crypto; seleção de features pendente.

[Índice de estado](#r05)

<a id="inv-005"></a>
### INV-005 — Tabela tenant-keyed tem exatamente uma disposição DSR
**Regra:** toda tabela tenant-keyed migrada deve estar em `TENANT_ID_TABLES`, `RETAIN_SET` ou bucket especial, nunca em mais de um.
**Imposição:** `ensure_tenant_keyed_tables_classified`, `ALL_TENANT_KEYED_TABLES` e `classification::classification_count`.
**Violação / prova:** falta ou duplicidade de classificação de tabela tenant-keyed viola o gate. As tabelas 0134 `usage_event_staging_conflicts`, 0136 `storage_mutation_liability`, 0137 `runner_checkout_attempts` e 0139 `runner_entitlement_reconcile_fence` são apagadas por tenant; as duas tabelas 0135 são retidas como evidência de billing. A 0140 adiciona `authority_is_current` à fence 0139.

0133 `stripe_webhook_event_effects` e 0138 `dsr_dlq_delivery_receipts` não têm `tenant_id` e ficam fora do registro tenant-keyed. Os testes nomeados de 0134/0135/0137/0139 foram lidos no pin `47f4db7`, não executados.

[Índice de estado](#r05)

<a id="inv-006"></a>
### INV-006 — Cap de storage encaminhado permanece autenticado e fail-closed
**Regra:** somente o cap parseado de `STORAGE_QUOTA_HEADER` pelo gate Cargo pode ser reutilizado no PUT; ausência ou valor inválido permanece `None`, nunca ilimitado.
**Imposição:** `storage_quota_from_headers` → `CARGO_STORAGE_QUOTA_CAP.scope` em `cargo_gate` → `CargoMoatStore::put`; chamadas diretas usam resolver explícito.
**Violação / prova:** aceitar header fornecido pelo cliente, consultar D1 repetidamente apesar de cap encaminhado ou converter ausência em ilimitado viola o contrato. Os dois testes de propagação estão no pin `47f4db7`, lidos e não executados.

[Índice de estado](#r05)

<a id="inv-007"></a>
### INV-007 — Retry após ACK perdido preserva o winner durável
**Regra:** para o mesmo `(tenant_id, request_id)` e fingerprint idêntico, se a persistência comita mas a resposta se perde, o retry classifica `Deduped` e mantém exatamente uma linha vencedora.
**Imposição:** `STAGE_INSERT_SQL` usa `ON CONFLICT ... DO NOTHING`; `D1UsageStagingStore::stage` lê e compara o fingerprint do winner durável.
**Violação / prova:** duplicar a linha, retornar nova inserção no retry ou não classificar como `Deduped` viola a regra. `durable_classification_matrix` contém esse caso com SQLite loopback em `tests_durable_classification.rs`; leitura SOURCE, não executada nesta campanha.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Boot HTTP nativo

Inicializa tracing; valida capacidade; classifica storage; aplica gates; monta router e rotas
condicionais; adiciona health/limite; inicia listener; drena em SIGTERM/SIGINT. R2/D1/Stripe reais
não foram ativados neste levantamento.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Fonte | Default | Condição | Efeito |
|---|---|---|---|
| `PORT` | 50051 | boot | endereço HTTP |
| `R2_S3_*`, D1 vars | ausentes em dev | `StorageEnv` | real ou fake |
| `byok-*-real` | desligadas | compilação | provider real selecionado |
| `cf-r2-real`, `cf-billing-real` | desligadas | target/feature | witness de binder CF |
| secrets de rota | ausentes | boot | mount ou fail-closed |

**Matriz:** nativo default foi inventariado; wasm e cada combinação real permanecem pendentes.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Causa | Estado após falha | Ação |
|---|---|---|---|
| erro de capacidade | envelope inválido | sem bind | PROC-002 |
| storage `inmemory` | env ausente | health expõe backing | PROC-003 |
| rota não montada | secret/D1 ausente | fail-closed | PROC-004 |
| provider indisponível | feature ausente | BYOK não serve | PROC-005 |

Logs de boot não devem expor valores de segredo; runtime e exportação de telemetry não foram observados.

<a id="r08"></a>
## R08 — Verificação e evidências

| Item | Método | Resultado / limite |
|---|---|---|
| identidade/targets | manifesto e fontes fixadas | dois bins, 13 targets de teste e build script; 14 arquivos Rust de teste incluindo helper; 401 arquivos Rust em `src/` |
| features | manifesto | sete features enumeradas |
| boot/rotas | leitura de `main.rs`, `main_runtime.rs` e `routes.rs` | implementado; não executado |
| consumer inverso | inspeção estática de manifestos/workspace | nenhum package consumidor direto visível; grafo Cargo não executado |
| registry DSR / migrations | leitura SOURCE no pin `91630ba` | classificação tenant-keyed conferida com migrations 0134–0140; 0133 e 0138 lidas e excluídas por não terem `tenant_id`; testes não executados | runtime D1, sweep e attestation não observados |
| staging billing | teste, workflow e contrato operador fixados no pin `91630ba` | caminho de ACK pós-commit perdido, retry `Deduped` e um winner; workflow seleciona teste dedicado; contrato orienta retry de 503 sem body | nenhum teste/workflow foi executado nesta campanha; runners externos não observados |

**Desconhecidos:** grafo Cargo resolvido, censo handler-a-handler e de consumidores, validação Rust,
mounts reais, targets wasm/release e runtime.
**Continuar:** [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
