---
id: "WI-S02-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
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
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
tags: ["wi", "s02", "cas", "reapi", "getblob", "findmissingblobs", "batch"]
---

# WI-S02-002 — REAPI `GetBlob` Unary + `FindMissingBlobs` Batch Discovery

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-02](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S02-002 |
| Título | GetBlob unary (≤ 4 MiB inline) + FindMissingBlobs batch |
| Sprint | S-02 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-tenant batch query catastrophic), FF-HR-005 (CTRL-ISO-002 enforcement em batch surface) |
| Tier | Todos (Bazel client otimization hot path) |
| Fase produto | Fase 1 — Remote Cache |

## 1. Intent

Implementar 2 REAPI v2 handlers complementares ao streaming Read (WI-S02-001):

```rust
// Unary GetBlob — small blobs ≤ 4 MiB inline em response
async fn get_blob(
    ctx: TenantCtx,
    req: GetBlobRequest,
) -> Result<GetBlobResponse, Status>;

// Batch discovery — Bazel pre-upload check
async fn find_missing_blobs(
    ctx: TenantCtx,
    req: FindMissingBlobsRequest,  // [Digest]
) -> Result<FindMissingBlobsResponse, Status>;  // [Digest] missing_subset
```

`GetBlob` é alternativa unary ao `ByteStream::Read` para blobs pequenos — single round-trip, response inline (não streaming). Cliente trade-off: simpler API mas memory-bounded em 4 MiB.

`FindMissingBlobs` é o **otimização chave do Bazel client**: antes de fazer N PutBlobs, cliente pergunta "quais destes N digests vocês ainda não tem?" → server responde subset missing → cliente upload only que falta. Reduz uploads desnecessários em 50-80% em workloads warm.

Ambos handlers compartilham: auth middleware (TenantCtx propagation), AuthZ check D1 pre-R2 (CTRL-ISO-002), tenant prefix derivation reusing S-01 corelink-tenant-path. Cross-tenant catastrophic se batch query sem proper filtering.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

`FindMissingBlobs` é particularmente perigoso: single request com 1000 digests, sem proper tenant scoping = **batch enumeration vulnerability**. Atacante (com PAT válido de Tenant B) pode enviar `FindMissingBlobs([1000 random digests])`; se server retorna "digest_X é missing" mas digest_X é blob legítimo de Tenant A, atacante aprende que Tenant A tem blob_X. Cross-tenant existence oracle = privacy leak (CTRL-ISO-005).

Mitigação:
1. **Per-digest AuthZ check**: cada digest do batch passa por D1 query `SELECT 1 WHERE tenant_id=? AND digest=? AND deleted_at IS NULL`. Filter response: blobs de outros tenants são "not found" (não distinguível de "really doesn't exist").
2. **Constant-time response per digest**: garante atacante não learn via timing diff entre "exists em outro tenant" vs "doesn't exist anywhere" (WI-S02-004 covers timing layer).
3. **Batch size limit**: max 1000 digests per request (REAPI v2 spec); reject 413 Request Entity Too Large.
4. **Rate limit per-PAT** (S-08 forward): per-tenant batch query rate-limited; previne enumeration storm.

`GetBlob` unary é simpler — single digest, mesma logic do Read mas response inline. Memory budget: 4 MiB body + JSON wrapping ~5 MiB peak per request. Worker memory 128 MiB → 25 concurrent unary GetBlobs sem swap.

Decision: **client trade-off documented**:
- Bazel/Buck2 small actions (< 4 MiB outputs) preferem GetBlob unary (simpler).
- Large outputs (logs, build artifacts > 4 MiB) precisam Read streaming.
- SDK (S-15) auto-routes baseado em size hint.

`FindMissingBlobs` batch otimização economics:
- Sem batch: 1000 PutBlob requests + 1000 round-trips × 50ms RTT = 50s.
- Com batch: 1 FindMissingBlobs (50ms) + ~100 PutBlobs (5s) = ~5s sustained.
- 10× speedup em warm builds.

**Risk justification HIGH_RISK:**
- **FF-HR-002**: batch enumeration vulnerability é cross-tenant existence oracle.
- **FF-HR-005**: implementa CTRL-ISO-002 + CTRL-ISO-005 em batch surface; security-critical layer.
- **Reversibility**: information disclosure é one-way; atacante já learned blob existence.

