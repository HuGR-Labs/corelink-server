---
id: "RB-FM-404"
type: "runbook"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "gc", "data-integrity", "tla", "dry-run-executed"]
---

> **Dry-run executed 2026-05-02** — host-side harness `scripts/rb_fm_404_dry_run.sh` (WI-S06-007) green; chaos magnitude pinned to UpdateActionResult fired at exactly `mark_started_at_ms + 1ms` (boundary case; protected_re_ref expected) per Lote 10.6bis P0-W7-4. 100k race property test cross-validates `gc_correctness.tla::InvGCReRefProtected` against the real Rust impl. Audit trace: `specs/_audits/2026-05-02-rb-fm-404-dry-run.md`.

# RB-FM-404 — GC Sweep Conflita Com Write (Refcount Race)

> **FM:** FM-404 (S=5, P1 S=5→upgrade) | **CTRL:** CTRL-GC-001 + INV-GC-001 (TLA+) | **SLA:** mitigate < 30 min (data integrity!)

## Detecção

- Customer report: "blob existia em commit anterior, agora 404".
- Alert `corelink_gc_unexpected_delete_total > 0`.
- Métrica `corelink_blob_resurrect_total` (tentativa de re-upload do mesmo digest após delete).

## Comunicação

- **SEV-1.** Page Architect + SRE Lead + dev responsável por GC.
- Status page: degraded (sem detalhe).

## Mitigação imediata (≤ 15 min)

1. **Pausar GC sweep globalmente** via degrade_mode flag.
2. Verificar quanto tempo desde último GC bem-sucedido.
3. Se blob ainda na grace period (72h CTRL-GC-001): undelete via tombstone reversão.
4. Se blob já physical delete: cliente vai re-uploadear (cache miss não fatal mas SLO-CAS-GET impactado).

## Mitigação completa (≤ 4h)

1. Diff entre GC mark-set e current AC entries: identificar todos blobs falsamente marcados como órfãos durante a janela.
2. Para cada falso positivo: verify se ainda existe (recovery de tombstone) ou se cliente re-uploadeará.
3. Patch hot-fix do bug de race (provavelmente `mark_started_at` vs `ac.created_at` comparison).
4. Re-enable GC só com TLA+ check verde do INV-GC-001 + INV-GC-004.

## Forensics

1. Re-rodar TLA+ model checking com cenário observado.
2. Adicionar regression test cobrindo cenário.
3. PAT-SOFT-DELETE-001 funcionou? Grace period suficiente?

## Post-mortem

- Atualizar `gc_correctness.tla` se modelo formal não cobria o caso.
- Considerar grace period maior (96h vs 72h).
- Audit anterior: este FM tinha O=1; atualizar para O observado.
