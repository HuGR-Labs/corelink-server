---
id: "SPEC-CONTRACT-S07"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s07", "dedup", "eviction", "standard", "sota"]
---

# Spec Contract — S-07: Dedup + Eviction Policy (SOTA)

> **SOTA framing:** concorrentes atuais (NativeLink, BuildBuddy, JFrog, bazel-remote) tipicamente têm dedup intra-tenant via chunk hash index. Dedup cross-tenant é raro (BuildBuddy Enterprise tem como ADR-only feature). Eviction policy LRU + TTL é padrão. Meta: paridade operacional + métrica de dedup ratio superior (target ≥ 3× médio em workloads Docker).

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-07 |
| Nome | Dedup + Eviction Policy (intra-tenant default; cross-tenant backlog) |
| Lane | STANDARD |
| Lane forcing factors | — (nenhum CRITICAL touched); dependent of S-06 GC SEALED |
| Duração estimada | 2.5 semanas (80–100h efetivas) |
| WIs antecipados | 5 |

## 1. Objetivo

Implementar duas peças economicamente críticas: (a) **dedup de chunks intra-tenant** reaproveitando infraestrutura Merkle do S-05 (`manifest_chunks` + index D1) pra reduzir R2 bytes ≥ 3× em workloads Docker/ML-tipicos; (b) **eviction policy per-tier** com LRU + TTL + quota-trigger enforcing `tenant_quota` de `data_model.md §4.2`, respeitando soft-delete + grace period do GC (S-06) para nunca violar `INV-GC-001`. Entrega valor direto em storage cost + hit ratio.

Cross-tenant dedup (potencial redução adicional 2–4×) **não é escopo** — requer BYOE + ADR + privacy review por causa de CTRL-ISO-005 (dedup como existence oracle).

## 2. Lane + forcing factors

- **Lane:** STANDARD.
- **Rationale:** não toca invariantes CRITICAL; respeita INV-GC-001 via herança de enforcement do S-06; eviction é reversível dentro do grace period; worst-case é cliente retry-uploading chunk evicted.
- **Forcing factors:** nenhum. Elevação a HIGH_RISK **não** é exigida.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
  - "SECURITY-MODEL"   # CTRL-ISO-005 (dedup boundaries)
  - "INVARIANT-REGISTRY"