Por isso: 10–12 sign-offs, property test 100k batch queries cross-tenant, side-channel timing analysis (WI-S02-004 dependency), constant-time per-digest filter, REAPI conformance suite.

## 3. Customer Impact & Journey

**JTBD (Bazel/Buck2 cliente):** "Eu tenho 500 actions com 500 outputs blobs; antes de upload, quero perguntar 'quais vocês já têm?' em single request, e fazer upload só do subset missing — economizar 50-80% bandwidth."

**Customer-visible:**
- `FindMissingBlobs` p99 ≤ 200ms para batch de 1000 digests (criterion benchmark).
- `GetBlob` p99 ≤ 150ms cold; ≤ 50ms warm.
- Clear error mapping: 404 individual digests em batch retornados como "missing"; 403 cross-tenant attempts → entire request rejected (não per-digest leak).

## 4. Capability Mapping

- **CAP-CAS-009** (FindMissingBlobs REAPI batch) — IMPLEMENTA primary.
- **CAP-CAS-004** (GET blob by digest) — IMPLEMENTA unary surface.
- Trace: REAPI v2 spec `ContentAddressableStorage.FindMissingBlobs` + `BatchReadBlobs`.

## 5. Tipo e Classificação

API Handler (batch + unary); HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **gRPC `GetBlob` unary handler** em `crates/corelink-worker/src/reapi/cas_unary.rs`:
   - Single digest request → blob bytes inline em response (≤ 4 MiB).
   - Reject 4 MiB+ blobs com `COR_CAS_BLOB_TOO_LARGE` + hint "Use ByteStream::Read for larger blobs".
   - Same auth + AuthZ + tenant prefix logic do WI-S02-001.
2. **gRPC `FindMissingBlobs` batch handler**:
   - Request: array of digests, max 1000.
   - Per-digest D1 AuthZ check (parallel via Tokio join_all).
   - Response: subset of digests não existentes em tenant scope.
   - Cross-tenant blobs → "missing" (não distinguishable de truly missing).
3. **HTTP REST surfaces equivalentes**:
   - `GET /v1/cas/<digest>?inline=true` → unary response (alternativa a Range-supported streaming).
   - `POST /v1/cas/find-missing` com JSON `{"digests": [...]}` → array response.
4. **Per-digest AuthZ parallel**: Tokio `join_all(digests.map(|d| authz_check(d)))`; bounded concurrency (max 100 parallel D1 queries).
5. **Métricas**:
   - `corelink.cas.get_blob.requests_total{tenant_tier, region, result}` (counter).
   - `corelink.cas.find_missing.batch_size_bucket{tenant_tier}` (histogram).
   - `corelink.cas.find_missing.duration_seconds_bucket{batch_size_bucket}` (histogram).
6. **error_taxonomy mapping**:
   - `COR_CAS_BLOB_NOT_FOUND` (per-digest em FindMissingBlobs response).
   - `COR_CAS_BLOB_TOO_LARGE` (GetBlob com blob > 4 MiB; suggest streaming Read).
   - Batch level: `COR_CAS_BATCH_SIZE_EXCEEDED` se > 1000 digests.

### 6.2 Out-of-scope (deferred)

- **BatchReadBlobs** (REAPI v2 batch read N blobs em single response): defer pós-GA; complexity vs benefit marginal.
- **Streaming Read** (large blobs): WI-S02-001.
- **Constant-time 404/403** at HTTP level: WI-S02-004 (este WI emit basic 404 vs 403 distinction).
- **Negative cache** integration: WI-S02-005.

## 7. Anti-Scope (expandido para HIGH_RISK)

