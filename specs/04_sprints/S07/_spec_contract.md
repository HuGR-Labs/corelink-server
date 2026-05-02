---
id: "SPEC-CONTRACT-S07"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.9.0"
created: "2026-04-24"
updated: "2026-05-02"
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

Implementar duas peças economicamente críticas: (a) **dedup de chunks intra-tenant** reaproveitando infraestrutura Merkle do S-05 (`chunks` table com PK `(tenant_id, chunk_digest)` + `refcount`; pre-existing S-05 WI-S05-004 schema; **NO new index/migration needed** per Lote 10.7bis P0-1) pra reduzir R2 bytes ≥ 3× em workloads Docker/ML-tipicos; (b) **eviction policy per-tier** com LRU + TTL + quota-trigger enforcing `tenant_quota` de `data_model.md §4.2`, respeitando soft-delete + grace period do GC (S-06) para nunca violar `INV-GC-001`. Entrega valor direto em storage cost + hit ratio.

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

- **CAP-DEDUP-001**: Intra-tenant chunk dedup via `chunks` table PK `(tenant_id, chunk_digest)` + `refcount` (pre-existing S-05 schema; Lote 10.7bis P0-1 canonical — was incorrectly `manifest_chunks UNIQUE INDEX` which would break dedup since same chunk_digest legitimately appears em multiple manifests); content-addressable means same chunk_digest → same R2 object reused.
- **CAP-DEDUP-002**: Dedup ratio métrica exposta (`corelink_dedup_ratio{tenant_id, type=chunk|blob}`).
- **CAP-DEDUP-003**: Dedup ratio per-tier dashboard (em DASH-CAS).
- **CAP-EVICT-001**: LRU eviction por tenant via `blob_meta.last_accessed_at` updated em hits; eviction worker bate 95%-of-quota trigger.
- **CAP-EVICT-002**: TTL-based AC entry expiry **per-tier (canonical defaults — supersedes S-04 CAP-AC-004 default 90d via ADR-0019)**: free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable (default 365d, max 730d).
- **CAP-EVICT-003**: **Storage soft-pressure eviction (renamed Lote 9.4 / ADR-0020 FROZEN)** — 80% telemetry SEV-3 oncall (Lote 10.7bis P0-4 fix: was '80% soft-warn email' but email infra defers to S-13 admin/notifications), 95% trigger eviction worker. **Hard-block 100% transferido para S-08 CAP-QUOTA-001** (rate-limit/429 + Retry-After layer); S-07 eviction reduz `bytes_used` antes do hit hard-block. Boundary: ≤ 95% = S-07 owns; ≥ 100% = S-08 owns.
- **CAP-EVICT-004**: Eviction is **soft-delete-first** (reuses GC grace period 72h) — cliente pode "undelete" via re-upload within grace.

## 5. Requirements específicos

- **R-S07-1**: Dedup lookup via existing `chunks` PK `(tenant_id, chunk_digest)` + `refcount` (S-05 WI-S05-004 schema; Lote 10.7bis P0-1 — NO new migration needed); O(1) PK lookup < 2 ms p99.
- **R-S07-2**: Eviction worker `worker-evict` (CF scheduled, daily 02:00 UTC per region) + trigger ad-hoc quando tenant atinge 95% quota.
- **R-S07-3**: LRU tracking: `blob_meta.last_accessed_at` update on every GET via DO singleton batch (canonical per WI-S07-004 §6 hot-path; Lote 10.7bis: per-GET atomic D1 UPDATE rejected — would saturate D1 1k writes/s budget).
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
- [ ] **Quota enforcement E2E** (S-07/S-08 boundary per ADR-0020 FROZEN): S-07 eviction triggers ≤95%; S-08 hard-block 429 + Retry-After ≥100% (S-08 scope; integration test cross-sprint EVT-024).
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

**Novas deste sprint** (6 NEW INVs total per Lote 10.7-tris cycle 4 canonical):

§3.12 (Sprint-driven invariants — S-07 dedup):
- **INV-DEDUP-CONSISTENCY** (HIGH; registry §3.12 L162; canonical section ref Lote 10.7-tris cycle 6): `(tenant_id, chunk_digest) → chunk_body` é 1:1 dentro do tenant; nenhum dup chunk_body para mesmo digest (enforced por `chunks` PK from S-05).

