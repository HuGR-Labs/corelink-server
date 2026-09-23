---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-cf-bindings
manifest: crates/corelink-cf-bindings/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: cf-bindings-pilot-source-20260920
---

# corelink-cf-bindings — blast radius

[Escopo](#b01) · [Método](#b02) · [Diretas](#b03) · [Propagação](#b04) · [Mudança](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

Este documento cobre a fronteira entre os adaptadores `worker::*`, os traits canônicos e seus consumidores conhecidos. Não prova que um binding Cloudflare existe, está configurado, foi entregue ou recebeu tráfego.

<a id="b02"></a>
## B02 — Método e populações

| Camada | Método/evidência | Limite |
|---|---|---|
| Inventário | `Cargo.toml`, targets, features e cfg; REL-CF-016–031 | declaração não é resolução |
| Resolvida | `cargo tree --locked --offline -i corelink-cf-bindings --edges normal` | seleção local, não deploy; resultado não reproduzido nesta revisão |
| Semântica | imports/reexports e fontes de consumidores | não substitui caminho executado |
| Configuração | `wrangler.toml` e `crates/corelink-clerk-cf/wrangler.toml`, fontes de wiring | declaração não é deploy |
| Externa | Workers e contratos externos | reconciliação ainda aberta |

<a id="b03"></a>
## B03 — Relações diretas

<a id="rel-cf-001"></a>
### REL-CF-001 — trait de R2
**Tipo/endpoints:** implementação de contrato; `CfR2BucketReal` → `corelink_cas::r2_storage::R2Backend`, reexportado por `corelink-cas`; o contrato canônico é mantido em `corelink-worker`.
**Superfície/ativação:** compilação da implementação de `put_if_none_match`, `get` e `head`; o alvo wasm seleciona a implementação Worker, o host seleciona o stub.
**Efeito/falha:** mudança incompatível no trait ou resultado quebra a implementação; no host, operações terminam em `WasmOnly:` após a validação aplicável.
**Validação/coordenação:** revisar contrato `corelink-worker`, reexport `corelink-cas` e `src/r2_real.rs`; fake/stub local não certifica R2 real.

<a id="rel-cf-002"></a>
### REL-CF-002 — trait de KV
**Tipo/endpoints:** dependência; este crate → `corelink-worker`.
**Superfície/ativação:** `KvBackend` por `CfKvNamespaceReal` em wasm.
**Efeito/falha:** prefixo inválido, audit negado ou target host impedem acesso a KV.
**Validação/coordenação:** preservar fake e trait owner; evidência `kv_real.rs`, `Cargo.toml`.

<a id="rel-cf-003"></a>
### REL-CF-003 — API do Worker
**Tipo/endpoints:** dependência/FFI; este crate → `worker`/wasm-bindgen/Cloudflare runtime.
**Superfície/ativação:** R2, D1, KV, Object Namespace, somente `target_arch=wasm32`.
**Efeito/falha:** incompatibilidade de API/target impede construir ou operar o adapter; host não é fallback operacional.
**Validação/coordenação:** build wasm selecionado e owner de Worker; evidência `Cargo.toml`, `lib.rs`.

<a id="rel-cf-004"></a>
### REL-CF-004 — reexport canônico
**Tipo/endpoints:** reexport; `corelink-adapters-cloud::cf` → esta API pública.
**Superfície/ativação:** `pub use corelink_cf_bindings::*`; qualquer consumidor desse caminho herda compatibilidade de símbolos.
**Efeito/falha:** quebra de tipo/erro pode alcançar consumidores sem import direto do crate.
**Validação/coordenação:** pesquisar ambos caminhos e coordenar com owner do umbrella; evidência `crates/corelink-adapters-cloud/src/cf.rs`.

<a id="rel-cf-005"></a>
### REL-CF-005 — consumidor Clerk
**Tipo/endpoints:** consumidor reverso; `corelink-clerk-cf` → este crate.
**Superfície/ativação:** wiring wasm para health, audit, tenant region e Durable Object.
**Efeito/falha:** mudança de API/escopo pode impedir o Worker consumidor ou alterar negação de acesso.
**Validação/coordenação:** owner `corelink-clerk-cf`, target/features selecionados; evidência árvore inversa e imports desse crate.

<a id="rel-cf-006"></a>
### REL-CF-006 — consumidor de status DSR
**Tipo/endpoints:** consumidor reverso; `corelink-dsr-statuspage-scheduler` → este crate.
**Superfície/ativação:** row source wasm/D1 do scheduler.
**Efeito/falha:** mudança de D1/error pode bloquear leitura ou propagar `WasmOnly` se executado no target errado.
**Validação/coordenação:** owner de privacy/DSR; evidência árvore inversa e `wasm32_row_source.rs`.

<a id="rel-cf-007"></a>
### REL-CF-007 — auditoria de R2
**Tipo/endpoints:** chamada de runtime; `CfR2BucketReal` → `R2AuditFn` injetado.
**Superfície/ativação:** `head`, `list_keys`, `put_if_absent`, `delete` e cada etapa multipart invocam o callback; `get_bytes` não. `new` instala no-op até `with_audit` substituir.
**Efeito/falha:** erro de audit interrompe a operação antes do acesso R2; no-op não prova comportamento externo.
**Validação/coordenação:** `prod_wiring.rs:180-193` conecta o hook R2 (SOURCE); owner e comportamento externo permanecem não observados.

<a id="rel-cf-034"></a>
### REL-CF-034 — auditoria de D1
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-d1-audit-hook-v1`.
**Tipo/endpoints:** chamada de runtime; `CfD1DatabaseReal` → `D1AuditFn` injetado.
**Superfície/ativação:** `prepare`, `bind`, `first`, `all` e `run` chamam audit; `run` chama antes da mutação e emite segundo evento quando `changes > 0`. `new` instala no-op até `with_audit`.
**Efeito/falha:** erro pré-run impede dispatch; erro pós-run pode ocorrer após mutação D1. O hook conectado não prova execução externa.
**Validação/coordenação:** `prod_wiring.rs:180-193` conecta o hook D1 (SOURCE); owner e comportamento externo permanecem não observados.

<a id="rel-cf-035"></a>
### REL-CF-035 — auditoria de KV
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-kv-audit-hook-v1`.
**Tipo/endpoints:** chamada de runtime; `CfKvNamespaceReal` → `KvAuditFn` injetado.
**Superfície/ativação:** `get_bytes`, `put_bytes`, `delete` e `list_keys` chamam audit antes do acesso. `new` instala no-op até `with_audit`.
**Efeito/falha:** erro de audit bloqueia o acesso KV; no-op não prova comportamento externo.
**Validação/coordenação:** `prod_wiring.rs:180-193` conecta o hook KV (SOURCE); owner e comportamento externo permanecem não observados.

<a id="rel-cf-036"></a>
### REL-CF-036 — auditoria de Durable Object
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-do-audit-hook-v1`.
**Tipo/endpoints:** chamada de runtime; `CfDurableObjectReal` → `DoAuditFn` injetado.
**Superfície/ativação:** cada fetch valida o nome escopado e chama audit antes de resolver/enviar ao stub; `new` instala no-op até `with_audit`.
**Efeito/falha:** erro de audit bloqueia o fetch; no-op/fake não provam chamada real.
**Validação/coordenação:** `prod_wiring.rs:180-193` conecta o hook DO (SOURCE); owner e comportamento externo permanecem não observados.

<a id="rel-cf-008"></a>
### REL-CF-008 — escopo de chave R2
**Tipo/endpoints:** dado/segurança; caller → `TenantPrefix`/`TenantScopedKey` de R2.
**Superfície/ativação:** `scoped_key` valida e deriva prefixo `<tenant>/` para operações de chave; list usa raiz `<tenant>/`.
**Efeito/falha:** chave vazia/malformada ou prefixo de outro tenant retorna erro antes de R2; wrapper não deriva a identidade do tenant.
**Validação/coordenação:** revisar `src/r2_real.rs`; derivação/autoridade de identidade upstream não reconciliada.

<a id="rel-cf-037"></a>
### REL-CF-037 — escopo de query D1
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-d1-tenant-query-v1`.
**Tipo/endpoints:** dado/segurança; caller → `TenantId`/`TenantScopedQuery` de D1.
**Superfície/ativação:** `scoped_query` exige cláusula de tenant; `bind` verifica o primeiro parâmetro contra o tenant configurado em tempo constante.
**Efeito/falha:** SQL sem escopo ou bind divergente retorna erro antes de executar no D1; este crate não autentica nem deriva a identidade.
**Validação/coordenação:** revisar `src/d1_real.rs`; owner upstream da identidade permanece desconhecido.

<a id="rel-cf-038"></a>
### REL-CF-038 — escopo de chave KV
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-kv-tenant-key-v1`.
**Tipo/endpoints:** dado/segurança; caller → `TenantPrefix`/`TenantScopedKey` KV.
**Superfície/ativação:** `scoped_key` deriva o separador `:` e compara prefixo já qualificado em tempo constante.
**Efeito/falha:** prefixo/key inválido retorna erro antes de acesso KV; contrato de chave é distinto do separador `/` do R2.
**Validação/coordenação:** revisar `src/kv_real.rs`; owner upstream da identidade permanece desconhecido.

<a id="rel-cf-039"></a>
### REL-CF-039 — nome de tenant DO
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-do-tenant-name-v1`.
**Tipo/endpoints:** dado/segurança; caller → `DoTenantPrefix`/`TenantScopedName`.
**Superfície/ativação:** valida nome `tenant:<id>:<purpose>` antes de obter stub; a comparação de prefixo usa igualdade em tempo constante.
**Efeito/falha:** nome fora do escopo retorna erro antes do fetch; não autoriza derivação/autenticação de tenant.
**Validação/coordenação:** revisar `src/do_real.rs`; autoridade upstream de identidade permanece desconhecida.

<a id="rel-cf-009"></a>
### REL-CF-009 — configuração de alvo
**Tipo/endpoints:** build; workspace `.cargo/config.toml` → `getrandom` wasm e crate.
**Superfície/ativação:** `getrandom_backend="wasm_js"` mais dependência `getrandom_v04` condicionada.
**Efeito/falha:** faltar uma metade quebra a resolução wasm de consumidores; build host pode ocultar a falha.
**Validação/coordenação:** build wasm explícito, não `--target all`; evidência `Cargo.toml`.

<a id="rel-cf-010"></a>
### REL-CF-010 — testes/fakes
**Tipo/endpoints:** teste; integration tests → traits/fakes de `corelink-worker`/`corelink-cas`.
**Superfície/ativação:** `d1_real`, `do_real`, `kv_real`, `prop_cas_idempotency` e stub.
**Efeito/falha:** teste host exercita wrapper/fake selecionado; não chama binding real.
**Validação/coordenação:** registrar comando/target e distinguir o que o teste não cobre; evidência `Cargo.toml`, `tests/`.

**Chaves de identidade estáveis das relações acima (âncora local = ID no heading):**

| IDs locais | Identidade/fingerprint estável | Peer/owner do contrato |
|---|---|---|
| 001 | `repo:1232040291:boundary:cf-bindings-r2-trait-v1` | contrato em `corelink-worker`, caminho canônico `corelink-cas`; peer pending |
| 002 | `repo:1232040291:boundary:cf-bindings-kv-trait-v1` | `corelink-worker`; peer pending |
| 003 | `repo:1232040291:boundary:cf-bindings-worker-api-v1` | `system:Cloudflare Workers`; external boundary `worker::*` |
| 004 | `repo:1232040291:boundary:cf-bindings-reexport-v1` | `corelink-adapters-cloud`; peer pending |
| 005 | `repo:1232040291:boundary:cf-bindings-clerk-consumer-v1` | `corelink-clerk-cf` |
| 006 | `repo:1232040291:boundary:cf-bindings-dsr-consumer-v1` | `corelink-dsr-statuspage-scheduler` |
| 007 | `repo:1232040291:boundary:cf-bindings-r2-audit-hook-v1` | composition root R2; route UNKNOWN |
| 008 | `repo:1232040291:boundary:cf-bindings-r2-tenant-key-v1` | upstream identity/tenant owner UNKNOWN |
| 009 | `repo:1232040291:boundary:cf-bindings-getrandom-wasm-v1` | workspace build config |
| 010 | `repo:1232040291:boundary:cf-bindings-native-fakes-v1` | test owners UNKNOWN |
| 011 | `repo:1232040291:boundary:cf-bindings-clerk-wiring-v1` | `corelink-clerk-cf`; route UNKNOWN |
| 012 | `repo:1232040291:boundary:cf-bindings-clerk-wrangler-v1` | Clerk Wrangler owner; route UNKNOWN |
| 013 | `repo:1232040291:boundary:cf-bindings-billing-v1` | `corelink-billing-stripe-materializer` |
| 014 | `repo:1232040291:boundary:cf-bindings-dsr-cron-v1` | DSR/privacy owner; route UNKNOWN |
| 015 | `repo:1232040291:boundary:cf-bindings-worker-logs-v1` | Worker/telemetry owner; route UNKNOWN |
| 034 | `repo:1232040291:boundary:cf-bindings-d1-audit-hook-v1` | composition root D1; route UNKNOWN |
| 035 | `repo:1232040291:boundary:cf-bindings-kv-audit-hook-v1` | composition root KV; route UNKNOWN |
| 036 | `repo:1232040291:boundary:cf-bindings-do-audit-hook-v1` | composition root DO; route UNKNOWN |
| 037 | `repo:1232040291:boundary:cf-bindings-d1-tenant-query-v1` | upstream identity/tenant owner UNKNOWN |
| 038 | `repo:1232040291:boundary:cf-bindings-kv-tenant-key-v1` | upstream identity/tenant owner UNKNOWN |
| 039 | `repo:1232040291:boundary:cf-bindings-do-tenant-name-v1` | upstream identity/tenant owner UNKNOWN |

### REL-CF-016..024 — dependências normais e target-specific
Cada linha é uma relação Cargo distinta com âncora própria; fonte: `Cargo.toml:26-88`. Alteração pode mudar seleção/compilação; nenhuma resolução/build é afirmada.

| ID | Identidade/fingerprint | Superfície/ativação | Efeito/falha/validação |
|---|---|---|---|
| <a id="rel-cf-016"></a>[REL-CF-016](#rel-cf-016) | `repo:1232040291:boundary:cf-bindings-worker-v1` | package → `worker`; normal, `d1/http`, wasm | API Worker/target incompatível quebra adapter; manifest/source |
| <a id="rel-cf-017"></a>[REL-CF-017](#rel-cf-017) | `repo:1232040291:boundary:cf-bindings-bytes-v1` | package → `bytes`; normal, R2 trait | tipo de payload incompatível quebra trait; manifest/source |
| <a id="rel-cf-018"></a>[REL-CF-018](#rel-cf-018) | `repo:1232040291:boundary:cf-bindings-subtle-v1` | package → `subtle`; normal, tenant comparisons | igualdade/feature muda validação; manifest/source |
| <a id="rel-cf-019"></a>[REL-CF-019](#rel-cf-019) | `repo:1232040291:boundary:cf-bindings-worker-traits-v1` | package → `corelink-worker`; normal, traits | trait/error incompatível quebra impl; manifest/source |
| <a id="rel-cf-020"></a>[REL-CF-020](#rel-cf-020) | `repo:1232040291:boundary:cf-bindings-cas-traits-v1` | package → `corelink-cas`; normal, reexports | superfície canônica muda exports; manifest/source |
| <a id="rel-cf-021"></a>[REL-CF-021](#rel-cf-021) | `repo:1232040291:boundary:cf-bindings-thiserror-v1` | package → `thiserror`; normal, erros | geração de erro/derive quebra build; manifest/source |
| <a id="rel-cf-022"></a>[REL-CF-022](#rel-cf-022) | `repo:1232040291:boundary:cf-bindings-wasm-futures-v1` | package → `wasm-bindgen-futures`; normal, bridge wasm | bridge incompatível quebra target; manifest/source |
| <a id="rel-cf-023"></a>[REL-CF-023](#rel-cf-023) | `repo:1232040291:boundary:cf-bindings-getrandom04-v1` | package → `getrandom_v04`; `wasm32`, feature `wasm_js` | faltar feature/cfg quebra wasm; manifest/config |
| <a id="rel-cf-024"></a>[REL-CF-024](#rel-cf-024) | `repo:1232040291:boundary:cf-bindings-serde-v1` | package → `serde`; `wasm32`, D1 `first` | bound de desserialização muda API do caminho wasm; manifest/source |

### REL-CF-025..031 — dependências de teste host
Cada relação abaixo é host-only em `Cargo.toml:97-106`; não é aresta de produção wasm. Validação é leitura de manifestos/testes; execução host não realizada.

| ID | Identidade/fingerprint | Superfície/ativação | Efeito/falha/validação |
|---|---|---|---|
| <a id="rel-cf-025"></a>[REL-CF-025](#rel-cf-025) | `repo:1232040291:boundary:cf-bindings-proptest-v1` | tests → `proptest`; dev host | prop density/test graph; manifest/tests |
| <a id="rel-cf-026"></a>[REL-CF-026](#rel-cf-026) | `repo:1232040291:boundary:cf-bindings-rand-v1` | tests → `rand`; dev host | geração de casos; manifest/tests |
| <a id="rel-cf-027"></a>[REL-CF-027](#rel-cf-027) | `repo:1232040291:boundary:cf-bindings-rand-chacha-v1` | tests → `rand_chacha`; dev host | determinismo/seeds; manifest/tests |
| <a id="rel-cf-028"></a>[REL-CF-028](#rel-cf-028) | `repo:1232040291:boundary:cf-bindings-test-bytes-v1` | tests → `bytes`; dev host | payload de fake; manifest/tests |
| <a id="rel-cf-029"></a>[REL-CF-029](#rel-cf-029) | `repo:1232040291:boundary:cf-bindings-tokio-v1` | async tests → `tokio`; dev host | runtime de teste; manifest/tests |
| <a id="rel-cf-030"></a>[REL-CF-030](#rel-cf-030) | `repo:1232040291:boundary:cf-bindings-test-worker-v1` | tests → `corelink-worker`; dev host | fake/trait contract; manifest/tests |
| <a id="rel-cf-031"></a>[REL-CF-031](#rel-cf-031) | `repo:1232040291:boundary:cf-bindings-test-cas-v1` | tests → `corelink-cas`; dev host | CAS fake/idempotency; manifest/tests |

**Índice complementar:** [REL-CF-011](#rel-cf-011) · [REL-CF-012](#rel-cf-012) · [REL-CF-013](#rel-cf-013) · [REL-CF-014](#rel-cf-014) · [REL-CF-015](#rel-cf-015) · [REL-CF-016](#rel-cf-016) · [REL-CF-017](#rel-cf-017) · [REL-CF-018](#rel-cf-018) · [REL-CF-019](#rel-cf-019) · [REL-CF-020](#rel-cf-020) · [REL-CF-021](#rel-cf-021) · [REL-CF-022](#rel-cf-022) · [REL-CF-023](#rel-cf-023) · [REL-CF-024](#rel-cf-024) · [REL-CF-025](#rel-cf-025) · [REL-CF-026](#rel-cf-026) · [REL-CF-027](#rel-cf-027) · [REL-CF-028](#rel-cf-028) · [REL-CF-029](#rel-cf-029) · [REL-CF-030](#rel-cf-030) · [REL-CF-031](#rel-cf-031) · [REL-CF-032](#rel-cf-032) · [REL-CF-033](#rel-cf-033) · [REL-CF-034](#rel-cf-034) · [REL-CF-035](#rel-cf-035) · [REL-CF-036](#rel-cf-036) · [REL-CF-037](#rel-cf-037) · [REL-CF-038](#rel-cf-038) · [REL-CF-039](#rel-cf-039).

<a id="rel-cf-011"></a>
### REL-CF-011 — wiring do Worker Clerk
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-clerk-wiring-v1`.
**Tipo/endpoints:** runtime wiring declarado; `corelink-clerk-cf` → R2/D1/KV/DO wrappers.
**Superfície/ativação:** `prod_wiring.rs` constrói `Cf*Real`; somente caminho wasm/Worker selecionado pode atingir bindings.
**Efeito/falha:** erro pré-operação de escopo/audit impede a chamada antes do
backend; o caminho D1 pós-run pode retornar erro depois de mutação e é tratado
separadamente em REL-CF-034. Wiring alterado pode afetar health, audit e região
de tenant.
**Validação/coordenação:** owner `corelink-clerk-cf`; evidência `crates/corelink-clerk-cf/src/prod_wiring.rs:128-220`, `health.rs:342-369`; nenhum Worker foi executado.

<a id="rel-cf-012"></a>
### REL-CF-012 — manifesto Wrangler do Clerk
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-clerk-wrangler-v1`.
**Tipo/endpoints:** config/build-deploy; `corelink-clerk-cf/wrangler.toml` → KV, D1, R2 e `CLERK_DO`.
**Superfície/ativação:** `cargo build --target wasm32-unknown-unknown --release -p corelink-clerk-cf` declarado para build.
**Efeito/falha:** binding ausente/diferente não satisfaz o wiring; IDs/vars são `PLACEHOLDER_*`, portanto não provam recurso ou deploy.
**Validação/coordenação:** operação Cloudflare autorizada; evidência Wrangler estática.

<a id="rel-cf-013"></a>
### REL-CF-013 — materializador de billing wasm
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-billing-v1`.
**Tipo/endpoints:** consumidor semântico; `corelink-billing-stripe-materializer` → `CfD1DatabaseReal`.
**Superfície/ativação:** `CfD1BillingWriter` em `wasm32_binders.rs`; feature `cf-billing-real` e target wasm; o binder usa tenant e writer D1 scoped.
**Efeito/falha:** mudança no wrapper/error pode interromper materialização ou seu fence de audit antes de D1.
**Validação/coordenação:** owner billing materializer, target wasm e teste/fake selecionado; não prova evento Stripe ou D1 real.

<a id="rel-cf-014"></a>
### REL-CF-014 — scheduler DSR wasm
**Tipo/endpoints:** consumidor semântico; `corelink-dsr-statuspage-scheduler` → `CfD1DatabaseReal`.
**Superfície/ativação:** `D1Wasm32RowSource` e consulta tenant-scoped, somente wasm; Wrangler declara cron diário `0 6 * * *`.
**Efeito/falha:** mudança de query/tenant/error pode impedir leitura de resultado DSR; host usa stub para testar validação.
**Validação/coordenação:** owner DSR/privacy, target e fake; evidência `wasm32_row_source.rs`/Wrangler, sem cron observado.

<a id="rel-cf-015"></a>
### REL-CF-015 — logs declarados do Worker
**Tipo/endpoints:** telemetry/config; `corelink-clerk-cf` → Workers Logs.
**Superfície/ativação:** Wrangler declara observabilidade habilitada e amostragem 1; audit wasm emite NDJSON por `worker::console_log!`.
**Efeito/falha:** mudança pode alterar a superfície de diagnóstico do wrapper, mas não prova ingestão, retenção, Logpush ou consumo externo.
**Validação/coordenação:** owner Worker/telemetry; configuração e `audit_sink.rs` estáticos; observação Cloudflare autorizada.

<a id="rel-cf-032"></a>
### REL-CF-032 — Wrangler raiz e bindings compartilhados
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-root-wrangler-v1`; `wrangler.toml:168-208:CLERK_JWKS_KV,NEGATIVE_CACHE_KV,CONFIG_DB,ROLLOUT_DO`.
**Tipo/endpoints:** config/build-deploy; root Wrangler → adapters/consumidores Worker. **Ativação:** configuração selecionada para o Worker raiz.
**Contrato/efeito/falha:** nomes/IDs devem coincidir com lookup/wiring; placeholders ou binding ausente impedem operação; não é prova de recurso/deploy. **Validação:** reconciliação fonte; operação externa autorizada.

<a id="rel-cf-033"></a>
### REL-CF-033 — consumidor opcional `corelink-container`
**Identidade/fingerprint:** `repo:1232040291:boundary:cf-bindings-container-feature-v1`; `corelink-container/Cargo.toml:74-111:cf-r2-real`.
**Tipo/endpoints:** consumidor Cargo opcional; `corelink-container` → `corelink-cf-bindings`. **Ativação:** feature `cf-r2-real`; native usa stub, Worker wasm é cutover planejado.
**Contrato/efeito/falha:** `CfR2BucketReal`/`R2Error` podem alterar compile/wiring; a descrição declara adoção futura, não alcance atual. **Validação:** feature/target e owner container; build não executado.

<a id="b04"></a>
## B04 — Propagação transitiva

| Caminho causal | Condição | Efeito/limite | Validação |
|---|---|---|---|
| `corelink-cf-bindings → corelink-adapters-cloud::cf → consumidores do façade` | import via `pub use` | quebra de símbolo/erro atravessa alias; não prova chamada | busca dos dois paths + checks dos consumers |
| `corelink-cf-bindings → corelink-clerk-cf → Worker Wrangler` | feature/target wasm e wiring selecionados | erro de adapter impede boot/health; configuração não prova deploy | REL-CF-011/012; build wasm separado |
| `corelink-cf-bindings → corelink-billing-stripe-materializer → D1 billing` | feature `cf-billing-real` + wasm | erro D1 interrompe materialização; não prova webhook Stripe/D1 | REL-CF-013; binder e fake |
| `corelink-cf-bindings → corelink-dsr-statuspage-scheduler → cron D1` | target wasm + cron configurado | erro de query impede leitura DSR; cron não prova execução | REL-CF-014; source/Wrangler |
| `corelink-cf-bindings → corelink-container` | feature opcional `cf-r2-real` | compile/wiring pode mudar; código declara cutover futuro | REL-CF-033; feature/source |

Dependência aponta do consumidor para esta crate; impacto propaga da superfície/feature para o consumidor. A fronteira de produção termina no build/configuração declarado: nenhuma linha prova deploy, chamada ou tráfego observado.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | Impacto provável | Validação mínima |
|---|---|---|
| tipo/erro/reexport | quebra de compile/compatibilidade em callers | busca de símbolo e build selecionado |
| escopo/audit | isolamento ou fail-closed muda | fake/stub que desafie rejeição |
| target/getrandom | Worker wasm deixa de resolver | check wasm explícito |
| TTL/serialização D1 | retenção ou dados externos mudam | owner do contrato e teste dirigido |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos/limite | Desconhecidos |
|---|---:|---:|---:|---:|
| Dependências normais/target | 9 | 9 (REL-CF-016–024) | 0 | resolução/build |
| Dependências dev host | 7 | 7 (REL-CF-025–031) | 0 | execução dos testes |
| Consumidores Cargo diretos | 5 | 5 (REL-CF-004/005/006/013/033) | 0 | seleção de features |
| Wranglers relevantes | 2 | 2 (REL-CF-012/032) | 0 | IDs, deploy e ambientes |
| Reexports/interfaces | 1 | 1 (REL-CF-004) | 0 | consumidores indiretos |
| Runtime/deploy/telemetry | declarado | relações 007/011/015 e 034–036 | operação externa fora do escopo | alcance, ingestão e tráfego |

O censo atual corrige a omissão de `corelink-container` e do Wrangler raiz. IDs/vars `PLACEHOLDER_*` não são recursos confirmados; consumidores indiretos do reexport, configuração por ambiente, deploy, ingestão/retenção de logs e runtime permanecem desconhecidos por limite operacional, não como ausência. A rota de revisão de código é `.github/CODEOWNERS:* → @gmhelmold`; a rota operacional Cloudflare não foi verificada.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
