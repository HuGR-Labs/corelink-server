---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-handler-customer
manifest: crates/corelink-handler-customer/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-customer-structural-normalization-20260921
---

# corelink-handler-customer — blast radius

Relações atômicas derivadas de manifesto, exports e fonte estática. Setas
indicam contrato ou fluxo local declarado; não estabelecem acesso a cliente,
identidade, persistência, Worker CF ou operação. O mount nativo é declarado
na composição do container, mas seleção do artefato e runtime não foram vistos.

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Impacto](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

Esta ficha segue os contratos desta crate até seus consumers source-discovered:
implementação D1, handlers HTTP, composição nativa, fake, audit/SLI e testes.
O proprietário de D1/rotas é `corelink-container`; o boundary do Worker/deploy
exige owner e evidência próprios. Os 11 métodos, 11 denials, 9 ramos de mutação,
2 seams, 7 relações Cargo/test e 3 edges de reexport/rota estão em B03.

<a id="b02"></a>
## B02 — Método e população

Busca estática limitada ao manifesto e a
`crates/corelink-handler-customer/src/{lib,handler,request,audit_query,audit,observer,error}.rs`,
`src/request/{overview,usage,billing,keys,team}.rs` e
`tests/{handler_customer,mutation_kills}.rs` dessa crate. No container, o
recorte inclui `Cargo.toml`, `src/customer_d1*.rs`,
`src/routes/customer/{part-00,part-00-01,part-01}.rs`, `src/routes/build.rs`
e `src/main.rs`.

Consultas por `corelink_handler_customer`, `CustomerRouteState`,
`customer::router`, imports, `impl Customer*Handler`, traits, DTOs e chamadas
do fake compõem o censo semântico. Cada relação tem condição, efeito, falha e
limite. Cargo declara dependência, mas não estabelece chamada, artefato shipped
ou tráfego. Repetir a busca com escopo atualizado após drift de fonte ou
manifesto.

<a id="b03"></a>
## B03 — Relações diretas

### Métodos implementados pelo consumer

Cada REL-001–011 é uma fronteira de método do trait definido nesta crate até
`corelink-container::D1CustomerHandler` e os handlers HTTP declarados. Ativação
depende da implementação/chamada no target selecionado; alterar assinatura,
DTO ou erro propaga ao implementador/caller. O `pub use` em `lib.rs` é uma
relação local separada [REL-041](#rel-041), e a dependência Cargo reversa é
[REL-036](#rel-036). Nenhuma delas prova uma invocação runtime.

[Server REL-033](../corelink-server/BLAST_RADIUS.md#rel-033) é somente o peer
do pacote [REL-036](#rel-036), nunca peer dos 11 métodos. Cada método tem
registro source-only recíproco no server; o par permanece `UNAPPROVED` até
revisão bilateral. Owner do contrato: handler-customer; owner da implementação
D1 e tradução HTTP: container/server.

Fingerprint de cada linha: SHA-256 dos bytes UTF-8, sem newline, de
`customer-method-v1|<key>|<Trait::method>(<Request>)->Result<<Response>,CustomerHandlerError>|<H>|<I>|<R>`.
`H` é o blob Git `0eeb3f875e452fe4b96dfc42aa2dedcf6a542c3f` de
`src/handler.rs` no pin `cca798ff`; `I` é o blob da implementação D1 e `R`
o blob do call site HTTP no pin server `91630ba`. Em ambos os documentos,
`OU=61bab5af3938b1565d99e3f904d388870a7e79cb`,
`BK=1788eda7d562547a4941cbd01f425660a2164fe8`,
`TA=a4391bbab92219484ee3b6774ce83e1d61b67092` para
`customer_d1_{overview_usage,billing_keys,team_audit}.rs`; `R0=c7504c45c3f515789ba2e1d732f79084cb3ff8f1`
para `routes/customer/part-00-01.rs` e `R1=16874290a27a4b6f9a879da5475f28bada5052a4`
para `part-01.rs`. O hash vincula as fontes selecionadas, não certifica
compatibilidade semântica, compilação ou runtime.

| REL | Trait::método (`Request` → `Response`) | Chave compartilhada | I/R | SHA-256 | Peer server |
|---|---|---|---|---|---|
| 001 | `CustomerOverviewHandler::overview` (`OverviewRequest` → `OverviewResponse`) | `repo:1232040291:boundary:customer-overview-method-001` | OU/R0 | `f7decbc6c4b051f505d10a423105339af6c5051f2dd879cf1eabfbf7e7bf9886` | [REL-088](../corelink-server/BLAST_RADIUS.md#rel-088) |
| 002 | `CustomerUsageHandler::usage` (`UsageRequest` → `UsageResponse`) | `repo:1232040291:boundary:customer-usage-method-002` | OU/R0 | `d895b69605d9fcc8557071645988e16df7c6eb222dfbfeebd570983688bd0c46` | [REL-089](../corelink-server/BLAST_RADIUS.md#rel-089) |
| 003 | `CustomerBillingHandler::billing` (`BillingRequest` → `BillingResponse`) | `repo:1232040291:boundary:customer-billing-method-003` | BK/R0 | `12f741069abd0ac1b3789919b7ca7e3a64b0fb88e4f31938c0871442b85e03e5` | [REL-090](../corelink-server/BLAST_RADIUS.md#rel-090) |
| 004 | `CustomerBillingHandler::portal_url` (`PortalRequest` → `PortalResponse`) | `repo:1232040291:boundary:customer-portal-url-method-004` | BK/R0 | `a758a9144dab55af6f5426302106914419cd588ca770fb9f9e8e5c1470f72a94` | [REL-091](../corelink-server/BLAST_RADIUS.md#rel-091) |
| 005 | `CustomerKeysHandler::list` (`KeysListRequest` → `KeysListResponse`) | `repo:1232040291:boundary:customer-keys-list-method-005` | BK/R1 | `43ba2ddaa099737e8408d1b87608006c2392f95a6f5a985c3e38fac8ffe8576d` | [REL-092](../corelink-server/BLAST_RADIUS.md#rel-092) |
| 006 | `CustomerKeysHandler::create` (`KeyCreateRequest` → `KeyCreateResponse`) | `repo:1232040291:boundary:customer-keys-create-method-006` | BK/R1 | `ea610043f09869601737623d6151c06827b4ce55a8693f8efd933e2d446d2e04` | [REL-093](../corelink-server/BLAST_RADIUS.md#rel-093) |
| 007 | `CustomerKeysHandler::revoke` (`KeyRevokeRequest` → `KeyRevokeResponse`) | `repo:1232040291:boundary:customer-keys-revoke-method-007` | BK/R1 | `044e4000273446d0b90c04ce53ab0aea126faf541d666848372119edfbb67aab` | [REL-094](../corelink-server/BLAST_RADIUS.md#rel-094) |
| 008 | `CustomerTeamHandler::list` (`TeamListRequest` → `TeamListResponse`) | `repo:1232040291:boundary:customer-team-list-method-008` | TA/R1 | `6b8f22a711886744ce6ff33a4e4f12d7fb3ac0155a3caf6f71e27c98821cefbc` | [REL-095](../corelink-server/BLAST_RADIUS.md#rel-095) |
| 009 | `CustomerTeamHandler::invite` (`TeamInviteRequest` → `TeamInviteResponse`) | `repo:1232040291:boundary:customer-team-invite-method-009` | TA/R1 | `40cd1f9665256fbbbe07c19057de8b22b1464ef22721b5296509dd28dfcd44e6` | [REL-096](../corelink-server/BLAST_RADIUS.md#rel-096) |
| 010 | `CustomerTeamHandler::remove` (`TeamRemoveRequest` → `TeamRemoveResponse`) | `repo:1232040291:boundary:customer-team-remove-method-010` | TA/R1 | `4ae18421dea785cdd476501e209a56e8ec11a806817f0847fe33c76e805d3884` | [REL-097](../corelink-server/BLAST_RADIUS.md#rel-097) |
| 011 | `CustomerAuditHandler::query` (`AuditQueryRequest` → `AuditQueryResponse`) | `repo:1232040291:boundary:customer-audit-query-method-011` | TA/R0 | `b9e0fb685cc6d77cbe10dc1f25ec575334179d3065844dc3bc7d162b7229f23a` | [REL-098](../corelink-server/BLAST_RADIUS.md#rel-098) |

Direção de dependência:
container → esta crate; dados de request: container → trait implementation;
response/erro: implementation → container; impacto de mudança de contrato:
esta crate → implementador/rota, e falha de execução: implementação → caller.
As chaves e hashes são source-bound nas duas vistas, com aprovação bilateral pendente.

**Estado de aprovação bilateral: UNAPPROVED (REL-001–011).** Owner do contrato
tipado: `corelink-handler-customer`; owner da implementação D1, chamadas HTTP e
peer faltante: `corelink-container`/server. Para cada chave candidata acima,
endpoint provedor = trait/DTO desta crate e endpoint consumidor = implementação
`D1CustomerHandler` e chamada em `routes/customer/*`. O efeito local desta crate
é exigir revisão de assinatura, DTO e erro; no server, uma alteração pode quebrar
implementação, tradução HTTP ou seleção D1/fake.

As fontes confirmam endpoints e direções; os peers de método no manual server
documentam a vista local, mas não demonstram seleção de target, wire format
ou execução. O REL-033 server permanece somente o edge de package.

Relation index: [001](#rel-001) · [002](#rel-002) · [003](#rel-003) · [004](#rel-004) · [005](#rel-005) · [006](#rel-006) · [007](#rel-007) · [008](#rel-008) · [009](#rel-009) · [010](#rel-010) · [011](#rel-011).

<a id="rel-001"></a>
### REL-001 — Overview method
**Tipo/superfície:** implementação de método; `CustomerOverviewHandler::overview`, `OverviewRequest/Response` → consumer. **Efeito/falha:** snapshot; shape/erro altera chamada. **Evidência/validação:** `lib.rs`, `handler.rs:45-52`, `customer_d1_overview_usage.rs`; [API-001](REFERENCE.md#api-001). [Index](#b03)

<a id="rel-002"></a>
### REL-002 — Usage method
**Tipo/superfície:** implementação de método; `CustomerUsageHandler::usage`, `UsageRequest/Response` → consumer. **Efeito/falha:** filtro de período; shape/erro altera chamada. **Evidência/validação:** `lib.rs`, `handler.rs:62-69`, `customer_d1_overview_usage.rs`; [API-002](REFERENCE.md#api-002). [Index](#b03)

<a id="rel-003"></a>
### REL-003 — Billing snapshot method
**Tipo/superfície:** implementação de método; `CustomerBillingHandler::billing`, `BillingRequest/Response` → consumer. **Efeito/falha:** snapshot/faturas; shape/erro altera chamada. **Evidência/validação:** `lib.rs`, `handler.rs:79-86`, `customer_d1_billing_keys.rs`; [API-003](REFERENCE.md#api-003). [Index](#b03)

<a id="rel-004"></a>
### REL-004 — Portal URL method
**Tipo/superfície:** implementação de método; `CustomerBillingHandler::portal_url`, `PortalRequest/Response` → consumer. **Efeito/falha:** URL-shaped result; shape/erro altera chamada independentemente do snapshot. **Evidência/validação:** `lib.rs`, `handler.rs:88-94`, `customer_d1_billing_keys.rs`; [API-004](REFERENCE.md#api-004). [Index](#b03)

<a id="rel-005"></a>
### REL-005 — Keys list method
**Tipo/superfície:** implementação de método; `CustomerKeysHandler::list`, `KeysListRequest/Response` → consumer. **Efeito/falha:** lista PAT/BYOK; shape/erro altera leitura. **Evidência/validação:** `lib.rs`, `handler.rs:107-114`, `customer_d1_billing_keys.rs`; [API-005](REFERENCE.md#api-005). [Index](#b03)

<a id="rel-006"></a>
### REL-006 — Key create method
**Tipo/superfície:** implementação de método; `CustomerKeysHandler::create`, `KeyCreateRequest/Response` → consumer. **Efeito/falha:** criação/token-shaped result; shape/erro altera mutation caller. **Evidência/validação:** `lib.rs`, `handler.rs:116-123`, `customer_d1_billing_keys.rs`; [API-006](REFERENCE.md#api-006). [Index](#b03)

<a id="rel-007"></a>
### REL-007 — Key revoke method
**Tipo/superfície:** implementação de método; `CustomerKeysHandler::revoke`, `KeyRevokeRequest/Response` → consumer. **Efeito/falha:** revogação idempotente; shape/erro altera mutation caller. **Evidência/validação:** `lib.rs`, `handler.rs:125-132`, `customer_d1_billing_keys.rs`; [API-007](REFERENCE.md#api-007). [Index](#b03)

<a id="rel-008"></a>
### REL-008 — Team list method
**Tipo/superfície:** implementação de método; `CustomerTeamHandler::list`, `TeamListRequest/Response` → consumer. **Efeito/falha:** lista membros; shape/erro altera leitura. **Evidência/validação:** `lib.rs`, `handler.rs:142-149`, `customer_d1_team_audit.rs`; [API-008](REFERENCE.md#api-008). [Index](#b03)

<a id="rel-009"></a>
### REL-009 — Team invite method
**Tipo/superfície:** implementação de método; `CustomerTeamHandler::invite`, `TeamInviteRequest/Response` → consumer. **Efeito/falha:** convite/token; shape/erro altera mutation caller. **Evidência/validação:** `lib.rs`, `handler.rs:151-158`, `customer_d1_team_audit.rs`; [API-009](REFERENCE.md#api-009). [Index](#b03)

<a id="rel-010"></a>
### REL-010 — Team remove method
**Tipo/superfície:** implementação de método; `CustomerTeamHandler::remove`, `TeamRemoveRequest/Response` → consumer. **Efeito/falha:** remover seat + PATs é obrigação do trait; shape/erro altera mutation caller. **Evidência/validação:** `lib.rs`, `handler.rs:160-181`, `customer_d1_team_audit.rs`; [API-010](REFERENCE.md#api-010). [Index](#b03)

<a id="rel-011"></a>
### REL-011 — Audit query method
**Tipo/superfície:** implementação de método; `CustomerAuditHandler::query`, `AuditQueryRequest/Response` → consumer. **Efeito/falha:** filtros/rows; shape/erro altera query caller. **Evidência/validação:** `lib.rs`, `handler.rs:184-191`, `customer_d1_team_audit.rs`; [API-011](REFERENCE.md#api-011). [Index](#b03)

### Request tenant para guard e audit local

Em cada método, `requested_tenant` diferente de `caller_tenant` ativa `reject_cross_tenant`: `AuditSink::emit` recebe o `*Denied` do grupo e o tenant solicitado; sucesso do sink produz `CrossTenantDenied`, falha produz `AuditFailed`. O guard precede lookup/mutação. O contrato é local ao fake; identidade real, persistência e transporte não foram verificados. Para cada relação, alterar recurso, kind, seleção de tenant ou erro afeta uma saída distinta; validar branch e evento com owner do handler/adapter.

Relation index: [012](#rel-012) · [013](#rel-013) · [014](#rel-014) · [015](#rel-015) · [016](#rel-016) · [017](#rel-017) · [018](#rel-018) · [019](#rel-019) · [020](#rel-020) · [021](#rel-021) · [022](#rel-022).

<a id="rel-012"></a>
### REL-012 — Overview denial
**Produtor/consumidor:** `OverviewRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `OverviewDenied`, recurso vazio. **Falha/validação:** sink failure troca typed denial por `AuditFailed`; `handler.rs:427-438`; [API-001](REFERENCE.md#api-001). [Index](#b03)

<a id="rel-013"></a>
### REL-013 — Usage denial
**Produtor/consumidor:** `UsageRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `UsageDenied`, recurso vazio; período não é consultado. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:471-482`; [API-002](REFERENCE.md#api-002). [Index](#b03)

<a id="rel-014"></a>
### REL-014 — Billing snapshot denial
**Produtor/consumidor:** `BillingRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `BillingDenied`, recurso vazio. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:521-532`; [API-003](REFERENCE.md#api-003). [Index](#b03)

<a id="rel-015"></a>
### REL-015 — Portal denial
**Produtor/consumidor:** `PortalRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `BillingDenied`, recurso `portal`; URL não é gerada. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:564-575`; [API-004](REFERENCE.md#api-004). [Index](#b03)

<a id="rel-016"></a>
### REL-016 — Keys list denial
**Produtor/consumidor:** `KeysListRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `KeysDenied`, recurso vazio; PATs não são lidos. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:603-614`; [API-005](REFERENCE.md#api-005). [Index](#b03)

<a id="rel-017"></a>
### REL-017 — Key create denial
**Produtor/consumidor:** `KeyCreateRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `KeysDenied`, recurso `name`; PAT/token não são criados. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:630-641`; [API-006](REFERENCE.md#api-006). [Index](#b03)

<a id="rel-018"></a>
### REL-018 — Key revoke denial
**Produtor/consumidor:** `KeyRevokeRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `KeysDenied`, recurso `pat_id`; row não é consultada. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:684-695`; [API-007](REFERENCE.md#api-007). [Index](#b03)

<a id="rel-019"></a>
### REL-019 — Team list denial
**Produtor/consumidor:** `TeamListRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `TeamDenied`, recurso vazio; membros não são lidos. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:746-757`; [API-008](REFERENCE.md#api-008). [Index](#b03)

<a id="rel-020"></a>
### REL-020 — Team invite denial
**Produtor/consumidor:** `TeamInviteRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `TeamDenied`, recurso `email`; role não é validada. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:768-779`; [API-009](REFERENCE.md#api-009). [Index](#b03)

<a id="rel-021"></a>
### REL-021 — Team remove denial
**Produtor/consumidor:** `TeamRemoveRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `TeamDenied`, recurso `target_user_id`; seat não é lido. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:835-846`; [API-010](REFERENCE.md#api-010). [Index](#b03)

<a id="rel-022"></a>
### REL-022 — Audit query denial
**Produtor/consumidor:** `AuditQueryRequest` → guard → `AuditSink`/caller. **Ativação/efeito:** tenant divergente; `AuditQueryDenied`, recurso vazio; rows não são lidas. **Falha/validação:** sink failure → `AuditFailed`; `handler.rs:896-907`; [API-011](REFERENCE.md#api-011). [Index](#b03)

### Mutações do fake para audit/error

Estas relações ligam request → audit sink → `Mutex<HashMap>` do fake → resposta/erro. A fronteira é in-memory; identidade, segredo, durabilidade e entrega externa dependem dos owners de chave, time e container. Alterar ordem ou erro propaga para tests e consumer; validar ramos de `handler.rs` e fixtures sem inferir runtime.

Relation index: [023](#rel-023) · [024](#rel-024) · [025](#rel-025) · [026](#rel-026) · [027](#rel-027) · [028](#rel-028) · [029](#rel-029) · [030](#rel-030) · [031](#rel-031).

<a id="rel-023"></a>
### REL-023 — PAT create
**Ativação/efeito:** create próprio; `KeyCreateAttempted` → PAT row → token map → `KeyCreateCommitted`. **Falha:** sink/lock pode falhar antes ou depois de inserção parcial; não há rollback. **Evidência:** `handler.rs:630-683`, [API-006](REFERENCE.md#api-006). [Index](#b03)

<a id="rel-024"></a>
### REL-024 — Revoke de PAT ausente
**Ativação/efeito:** `pat_id` não pertence ao caller; `KeyRevokeAttempted` → lookup → `NotFound`; nenhum Committed/mutação. **Falha:** sink/lock pode retornar antes de NotFound. **Evidência:** `handler.rs:684-714`, [API-007](REFERENCE.md#api-007). [Index](#b03)

<a id="rel-025"></a>
### REL-025 — Revoke já concluído
**Ativação/efeito:** PAT com `revoked_at`; Attempted → row existente → Committed; no-op idempotente. **Falha:** Committed pode dar `AuditFailed` mesmo sem nova mutação. **Evidência:** `handler.rs:715-744`, [API-007](REFERENCE.md#api-007). [Index](#b03)

<a id="rel-026"></a>
### REL-026 — Revoke de PAT ativo
**Ativação/efeito:** PAT sem `revoked_at`; Attempted → insere row revogada → Committed. **Falha:** Committed pode dar `AuditFailed` após alteração local; sem rollback. **Evidência:** `handler.rs:715-744`, [API-007](REFERENCE.md#api-007). [Index](#b03)

<a id="rel-027"></a>
### REL-027 — Invite com role inválida
**Ativação/efeito:** `canonical_invite_role` rejeita role; `InvalidRequest` antes de Attempted, sem inserção. **Falha:** não há evento de tentativa para esse ramo; mudança de parser altera rejeição. **Evidência:** `handler.rs:768-787`, `request/team.rs`, [API-009](REFERENCE.md#api-009). [Index](#b03)

<a id="rel-028"></a>
### REL-028 — Invite aceito
**Ativação/efeito:** role válida; `TeamInviteAttempted` → membro inserido → `TeamInviteCommitted` → token 64 hex. **Falha:** audit após inserção pode dar `AuditFailed`; token ainda não é entrega. **Evidência:** `handler.rs:788-833`, [API-009](REFERENCE.md#api-009). [Index](#b03)

<a id="rel-029"></a>
### REL-029 — Remove de membro ausente
**Ativação/efeito:** target sem row; `TeamRemoveAttempted` → lookup → `NotFound`; sem Committed. **Falha:** sink pode dar `AuditFailed` antes do lookup. **Evidência:** `handler.rs:834-865`, [API-010](REFERENCE.md#api-010). [Index](#b03)

<a id="rel-030"></a>
### REL-030 — Remove de owner
**Ativação/efeito:** row role `owner`; Attempted → `Unauthorized`; sem alteração nem Committed. **Falha:** sink pode falhar antes da proteção. **Evidência:** `handler.rs:866-874`, [API-010](REFERENCE.md#api-010). [Index](#b03)

<a id="rel-031"></a>
### REL-031 — Remove de membro permitido
**Ativação/efeito:** row não-owner; Attempted → status `removed` → Committed → resposta com zero PATs revogados. **Falha:** Committed pode falhar após tombstone; fake não liga PAT a principal. **Contrato:** trait exige revogar PATs do membro, logo esta fixture não prova tal efeito. **Evidência:** `handler.rs:875-894`, [API-010](REFERENCE.md#api-010). [Index](#b03)

### Saída do handler para observação SLI

Relation index: [039](#rel-039) · [040](#rel-040).

<a id="rel-039"></a>
### REL-039 — Observação local SLI
**Produtor/consumidor:** cada retorno do fake → `SliObserver::observe(SliObservation)`; `Sli::AvailControlPlane` vem do reexport de `corelink-slo::definition::Sli`. **Ativação:** chamada de qualquer dos 11 métodos; erro/negação muda `is_error`. **Efeito/falha:** interface infalível; observer em memória descarta linha quando lock envenenado, logo ausência de erro não prova captura. **Limite/validação:** sem prova de agregador, transporte ou alerta; seguir `handler.rs:359-368,426-939`, `observer.rs:10-103`, [API-013](REFERENCE.md#api-013), [API-016](REFERENCE.md#api-016). Peer da dependência: [corelink-slo REL-016](../corelink-slo/BLAST_RADIUS.md#rel-016). [Index](#b03)

<a id="rel-040"></a>
### REL-040 — Emissão audit local
**Produtor/consumidor:** métodos do fake → `AuditSink::emit(AuditEvent)`; `AuditEventKind::slug` fixa taxonomia. **Ativação:** Attempted/Served/Denied/Committed conforme o método e ramo REL-012–031. **Efeito/falha:** `Err(String)` vira `AuditFailed`; a falha em Committed pode seguir mutação, sem compensação automática. **Limite/validação:** sink injetado não prova persistência; conferir `handler.rs:391-409`, `audit.rs:19-260`, [API-012](REFERENCE.md#api-012)–[API-015](REFERENCE.md#api-015). [Index](#b03)

### DTOs, erros e compatibilidade

Módulos request/audit-query → structs `#[non_exhaustive]` → traits/reexports. Campos de
overview/usage/billing, rows de audit, PAT, equipe e filtros podem afetar serializadores
ou callers que os usem. `CustomerHandlerError` e `AuditEventKind` fazem parte da mesma
superfície. Não foram estabelecidos formato HTTP/JSON efetivo, dados históricos,
consumidores gerados ou compatibilidade com versões anteriores.

### Consumers conhecidos

Relation index: [032](#rel-032) · [033](#rel-033) · [034](#rel-034) · [035](#rel-035) · [036](#rel-036) · [037](#rel-037) · [038](#rel-038) · [041](#rel-041) · [042](#rel-042) · [043](#rel-043).

População Cargo/test: 3 dependências normais (`corelink-slo`, `uuid`, `thiserror`),
1 dev-dependency repetida (`corelink-slo`), 1 consumer reverso por manifesto
(`corelink-server`) e 2 arquivos de integration test locais. O censo semântico
também inclui 11 implementações/invocações de método em D1/HTTP, a seleção
D1-vs-fake em `CustomerRouteState`, a declaração de rotas em
`routes/customer/part-00.rs:255-279`, e a instalação de native PAT gate seguida
do merge em `routes/build.rs:329-390`. Nenhum outro consumer Cargo foi encontrado
na busca estática deste checkout; externos e target/deploy não foram certificados.

<a id="rel-032"></a>
### REL-032 — Dependência `corelink-slo`
**Produtor/consumidor:** `corelink-slo::definition::Sli` → observer e fake deste package. **Ativação:** resolução normal; manifesto também a declara em dev. **Falha:** mudança de enum/import quebra compilação ou classificação SLI. **Limite/validação:** nenhum agregador ou alerta provado; manifesto, `observer.rs`, [corelink-slo REL-016](../corelink-slo/BLAST_RADIUS.md#rel-016). [Index](#b03)

**Identidade proposta:** `repo:1232040291:boundary:customer-slo-sli-observer-001`; fingerprint local `sha256:7b9fefb4857ebce6ea5da53e7aadfc850e3b516d7868fe8d19e0f117176ed672`.
SHA-256 de `customer-sli-v1|<key>|corelink-slo::definition::Sli->corelink-handler-customer::observer::Sli|<handler-manifest>|<observer>|<slo-manifest>|<definition>` (UTF-8, sem newline). Blobs Git, nesta ordem: handler `cca798ff` `4aafb7ca54f8bc586a7a846c28a1ba35d4cf3097`, `5c1c7b1d849050550a5e15db62b8ea8433f849e6`; slo `6ed297f` `cd12aa05323b31644e3b2efe6231bc49825e761d`, `d2856258e269c38847b0cb3b030053bb0630f045`.
Owner `corelink-slo`; dependência handler→slo; dados `Sli` slo→handler; mudança slo→imports/observações. [Slo REL-016](../corelink-slo/BLAST_RADIUS.md#rel-016) não registra esta chave/fingerprint: `UNAPPROVED`.

<a id="rel-033"></a>
### REL-033 — Dependência `uuid`
**Produtor/consumidor:** `uuid::Uuid::new_v4().as_simple()` → token de convite do fake. **Ativação:** `CustomerTeamHandler::invite` aceito. **Falha:** formato/entropia alterado muda token local de 64 hex (duas UUIDs). **Limite/validação:** não prova entrega nem segredo de produção; manifesto, `handler.rs:812-826`, [API-009](REFERENCE.md#api-009). [Index](#b03)

<a id="rel-034"></a>
### REL-034 — Dependência `thiserror`
**Produtor/consumidor:** derive `thiserror::Error` → `CustomerHandlerError` e callers. **Ativação:** build da biblioteca. **Falha:** mudança de macro/formatos afeta erro compilado ou mensagem; não é política HTTP. **Validação:** manifesto, `error.rs:3-49`, [R07](REFERENCE.md#r07). [Index](#b03)

<a id="rel-035"></a>
### REL-035 — Dev-dependency `corelink-slo`
**Produtor/consumidor:** manifesto test target → `corelink-slo`. **Ativação:** seleção de testes; chave repetida além da dependência normal. **Falha:** resolução de teste pode divergir da library. **Validação:** ambas as seções do manifesto; não inferir execução. [Index](#b03)

<a id="rel-036"></a>
### REL-036 — Consumer reverso `corelink-server`
**Superfície:** package handler-customer → manifesto, imports e D1 do server;
`/v1/customer/*` source-declared quando target server selecionado. **Owner:**
API `corelink-handler-customer`; consumer/rota `corelink-container`. **Direções:**
dependência server → handler-customer; request server → trait; response/erro →
server; mudança API → server. **Falha:** DTO/API pode quebrar D1 ou HTTP.

**Peer/identidade:** [server REL-033](../corelink-server/BLAST_RADIUS.md#rel-033), `repo:1232040291:boundary:customer-server-package-001`; `sha256:0e27a1885fba6732e784e8021f83955895660da221d30fcb81172d11a8aa13b2`.
SHA-256 de `customer-package-v1|<key>|<handler-manifest-blob>|<server-manifest-blob>` (UTF-8, sem newline); blobs Git dos manifests nos pins handler `cca798ff`/server `91630ba`: `4aafb7ca54f8bc586a7a846c28a1ba35d4cf3097`/`e9feace6365776c6a4141ff59efa70fe1b36e158`.
Estado `SOURCE-PAIRED / UNAPPROVED`; REL-001–011 são métodos separados. Artefato shipped/runtime `UNKNOWN`. Fontes: manifests, `customer_d1.rs`, `routes/customer/part-00.rs`. [Index](#b03)

<a id="rel-037"></a>
### REL-037 — Test target explícito
**Produtor/consumidor:** API e fake → `tests/handler_customer.rs`. **Ativação:** `cargo test -p corelink-handler-customer --test handler_customer`; nenhuma execução alegada. **Falha:** mudança em guard/audit/SLI/DTO altera asserts. **Validação:** manifesto e arquivo de teste; [M04](MAINTENANCE.md#m04). [Index](#b03)

<a id="rel-038"></a>
### REL-038 — Test target autodetectado
**Produtor/consumidor:** API e fake → `tests/mutation_kills.rs`. **Ativação:** `cargo test -p corelink-handler-customer --test mutation_kills`; nenhuma execução alegada. **Falha:** regressão de negação por endpoint altera assertions. **Validação:** arquivo de teste e convenção Cargo; [M04](MAINTENANCE.md#m04). [Index](#b03)

<a id="rel-041"></a>
### REL-041 — Reexport local
**Tipo/direções:** `reexport`; módulos `handler`, `request`, `audit`, `observer`, `error` → `lib.rs` → imports públicos; dependência local, dados runtime não aplicáveis, impacto de campo/assinatura para imports. **Ativação/falha:** seleção de biblioteca; remover alias quebra o caminho público mesmo se o módulo ainda existir. **Validação/limite:** `src/lib.rs:66-90`; não prova consumer externo ou chamada. [R04](REFERENCE.md#r04). [Index](#b03)

<a id="rel-042"></a>
### REL-042 — Seleção de implementação e router HTTP
**Tipo/direções:** `runtime-call` source-declared; `CustomerRouteState` recebe seis trait objects de D1 se `D1CustomerHandler::from_env()` retorna `Some`, caso contrário fake; os handlers HTTP passam requests aos traits e recebem respostas/erros. **Ativação/falha:** binário nativo executa o builder; variação de ambiente altera backend e semântica de dados. **Validação/limite:** `routes/customer/part-00.rs:139-199,255-279`, `part-00-01.rs`, `part-01.rs`; rota `implemented`, ainda sem artefato shipped/tráfego. Owner: container. [R03](REFERENCE.md#r03). [Index](#b03)

<a id="rel-043"></a>
### REL-043 — Composição nativa e PAT gate
**Tipo/direções:** `runtime-call` source-declared; `routes/build.rs` constrói `customer_state`, injeta `native_pat_gate` e mescla `customer::router(customer_state)` no router; `main.rs` chama o builder. **Ativação/falha:** seleção desse binário/caminho; mudança no gate ou merge altera acesso ao customer plane. **Validação/limite:** `routes/build.rs:329-390`, `main.rs:389`; `wired` na fonte nativa, sem verificação de Worker, deploy ou request real. Owner: container. [R03](REFERENCE.md#r03). [Index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino alcançável | Caminho testemunha | Condição e efeito | Contenção e validação |
|---|---|---|---|
| Implementação D1 | REL-036 → REL-041 → REL-001–011 | Target nativo selecionado; contrato alterado exige implementação compatível | Owner container; comparar signatures em `customer_d1_*.rs`; sem runtime claim |
| Handlers HTTP/customer router | REL-001–011 → REL-042 | Builder selecionado; DTO/erro chega ao handler/response | Revisar `part-00-01.rs` e `part-01.rs`; wire format ainda não certificado |
| Router nativo com PAT gate | REL-042 → REL-043 | `build_with_factory` chamado; route merge e gate são source-wired | Conferir builder e `main.rs`; shipped artifact/Worker fora do limite |
| Fixtures/testes locais | REL-012–031, REL-039–040 → REL-037–038 | Test target selecionado; ordem de audit/guard/mutação muda asserts | Gates exatos em [M04](MAINTENANCE.md#m04); execução ainda não atestada |
| SLI externo | REL-032 → REL-039 | Somente enum reexportado e seam tipado/fake visíveis | Parar em trait injetado; entrega requer owner externo |
| Audit externo | REL-040 | Somente seam tipado/fake visível | Parar em `AuditSink`; armazenamento requer owner externo |

<a id="b05"></a>
## B05 — Mudança, impacto e validação

| Mudança | Impacto direto/transitivo | Gate e coordenação |
|---|---|---|
| Trait, DTO, erro ou reexport | REL-001–011/041 → D1/HTTP via REL-042/043 | [PROC-001](MAINTENANCE.md#proc-001), targets Rust em [M04](MAINTENANCE.md#m04), owner do container, compatibilidade N/N-1 em [M05](MAINTENANCE.md#m05) |
| Tenant guard ou audit kind | REL-012–022/040 → fake e testes | [PROC-002](MAINTENANCE.md#proc-002); não inferir identidade real ou audit persistente |
| PAT/team mutation | REL-023–031 → estado parcial, audit e testes | [PROC-002](MAINTENANCE.md#proc-002), recuperação [M05](MAINTENANCE.md#m05), owner de D1 para backend real |
| SLI/dependência | REL-032/035/039 → observer/testes | [M04](MAINTENANCE.md#m04); parar no seam, sem afirmação de métrica entregue |
| Factory, gate ou merge de rota | REL-042/043 → router nativo | Owner container; rever seleção e source wiring, pedir evidência de artefato/runtime separada |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Nesta população limitada: 43 relações descobertas = 43 documentadas (REL-001–043)
+ 0 excluídas. Desdobramento: 11 métodos, 11 denials, 9 ramos mutacionais,
2 seams audit/SLI, 7 relações Cargo/test e 3 reexport/rota. Candidatos
inspecionados e excluídos por ausência no manifesto/fonte desta crate: bin,
build-dependency, feature, FFI e schema local; eles não entram nas 43 relações
descobertas. Os cinco módulos `src/request/*` definem os DTOs dos métodos já
contados; `part-00-01.rs` contém overview, usage, audit-query, billing e
portal, e `part-01.rs` contém keys list/create/revoke e team
list/invite/remove: 11 calls cobertas por REL-001–011, sem nova relação
atômica encontrada nesse recorte. A busca não certifica outros arquivos do
router, consumers externos ou comportamento de execução.

Desconhecidos materiais fora da população source: consumer externo,
artefato shipped, forwarding Worker, configuração implantada, tráfego e efeitos
persistentes.

Os 11 peers de método REL-001–011 e o package peer REL-036
agora têm chaves e fingerprints source-bound recíprocos no server, mas seguem
`UNAPPROVED` até revisão bilateral; REL-032 tem só identidade/fingerprint
local proposto e aguarda atualização do peer slo REL-016. A igualdade não
demonstra completude do universo externo nem aprovação dessas 13 relações.

Reexecutar a descoberta após drift; ausência de relação estática não prova
ausência de integração externa.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