§3.18 (Dedup + Eviction + Quota domain — Lote 10.7 S-07 sprint NEW group; canonical section ref Lote 10.7-tris cycle 6):
- **INV-EVICT-SOFT-DELETE-FIRST** (HIGH; registry §3.18 L371): Eviction sets `blob_meta.deleted_at`; NEVER R2 DELETE direct (reuses S-06 GC grace 72h via WI-S06-004 physical-delete).
- **INV-EVICT-CASCADE-PREVENTED** (HIGH; registry §3.18 L372): Pre-evict reachable check refuses if blob ref'd by active `ac_meta.blob_refs` (BLOB-scope per Lote 10.7bis P0-8).
- **INV-EVICT-TTL-CAP-RESPECTED** (MEDIUM; registry §3.18 L373): Enterprise TTL ≤ 730d hard cap (CAP-EVICT-002 boundary).
- **INV-LRU-CONSISTENCY** (HIGH; registry §3.18 L374): Eviction respects authoritative `last_accessed_at` via DO buffered + D1 base UNION.
- **INV-QUOTA-RESERVATION-TTL** (HIGH; registry §3.18 L375): Pending reservations auto-release after size-proportional TTL `min(7d, max(60s, req_bytes/1MB/s × 2))`.

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
| WI-S07-001 | Dedup lookup via `chunks` PK + FindMissingBlobs otimizado (Lote 10.7bis P0-1: pre-existing S-05 schema; NO new migration) | 20 |
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

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo (Lote 9.1) | Initial sprint contract S-07 (Dedup + Eviction Policy; STANDARD lane). |
| 1.1.0 | 2026-04-24 | Gustavo (Lote 9.1 SOTA elevation) | SOTA upgrade endereçando "tudo SOTA, fucking awesome": 19 sections; benchmarks externos NativeLink + BuildBuddy; 6-row risk register; SLA targets; ADR-0019/0020 references. |
| 1.6.0 | 2026-04-28 | Gustavo (Lote 10.7-tris cycle 6 codex SEAL remediation) | **4 codex 8.4 blockers + 2 secondary fixed**: (a) Metric naming Prometheus convention clarified (CloudEvent dotted spec → underscored Prometheus exposed; added clarification at WI-002/003/004 §metric blocks); missing `corelink.lru.consistency_violation_total` metric declared in WI-004; missing `corelink.lru.drift_ms` histogram declared; PromQL `histogram_quantile` fixed to canonical shape `sum(rate(...[5m])) by (le)`; (b) 95% trigger semantics async-canonical: WI-002 §8 sync→async-spawn fire-and-forget per `worker::send_future()` (Lote 10.7bis R5 P0-3); aligns WI-003 Gherkin L298 + design decision §9.4; (c) Registry traceability: invariant_registry body banner L19 0.1.0→0.2.0 + 2026-04-28; spec_contract §8 section refs corrected — `§3.3 (Dedup)` → §3.12 L162 (Sprint-driven; INV-DEDUP-CONSISTENCY actual location); `§3.X` → §3.18 L365 (Lote 10.7 S-07 NEW group); WI-001 §12 INV-DEDUP-CONSISTENCY §3.X→§3.12 + INV-CAS-IMMUTABILITY §3.3→§3.2 (canonical CAS section L86); (d) WI-001 dedup duplicate-insert behavior: chaos #6 ON CONFLICT bumps refcount (canonical per Lote 10.7bis P0-1; was 409 reject — wrong); (sec1) Parent: [S-07](../sprint.md) → ../_spec_contract.md across all 5 WIs (sprint.md not authored); (sec2) RFC 7231 §6.6.4 → RFC 6585 §4 (canonical 429 anchor; 4 references in WI-003). |
| 1.9.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S07-003 SEAL) | **WI-S07-003 SEALED — `crates/corelink-quota/` v0.1.0 shipped + `migrations/d1/0009_quota_reservations.sql` (NEW table; durable mirror of DO singleton in-memory pending reservation map).** New crate ships 7 sub-modules (~2030 LOC + tests; 95 tests all green): `audit` (5-event taxonomy `corelink.quota.{check_passed, denied_429, reserved, reservation_expired, reservation_rolled_in}`), `metrics` (4 canonical metrics including `check_total{result}` / `denials_total{tenant}` / `reservation_active{tenant,region}` / `check_duration_ms{result}`), `error` (`#[non_exhaustive]` taxonomy), `config` (canonical thresholds 95%/100% + Retry-After floor 60s + invariant validation), `reservation` (ReservationTracker trait + InMemoryReservationTracker fake byte-for-byte mirroring SQL `quota_reservations` table; size-proportional TTL via `corelink-eviction::reservation_ttl_ms` reuse), `retry_after` (PROVISIONAL transitional formula per ADR-0020 FROZEN; canonical S-08 ships `Retry-After: days-until-month-reset`), `check` (QuotaCheck trait + InMemoryQuotaCheck decision engine; per-instance `Mutex<()>` decision_lock mirrors DO actor model eliminating FM-059 race; 5-step pipeline with audit-emit BEFORE state mutation; QuotaDecision `#[non_exhaustive]` (Allow/Deny429/Reserve) carries `trigger_eviction` flag from `should_fire_quota_trigger` projection). Migration `0009_quota_reservations.sql`: composite PK `(tenant_id, reservation_id)` tenant-leftmost; 4 inline CHECK constraints; 2 secondary indices; idempotent + additive. Tests: 66 inline lib + 16 migration canonical + 13 prop_quota (9 @ 10k iter) covering `prop_quota_atomic_no_race` (FM-059 elimination via Mutex serialisation), `prop_reservation_expiry_releases_bytes` (INV-QUOTA-RESERVATION-TTL), `prop_tenant_isolation` (CTRL-ISO-005), `prop_audit_emit_per_decision_arm` (fail-closed envelope), and SLO probe `prop_check_duration_under_3ms_p99`. **Trait-abstraction-defer per charter**: real CF DO singleton + real D1 atomic batch + real Tower layer (gated `corelink-worker/tower-middleware`) + 100k nightly + chaos 8 + RB-FM-059 dry-run — all consolidated alongside WI-S07-005 (DASH-DEDUP + alerts + PRR ship gate). Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-quota` member + workspace dep + reuses `corelink-eviction` reservation_ttl_ms / should_fire_quota_trigger / EvictionRegion / TenantStorageStateStore. Quality gates verde: `cargo test -p corelink-quota --all-targets` 95 tests 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (283 docs); `check_migrations_additive.py` clean (9 migrations). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-07 corpus. |
| 1.8.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S07-002 SEAL) | **WI-S07-002 SEALED — `crates/corelink-eviction/` v0.1.0 shipped + `migrations/d1/0008_tenant_storage_state.sql` (NEW table; Lote 10.7bis P0-2 STATE/POLICY separation).** New crate ships 11 sub-modules (~3170 LOC + tests; 132 tests all green): EvictionPhase trait + InMemoryEvictionPhase orchestrator wired to BlobMetaSoftDeleteStore + AcReferenceProbe + TenantStorageStateStore + EvictionAuditSink + EvictionMetricsObserver + EvictionClock seam. Race-aware reachable check uses canonical `<` evict / `>=` protect predicate (Lote 10.7bis P0-6; mirrors S-06 INV-GC-004 protect-if-`>=` per `gc_correctness.tla` L152-154). 5-event audit taxonomy `corelink.evict.{evicted,skipped_reachable,skipped_ttl,skipped_quota_ok,quota_trigger_fired}`; 9 canonical metrics per WI §6.1.10 (incl. `gc_invariant_violation_total` SEV-0 alert gate). EvictionConfig pins canonical TTLs (free=7d, solo=30d, team=90d, business=365d, enterprise=365d default capped 730d per ADR-0019; Lote 10.7bis P0-7 default-was-not-cap fix); size-proportional reservation TTL `max(60s, request_bytes/1024 × 2), capped 7d` per Lote 10.7bis R5 P0-2; 95% quota-trigger via `worker::send_future()` fire-and-forget envelope per Lote 10.7bis R5 P0-3 simulated by `spawn_quota_trigger_in_memory`. Migration `0008_tenant_storage_state.sql`: composite PK `(tenant_id, region)` tenant-leftmost; 7 inline CHECK constraints; idempotent `IF NOT EXISTS`; additive-only; 2 secondary indices. Tests: 103 inline lib unit + 15 migration canonical + 14 prop_eviction @ 10k iter — covering `prop_evict_protect_if_re_referenced_strict_boundary` (off-by-one boundary at offsets 0/-1/+1; data-loss bug pinned), `prop_tenant_isolation` (CTRL-ISO-005), `prop_idempotent_re_run`, `prop_blob_only_scope` (Lote 10.7bis P0-8 — orchestrator has NO chunks-table dependency by type signature), `prop_ttl_size_proportional_reservation` + `prop_ttl_size_proportional_monotone`, `prop_quota_trigger_fires_at_95pct` (exact-boundary), `prop_ttl_enterprise_cap_respected` (730d hard cap), `prop_evict_dedup_byte_count_consistent`, `prop_storage_state_reclaim_monotone`. **Trait-abstraction-defer per charter**: real CF Cron DO binding + real D1 atomic batch + 100k nightly property iter + chaos suite (8 scenarios) — all consolidated alongside WI-S07-005 (DASH-DEDUP + alerts + PRR ship gate). Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-eviction` member + workspace dep. Quality gates verde: `cargo test --workspace --all-targets` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (283 docs); `check_migrations_additive.py` clean (8 migrations including new 0008). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-07 corpus. |
| 1.7.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M; orchestrator-finalized after agent rate-limit) | **WI-S07-001 SEALED — `crates/corelink-dedup/` v0.1.0 shipped; intra-tenant dedup index trait + `FindMissingBlobs` orchestrator + dedup-on-write counters.** New crate `crates/corelink-dedup/` ships 5 sub-modules (~2151 LOC + tests): `index` (DedupIndex trait + InMemoryDedupIndex with byte-for-byte mirror of S-05 WI-S05-004 `chunks` table set-difference semantic + BlobDigest newtype + DedupConfig + MAX_FIND_MISSING_BATCH_SIZE); `audit` (DedupEventType `#[non_exhaustive]` + canonical 1-event taxonomy `corelink.dedup.find_missing_blobs_executed` + DedupAuditSink + InMemoryDedupAuditSink); `metrics` (DedupMetricsObserver + 3 canonical metrics: `chunks_inserted_total` / `chunks_reused_total` / `find_missing_blobs_total`); `error` (DedupError `#[non_exhaustive]` taxonomy with explicit `CrossTenantBlocked` arm enforcing CTRL-ISO-005 cross-tenant existence-oracle gate; `dedup.cross_tenant.enabled = false` default); `write` (record_chunk_write helper invoked by production BatchUpdateBlobs / WriteBlob per WI §6.1.7 R-S07-2; monotone counter accounting). NO new migration — dedup consumes existing S-05 `chunks` table per Lote 10.7bis P0-1. Tests: 41 inline lib unit + 9 prop_dedup @ 10k iter (canonical: prop_find_missing_returns_only_absent_digests; prop_tenant_isolation CTRL-ISO-005; prop_idempotent; prop_dedup_ratio_monotone; + 5 sanity/boundary) — **50 tests across all targets, 0 failures, parallel-safe**. **Trait-abstraction-defer per charter**: real D1 `chunks` SELECT batch + real REAPI v2 `FindMissingBlobs` gRPC handler integration + 100k nightly property iter + cargo-fuzz target — all consolidated alongside WI-S07-005 (DASH-DEDUP + alerts + PRR ship gate). Quality gates verde: `cargo test -p corelink-dedup --all-targets` 0 failures (50 tests); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (283 docs); `check_migrations_additive.py` clean (7 migrations; no new migration). **Note**: agent hit Anthropic rate limit at 1.3M tokens / 53 tool uses post-quality-gates; orchestrator-finalized SEAL ceremony per charter `Real bug detected; don't paper over` pattern. No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-07 corpus. |
| 1.5.0 | 2026-04-28 | Gustavo (Lote 10.7-tris cycle 5 codex SEAL remediation) | **4 codex 8.1 blockers fixed**: (a) WI-S07-005 INV count 3→5 swept across H1, title row, intent narrative, completeness §11, validation §10.s07.005.7, Gherkin (8 locations total cumulative through cycles 4+5); (b) Alert SEV reconciliation: INV-GC-001 violation SEV-1→SEV-0 in WI-002 L265 (matches WI-005 ship-gate + invariant_registry CRITICAL severity); D1↔DO sync lag SEV-2→SEV-1 in WI-003 R-004 risk register (matches L475 post-mortem hooks); (c) Version metadata aligned: spec_contract frontmatter 1.3.0→1.4.0 then 1.4.0→1.5.0 + footer v1.2.0→v1.4.0; invariant_registry frontmatter 0.1.0→0.2.0 + updated 2026-04-28; (d) 100% provisional propagation: WI-S07-003 4 stale plain-429 references swept (title row L39, scenario summary L255, design decision L362, API contract L468) — all now explicit PROVISIONAL transitional. |
| 1.4.0 | 2026-04-28 | Gustavo (Lote 10.7-tris cycle 4 codex SEAL remediation) | **3 codex 8.2 blockers fixed**: (a) Invariant ledger canonicalized — §8 lists all 6 NEW INVs (1 §3.3 DEDUP + 5 §3.X EVICT/QUOTA); WI-005 §7 promotion list expanded 3→5 NEW INVs aligned with registry §3.X; WI-005 §11 DoD + Gherkin updated. (b) 100% quota ownership boundary clarified: WI-S07-003 explicitly frames 429 + Retry-After as PROVISIONAL transitional (title, §6.6 section, Gherkin) — canonical S-08 CAP-QUOTA-001 rate-limit DO with `Retry-After: days-until-month-reset` ships in S-08; WI-005 ship-gate scenario aligned. (c) ADR-0019 migration plan execution scope amended: rollout + notification + audit event emit deferred to S-13 admin plane (matching ADR-0020 email defer pattern); S-07 owns runtime TTL config consumption (already in WI-S07-002 §6.1.3); S-13 owns migration orchestration. |
| 1.3.0 | 2026-04-28 | Gustavo (Lote 10.7-tris cycle 2 codex SEAL remediation) | **4 hard blockers fixed (codex 5.9 → ≥9.0 trajectory)**: (a) Parent contract dedup terminology aligned to `chunks` table PK canonical (was `manifest_chunks` UNIQUE INDEX — broken dedup); §1 + CAP-DEDUP-001 + R-S07-1 + WI-001 table updated. (b) Eviction reachability split-brain resolved: invariant_registry INV-EVICT-CASCADE-PREVENTED + WI-S07-002 Gherkin scenario aligned to ac_meta-only scope (Lote 10.7bis P0-8 BLOB-only canonical; chunks reachability owned by S-06 GC via `chunks.refcount`). (c) TTL Enterprise default 730d → 365d (per ADR-0019 §Decision; max 730d as admin override cap). (d) WI-S07-003 size-proportional TTL formula propagated to 5 stale `60s` hard-coded references (Rust comment, narrative, risk register, code logic, property test). Sign-off slot names remain `_TBD_` per design-readiness gate (ship-readiness names filled at PRR ceremony per ADR-0034 solo-tier waiver path). |
| 1.2.0 | 2026-04-25 | Gustavo (Lote 10.7bis R4 + R5 review remediation) | **9 P0s + 3 R5 P0s applied** (não exhaustive — phased approach Phases 1-5): (a) **P0-1** WI-S07-001 chunks NOT manifest_chunks (was wrong table; UNIQUE on per-blob list breaks dedup); (b) **P0-3** column-name drift `_ms` suffix → canonical (deleted_at, created_at, last_accessed_at); (c) **P0-5 + P0-4** ADR-0019/0020 DRAFT → FROZEN promotion + 80% email defer to S-13 (boundary clarification em ADR-0020); (d) **P0-2** new `tenant_storage_state` table (separates STATE from POLICY tenant_quota; NEW migration); (e) **P0-6** race-aware reachable check com strict-< predicate (`a.created_at < evict_started_at_ms`; analogous a S-06 INV-GC-004); (f) **P0-8** scope-reduce eviction to BLOB-only (chunks lifecycle owned by S-06 GC); (g) **P0-9** DO routing via tenant.primary_region; (h) **R5 P0-2** size-proportional reservation TTL (ttl = max(60s, request_bytes/1MB/s × 2x), capped 7d) — multipart 160 GiB no longer over-quota mid-upload; (i) **R5 P0-3** fire-and-forget via `worker::send_future()` (NOT `tokio::spawn` nor `wasm_bindgen_futures::spawn_local` — those don't exist em CF Workers Rust runtime). Score trajectory: pre-bis 7.2/10 (R4 7.6 + R5 6.8) → post-bis projected 8.5/10 (R4 estimate). Remaining: P0-7 tier taxonomy ADR amendment, P1s (SLO-DEDUP-RATIO definition, race property test 100k iter, etc.) — defer to Lote 10.7-tris. |

---

**Fim de spec contract S-07 SOTA (v1.9.0).** Upgrade history: v1.0→v1.1 SOTA elevation (Lote 9.1); v1.1→v1.2 R4+R5 P0 fixes (Lote 10.7bis); v1.2→v1.3 cycle 2 codex SEAL remediation; v1.3→v1.4 cycle 4 codex SEAL remediation; v1.4→v1.5 cycle 5 INV count + alert SEV + version metadata + 100% propagation; v1.5→v1.6 cycle 6 metric naming + async trigger + section refs + dedup behavior + RFC anchor + Parent ref; v1.6→v1.7 WI-S07-001 SEAL; v1.7→v1.8 WI-S07-002 SEAL; v1.8→v1.9 WI-S07-003 SEAL.
