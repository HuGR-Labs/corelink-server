---
id: "RB-FM-305"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "gc", "dedup", "data-integrity"]
---

# RB-FM-305 — Tombstone Lost (Eviction reverte com re-upload)

> **FM:** FM-305 (S=4, O=2, D=4, RPN=32, P1) | **INV:** INV-GC-002, INV-DEDUP-CONSISTENCY HIGH | **SLA:** investigate ≤ 24h

## Detecção

- Métrica `corelink_blob_resurrect_total > 0` (digest deletado e re-uploaded em ≤ 24h, mesmo tenant).
- Reconcile diário detecta gap entre `tombstone_log` e `blob_meta.deleted_at`.
- AC entry referencing blob marked deleted.
- Customer report (rare): "blob came back after delete".

## Comunicação

- **SEV-2** (não SEV-1: dado já existe; questão é compliance/billing).
- Page SRE + Engineer GC subsystem.
- Privacy Officer notify se DSR-erasure tombstone afetado (esca a SEV-1 nesse caso).

## Mitigação imediata (≤ 30 min)

1. **Identificar blobs afetados**: query D1 `tombstone_log` cross-ref `blob_meta` para `deleted_at IS NULL AND tombstone_id IS NOT NULL`.
2. **Re-aplicar tombstone**: UPDATE `blob_meta SET deleted_at = tombstone_log.deleted_at` para affected blobs.
3. **Verificar billing impact**: events emitidos durante "ressurrect window" são contabilizados? Reconciliation worker (S-10) detecta drift.
4. **Audit emissão**: cada re-tombstone gera CloudEvent `corelink.gc.tombstone_reapplied` com `prev_state, fix_reason, operator_id`.

## Diagnóstico (≤ 24h)

Causa raiz típica:

1. **Race condition** entre eviction (S-07) e re-upload do mesmo digest:
   - Worker A: evict blob X (write tombstone t1).
   - Worker B: write blob X (idempotent → mesmo digest, sem ler tombstone).
   - Resultado: blob ressuscitado.
2. **Replication lag** D1 entre regiões: tombstone só na primary, secondary aceita write.
3. **Refcount bug** (FM-300 vizinho): refcount não decrementado, eviction não viu razão pra tombstone.

## Resolução

- Hot fix: write path lê tombstone_log + soft-block 24h re-write se digest tombstoned recente.
- Cold fix:
  - Tombstone como hard-block durante grace period (rejeita re-upload com 410 Gone se < grace).
  - INV-DEDUP-CONSISTENCY property test cobre cenário de race.
  - TLA+ extension de `gc_correctness.tla` modeling tombstone race.

## Post-incident

- Post-mortem dentro de 7d.
- Adicionar property test cobrindo specific scenario.
- Review com Architect + Privacy Officer.

## Evidence

- D1 query results pre/post fix.
- Reconciliation drift report.
- CloudEvents audit trail.

## References

- `failure_modes.md` FM-305 entry (criar se ausente).
- `invariant_registry.md` INV-DEDUP-CONSISTENCY (S-07).
- `specs/04_sprints/S07/_spec_contract.md`.
