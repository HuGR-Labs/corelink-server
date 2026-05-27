---
id: "RB-FM-300"
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

> **Dry-run executed 2026-05-02** — host-side harness `scripts/rb_fm_300_dry_run.sh` (WI-S06-007) green; chaos magnitude pinned to 0.5% per-tenant refcount drift per Lote 10.6bis P0-W7-4. Audit trace: `specs/_audits/sealed/2026-05-02-rb-fm-300-dry-run.md`.

# RB-FM-300 — GC Deleta Blob Ainda Referenciado (Refcount Bug)

> **FM:** FM-300 (S=5, O=2, D=4, RPN=40, P1) | **INV:** INV-GC-001 CRITICAL (TLA+) + CTRL-GC-001..002 | **SLA:** recover ≤ 1h

## Detecção

- Customer report: "cache hit ratio caiu abruptamente; builds falhando com `CAS_NOT_FOUND`".
- Alert `corelink_gc_unexpected_delete_total > 0` (reconcile diário detecta).
- Métrica `corelink_blob_resurrect_total > 0` (re-upload do mesmo digest após delete = smell).
- TLA+ re-check em ambiente simulado falha.

## Comunicação

- **SEV-1.** Page Architect (owner do INV-GC-001 TLA+) + SRE Lead + dev responsável pelo último mudança em GC.
- Status page: `degraded` (cache inconsistency).
- Customer notification preparada se dado perdido.

## Mitigação imediata (≤ 15 min)

1. **Pausar GC sweep globalmente** via `degrade_mode=cache-only` (PAT-DEGRADE-001).
2. Verificar última execução de GC bem-sucedida (timestamp + blobs afetados).
3. Identificar universo de blobs deletados incorretamente (query D1: `deleted_at` recente + refcount > 0).
4. Se ainda no grace period (72h, CTRL-GC-001): **undelete via tombstone reversão** (`UPDATE blob_meta SET deleted_at = NULL`).

## Mitigação completa (≤ 4h)

1. Para cada blob perdido fora do grace:
   - Verificar R2 versioning: restore versão anterior se ainda existe.
   - Se physical delete irreversível: notificar tenant (cache miss forçado = re-upload).
2. Diff entre GC mark-set e AC entries ativos: identificar falsos positivos sistemáticos.
3. Patch hot-fix do bug:
   - Race entre Mark phase e UpdateActionResult? (INV-GC-004)
   - `mark_started_at` comparison incorreto?
   - Refcount subtracted 2x?
4. Re-enable GC APENAS após TLA+ model check verde + property test passa + chaos test semana sem recurrence.

## Forensics

1. TLA+ scenario replay: rodar TLC com trace do incident (seed = trace hash).
2. Add regression test cobrindo cenário exato.
3. Se `mark_started_at` foi root: reforçar CTRL-GC-001 (grace 72h).
4. Preservar evidence: D1 snapshot pre-mitigation + GC logs + audit events.

## Notificação ao customer

- Se dados perdidos sem recovery: email formal em ≤ 24h.
- Template:
  - "Blobs afetados: [lista de digests]"
  - "Período de impact: [start] a [end]"
  - "Ação: re-upload necessário em builds que referenciam estes digests"
  - "Causa raiz: [breve descrição]"
  - "Prevenção: [patch + TLA+ update]"

## Post-mortem obrigatório

- Public incident report em ≤ 14d.
- TLA+ spec atualizada para cobrir o cenário violado.
- Grace period re-avaliado (72h mantido ou estendido para 96h?).
- Code review de GC: quem revisou PR que introduziu bug?

## Prevenção

- INV-GC-001 TLA+ CI obrigatório (CTRL-FORMAL-001).
- Chaos test semanal: inject race entre GC Mark e AC update.
- Dry-run semestral deste runbook (destructive — só em staging).
- PAT-SOFT-DELETE-001 garantem recovery dentro do grace window.
- Reconcile diário (CTRL-GC-002) detecta drift > 0.1%.
