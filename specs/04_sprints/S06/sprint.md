---
id: "S-06"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-011", "FF-HR-005", "FF-HR-006"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
tags: ["sprint", "s06", "gc", "mark-sweep", "tla-verified", "ff-hr-011", "high-risk"]
---

# Sprint S-06 — Garbage Collection (Mark & Sweep + TLA+ verified + Mark-phase-aware re-ref protection)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-25**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA)

> **🚦 Phase boundary:** Fase 1 — Remote Cache.
> **🛡️ FF-HR-011 specific**: GC reachability é forcing factor único criado para este sprint.

---

## 1. Objetivo

Implementar **GC production-grade** que reclaim blobs não-referenciados sem **nunca** deletar dado reachable: Mark-and-Sweep multi-pass não-bloqueante, grace 72h CAS / 24h AC com soft-delete reversível, mark-phase-aware re-ref protection (INV-GC-004), refcount reconciliation diária com auto-fix small drifts, TLA+ `gc_correctness.tla` CI gate. **Bug em GC = customer trust permanently lost**.

## 2. Escopo

### 2.1 In-scope

- **WI-S06-001**: Worker-gc binary skeleton + scheduler + degrade-mode `gc-pause`.
- **WI-S06-002**: Mark phase multi-pass scan D1 + batching 250 rows canonical (Lote 10.4bis lesson D1 100KB envelope; aligned _spec_contract §5.2) + jitter + `mark_started_at` persistence.
- **WI-S06-003**: Sweep phase soft-delete + grace check + INV-GC-004 enforce + audit emit.
- **WI-S06-004**: Physical delete job (post-grace) + idempotency.
- **WI-S06-005**: Refcount reconciliation daily (CTRL-GC-002) + auto-fix small drifts.
- **WI-S06-006**: TLA+ CI gate integration + property test 100k race Mark+UpdateActionResult.
- **WI-S06-007**: DASH-GC dashboard + métricas reclaimed customer-visible + RB-FM-300/404/305 dry-runs + PRR.

### 2.2 Anti-scope

- ❌ Cross-tenant GC (per-tenant only).
- ❌ Aggressive GC (< 72h grace) sem ADR + customer opt-in.
- ❌ Delete audit log (R2 Object Lock 7y).
- ❌ Generational GC (over-engineering).
- ❌ User-triggered force GC customer-facing (admin S-13 only).

## 3. Customer Impact & Journey

**JTBD:** "Como tenant, preciso garantia que GC **nunca** deleta blob reachable (mesmo durante write storms ou race com AC update); como Finance, preciso de reclaim measurable per tenant per tier."

**CAPs entregues:** CAP-GC-001..008 (mark-and-sweep + soft-delete reversível + INV-GC-004 + refcount reconciliation + DASH-GC + customer-visible reclaim + non-blocking + TLA+ CI gate).

## 4. Capability Mapping

Ver `_spec_contract.md §4`. Foundation: TLA+ gc_correctness.tla green sustained.

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S06-D1 | Worker-gc skeleton + scheduler + degrade-mode | `crates/corelink-gc/` | Cron daily 02:00 UTC + jitter; sticky DO; `gc-pause` flag PAT-DEGRADE-001 |
| S06-D2 | Mark phase | `crates/corelink-gc/src/mark.rs` | Multi-pass batched 250 rows canonical (D1 envelope; Lote 10.4bis); mark_started_at persistente em gc_run; benchmark 1M ≤ 10 min |
| S06-D3 | Sweep phase + INV-GC-004 + audit | `crates/corelink-gc/src/sweep.rs` | Soft-delete + grace 72h CAS / 24h AC; INV-GC-004 enforcement: `ac.created_at >= mark_started_at` protects (canonical TLA `gc_correctness.tla` L152-154; equivalent: delete only if all `ac.created_at < mark_started_at`); CloudEvent emit per sweep |
| S06-D4 | Physical delete job | `crates/corelink-gc/src/physical_delete.rs` | Idempotent re-run; R2 DeleteObject; row purge |
| S06-D5 | Refcount reconciliation | `crates/corelink-gc/src/reconcile.rs` | Daily; auto-fix < 5 records; SEV-2 alert > 0.1%; SEV-1 > 1% |
| S06-D6 | TLA+ CI gate + property test 100k | `.github/workflows/tla_check.yml` + `tests/prop_gc_race.rs` | TLC integration triggers PR em corelink-gc; race Mark+UpdateAR 100k iter green (canonical workflow filename pós Lote 10.6 cycle 4) |
| S06-D7 | DASH-GC + reclaimed métrica + RBs + PRR | `observability/dashboards/dash_gc.json` + `PRR-S06.md` | Dashboard live; customer-visible bytes_reclaimed_last_30d em S-16; 3 RB dry-runs; PRR 11 sign-offs |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Storage

