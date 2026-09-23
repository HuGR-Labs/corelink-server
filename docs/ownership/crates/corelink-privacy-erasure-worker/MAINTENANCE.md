---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-privacy-erasure-worker
manifest: crates/corelink-privacy-erasure-worker/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-privacy-erasure-worker-structural-normalization-20260921
---

# corelink-privacy-erasure-worker — manual de manutenção

[M01](#m01) · [M02](#m02) · [M03](#m03) · [M04](#m04) · [M05](#m05) · [M06](#m06)

[Preparação](#m01) · [Procedimentos](#m02) · [Matriz](#m03) · [Recuperação](#m04) · [Escalação](#m05).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Confirme baseline, manifesto, módulo, predicado e relações antes de alterar. Nesta campanha, somente leitura e checagens documentais estáticas são permitidas: não executar Cargo/testes/rede, nem tocar Queue, cron, R2, D1, Neon, KV, Stripe, Loki, KMS, credencial, tenant/dado real, migration, deploy, alerta, URL ou publicação. Leia [Referência](REFERENCE.md#r01) e [Relações](BLAST_RADIUS.md#b03).

<a id="m02"></a>
## M02 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Alterar fan-out ou adapter
**Pré-condição:** enum, ordem e consumer foram classificados. 1. Compare os 12 slots e `is_effective`; 2. trace `erase` e `verification_hash`; 3. atualize relações afetadas. **Esperado:** contrato/fake permanece explícito. **Pare:** provider, schema ou operação real entrar. **Evidência:** `event.rs`, `backends.rs`, B03. **Recuperação:** restaure bytes locais; não alegue reversão de provider. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar idempotência, legitimidade ou audit
**Pré-condição:** chave, ordem e efeitos foram mapeados. 1. Preserve `(dsr_id, backend)`, os únicos outcomes `Inserted`/`Replayed` e `Err(ErasureIdempotencyError::DivergentPayload)` separado; 2. registre que replay normal vem de `get` e retorna a completion antes de `upsert`; 3. compare `is_requested` antes do início; 4. registre a ordem da fonte (adapter mutation → audit backend → ledger, após audit Started). **Pare:** D1/auth/audit durável for necessário. **Evidência:** `orchestrator.rs`, `idempotency.rs`, `legitimacy.rs`, `audit_emit.rs`. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Alterar verificação, relatório ou attestation adjacente
**Pré-condição:** decision arm, completions, JCS e consumers identificados. 1. Mantenha complete/partial/SLA; 2. confronte sentinel/hash e report opcional; 3. separe MAC fake de KMS e attestation Ed25519 externa. **Pare:** cron, R2, key, URL, assinatura externa ou evidência de auditor exigir ação. **Evidência:** `verification_job.rs`, `report.rs`, REL-ERASURE-002/006. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Alterar pseudonimização
**Pré-condição:** helper, marker e consumidores foram localizados. 1. Trace reexport até `corelink-privacy-pseudonymize`; 2. preserve os 4 braços pseudonimizados; 3. trate salt/key real e conclusão de anonimização como externos. **Pare:** cofre, rotação ou decisão legal. **Evidência:** `pseudonymize.rs`, `event.rs`, B03. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Alterar API/reexport/consumidor
**Pré-condição:** censo B03 atualizado. 1. Busque package/crate fora de archive; 2. distinga manifest/import de bridge/documentação; 3. classifique container, scheduler, privacy, E2E e attestation adjacente. **Pare:** consumer material sem classificação. [Procedure index](#m02)


<a id="m03"></a>
## M03 — Matriz de validação

| Alteração | Checagem permitida | Não prova |
|---|---|---|
| docs de ownership | cabeçalhos, links, paths, SHA e diff | compilação, teste ou revisão fria |
| enum/trait | busca de símbolo e assinatura fonte | compatibilidade total ou provider |
| ordem do pipeline | leitura linear do orquestrador | atomicidade distribuída/durabilidade |
| report/verificação | forma tipada, JCS e relação estática | KMS, cron, R2, upload ou alertas |
| pseudônimo | helper/marker e censo estático | salt real ou adequação legal |

<a id="m04"></a>
## M04 — Recuperação e compatibilidade

Para mudança ainda local, reverta somente bytes locais e restaure contrato/reexport anterior. Não use `git revert` como alegação de compensação em D1/R2/Neon/KV/Stripe/Loki, fila, cron ou evidência: qualquer um exige owner, autorização e runbook próprio. Mantenha enum/trait/formato persistido até consumidores e schema terem decisão coordenada.

<a id="m05"></a>
## M05 — Escalação e saída

Escale container por route/adapters/legitimidade, scheduler/clerk/statuspage por cron/publicação, privacy/pseudonymize por formato/salt, SLO por métrica, attestation por Ed25519, e owner de cada provider por operação real. Entregue SHA, paths, predicado, consumidores, comando documental, resultado literal e desconhecidos. Esta documentação não é revisão independente, evidência de produção nem certificação legal.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)

<a id="m06"></a>
## M06 — Escalation and evidence

Before handoff, name the source commit and touched module, changed API/invariant, affected REL records, and static check result. Route container and direct adapter changes to `corelink-container`; outcome/metric parsing to scheduler, Clerk, statuspage, or SLO owners named in B03; pseudonym format to privacy/pseudonymize owners; attestation to `corelink-erasure-attestation`.

For provider, queue, cron, migration, key, alert, or data-state claims, name the missing operational evidence and route it to that boundary's owner. A source search, fake, test declaration, or document checker is not execution or independent review.
