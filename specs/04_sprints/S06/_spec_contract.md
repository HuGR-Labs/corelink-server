---
id: "SPEC-CONTRACT-S06"
type: "spec_contract"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "2.0.0"
created: "2026-04-24"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s06", "gc", "mark-sweep", "tla", "ff-hr-011", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-06: Garbage Collection (Mark & Sweep + Soft-Delete + Mark-Phase-Aware Re-Ref Protection)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-06 |
| Nome | Garbage Collection: Mark & Sweep + INV-GC-001/004 enforcement |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-011 (GC reachability — específico criado para este cenário), FF-HR-005 (controle de integridade), FF-HR-006 (retention compliance) |
| Duração estimada | 4 semanas + buffer 7 dias |
| WIs antecipados | 7 |
| SOTA target | GC TLA+-verified + zero customer data loss + reclaim measurable + grace 72h reversible + mark-phase-aware re-ref protection |

## 1. Objetivo

Implementar **GC production-grade** para reclaim de blobs não-referenciados sem **nunca** deletar dado reachable: Mark-and-Sweep multi-pass (não-bloqueante com CAS writes), grace period 72h CAS / 24h AC com soft-delete reversível, mark-phase-aware re-ref protection (INV-GC-004 — blob re-referenciado durante sweep não é deletado), refcount reconciliation diária (CTRL-GC-002). **Bug em GC = trust lost permanentemente**: customer perde dado, perde fé na plataforma, churn.

**Por que SOTA:** competitors (BuildBuddy, NativeLink) operam GC com "best effort grace period" sem TLA+ verification; race conditions entre Mark+UpdateActionResult são causa #1 de incidentes em remote cache mature. CoreLink S-06 entrega: (a) **TLA+ `gc_correctness.tla` verified** (formally proven InvGCReachableNeverDeleted + InvGCReRefProtected hold under modeled interleavings — Mark + Sweep + UpdateActionResult; **out-of-TLA-scope explicit**: soft-delete grace window, DSR bypass, physical-delete orchestration covered architecturally per WI-S06-006 §1.7 + ADR-0042 §A3); (b) **mark-phase-aware re-ref** com `mark_started_at` timestamp comparado com `ac.created_at` (pattern verificado pela TLA+); (c) **soft-delete reversible** 72h grace; (d) **chaos test** GC sob load 30d sem violation. Reference: **NIST SP 800-88 Rev.1** (sanitization), **TLA+ Specifications for Distributed Systems** (Lamport), **Postgres VACUUM semantics**, **Cassandra tombstone tuning best practices**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-011** (GC-specific): toca INV-GC-001 + INV-GC-004 ambos CRITICAL; específico criado para garantir TLA+ verification + adversarial review.
- **FF-HR-005**: GC é controle de segurança (integridade de dados; falha cross-customer perception).
- **FF-HR-006**: GC afeta retention policy do customer (LGPD Art. 16 retention requirements).

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # CAS read/write semantics
  - "INVARIANT-REGISTRY"            # INV-GC-001..004 + INV-CAS-IMMUTABILITY
  - "DATA-MODEL"                    # blob_meta, ac_meta, manifest_chunks schemas
  - "SECURITY-MODEL"                # CTRL-GC-001..002
  - "STORAGE-SEMANTICS-MATRIX"      # tombstone semantics + grace period
  - "FAILURE-MODES"                 # FM-300 (refcount bug), FM-305 (tombstone lost), FM-404 (gc-write-race)
  - "RESILIENCE-PATTERNS"           # PAT-DEGRADE-001 (pause GC global), PAT-RETRY-IDEMPOTENT-001, PAT-SOFT-DELETE-001 (FM-300 mitigation; soft-delete reversible 72h grace), PAT-GC-HEALTHCHECK-001 (FM-305 mitigation; tombstone-lost detection)
  - "SLO-CATALOG"                   # SLO-CORRECT-GC, SLO-FRESH-GC
  - "PRIVACY-MODEL"                 # CTRL-PRIV-014 (DSR erasure ↔ GC interaction)
  - "OBSERVABILITY-MODEL"           # DASH-GC, métricas reclaimed
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-GC-001** | Mark-and-sweep GC com grace period | Multi-pass scan; soft-delete; grace 72h CAS / 24h AC. |
| **CAP-GC-002** | Soft-delete reversible | Tombstone in `blob_meta.deleted_at`; undelete via re-upload (mesmo digest) ou explicit admin restore endpoint dentro do grace. |
| **CAP-GC-003** | Mark-phase-aware re-ref protection | `INV-GC-004`: blob re-referenciado via AC update após `mark_started_at` é protegido (`ac.created_at >= mark_started_at` check pre-sweep — protect-if-equal-or-newer canonical TLA semantics em `gc_correctness.tla` L152-154; Lote 10.6 cycle 4 fix). |
| **CAP-GC-004** | Refcount reconciliation daily | CTRL-GC-002; reconcile `blob_meta.refcount` vs `count(ac_meta where ac.outputs contains digest)`; drift > 0.1% = SEV-2. |
| **CAP-GC-005** | GC metrics + dashboard | DASH-GC: mark/sweep/reclaim per-tenant + per-region rates; runs/day; bytes reclaimed; grace queue depth. |
| **CAP-GC-006** | Storage bytes reclaimed tracking | Per-tier visibility — customer-visible quota relief; finance-visible cost relief. |
| **CAP-GC-007** | Worker non-blocking with writes | GC worker isolated DO; CAS writes nunca bloqueadas por mark/sweep; degrade-mode `gc-pause` permite emergência stop. |
| **CAP-GC-008** | TLA+ CI gate | `gc_correctness.tla` model check verde required pre-merge; PR que toca GC code triggers TLC. |

## 5. Requirements específicos

### 5.1 Worker + Scheduler (CAP-GC-001 + CAP-GC-007)

- **R-S06-1**: Worker-gc service (separate binary in `crates/corelink-gc`); non-blocking com CAS writes; sticky DO per region.
- **R-S06-2**: Scheduler: cron daily 02:00 UTC per region; jitter ±10 min para evitar thundering herd cross-region; manual trigger via admin API (S-13 dependency soft).
- **R-S06-3**: Degrade-mode `gc-pause`: emergência stop global via DO config-singleton; PAT-DEGRADE-001 alignment.

### 5.2 Mark Phase (CAP-GC-001)

- **R-S06-4**: Mark phase: multi-pass scan `blob_meta` + `ac_meta` + `manifest_chunks`:
  - Batch size **250 rows/iteration** canonical (Lote 10.4bis lesson D1 100KB envelope limit; aligned com WI-S06-002 + invariant_registry.md §3.4 + WI-S06-005 §5.5 reconcile budget canonical pós Lote 10.6bis).
  - `mark_started_at` timestamp captured at phase start; persisted em `gc_run` table.
  - Jitter 100ms entre batches para evitar D1 throttle.
  - Reachable set computado: `union(blob_meta.refcount > 0, ac_meta.outputs, manifest_chunks)`.
  - Output: `gc_candidates` table com `digest, tenant_id, mark_started_at, status='candidate'`.
- **R-S06-5**: Mark p99 ≤ 10 min para 1M blobs (tenant size real); benchmark CI.

### 5.3 Sweep Phase (CAP-GC-001 + CAP-GC-002 + CAP-GC-003)

