---
schema: corelink-ownership/1.1
document: reference
package: corelink-handler-admin
manifest: crates/corelink-handler-admin/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-admin-structural-normalization-20260921
---

# corelink-handler-admin — referência de ownership

Referência de fonte estática na baseline fixada. Descreve contratos, tipos e a
implementação em memória observados; não certifica HTTP, Cloudflare Worker,
control plane, runtime, persistência, provider de auditoria/telemetria,
identidade, autoridade de aprovação ou cobertura total de consumers.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Leitura](#r04) ·
[Auditoria](#r05) · [Aprovação](#r06) · [SLI](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade

| Campo | Fonte estática |
|---|---|
| Package | `corelink-handler-admin`; `crates/corelink-handler-admin/Cargo.toml` |
| Papel | Superfície de traits de leitura/admin-mutação, fake em memória, ledger de aprovação, auditoria e observer SLI |
| Dependência CoreLink | `corelink-slo` para reexportar `Sli` |
| Limite | O manifesto descreve o crate como skeleton; não entrega adapter HTTP/Worker nem um provider real |

<a id="r02"></a>
## R02 — Fronteiras

| Área | Nesta crate | Fora desta crate |
|---|---|---|
| API local | Requests, responses, operações, traits e erros públicos | Rota HTTP, serialização de protocolo e autenticação upstream |
| Mutação | Fake com mapa em memória e consulta/consumo via `ApprovalLedger` | Criação autenticada de aprovação, autoridade e backend durável |
| Auditoria/SLI | Taxonomia, portas e capturas em memória | Provider, persistência, exportação e telemetria operacional |

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo | Responsabilidade observada |
|---|---|
| `lib.rs` | Módulos e reexports públicos; comentário de contrato |
| `handler.rs` | Traits read/mutate, shapes, `MutateOp` e `InMemoryAdminHandler` |
| `ledger.rs` | Portas de verificação/escrita, rejeições e ledger em memória |
| `audit.rs` | Tipos de eventos e `AuditSink`/`InMemoryAuditSink` |
| `observer.rs` | Observação de `Sli::AvailControlPlane` e observer em memória |
| `error.rs` | Taxonomia de rejeição e falha da superfície |

<a id="r04"></a>
## R04 — Leitura e erro

`AdminReadHandler::read` recebe recurso, principal, flag `is_admin` e instante; a
resposta contém o recurso e bytes opacos. O fake consulta um `HashMap` protegido
por `Mutex`: devolve `NotFound` quando não há corpo, `Forbidden` quando a flag é
falsa e `AuditFailed`/`Internal` nas falhas locais correspondentes. A flag e o
principal são dados recebidos pelo trait; esta fonte não prova RBAC ou identidade
verificados upstream.

<a id="r05"></a>
## R05 — Auditoria: contrato e ordem implementada

`AuditEventKind` declara leitura attempted/served/denied e mutação
attempted/dual-approval-rejected/committed/denied, com principal, recurso,
aprovador opcional e timestamp. A ordem abaixo é somente a sequência de chamadas
visível em `InMemoryAdminHandler`, não prova persistência ou ordem em runtime:

- leitura autorizada: `ReadAttempted` antes do lookup, depois `ReadServed` no sucesso;
- leitura negada no fake: `ReadDenied` antes de `Forbidden`, sem `ReadAttempted`;
- mutação: `MutateAttempted` antes de RBAC e aprovação; rejeições visíveis emitem
  `MutateDenied` ou `MutateDualApprovalRejected` antes de retornar; sucesso altera
  o mapa e então emite `MutateCommitted`.

O comentário de `AdminReadHandler` exige `ReadAttempted` antes do lookup e o de
`AdminMutateHandler` exige outras ordens; esses são requisitos de implementadores.
Eles não eliminam a diferença observável no caminho negado do fake nem demonstram
um sink de auditoria real.

<a id="r06"></a>
## R06 — Mutação e segunda aprovação

`AdminMutateHandler` recebe `MutateOp`, iniciador, flag admin, token opcional e
timestamp. `DualApprovalToken` contém `approval_id` e `approver`, mas a
implementação ignora o segundo campo para decidir autorização. Ela chama
`ApprovalLedger::verify_and_consume(approval_id, initiator, resource)` antes da
alteração de estado e usa o `VerifiedApproval.approver` retornado para a auditoria
de commit.

No ledger em memória, sob o mesmo `Mutex`, a entrada deve existir, ter recurso igual, ter `approver` diferente de `initiator` e estar não consumida; sucesso marca a entrada consumida. Isso é uma verificação real de distinção entre as duas strings registradas nesse caminho de fonte, com rejeições `Unknown`, `ScopeMismatch`, `SelfApproval`, `Consumed` e `Backend`. Não verifica a identidade da pessoa associada a uma string, não prova que uma aprovação foi criada

por um segundo ator autenticado, nem estabelece autoridade de aprovação fora desse contrato local.

<a id="r07"></a>
## R07 — SLI e implementações em memória

`SliObserver` é infalível e recebe `SliObservation { sli, is_error, latency_us }`.
O fake constrói observações `Sli::AvailControlPlane` com latência zero nos caminhos
diretamente visíveis após seus emits/decisões. `InMemorySliObserver` e
`InMemoryAuditSink` acumulam vetores em processo sob `Mutex`; são ferramentas de
captura local, não evidência de telemetria ou auditoria entregue. Em particular,
um `AuditSink::emit` que falha retorna `AuditFailed` antes das observações que vêm
depois daquela chamada no fluxo selecionado.

<a id="r08"></a>
## R08 — Lacunas e encaminhamento

Desconhecidos explícitos: owner nominal, todos os implementadores/consumers,
entrada HTTP, Worker/CF, control plane, autenticação e RBAC reais, origem da
identidade do aprovador, endpoint de criação, provider ou durabilidade do ledger,
audit sink e telemetry sink, atomicidade de um backend externo, deploy e métricas
observadas. Para fechar uma lacuna, localizar o adapter/entrypoint específico e
coletar evidência no escopo dele; comentários, traits e fakes não bastam.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
