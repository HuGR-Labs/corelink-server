---
name: own-corelink-privacy-erasure-worker
description: Ownership routing for corelink-privacy-erasure-worker; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-privacy-erasure-worker
  manifest: crates/corelink-privacy-erasure-worker/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-privacy-erasure-worker-structural-normalization-20260921
---

# Ownership — corelink-privacy-erasure-worker

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

Fonte estática somente: não prova runtime, operação de provider, retenção, entrega,
assinatura por KMS ou conclusão legal.

[Escopo](#s01) · [Contrato](#s02) · [Triagem](#s03) · [Fluxo](#s04) · [Parada](#s05) · [Saída](#s06).

<a id="s01"></a>
## S01 — Escopo e autoridade

| Use quando | Não use quando |
|---|---|
| Alterar tipos/traits/fakes de fan-out, ledger, audit, relatório ou sweep | Executar Queue, cron, R2, D1, Neon, KV, Stripe, Loki ou deploy |
| Mudar taxonomia de 12 backends, decisão, CloudEvent, SLA calculado ou pseudônimo | Declarar eliminação física, retenção cumprida, alerta entregue ou certificação jurídica |
| Traçar consumidores estáticos e compatibilidade de payload | Operar dados/tenants reais, KMS, URL assinada, fila ou storage |

<a id="s02"></a>
## S02 — Contrato sob ownership

**Implementação local:** `event`, `orchestrator`, `backends`, `idempotency`,
`audit_emit`, `report`, `verification_job`, `legitimacy` e reexport de
`pseudonymize`, `error` e `statuspage_publish`. **Superfície pública:** `ErasureWorker`,
`BackendErasureAdapter`, `ErasureIdempotencyLedger`, `ErasureAuditSink`,
`DsrLegitimacyStore`, `ReportSigner`, `VerificationJob`, enums/reexports e fakes em memória.

`statuspage_publish::{aggregate_24h_window,p95_of_observations,DsrCompletionStats}` é agregação
pura sob este ownership; scheduler, clerk-cf e `corelink-statuspage-real` são consumidores/bridge,
e qualquer publish HTTP Statuspage permanece fora desta unidade.

Menções a D1, R2, Neon, KV, Stripe, Loki, Queue, cron e KMS são contrato alvo,
comentário de wiring ou integração adjacente até verificação autorizada. A crate produz e
reverifica MAC BLAKE3 no fake; `ErasureAttestation` Ed25519 e seu armazenamento pertencem à
crate especializada/container, não a esta unidade.

<a id="s03"></a>
## S03 — Triagem obrigatória

| Se mudar | Preserve / investigue | Pare quando |
|---|---|---|
| `BackendKind`, plan ou outcome | 12 slots, ordem e consumidores de schema | novo backend/provider ou formato persistido sem owner |
| `BackendErasureAdapter` | `(tenant_id, subject_id, salt, legal_hold)` e fingerprint | for preciso chamar provider real |
| ledger | chave `(dsr_id, backend)`, replay versus divergência | D1/migração ou dado durável forem necessários |
| orquestrador/audit | legitimidade antes do início e ordem-fonte | efeito externo não puder ser desfeito localmente |
| report/verificação | JCS, assinatura/verify e complete/partial/SLA | KMS, R2, cron ou evidence URL forem necessários |
| pseudônimo | helper canônico e `pii_redacted=true` | decisão jurídica ou salt real for requerida |

<a id="s04"></a>
## S04 — Fluxo de trabalho

1. Fixe baseline, manifesto, contrato e consumidores em [R04](../../../docs/ownership/crates/corelink-privacy-erasure-worker/REFERENCE.md#r04) e [B03](../../../docs/ownership/crates/corelink-privacy-erasure-worker/BLAST_RADIUS.md#b03).
2. Escreva predicado falsificável; classifique todo efeito como fake local, contrato alvo ou integração observada separadamente.
3. Em replay, diferencie o lookup prévio que retorna completion sem `upsert`, os únicos outcomes
   `LedgerOutcome::{Inserted,Replayed}`, e `Err(ErasureIdempotencyError::DivergentPayload)`.
4. Em verificação, separe decisão em memória/relatório/MAC da alegação de cron, R2 ou alerta executado.
5. Rode somente checagens documentais autorizadas e declare lacunas.

<a id="s05"></a>
## S05 — Paradas obrigatórias

Pare diante de rede, Cargo/testes, Queue/cron, Cloudflare binding, D1/R2/Neon/KV,
Stripe/Loki, KMS/chave, tenant/dado real, migration, deploy, alerta, e-mail, URL assinada,
retenção ou pedido de atestado/conclusão legal. Pare também se um consumidor, schema ou
adapter concreto não puder ser classificado por fonte estática.

<a id="s06"></a>
## S06 — Saída mínima

Registre SHA, paths-fonte, contratos/relações, predicados, comandos documentais e resultado
literal. Declare desconhecidos e rota de escalação. Não chame busca/diff de teste, revisão
independente, prova de produção, attestation externa ou certificação de GDPR/LGPD/CCPA.

[Referência](../../../docs/ownership/crates/corelink-privacy-erasure-worker/REFERENCE.md#r01) · [Impactos](../../../docs/ownership/crates/corelink-privacy-erasure-worker/BLAST_RADIUS.md#b01) · [Manutenção](../../../docs/ownership/crates/corelink-privacy-erasure-worker/MAINTENANCE.md#m01)

<a id="s07"></a>
## S07 — Evidence and output

Report static evidence at its actual boundary: local error enums and the failing audit fake describe available failure shapes; `VerificationJob` describes report/signature construction for selected decision arms; the status-page module describes aggregation over supplied outcomes.

Cite source paths and state that Cargo/tests, provider effects, cron/Queue, durable audit, report upload, external signing, alert delivery, and legal completion remain unobserved. Handoff includes baseline, changed paths, affected REL/R/B/M records, permitted checker result, owner route, and unresolved evidence request. Never turn a reference, fake, or checker PASS into runtime proof.
