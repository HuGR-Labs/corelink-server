---
schema: corelink-ownership/1.1
document: reference
package: corelink-privacy-erasure-worker
manifest: crates/corelink-privacy-erasure-worker/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-privacy-erasure-worker-structural-normalization-20260921
---

# corelink-privacy-erasure-worker — referência de ownership

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08)

Referência SOURCE estática: descreve contratos e fakes declarados, não wiring ou resultado operacional.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Contratos](#r04) · [Invariantes](#r05) · [Evidência](#r06).

<a id="r01"></a>
## R01 — Identidade

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005)

| Campo | Evidência SOURCE |
|---|---|
| Package | `crates/corelink-privacy-erasure-worker/Cargo.toml` |
| Papel | pipeline tipado DSR de 12 braços, decisão e verificação em memória |
| Raiz | `src/lib.rs` e seus reexports |
| Runtime observado nesta crate | nenhum |

<a id="r02"></a>
## R02 — Fronteiras: fonte não é operação

| Superfície declarada | Esta crate possui | Não prova |
|---|---|---|
| Orquestração | `ErasureWorker`, `InMemoryErasureWorker`, mutex e plano | Queue/DO, paralelismo, retry ou tráfego |
| Backends | trait + `InMemoryBackendErasureAdapter` por `BackendKind` | DELETE/update/purge em R2, D1, Neon, KV, Stripe ou Loki |
| Ledger/legitimidade | traits e HashMap/HashSet/fakes | tabela D1, migração, auth Clerk ou durabilidade |
| Verificação | sweep, sentinel hash e `VerificationJob` | cron, alerta, status de ticket ou leitura de provider |
| Report | JCS, MAC BLAKE3, key helper e fake signer | KMS, upload R2, URL assinada ou attestation Ed25519 |
| Pseudônimo | reexport SHA-256/salt/marker | gestão de salt, irreversibilidade jurídica ou aplicação em dados reais |

Os 8 braços efetivos declarados são Neon main/billing, R2 CAS/AC, D1, KV, Stripe e Loki; os 4 pseudonimizados são R2 audit, Neon PITR, R2 CAS legal hold e R2 evidence. Isso é taxonomia de fonte, não comprovação de que algum provider recebeu operação.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo | Responsabilidade observada |
|---|---|
| `event` | tipos 12-backend, decisões, outcomes, CloudEvents, request/plan/completion e constantes |
| `orchestrator` | gate de legitimidade, fan-out serial fake, audit e decisão de verificação |
| `backends` | trait de erase/fingerprint, linhas e adapters em memória |
| `idempotency` | ledger por `(dsr_id, backend)`, snapshot e fixtures |
| `audit_emit` | record, sink e captura/falha em memória |
| `error` | taxonomias tipadas de erro de worker, backend, ledger, report e audit |
| `report` | payload, JCS, MAC keyed-BLAKE3 e chave de objeto canônica |
| `verification_job` | sweep, outcome, assinatura local, cálculo SLI e helper de deadline |
| `statuspage_publish` | agregador puro 24h, p95 e `DsrCompletionStats`; não publica HTTP |
| `legitimacy`, `pseudonymize` | trait/fakes de pré-checagem; reexport do helper externo |

<a id="r04"></a>
## R04 — Contratos públicos

<a id="api-001"></a>
### API-001 — Fan-out e backend
`BackendKind`, `ErasureRequest`, `ErasurePlan`, `BackendCompletion`, `BackendErasureOutcome` e `BackendErasureAdapter::{erase,verification_hash}` são o contrato tipado. `try_new[_with_legitimacy]` exige 12 adapters na ordem canônica. O adapter fornecido é em memória; nomes de providers não convertem sua execução em evidência.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Orquestração, legitimidade e audit
`ErasureWorker::process_erasure`, `DsrLegitimacyStore` e `ErasureAuditSink` formam a fronteira de entrada. A implementação de três argumentos instala `AllowAllDsrLegitimacyStore` de teste; a variante explícita aceita a trait. `ErasureAuditRecord` transporta UUID bruto no fake; o serializer pseudonimizado produtivo citado em comentário é integração, não fato observado.

