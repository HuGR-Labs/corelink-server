---
schema: corelink-ownership/1.1
document: reference
package: corelink-handler-customer
manifest: crates/corelink-handler-customer/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-customer-structural-normalization-20260921
---

# corelink-handler-customer — referência de ownership

Referência de manifesto e fonte estática na baseline fixada. Descreve contratos
declarados e o fake local; não certifica dados de cliente, identidade, rotas,
backends de billing/chaves/audit, Worker CF ou operação real.

[Identidade](#r01) · [Fronteiras](#r02) · [Superfície](#r03) · [Audit e SLI](#r04) · [Fake](#r05) · [Erros](#r06) · [Evidência](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade

| Campo | Fonte estática |
|---|---|
| Package | `corelink-handler-customer`; `crates/corelink-handler-customer/Cargo.toml` |
| Papel | Superfície tipada de seis grupos de dashboard, DTOs, observação SLI e envelope local de auditoria |
| Dependências declaradas | `corelink-slo`, `thiserror`, `uuid`; `corelink-slo` também como dev-dependency |
| Target declarado | Biblioteca e integration tests; não há bin, feature ou build-dependency no manifesto |

<a id="r02"></a>
## R02 — Fronteiras

| Área | Nesta crate | Fora desta crate |
|---|---|---|
| Dashboard | Traits e DTOs para overview, usage, billing, keys, team e audit query | Mount HTTP, request real e composição de container |
| Identidade/tenant | Campos de caller/tenant e guard do fake | Autenticação, sessão, identidade e autorização reais |
| Billing e keys | Shapes e mutações em memória | Stripe, secret/PAT real, backend de billing ou chaves |
| Audit/SLO | Traits, eventos/observações e fakes locais | Persistência, entrega, alerting, métrica ou backend real |

<a id="r03"></a>
## R03 — Mapa de implementação

O proprietário desta crate é a superfície tipada e o fake em
`src/{lib,handler,request,audit_query,audit,observer,error}.rs`; o fake só mantém
estado em memória. `crates/corelink-container/src/customer_d1.rs` e seus módulos
`customer_d1_*` implementam os traits sobre uma fronteira D1, de propriedade do
container.

`routes/customer/part-00.rs:255-279` declara o router HTTP e seus
handlers; `routes/customer/part-00.rs` constrói `CustomerRouteState` por ambiente,
selecionando D1 quando configurado e fallback in-memory conforme fonte.
`routes/build.rs:329-390` constrói o estado, instala `native_pat_gate` e faz
`.merge(customer::router(customer_state))`; `src/main.rs:389` chama esse builder.
Assim, a rota nativa está `implemented` e `wired` na fonte sob seleção desse
binário/caminho de build. Artefato shipped, configuração implantada, forwarding
do Worker e tráfego permanecem sem verificação (`runtime_verified = UNKNOWN`).

`lib.rs` reexporta seis traits: `CustomerOverviewHandler`,
`CustomerUsageHandler`, `CustomerBillingHandler`, `CustomerKeysHandler`,
`CustomerTeamHandler` e `CustomerAuditHandler`. Seus métodos são, respectivamente,
`overview`, `usage`, `billing`/`portal_url`, `list`/`create`/`revoke`,
`list`/`invite`/`remove`, e `query`.

| Módulo | Shapes e contrato estático |
|---|---|
| `request/{overview,usage}.rs` | Snapshot overview; período, buckets diários e totais de uso |
| `request/billing.rs` | Snapshot/faturas e `PortalRequest`/`PortalResponse`; o URL do fake é stub determinístico |
| `request/keys.rs` | Lista, criação e revogação de `PatRow`; resposta de criação inclui token somente no shape local |
| `request/team.rs` | Listagem, convite e remoção; `canonical_invite_role` aceita somente `admin`, `member`, `viewer` |
| `audit_query.rs` | Filtros `since`/`event_types` e linhas de resposta |
| `request.rs` | Reexporta os cinco módulos e inclui `audit_query.rs` por `#[path]` |

Requests carregam `caller_tenant`, `principal`, instante e, em geral, um
`requested_tenant` opcional que defaulta ao caller. Isso define forma e comportamento
do fake, não prova um caller autenticado nem uma rota que aceite esses campos.

### Audit, SLI e ordem local

`AuditEventKind` declara slugs de overview/usage/billing, criação/revogação de
keys, convite/remoção de team e consulta de audit. `AuditSink::emit` retorna erro;
`InMemoryAuditSink` captura linhas ou injeta falha. `SliObserver::observe` recebe
`SliObservation`; `observer.rs` reexporta `corelink_slo::definition::Sli` e o fake
registra observações em memória.

As docstrings e os caminhos de `InMemoryCustomerHandler` declaram tentativa antes de lookup/mutação nos métodos que a emitem, negação cross-tenant auditada antes do erro, e observação `Sli::AvailControlPlane` por saída. As listas de keys e team não emitem um evento `Attempted`. Para `create`, `revoke`, `invite` e `remove`, a tentativa precede uma mudança de estado nos caminhos bem-sucedidos que mudam estado; `Committed` segue essa mudança e sua falha pode retornar `AuditFailed` depois dela.

Em `revoke`, PAT ausente emite `Attempted` e retorna `NotFound`, sem mutação ou `Committed`; PAT já revogado é no-op idempotente que emite `Attempted` e `Committed`. Isto não prova durabilidade do sink, atomicidade distribuída, entrega de telemetria ou semântica de qualquer adaptador.

<a id="r04"></a>
## R04 — Contratos públicos

Os IDs API abaixo pertencem à superfície desta crate. Implementações D1,
invocações HTTP e seleção de backend pertencem ao container; as relações de
reexport, dependência e método são distintas em [B03](BLAST_RADIUS.md#b03).

### Contratos públicos por endpoint

Cada assinatura abaixo é `fn método(&self, req: Request) -> Result<Response, CustomerHandlerError>` no trait indicado. Os requests trazem `caller_tenant`, `principal`, `at_unix_ms` e `requested_tenant` opcional; o guard do fake usa o tenant solicitado ou o caller. As saídas e efeitos descritos são locais ao fake, salvo obrigação expressa da docstring do trait.

| ID | Trait, método, Request → Response | Precondição, saída e efeito local | Erros, compatibilidade, invariante e relação |
|---|---|---|---|
| <a id="api-001"></a>API-001 | `CustomerOverviewHandler::overview`: `OverviewRequest` → `OverviewResponse` | Snapshot do caller após audit Attempted; Served antes da resposta. | `NotFound`/`AuditFailed`/guard; DTO e trait públicos. [INV-001](#inv-001), [REL-001](BLAST_RADIUS.md#rel-001), [REL-012](BLAST_RADIUS.md#rel-012); `handler.rs:45-52,426-469`. |
| <a id="api-002"></a>API-002 | `CustomerUsageHandler::usage`: `UsageRequest` → `UsageResponse` | Snapshot filtrado por `period`, se fornecido; Attempted/Served. | Período sem match dá `NotFound`; [INV-001](#inv-001), [REL-002](BLAST_RADIUS.md#rel-002), [REL-013](BLAST_RADIUS.md#rel-013); `handler.rs:62-69,470-519`. |
| <a id="api-003"></a>API-003 | `CustomerBillingHandler::billing`: `BillingRequest` → `BillingResponse` | Snapshot/faturas do caller; Attempted/Served. | Ausência dá `NotFound`; não prova backend Stripe. [INV-001](#inv-001), [REL-003](BLAST_RADIUS.md#rel-003), [REL-014](BLAST_RADIUS.md#rel-014); `handler.rs:79-86,520-562`. |
| <a id="api-004"></a>API-004 | `CustomerBillingHandler::portal_url`: `PortalRequest` → `PortalResponse` | Fake gera URL stub determinístico e usa audit BillingAttempted/Served com recurso `portal`. | URL não é sessão real; sink pode dar `AuditFailed`. [INV-001](#inv-001), [REL-004](BLAST_RADIUS.md#rel-004), [REL-015](BLAST_RADIUS.md#rel-015); `handler.rs:88-94,564-601`. |
| <a id="api-005"></a>API-005 | `CustomerKeysHandler::list`: `KeysListRequest` → `KeysListResponse` | Lista PATs do caller e BYOK status fake; não emite Attempted. | Lock/guard podem falhar; nenhum token é retornado. [INV-001](#inv-001), [REL-005](BLAST_RADIUS.md#rel-005), [REL-016](BLAST_RADIUS.md#rel-016); `handler.rs:107-114,602-629`. |
| <a id="api-006"></a>API-006 | `CustomerKeysHandler::create`: `KeyCreateRequest` → `KeyCreateResponse` | Insere PAT e token-shaped fake; Attempted antes, Committed depois. | Falha Committed pode seguir inserção; token fake não é segredo real. [INV-002](#inv-002), [REL-006](BLAST_RADIUS.md#rel-006), [REL-017](BLAST_RADIUS.md#rel-017), [REL-023](BLAST_RADIUS.md#rel-023); `handler.rs:116-123,630-683`. |
| <a id="api-007"></a>API-007 | `CustomerKeysHandler::revoke`: `KeyRevokeRequest` → `KeyRevokeResponse` | PAT do caller: ativo vira revogado; já revogado retorna linha existente. | Ausente dá `NotFound`; idempotência local. [INV-002](#inv-002), [REL-007](BLAST_RADIUS.md#rel-007), [REL-018](BLAST_RADIUS.md#rel-018), [REL-024](BLAST_RADIUS.md#rel-024)–[REL-026](BLAST_RADIUS.md#rel-026); `handler.rs:125-132,684-744`. |
| <a id="api-008"></a>API-008 | `CustomerTeamHandler::list`: `TeamListRequest` → `TeamListResponse` | Lista membros do caller; não emite Attempted. | Guard/lock; sem envio de convite. [INV-001](#inv-001), [REL-008](BLAST_RADIUS.md#rel-008), [REL-019](BLAST_RADIUS.md#rel-019); `handler.rs:142-149,745-767`. |
| <a id="api-009"></a>API-009 | `CustomerTeamHandler::invite`: `TeamInviteRequest` → `TeamInviteResponse` | Role `admin/member/viewer`; insere membro, retorna token de convite local de 64 hex. | Role inválida antes de audit; Committed pode falhar após inserção. [INV-002](#inv-002), [REL-009](BLAST_RADIUS.md#rel-009), [REL-020](BLAST_RADIUS.md#rel-020), [REL-027](BLAST_RADIUS.md#rel-027)–[REL-028](BLAST_RADIUS.md#rel-028); `handler.rs:151-158,768-833`. |
| <a id="api-010"></a>API-010 | `CustomerTeamHandler::remove`: `TeamRemoveRequest` → `TeamRemoveResponse` | Trait exige remoção e revogação dos PATs do membro; fake marca `removed` e retorna zero revogações. | Ausente dá `NotFound`, owner dá `Unauthorized`; fake não satisfaz prova de revogação. [INV-002](#inv-002), [REL-010](BLAST_RADIUS.md#rel-010), [REL-021](BLAST_RADIUS.md#rel-021), [REL-029](BLAST_RADIUS.md#rel-029)–[REL-031](BLAST_RADIUS.md#rel-031); `handler.rs:160-181,834-894`. |
| <a id="api-011"></a>API-011 | `CustomerAuditHandler::query`: `AuditQueryRequest` → `AuditQueryResponse` | Filtra linhas locais por `event_types`; `since` não é aplicado pelo fake; Attempted/Served. | Sem garantia de persistência/retenção. [INV-001](#inv-001), [REL-011](BLAST_RADIUS.md#rel-011), [REL-022](BLAST_RADIUS.md#rel-022); `handler.rs:184-191,895-939`. |

### Contratos auxiliares exportados

| ID | Símbolos e entradas | Saída, efeito, erro e compatibilidade | Relação e evidência |
|---|---|---|---|
| <a id="api-012"></a>API-012 | `AuditSink::emit(&self, AuditEvent) -> Result<(), String>`; evento inclui `at_unix_ms: u64` em milissegundos Unix | `Ok(())` aceita uma linha conforme o sink; `Err(String)` vira `AuditFailed` no fake. O trait não certifica persistência. Mudança de assinatura afeta implementadores. | [INV-002](#inv-002), [REL-040](BLAST_RADIUS.md#rel-040); `audit.rs:138-186`, `handler.rs:391-409`. |
| <a id="api-013"></a>API-013 | `SliObserver::observe(&self, SliObservation)`; `latency_us: u64` em microssegundos | Interface sem retorno de erro; fake descarta observação se o lock estiver envenenado. Não prova entrega ao agregador. | [REL-039](BLAST_RADIUS.md#rel-039); `observer.rs:18-52,94-103`. |
| <a id="api-014"></a>API-014 | `AuditEventKind::slug(self) -> &'static str` | Mapeia cada variante ao slug pontuado estático; mudar slug altera taxonomia de eventos. | [REL-040](BLAST_RADIUS.md#rel-040); `audit.rs:19-128`. |
| <a id="api-015"></a>API-015 | `AuditEvent::new(kind, tenant, principal, resource, at_unix_ms: u64) -> Self` | Constrói envelope local sem validação de identidade/tempo; campos e taxonomia são `#[non_exhaustive]`. | [REL-040](BLAST_RADIUS.md#rel-040); `audit.rs:138-174`. |
| <a id="api-016"></a>API-016 | `SliObservation::new(sli, is_error, latency_us: u64) -> Self` | Constrói observação tipada, sem emissão; mudança de unidade/enum atinge observers. | [REL-039](BLAST_RADIUS.md#rel-039); `observer.rs:18-44`. |
| <a id="api-017"></a>API-017 | `InMemoryAuditSink::{new,snapshot}` | Cria sink vazio; `snapshot` clona linhas em ordem de emissão ou retorna erro de lock; efeito somente local. | [REL-040](BLAST_RADIUS.md#rel-040); `audit.rs:192-217`. |
| <a id="api-018"></a>API-018 | `InMemoryAuditSink::{inject_failure,clear_failure}`; mensagem string | Controla falha futura de `emit`; cada método pode retornar erro de lock. Útil para fixture, não para compensar audit persistente. | [REL-040](BLAST_RADIUS.md#rel-040); `audit.rs:219-248`. |
| <a id="api-019"></a>API-019 | `InMemorySliObserver::{new,snapshot,count}`; `count` recebe `Sli` | Cria observer vazio; snapshot/count leem linhas locais ou retornam erro de lock. `observe` pode descartar em lock envenenado. | [REL-039](BLAST_RADIUS.md#rel-039); `observer.rs:58-103`. |
| <a id="api-020"></a>API-020 | `InMemoryCustomerHandler::new(Arc<dyn AuditSink>, Arc<dyn SliObserver>) -> Self` | Injeta colaboradores e cria mapas vazios; não escolhe sink/observer de produção. | [REL-039](BLAST_RADIUS.md#rel-039), [REL-040](BLAST_RADIUS.md#rel-040); `handler.rs:202-248`. |
| <a id="api-021"></a>API-021 | `InMemoryCustomerHandler::{seed_overview,seed_usage,seed_billing,seed_pat,seed_member,seed_audit_rows}` | Fixtures inserem estado local por tenant; cada uma retorna `Result<(), CustomerHandlerError>` e pode falhar com `Internal` por lock. Não passam pelo audit de mutação. | [REL-037](BLAST_RADIUS.md#rel-037), [REL-038](BLAST_RADIUS.md#rel-038); `handler.rs:253-357`. |
| <a id="api-022"></a>API-022 | `canonical_invite_role(&str) -> Option<&'static str>` | Trim/lowercase aceita somente `admin`, `member`, `viewer`; outros valores dão `None` antes do audit no fake. | [REL-027](BLAST_RADIUS.md#rel-027); `request/team.rs:6-15`. |
| <a id="api-023"></a>API-023 | `KeyRevokeRequest::with_role(..., caller_role, ..., at_unix_ms: u64) -> Self`; `KeyRevokeResponse::with_cache_token_id(token_id) -> Self` | Transporta papel confiado ao server e handle não secreto de invalidação de cache; fake não preenche o handle. Não valida papel aqui. | [REL-007](BLAST_RADIUS.md#rel-007), [REL-036](BLAST_RADIUS.md#rel-036); `request/keys.rs:220-235,266-281`. |
| <a id="api-024"></a>API-024 | `TeamInviteResponse::{new,with_token}`; membro e token opcional | `new` omite token; `with_token` inclui capability uma vez na resposta, sem persistência nesta crate. Mudança de shape afeta caller. | [REL-009](BLAST_RADIUS.md#rel-009); `request/team.rs:166-194`. |

Taxonomia completa de API-014, conforme o `match` de `audit.rs:104-128`:

| Variante | Slug | Variante | Slug |
|---|---|---|---|
| `OverviewAttempted` | `corelink.customer.overview.attempted` | `OverviewServed` | `corelink.customer.overview.served` |
| `OverviewDenied` | `corelink.customer.overview.denied` | `UsageAttempted` | `corelink.customer.usage.attempted` |
| `UsageServed` | `corelink.customer.usage.served` | `UsageDenied` | `corelink.customer.usage.denied` |
| `BillingAttempted` | `corelink.customer.billing.attempted` | `BillingServed` | `corelink.customer.billing.served` |
| `BillingDenied` | `corelink.customer.billing.denied` | `KeyCreateAttempted` | `corelink.customer.keys.create.attempted` |
| `KeyCreateCommitted` | `corelink.customer.keys.create.committed` | `KeyRevokeAttempted` | `corelink.customer.keys.revoke.attempted` |
| `KeyRevokeCommitted` | `corelink.customer.keys.revoke.committed` | `KeysDenied` | `corelink.customer.keys.denied` |
| `TeamInviteAttempted` | `corelink.customer.team.invite.attempted` | `TeamInviteCommitted` | `corelink.customer.team.invite.committed` |
| `TeamDenied` | `corelink.customer.team.denied` | `TeamRemoveAttempted` | `corelink.customer.team.remove.attempted` |
| `TeamRemoveCommitted` | `corelink.customer.team.remove.committed` | `AuditQueryAttempted` | `corelink.customer.audit.query.attempted` |
| `AuditQueryServed` | `corelink.customer.audit.query.served` | `AuditQueryDenied` | `corelink.customer.audit.query.denied` |

Os demais construtores `new` e builders `for_tenant`/`with_requested_tenant` em `request/*` apenas montam os DTOs dos endpoints API-001–011; estão cobertos pelo shape de cada método e não têm efeito externo próprio. Mudança em campos/default de tenant exige reabrir o contrato do método. Traits injetados não estabelecem sinks de produção.

<a id="r05"></a>
## R05 — Fake em memória e predicados observáveis na fonte

`InMemoryCustomerHandler` mantém `HashMap`s protegidos por `Mutex` para snapshots, PATs/tokens, membros e linhas de audit, e recebe `Arc<dyn AuditSink>` e `Arc<dyn SliObserver>`. Os métodos `seed_*` são fixtures locais. O guard compara tenant solicitado com caller; erro no audit de negação retorna `AuditFailed`. Revogação de PAT é idempotente no fake. A remoção marca o membro como removido; o fake não mantém vínculo de PAT por principal e devolve zero revogações. Esses são predicados de fonte do fake, não afirmações sobre dados externos.

| Estado possuído pelo fake | Chave/partição exata | Vida, mutação e durabilidade |
|---|---|---|
| `overviews`, `usages`, `billings` | Cada mapa: `tenant_id: String →` seu `*Response` | Vazio em `new`; `seed_overview`, `seed_usage`, `seed_billing` substituem o snapshot da chave; leitura por caller tenant. Somente memória da instância. |
| `pats` | `(tenant_id: String, pat_id: String) → PatRow` | `seed_pat` insere; `create` insere após Attempted e `revoke` substitui a row ativa antes de Committed. `list` filtra pelo tenant. Memória da instância. |
| `pat_tokens` | `pat_id: String → token: String`, **sem tenant na chave** | `create` insere após a row em `pats`, sob outro lock; falha nesse lock deixa a row sem token inserido. Não há API pública de leitura/snapshot deste mapa; `seed_pat` não o preenche. Memória da instância. |
| `team` | `(tenant_id: String, user_id: String) → TeamMemberRow` | `seed_member` insere; `invite` insere após Attempted; `remove` substitui status por `removed` antes de Committed. Memória da instância. |
| `audit_store` | `tenant_id: String → Vec<CustomerAuditEventRow>` | Apenas `seed_audit_rows` substitui o vetor; `query` lê e filtra `event_types`, sem aplicar `since`. É distinto do `AuditSink` injetado e não recebe automaticamente os eventos emitidos. Memória da instância. |

Cada mapa tem seu próprio `Mutex`; não há transação entre locks, persistência ou
reconstrução após descarte da instância. O `AuditSink` e o `SliObserver` são
colaboradores `Arc` injetados, com vida/armazenamento próprios desconhecidos neste
contrato. Fonte: `handler.rs:202-247,253-357,602-939`.

Record index: [INV-001](#inv-001) · [INV-002](#inv-002).

<a id="inv-001"></a>
### INV-001 — Cross-tenant requests are denied before handler result
**Predicate:** when requested tenant differs from caller tenant, each fake method returns `CrossTenantDenied` unless denial audit fails, then `AuditFailed`. **Enforcement:** `reject_cross_tenant` before endpoint lookup/mutation. **Violation:** mismatched tenant reaches data/mutation path. **Verification:** inspect each endpoint branch and local tests; not run here. **Current state:** source-declared fake behavior only; identity provenance unknown. [API-001](#api-001)–[API-011](#api-011), [REL-012](BLAST_RADIUS.md#rel-012)–[REL-022](BLAST_RADIUS.md#rel-022). [Index](#r05)

<a id="inv-002"></a>
### INV-002 — Audit failure gates selected fake mutations
**Predicate:** create/revoke/invite/remove paths emit `Attempted` before local change; failure returns `AuditFailed`; later `Committed` may fail after state changed. **Enforcement:** endpoint methods in `handler.rs`. **Violation:** change precedes required Attempted or error is suppressed. **Verification:** trace each branch; no run claim. **Current state:** fake has post-mutation failure path and no PAT-to-member association, so removal does not prove the trait's revocation obligation. [API-006](#api-006), [API-007](#api-007), [API-009](#api-009), [API-010](#api-010), [REL-023](BLAST_RADIUS.md#rel-023)–[REL-031](BLAST_RADIUS.md#rel-031). [Index](#r05)

<a id="r06"></a>
## R06 — Configuração, targets e features

O manifesto declara a biblioteca e `[[test]] handler_customer`; Cargo também descobre `tests/mutation_kills.rs`. Não declara feature, bin ou build-dependency. A crate não lê configuração/env diretamente nos módulos inspecionados; a escolha de sink, observer e handler D1 pertence à composição do container. `corelink-slo` aparece em dependências normal e dev; seleção real de target/deploy permanece desconhecida. [Censo](BLAST_RADIUS.md#b06).

<a id="r07"></a>
## R07 — Falhas, observabilidade e compatibilidade

`CustomerHandlerError` é `#[non_exhaustive]` e declara `InvalidRequest`,
`CrossTenantDenied`, `Unauthorized`, `NotFound`, `AuditFailed`, `NotImplemented` e
`Internal`. DTOs, `AuditEvent`, `SliObservation` e a taxonomia também usam
`#[non_exhaustive]` quando expostos. Adicionar/remover variante, campo, método ou
reexport pode afetar matches e implementadores estáticos; não há censo completo de
consumidores nem política N/N-1 estabelecida nesta leitura.

`AuditSink::emit` propaga `AuditFailed`; lock envenenado gera `Internal` nos mapas do fake. `SliObserver::observe` é infalível na assinatura, mas o observer em memória descarta silenciosamente se seu lock estiver envenenado; `snapshot` e `count` então retornam erro. Uma observação local não demonstra métrica entregue, alerta ou SLO. `AuditFailed` após `Committed` não desfaz a mutação.

<a id="r08"></a>
## R08 — Verificação, evidência e lacunas

| Evidência | Escopo |
|---|---|
| Manifesto | Identidade, dependências, library e um `[[test]]` explícito (`handler_customer`) |
| Fonte | Exports, traits, DTOs, taxonomias, fake, guard, erros e comentários de ordem |
| Busca reversa | Declarações/imports estáticos no workspace, inclusive `corelink-container`; a composição nativa é source-wired, sem prova de seleção do artefato |
| Tests presentes | `handler_customer` é o target explícito; `mutation_kills.rs` é arquivo de integration test auto-descoberto; nenhum foi executado nesta autoria |

Desconhecidos explícitos: owner nominal, identidade/sessão verdadeira, autorização,
fonte de dados de cliente, lifecycle de PAT secreto, Stripe/billing, backend e
retenção de audit, entrega de SLI, compatibilidade completa, seleção do binário
shipped, CF Worker, deploy e comportamento de runtime. A fonte do container
declara, constrói e mescla `/v1/customer/*` com native PAT gate; esta evidência
estabelece `implemented = yes`, `wired = yes` no caminho nativo declarado, e
`runtime_verified = UNKNOWN`. Investigar com owner do container/adapter e
evidência de artefato e runtime apropriada.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
