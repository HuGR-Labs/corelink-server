---
schema: corelink-ownership/1.1
document: reference
package: corelink-handler-cas-erase
manifest: crates/corelink-handler-cas-erase/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-cas-erase-structural-normalization-20260921
---

# corelink-handler-cas-erase — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003)

`corelink-handler-cas-erase` é um kernel de lógica pura para uma solicitação de erase por hash: valida a forma da solicitação, constrói um marcador de tombstone, decide o gate de leitura a partir de um booleano recebido e classifica re-erase. A crate não declara trait de storage, cliente R2/D1, rota, autenticação, I/O ou estado próprio.

| Campo | Evidência SOURCE |
|---|---|
| Package / manifesto | `crates/corelink-handler-cas-erase/Cargo.toml` |
| Raiz e reexports | `src/lib.rs` |
| Módulos | `handler`, `error` |
| Test target declarado | `tests/prop_handler_cas_erase.rs` |
| Runtime | não observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Esta crate possui | Não prova / não possui |
|---|---|---|
| Pedido e validação | tipos de string e decisão pura | identidade autenticada real ou autorização |
| Tombstone | `TombstoneMarker`, `ReadGate` e mapeamento booleano | tabela D1, lookup, upsert ou durabilidade |
| Re-erase | enum `EraseOutcome` e mapeamento de tombstone prévio | delete R2, ordem delete/upsert ou rollback |
| Falhas | taxonomia tipada | backend/HTTP efetivo ou logging seguro em runtime |

R2 delete e transporte de tombstone D1 pertencem à composição em
`corelink-container::routes::cas_erase`; essa é uma relação externa em B03,
não implementação desta crate nem evidência de execução.

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo | Papel fonte |
|---|---|
| `src/lib.rs` | módulos públicos e reexports canônicos |
| `src/handler.rs` | request, marker, outcomes e quatro funções puras |
| `src/error.rs` | `CasEraseError` não exaustivo |
| `tests/prop_handler_cas_erase.rs` | propriedades declaradas; não executadas nesta campanha |

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006).

<a id="api-001"></a>
### API-001 — Validação de digest
**Símbolo:** `validate_digest(&str) -> Result<(), CasEraseError>`.
**Contrato:** aceita somente digest não vazio, com no máximo 128 bytes e caracteres ASCII alfanuméricos, `-` ou `_`; rejeita qualquer outra entrada como `InvalidDigest` e expõe no erro no máximo os primeiros 128 caracteres iterados.
**Efeitos:** nenhuma persistência, consulta ou I/O. **Compatibilidade:** uma gramática diferente altera a admissão antes da fronteira de composição. Fonte: `src/handler.rs`.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Preparação fail-closed
**Símbolos:** `CasEraseRequest`, `prepare_erase`.
**Contrato:** compara primeiro `path_tenant` contra `auth_tenant`; se diferirem retorna `CrossTenantDenied` sem validar o digest. Quando iguais, valida digest e retorna `TombstoneMarker { tenant: auth_tenant, digest, reason }`.
**Limite:** os três campos são strings fornecidas pelo caller; a função não autentica o tenant nem grava o marker. Fonte: `src/handler.rs`.