<a id="api-003"></a>
[↩](#r01)
### API-003 — Idempotência
`ErasureIdempotencyLedger::{get,upsert,snapshot}` trata tombstone tipada. `LedgerOutcome` tem
somente `Inserted` e `Replayed`; payload divergente é
`Err(ErasureIdempotencyError::DivergentPayload)`, não um outcome. O replay normal do orquestrador
vem de `get` e retorna a completion armazenada antes de alcançar `upsert`; os métodos de snapshot
de outcome têm default no-op. A referência a UNIQUE D1 é correspondência de contrato, não observação de D1.

<a id="api-004"></a>
[↩](#r01)
### API-004 — Relatório, verificação e attestation
`VerificationJob::run_24h_sweep` retorna `VerificationOutcome`; somente complete/partial criam `ErasureReport`, `ReportSignature` e `canonical_report_key`. `ReportSigner` usa JCS e MAC BLAKE3; `InMemoryReportSigner` é fake determinístico. `verify_report` revalida esse MAC local. Não cria nem verifica `ErasureAttestation` Ed25519 da crate especializada.

<a id="api-005"></a>
[↩](#r01)
### API-005 — Pseudonimização
`pseudonymize` reexporta `corelink-privacy-pseudonymize`: hash de subject+salt, verifier e `PseudonymizationMarker`. Compatibilidade inclui formato/hash/marker; não inclui cofre/salt, rotação de chave ou parecer de anonimização.
[↩](#r01)

<a id="r05"></a>
## R05 — Invariantes de fonte

**INV-ERASURE-001 — legitimidade fail-closed.** Em `process_erasure`, `false` ou `Err` de `is_requested(dsr_id, tenant_id)` retorna `Rejected` antes de `Started`, fan-out ou ledger. O predicado é sobre orquestrador/fake; não prova D1 ou identidade de entrada.

**INV-ERASURE-002 — replay por slot.** Se `ledger.get(dsr_id, backend)` retorna completion, `fanout_one` retorna esse valor sem adapter, `BackendCompleted` ou segundo insert. Sem prior, a mutation do adapter vem antes do audit de backend, que vem antes do `upsert`. Não há atomicidade distribuída demonstrada.

**INV-ERASURE-003 — verificação é 12-deep.** `verify_erasure` percorre `canonical_backend_kinds`; cada hash deve igualar `CANONICAL_EMPTY_TENANT_HASH` e outcome deve ser successful para complete. Ausência/falha/mismatch conta para partial; prazo vencido com snapshot incompleto produz SLA breach.

**INV-ERASURE-004 — report local verificável.** O job gera report/MAC apenas para complete/partial; `verify_report` usa o mesmo signer sobre bytes JCS. Isso prova somente algoritmo e chave em memória.

<a id="r06"></a>
## R06 — Evidência e desconhecidos

Foram lidos manifesto, `src/{lib,event,orchestrator,backends,idempotency,audit_emit,error,report,verification_job,statuspage_publish,legitimacy,pseudonymize}.rs` e busca reversa estática. Testes declarados são evidência de intenção/cobertura de fonte, mas não foram executados. Desconhecidos: censo inverso completo, schema/migração aplicada, bindings/credenciais, comportamento de providers, dados reais, cron/queue, upload/URL, alertas, retenção, attestation externa e conformidade legal.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)

<a id="r07"></a>
## R07 — Failures and observability

`error.rs` defines distinct typed variants for audit-store, backend transport/not-applicable/verification mismatch, idempotency backend/divergent payload, report canonicalization/signing/signature, configuration, and internal failures. `audit_emit.rs` exposes `ErasureAuditSink::emit -> Result` and an in-memory failing sink; `event.rs` maps the `VerificationFailed` event to the SEV-1 classification predicate. **SOURCE:** `src/error.rs`, `src/audit_emit.rs:75-95,180-193`, `src/event.rs:664-671`. These are error/event contracts and fake behavior only. They do not establish retries, alert delivery, a durable sink, observed incidents, or runtime metrics.

<a id="r08"></a>
## R08 — Verification and evidence

`VerificationJob::run_24h_sweep` calls the worker verification path; complete/partial decisions construct a report, signature, and canonical object-key value, while SLA-breached and verification-failed decisions return without those optional values. `verify_report` delegates to the configured `ReportSigner`; the supplied signer is in-memory. **SOURCE:** `src/verification_job.rs:155-238`, `src/report.rs:100-140`. `statuspage_publish::aggregate_24h_window` computes counts and p95 from supplied outcomes. These are local source algorithms only: no cron/sweep invocation, R2 write, URL, KMS/Ed25519 signing, Statuspage publish, or alert was observed; tests were not run.