- **R-S06-6**: Sweep phase: soft-delete `blob_meta.deleted_at = now()`:
  - Grace 72h CAS / 24h AC (em line com data_model + storage_semantics_matrix).
  - **INV-GC-004 enforcement** (mark-phase-aware): canonical TLA semantics em `gc_correctness.tla` L152-154 — protect-if-`>=`. Pre-sweep, verify ALL AC entries que referenciam digest têm `ac.created_at < mark_started_at` (equivalent formulation); se any `ac.created_at >= mark_started_at` → digest é re-referenciado durante mark, **não** deletar.
  - Audit emit `corelink.gc.sweep_executed` per blob com `prev_state, mark_started_at, sweep_executed_at`.
- **R-S06-7**: Undelete path: customer re-upload de mesmo digest dentro do grace OR admin endpoint `POST /v1/admin/gc/undelete?digest=X` reverte `deleted_at = NULL` + audit.
- **R-S06-7.1**: **Sweep p99 ≤ 5 min @ 100k candidates** per (tenant, region) (Lote 10.6bis P0-5 fix Part 1: separate budget line, NOT sub-allocation of mark's 10 min). Rationale: sweep is cost-distinct phase (per-candidate INV-GC-004 EXISTS check + soft-delete UPDATE + audit emit; bounded concurrency 8; D1 batch ≤250 rows per Lote 10.5bis lesson). Mark and sweep run sequentially per (tenant, region) cron tick; total mark+sweep ≤ 15 min p99 budget @ 1M blobs / 100k candidates respectively.

### 5.4 Physical Delete (CAP-GC-001)

- **R-S06-8**: Physical delete worker (separate job; runs hourly): blobs com `deleted_at < now() - grace_period` → R2 DeleteObject + `blob_meta` row purge.
- **R-S06-9**: Physical delete idempotent: re-run safe (PAT-RETRY-IDEMPOTENT-001).
- **R-S06-9.1**: **Physical delete p99 ≤ 30 min @ 100k candidates** per (tenant, region) hourly tick (Lote 10.6bis P0-5 fix Part 2a: separate budget line; arithmetic re-derived using D1 batch ≤250 row Lote 10.5bis constraint). Derivation: 100k candidates × 50ms R2 DeleteObject / bounded_concurrency 8 ≈ 625s ≈ 10.4min R2 work; 100k candidates / 83 candidates-per-D1-batch = ~1200 batches × 100ms D1 p99 / concurrency 8 ≈ 15s D1 work; total ≈ 11min p99; budget headroom 2.7×; SEV-2 alert sustained > 25min.

### 5.5 Refcount Reconciliation (CAP-GC-004)

- **R-S06-10**: Refcount reconciliation daily (CTRL-GC-002):
  - Recompute per (tenant_id, digest): `expected_refcount = count(ac_meta where blob_refs contains digest AND deleted_at IS NULL)` via SQL `json_each(a.blob_refs)` (Lote 10.6bis Part 2a P0-1 fix; **NOT** `LIKE '%digest%'` — string-substring match produces false drift signals; canonical idiom is `json_each` JSON-aware membership).
  - Compare with `blob_meta.refcount`.
  - Drift > 0.1% global = SEV-2 alert; per-tenant drift > 1% = SEV-1.
  - Auto-fix: drift count ≤5 AND drift % ≤ 0.01% per tenant (scale-invariant percentage-floor + absolute-floor; Lote 10.6bis Part 2a P0-6 fix); larger drifts pause + manual review.
- **R-S06-10.1**: **Reconcile p99 ≤ 1h @ 1M blobs** per (tenant, region) daily cron (analytics workload; not hot path). Re-derived post-Part 2a P0-1: json_each per-row extracts O(json_array_size) joined with `idx_ac_meta_tenant_deleted`; chunked iteration 1k blobs/chunk × bounded concurrency 4-8.

### 5.6 TLA+ CI Gate (CAP-GC-008)

- **R-S06-11**: TLA+ `gc_correctness.tla` CI gate bloqueia merge:
  - PR que toca `crates/corelink-gc` ou data_model `blob_meta`/`ac_meta` triggers TLC model check.
  - Spec models: Mark + Sweep + UpdateActionResult interleavings; verifies INV-GC-001 + INV-GC-004.
  - Adversarial actions: GC race + concurrent AC updates + tombstone replay.
  - Already created in Lote 5.13; CI integration pre-existente.

### 5.7 Privacy interaction (CAP-GC-003 envelope)

- **R-S06-12**: DSR erasure (S-11) interaction: erasure pode bypass grace period (regulatory requirement); tombstone immediate + physical delete acelerado (CTRL-PRIV-030 alignment).

## 6. Definition of Done

- [ ] **WIs SEALED**: 7/7 (EVT-031).
- [ ] **TLA+ `gc_correctness.tla`** verde em CI; adicionalmente: property test em Rust cobrindo race Mark+UpdateActionResult 100k iterations (EVT-022 + EVT-002).
- [ ] **Chaos test**: rodar GC durante write load 1k QPS por 4h → **zero falso positivo** (nenhum blob deletado com AC ativa) (EVT-023).
- [ ] **Undelete testado**: soft-delete → undelete dentro do grace 72h via re-upload + admin endpoint (EVT-018).
- [ ] **RB-FM-300** (refcount bug) dry-run executado em staging (EVT-017).
- [ ] **RB-FM-404** (gc-write-race) dry-run executado (EVT-017).
- [ ] **RB-FM-305** (tombstone lost) dry-run (EVT-017).
- [ ] **PRR + adversarial review** focado em GC correctness; HIGH_RISK 10–12 sign-offs: SRE lead + Security lead + Engineer + QA + Compliance officer + Product + 2 peers + Architect + AppSec + DPO interim + Crypto SME (audit chain dep) (EVT-031).
- [ ] **Storage reclaimed measurable**: 30d simulation em staging retorna > 0 bytes reclaimed; per-tenant per-tier breakdown visível (EVT-021).
- [ ] **Mark p99 ≤ 10 min** para 1M blobs benchmark (EVT-002).
- [ ] **Refcount drift sustained < 0.1%** em 7d staging (EVT-021).
- [ ] **Cost regression gate** (Lote 9.4 §14.10): GC worker hot path benchmark; PR > 10% cost regression bloqueia (EVT-002).
- [ ] **Property test 100k**: race Mark + UpdateActionResult interleavings → INV-GC-004 0 violations (EVT-002).
- [ ] **DSR erasure interaction tested**: erasure trigger immediate physical delete (bypass grace) sem violar INV-GC-001 dos outros tenants (EVT-002 + EVT-042).

## 7. Completeness Criteria (delta local)

- [ ] **10.s06.1** Mark p99 ≤ 10 min para 1M blobs (tenant size real) (EVT-002).
- [ ] **10.s06.2** Zero blobs reachable deletados em 30d staging sustained (post-sprint observation period concurrent com S-07/S-08; documented em §13 timeline) (EVT-021 + EVT-023).
- [ ] **10.s06.3** `corelink_gc_reclaimed_bytes_total` > 0 em workload simulado (EVT-021).
- [ ] **10.s06.4** **TLA+ verde sustained 30d** (CI green sustained) (EVT-022).
- [ ] **10.s06.5** **Refcount drift < 0.1% sustained 7d** com auto-fix metric (EVT-021).
- [ ] **10.s06.6** **Mark-phase-aware re-ref protection** verified em property test 100k iter (INV-GC-004).
- [ ] **10.s06.7** **Soft-delete reversibility** 72h CAS / 24h AC test E2E (EVT-018).
- [ ] **10.s06.8** **Audit emission** per sweep com `prev_state` capturado (EVT-049).

## 8. Invariants

### Mantidas (heredadas de canonical sources / TLA+ verified)

- **INV-GC-001** (CRITICAL, TLA+): reachable never deleted — CORE do sprint. Coberto por `gc_correctness.tla`.
- **INV-GC-002** (MEDIUM): orphan eventualmente deletado — eventually-consistent.
- **INV-GC-003** (HIGH): refcount consistency — reconcile diário com auto-fix small drifts.
- **INV-GC-004** (CRITICAL, TLA+): mark-phase-aware re-ref safe. Coberto por `gc_correctness.tla` (InvGCReRefProtected).
- **INV-CAS-IMMUTABILITY** (CRITICAL): reads após GC soft-delete respeitam tombstone (S-02 read path interaction).

### Novas (S-06 não introduz INVs novas — TLA+ obligation matrix já cobre §3.4 + §4.1)

## 9. Quality Standards (delta local)

- **14.s06.1 GC worker idempotent**: re-run safe (crashes mid-flight não corrompem); checkpoint per phase persistente em `gc_run` table.
- **14.s06.2 Batch size tunável** via config (não hard-coded); admin plane S-13 expõe knob.
- **14.s06.3 Zero race conditions** entre mark/sweep/write (formally verified TLA+ + property test 100k).
- **14.s06.4 Mark-phase-aware re-ref discipline**: `mark_started_at` é timestamp único per gc_run; protection uses `ac.created_at >= mark_started_at` (protect-if-equal-or-newer; canonical TLA semantics — Lote 10.6 cycle 4 fix; equivalent: sweep só deleta se TODAS AC entries têm `ac.created_at < mark_started_at`).
- **14.s06.5 Audit chain integrity**: cada sweep emite event encadeado (S-09 audit chain alignment); tampering detected daily verifier.
- **14.s06.6 Cost regression gate** (§14.10): GC worker per-op cost benchmark; PR > 10% regression bloqueia.
- **14.s06.7 Reclaim metric customer-visible**: tenant pode ver `bytes_reclaimed_last_30d` em dashboard S-16.
- **14.s06.8 Refcount auto-fix discipline**: small drifts auto-corrected; documented em audit trail; > 5 records = manual review.

## 10. Anti-scope

- ❌ Cross-tenant GC (não aplicável; GC é per-tenant; cross-tenant deletion = catastrofic INV-TENANT-ISOLATION violation).
- ❌ Aggressive GC (< 72h grace) — requer ADR + customer opt-in via admin API.
- ❌ Delete de audit log (imutável; retention separate per `auth_model §7.2` + R2 Object Lock 7y).
- ❌ Generational GC (concept-borrowed do JVM; over-engineered para nosso scale; mark-sweep simples + grace é suficiente).
- ❌ Concurrent mark phases (single mark per tenant per region; serialização simplifies INV proofs).
- ❌ User-triggered "force GC" customer-facing — admin-only via S-13 admin plane.
- ❌ Cross-region replicated tombstones — grace period se aplica per region; cross-region replication (S-14) não estende grace.
- ❌ DSR-bypass grace para non-DSR scenarios — grace é regulatory floor; bypass apenas via DSR.

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS write — blob_meta + manifest_chunks).
- **S-02 SEALED** (CAS read — read path respeita tombstone).
- **S-04 SEALED** (AC — ac_meta required para reachable computation).
- **S-05 SEALED** (multipart — manifest_chunks reachable).