```

## 4. CAPs entregues

- **CAP-DEDUP-001**: Intra-tenant chunk dedup via `manifest_chunks` index; content-addressable means same chunk_digest → same R2 object reused.
- **CAP-DEDUP-002**: Dedup ratio métrica exposta (`corelink_dedup_ratio{tenant_id, type=chunk|blob}`).
- **CAP-DEDUP-003**: Dedup ratio per-tier dashboard (em DASH-CAS).
- **CAP-EVICT-001**: LRU eviction por tenant via `blob_meta.last_accessed_at` updated em hits; eviction worker bate 95%-of-quota trigger.
- **CAP-EVICT-002**: TTL-based AC entry expiry **per-tier (canonical defaults — supersedes S-04 CAP-AC-004 default 90d via ADR-0019)**: free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable (default 365d, max 730d).
- **CAP-EVICT-003**: **Storage soft-pressure eviction (renamed Lote 9.4 / ADR-0020)** — 80% soft-warn (email), 95% trigger eviction worker. **Hard-block 100% transferido para S-08 CAP-QUOTA-001** (rate-limit/429 layer); S-07 eviction reduz `bytes_used` antes do hit hard-block. Boundary: ≤ 95% = S-07 owns; ≥ 100% = S-08 owns.
- **CAP-EVICT-004**: Eviction is **soft-delete-first** (reuses GC grace period 72h) — cliente pode "undelete" via re-upload within grace.

## 5. Requirements específicos

- **R-S07-1**: Indexar `manifest_chunks (tenant_id, chunk_digest) UNIQUE INDEX` em D1 pra dedup lookup O(1) (< 2 ms p99).
- **R-S07-2**: Eviction worker `worker-evict` (CF scheduled, daily 02:00 UTC per region) + trigger ad-hoc quando tenant atinge 95% quota.
- **R-S07-3**: LRU tracking: `blob_meta.last_accessed_at` update on every GET (atomic D1 UPDATE ou DO singleton batch).
- **R-S07-4**: REAPI `FindMissingBlobs` consulta dedup index: retorna apenas chunks genuinamente ausentes, client skip re-upload de chunks existentes (core de CAP-DEDUP-001 benefit customer-facing).
- **R-S07-5**: TTL configurable per-tenant override via admin API (S-13 dep; stub interim).
- **R-S07-6**: DASH-DEDUP dashboard em `observability/dashboards/` com widgets: dedup ratio, bytes saved, eviction rate, tenant quota utilization heatmap.
- **R-S07-7**: Alert anomaly detection: dedup ratio cair > 30% WoW = inspect (pode indicar workload change ou bug em chunker).
- **R-S07-8**: Unit + integration tests: 90% coverage; property test (10k iter) cobrindo edge cases de eviction vs GC race.
- **R-S07-9**: Runbook `RB-FM-305` (tombstone lost) dry-run + `RB-FM-059` (DO quota exceeded) dry-run.

## 6. Definition of Done

Toda DoD universal (`_sprint_creation_contract §7`) **+**:

- [ ] 5 WIs SEALED.
- [ ] **Dedup ratio measurable**: ≥ 3 tenants em staging com workloads Docker pulls; `corelink_dedup_ratio{type=chunk}` ≥ 2.5× sustained 7d (target SOTA: ≥ 3×).
- [ ] **Eviction não viola INV-GC-001**: chaos test inject eviction during mark phase → zero reachable blob deleted (EVT-023).
- [ ] **Quota enforcement E2E**: load test push tenant até 100% quota → writes retornam `TENANT_QUOTA_EXCEEDED` 429 com Retry-After (EVT-024).
- [ ] **FindMissingBlobs optimization**: workload sintético reduz upload bytes em ≥ 50% vs baseline (EVT-004 benchmark).
- [ ] **Dashboard DASH-DEDUP** live em Grafana (EVT-013).
- [ ] **Alerts armados**: dedup ratio anomaly detection (SEV-3) + quota 95% breach (SEV-2 per-tenant).
- [ ] **Runbook dry-runs**: RB-FM-305 + RB-FM-059 executados em staging (EVT-017).
- [ ] **Coverage ≥ 90%** (EVT-003).
- [ ] **Property test 10k iter verde** cobrindo GC+Evict race (EVT-002).

## 7. Completeness Criteria SOTA (delta local)

Herda `_sprint_creation_contract §8`. Adiciona:

- [ ] **10.s07.1 Benchmark SOTA comparison**: dedup ratio comparado a NativeLink OSS (baseline ~2.1×) e BuildBuddy enterprise (~2.8× reported) — superior OU justificativa documentada.
- [ ] **10.s07.2 Eviction latency**: p99 de eviction decision < 50ms; physical delete async.
- [ ] **10.s07.3 Quota check latency**: middleware quota check adds < 3ms p99.
- [ ] **10.s07.4 CTRL-ISO-005 enforcement**: dedup cross-tenant é **OFF** by default + code path deliberately gated behind `dedup.cross_tenant.enabled=false` config (default false; future ADR gate).
- [ ] **10.s07.5 INV-QUOTA-ENFORCEMENT** verificada via property test.

## 8. Invariants (específicas + herdadas)

**Mantidas:**
- INV-TENANT-ISOLATION (CRITICAL, TLA+): dedup index é tenant-scoped (`(tenant_id, chunk_digest)` unique PER tenant).
- INV-QUOTA-ENFORCEMENT (HIGH): tenant real-time check; atomic via DO actor model.
- INV-GC-001 (CRITICAL): eviction respeita grace period 72h + soft-delete-first.
- INV-CAS-IMMUTABILITY (CRITICAL): eviction é metadata mark; body imutável até physical delete post-grace.

**Novas deste sprint:**
- **INV-DEDUP-CONSISTENCY** (HIGH — novo, adicionar ao invariant_registry): `(tenant_id, chunk_digest) → chunk_body` é 1:1 dentro do tenant; nenhum dup chunk_body para mesmo digest (enforced por UNIQUE index).

## 9. Quality Standards SOTA (delta local)

- **14.s07.1 Zero overhead no hot path de dedup**: check é O(1) index lookup; nunca scan D1.
- **14.s07.2 Eviction batch jitter ±10%** (PAT-JITTER-001) pra evitar thundering herd em 02:00 UTC.
- **14.s07.3 Tunable config**: batch size, trigger thresholds, TTLs — tudo via DO `config-singleton` (pós S-13; interim via env).
- **14.s07.4 Cascade prevention**: eviction NUNCA cascateia pra remover chunks ainda referenciados por manifest ativo (INV-GC-003 + INV-DEDUP-CONSISTENCY).
- **14.s07.5 Observability deep**: cada eviction decision emite log estruturado com `{tenant_id, chunk_digest_hex8, reason, bytes_reclaimed}`.
- **14.s07.6 Benchmark SOTA**: criterion benchmark published em `benches/eviction/history/` comparando against baseline; PR regression > 10% bloqueia merge.
- **14.s07.7 Cost regression gate** (Lote 9.5b — meta-contract §14.10 universal HIGH_RISK): per-op cost benchmark em $USD/million ops para hot path eviction worker (D1 query + R2 DeleteObject overhead); PR > 10% cost regression bloqueia merge sem ADR. Per-tenant cost projection sustained 7d staging em DoD.

## 10. Anti-scope

- ❌ **Cross-tenant dedup** (backlog post-GA). Privacy risk via existence oracle (CTRL-ISO-005). Requer BYOE + ADR novo.
- ❌ **Content-defined chunking** (FastCDC). S-05 estabeleceu fixed 2 MiB; redesign fora de escopo.
- ❌ **Delta compression** entre chunks similares (research-grade; pós-GA).
- ❌ **Zstd/LZ4 compression** antes de store. **Anti-scope GA** (Lote 9.5c — Codex R3-16 fix; era erroneamente "defer pra S-11" mas S-11 é privacy/DSR, não storage/perf). Compression é decisão pós-GA Q1 com ADR específica + benchmark vs storage/CPU trade-off (ROI duvidoso pra binários já compressed como Docker layers).
- ❌ **Eviction ML-based** (predictive LRU). Simple LRU baseline primeiro; ML se métrica justificar.

## 11. Dependencies

- **Blocker (hard):** S-05 SEALED — multipart + chunking foundation.
- **Blocker (hard):** S-06 SEALED — GC mark&sweep respeitando INV-GC-001; eviction **herda** esse enforcement.
- **Blocker (soft):** S-09 — observability stack pra emitir métricas dedup/eviction (pode stub interim se paralelo).
- **Bloqueia:** S-10 (billing precisa conhecer bytes reclaimed pra refund credits); S-14 (region expansion precisa dedup funcionar pra reduce replication bandwidth).

## 12. WIs antecipados (PERT estimates)

| ID | Título | PERT (h) |
|---|---|---|
| WI-S07-001 | Dedup index `manifest_chunks` + FindMissingBlobs otimizado | 20 |
| WI-S07-002 | Eviction worker LRU + TTL + quota trigger | 24 |
| WI-S07-003 | Quota enforcement middleware + error tipado | 16 |
| WI-S07-004 | `blob_meta.last_accessed_at` update hot path + property test race | 16 |
| WI-S07-005 | DASH-DEDUP + alertas anomaly detection | 12 |

Total: ~88h (~2.2 weeks 1 dev; 1.5 weeks 2 devs parcial). Buffer 5 dias cf. STANDARD defaults.

## 13. Duração + Timeline

- **Start:** após S-06 SEALED (depends on real staging validation). Assume 2026-08-10 (Mon).
- **Mid-check:** 2026-08-17 (Fri) — dedup index + quota enforcement running in staging.
- **Target complete:** 2026-08-27 (Thu).
- **Buffer:** 3 dias úteis.

## 14. Critérios de promoção ao próximo sprint (S-08)

- DoD complete.
- Dedup ratio ≥ 2.5× sustentado 7d em staging.
- RB-FM-305 + RB-FM-059 dry-runs limpos.
- Sprint retrospective criada.
- PRR não obrigatória (STANDARD), mas sprint review com sign-off 5 papéis.

## 15. Riscos (completo, com Prob/Det/Impacto/Exposure/Residual)

| ID | Risco | Prob | Det | Impacto | Exposure | Mitigação | Residual |
|---|---|---|---|---|---|---|---|
| R-S07-001 | Quota race condition (FM-059) sob alto throughput | M | M | HIGH | Tenant blocked write sem explicação | DO atomic counter + pessimistic check; property test race | LOW |
| R-S07-002 | Eviction deleta chunk ainda ref (FM-300 adjacente) | L | H | CRITICAL | Cross-customer impact se INV-GC-001 violated | Herda soft-delete + grace + reconcile (CTRL-GC-002); chaos test mandatory | LOW |
| R-S07-003 | Dedup ratio baixo em workload customer real | M | H | LOW (métrica, não deploy blocker) | Customer disappointment se advertise > real | Baseline em múltiplos workloads antes de claim público | LOW |
| R-S07-004 | D1 index size explode (milhões chunks) → latency degrade | M | M | MEDIUM | Dedup lookup p99 > 2ms | Monitor index size; sharding plan se needed (pós-GA) | MEDIUM |
| R-S07-005 | Anomaly detection false positives → pager fatigue | H | L | LOW | Oncall ignores future | Alert threshold tunable; 2-week tune-in period | LOW |
| R-S07-006 | LRU update em hot path adds latency write/read | M | L | MEDIUM | SLO-LAT-CAS-GET ou PUT regressão | Batch updates via DO; não é sync per op | LOW |

## 16. Benchmarks SOTA externos a citar

Sprint deve publicar comparação final vs:

- **NativeLink OSS** (https://github.com/TraceMachina/nativelink): dedup ratio baseline relatado ~2.1× em Bazel builds típicos.
- **BuildBuddy Enterprise**: claim ~2.8× com cross-tenant dedup. Nosso target: 3×+ intra-tenant (paridade defendível).
- **Bazel disk cache local**: baseline 1× (no dedup); mostra valor da remote cache com dedup.

## 17. References (standards / papers)

- **Merkle tree chunking**: Merkle (1979) *"Protocols for Public Key Cryptosystems"* — baseline conceitual.
- **FastCDC** (Xia et al., 2016): considered como anti-scope mas referenciado se eviction ML-based futuro.
- **Google SRE Workbook** Ch. 5 *"Alerting on SLOs"* — multi-burn-rate pattern aplicado em eviction alerts.
- **AWS S3 Lifecycle policy** best practices — eviction operational pattern.

## 18. Post-mortem hooks

Trigger post_mortem automático se:

- Cliente reporta "dado perdido" + eviction recent (combo pode indicar FM-300 violation).
- Dedup ratio cair > 50% WoW sem mudança intencional no chunker.
- Quota false-positive block em staging tests.

## 19. Waiver policy

Nenhum waiver previsto. Se sprint precisar relaxar requirement, criar waiver conforme `_templates/waiver.md` com expires_at ≤ 90d + compensating control.

---

**Fim de spec contract S-07 SOTA (v1.1).** Upgrade de v1.0 (compacto) para v1.1 (SOTA) executado em Lote 9.1 endereçando user intent "tudo SOTA, fucking awesome".
