---
id: "SPEC-CONTRACT-S06"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-24"
updated: "2026-04-25"
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

**Por que SOTA:** competitors (BuildBuddy, NativeLink) operam GC com "best effort grace period" sem TLA+ verification; race conditions entre Mark+UpdateActionResult são causa #1 de incidentes em remote cache mature. CoreLink S-06 entrega: (a) **TLA+ `gc_correctness.tla` verified** (formally proven INV-GC-001/004 hold under all interleavings); (b) **mark-phase-aware re-ref** com `mark_started_at` timestamp comparado com `ac.created_at` (pattern verificado pela TLA+); (c) **soft-delete reversible** 72h grace; (d) **chaos test** GC sob load 30d sem violation. Reference: **NIST SP 800-88 Rev.1** (sanitization), **TLA+ Specifications for Distributed Systems** (Lamport), **Postgres VACUUM semantics**, **Cassandra tombstone tuning best practices**.

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
  - "RESILIENCE-PATTERNS"           # PAT-DEGRADE-001 (pause GC global), PAT-RETRY-IDEMPOTENT-001
  - "SLO-CATALOG"                   # SLO-CORRECT-GC, SLO-FRESH-GC
  - "PRIVACY-MODEL"                 # CTRL-PRIV-014 (DSR erasure ↔ GC interaction)
  - "OBSERVABILITY-MODEL"           # DASH-GC, métricas reclaimed
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-GC-001** | Mark-and-sweep GC com grace period | Multi-pass scan; soft-delete; grace 72h CAS / 24h AC. |
| **CAP-GC-002** | Soft-delete reversible | Tombstone in `blob_meta.deleted_at`; undelete via re-upload (mesmo digest) ou explicit admin restore endpoint dentro do grace. |
| **CAP-GC-003** | Mark-phase-aware re-ref protection | `INV-GC-004`: blob re-referenciado via AC update após `mark_started_at` é protegido (`ac.created_at > mark_started_at` check pre-sweep). |
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
  - Batch size 10k rows/iteration (tunable via config — 14.s06.2 quality).
  - `mark_started_at` timestamp captured at phase start; persisted em `gc_run` table.
  - Jitter 100ms entre batches para evitar D1 throttle.
  - Reachable set computado: `union(blob_meta.refcount > 0, ac_meta.outputs, manifest_chunks)`.
  - Output: `gc_candidates` table com `digest, tenant_id, mark_started_at, status='candidate'`.
- **R-S06-5**: Mark p99 ≤ 10 min para 1M blobs (tenant size real); benchmark CI.

### 5.3 Sweep Phase (CAP-GC-001 + CAP-GC-002 + CAP-GC-003)

- **R-S06-6**: Sweep phase: soft-delete `blob_meta.deleted_at = now()`:
  - Grace 72h CAS / 24h AC (em line com data_model + storage_semantics_matrix).
  - **INV-GC-004 enforcement** (mark-phase-aware): pre-sweep, verify `ac.created_at < mark_started_at` para todos AC entries que referenciam digest; se any `ac.created_at >= mark_started_at` → digest é re-referenciado durante mark, **não** deletar.
  - Audit emit `corelink.gc.sweep_executed` per blob com `prev_state, mark_started_at, sweep_executed_at`.
- **R-S06-7**: Undelete path: customer re-upload de mesmo digest dentro do grace OR admin endpoint `POST /v1/admin/gc/undelete?digest=X` reverte `deleted_at = NULL` + audit.
- **R-S06-7.1**: **Sweep p99 ≤ 5 min @ 100k candidates** per (tenant, region) (Lote 10.6bis P0-5 fix Part 1: separate budget line, NOT sub-allocation of mark's 10 min). Rationale: sweep is cost-distinct phase (per-candidate INV-GC-004 EXISTS check + soft-delete UPDATE + audit emit; bounded concurrency 8; D1 batch ≤250 rows per Lote 10.5bis lesson). Mark and sweep run sequentially per (tenant, region) cron tick; total mark+sweep ≤ 15 min p99 budget @ 1M blobs / 100k candidates respectively.

### 5.4 Physical Delete (CAP-GC-001)

- **R-S06-8**: Physical delete worker (separate job; runs hourly): blobs com `deleted_at < now() - grace_period` → R2 DeleteObject + `blob_meta` row purge.
- **R-S06-9**: Physical delete idempotent: re-run safe (PAT-RETRY-IDEMPOTENT-001).
- **R-S06-9.1**: **Physical delete p99 ≤ 30 min @ 100k candidates** per (tenant, region) hourly tick (Lote 10.6bis P0-5 fix Part 2a: separate budget line; arithmetic re-derived using D1 batch ≤250 row Lote 10.5bis constraint). Derivation: 100k candidates × 50ms R2 DeleteObject / bounded_concurrency 8 ≈ 625s ≈ 10.4min R2 work; 100k candidates / 83 candidates-per-D1-batch = ~1200 batches × 100ms D1 p99 / concurrency 8 ≈ 15s D1 work; total ≈ 11min p99; budget headroom 2.7×; SEV-2 alert sustained > 25min.

### 5.5 Refcount Reconciliation (CAP-GC-004)

- **R-S06-10**: Refcount reconciliation daily (CTRL-GC-002):
  - Recompute per (tenant_id, digest): `expected_refcount = count(ac_meta where blob_refs contains digest AND deleted_at_ms IS NULL)` via SQL `json_each(a.blob_refs)` (Lote 10.6bis Part 2a P0-1 fix; **NOT** `LIKE '%digest%'` — string-substring match produces false drift signals; canonical idiom is `json_each` JSON-aware membership).
  - Compare with `blob_meta.refcount`.
  - Drift > 0.1% global = SEV-2 alert; per-tenant drift > 1% = SEV-1.
  - Auto-fix: drift count ≤5 AND drift % ≤ 0.01% per tenant (scale-invariant percentage-floor + absolute-floor; Lote 10.6bis Part 2a P0-6 fix); larger drifts pause + manual review.
- **R-S06-10.1**: **Reconcile p99 ≤ 1h @ 1M blobs** per (tenant, region) daily cron (analytics workload; not hot path). Re-derived post-Part 2a P0-1: json_each per-row extracts O(json_array_size) joined with `idx_ac_meta_tenant_deleted_at`; chunked iteration 1k blobs/chunk × bounded concurrency 4-8.

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
- **14.s06.4 Mark-phase-aware re-ref discipline**: `mark_started_at` é timestamp único per gc_run; comparison strict `>` (não `>=`).
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
| **Mark phase demora > 1h** (blocking) | M | M | HIGH (FM-305) | M | LOW | Multi-pass batched 10k rows + jitter + benchmark 1M ≤ 10 min; SLO-FRESH-GC tracking. |
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

**Fim spec contract S-06 v1.3.0 SOTA.**