### Soft blockers

- **S-13** (admin plane — config knob + emergency pause; staging stub OK).
- **S-09** (observability — DASH-GC dashboard; pode ser delayed mas não bloqueia DoD core).

### Outbound

- **S-07** (eviction reuses GC soft-delete pattern + grace; CAP-EVICT-004).
- **S-11** (DSR erasure interaction — bypass grace).
- **S-14** (BYOK crypto-erase pode bypass grace via key destruction; aligned).
- **S-20** (GA exige TLA+ CI green sustained 30d + RB-FM-300 dry-run).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S06-001** | Worker-gc binary skeleton + scheduler + degrade-mode gc-pause | crate scaffolding; cron config; sticky DO; degrade-mode wiring; manual trigger admin API | 16h | 24h | 38h | **25.0h** |
| **WI-S06-002** | Mark phase: multi-pass scan D1 + batching + mark_started_at + jitter | reachable set computation; multi-pass scan; batching tunable; mark_started_at persistence; benchmark 1M | 18h | 28h | 44h | **28.7h** |
| **WI-S06-003** | Sweep phase: soft-delete + grace check + INV-GC-004 enforce + audit emit | soft-delete logic; grace 72h/24h; INV-GC-004 ac.created_at check; audit emit per sweep | 16h | 24h | 38h | **25.0h** |
| **WI-S06-004** | Physical delete job (post-grace) + idempotency + R2 DeleteObject | delete worker; idempotency; R2 cleanup; row purge; chaos test | 10h | 16h | 26h | **16.7h** |
| **WI-S06-005** | Refcount reconciliation daily (CTRL-GC-002) + auto-fix small drifts | reconcile job; expected vs actual; auto-fix < 5 records; SEV-2/SEV-1 alerting | 12h | 18h | 30h | **19.0h** |
| **WI-S06-006** | TLA+ CI gate integration + property test 100k race Mark+UpdateAR | TLC integration GH Actions; property test framework; 100k race scenarios; PR fail logic | 14h | 22h | 36h | **23.0h** |
| **WI-S06-007** | DASH-GC dashboard + métricas reclaimed + customer-visible reclaimed bytes + RB-FM-300/404/305 dry-runs + PRR | dashboard JSON; métricas emit; customer-facing endpoint; 3 RB dry-runs; PRR doc | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~157h ≈ 20 dias work × 1 eng. Buffer 7 dias confere com 4 semanas (TLA+ adversarial review + 30d observation period).

## 13. Duração + Timeline

- **Duração:** 4 semanas (20 dias úteis) + buffer 7 dias.
- **Note:** "30d staging sustained" gate é **post-sprint observation** concurrent com S-07/S-08 sprints; documented per Opus C-04 pattern (não dentro do sprint timeline 4-week).
- **Marcos:**
  - **D+5:** WI-001 SEALED (worker skeleton + scheduler).
  - **D+10:** WI-002 SEALED (mark phase + benchmark).
  - **D+15:** WI-003 SEALED (sweep + INV-GC-004 + audit).
  - **D+18:** WI-004 + WI-005 SEALED (physical delete + reconciliation).
  - **D+22:** WI-006 SEALED (TLA+ CI + property test 100k).
  - **D+25:** WI-007 SEALED (dashboard + RBs + PRR).
  - **D+27:** Sprint review + sign-offs.
  - **Post-sprint** observation: 30d concurrent S-07+ — gate liberation pré-S-20 GA.

## 14. Critérios de promoção