- D1 `blob_meta.deleted_at` tombstone semantics.
- R2 DeleteObject post-grace.
- New table `gc_run` track mark_started_at + checkpoint.

### 6.2 Invariants

- **INV-GC-001** (CRITICAL, TLA+): reachable never deleted — `gc_correctness.tla`.
- **INV-GC-002** (MEDIUM): orphan eventually deleted.
- **INV-GC-003** (HIGH): refcount consistency reconcile.
- **INV-GC-004** (CRITICAL, TLA+): mark-phase-aware re-ref safe — `InvGCReRefProtected`.
- **INV-CAS-IMMUTABILITY** (CRITICAL): reads após soft-delete respeitam tombstone.

### 6.3 Privacy interaction

- DSR erasure (S-11) bypass grace period (regulatory); CTRL-PRIV-030 alignment.

### 6.4 SLOs

- SLO-CORRECT-GC: zero reachable deleted em 30d staging.
- SLO-FRESH-GC: mark p99 ≤ 10 min para 1M blobs.

## 7. Definition of Done (HIGH_RISK + 30d post-sprint observation)

- [ ] 7 WIs SEALED (EVT-031).
- [ ] TLA+ `gc_correctness.tla` verde em CI (EVT-022).
- [ ] Property test em Rust race Mark+UpdateActionResult 100k iter (EVT-002).
- [ ] Chaos: GC durante write load 1k QPS por 4h → zero falso positivo (EVT-023).
- [ ] Undelete testado: soft-delete → undelete dentro do grace via re-upload + admin endpoint (EVT-018).
- [ ] RB-FM-300 (refcount bug) dry-run (EVT-017).
- [ ] RB-FM-404 (gc-write-race) dry-run (EVT-017).
- [ ] RB-FM-305 (tombstone lost) dry-run (EVT-017).
- [ ] PRR HIGH_RISK 11 sign-offs: SRE + Security + Engineer + QA + Compliance + Product + 2 peers + Architect + AppSec + DPO interim + Crypto SME (audit chain) (EVT-031).
- [ ] Storage reclaimed measurable: 30d simulation > 0 bytes per tenant per tier (EVT-021).
- [ ] Mark p99 ≤ 10 min para 1M blobs (criterion EVT-002).
- [ ] Refcount drift sustained < 0.1% em 7d staging (EVT-021).
- [ ] Cost regression gate §14.10 (EVT-002).
- [ ] DSR erasure interaction tested: bypass grace sem violar INV-GC-001 outros tenants (EVT-002 + EVT-042).
- [ ] **30d post-sprint observation period** concurrent S-07/S-08 sprints (documented em §13 timeline; gate liberation pré-S-20 GA).

## 8. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS write blob_meta + manifest_chunks).
- **S-02 SEALED** (CAS read tombstone respect).
- **S-04 SEALED** (AC ac_meta required reachable computation).
- **S-05 SEALED** (multipart manifest_chunks reachable).

### Soft blockers

- S-13 admin plane (config knob + emergency pause; staging stub OK).
- S-09 observability (DASH-GC; nice-to-have).

### Outbound

- S-07 (eviction reuses GC soft-delete pattern + grace; CAP-EVICT-004).
- S-11 (DSR erasure bypass grace).
- S-14 (BYOK crypto-erase).
- S-20 (GA exige TLA+ CI green sustained 30d + RB-FM-300 dry-run).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-05 SEALED).
- **Mid-check**: D+13.
- **Sprint close**: D+27 (4 semanas + 7 dias buffer).
- **Post-sprint observation period**: 30d concurrent S-07+ — gate liberation pré-S-20 GA. **NOTA**: 30d obs é wall-clock concurrent, não dentro do sprint timeline 4-week.

## 10. Risk Register

Ver `_spec_contract.md §15` (12 risks 6-col com Owner per item).

## 11. Observability Plan

DASH-GC com mark/sweep/reclaim per-tenant per-region rates; runs/day; bytes reclaimed; grace queue depth; customer-facing endpoint S-16.

## 12. Security & Privacy

STRIDE: tampering com tombstone → INV-CAS-IMMUTABILITY violation. LINDDUN: DSR-bypass single-tenant scope; cross-tenant impossível por INV-TENANT-ISOLATION.

## 13. Post-mortem hooks

INV-GC-001 violation / TLA+ CI red sustained > 4h / refcount drift > 1% / tombstone lost / mark > 30 min / physical delete orphan / DSR erasure interaction violation.

## 14. Sign-off (HIGH_RISK 10–12)

11 roles incl. AppSec + DPO interim + Crypto SME (audit chain dep).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-06 (Lote 9.5b). |

---

**Fim de S-06 sprint contract.**
