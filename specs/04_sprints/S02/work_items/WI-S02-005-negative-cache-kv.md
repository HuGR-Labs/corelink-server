---
id: "WI-S02-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-02"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SECURITY-MODEL"
tags: ["wi", "s02", "kv", "negative-cache", "performance", "cost-reduction", "pat-kv-ttl-001"]
---

# WI-S02-005 — Negative Cache KV Adapter (`ac_neg:<digest>` per-region TTL 300s)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-02](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S02-005 |
| Título | Negative cache KV adapter (404 result-caching) + invalidation hook |
| Sprint | S-02 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cache key per-tenant; bug → cross-tenant via cache poisoning), FF-HR-005 (cache invalidation correctness é storage semantics) |

## 1. Intent

Implementar `NegativeCache` em `crates/corelink-worker/src/cache/negative.rs` que cacheia 404 results em CF KV per-region:

```rust
pub struct NegativeCache {
    kv: KvNamespace,  // per-region; bound via env
    region: Region,
}

impl NegativeCache {
    /// Lookup if (tenant, digest) is known-missing. Returns Some(MissReason) if cached.
    pub async fn lookup(&self, tenant: TenantId, digest: &Digest) -> Result<Option<MissReason>>;

    /// Populate cache: blob digest is known-missing for tenant.
    pub async fn put_miss(&self, tenant: TenantId, digest: &Digest, reason: MissReason) -> Result<()>;

    /// Invalidate when blob is written (S-01 write path hook).
    pub async fn invalidate_on_write(&self, tenant: TenantId, digest: &Digest) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MissReason {
    NotFound,           // digest never existed in tenant scope
    Tombstoned,         // soft-deleted (S-06 GC)
    CrossTenantMasked,  // exists in another tenant; masked as not found
}
```

Key format: `ac_neg:<region>:<HMAC16>:<digest_hex>` (per-region per-tenant per-digest; HMAC16 canonical = `b64(HMAC_SHA256(TDK, tenant_id))[0:16]` per remote_cache_product_profile.md §7.1; NUNCA plaintext tenant_id).
TTL: 300s (PAT-KV-TTL-001).
Invalidation: WI-S01-005 (REAPI BatchUpdateBlobs handler) calls `invalidate_on_write` on successful write; ensures stale negatives evict immediately.

Custo: probe storms (atacante OR Bazel cliente warm-up enumeration) hit cache → não-bate D1+R2 → reduced cost ≥ 80% target.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Negative caching é dual-edged sword: **performance optimization** (reduce cost ≥ 80% probe storms) + **vulnerability surface** (incorrect invalidation = stale negatives → cliente sees 404 mesmo after write). Bugs catastróficos:

1. **Stale negative survives write** (FM-302 adjacent): cliente PUT digest_X (success); imediato GET digest_X retorna stale 404 from KV cache; cliente confused → retry storm.
2. **Cross-tenant cache poisoning**: key não inclui tenant_id; Tenant A cache miss para digest_X poisons Tenant B; Tenant B's GET retorna spurious 404 mesmo se tem blob.
3. **Cache populates wrong reason**: marks blob as NotFound quando deveria ser Tombstoned (decisão GA freeze: ambos mapeiam para HTTP 404 — vide §9.10; enum é forward-future-ready mas handler atualmente uniforma).
4. **TTL too long**: stale negatives sustain past write; window of inconsistency > 300s.
5. **TTL too short**: cache miss rate high; cost benefit eliminated.