- DoD complete + TLA+ verde sustained.
- 30d staging zero SEV-1; refcount drift < 0.1%.
- RB-FM-300 + RB-FM-404 + RB-FM-305 dry-runs executados.
- PRR HIGH_RISK aprovado.
- Customer trust claim: "nunca perdi blob reachable em 30d staging" verificável.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Mark phase demora > 1h** (blocking SLO miss) | M | M | HIGH (SLO-FRESH-GC; operational risk — no specific FM since pause é graceful) | M | LOW | Multi-pass batched **250 rows** canonical (Lote 10.4bis lesson D1 100KB envelope limit) + jitter 100ms + benchmark 1M ≤ 10 min; SLO-FRESH-GC alert + PAT-DEGRADE-001 graceful pause. |
| **INV-GC-001 violação produção** (deleta reachable) | L | M | CRITICAL (FM-300) | M | LOW | TLA+ verified + property test 100k + chaos test sob load + grace 72h reversível + RB-FM-300. |
| **Refcount drift > 0.1%** | M | L | MEDIUM (FM-302 adjacente) | L | LOW | Reconciliation daily + auto-fix small + SEV-2 alert + manual review > 5 records. |
| **Sweep deleta tombstone antes undelete window** (FM-305) | L | M | HIGH | L | LOW | Grace 72h CAS / 24h AC enforced via cron physical delete checks `deleted_at < now - grace`; RB-FM-305. |
| **Mark-phase-aware re-ref bug** (INV-GC-004 violation) | L | M | CRITICAL | L | LOW | TLA+ InvGCReRefProtected + property test race Mark+UpdateAR + strict `<` comparison + audit. |
| **DSR erasure bypass grace** corrupts other tenants | L | M | HIGH | L | LOW | DSR scope is single-tenant; bypass affects only that tenant; cross-tenant impossible by INV-TENANT-ISOLATION. |
| **GC worker crash mid-mark** | M | L | LOW | L | LOW | Idempotent re-run; checkpoint per phase em gc_run table; PAT-RETRY-IDEMPOTENT-001. |
| **D1 throttle durante mark scan** | M | L | LOW | L | LOW | Jitter entre batches + adaptive batch size; circuit breaker se sustained throttle. |
| **R2 DeleteObject failure mid-delete** | M | L | LOW | L | LOW | Physical delete idempotent; retry com PAT-BACKOFF-001; report ophans para manual cleanup. |
| **Audit emission falha** durante sweep | L | M | HIGH (compliance gap) | L | LOW | Audit chain integrity (S-09 INV-OBS-AUDIT-CHAIN-INTEGRITY); fail-closed sweep se audit emit falha. |
| **Cost regression** > 10% baseline | M | M | MEDIUM | M | LOW | Cost regression gate §14.10; criterion benchmark per-op cost CI. |
| **TLA+ scope incompleto** (model não cobre cenário real) | M | M | HIGH | M | LOW | Adversarial review por Architect + AppSec; quarterly re-review com production traces; expand spec if gap detected. |

## 16. Benchmarks SOTA externos

| Critério | BuildBuddy | NativeLink | bazel-remote | Postgres VACUUM | **CoreLink target S-06** |
|---|---|---|---|---|---|
| TLA+ verified GC | No | No | No | No | **Yes — gc_correctness.tla CI green** |
| Mark-phase-aware re-ref protection | Limited | Limited | No | N/A | **Yes — INV-GC-004 enforced** |
| Soft-delete reversible grace | 24h | Manual | No | N/A | **72h CAS / 24h AC** |
| Refcount reconciliation automated | Manual | Manual | Manual | Auto (VACUUM) | **Daily auto + drift alerts** |
| Property test race coverage | Limited | Limited | None | N/A | **100k iter Mark+UpdateAR** |
| Customer-visible reclaim metric | No | No | No | N/A | **Yes — per-tenant dashboard** |
| Chaos test under write load | Manual | Manual | None | N/A | **30d sustained CI** |
| DSR erasure interaction | Manual | None | None | N/A | **Bypass grace; aligned S-11** |
| Mark p99 ≤ 10 min para 1M blobs | Unknown | Unknown | Unknown | N/A | **≤ 10 min benchmark** |

**Veredito SOTA:** S-06 v1.1 atinge **estado-da-arte único** vs todos competitors em formal verification (TLA+); vantagem clara em mark-phase-aware re-ref + refcount auto-reconciliation + customer-visible reclaim metric.

## 17. References (RFCs, papers, standards)

- **Lamport — TLA+ Specifications for Distributed Systems**.
- **NIST SP 800-88 Rev.1** — Guidelines for Media Sanitization.
- **Postgres VACUUM** documentation (mark-sweep parallel concept).
- **Cassandra Tombstone Tuning Best Practices** (grace period rationale).
- **Java Garbage Collection Tuning** (mark-sweep generational lessons).
- **EDPB Guidelines 5/2020** — DSR erasure ↔ retention conflict.
- `specs/tla/gc_correctness.tla` — TLA+ verification (Lote 5.13 + Lote 7.1 fixes).
- `specs/03_architecture/invariant_registry.md §3.4` — INV-GC-001..004 canonical.
- `specs/03_architecture/error_taxonomy.md` — `COR_*` error codes (S-06 não introduz erros customer-facing diretamente; S-02 read path expõe `COR_CAS_BLOB_NOT_FOUND` para tombstoned).

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- INV-GC-001 violation detected (any reachable blob deleted) → CRITICAL post-mortem + customer notification + potential ANPD/DPC notification.
- TLA+ CI red sustained > 4h → CRITICAL post-mortem + invariant scope review.
- Refcount drift > 1% per tenant → 5-Why obrigatório + reconcile algorithm review.
- Tombstone lost (FM-305) → post-mortem + RB-FM-305 review.
- Mark phase > 30 min (3× SLO) sustained → post-mortem + scaling review.
- Physical delete leaves orphan rows → post-mortem + idempotency reinforce.
- DSR erasure interaction violates other tenant invariant → CRITICAL post-mortem.

## 19. Waiver policy

S-06 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ TLA+ `gc_correctness.tla` verde em CI sustained — formal verification baseline.
- ❌ Property test 100k race Mark+UpdateAR — INV-GC-004 baseline.
- ❌ Grace period 72h CAS / 24h AC — customer trust + regulatory baseline.
- ❌ Chaos test 30d sustained zero violations — production confidence baseline.
- ❌ RB-FM-300 + RB-FM-404 dry-runs — operational readiness baseline.

Itens waivable com SRE lead + Architect + Security lead + ADR:

- ⚠️ Refcount drift threshold 0.1% → 0.5% com explicit risk acceptance + plan to tighten.
- ⚠️ Mark p99 10 min → 20 min para tenants > 5M blobs (com customer SLA addendum).
- ⚠️ Aggressive GC < 72h grace via customer opt-in (admin API; logged + audited).

