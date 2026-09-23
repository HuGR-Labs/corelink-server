---
schema: corelink-ownership/1.1
document: reference
package: corelink-cf-bindings
manifest: crates/corelink-cf-bindings/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: cf-bindings-pilot-source-20260920
---

# corelink-cf-bindings — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

`corelink-cf-bindings` implementa adapters de bindings Cloudflare para R2, D1, KV e Durable Objects. O alvo entregue pretendido é `wasm32-unknown-unknown`; no host, wrappers `*Real` preservam validação e terminam em stub/fake ou `WasmOnly:`, sem acessar Cloudflare. Possui `cdylib` e `rlib`, quatro integration targets e nenhum feature próprio.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-cf-bindings` / `crates/corelink-cf-bindings/Cargo.toml` |
| Targets | cdylib, rlib, quatro tests |
| Dependências diretas normais | 7: `worker`, `bytes`, `subtle`, `corelink-worker`, `corelink-cas`, `thiserror`, `wasm-bindgen-futures`; uma relação Cargo por declaração, REL-CF-016–022 |
| Dependências diretas target wasm32 | 2: `getrandom_v04` (package `getrandom`, feature `wasm_js`) e `serde`; REL-CF-023–024 |
| Dependências diretas de teste host | 7: `proptest`, `rand`, `rand_chacha`, `bytes`, `tokio`, `corelink-worker`, `corelink-cas`; REL-CF-025–031 |
| Cobertura do inventário | 16 declarações reconciliadas com `[dependencies]`, `[target.'cfg(target_arch = "wasm32")'.dependencies]` e `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]` em `Cargo.toml:26-106`; todas as relações são arestas de manifesto, não seleção/resolução; Cargo não executado |
| implemented | yes — módulos, wrappers, stubs e exports existem no source pin |
| wired | partial — composition root e Wrangler são declarações SOURCE; bindings/deploy não observados |
| runtime_verified | unknown — nenhum Worker/binding/runtime foi observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Implementação | Contrato | Composition root / operação |
|---|---|---|---|
| R2/KV traits | CF bindings | worker/cas | `corelink-clerk-cf::prod_wiring`; Cloudflare autorizado |
| D1/DO wrappers | CF bindings | wrapper público | `health::main`; Cloudflare autorizado |
| import canônico | adapters-cloud | reexport | consumidores; operador não verificado |
| root/Clerk Wrangler | configuração externa | bindings e deploy | `wrangler.toml` / `crates/corelink-clerk-cf/wrangler.toml`; não é implementação |

Não possui credenciais ou prova de Worker ativo. `okf_context.py` não retornou conceito direto para este manifesto nem para `r2_real.rs`; o [OKF de credential handling](../../../knowledge/security/credential-handling.md) é só referência ampla de segurança, não classificação desta superfície. `.github/CODEOWNERS` aponta `@gmhelmold` como owner default e avisa que a regra solicita revisão, mas não é merge gate; aprovador operacional Cloudflare permanece UNKNOWN.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo | Papel | Evidência |
|---|---|---|
| `cf_r2`, `cf_kv`, `cf_d1`, `cf_do` | adapters mínimos wasm32 | `worker::*` |
| `CfR2/CfKvAdapter` | adapters públicos cfg-wasm e traits | API-040–041 |
| `CfD1/CfDurableObjectAdapter` | acesso raw cfg-wasm | API-042–043 |
| `r2_real` | R2 estendido, prefixo, audit e stub | API-001–012, 039 |
| `d1_real` | query tenant-scoped, audit e stub | API-013–022, 039 |
| `kv_real` | KV scoped, TTL e audit | API-023–030, 039 |
| `do_real` | nome de tenant, fetch, fake e audit | API-031–039 |
| `lib.rs` | gating, reexports e aliases | API-001–007 |
| `tests.rs` e `tests/*.rs` | testes/fakes do package | R08 S06 |

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001)[API-002](#api-002)[API-003](#api-003)[API-004](#api-004)[API-005](#api-005)[API-006](#api-006)[API-007](#api-007)[API-008](#api-008)[API-009](#api-009)[API-010](#api-010)[API-011](#api-011)[API-012](#api-012)[API-013](#api-013)[API-014](#api-014)[API-015](#api-015)[API-016](#api-016)[API-017](#api-017)[API-018](#api-018)[API-019](#api-019)[API-020](#api-020)[API-021](#api-021)[API-022](#api-022)[API-023](#api-023)[API-024](#api-024)[API-025](#api-025)[API-026](#api-026)[API-027](#api-027)[API-028](#api-028)[API-029](#api-029)[API-030](#api-030)[API-031](#api-031)[API-032](#api-032)[API-033](#api-033)[API-034](#api-034)[API-035](#api-035)[API-036](#api-036)[API-037](#api-037)[API-038](#api-038)[API-039](#api-039)[API-040](#api-040)[API-041](#api-041)[API-042](#api-042)[API-043](#api-043)

Cada registro abaixo descreve um símbolo público exato. Erros de binding são convertidos para `R2Error::Backend`, `D1Error::Backend`, `KvError::Backend` ou `DoError::Backend`; rejeição do hook é propagada antes do dispatch. Os construtores `new` usam audit no-op, portanto produção deve substituir o hook com `with_audit`. Fontes são do source pin no front matter.

<a id="api-001"></a>
### API-001 — `r2_real::TenantPrefix::new` [↩](#r04)
**Entrada/pré-condição:** string convertível, sem requisitos de encoding adicionais. **Saída/erro:** `TenantPrefix`; vazio, `/` ou NUL → `R2Error::Backend("tenant_prefix: …")`. **Efeito/compatibilidade:** valida forma, não deriva identidade canônica; mudança altera aceitação do prefixo. **Evidência:** `src/r2_real.rs:97-180`.
[Índice](#r04) · [INV-001](#inv-001)

<a id="api-002"></a>
### API-002 — `CfR2BucketReal::{new,stub_for_native_tests}` [↩](#r04)
**Entrada/pré-condição:** wasm recebe `worker::Bucket` e `TenantPrefix`; host recebe apenas `TenantPrefix`. **Saída/erro:** wrapper com hook no-op. **Efeito/compatibilidade:** host é stub, não binding; assinatura difere por `cfg`. Trocar construtor/configuração exige revisar consumidores e ativação do target. **Evidência:** `src/r2_real.rs:354-360,570-576`.

<a id="api-003"></a>
### API-003 — `CfR2BucketReal::tenant` [↩](#r04)
**Entrada/pré-condição:** referência ao wrapper. **Saída/erro:** `&TenantPrefix`, sem erro. **Efeito/compatibilidade:** leitura somente; não expõe mutação. **Evidência:** `src/r2_real.rs:272-275`.

<a id="api-004"></a>
### API-004 — `CfR2BucketReal::with_audit` [↩](#r04)
**Entrada/pré-condição:** closure `AuditFn: Fn(R2Op, &str) -> Result<(), R2Error> + Send + Sync + 'static`. **Saída/erro:** wrapper consumido e devolvido, sem erro. **Efeito/compatibilidade:** substitui o hook; erro do hook bloqueia operações auditadas antes do dispatch. **Evidência:** `src/r2_real.rs:221-221,278-281`.

<a id="api-005"></a>
### API-005 — `CfR2BucketReal::scoped_key` [↩](#r04)
**Entrada/pré-condição:** `&str`; prefixo já completo ou tail local. **Saída/erro:** `TenantScopedKey`; vazio/NUL, slash inicial, `//`, prefixo sem tail → `R2Error::Backend(tenant_prefix: …)`. **Efeito/compatibilidade:** completa `<tenant>/`; `tenant//tail` atualmente passa. **Evidência:** `src/r2_real.rs:302-341`.
[INV-001](#inv-001) · REL-CF-037

<a id="api-006"></a>
### API-006 — `CfR2BucketReal::head` [↩](#r04)
**Entrada/pré-condição:** chave escopável. **Saída/erro:** `bool` (ausente = `false`); validação, audit ou binding → `R2Error`. **Efeito/compatibilidade:** leitura auditada como `R2Op::Head`, sem corpo. **Evidência:** `src/r2_real.rs:372-385`.

<a id="api-007"></a>
### API-007 — `CfR2BucketReal::get_bytes` [↩](#r04)
**Entrada/pré-condição:** chave escopável. **Saída/erro:** `Bytes`; ausência → `R2Error::NotFound`, objeto sem corpo ou binding → `R2Error::Backend`. **Efeito/compatibilidade:** leitura wasm; não chama audit neste método. **Evidência:** `src/r2_real.rs:387-407`.

<a id="api-008"></a>
### API-008 — `CfR2BucketReal::put_if_absent` [↩](#r04)
**Entrada/pré-condição:** chave escopável e payload `Bytes`. **Saída/erro:** `BackendPutOutcome::{Stored,AlreadyExists}` ou `R2Error`; audit precede head-probe/put. **Efeito/compatibilidade:** probe seguido de PUT não é operação condicional atômica; duplicata não sobrescreve pela decisão local, mas há janela de corrida. **Evidência:** `src/r2_real.rs:409-439`.

<a id="api-009"></a>
### API-009 — `CfR2BucketReal::delete` [↩](#r04)
**Entrada/pré-condição:** chave escopável. **Saída/erro:** `()` ou `R2Error`. **Efeito/compatibilidade:** `R2Op::Delete` antes de apagar; remover objeto ausente é sucesso. **Evidência:** `src/r2_real.rs:441-451`.

<a id="api-010"></a>
### API-010 — `CfR2BucketReal::list_keys` [↩](#r04)
**Entrada/pré-condição:** `Option<u32>` limite. **Saída/erro:** `Vec<String>` de chaves completas ou `R2Error`. **Efeito/compatibilidade:** lista somente sob `<tenant>/`, audita a raiz; limite usa unidade de objetos/chaves conforme binding. **Evidência:** `src/r2_real.rs:453-470`.

<a id="api-011"></a>
### API-011 — `CfR2BucketReal::{create_multipart_upload,upload_part}` [↩](#r04)
**Entrada/pré-condição:** create recebe chave; upload recebe handle, chave scoped, `u16` part number e `Bytes`. **Saída/erro:** create retorna `(MultipartUpload, String)`; upload retorna `UploadedPart`; `R2Error` em validação/audit/binding. **Efeito/compatibilidade:** create retorna chave canônica para trilha; upload audita a chave fornecida, cujo vínculo ao handle é responsabilidade do caller. **Evidência:** `src/r2_real.rs:473-506`.

<a id="api-012"></a>
### API-012 — `CfR2BucketReal::{complete_multipart_upload,abort_multipart_upload}` [↩](#r04)
**Entrada/pré-condição:** consome `MultipartUpload`; complete também recebe `Vec<UploadedPart>`; ambos recebem chave scoped de auditoria. **Saída/erro:** `()` ou `R2Error`. **Efeito/compatibilidade:** `CompleteMultipart`/`AbortMultipart` são auditados antes da chamada; erro no hook bloqueia a chamada. **Evidência:** `src/r2_real.rs:508-538`.

<a id="api-013"></a>
### API-013 — `d1_real::TenantId::new` [↩](#r04)
**Entrada/pré-condição:** string não vazia, até 128 bytes, sem aspas simples/duplas, barra invertida, NUL ou whitespace. **Saída/erro:** `TenantId` ou `D1Error::Backend("tenant_id: …")`. **Efeito/compatibilidade:** valida shape, não prova derivação canônica. **Evidência:** `src/d1_real.rs:166-208`.

<a id="api-014"></a>
### API-014 — `TenantId::{as_str,ct_eq_str}` [↩](#r04)
**Entrada/pré-condição:** referência ou candidato `&str`. **Saída/erro:** string emprestada ou igualdade constante por byte (`false` em comprimentos diferentes). **Efeito/compatibilidade:** `as_str` exige não registrar tenant; igualdade protege o primeiro bind. **Evidência:** `src/d1_real.rs:208-249`.

<a id="api-015"></a>
### API-015 — `CfD1DatabaseReal::{new,stub_for_native_tests}` [↩](#r04)
**Entrada/pré-condição:** wasm recebe `worker::D1Database` e `TenantId`; host recebe `TenantId`. **Saída/erro:** wrapper com audit no-op. **Efeito/compatibilidade:** wrapper wasm guarda `Arc<D1Database>`; host é stub e assinatura varia por `cfg`. **Evidência:** `src/d1_real.rs:307-328,547-556,667-674`.

<a id="api-016"></a>
### API-016 — `CfD1DatabaseReal::{tenant,with_audit}` [↩](#r04)
**Entrada/pré-condição:** wrapper e, para `with_audit`, closure `AuditFn(D1Op, &str) -> Result<(), D1Error>`. **Saída/erro:** `&TenantId` ou wrapper consumido/devolvido. **Efeito/compatibilidade:** hook substitui o no-op; erro em hook auditado impede dispatch subsequente. **Evidência:** `src/d1_real.rs:290,342-351`.

<a id="api-017"></a>
### API-017 — `CfD1DatabaseReal::scoped_query` [↩](#r04)
**Entrada/pré-condição:** SQL não vazio/NUL; SELECT/UPDATE/DELETE exige substring rasa `WHERE tenant_id = ?`; INSERT exige `tenant_id` na primeira lista entre parênteses. **Saída/erro:** `TenantScopedQuery` ou `D1Error::Backend("tenant_scope: …")`. **Efeito/compatibilidade:** não é parser SQL; preserva SQL original e pode aceitar falsos positivos estruturais. **Evidência:** `src/d1_real.rs:267-274,363-408`.
[INV-002](#inv-002)

<a id="api-018"></a>
### API-018 — `CfD1DatabaseReal::verify_first_bind` [↩](#r04)
**Entrada/pré-condição:** candidato `&str`. **Saída/erro:** `()` se byte-equal ao tenant ancorado; divergência → `D1Error::Backend("tenant_bind: …")`. **Efeito/compatibilidade:** comparação constante por byte para comprimentos iguais; não verifica outros parâmetros. **Evidência:** `src/d1_real.rs:416-427`.
[INV-002](#inv-002)

<a id="api-019"></a>
### API-019 — `CfD1DatabaseReal::prepare` [↩](#r04)
**Entrada/pré-condição:** `&TenantScopedQuery`. **Saída/erro:** wasm `D1PreparedStatement`; host `Result<(), D1Error>` termina `WasmOnly`; audit `Prepare` precede prepare. **Efeito/compatibilidade:** SQL bruto pode ser obtido via `as_str`; binding não executado no host. **Evidência:** `src/d1_real.rs:566-580,676-680`.

<a id="api-020"></a>
### API-020 — `CfD1DatabaseReal::bind` [↩](#r04)
**Entrada/pré-condição:** wasm statement + `&[&str]`; primeiro item obrigatório e igual ao tenant. **Saída/erro:** statement bindado no wasm; host stub retorna `()` somente se depois falhar `WasmOnly`; vazio/divergente → `tenant_bind:`, erro Worker → `D1Error::Backend`. **Efeito/compatibilidade:** todos parâmetros são strings, em ordem; `Bind` auditado depois da validação. **Evidência:** `src/d1_real.rs:584-602,683-691`.

<a id="api-021"></a>
### API-021 — `CfD1DatabaseReal::{first,all}` [↩](#r04)
**Entrada/pré-condição:** wasm recebe statement e `first` opcionalmente nome da coluna; host não recebe esses argumentos. **Saída/erro:** `first<T>` retorna `Option<T>` desserializado; `all` retorna `D1Result`; host retorna stub `WasmOnly`; hook/binding → `D1Error`. **Efeito/compatibilidade:** leituras auditadas, sem mutação declarada. **Evidência:** `src/d1_real.rs:604-633,693-705`.

<a id="api-022"></a>
### API-022 — `CfD1DatabaseReal::run` [↩](#r04)
**Entrada/pré-condição:** statement preparado no wasm; host não recebe statement. **Saída/erro:** wasm `D1Result`; host `WasmOnly`; erro pre-audit bloqueia dispatch. **Efeito/compatibilidade:** wasm emite audit antes do run e de novo após se `meta.changes > 0`; falha no pós-audit pode ocorrer depois da mutação. **Evidência:** `src/d1_real.rs:635-659,707-713`.
[INV-004](#inv-004) · REL-CF-034

<a id="api-023"></a>
### API-023 — `kv_real::TenantPrefix::new` [↩](#r04)
**Entrada/pré-condição:** string não vazia, sem `:` e sem NUL. **Saída/erro:** prefixo KV ou `KvError::Backend("tenant_prefix: …")`. **Efeito/compatibilidade:** usa separador `:` e é distinto de `r2_real::TenantPrefix`. **Evidência:** `src/kv_real.rs:95-157`.

<a id="api-024"></a>
### API-024 — `CfKvNamespaceReal::{new,stub_for_native_tests}` [↩](#r04)
**Entrada/pré-condição:** wasm recebe `KvStore` e prefixo; host recebe só prefixo. **Saída/erro:** wrapper com hook no-op. **Efeito/compatibilidade:** host stub; assinatura varia por `cfg`; construtor real requer prefixo já validado. **Evidência:** `src/kv_real.rs:244-267,382-391,500-506`.

<a id="api-025"></a>
### API-025 — `CfKvNamespaceReal::{tenant,with_audit}` [↩](#r04)
**Entrada/pré-condição:** wrapper; closure `AuditFn(KvOp, &str) -> Result<(), KvError>`. **Saída/erro:** `&TenantPrefix` ou wrapper consumido/devolvido. **Efeito/compatibilidade:** substitui hook; erro do hook bloqueia operação auditada. **Evidência:** `src/kv_real.rs:226,276-285`.

<a id="api-026"></a>
### API-026 — `CfKvNamespaceReal::scoped_key` [↩](#r04)
**Entrada/pré-condição:** `&str` como chave completa ou tail local. **Saída/erro:** `TenantScopedKey`; vazio/NUL, `:` inicial ou prefixo completo sem tail → `KvError::Backend(tenant_prefix: …)`. **Efeito/compatibilidade:** completa `<tenant>:`; `tail` contendo `::` é aceito no branch local. **Evidência:** `src/kv_real.rs:302-369`.
[INV-003](#inv-003) · REL-CF-038

<a id="api-027"></a>
### API-027 — `CfKvNamespaceReal::get_bytes` [↩](#r04)
**Entrada/pré-condição:** chave escopável. **Saída/erro:** `Option<Vec<u8>>` (`None` em miss/soft eviction) ou `KvError`. **Efeito/compatibilidade:** audit `Get` e leitura dos bytes; não define duração de retenção. **Evidência:** `src/kv_real.rs:400-411`.

<a id="api-028"></a>
### API-028 — `CfKvNamespaceReal::put_bytes` [↩](#r04)
**Entrada/pré-condição:** chave escopável, `&[u8]`, TTL opcional em segundos. **Saída/erro:** `()` ou `KvError`; `Some(n < 60)` é elevado a 60. **Efeito/compatibilidade:** `None` não configura expiração; `Put` auditado antes do write. **Evidência:** `src/kv_real.rs:80-81,413-434`.
[INV-003](#inv-003)

<a id="api-029"></a>
### API-029 — `CfKvNamespaceReal::delete` [↩](#r04)
**Entrada/pré-condição:** chave escopável. **Saída/erro:** `()` ou `KvError`. **Efeito/compatibilidade:** `Delete` auditado; inexistência é sucesso. **Evidência:** `src/kv_real.rs:436-446`.

<a id="api-030"></a>
### API-030 — `CfKvNamespaceReal::list_keys` [↩](#r04)
**Entrada/pré-condição:** limite opcional em contagem de chaves (`u64`). **Saída/erro:** `Vec<String>` com nomes completos ou `KvError`. **Efeito/compatibilidade:** prefixo `<tenant>:` e audit na raiz; limite é encaminhado ao binding. **Evidência:** `src/kv_real.rs:448-468`.

<a id="api-031"></a>
### API-031 — `do_real::DoTenantPrefix::new` [↩](#r04)
**Entrada/pré-condição:** string não vazia, sem `:`, `/`, NUL ou whitespace. **Saída/erro:** prefixo ou `DoError::Backend("tenant_scope: …")`. **Efeito/compatibilidade:** valida forma; identidade canônica é responsabilidade upstream. **Evidência:** `src/do_real.rs:174-225`.

<a id="api-032"></a>
### API-032 — `CfDurableObjectReal::{new,stub_for_native_tests,with_fake_router}` [↩](#r04)
**Entrada/pré-condição:** wasm recebe namespace e prefixo; host recebe prefixo, opcionalmente `FakeDoRouter`. **Saída/erro:** wrapper; hook no-op por padrão. **Efeito/compatibilidade:** host sem router é `WasmOnly`; fake permite exercício local e não equivale a Cloudflare DO. **Evidência:** `src/do_real.rs:429-457,590-598,727-749`.

<a id="api-033"></a>
### API-033 — `CfDurableObjectReal::{tenant,with_audit,scoped_name}` [↩](#r04)
**Entrada/pré-condição:** prefixo/hook ou nome não vazio, sem whitespace/NUL. **Saída/erro:** `&DoTenantPrefix`, wrapper ou `TenantScopedName`; nome em forma canônica exige tenant id coincidente e purpose não vazio, senão `DoError::Backend(tenant_scope: …)`. **Efeito/compatibilidade:** bare purpose vira `tenant:<id>:<purpose>`. **Evidência:** `src/do_real.rs:284,464-550`.
[INV-004](#inv-004)

<a id="api-034"></a>
### API-034 — `CfDurableObjectReal::stub_by_name` [↩](#r04)
**Entrada/pré-condição:** nome com escopo validável. **Saída/erro:** wasm `worker::Stub`; host sem fake retorna `WasmOnly`; validação/audit/resolve retornam `DoError`. **Efeito/compatibilidade:** audita `Resolve`, resolve id e stub sem fetch. **Evidência:** `src/do_real.rs:615-635,758-765`.

<a id="api-035"></a>
### API-035 — `CfDurableObjectReal::stub_by_hex_id` [↩](#r04)
**Entrada/pré-condição:** ID hexadecimal ASCII exatamente 64 caracteres. **Saída/erro:** wasm `Stub`; host sem fake `WasmOnly`; inválido → `tenant_scope:`, falha de resolve → `do_resolve:`. **Efeito/compatibilidade:** ID opaco, audit usa `hex:<id>` e resolve via `id_from_string`. **Evidência:** `src/do_real.rs:637-670,768-777`.

<a id="api-036"></a>
### API-036 — `CfDurableObjectReal::fetch_with_str` [↩](#r04)
**Entrada/pré-condição:** nome escopável e URL string aceita pelo Worker. **Saída/erro:** wasm `worker::Response`; host fake/stub `DoError`; validação/hook/resolve/fetch podem falhar. **Efeito/compatibilidade:** audit `FetchStr` antes de resolve e fetch; URL é encaminhada. **Evidência:** `src/do_real.rs:672-691,797-814`.

<a id="api-037"></a>
### API-037 — `CfDurableObjectReal::fetch_with_request` [↩](#r04)
**Entrada/pré-condição:** wasm recebe nome escopável e `worker::Request` consumível; host recebe nome, method, URL e body bytes. **Saída/erro:** wasm `worker::Response`; host `FakeFetchResponse` pelo router ou `DoError`. **Efeito/compatibilidade:** audit `FetchRequest` antes de resolve/fetch; o fake codifica method como prefixo da URL roteada e não encaminha os bytes do body. **Evidência:** `src/do_real.rs:693-725,816-839`.

<a id="api-038"></a>
### API-038 — `FakeDoRouter::{new,constant,always_fail,on,dispatch}` [↩](#r04)
**Entrada/pré-condição:** handler `Fn(&str,&str) -> Result<FakeFetchResponse,DoError>` ou resposta/regra; `on` casa prefixo de nome e URL. **Saída/erro:** router imutável e resposta fake ou `DoError`. **Efeito/compatibilidade:** somente native test seam; nunca comprova execução Cloudflare. **Evidência:** `src/do_real.rs:302-414`.

<a id="api-039"></a>
### API-039 — `CfR2BucketReal::inner`, `CfKvNamespaceReal::inner`, `CfD1DatabaseReal::inner`, `CfDurableObjectReal::inner` [↩](#r04)
**Entrada/pré-condição:** referência ao wrapper e target wasm32. **Saída/erro:** referência ao binding subjacente. **Efeito/compatibilidade:** escape raw que contorna validação tenant scoped e audit do wrapper; indisponível nos stubs host. **Evidência:** `src/r2_real.rs:366-369`, `src/kv_real.rs:394-397`, `src/d1_real.rs:559-562`, `src/do_real.rs:602-605`.

<a id="api-040"></a>
### API-040 — `CfR2BucketAdapter::{new,inner}` e `R2Backend` [↩](#r04)
**Entrada/pré-condição:** `worker::Bucket`; somente wasm32. **Saída/erro:** adapter raw e métodos `put_if_none_match`, `get`, `head` pela trait. **Efeito/compatibilidade:** contrato de CAS/leitura da trait; erro `R2Error`; sem shim host. **Evidência:** `src/cf_r2.rs:40-143`; REL-CF-001.

<a id="api-041"></a>
### API-041 — `CfKvNamespaceAdapter::{new,inner}` e `KvBackend` [↩](#r04)
**Entrada/pré-condição:** `KvStore`; somente wasm32. **Saída/erro:** adapter raw e `get`, `put_with_ttl`, `delete` pela trait; `KvError`. **Efeito/compatibilidade:** TTL em segundos pela trait; sem shim host. **Evidência:** `src/cf_kv.rs:30-96`; REL-CF-002.

<a id="api-042"></a>
### API-042 — `CfD1DatabaseAdapter::{new,inner,prepare,exec}` [↩](#r04)
**Entrada/pré-condição:** `D1Database`; raw SQL `&str`; somente wasm32. **Saída/erro:** prepared statement ou `WorkerResult<()>`. **Efeito/compatibilidade:** caminho raw não aplica `TenantScopedQuery` nem audit real. **Evidência:** `src/cf_d1.rs:30-74`.

<a id="api-043"></a>
### API-043 — `CfDurableObjectAdapter::{new,inner,stub_by_name,stub_by_hex_id}` [↩](#r04)
**Entrada/pré-condição:** `ObjectNamespace`, nome ou hex ID string; somente wasm32. **Saída/erro:** `Stub` ou `WorkerResult`. **Efeito/compatibilidade:** acesso raw não aplica escopo/audit de `CfDurableObjectReal`. **Evidência:** `src/cf_do.rs:35-79`.

[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

[INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

| Estado | Owner | Vida |
|---|---|---|
| prefixo/nome/query scoped | wrapper | request/operação |
| audit hook | consumidor | wrapper |
| binding `worker::*` | Cloudflare | runtime externo |

Os wrappers mantêm apenas prefixo/tenant e callback em memória durante a instância/request; não possuem schema, fila, lock ou persistência própria. Chaves R2/KV, nomes DO e SQL D1 são strings UTF-8; somente KV tem TTL explícito, com piso de 60 segundos. Durabilidade, concorrência e compensação pertencem aos bindings/consumidores externos e estão nas RELs correspondentes, não neste package.

<a id="inv-001"></a>
### INV-001 — R2 não cruza tenant
**Predicado:** toda chave aceita entregue ao backend começa com `<tenant>/`; tail bare é prefixado; entrada vazia/NUL, slash inicial ou `//` no tail bare é rejeitada, e chave já-prefixada exige tail não vazio. `tenant//tail` é aceito pela implementação atual e não é declarado rejeitado.
**Imposição:** `scoped_key` em `r2_real`; **violação:** backend recebe chave sem prefixo ou forma bare proibida; **verificação:** leitura de `r2_real.rs:302-331` e testes fake/stub; **estado:** SOURCE, não runtime CF.
[Índice de invariantes](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — D1 liga query ao tenant
**Predicado:** SQL não vazio e sem NUL só é aceito quando o primeiro verbo é SELECT/UPDATE/DELETE com a busca rasa por `WHERE`, `tenant_id`, `=` e `?`, ou INSERT com `tenant_id` na primeira lista entre parênteses.

**Imposição:** `scoped_query`, `verify_first_bind` e `bind`. **Violação:** verbo não permitido, escopo ausente ou bind vazio/divergente passa pela validação. **Verificação:** testes host em `src/d1_real.rs` e `tests/d1_real.rs`. **Estado:** SOURCE; validação textual/rasa não prova semântica SQL nem uso do primeiro bind via `inner()`.
[Índice de invariantes](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — KV respeita prefixo e piso TTL
**Predicado:** para `get_bytes`, `put_bytes` e `delete`, chave vazia/NUL ou começando por `:` é rejeitada; chave local vira `<tenant>:<key>` e já prefixada exige tail não vazio. Todo TTL `Some(n)` enviado é `max(n,60)` segundos; `None` não configura expiração.

**Imposição:** `scoped_key`, `audit_and_scope`, `clamp_ttl`. **Violação:** chave inválida alcança o binding ou `Some(n<60)` não vira 60. **Verificação:** testes host em `src/kv_real.rs` e `tests/kv_real.rs`. **Estado:** SOURCE; `list_keys` usa raiz; `inner()` pode contornar a regra.
[Índice de invariantes](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Audit pré-operação impede mutação/fetch
**Predicado:** em DO fetch e `stub_by_name`, erro do `AuditFn` antecede resolução/fetch. Em D1 wasm `run`, erro pre-run bloqueia dispatch; erro pós-run só pode ocorrer depois de `changes > 0` e não desfaz mutação.

**Imposição:** callsites de audit em `do_real.rs`/`d1_real.rs`. **Violação:** binding chamado apesar da rejeição prévia ou erro pós-run documentado como rollback. **Verificação:** testes audit fail-closed em `tests/do_real.rs`, `tests/d1_real.rs` e fonte `d1_real.rs:635-659`. **Estado:** SOURCE; runtime CF não observado.
[Índice de invariantes](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

O único target operacional pretendido é `wasm32-unknown-unknown`. `worker` habilita `d1` e `http`; `getrandom_v04/wasm_js` e `serde` são dependências condicionadas a wasm, e `getrandom_v04` requer o `rustflag` de workspace. No host, módulos que alcançariam `worker::*` são cfg-gated e os wrappers dual-target usam stub/fake; isso é compatibilidade de teste, não adapter funcional.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Significado | Ação |
|---|---|---|
| `WasmOnly:` | target errado, após validação | selecionar wasm ou fake |
| `tenant_scope:` | escopo inválido | parar e corrigir caller |
| `audit:` | policy negou operação | não repetir cegamente |

<a id="r08"></a>
## R08 — Verificação e evidências

| ID | Fonte/âncora no pin | Classe | Resultado observado / limite |
|---|---|---|---|
| S01 | `crates/corelink-cf-bindings/Cargo.toml:1-106` | SOURCE | package, targets, deps, cfg e dev-deps transcritos; sem resolver/build |
| S02 | `src/lib.rs:53-135`, `src/cf_r2.rs:40-143`, `src/cf_kv.rs:30-96`, `src/cf_d1.rs:30-74`, `src/cf_do.rs:30-79`, `src/r2_real.rs:231-893`, `src/d1_real.rs:300-745`, `src/kv_real.rs:238-731`, `src/do_real.rs:425-905` | SOURCE | exports, adapters, aliases, invariantes, raw escapes, fakes e stubs lidos; sem execução |
| S03 | `crates/corelink-clerk-cf/src/prod_wiring.rs:128-220`, `health.rs:342-369` | SOURCE | composition root e entrada fetch identificados; Worker não observado |
| S04 | `wrangler.toml:168-208`, `crates/corelink-clerk-cf/wrangler.toml:20-153` | SOURCE | bindings, cron, migrations e logs declarados; IDs PLACEHOLDER e deploy desconhecido |
| S05 | `crates/corelink-adapters-cloud/src/cf.rs:1-13`, `crates/corelink-container/Cargo.toml:74-111`, `crates/corelink-billing-stripe-materializer/Cargo.toml:24-71`, `crates/corelink-clerk-cf/Cargo.toml:68-76`, `crates/corelink-dsr-statuspage-scheduler/Cargo.toml:20-31` | SOURCE | consumidores diretos/reexport e features enumerados no BLAST; grafo resolvido/runtime desconhecidos |
| S06 | `src/tests.rs:1-414`, `tests/d1_real.rs:1-353`, `tests/do_real.rs:1-464`, `tests/kv_real.rs:1-501`, `tests/prop_cas_idempotency.rs:1-307` | SOURCE | suites/fakes catalogadas; execução não realizada |

O source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` não mudou nesses caminhos até o HEAD da campanha. A rota humana verificada é `.github/CODEOWNERS:* → @gmhelmold`; operação Cloudflare e execução wasm permanecem UNKNOWN; nenhuma execução de Cargo, binding ou runtime é afirmada.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01).