Mitigação:
1. **Key per-tenant via HMAC16**: `ac_neg:<region>:<HMAC16>:<digest_hex>` (HMAC16 canonical per remote_cache_product_profile.md §7.1; NUNCA plaintext tenant_id) — cross-tenant impossible by construction; alinha contract com R2/D1 path layout.
2. **Invalidation hook em write path**: WI-S01-005 PutBlob handler chama `invalidate_on_write(tenant, digest)` em success → KV.delete(key) imediato → stale negative removed before cliente next GET.
3. **MissReason enum** explicit: NotFound / Tombstoned / CrossTenantMasked — handler decide HTTP semantic per reason.
4. **TTL 300s**: balance — long enough to catch repeated probes em 5min window; short enough that stale-window é bounded; PAT-KV-TTL-001 canonical.
5. **Per-region KV**: residency aligned (Tenant EU's negatives stay em EU KV bucket).

**Atacante adversarial scenarios:**
- **Probe storm enumeration**: atacante envia 1000 random digests; sem negative cache, cada hit D1+R2 = $$. Com negative cache + per-PAT rate limit (S-08 forward), cost reduced + enumeration speed capped.
- **Cache pollution**: atacante envia random non-existent digests para fill KV. KV per-tenant; atacante's tenant scope only; outros não afetados. KV size bounded by tenant; eviction LRU at scale.
- **Race write vs negative cache populate**: cliente PUT digest_X concurrent com prior probe que populated KV `ac_neg:digest_X`. Order matters: invalidation precedes populate? Lazy populate (write returns success → invalidate → cliente retry GET → no negative cache hit). Race resolved.

**Risk justification HIGH_RISK:**
- **FF-HR-002**: cross-tenant cache poisoning é isolation breach.
- **FF-HR-005**: cache invalidation correctness é storage semantics enforcement.
- **Reversibility**: stale negative bug = customer support storm; rapidly fixable mas customer trust impacted.

11 sign-offs canonical HIGH_RISK incl. Architect (cache invalidation review).

## 3. Customer Impact & Journey

**JTBD:** "Como dev de CI fazendo `bazel build` repetido, espero subsequent calls hit cache rapidamente — sem repeated D1+R2 lookups que I'm not paying for indirectly via increased latency."

**Customer-visible:**
- Latency p99 GET cold com negative cache hit: ~50ms (vs ~150ms uncached).
- Cost: customer não vê direct (CoreLink absorbs reduction; potential pricing benefit em pricing v2).
- Stale negative window: max 300s; documented em SLA addendum.

## 4. Capability Mapping

- **CAP-CAS-007** (Negative caching 404 com TTL curto) — IMPLEMENTA primary.
- Trace: `resilience_patterns.md PAT-KV-TTL-001` + `data_model.md §X (KV namespace)`.

## 5. Tipo

Cache adapter; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`NegativeCache` struct** em `crates/corelink-worker/src/cache/negative.rs`:
   - `lookup`, `put_miss`, `invalidate_on_write` métodos.
   - Key format canônico.
   - TTL 300s default; tunable via DO config-singleton (S-13 forward).
2. **`MissReason` enum** com 3 variants + derive Serialize/Deserialize.
3. **Integration em CAS read path** (WI-S02-001 GetBlob):
   - GET handler check `negative_cache.lookup` antes D1 AuthZ check.
   - Hit → return 404 imediato uniform (com timing padding via WI-S02-004 middleware). Short-circuit é canonical para GetBlob — invalidation hook em S-01 PUT (§6.1.4) garante freshness; window stale ≤ 5s region-local per §8 AC scenario.
   - Miss → fall-through to D1 + R2 paths.
4. **Integration em CAS write path** (WI-S01-005):
   - PutBlob success → call `negative_cache.invalidate_on_write(tenant, digest)`.
5. **Integration em FindMissingBlobs** (WI-S02-002):
   - **MUST be authoritative** per `storage_semantics_matrix.md §FindMissingBlobs` (Bazel cliente decide upload baseado em response — stale negative = redundant upload sad path mas NÃO catastrophic; stale positive = missed upload é catastrophic).
   - Batch lookup pattern: parallel `negative_cache.lookup` as **hint**; **all "missing" decisions verified via D1 + R2 HEAD compensating check** (KV pode estar stale após PUT). Cache hit é hint; falsh-through still required.
   - Populate cache for confirmed missing digests post-batch.
   - Distinção semantic: GetBlob (§6.1.3) PODE short-circuit em cache hit (single-digest read; invalidation hook); FindMissingBlobs DEVE fall-through (authoritative batch dedup discovery).
6. **Per-region KV namespace binding** via wrangler.toml: `KV_NEGATIVE_CACHE_<REGION>`.
7. **Métricas**:
   - `corelink.cas.negative_cache.hits_total{tenant_tier, region}` (counter).
   - `corelink.cas.negative_cache.invalidation_total{trigger}` (trigger ∈ s01_write, manual).
   - `corelink.cas.negative_cache.stale_window_seconds_bucket` (histogram; tracks time between write and invalidation propagation).

### 6.2 Out-of-scope (deferred)

- **Cross-region replication** (eventual consistency global): S-14.
- **Adaptive TTL** (longer for stable digests): pós-GA Q1.
- **Positive cache** (cache hits content): defer pós-GA; CF Edge cache at HTTP layer suffices.
- **Cache warming** (pre-populate negatives): anti-pattern; lazy populate is correct.

## 7. Anti-Scope

- ❌ Cache de 200 OK responses (positive cache; out of scope; anti-pattern em CAS).
- ❌ Global KV namespace (per-region; residency).
- ❌ TTL > 600s (stale window too long).
- ❌ Sync write to KV (use async; non-blocking cache populate).
- ❌ Cache invalidation via TTL only (use explicit invalidate em write path).
- ❌ Custom KV serialization beyond serde (use canonical JSON).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Negative cache KV adapter

  Background:
    Given NegativeCache configured TTL 300s per-region

  Scenario: Lookup miss — cache empty
    When negative_cache.lookup(tenant_A, digest_X) called
    Then Result::Ok(None) returned
    (não known-missing; fall-through to D1)

  Scenario: Populate miss + lookup hit
    When negative_cache.put_miss(tenant_A, digest_X, NotFound) called
    Then KV key "ac_neg:wnam:<HMAC16(tenant_A)>:<digest_X_hex>" set with TTL 300s (HMAC16 canonical)
    When negative_cache.lookup(tenant_A, digest_X) called within 300s
    Then Result::Ok(Some(NotFound)) returned

  Scenario: TTL expiration
    When KV key set 301s ago
    When negative_cache.lookup called
    Then Result::Ok(None) returned (TTL expired; KV evicted)

  Scenario: Cross-tenant isolation
    Given negative_cache.put_miss(tenant_A, digest_X, NotFound)
    When negative_cache.lookup(tenant_B, digest_X) called
    Then Result::Ok(None) returned
    (Tenant B sees no cached negative; per-tenant key isolation)

  Scenario: Invalidation on write
    Given negative_cache.put_miss(tenant_A, digest_X, NotFound)
    When CAS write succeeds for (tenant_A, digest_X)
    And invalidate_on_write(tenant_A, digest_X) called
    Then KV key deleted
    When negative_cache.lookup(tenant_A, digest_X) called
    Then Result::Ok(None) returned (invalidated; can fall-through to D1)

  Scenario: Probe storm cost reduction
    Given Tenant A probes 1000 random non-existent digests
    When negative cache empty initially
    Then first 1000 lookups: cache miss → D1 + R2 query (cost = 1000 × full path)
    When second iteration of same 1000 digests
    Then 1000 lookups: cache hit → return 404 (no D1/R2)
    And cost reduction ≥ 80% measured (vs no-cache baseline)

  Scenario: Stale window bounded — region-local strong; cross-region eventual
    Given negative cache populated for digest_X em region wnam at T=0
    Given CAS write succeeds at T=10s (writes propagate same-region wnam)
    Given invalidate_on_write fires at T=10s em wnam KV
    Then cliente GET em wnam at T=11s returns 404 stale max 1s (region-local strongly consistent)
    And p99 stale window dentro do originating region ≤ 5s
    And **cross-region propagation eventual ≤ 60s** (CF KV global eventual consistency; documented limitation)
    Note: cache miss em outra região = fall-through to D1 + R2 (correct fallback; nunca incorrect read)

  Scenario: MissReason variant correctness — S-02 GA freeze 404 uniform
    Given Tenant A has tombstoned digest D_T (deleted_at != NULL)
    When CAS read handler resolves
    Then negative_cache.put_miss(tenant_A, D_T, Tombstoned) called
    And lookup returns MissReason::Tombstoned
    And handler maps to HTTP 404 Not Found com error_code COR_CAS_BLOB_NOT_FOUND (uniform com NotFound)
    (S-02 GA decision: enum existe forward-future-ready mas todos variants → HTTP 404 + uniform error_code para minimizar information disclosure surface; 410 Gone semantic reserved para S-06 GC sprint com explicit ADR + migration plan)
    And response body identical para NotFound + Tombstoned + CrossTenantMasked (atacante não distingue)
    (alinhamento com WI-S02-001 §6.1.6 não-tombstoned 404)

  Scenario: Concurrent put_miss vs invalidate_on_write — race resolution via monotonic version stamp
    Given KV é eventually consistent (cross-region; per-region strong)
    Given client A probes digest_X at T+0 → handler computes miss → put_miss(tenant_A, digest_X, NotFound) com version_stamp v=1
    Given concurrent client B writes digest_X at T+1ms → invalidate_on_write(tenant_A, digest_X) com version_stamp v=2
    When KV ordering arrives [v=2 invalidate, v=1 put_miss] em eventual order
    Then put_miss with v=1 < current_v=2 is **rejected silently** (stale write loses; KV stores tagged version)
    Then post-race state: KV key absent (invalidation wins; correct)
    And property test prop_negative_cache_race (WI-S02-006 §6.1.4) exercises 10k iter
    And invariant INV-NEG-CACHE-MONOTONIC enforced
```

## 9. Design Decisions

### 9.1 Why CF KV (não DO ou D1)

- **Eventually consistent global**: KV is fine for cache (stale tolerable max 5s).
- **Edge-distributed**: low latency reads em todas as regiões.
- **TTL native**: KV supports TTL natively; no manual eviction worker.
- **Cost**: KV $0.50/M reads vs D1 $1/M; cache hits cheaper.
- D1 is per-region; cache per-region matches OK.
- DO is overkill (single-tenant ownership; cache é shared across tenants).

### 9.2 Why per-region KV (não global)

Tenant residency: EU tenant's negative cache stays em EU KV. Aligned com S-14 region pinning forward. Cross-region propagation overhead unjustified at GA.

### 9.3 Why TTL 300s

- < 60s: cache miss rate high; cost benefit eliminated.
- 300s: balance; covers Bazel build storm window (typical 5-10min).
- > 600s: stale window > business tolerance.
- PAT-KV-TTL-001 canonical em resilience_patterns.

### 9.4 Why explicit invalidation em write path (não TTL only)

TTL-only = stale window 5min after write; cliente confused. Explicit invalidation removes stale immediately; window ≤ 5s (KV propagation). Cost minor (1 KV.delete per write).

### 9.5 Why MissReason enum (não bool flag)

Future-proof: NotFound vs Tombstoned vs CrossTenantMasked carry different audit semantic em internal logs (NotFound = expected; Tombstoned = soft-delete log; CrossTenantMasked = privacy-significant attempt). Enum provides type-safe switch em audit emission. **HTTP semantic é uniformizada em GA** (vide §9.10).

### 9.10 GA decision freeze — todos MissReason mapeiam para HTTP 404

**Decisão (Lote 10.2bis P0 fix — supersede gap WI-005 vs WI-001 review)**: ao GA, **todos** os MissReason variants retornam HTTP 404 com `error_code = COR_CAS_BLOB_NOT_FOUND` uniform; response body identical (atacante não distingue NotFound vs Tombstoned vs CrossTenantMasked via response).

Razões:
- **Information disclosure minimization**: 410 Gone vs 404 Not Found revela existence (Tombstoned implica "existed previously"; cliente cross-tenant infere via 410). Uniformização elimina oracle.
- **REAPI v2 compatibility**: bazelbuild/remote-apis test suite assume 404 para missing blob; 410 não-canonical em REAPI semantics.
- **Audit-only differentiation**: enum sustains internal audit reason (S-09 chain logs Tombstoned distinctly); cliente vê uniform 404.
- **Migration path**: S-06 GC sprint pode revisitar 410 Gone semantic com explicit ADR + privacy review + REAPI conformance re-validation.

ADR `ADR-0028: MissReason → HTTP 404 uniform freeze (S-02 GA)` documenta + 410 Gone deferido S-06.

Whitelist em validate_references.py.

### 9.11 Why monotonic version stamp para race resolution put_miss vs invalidate

KV é eventually consistent globally. Concurrent ops em mesmo key (`ac_neg:<region>:<tenant>:<digest>`) podem chegar fora de ordem em replica nodes. Sem ordering control:
- T+0 cliente A probes → put_miss arrives at KV node X.
- T+1ms cliente B writes → invalidate arrives at KV node Y.
- Eventual replication: node X→Y, Y→X em ordem inconsistent.
- Pior caso: invalidate aplicada primeiro (correct), put_miss arrives second (stale; sobrescreve com NotFound permanente).

**Mitigação**: cada KV write inclui `version_stamp` (monotonic; derived from request timestamp + tenant_id hash). KV value envelope:
```rust
struct CachedMissEnvelope {
    miss_reason: MissReason,
    version_stamp: u64,  // monotonic
    cached_at: u64,      // epoch ms
}
```
- put_miss: KV.put se NEW.version_stamp > EXISTING.version_stamp OR EXISTING absent.
- invalidate_on_write: KV.delete sempre (cleanup é absolute).
- Conflict resolution: write-with-newest-stamp wins; older stamps rejected silently.

Property test `prop_negative_cache_race` (WI-S02-006 §6.1.4) exercises 10k iter concurrent put/invalidate; invariant INV-NEG-CACHE-MONOTONIC: "KV state após qualquer race trace é igual ao state após sequential ordering with newest stamp".

### 9.6 Lazy populate vs eager

Eager (pre-populate negatives) é anti-pattern — wastes cache space + adds latency tax to first writes. Lazy populate: only after first miss → cache contains "useful" negatives.

### 9.7 ADR potencial?

Não. Patterns reused (KV cache pattern standard).

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Probe storm cost reduction ≥ 80% measured em integration test (EVT-021).
- [ ] **10.5.2** Stale window p99 ≤ 5s (criterion benchmark write-then-GET timing).
- [ ] **10.5.3** Cross-tenant isolation property test 100k iter → 0 cross-tenant hits (EVT-002).
- [ ] **10.5.4** Invalidation hook coverage: 100% successful writes trigger invalidate (EVT-002).
- [ ] **10.5.5** TTL 300s configurable via DO config-singleton (S-13 forward; static at GA).
- [ ] **10.5.6** Cost regression gate (§14.10): cache hit reduces per-op cost; baseline benchmark.

## 11. DoD

- [ ] NegativeCache impl + integration em 3 paths (read, write, batch).
- [ ] Per-region KV binding wrangler.toml.
- [ ] Property test cross-tenant isolation.
- [ ] Probe storm benchmark (cost reduction).
- [ ] Stale window measurement.
- [ ] Métricas + dashboard.
- [ ] Code review + Architect.

## 12. Invariants

- INV-TENANT-ISOLATION (CRITICAL): per-tenant key prevents cache poisoning cross-tenant.
- INV-CAS-IMMUTABILITY (CRITICAL): tombstoned blobs respected via MissReason::Tombstoned.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| NegativeCache | `crates/corelink-worker/src/cache/negative.rs` | Rust |
| MissReason enum | `crates/corelink-worker/src/cache/miss_reason.rs` | Rust |
| Per-region binding | `wrangler.toml` (KV_NEGATIVE_CACHE_<REGION>) | TOML |
| Property test cross-tenant | `tests/prop_neg_cache.rs` | Rust |
| Cost reduction benchmark | `benches/probe_storm.rs` | Rust |

## 14. Quality Standards SOTA

- **14.5.1** Zero unsafe; zero unwrap.
- **14.5.2** rustdoc + 3 examples.
- **14.5.3** Test coverage ≥ 90%.
- **14.5.4** Cache lookup p99 ≤ 5ms (KV read warm).
- **14.5.5** SAST clean.
- **14.5.6** Métricas RED + cache hit ratio.
- **14.5.7** Runbook: nenhum (cache miss é feature behavior).
- **14.5.8** Breaking changes em key format = bump major + invalidate all KV.
- **14.5.9** Memory bounded (KV não memory-resident).
- **14.5.10** Cost regression gate.

## 15. Chaos Experiments

1. **KV outage**: verify graceful fall-through to D1 + R2 (cache miss = no failure).
2. **Race write vs cache populate**: verify invalidation precedence.
3. **Probe storm 10k random digests**: verify cost reduction sustained.
4. **TTL boundary**: verify exact 300s ± propagation.

## 16. PRR

PRR + Architect.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | NegativeCache scaffold + key format | 1.5h |
| ST-002 | lookup/put_miss/invalidate_on_write impl | 2.5h |
| ST-003 | MissReason enum + serialize/deserialize | 1h |
| ST-004 | Integration em CAS read path | 2h |
| ST-005 | Integration em CAS write path (invalidation hook) | 1.5h |
| ST-006 | Integration em FindMissingBlobs batch | 2h |
| ST-007 | Per-region KV binding wrangler.toml | 1h |
| ST-008 | Métricas emit | 1h |
| ST-009 | Property test cross-tenant 100k iter | 2.5h |
| ST-010 | Probe storm benchmark | 2h |
| ST-011 | Stale window measurement | 1.5h |
| ST-012 | rustdoc + examples + PRR | 2h |

**Total**: ~20.5h Optimistic; PERT ~25h.

## 18. Dependencies

### Hard blockers

- WI-S02-001 SEALED (read path; cache lookup integration).
- WI-S01-005 SEALED (write path; invalidation hook).

### Soft blockers

- WI-S02-002 (FindMissingBlobs) — batch integration.

### Outbound

- S-08 rate limit — complementary defense (storm caps even sem cache).
- S-13 admin plane — TTL tunable via config-singleton.

## 19. Effort PERT

O: 18h, M: 20.5h, P: 36h → PERT 23.7h.

## 20. Time-boxing

26h hard limit.

## 21. Observability

3 métricas listadas; dashboard widget DASH-CAS (negative cache effectiveness ratio).

## 22. Cost Analysis

- Cache hit avoids D1 query ($1/M) + R2 GetObject ($0.36/M).
- Cache cost: $0.50/M KV reads + $5/M KV writes.
- Per probe storm 1k digests × 100 reps: savings ~$0.04 cumulative; em 10M req/dia ratio: ~$2k/yr saved.

## 23. API Contract

NegativeCache é internal Rust; no external API impact.

## 24. Post-mortem Hooks

- Stale negative bug em produção (write→GET inconsistency > 5s) → SEV-2 + 5-Why.
- Cross-tenant cache leak → CRITICAL post-mortem + breach notification consideration.
- Invalidation hook miss (write succeeded; cache not invalidated) → post-mortem.

## 25. Rollback / Recovery

Hot rollback via WASM. Cache flush via `KV.delete` on all keys (manual; rare).

## 26. Security & Privacy

STRIDE: cross-tenant cache poisoning prevented via per-tenant key.
LINDDUN: linkability — tenant_id em key é pseudonymous UUID.

## 27. Knowledge Transfer

Doc `docs/internal/negative-cache-pattern.md` — reusable pattern para outros caches.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Stale negative > TTL after write (invalidation miss) | M | M | HIGH | M | LOW | Explicit invalidation hook em write path; chaos test |
| R-002 | Cross-tenant cache key collision | L | M | CRITICAL | L | LOW | Per-tenant key construction + property test |
| R-003 | KV outage breaks read path | L | L | LOW | L | LOW | Graceful fall-through to D1 |
| R-004 | TTL too long causes business issue | L | M | MEDIUM | L | LOW | 300s default + tunable via DO config |
| R-005 | Cache size bloat (atacante pollution) | L | L | LOW | L | LOW | KV LRU eviction at scale; per-tenant scope |
| R-006 | Cost regression gate | M | L | LOW | L | LOW | §14.10 |

## 29. Review Checkpoints

1. Design (D+0): Architect approves cache invariants.
2. Code (D+2): peer.
3. Adversarial (pre-merge): cross-tenant property + stale window.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; emphatic — cache invariants + concurrency race review_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; cross-tenant key construction validation_ | _pending_ | _pending_ |
| 13 | Crypto SME | _advisory; non-crypto-touching WI mas mantém alinhamento sprint contract §14_ | _pending_ | _pending_ |

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.2.

## 32. Anti-patterns evitados

- ❌ Global KV (residency violation).
- ❌ TTL-only invalidation (stale window > 5min).
- ❌ Eager populate (cache space waste).
- ❌ Sync KV write em hot path (latency tax).
- ❌ Cache 200 OK responses (positive cache anti-pattern).
- ❌ Cross-tenant key sharing.

---

**Fim WI-S02-005.** Próximo: WI-S02-006 (property test 100k + RB-FM-253 + PRR).