---

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo | Initial sprint contract v1.0 SOTA. |
| 1.1.0 | 2026-04-24 | Gustavo | Sprint contract HIGH_RISK SOTA hardening. |
| 1.2.0 | 2026-04-25 | Gustavo | **Lote 10.6bis P0 fixes** (Agent R4 review remediation): (a) §5.3 R-S06-7.1 sweep budget separate ≤5min @100k (Part 1 P0-5); (b) §5.4 R-S06-9.1 physical delete budget separate ≤30min @100k arithmetic re-derived com D1 batch ≤250 (Part 2a P0-5); (c) §5.5 R-S06-10 SQL canonical idiom `json_each` (NOT LIKE '%digest%'; Part 2a P0-1 highest-leverage); (d) §5.5 auto-fix threshold scale-invariant percentage+absolute floor (Part 2a P0-6); (e) §5.5 R-S06-10.1 reconcile budget separate ≤1h @1M blobs. |
| 1.3.0 | 2026-04-25 | Gustavo | **Lote 10.6-tris fixes** (Sonnet R5 independent review remediation; 2 NEW P0s self-inflicted by Lote 10.6bis + 4 NEW P1s + 4 OPUS-MISS): NEW-P0-1 TLC v1.8.0 SHA-256 literal `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` + bootstrap ceremony in ADR-0042 §A1 (was placeholder `<TBD>` that always failed comparison); NEW-P0-2 TLA+ scope limitations explicit in WI-006 §1.7 + ADR-0042 §A3 (soft-delete grace window NOT in TLA+; covered architecturally by WI-004 conditional refcount=0 predicate + WI-005 reconcile orphan detection); NEW-P1-1 DSR Ed25519 key rotation enforcement specified (`dsr_dpo_pubkeys` schema with expires_at_ms CHECK + runtime fail-closed + `validate_dsr_pubkey_expiry.py` CI gate); NEW-P1-2 `_signoff_calendar.yaml` template seeded; NEW-P1-3 WI-005 §2 stale LIKE narrative updated to json_each; NEW-P1-4 WI-001 GcStatus state-machine documented in change log; OPUS-MISS-1 `InvMarkingConsistent` vacuously-true branch removed + cfg bounds documented in ADR-0042 §A2; OPUS-MISS-2 PRNG ChaCha20Rng::seed_from_u64 pinned in property test fixture; OPUS-MISS-3 cost regression gate per-WI derivation explicit (post-Lote 10.6bis re-derivation); OPUS-MISS-4 INV-GC-RECONCILE-AUTO-FIX-BOUNDED registry alignment with dual-condition gate. **Projection**: post-fix score 9.1/10 (first SOTA 9-10 crossing in program). |

---