- ❌ Batch FindMissingBlobs > 1000 digests (REAPI v2 limit).
- ❌ GetBlob com Range support (defer; use Read streaming).
- ❌ Cross-tenant batch query "OR" semantics (sempre AND tenant_id filter).
- ❌ Metadata batch (batch que retorna size/timestamps; not REAPI semantics; defer).
- ❌ FindMissingBlobs com filter por last_accessed_at (privacy leak; not REAPI).
- ❌ Sequential per-digest AuthZ (use parallel; latency budget tight).
- ❌ Cache D1 results em-memory per request (Worker é stateless; cache em KV WI-S02-005).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: GetBlob unary + FindMissingBlobs batch

  Background:
    Given Tenant A has blobs D_X (5 KB), D_Y (50 KB), D_Z (100 KB)
    And Tenant B has blob D_W (5 KB)
    And Tenant A is authenticated with PAT scope cache:r

  Scenario: GetBlob unary happy path (small blob)
    When Tenant A requests GetBlob(D_X)
    Then response status is OK
    And response.data == 5 KB content of D_X
    And response time p99 < 100ms warm
    And metric corelink_cas_get_blob_requests_total{result="hit"} incremented

  Scenario: GetBlob — blob too large
    Given Tenant A has blob D_BIG (5 MiB)
    When Tenant A requests GetBlob(D_BIG)
    Then response status is FAILED_PRECONDITION (HTTP 413)
    And error_code is COR_CAS_BLOB_TOO_LARGE
    And next_action hints "Use ByteStream::Read for blobs > 4 MiB"

  Scenario: GetBlob — cross-tenant attempt
    When Tenant A requests GetBlob(D_W)  # belongs to B
    Then response status is NOT_FOUND
    And metric corelink_cas_isolation_assertion_total{outcome="rejected"} incremented
    And audit event "corelink.cas.cross_tenant_attempt" emitted
    (404 não 403 — não distinguishable per CTRL-ISO-004; full constant-time em WI-S02-004)

  Scenario: FindMissingBlobs happy path
    Given digest list [D_X, D_Y, D_NEW1, D_NEW2, D_NEW3]
    When Tenant A requests FindMissingBlobs(digests)
    Then response.missing == [D_NEW1, D_NEW2, D_NEW3]
    And D_X, D_Y filtered out (already exist)
    And response time p99 < 200ms

  Scenario: FindMissingBlobs — cross-tenant masking
    Given digest list [D_X (A's), D_W (B's), D_NEW (truly missing)]
    When Tenant A requests FindMissingBlobs(digests)
    Then response.missing == [D_W, D_NEW]
    (D_W belongs to B; from A's perspective, "missing" — não cross-tenant existence oracle)
    And D_X (A's own) filtered out (exists in A scope)

  Scenario: FindMissingBlobs — batch size limit
    Given digest list with 1001 digests
    When Tenant A requests FindMissingBlobs
    Then response status is OUT_OF_RANGE (HTTP 413)
    And error_code is COR_CAS_BATCH_SIZE_EXCEEDED
    And max allowed = 1000

  Scenario: FindMissingBlobs — parallel AuthZ
    Given digest list with 1000 valid digests
    When request processed
    Then D1 AuthZ checks executed em parallel (Tokio join_all)
    And total p99 latency ≤ 200ms (não 1000 × 5ms = 5s sequential)

  Scenario: Property test — 10k batch enumeration attempts
    Given 10k random digests (sampled from cross-tenant universe)
    And Tenant A authenticated
    When FindMissingBlobs called
    Then 0 digests "found" that belong to Tenant B
    And response treats cross-tenant as "missing" consistently

  Scenario: REAPI v2 conformance
    Given bazelbuild/remote-apis test suite
    When GetBlob + FindMissingBlobs ops invoked
    Then 100% pass rate
```

## 9. Design Decisions

### 9.1 Why parallel AuthZ (Tokio join_all)

Sequential per-digest AuthZ em batch de 1000 = 1000 × 5ms = 5s — exceeds latency budget. Parallel via `join_all` com bounded concurrency (max 100 simultaneous D1 queries) = ~50ms total. D1 supports concurrent reads bem; bottleneck é Tokio scheduler, não D1.

### 9.2 Why max batch size 1000

REAPI v2 spec recommendation. Larger batches stress Worker memory + D1 connection pool. 1000 é sweet spot: covers 99% Bazel client batches, mantém latency tractable.

### 9.3 Why cross-tenant masked as "missing" (not 403)

CTRL-ISO-005 prevents existence oracle: atacante com PAT B não pode learn "Tenant A has blob_X". Response treats cross-tenant blobs como "missing"; atacante não distingue de truly missing. Combined com WI-S02-004 constant-time, defense-in-depth.

### 9.4 Why GetBlob unary inline 4 MiB threshold

- gRPC max message size default 4 MiB; aligning avoids fragmentation.
- HTTP/2 header overhead + JSON wrapping em larger blobs incurs cost; streaming Read é better fit.
- SDK auto-routing simplifies cliente decision.

### 9.5 Why no BatchReadBlobs

Bazel cliente consume sequentially; batch read não saves much vs N parallel GetBlobs (HTTP/2 multiplexing). Complexity (memory budget management for N inline blobs) vs benefit marginal. Defer pós-GA se demand emerge.

### 9.6 ADR potencial?

Não. Patterns reused do WI-S02-001 + REAPI v2 spec compliance.

## 10. Completeness Criteria SOTA

- [ ] **10.2.1** REAPI v2 conformance (bazelbuild/remote-apis) GetBlob + FindMissingBlobs 100% pass (EVT-002).
- [ ] **10.2.2** Property test 10k batch enumeration → 0 cross-tenant existence leak (EVT-002).
- [ ] **10.2.3** FindMissingBlobs p99 ≤ 200ms para batch de 1000 (criterion EVT-002).
- [ ] **10.2.4** GetBlob p99 ≤ 100ms warm; ≤ 150ms cold (EVT-021).
- [ ] **10.2.5** Cross-tenant masked as "missing" verified em integration test.
- [ ] **10.2.6** Cost regression gate (§14.10): batch hot path benchmark.

## 11. DoD

- [ ] gRPC + HTTP handlers impl.
- [ ] Parallel AuthZ via Tokio join_all com bounded concurrency.
- [ ] REAPI conformance test suite passa.
- [ ] Property test cross-tenant.
- [ ] Métricas + audit emit.
- [ ] error_taxonomy mapping completo.
- [ ] PRR + Architect + Security sign-off.

## 12. Invariants Validated

- INV-TENANT-ISOLATION (CRITICAL, TLA+): batch query honra isolation.
- INV-CAS-IMMUTABILITY (CRITICAL): tombstone respect.
- INV-CAS-IDEMPOTENCY (CRITICAL): same digest sempre same body byte-identical.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| gRPC unary handler | `crates/corelink-worker/src/reapi/cas_unary.rs` | Rust |
| gRPC batch handler | `crates/corelink-worker/src/reapi/cas_batch.rs` | Rust |
| HTTP REST surfaces | `crates/corelink-worker/src/http/cas_batch.rs` | Rust |
| Parallel AuthZ helper | `crates/corelink-worker/src/auth/parallel_check.rs` | Rust |
| REAPI conformance test | `tests/reapi_conformance/cas_read.rs` | Rust |
| Property test batch | `tests/prop_find_missing_batch.rs` | Rust |

## 14. Quality Standards SOTA

- **14.2.1** Zero unsafe; zero unwrap.
- **14.2.2** Documentação rustdoc + 3 examples.
- **14.2.3** Test coverage ≥ 90%.
- **14.2.4** FindMissingBlobs p99 ≤ 200ms 1000 digests; GetBlob p99 ≤ 100ms warm.
- **14.2.5** SAST clean.
- **14.2.6** Métricas RED.
- **14.2.7** Runbook RB-FM-253 reused.
- **14.2.8** Breaking REAPI changes = bump major + Bazel community engage.
- **14.2.9** Memory bounded: GetBlob 4 MiB + overhead ≤ 8 MiB peak.
- **14.2.10** Cost regression gate.

## 15. Chaos Experiments

1. **FindMissingBlobs com 1000 random digests** (mostly missing): verify parallel scaling.
2. **D1 throttle durante batch**: verify graceful degradation (no full request fail).
3. **Concurrent 50 batch requests**: verify Tokio scheduler doesn't oversubscribe D1.
4. **Adversarial enumeration**: 10k batch queries from same PAT; expect rate limit S-08 forward kicks in (post-S-08 SEALED).

## 16. PRR

PRR doc + Architect + Security + AppSec.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | gRPC GetBlob unary handler scaffold | 2h |
| ST-002 | gRPC FindMissingBlobs handler com parallel AuthZ | 4h |
| ST-003 | HTTP REST surfaces equivalentes | 2h |
| ST-004 | Parallel AuthZ helper (Tokio join_all bounded) | 2.5h |
| ST-005 | error_taxonomy mapping (3 new codes) | 1h |
| ST-006 | Cross-tenant masking logic (filter como "missing") | 1.5h |
| ST-007 | Métricas emit + dashboard integration | 1.5h |
| ST-008 | REAPI conformance test integration | 3h |
| ST-009 | Property test 10k cross-tenant enumeration | 2.5h |
| ST-010 | Criterion benchmark batch latency | 2h |
| ST-011 | Integration test E2E (Bazel pre-upload simulation) | 2h |
| ST-012 | rustdoc + examples | 1h |
| ST-013 | PRR + Security walkthrough | 1.5h |

**Total**: ~26.5h Optimistic; PERT ~31h.

## 18. Dependencies

### Hard blockers

- **WI-S02-001 SEALED** (auth middleware + ByteStream::Read pattern; reuses TenantCtx + AuthZ helper).
- **WI-S01-004 SEALED** (BlobMetaStore D1).

### Soft blockers

- WI-S02-004 (constant-time middleware) — desejável paralelo; este WI emit basic 404 sem full timing-padding (atacante poderia distinguir cross-tenant via timing pré-WI-004).

### Outbound

- WI-S02-005 (negative cache) — populate em FindMissingBlobs misses; lookup em GetBlob.
- WI-S15-001 (CLI) — `corelink ls` command consume FindMissingBlobs.

## 19. Effort PERT

O: 24h, M: 26.5h, P: 45h → PERT 29.5h.

## 20. Time-boxing

- 32h hard limit.
- Escalation ST-002 (parallel AuthZ) > 5h → Architect.

## 21. Observability

3 métricas listadas em §6.1 + dashboard widget DASH-CAS (Read tab).

## 22. Cost Analysis

- Per FindMissingBlobs 1000 digests: 1000 D1 reads × $1/M = $1/M ops.
- TCO 12m com 1M batch/dia × 365 = 365M batch × 1k digests = 365B D1 reads = ~$365/yr (well within budget).

## 23. API Contract

REAPI v2 proto-frozen.
HTTP equivalent semver-locked.
error_taxonomy COR_CAS_BLOB_TOO_LARGE + COR_CAS_BATCH_SIZE_EXCEEDED public.

## 24. Post-mortem Hooks

- Cross-tenant existence leak detected (atacante learns blob exists em outro tenant) → CRITICAL post-mortem + breach notification consideration.
- FindMissingBlobs p99 > 500ms sustained → 5-Why (D1 scaling ou Tokio scheduler).
- REAPI conformance regression → post-mortem + Bazel community engage.

## 25. Rollback / Recovery

Hot rollback via WASM previous version.

## 26. Security & Privacy

STRIDE: information disclosure (existence oracle) é THE primary threat — mitigated via cross-tenant masking + constant-time (WI-S02-004 forward).
LINDDUN: linkability — cross-tenant batch é privacy concern (CTRL-ISO-005 alignment); detectability — covered.

## 27. Knowledge Transfer

Tech talk: "REAPI v2 Batch Discovery — Why Bazel Lives or Dies on FindMissingBlobs Performance" — 30 min.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-tenant existence oracle via batch | L | M | CRITICAL | M | LOW | Per-digest AuthZ + masking + property test 10k |
| R-002 | Parallel AuthZ overwhelms D1 | M | M | MEDIUM (latency spike) | M | LOW | Bounded concurrency 100; D1 quota monitoring |
| R-003 | Batch size DoS (atacante storm 1000-digest queries) | M | L | MEDIUM | L | LOW | Rate limit per-PAT (S-08 forward) |
| R-004 | REAPI v2 spec interpretation drift Bazel | L | L | MEDIUM | L | LOW | Conformance suite CI per PR |
| R-005 | GetBlob memory bloat em concurrent 4 MiB requests | M | M | MEDIUM (FM-403) | M | LOW | 25 concurrent budget + monitoring |
| R-006 | Cost regression > 10% baseline | M | L | MEDIUM | L | LOW | §14.10 |

## 29. Review Checkpoints

1. Design (D+0): Architect + Security.
2. Code review (D+3): peer + Security.
3. Pre-merge: REAPI conformance + property test green.

## 30. Sign-off (HIGH_RISK 10-12)

11 roles incl. Architect (parallel AuthZ design) + Security (existence oracle review) + AppSec.

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.2.

## 32. Anti-patterns evitados

- ❌ Sequential per-digest AuthZ (latency disaster).
- ❌ Cross-tenant 403 distinct from 404 (existence oracle).
- ❌ Batch size unbounded (DoS vector).
- ❌ Cache D1 results em-memory per Worker request (stateless violation).
- ❌ Trust client-supplied digest list sem AuthZ filter.
- ❌ Metadata batch beyond REAPI spec (privacy leak).

---

**Fim WI-S02-002.** Próximo: WI-S02-003 (corelink-client-verify crate).
