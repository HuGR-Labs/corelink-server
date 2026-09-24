---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-handler-admin
manifest: crates/corelink-handler-admin/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-admin-structural-normalization-20260921
---

# corelink-handler-admin — blast radius

Relações atômicas derivadas de exports, manifesto e fluxo de fonte do fake.
Setas indicam dependência ou sequência declarada, não execução, persistência,
integração HTTP/CF, identidade, autoridade ou cobertura completa.

[Base](#b01) · [Mutação](#b02) · [Ledger](#b03) · [Audit/SLI](#b04) ·
[Fora do crate](#b05) · [Consumers](#b06).

<a id="b01"></a>
## B01 — Tipos públicos para implementadores

`AdminReadRequest`/`AdminReadResponse` → `AdminReadHandler`; `AdminMutateRequest`/
`AdminMutateResponse`/`MutateOp`/`DualApprovalToken` → `AdminMutateHandler`.
Alterar campo, variante, assinatura, `#[non_exhaustive]`, trait bound ou erro pode
afetar implementadores e callers estáticos. Não há uma enumeração completa de
consumers nesta evidência.

<a id="b02"></a>
## B02 — Pedido de mutação para estado aplicado

`AdminMutateRequest` → recurso de `MutateOp` → `MutateAttempted` → RBAC → token
→ `ApprovalLedger::verify_and_consume` → mapa `applied` → `MutateCommitted`.
Alterar essa relação afeta os caminhos de rejeição, single-use e as expectativas
de observação no fake. A sequência é lida da fonte; o mapa é estado em memória e
não um sistema de mutação produtivo.

<a id="b03"></a>
## B03 — Ledger para controle local de distinção

`approval_id` + iniciador + recurso → `ApprovalLedger` → `VerifiedApproval` ou
`ApprovalRejection`. O ledger em memória compara o `approver` registrado com o
iniciador, restringe o recurso e consome a entrada no sucesso sob um `Mutex`.
Assim, mudar campo, precedência de rejeição, consumo ou recurso impacta a
distinção de strings e replay dentro desse fake. Isso não transforma o registro
numa verificação de identidade, nem prova criação independente, autoridade real
ou atomicidade de qualquer implementação externa.

<a id="b04"></a>
## B04 — Resultado para auditoria e SLI

Read/mutate → `AuditSink::emit(AuditEvent)` e →
`SliObserver::observe(SliObservation { Sli::AvailControlPlane, ... })`.
Uma alteração de kind, payload, posição do emit, erro ou observação muda os
contratos das portas e o comportamento capturado em memória. No fluxo do fake,
o commit é emitido após alterar o mapa e as rejeições de mutação são emitidas
antes do retorno; não há garantia além dessa ordem de chamadas no código.

<a id="b05"></a>
## B05 — Limites de composição

`corelink-handler-admin` → `corelink-slo::definition::Sli` por reexport.
O pacote não declara adapter de HTTP, Worker/CF, control plane, D1, provider de
auditoria ou provider de telemetria. Qualquer efeito nesses sistemas depende de
implementador, composição e entrypoint fora desta relação estática.

<a id="b06"></a>
## B06 — Consumers e cobertura

O manifesto declara `thiserror` e `corelink-slo`, e a fonte expõe traits para
implementação externa. Esta edição não afirma uma lista de importadores ou
implementadores, nem resolve features, geração, builds selecionados, deploys ou
consumers externos. Antes de uma mudança compatível/incompatível, refazer busca
reversa no recorte e classificar cada resultado, sem promover ausência de busca a
ausência de consumer.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