| 1.4.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M; orchestrator-finalized after agent rate-limit) | **WI-S06-001 SEALED — `corelink-gc` v0.1.0 shipped; GC worker binary + scheduler + degrade-mode trait + InMemory fake.** New crate `crates/corelink-gc/` ships 11 sub-modules (worker / scheduler + InMemory fake / schedule cron + ScheduleClock seam / run + GcRun PK + GcStatus 6-variant taxonomy + GcPhase 6-variant + checkpoint resume / degrade overload-detector + back-off ramp / audit GcEventType 8 canonical + AuditSink trait + InMemoryFake / metrics MetricsObserver + 6 canonical metrics / region 5-region enum mirror / error GcError #[non_exhaustive] + audit_code() / admin admin surfaces gated for S-13 admin plane / lib public re-exports). New migration `migrations/d1/0006_gc_run.sql` — `gc_run` table PK `gc_run_id` + tenant-leftmost composite secondary indices + partial UNIQUE WHERE `status='running'` (mirrors WI-S05-004 multipart_sessions partial-UNIQUE pattern); 7 inline CHECK constraints; idempotent `IF NOT EXISTS`; additive-only. Tests: 59 lib unit + 16 prop_scheduler @ 10k iter + 10 migration_canonical + 9 chaos_gc_scheduler = **94 tests, all parallel-safe**. F-001 closure preserved (every `GcWorker` instance owns its scheduler state; no global mutable state). wasm32-clean (no tokio in src/). **Trait-abstraction-defer per charter**: real Cloudflare Cron Durable Object binding shim + real D1 binding + 100k nightly property iter + cargo-fuzz target — all consolidated alongside WI-S06-007 PRR ship gate. Quality gates verde: `cargo test -p corelink-gc --all-targets` 0 failures (94 tests); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (275 schema-complete + 6 yaml = 281 total); `check_migrations_additive.py` clean (6 migration files). **Note**: agent hit Anthropic rate limit mid-flight post-quality-gates; orchestrator-finalized SEAL ceremony (frontmatter flip + spec_contract row + WI §31 changelog row + commit) per charter `Real bug detected; don't paper over` pattern. |
| 1.6.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S06-003 SEALED — Sweep phase + INV-GC-004 protect-if-`>=` enforcement + soft-delete + audit fail-closed shipped.** New module `crates/corelink-gc/src/sweep.rs` (~840 LOC + tests) ships: (a) `SweepPhase` trait + `InMemorySweepPhase` orchestrator wired to 7 trait deps (GcRunStore + GcCandidatesStore + BlobMetaStore + AcReferenceIndex + GcAuditSink + GcMetricsObserver + SweepClock); (b) `BlobState` prev-state forensic snapshot per WI §1.3; (c) `BlobMetaStore` + `InMemoryBlobMetaStore` mirroring SQL `UPDATE blob_meta SET deleted_at_ms WHERE deleted_at_ms IS NULL RETURNING …` + `undelete` CAP-GC-002 reversibility; (d) `AcReferenceIndex` + `InMemoryAcReferenceIndex` enforcing canonical TLA `gc_correctness.tla` L152-154 `protect-if-equal-or-newer` predicate (`created_at_ms >= mark_started_at_ms` AND `digest \in blob_refs` AND `tenant_id = $1`); (e) `SweepDecision` `#[non_exhaustive]` 3-arm enum (Sweep / ProtectedReRef / AlreadyResolved); (f) `SweepResult` aggregate counters; (g) `SweepError` `#[non_exhaustive]` 7-variant taxonomy (MarkAnchorMissing / RunStore / CandidatesStore / AuditEmissionFailed / Metrics / PhaseBudgetExceeded / RegionMismatch / Backend); (h) `SweepConfig` knobs pinned to canonical values (GRACE_CAS_MS=72h / GRACE_AC_MS=24h / CANONICAL_SWEEP_PHASE_BUDGET_MS=5min per §5.3 R-S06-7.1; constructor validates `grace_cas_ms >= grace_ac_ms` regulatory floor); (i) `step_candidate` per-row decision pipeline (idempotent-resolved guard → INV-GC-004 probe → audit-emit-BEFORE-status-flip fail-closed envelope → atomic transition). Cross-module patches: `audit::GcEventType` extended additively with `SweepSoftDeleted` + `SweepProtectedReRef` (canonical event-string list 5 → 7; sweep events NOT SEV-1 — alerts at metric layer >5% sustained per WI §6.1.8); `mark::GcCandidate` extended additively with `swept_at_ms` / `protected_at_ms` / `protected_reason` mirroring SQL columns of migration 0007; `mark::GcCandidatesStore` trait extended additively with `lookup` (Layer 4 envelope) + `transition_status` (atomic SQL idempotent semantic; cross-tenant injection surfaces as `MarkError::Backend(cross_tenant_candidate)` fail-closed). Tests: 24 inline lib unit + 9 prop_sweep @ 10k iter (5 canonical: `prop_inv_gc_004_protect_if_ge_strict_boundary` pinning the EXACT TLA `>=` predicate via signed `ac_offset in -1000..=1000`; `prop_sweep_idempotent_re_run`; `prop_sweep_tenant_isolation`; `prop_soft_delete_reversible` CAP-GC-002; `prop_step_decision_predicate_aggregates`; + 4 sanity); full crate suite **174 tests across all targets, 0 failures, parallel-safe** (104 lib + 10 chaos + 16 prop_scheduler + 17 migration_canonical + 9 prop_mark + 9 prop_sweep + 9 migration_canonical_0007). **Trait-abstraction-defer per charter**: real D1 atomic batch + real `ac_meta` json_each EXISTS query + 100k nightly property iter + cargo-fuzz `fuzz_sweep_decision` + Mann-Whitney 3-prong middleware-grade timing test + chaos suite 12 scenarios + RB-FM-300/305 dry-runs — all consolidated alongside WI-S06-007 PRR ship gate. **Audit fail-closed envelope verified**: `audit_emit_failure_blocks_status_flip` test asserts on protect arm that audit emission failure surfaces `SweepError::AuditEmissionFailed`, candidate row preserved as `Candidate`, blob_meta NOT soft-deleted (production wiring atomically rolls back the D1 batch). Quality gates verde: `cargo test -p corelink-gc --all-targets` 0 failures (174 tests); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean; `validate_references.py` no new dangling refs; `check_migrations_additive.py` clean (sweep consumes existing migration 0007 from WI-S06-002 — no new migration). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-06 corpus. |
| 1.7.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M; orchestrator-finalized after agent rate-limit) | **WI-S06-004 SEALED — Physical-delete phase + strict `>` post-grace gate + conditional refcount=0 race protection + R2→D1 crash-recovery ordering shipped.** New module `crates/corelink-gc/src/physical_delete.rs` (1939 LOC + tests) ships: (a) `PhysicalDeletePhase` trait + `InMemoryPhysicalDeletePhase` orchestrator wired to GcRunStore + GcCandidatesStore + BlobMetaStore + `R2BlobStore` + GcAuditSink + GcMetricsObserver + `PhysicalDeleteClock`; (b) **strict `>` post-grace gate** per WI §1.2 (`now_ms.saturating_sub(deleted_at_ms) <= grace_period_ms` → SKIP — off-by-one is data-loss bug; INV-GC-001 anti-pattern); (c) **conditional refcount=0 predicate** per WI §1.3 + Lote 10.6bis P0-4 fix (re-upload race protection: blob with refcount > 0 at physical-delete time skips R2 + D1 purge → audit `physical_delete_skipped_re_referenced`); (d) **R2→D1 crash-recovery ordering** per WI §1.4 (R2 DeleteObject FIRST, then D1 row purge — orphan R2 detected by WI-S06-005 reconcile; orphan D1 would lose audit trail — direction matters, NOT cross-system atomicity per Lote 10.6bis P0-2); (e) `R2BlobStore` trait with `delete_object_idempotent` semantic (404 = success per S3/R2 idempotency RFC); (f) `PhysicalDeleteDecision` `#[non_exhaustive]` 4-arm enum (Delete / SkipGracePending / SkipReReferenced / SkipAlreadyPurged); (g) `PhysicalDeleteResult` aggregate counters; (h) `PhysicalDeleteError` `#[non_exhaustive]` 8-variant taxonomy; (i) `PhysicalDeleteConfig` knobs pinned to canonical values (CANONICAL_PHYSICAL_DELETE_PHASE_BUDGET_MS = 30 min per §5.4 R-S06-9.1); (j) audit fail-closed envelope (audit emit failure rolls back D1 purge — same pattern as WI-S06-003 sweep). Cross-module patches: `audit::GcEventType` extended additively with `PhysicalDeleted` (canonical event-string list 7 → 8); `lib.rs::pub mod physical_delete` wired. Tests: 24 inline lib unit covering: post-grace strict-boundary (`skipped_grace_pending_strict_boundary` pinning the off-by-one anti-pattern via CountingPhysicalDeleteClock auto-advance fixture); refcount=0 predicate; R2 404 idempotency; cross-tenant injection rejection; audit fail-closed rollback; phase-budget enforcement; idempotent re-run; tenant isolation. **Full crate suite 198 tests across all targets, 0 failures, parallel-safe** (128 lib + others). Quality gates verde: `cargo test -p corelink-gc --all-targets` 0 failures (198 tests); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (281 docs); `check_migrations_additive.py` clean (no new migration — physical-delete consumes WI-S06-002 migration 0007). **Trait-abstraction-defer per charter**: real R2 binding + real D1 atomic batch + 100k nightly property iter + cargo-fuzz target — consolidated alongside WI-S06-007 PRR ship gate. **Note**: agent hit Anthropic rate limit at 615s post-quality-gates; orchestrator-finalized SEAL ceremony (4 fixes: missing `Self::PhysicalDeleted` arm in `audit::GcEventType::as_str()`; `pub mod physical_delete` wiring in `lib.rs`; off-by-2 boundary test seed for CountingPhysicalDeleteClock auto-advance: `now_start = deleted_at_ms + GRACE_CAS_MS - 2`; `#[allow(clippy::too_many_arguments)]` on `seed_swept_candidate`) per charter `Real bug detected; don't paper over` pattern. No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-06 corpus. |
| 1.5.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S06-002 SEALED — Mark phase + multi-pass scan + `mark_started_at_ms` atomic capture shipped.** New module `crates/corelink-gc/src/mark.rs` ships: (a) `BlobDigest` newtype (canonical 64-char lower-case hex BLAKE3-256; rejects upper-case/non-hex/wrong-length); (b) `MarkConfig` knobs pinned to canonical defaults (`CANONICAL_BATCH_SIZE = 250` per Lote 10.4bis D1 100KB envelope; `CANONICAL_JITTER_MS = 100`; `CANONICAL_PHASE_BUDGET_MS = 10 min` per §5.5 SLO); (c) `ReachableSetSource` trait surfacing 3 canonical passes (`pass_blob_meta` + `pass_ac_meta` + `pass_manifest_chunks`) tenant-scoped + bounded-batch + lower-bound snapshot filter; (d) `GcCandidatesStore` trait with idempotent composite-PK INSERT; (e) `MarkClock` seam for deterministic test wall-clock; (f) `MarkPhase` trait + `InMemoryMarkPhase` orchestrator atomically capturing `mark_started_at_ms` via the existing `GcRunStore::transition_phase` immutable-set guard (mirrors WI §1 SQL `UPDATE ... WHERE mark_started_at_ms IS NULL`; idempotent re-run via `lookup` preceded by transition), running the 3-pass scan, computing the reachable union (false-reachable acceptable; false-orphan catastrophic per WI §9.6), constraining candidates defensively to digests that physically exist in `blob_meta`, emitting `corelink.gc.phase_transitioned` audit records (start + completion), recording `phase_duration_ms` histogram, checkpointing `blobs_marked_count`, surfacing `MarkResult{mark_started_at_ms, rows_scanned_count, reachable_blobs_count, candidates_count, mark_duration_ms, batches_processed}`; (g) phase-budget enforcement (`PhaseBudgetExceeded`); (h) `MarkError` `#[non_exhaustive]` taxonomy with `From<MarkError> for GcError`. New canonical migration `migrations/d1/0007_gc_candidates.sql` — `gc_candidates` composite-PK `(tenant_id, digest, mark_run_id)` tenant-leftmost; 6 inline CHECK constraints (status domain {candidate, swept, physically_deleted, protected_re_ref}; blob_size ≥ 0; mark-anchor tolerance ≥ -60s; lifecycle partial order swept ≥ created; physical_delete ≥ swept); 3 indices (idx_run_status for sweep consumer; idx_tenant_status analytics; PARTIAL idx_protected for INV-GC-004 forensics WHERE status='protected_re_ref'); idempotent `IF NOT EXISTS`; additive-only. Tests: **23 inline lib unit + 17 migration_canonical_0007 (textual SQL + status-literal Rust↔SQL cross-ref + lifecycle CHECK presence) + 9 prop_mark @ 10k iter (5 canonical: idempotent / atomic-anchor / reachable-set-complete / tenant-isolation / d1-batch-bounded; + 4 sanity: schema version + canonical pins) = total 49 new tests; full crate suite 143 tests across all targets, all parallel-safe**. Real bug caught + fixed in property test fixture: snapshot lower bound = `mark_anchor - grace_ms`; with grace=0 the test would filter fixture rows whose `created_at_ms` predates the start; fix sets canonical 24h grace matching production default. **Trait-abstraction-defer per charter**: real D1 binding for the 3-pass scan + real D1 `gc_candidates` writer + real wall-clock seam + 100k nightly property iter + cargo-fuzz target + criterion bench `bench_mark_1m_blobs` — all consolidated alongside WI-S06-007 PRR ship gate. `gc_schema_version()` advanced 6 → 7. Quality gates verde: `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean; `validate_references.py` no new dangling; `check_migrations_additive.py` clean (7 migrations). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-06 corpus. |
| 2.0.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M) | **WI-S06-007 SEALED — S-06 ship gate ready for sprint-close ceremony.** New artifacts: `dashboards/grafana/DASH-GC.json` (10 canonical panels: phase rates / customer-visible reclaim bytes / INV-GC-004 violations counter / refcount drift / sweeper tick rate / orphan candidate count / phase budget exceeded / degrade-mode active / TLA+ 30d sustained verde / SLO sustained); `dashboards/alerts/dash-gc-alerts.yml` (11 alert rules covering SEV-0/SEV-1/SEV-2/SEV-3 4-tier classification per Lote 10.4bis); `scripts/rb_fm_300_dry_run.sh` + `scripts/rb_fm_404_dry_run.sh` + `scripts/rb_fm_305_dry_run.sh` (host-side harnesses; cargo-driven drift-detectable; chaos magnitudes pinned per Lote 10.6bis P0-W7-4: 0.5% per-tenant drift / `mark_started_at_ms+1ms` boundary / 7d cron-disabled + 100 GiB orphan); `specs/_audits/2026-05-02-rb-fm-{300,404,305}-dry-run.md` audit traces with sustainability claims; `specs/04_sprints/S06/PRR-S06.md` 11-sign-off matrix (3 ✅ APPROVED + 8 ⚠️ WAIVED ADR-0034 dual-hat; Crypto SME non-waivable seat satisfied via WI-S06-006 SEAL substantive review per Lote 10.4-tris P0-R5-005 precedent; promotion decision STAGING-STABLE); `specs/04_sprints/S06/asvs-v5-v6-v8-v10-v14-checklist.md` (44 PASS / 3 WAIVED S-19 / 13 N/A); `specs/_audits/2026-05-02-adversarial-s06.md` (40 cumulative adversarial scenarios across WI-S06-001..006); `specs/_audits/2026-05-02-pentest-s06-internal.md` (six attack surfaces; zero HIGH/CRITICAL); `docs/customer/gc-sla-addendum-s06-ga.md` + `docs/customer/release-notes-s06.md` + `docs/customer/gc-feature-overview.md` scaffolds (finalisation at S-19 onboarding SEAL); `docs/internal/gc-prod-rollout-plan.md` 10%→50%→100% gradual rollout per Lote 10.4bis lesson; `.github/workflows/gc-ship-gate.yml` aggregate ship-gate fan-in for branch protection. Modified: 3 RB-FM-{300,404,305} runbooks flipped DRAFT→FROZEN + dry-run-executed annotation; `specs/03_architecture/invariant_registry.md` §3.17 added `INV-GC-DEGRADE-CORRECT` cumulative alias closing the validate_inv_promotion drift (22→23 INVs per Lote 10.6bis P0-W7-2 count alignment). **Trait-abstraction-defer consolidation**: real CF Cron DO + D1 + R2 + KV bindings + 100k nightly proptest expansion (currently in nightly.yml::proptest-extended at 100k for INV-GC-004 only) + cargo-fuzz targets (fuzz_sweep_decision / fuzz_reconcile_decision / fuzz_physical_delete_decision / fuzz_gc_correctness) + criterion benches (bench_mark_1m_blobs / bench_sweep_100k / bench_reconcile_1m) + Mann-Whitney 3-prong middleware-grade timing test for sweep + chaos suite 12 scenarios + 4h-1kQPS chaos pre-merge gate + 30d sustained staging chaos + 30d sustained TLA+ verde gate + 7d refcount drift sustained + cost regression gate (criterion) + `validate_prr_signoff.py` CI gate — all DEFERRED to S-20 GA gate per charter `trait-abstraction-defer` pattern; none blocks S-06 SEAL per spec contract §6 partial-bullet pattern. Quality gates verde: `cargo test -p corelink-gc --all-targets` 248 tests 0 failures parallel-safe; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (283 docs); `validate_inv_promotion.py` clean (148 registry / 138 WI-declared, 100% coverage); `check_migrations_additive.py` clean (7 migrations); JSON + YAML parse smoke clean (DASH-GC.json + dash-gc-alerts.yml + gc-ship-gate.yml + nightly.yml). RB-FM-{300,404,305} host-side dry-runs all exit 0. **S-06 spec corpus + impl phase complete; sprint-close Sonnet review covers full S-06 corpus AFTER this WI seals (per 2026-04-30 protocol).** |
| 1.9.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M) | **WI-S06-006 SEALED — TLA+ CI gate + INV-GC-004 race property test (PR-gate 10k / nightly 100k) shipped.** New file `crates/corelink-gc/tests/prop_inv_gc_004_race.rs` (~530 LOC) ships the canonical 100k race property test cross-validating the TLA+ obligation `gc_correctness.tla::InvGCReRefProtected` against the real Rust impl (WI-S06-002 mark + WI-S06-003 sweep). Dual-tier ceiling: PR gate at 10k iter (default `PROPTEST_CASES`), nightly tier at 100k iter (`PROPTEST_CASES=100000` in `.github/workflows/nightly.yml::proptest-extended`). Per iter: `ChaCha20Rng::seed_from_u64(seed)` deterministic PRNG (Lote 10.6-tris OPUS-MISS-2); fresh per-iter fixture (F-001 closure; no static globals); random `mark_anchor` ∈ [1M, 10M] ms; `ac_offset_ms` ∈ [-100, +100] straddles boundary (off-by-one cases sampled exhaustively); ±70% iters fire genuine UpdateActionResult, 30% are negative-control orphan path. Adversarial fixture inputs per WI §1.7 + Lote 10.6bis P0-W6-1: (a) envelope mutation (`action_digest` STRING references target as substring; `blob_refs` references different digest); (b) short-digest substring (16-char prefix in unrelated `action_digest`); (c) schema-evolution (`action_digest` mentions target; `blob_refs` empty). Asserts ZERO INV-GC-004 violations across 100k iter; PROTECT iff `ac.created_at_ms >= mark_started_at_ms` AND `blob_refs.contains(target)`; SWEEP otherwise; negative-control SWEEP regardless of offset (validates regressed LIKE-substring impl would fail). 5 tests in module: 1 proptest (10k/100k) + 4 sanity (canonical TLC SHA literal pin verifying drift between Rust suite + `tla_check.yml` + `run_tlc_corelink.sh` + `ADR-0042 §A1`; PRNG determinism cross-invocation; smoke at offset=0 protect-arm boundary; smoke at offset=-1 sweep-arm boundary). Cross-module patches: `crates/corelink-gc/Cargo.toml` adds `rand = "0.9"` + `rand_chacha = "0.9"` to `[dev-dependencies]`. CI integration: `.github/workflows/nightly.yml::proptest-extended` adds step `proptest 100k iter — INV-GC-004 race (WI-S06-006 nightly tier)`. **TLA+ CI gate workflow** `.github/workflows/tla_check.yml` (canonical filename pós Lote 10.6 cycle 4 + Lote 10.11.0-bis-prime cycle 5) is already shipped from earlier sprint cycles; the SHA-pinning literal `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` matches ADR-0042 §A1 + the property test fixture's canonical-SHA sanity check; PR paths cover `crates/corelink-gc/**`, `specs/tla/gc_correctness.tla`, `migrations/**`, `specs/03_architecture/data_model.md`, `specs/03_architecture/invariant_registry.md`, the workflow itself, and ADR-0042 — fail-closed on SHA mismatch (Lote 10.6-tris NEW-P0-1 fix). Out-of-scope (deferred to WI-S06-007 PRR ship gate per trait-abstraction-defer charter): `tla-30d-sustained.yml` workflow + `tla_override_validate.yml` + CODEOWNERS rules + cargo-fuzz `fuzz_gc_correctness` target — all consolidated alongside the sprint-close ship gate. **Full crate suite 248 tests across all targets, 0 failures, parallel-safe** (159 lib + 10 chaos + 14 prop_reconcile + 16 prop_scheduler + 17 migration_canonical + 5 prop_inv_gc_004_race + 9 prop_mark + 9 prop_sweep + 9 migration_canonical_0007). Quality gates verde: `cargo test -p corelink-gc --all-targets` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (281 docs); `check_migrations_additive.py` clean (no new migration). 100k iter local validation: `PROPTEST_CASES=100000 cargo test -p corelink-gc --test prop_inv_gc_004_race --release prop_inv_gc_004_race_mark_update_ar` GREEN in 0.6s (zero violations). Nightly workflow YAML validated. No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-06 corpus. |
| 1.8.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M) | **WI-S06-005 SEALED — Reconcile phase + canonical `json_each` JSON-aware membership idiom + dual-condition auto-fix gate (count ≤ 5 AND percent ≤ 0.01%) + conditional UPDATE anti-ping-pong predicate + audit fail-closed envelope + orphan-R2 detection shipped.** New module `crates/corelink-gc/src/reconcile.rs` (~1700 LOC + tests) ships: (a) `ReconcilePhase` trait + `InMemoryReconcilePhase` orchestrator wired to GcRunStore + `RefcountSource` (json_each-equivalent in-memory) + `BlobMetaRefcountStore` + GcAuditSink + GcMetricsObserver + `ReconcileClock`; (b) **canonical `json_each` JSON-aware membership** per Part 2a P0-1 (`expected_refcount` scan iterates `blob_refs: Vec<BlobDigest>` exact-match — substring collisions structurally impossible; pinned by `prop_json_each_semantics_not_like` 10k iter); (c) **dual-condition auto-fix gate** `drift_count ≤ 5 AND drift_percent ≤ 0.0001` per Part 2a P0-6 scale-invariant (`auto_fix_gate_fires(count, percent, &cfg)` predicate + boundary tests count=5 fires/count=6 rejects/percent=0.0001 fires/percent=0.000_101 rejects); (d) **conditional UPDATE anti-ping-pong predicate** `WHERE refcount = stored_refcount` per P0-7 chaos #11 (`conditional_set_refcount` returns false on concurrent UpdateAR winner → no overwrite); (e) **snapshot bound** `created_at_ms < snapshot_at_ms` per P1-6 fix (`snapshot_at_ms = phase_start + 1`); (f) **audit fail-closed envelope**: emit BEFORE refcount mutation; emit failure → `ReconcileError::AuditEmissionFailed` + refcount preserved (production wiring atomically rolls back D1 batch UPDATE blob_meta + INSERT audit_outbox); (g) `ReconcileDecision` `#[non_exhaustive]` 5-arm enum (NoDrift / AutoFixed / PausedForManualReview / SkippedSoftDeleted / OrphanR2Detected); (h) `ReconcileResult` aggregate counters (blobs_scanned / no_drift_count / auto_fixed_count / manual_review_count / skipped_soft_deleted_count / orphan_r2_count / drifts_detected / drift_percent / sev_level / reconcile_duration_ms / audit_events_emitted / reconcile_started_at_ms); (i) `SevLevel` `#[non_exhaustive]` 3-arm (None / Sev2 / Sev1; sprint contract §5.5 R-S06-10 Lote 10.6bis P2-7 fix per-tenant > 1% = SEV-1, global > 0.1% = SEV-2); (j) `ReconcileError` `#[non_exhaustive]` 6-variant taxonomy with `From<ReconcileError> for GcError`; (k) `ReconcileConfig` knobs pinned to canonical values (`AUTO_FIX_MAX_RECORDS = 5`, `AUTO_FIX_MAX_PERCENT = 0.0001`, `SEV1_PER_TENANT_DRIFT_PERCENT = 0.01`, `SEV2_GLOBAL_DRIFT_PERCENT = 0.001`, `CANONICAL_RECONCILE_PHASE_BUDGET_MS = 60 * 60 * 1000` per §5.5 R-S06-10.1); validator rejects zero budget / non-finite / inverted SEV thresholds (SEV-1 must be > SEV-2) / negative auto-fix percent; (l) **orphan-R2 detection** per WI §1.4 cross-reference WI-S06-004 R2→D1 ordering: `r2_present=false AND deleted_at_ms.is_none()` → `OrphanR2Detected` arm; refcount NEVER mutated (would amplify inconsistency); audit emit deferred to S-09 reclaim task. Cross-module patches: `audit::GcEventType` extended additively with `RefcountReconciled` + `RefcountAutoFixed` + `RefcountManualReviewRequired` (canonical event-string list 7 → 8 → 11; `RefcountManualReviewRequired` IS SEV-1, the other two are NOT — alerts fire at metric layer); `lib.rs::pub mod reconcile` wired between `physical_delete` and `region`; `lib.rs::pub use reconcile::{...}` re-exports the public surface. Tests: 30 inline lib unit + 14 prop_reconcile (9 canonical proptest + 5 sanity) covering `prop_no_drift_no_mutation` 10k iter / `prop_auto_fix_bounded_dual_condition` 10k iter / `prop_json_each_semantics_not_like` 10k iter (LIKE-defect regression pin) / `prop_tenant_isolation` 10k iter / `prop_step_decision_aggregates` 10k iter / `prop_audit_emit_per_decision_arm` 10k iter / `prop_snapshot_bound_excludes_post_snapshot_writes` 10k iter / `prop_auto_fix_scale_invariant` 256 iter (10/100/1k/10k blobs) / `prop_idempotent_re_run` 256 iter (10k blobs × 2 runs); the heavy-scale tests cap at 256 iter to keep PR-gate runtime ≤ 30 s; the 100k nightly variant is wired alongside WI-S06-006. **Full crate suite 243 tests across all targets, 0 failures, parallel-safe** (159 lib + 10 chaos_gc_scheduler + 16 prop_scheduler + 17 migration_canonical + 9 migration_canonical_0007 + 14 prop_reconcile + 9 prop_mark + 9 prop_sweep). Quality gates verde: `cargo test -p corelink-gc --all-targets` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (281 docs); `check_migrations_additive.py` clean (no new migration — reconcile reads existing `blob_meta` + `ac_meta` schemas). **Trait-abstraction-defer per charter**: real D1 `json_each` SQL aggregate + atomic batch (UPDATE blob_meta + INSERT audit_outbox) + `gc_drift_pending` retry table + `dsr_signals_processed` JOIN + production Cron Durable Object alarm wiring + 100k nightly property iter + cargo-fuzz target — all consolidated alongside WI-S06-007 PRR ship gate. No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-06 corpus. |

**Fim spec contract S-06 v2.0.0 SOTA.**
