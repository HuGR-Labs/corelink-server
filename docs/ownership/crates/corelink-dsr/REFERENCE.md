---
schema: corelink-ownership/1.1
document: reference
package: corelink-dsr
manifest: crates/corelink-dsr/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dsr-structural-normalization-20260921
---

# corelink-dsr — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004)

`corelink-dsr` é a superfície Rust de lógica pura para solicitações de direitos do titular. O manifesto e `src/lib.rs` expõem traits e fakes em memória; não expõem uma rota HTTP, store remoto, chave real nem entrega de email. A fonte enumera seis direitos: Access, Portability, Rectification, Erasure, Restriction e Objection.

| Campo | Evidência SOURCE |
|---|---|
| Package / manifesto | `crates/corelink-dsr/Cargo.toml` |
| Raiz e reexports | `crates/corelink-dsr/src/lib.rs` |
| Módulos | `audit`, `calendar`, `endpoint`, `error`, `event`, `mfa`, `receipt`, `store` |
| Runtime | não observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Esta crate possui | Não prova / não possui |
|---|---|---|
| Intake DSR | tipos, `DsrEndpoint` e orquestrador em memória | rota CF em produção |
| Recibo | trait, claims e emissor determinístico em memória | chave RS256/KMS real |
| Step-up | trait e fake de token | cerimônia WebAuthn/TOTP/FIDO2 real |
| Ticket | trait e `InMemoryDsrRequestStore` | mirror Neon durável |
| Notificação | nenhum adaptador Email | envio/entrega de email |

As integrações listadas como diferidas nos comentários de `lib.rs`, `endpoint.rs`, `mfa.rs` e `receipt.rs` são backlog de wiring, não evidência de conclusão legal ou operacional.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo | Papel fonte |
|---|---|
| `event` e `calendar` | direitos, estados, decisões, jurisdições e deadline de SLA |
| `endpoint` | trait e pipeline serializado em memória |
| `audit` | taxonomia/record/sink e fake falho/em memória |
| `store` | trait de ticket com chave `(tenant_id, request_id)` e fakes |
| `mfa` | token, verifier e fakes |
| `receipt` | claims, issuer/verifier e fake determinístico |
| `error` | taxonomias de falha propagadas pelas traits |

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Taxonomia e calendário
**Símbolos:** `DsrRequestKind`, `DsrJurisdiction`, `DsrStatus`, `DsrDecision`, `DsrRequest`, `DsrTicket`, `sla_for`.
**Entradas/efeitos:** request tipado e jurisdição → decisão/ticket e deadline numérico; não há chamada remota no contrato.
**Compatibilidade:** enums são `#[non_exhaustive]`; os valores e reexports exigem censo de consumidor antes de mudança. Fonte: `event.rs`, `lib.rs`.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Endpoint e estado
**Símbolos:** `DsrEndpoint::{submit,poll_status}`, `InMemoryDsrEndpoint`, `DsrRequestStore`.
**Entradas/efeitos:** submit consulta/insere ticket pelo store injetado; poll consulta `(tenant_id, request_id)` e retorna decisão tipada.
**Compatibilidade:** a implementação fornecida é em memória; a trait não instala um backend Neon. Fonte: `endpoint.rs`, `store.rs`.