<a id="api-003"></a>
[↩](#r01)
### API-003 — Gate de leitura
**Símbolos:** `read_gate`, `ReadGate`.
**Contrato:** `read_gate(true) == Gone`; `read_gate(false) == Proceed`.
**Limite:** o booleano já foi resolvido pelo caller; a crate não busca tombstone, chama D1 nem produz resposta HTTP. Fonte: `src/handler.rs`.

<a id="api-004"></a>
[↩](#r01)
### API-004 — Re-erase idempotente
**Símbolos:** `erase_outcome`, `EraseOutcome`.
**Contrato:** `erase_outcome(true) == AlreadyErased`; `erase_outcome(false) == Erased`. A decisão depende exclusivamente do sinal prévio fornecido.
**Limite:** a crate não executa delete nem upsert, portanto não estabelece ordenação, atomicidade ou recuperação de efeitos externos. Fonte: `src/handler.rs`.

<a id="api-005"></a>
[↩](#r01)
### API-005 — Tipos de dados e construtores
**Símbolos:** `CasEraseRequest::new`, `TombstoneMarker::new`.
**Contrato:** ambos convertem entradas para `String` e as armazenam; os campos das structs são públicos. Os construtores não chamam `validate_digest`, não comparam tenants e não limitam `reason`.
**Compatibilidade:** nomes, campos e ownership são superfície pública reexportada por `lib.rs`. Fonte: `src/{handler,lib}.rs`.

<a id="api-006"></a>
[↩](#r01)
### API-006 — Taxonomia de erro
**Símbolo:** `CasEraseError` `#[non_exhaustive]`.
**Contrato:** enum distingue `CrossTenantDenied`, `InvalidDigest` e `Transport(String)`. Nesta fonte, `prepare_erase`/`validate_digest` produzem apenas os dois primeiros; `Transport` é uma variante para integração, não uma implementação de transporte.
**Compatibilidade:** consumers devem manter braço curinga por `#[non_exhaustive]`; não parsear `Display`. Fonte: `src/error.rs`, `src/handler.rs`.
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Owner nesta fonte | Vida observável |
|---|---|---|
| request/marker/enum | caller | valor Rust em memória |
| presença de tombstone | caller da função `read_gate` | apenas booleano de entrada |
| existência prévia | caller da função `erase_outcome` | apenas booleano de entrada |

<a id="inv-001"></a>

### INV-001 — Tenant divergente vence digest malformado
**Predicado falsificável:** para `path_tenant != auth_tenant`, `prepare_erase` retorna `CrossTenantDenied` mesmo quando o digest é inválido; não alcança `validate_digest` nessa chamada.
**Imposição SOURCE:** ordem dos dois `if`/chamada em `src/handler.rs`. **Limite:** não prova que `auth_tenant` veio de autenticação real.

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Tombstone presente decide Gone exatamente
**Predicado falsificável:** `read_gate(present)` retorna `Gone` se e somente se `present` é `true`; para `false` retorna `Proceed`.
**Imposição SOURCE:** expressão total em `src/handler.rs`. **Limite:** não prova a precisão/consistência do lookup que produziu `present`.

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Re-erase é um no-op classificatório
**Predicado falsificável:** `erase_outcome(true)` retorna somente `AlreadyErased`, e `erase_outcome(false)` somente `Erased`.
**Imposição SOURCE:** expressão total em `src/handler.rs`. **Limite:** o no-op de storage é uma responsabilidade da composição; esta crate apenas classifica o sinal recebido.
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

O manifesto declara `thiserror` em `[dependencies]`, `proptest` em `[dev-dependencies]` e um test target explícito. Não há feature, endpoint, credencial, binding, configuração R2/D1 ou target de runtime demonstrado por esta fonte. Valores workspace de versão/licença/publicação não provam publicação ou seleção de target. Cargo e testes não foram executados nesta campanha.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Significado neste contrato | Ação segura |
|---|---|---|
| `CrossTenantDenied` | strings de tenant divergiram na validação pura | revisar origem da identidade no owner da rota |
| `InvalidDigest` | gramática/limite local recusou a entrada | não expandir chave de storage por inferência |
| `Transport` | categoria reservada para integração | encaminhar para owner da composição; não alegar backend |

Não há logger, métrica, audit sink ou observer nesta crate. Comentários de código que descrevem status HTTP são documentação de intenção local, não evidência de rota montada ou resposta observada.

<a id="r08"></a>
## R08 — Verificação e evidências

Evidência permitida: SOURCE em `Cargo.toml`, `src/{lib,handler,error}.rs`, test target declarado e pesquisa estática de manifest/import. O consumidor estático conhecido é `corelink-container`, classificado em B03. Não foram executados Cargo, testes, rede, R2, D1, rota, autenticação, deploy, publicação ou revisão fria. Desconhecidos: censo inverso completo, construção do booleano de tombstone, armazenamento/retensão do marker, ordenação delete/upsert, mapeamento HTTP efetivo, autorizador, dados reais e comportamento em runtime.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01).
