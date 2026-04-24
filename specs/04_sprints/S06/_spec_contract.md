---
id: "SPEC-CONTRACT-S06"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s06", "gc", "mark-sweep", "high-risk"]
---

# Spec Contract — S-06: Garbage Collection (Mark & Sweep)

## 0. Metadata

| Sprint ID | S-06 |
|---|---|
| Nome | Garbage Collection: Mark & Sweep + INV-GC-001/004 enforcement |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-011 (GC/reachability CRITICAL), FF-HR-005, FF-HR-006 (retention) |
| Duração estimada | 4 semanas |
| WIs antecipados | 7 |

## 1. Objetivo

Implementar GC para reclaim de blobs não-referenciados. CRITICAL porque bug deleta dado cliente = trust lost permanentemente. Multi-pass mark (para não bloquear writes) + sweep com grace 72h + soft-delete + mark-phase-aware re-ref protection. TLA+ `gc_correctness.tla` é CI gate obrigatório.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-011**: toca INV-GC-* CRITICAL — forcing factor específico criado pra este cenário.
- **FF-HR-005**: GC é controle de segurança (integridade de dados).
- **FF-HR-006**: GC afeta retention policy do customer.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-GC-001**: Mark-and-sweep GC com grace period 72h.
- **CAP-GC-002**: Soft-delete + tombstone reversible em 24h.
- **CAP-GC-003**: Mark-phase-aware re-ref protection (INV-GC-004).
- **CAP-GC-004**: Refcount reconciliation (CTRL-GC-002).
- **CAP-GC-005**: GC metrics + dashboard (DASH-GC).
- **CAP-GC-006**: Storage bytes reclaimed tracking (per-tier).

## 5. Requirements específicos

- **R-S06-1**: Worker-gc service (separate binary; non-blocking com CAS writes).
- **R-S06-2**: Mark phase: multi-pass scan `blob_meta` + `ac_meta` + `manifest_chunks`; batch ~10k rows; jitter.
- **R-S06-3**: Sweep phase: soft-delete `blob_meta.deleted_at = now`; grace 72h CAS / 24h AC.
- **R-S06-4**: Mark-phase-aware check: blob não deletado se AC com `created_at >= mark_started_at`.
- **R-S06-5**: Physical delete worker (separate job após grace period).
- **R-S06-6**: Refcount reconciliation daily (CTRL-GC-002).
- **R-S06-7**: TLA+ `gc_correctness.tla` CI gate bloqueia merge.

## 6. Definition of Done

- [ ] 7 WIs SEALED.
- [ ] TLA+ `gc_correctness.tla` verde; adicionalmente: property test em Rust cobrindo race Mark+UpdateActionResult.
- [ ] Chaos: rodar GC durante write load → zero falso positivo (no blob deletado com AC ativa).
- [ ] Undelete testado: soft-delete → undelete dentro do grace.
- [ ] RB-FM-300 dry-run executado em staging.
- [ ] PRR + adversarial review focado em GC correctness.
- [ ] Storage reclaimed measurable: 30d simulation em staging retorna > 0 bytes reclaimed.

## 7. Completeness Criteria (delta local)

- [ ] **10.s06.1** Mark p99 ≤ 10 min para 1M blobs (tenant size real).
- [ ] **10.s06.2** Zero blobs reachable deletados em 30d staging sustained.
- [ ] **10.s06.3** `corelink_gc_reclaimed_bytes_total` > 0 em workload simulado.

## 8. Invariants

- **INV-GC-001** (CRITICAL, TLA+): reachable never deleted — CORE do sprint.
- **INV-GC-002** (MEDIUM): orphan eventualmente deletado.
- **INV-GC-003** (HIGH): refcount consistency (reconcile diário).
- **INV-GC-004** (CRITICAL, TLA+): mark-phase-aware re-ref safe.

## 9. Quality Standards (delta local)

- **14.s06.1** GC worker idempotent: re-run safe (crashes mid-flight não corrompem).
- **14.s06.2** Batch size tunable via config (não hard-coded).
- **14.s06.3** Zero race conditions entre mark/sweep/write (formally verified TLA+).

## 10. Anti-scope

- ❌ Cross-tenant GC (não aplicável; GC é per-tenant).
- ❌ Aggressive GC (< 72h grace) — requer ADR + customer opt-in.
- ❌ Delete de audit log (imutável; retention separate — `auth_model §7.2`).

## 11. Dependencies

- **Blocker:** S-01 + S-02 + S-04 + S-05 SEALED (CAS + AC + multipart mature).
- **Blocker:** canonical source REMOTE-CACHE-PRODUCT-PROFILE + INVARIANT-REGISTRY ✅.

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S06-001 | Worker-gc binary skeleton + scheduler |
| WI-S06-002 | Mark phase: multi-pass scan D1 + batching |
| WI-S06-003 | Sweep phase: soft-delete + grace check + INV-GC-004 enforce |
| WI-S06-004 | Physical delete job (post-grace) |
| WI-S06-005 | Refcount reconciliation (CTRL-GC-002 daily) |
| WI-S06-006 | TLA+ CI gate + property test race Mark+UpdateAR |
| WI-S06-007 | DASH-GC dashboard + métricas reclaimed |

## 13. Duração

4 semanas (GC é CRITICAL, buffer extra); buffer 7 dias.

## 14. Critérios de promoção

- DoD + TLA+ verde + zero SEV-1 em 30d staging sustained.
- RB-FM-300 + RB-FM-404 dry-runs executados.
- Customer trust: "nunca perdi blob reachable em 30d" claim verificável.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Mark phase demora > 1h (blocking) | M | HIGH (FM-305) |
| INV-GC-001 violação produção (deleta reachable) | L | CRITICAL (FM-300) |
| Refcount drift > 0.1% | M | MEDIUM (FM-302 adjacente) |
| Sweep deleta tombstone antes undelete window | L | HIGH |

---

**Fim spec contract S-06.**