<a id="api-003"></a>
[↩](#r01)
### API-003 — Audit e MFA
**Símbolos:** `DsrAuditSink`, `DsrAuditEventType`, `MfaStepUpVerifier`, `MfaStepUpToken`.
**Entradas/efeitos:** sink recebe record tipado; verifier recebe token opcional. Erro dessas traits é convertido em `DsrError` pelo endpoint.
**Compatibilidade:** as sete strings de evento e os erros são superfície de consumidor; sink real permanece externo. Fonte: `audit.rs`, `mfa.rs`, `error.rs`.

<a id="api-004"></a>
[↩](#r01)
### API-004 — Recibo
**Símbolos:** `DsrReceipt`, `JwtReceiptIssuer`, `JwtReceiptToken`, `RECEIPT_EXPIRY_DAYS`.
**Entradas/efeitos:** issuer emite/verifica token opaco para claims tipadas; fake em memória é determinístico.
**Compatibilidade:** `RECEIPT_ALG_RS256` é uma constante de formato; ela não demonstra assinatura RS256 real. Fonte: `receipt.rs`.
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Owner | Vida observável nesta fonte |
|---|---|---|
| request/ticket/receipt | caller + store/issuer injetados | processo do fake ou implementação da trait |
| audit record | sink injetado | definido pelo sink; fake guarda memória |
| mutex do endpoint | `InMemoryDsrEndpoint` | instância do orquestrador |

<a id="inv-001"></a>

### INV-001 — Step-up é limitado aos braços destrutivos
[↩](#r01)
**Predicado falsificável:** `Erasure` ou `Rectification` sem token retorna `MfaRequired` somente quando `RequestReceived`, a consulta inicial do store e o emit `MfaStepUpRequired` têm sucesso; tanto esse retorno quanto uma falha em qualquer um desses passos não chamam `DsrRequestStore::insert`. Para Access, Portability, Restriction e Objection, o ramo MFA não invoca o verifier. Falha de qualquer emit ou lookup retorna `DsrError` em vez

de alcançar/completar esse retorno. **Imposição SOURCE:** `DsrRequestKind::is_destructive` e o ramo em `InMemoryDsrEndpoint::submit` (`event.rs`, `endpoint.rs`). **Violação:** um dos quatro braços não destrutivos exige verifier, ou um dos dois destrutivos insere ticket sem token. **Limite:** não prova identidade/step-up real.

<a id="inv-002"></a>

### INV-002 — Audit precede mutação de ticket
**Predicado falsificável:** no caminho de aceitação, `RequestAccepted` e `ReceiptIssued` são emitidos antes de `DsrRequestStore::insert`; se `emit` falhar antes desse ponto, `submit` retorna erro e não alcança esse `insert`.
**Imposição SOURCE:** ordem explícita em `endpoint.rs`. **Violação:** uma implementação altera `insert` para ocorrer antes dos emits, ou continua após erro do sink. **Limite:** refere-se à mutação do store no orquestrador em memória, não a durabilidade/audit externo.

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Ticket é consultado no escopo tenant + request
**Predicado falsificável:** `poll_status(tenant_a, request_id)` não obtém ticket inserido apenas sob `tenant_b`; o contrato de `get` recebe ambos os IDs.
**Imposição SOURCE:** assinatura de `DsrRequestStore::get` e chamadas em `endpoint.rs`/`store.rs`. **Violação:** lookup reduzido apenas a `request_id`. **Limite:** não prova autenticação do tenant de entrada.

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Recibo tem janela calculada de 90 dias
**Predicado falsificável:** `DsrReceipt::new` calcula `expires_at_ms = submitted_at_ms + RECEIPT_EXPIRY_DAYS × 86_400_000`, e issuer em memória rejeita claims com exp divergente.
**Imposição SOURCE:** `receipt.rs`; `RECEIPT_EXPIRY_DAYS` em `event.rs`. **Violação:** claim divergente é emitido pelo fake. **Limite:** não prova key management ou verificação RS256 real.
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

O manifesto declara dependências Rust e os testes do pacote; a presente campanha não resolveu o grafo nem executou Cargo. `lib.rs` descreve uma rota CF, key/WebAuthn, Neon e Email como wiring diferido. Portanto, nenhum target, feature ou comentário é tratado como prova de deploy, tráfego ou conformidade jurídica.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal tipado | Significado no contrato | Ação segura |
|---|---|---|
| `DsrError::Audit` | sink recusou o record | não presumir mutação de ticket; trace a ordem fonte |
| `DsrError::Store` | trait de store retornou erro | classificar backend; não diagnosticar Neon sem owner |
| `DsrError::Mfa` | verifier recusou token | distinguir do `DsrDecision::MfaRequired` |
| `DsrError::Receipt` | issuer recusou claim/token | não inferir KMS/RS256 real |

<a id="r08"></a>
## R08 — Verificação e evidências

Evidência permitida é SOURCE: manifesto, `src/{lib,audit,calendar,endpoint,error,event,mfa,receipt,store}.rs` e busca estática de manifestos/imports. A população encontrada inclui reexport por `corelink-privacy`, import/manifests de `corelink-container`, `e2e-dsr`, `e2e-billing-flow` e uso dev no erasure worker; relações clerk-cf/cf-bindings/replication são adjacências estáticas classificadas em B03, não imports diretos comprovados. `_archive/wi-s11-002-partial/Cargo.toml` foi excluído do censo de consumidor atual. Nenhum teste, Cargo, runtime, deploy, revisão fria ou conclusão legal foi alegado.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01).
