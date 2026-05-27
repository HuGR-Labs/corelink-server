---
id: "WI-S01-003"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s01", "cas", "r2", "storage", "tenant-isolation", "single-blob"]
---

# WI-S01-003 — R2 Adapter Single-Blob (≤ 5 MiB) com HMAC Tenant Path

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-003 |
| Título | R2 Adapter Single-Blob com HMAC tenant path |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (tenant isolation via path prefix), FF-HR-005 (CTRL-AUTH-004 HMAC enforcement) |
| Tier | Todos (CAS storage hot path) |
| Fase produto | Fase 1 — Remote Cache |

## 1. Intent

Implementar **`R2Writer` + `R2Reader`** em `crates/corelink-worker/src/storage/r2.rs` que encapsula toda interação com Cloudflare R2 buckets para CAS single-blob ≤ 5 MiB. Shape canônico (v1.1, pós split-em-duas-camadas):

```rust
pub struct R2Writer<B: R2Backend> { /* region + Arc<B> + Arc<dyn MetricsObserver> */ }
impl<B: R2Backend> R2Writer<B> {
    pub fn new(region: Region, backend: Arc<B>) -> Self;
    pub fn with_metrics(region: Region, backend: Arc<B>, metrics: Arc<dyn MetricsObserver>) -> Self;
    pub async fn put(&self, ctx: &TenantCtx, vb: &VerifiedBody) -> Result<PutOutcome, R2Error>;
    pub fn for_tenant<'a>(&'a self, ctx: &'a TenantCtx) -> ScopedR2Writer<'a, B>; // impl BlobStoreWrite
}

pub struct R2Reader<B: R2Backend> { /* same shape as R2Writer */ }
impl<B: R2Backend> R2Reader<B> {
    pub async fn get(&self, ctx: &TenantCtx, digest: &Digest) -> Result<Bytes, R2Error>;
}

// Backend trait abstraction — InMemoryR2 fake here, CF binding shim in WI-S01-005.
pub trait R2Backend: Send + Sync {
    async fn put_if_none_match(&self, key: &str, body: Bytes) -> Result<BackendPutOutcome, R2Error>;
    async fn get(&self, key: &str) -> Result<Bytes, R2Error>;
    async fn head(&self, key: &str) -> Result<bool, R2Error>;
}
```

`PutOutcome` ∈ `{Fresh, Duplicate}` para o metric-distinguishable surface; o `BlobStoreWrite::put_verified` trait surface (em `corelink-hash`) mapeia ambos para `Ok(())` — duplicate é semanticamente success per INV-CAS-IDEMPOTENCY.

Path canônico (REG-NAMESPACE-001..005; HMAC16 = `b64(HMAC_SHA256(TDK, tenant_id))[0:16]`):
```
cas-<region>/<HMAC16>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>
```

Onde `TenantPrefix` é construído via S-01 WI-S01-001 `corelink-tenant-path` crate. `TenantCtx::new(&tdk, tenant_id, region)` deriva o prefix internamente (caller não pode forjar — fechamento P0 do round-1 codex review). `VerifiedBody` é o envelope from WI-S01-002 (already verified integrity).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

R2 é o blob storage ground truth do CoreLink CAS. **Cada write/read passa por este adapter**; bug aqui afeta 100% das blobs de 100% dos tenants. Há duas falhas catastróficas:

**Falha A: Cross-tenant key construction** (FM-253). Se path construction não usa `TenantPrefix` derivation correto (e.g., bug usa `tenant_id` plaintext, ou prefix de tenant errado), blob de Tenant A pode ser persistido em namespace de Tenant B → cross-tenant exposure imediata em next read.

**Falha B: Eventual consistency violation**. R2 oferece "strong read-after-write consistency for new objects, eventual consistency for overwrites em older regions". Se code permite overwrites (não reject duplicate digests), atacante pode racing-write substituir blob legítimo por payload manipulado.

Mitigação:
1. **Path construction sealed**: only via `R2Writer::put` que chama `TenantPath::derive(ctx.tenant_id)` internamente; impossível bypassar.
2. **`If-None-Match: *`** header em PutObject — R2 reject duplicate writes; consistency garantida via INSERT-OR-NOTHING.
3. **R2 SSE-S3** at-rest encryption (CTRL-CRYPTO-002 INV-CONF-AT-REST).
4. **Per-region buckets** (`cas-wnam`, `cas-weur`, `cas-sam`): tenant region pinning enforce em writer construction; cross-region writes physically impossible se bucket bind correto.
5. **TLS 1.3 only** Worker→R2 (CF default; INV-CONF-IN-FLIGHT).
6. **Strong read-after-write** for new objects (R2 guarantee for objects written após CompleteMultipartUpload OR PutObject não-overwrite).

R2 SDK choice: native Cloudflare Workers binding (`env.R2_BUCKET.put(key, value)`) vs aws-sdk-s3 compatible. Cloudflare native é faster (zero-copy entre Worker e R2; same datacenter network) e tem strong consistency for new objects. Trade-off: vendor lock; mitigated via abstraction trait `BlobStore` que pode swap futuro se needed.

**Risk justification HIGH_RISK:**
- **FF-HR-002**: path construction errada = INV-TENANT-ISOLATION violation; catastrophic blast radius.
- **FF-HR-005**: implementa CTRL-AUTH-004 (HMAC tenant prefix) na storage layer.
- **Reversibility**: blob escrito em path errado é one-way-door — recovery requer manual sweep + customer notification.

Por isso exige: 11 sign-offs canonical HIGH_RISK, TLA+ tenant_isolation.tla cobrindo storage layer, property test 100k iter cross-tenant, chaos test R2 latency injection, RB-FM-253 dry-run.

## 3. Customer Impact & Journey

**JTBD:** "Como tenant, preciso garantia que meu PUT vai para o meu namespace exclusivo em R2 — não vaza para outros tenants — e que meu GET sempre retorna o mesmo body que foi escrito (não corrupted by overwrite race)."

**Journey:**
- **Direct**: cada CAS write/read passa por R2 adapter.
- **Customer-visible**: latency p99 PUT < 1s (5 MiB); GET < 300ms cold.
- **Indirect**: foundation pra S-02 read path + S-05 multipart + S-06 GC physical delete.

## 4. Capability Mapping

- **CAP-CAS-002** (tenant isolation criptográfica via HMAC prefix) — IMPLEMENTA storage layer.
- Trace: `auth_model.md §8.1 layer 5 (R2 key prefix)` + `storage_semantics_matrix.md §3.1 (R2 namespace rules)`.

## 5. Tipo e Classificação

- **Tipo:** Foundation (CAS storage)
- **Lane:** HIGH_RISK
- **Lane forcing factors:** FF-HR-002, FF-HR-005

## 6. Escopo

> **v1.1 architectural note (2026-04-29):** the WI originally said the
> Cloudflare native R2 binding (`worker::R2Bucket`) lands in this WI. After
> implementation review (codex round 1, 2026-04-29) we split the adapter
> into a **two-layer design**: WI-S01-003 ships the tenant-path /
> `If-None-Match` / error-mapping / metrics core behind a small
> [`R2Backend`] trait abstraction (with an in-memory test fake exercising
> every code path); the **real Cloudflare binding adapter** that wraps
> `env.R2_CAS_<REGION>` lands as a thin shim in **WI-S01-005** alongside
> the REAPI handler, where miniflare/wrangler-dev is available to drive
> integration tests on the real binding. This split keeps the
> tenant-isolation / immutability / residency invariants
> deploy-target-independent and trait-testable. INV-DATA-RESIDENCY is
> enforced here at the writer/reader boundary (region-mismatch returns
> `COR_INTERNAL`); the binding-level enforcement (bucket name
> must match `R2_CAS_<REGION>` env var) is the WI-S01-005 scope.

### 6.1 In-scope

1. **`R2Writer<B: R2Backend>` struct** + `put(&self, &TenantCtx, &VerifiedBody) -> Result<PutOutcome, R2Error>`:
   - Path derivation via `corelink_tenant_path::derive_prefix(&tdk, tenant_id)` — internalizado no `TenantCtx::new(&tdk, tenant_id, region)` constructor (caller não pode forjar).
   - Key format: `cas-<region>/<HMAC16>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>` (HMAC16 canonical per remote_cache_product_profile.md §7.1).
   - `R2Backend::put_if_none_match(key, body)` semantics — backend trait abstraction (real CF R2 binding usa `If-None-Match: *`; in-memory test fake replica idempotency).
   - SSE-S3 — runtime verify deferido para WI-S01-005 (real binding tier); enabled at bucket-level via Terraform pre-S-01.
2. **`R2Reader<B: R2Backend>` struct** + `get(&self, &TenantCtx, &Digest) -> Result<Bytes, R2Error>`:
   - Path derivation idêntico (mesmo tenant_id sob mesmo TDK → mesmo prefix).
   - `R2Backend::get(key)` — respeita 404 (`R2Error::NotFound`); cross-tenant também surface 404 uniform (ADR-0028; closes enumeration oracle).
   - `Bytes` returned para caller (S-02 read path consume).
3. **Unified `BlobStore` (read + write) trait** abstrata para futuro swap:
   ```rust
   pub trait BlobStore: Send + Sync {
       type Ctx;
       type Error: StdError + Send + Sync + 'static;
       fn put_verified<'a>(&'a self, ctx: &'a Self::Ctx, vb: &'a VerifiedBody)
           -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
       fn get<'a>(&'a self, ctx: &'a Self::Ctx, digest: &'a Digest)
           -> impl Future<Output = Result<Bytes, Self::Error>> + Send + 'a;
   }
   pub struct R2BlobStore<B: R2Backend> { /* writer + reader pinned to same Region */ }
   impl<B: R2Backend> BlobStore for R2BlobStore<B> { type Ctx = TenantCtx; type Error = R2Error; ... }
   ```
   Use Rust 2024 RPITIT (return-position `impl Future`) — sem `#[async_trait]` macro pull. `R2BlobStore::new` rejects writer/reader region mismatch with `R2BlobStoreRegionMismatch`. The narrower write-only seam `corelink_hash::BlobStoreWrite` (used em `R2Writer::for_tenant`) is separate; `BlobStore` is the unified seam for the read path / GC sweeper.
4. **Per-region bucket binding**: 3 regiões S-01 (WNAM, WEUR, SAM); env vars `R2_CAS_WNAM`, `R2_CAS_WEUR`, `R2_CAS_SAM`.
5. **Métricas**:
   - `corelink.storage.r2.put_duration_seconds_bucket{region, blob_size_bucket}`.
   - `corelink.storage.r2.put_total{region, result}` (result ∈ {ok, conflict_duplicate, error}).
   - `corelink.storage.r2.get_duration_seconds_bucket{region}`.
6. **Error taxonomy mapping**:
   - `COR_CAS_BLOB_NOT_FOUND` em GET miss (também em cross-tenant prefix mismatch — uniform 404 fecha enumeration oracle per ADR-0028).
   - `COR_CAS_BLOB_TOO_LARGE` em body > 5 MiB (multipart é WI-S05-003).
   - `COR_INTERNAL` em region mismatch entre `ctx.region()` e writer/reader pinned region (programmer error em dispatcher; clientes não podem driveear esse code, então 500 é o classifier correto e está canonicalmente listado em `error_taxonomy.md` linha 218).
   - `COR_SERVICE_DEGRADED` em R2 5xx / transport faults.
   - **Note**: `If-None-Match: *` rejection (R2 412) **não é erro** — o duplicate write é idempotente per INV-CAS-IDEMPOTENCY e surface como `Ok(PutOutcome::Duplicate)` no writer; a métrica `result="conflict_duplicate"` é emitida pelo writer no path. (A v1.0 do WI mencionava um `COR_CAS_DUPLICATE_REJECTED` taxonomy code; isso foi corrigido em v1.1 — o code não existe e não é necessário, dado que duplicate é Ok semanticamente.)

### 6.2 Out-of-scope (deferred)

- **Real Cloudflare R2 binding shim** (`env.R2_CAS_<REGION>` → `R2Backend`):
  WI-S01-005 (REAPI handler integration; miniflare-driven tests).
- **Multipart upload** (blobs > 5 MiB): WI-S05-003.
- **R2 streaming download** (chunked bytes): WI-S02-001 (read path).
- **Cross-region replication**: S-14 (region failover).
- **R2 lifecycle rules** (cold tier transition): S-06 GC interaction.
- **SSE-S3 verification at runtime** (R2 admin API check): WI-S01-005 deploy
  smoke + EVT-028 quarterly. SSE-S3 is enabled at bucket-creation time via
  Terraform (pre-S-01); the adapter trusts the bucket-level setting.

## 7. Anti-Scope (expandido para HIGH_RISK)

- ❌ aws-sdk-s3 compatibility (CF R2 native binding only at GA).
- ❌ Pre-signed URLs cliente-direct (anti-pattern; sempre via Worker para enforce auth + verify).
- ❌ R2 ACL granular per-object (per-bucket SSE só; tenant isolation via path prefix).
- ❌ Multipart < 5 MiB threshold tuning (CF R2 padrão 5 MiB; aceitar).
- ❌ R2 versioning (blobs immutable post-write; INV-CAS-IMMUTABILITY enforced).
- ❌ Custom retry logic (PAT-RETRY-IDEMPOTENT-001 reused do CF SDK).
- ❌ R2 metadata custom além do necessário (digest é o key; metadata bloat = cost).
- ❌ Read-after-write polling (R2 strong consistency for new objects).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: R2 single-blob adapter

  Background:
    Given Tenant A authenticated with tenant_id "uuid-A"
    And TenantPrefix(A) HMAC16 (b64 URL-safe, 16 chars) = "ABC123XYZ4567PQR" (illustrative; real HMAC16 derived from TDK)
    And R2 bucket cas-wnam is provisioned

  Scenario: Successful PUT — happy path
    Given VerifiedBody { body=5KB, digest=D_X }
    When R2Writer.put(ctx_A, vb) called
    Then R2 PutObject called with key "cas-wnam/ABC123XYZ4567PQR/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>"
    And If-None-Match: * header present (real binding; in-memory fake replicates the semantics)
    And SSE-S3 encryption active at bucket-level (Terraform pre-S-01; runtime admin-API verify is WI-S01-005 / EVT-028)
    And Result::Ok(PutOutcome::Fresh) returned
    And metric corelink.storage.r2.put_total{result="ok"} incremented

  Scenario: Cross-tenant attempt — different tenant_id different prefix
    Given Tenant B with prefix "XYZ789..."
    When R2Writer.put(ctx_B, vb) called
    Then R2 key starts with "cas-wnam/XYZ789..." (NOT ABC123)
    And key is verifiably distinct from any Tenant A path

  Scenario: Idempotent duplicate PUT
    Given digest D_X already exists in R2
    When R2Writer.put(ctx_A, vb) called again with same digest
    Then R2 PutObject returns 412 Precondition Failed (If-None-Match: * mismatch)
    And R2Writer.put maps to Ok(PutOutcome::Duplicate); the BlobStoreWrite trait surface flattens this to Ok(()) (idempotent semantics)
    And metric corelink.storage.r2.put_total{result="conflict_duplicate"} incremented
    (cliente sees success; first writer wins; INV-CAS-IDEMPOTENCY ensures both bodies are byte-identical)

  Scenario: Property test — path determinism
    Given any (tenant_id, digest) pair
    When path is derived twice
    Then both paths byte-identical
    (100k iter via proptest, drives R2Writer::put end-to-end)

  Scenario: Property test — cross-tenant disjointness
    Given any pair of distinct tenant_ids under the same TDK + region
    When the canonical R2 keys are derived end-to-end via R2Writer::put
    Then 0 collisions across distinct tenant_ids
    (100k iter via proptest; expected birthday-bound collisions over 100k random pairs ≈ 10^-23)
    (HMAC injectivity + property test)

  Scenario: GET happy path
    Given blob D_X persisted at "cas-wnam/ABC123XYZ4567PQR/blake3/.../<hex>"
    When R2Reader.get(ctx_A, D_X) called
    Then R2 GetObject called with same key
    And Bytes returned matches original body
    And metric corelink.storage.r2.get_duration_seconds_bucket recorded

  Scenario: GET miss — 404
    Given digest D_Y does not exist in R2
    When R2Reader.get(ctx_A, D_Y) called
    Then R2 GetObject returns 404
    And Result::Err(StoreError::NotFound) returned
    And error mapped to COR_CAS_BLOB_NOT_FOUND

  Scenario: Cross-tenant read attempt
    Given blob D_Z persisted in Tenant A namespace
    When R2Reader.get(ctx_B, D_Z) called
    Then R2 GetObject called with key for Tenant B prefix (not A's)
    And R2 returns 404 (key não existe em B's path)
    And caller sees NotFound (não cross-tenant data leak)

  Scenario: R2 5xx error
    Given R2 returns 500 Internal Server Error
    When R2Writer.put called
    Then Result::Err(StoreError::Backend) returned
    And mapped to COR_SERVICE_DEGRADED (HTTP 503)
    And SDK retry policy triggers exponential backoff (PAT-RETRY-IDEMPOTENT-001)

  Scenario: Per-region binding correctness
    Given Worker provisioned with R2_CAS_WNAM binding to bucket "cas-wnam"
    When R2Writer constructed with region=Wnam
    Then writer points to cas-wnam bucket
    And keys start with "cas-wnam/" prefix
```

## 9. Design Decisions

### 9.0 Why two-layer adapter (trait + binding shim) — added v1.1

Cloudflare's `worker::R2Bucket` only exists at deploy time inside the Workers
runtime; it is unavailable in host-side `cargo test` builds. Putting the
binding directly in this WI would either (a) require miniflare in every
property-test run (slow, brittle), or (b) leave the tenant-path /
idempotency / metrics / taxonomy logic deploy-target-coupled and untestable
on the host. The two-layer split keeps every load-bearing invariant
(REG-NAMESPACE-001..005, INV-CAS-IDEMPOTENCY, INV-DATA-RESIDENCY)
trait-testable today and defers only the thin binding shim
(`worker::R2Bucket` → `R2Backend`) to WI-S01-005, where miniflare and the
REAPI handler integration tier are already in scope. The binding shim is
≤ 100 LoC mechanical wrap; the load-bearing logic is here and 100% covered.

### 9.1 Why CF R2 native binding (não aws-sdk-s3)

- **Performance**: CF native binding é zero-copy entre Worker e R2 (same datacenter); aws-sdk-s3 incurs HTTP overhead.
- **Strong consistency for new objects**: CF R2 documenta strong RAW for new objects via PutObject (não overwrite); aws-sdk-s3 abstraction não exposes this guarantee directly.
- **Cost**: CF native bindings are free; aws-sdk-s3 incurs CPU cost for HTTP serialization.
- **Trade-off**: vendor lock-in mitigated via `BlobStore` trait abstraction; swap impl se needed.

### 9.2 Why `If-None-Match: *` header

R2 PutObject default permite overwrites. Para CAS, queremos **fail-on-overwrite** semantics (INV-CAS-IMMUTABILITY): same digest sempre maps to same body; race-write impossible.

`If-None-Match: *` instructs R2: only PUT se key não existe; if exists, return 412 Precondition Failed. Convertemos 412 em Ok(()) — idempotent semantics (writer 1 e writer 2 com mesmo body veem ambos sucesso; physical write feito apenas uma vez).

### 9.3 Why per-region buckets (não global single bucket)

- **Latency**: tenant pinned EU lê de `cas-weur` localmente, não atravessa Atlantic.
- **Residency** (S-14 forward): EU tenant data physically stays em EU bucket; INV-DATA-RESIDENCY enforced at storage layer.
- **Blast radius**: bucket misconfiguration affects only one region.
- **Cost**: same R2 pricing per region; no overhead.

### 9.4 Why path with sharding (`<hex[0:2]>/<hex[2:4]>`)

- **R2 limits**: single prefix has practical limits (rate limits + listing perf).
- **Sharding**: 256 × 256 = 65k sub-prefixes spread load.
- **Listing performance**: GC sweep (S-06) lista per-prefix; 65k prefixes parallelizable.

### 9.5 ADR potencial?

Não identificada decisão arquitetural disruptiva nova. CF native binding é canonical choice documented em framework + remote_cache_product_profile.

## 10. Completeness Criteria SOTA

- [x] **10.3.1** Property test 100k iter cross-tenant disjointness 0 collisions (EVT-002) — `prop_cross_tenant_keys_distinct_100k` drives `R2Writer::put` end-to-end.
- [ ] **10.3.2** TLA+ tenant_isolation.tla cobre storage layer (camada 5) sustained CI (EVT-022) — sprint-level deliverable.
- [ ] **10.3.3** R2 SSE-S3 verify active via R2 admin API check (EVT-028 quarterly) — deferred to WI-S01-005 deploy.
- [x] **10.3.4** Idempotent duplicate PUT semantic verified em integration test (EVT-002) — `concurrent_same_tenant_duplicate_writes_idempotent` (100 concurrent same-tenant) + `chaos_4_concurrent_writes_across_distinct_tenants` (100 concurrent cross-tenant).
- [ ] **10.3.5** RB-FM-253 (cross-tenant read) dry-run executed (EVT-017) — deferred to S-01 sprint-close ceremony.
- [x] **10.3.6** Per-region binding verified at the adapter layer (3 regions: WNAM/WEUR/SAM) — `per_region_binding_isolates_buckets` + `region_mismatch_writer_rejects_with_internal` + `region_mismatch_reader_rejects_with_internal`. Real bucket-binding deploy verification in WI-S01-005.
- [ ] **10.3.7** Cost regression gate (§14.10): R2 PutObject per-MiB cost benchmark sustained (EVT-002) — deferred to WI-S01-005 (real binding). The single-blob 5 MiB upper bound is already enforced (`single_blob_limit_bytes_is_exactly_5_mib`).

## 11. Definition of Done

- [x] `R2Writer` + `R2Reader` impl completos (`crates/corelink-worker/src/storage/r2.rs`).
- [x] `BlobStore` (read+write) trait + `R2BlobStore` impl + `BlobStoreWrite` seam wired (`storage/blob_store.rs`).
- [x] Per-region adapter pinning (3 regions: WNAM/WEUR/SAM) com region-mismatch enforcement; real env-var binding in WI-S01-005.
- [x] Path derivation via `corelink-tenant-path::derive_prefix` — `TenantCtx::new(&tdk, tenant_id, region)` derives prefix internally; caller cannot forge.
- [x] `If-None-Match: *` semantics via `R2Backend::put_if_none_match`; idempotent duplicate → `Ok(PutOutcome::Duplicate)` per INV-CAS-IDEMPOTENCY.
- [ ] SSE-S3 verify — deferred to WI-S01-005 (real binding); bucket-level Terraform setting.
- [x] error_taxonomy mapping — `COR_CAS_BLOB_NOT_FOUND` / `COR_CAS_BLOB_TOO_LARGE` / `COR_INTERNAL` / `COR_SERVICE_DEGRADED` via `R2Error::taxonomy_code`.
- [x] Métricas emitidas via `MetricsObserver` trait + `InMemoryMetrics` test sink + `NoopMetrics` default; wire to Workers Analytics in S-09.
- [x] Property test 100k iter green — drives `R2Writer::put` end-to-end (`prop_cross_tenant_keys_distinct_100k`).
- [x] Integration test E2E (PUT → R2 → GET → byte-identical) — `put_then_get_round_trip` + `blob_store_trait_round_trip`.
- [ ] RB-FM-253 dry-run — sprint-close ceremony.
- [ ] Code review por 2 peers + Architect + Security lead — solo-tier waiver per ADR-0034 (HIGH_RISK lane staffing-blocked at S-01); codex adversarial review serves as the surrogate signal until staffing.

## 12. Invariants

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): camada 5 (R2 path prefix) implementada aqui.
- **INV-CAS-IMMUTABILITY** (CRITICAL): `If-None-Match: *` enforces write-once.
- **INV-CAS-IDEMPOTENCY** (CRITICAL): idempotent PUT (same body → mesmo R2 path → first-writer-wins; semantically Ok pra ambos).
- **INV-CONF-AT-REST** (HIGH): R2 SSE-S3 active.
- **INV-CONF-IN-FLIGHT** (HIGH): TLS 1.3 Worker→R2.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate root + public API | `crates/corelink-worker/src/lib.rs` | Rust source |
| `Region` enum + bucket-name mapping | `crates/corelink-worker/src/region.rs` | Rust source |
| `TenantCtx` (TDK-derived prefix) | `crates/corelink-worker/src/tenant.rs` | Rust source |
| Storage module organizer | `crates/corelink-worker/src/storage.rs` | Rust source |
| `R2Error` taxonomy + codes | `crates/corelink-worker/src/storage/error.rs` | Rust source |
| Canonical key constructor | `crates/corelink-worker/src/storage/key.rs` | Rust source |
| `MetricsObserver` trait + `InMemoryMetrics` test sink + `NoopMetrics` default | `crates/corelink-worker/src/storage/metrics.rs` | Rust source |
| Unified `BlobStore` (read+write) trait + `R2BlobStore` impl | `crates/corelink-worker/src/storage/blob_store.rs` | Rust source |
| `R2Writer` / `R2Reader` / `R2Backend` trait / `InMemoryR2` fake | `crates/corelink-worker/src/storage/r2.rs` | Rust source |
| Canonical key regression vectors | `crates/corelink-worker/tests/canonical_keys.rs` | Rust test |
| Property test 100k iter cross-tenant disjointness | `crates/corelink-worker/tests/prop_r2_path.rs` | Rust test |
| Integration test E2E + chaos #4 | `crates/corelink-worker/tests/integration_r2.rs` | Rust test |
| Fuzz harness — path construction | `crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs` | Rust fuzz |
| Fuzz harness — round-trip + cross-tenant oracle | `crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs` | Rust fuzz |
| CI workflow (PR + nightly fuzz/mutants/audit) | `.github/workflows/corelink-worker.yml` | YAML |
| **Real CF binding shim** (deferred) | `crates/corelink-worker/src/storage/r2_cf_binding.rs` (TBD) | **WI-S01-005** |
| **Per-region wrangler bindings** (deferred) | `wrangler.toml` (env vars `R2_CAS_<REGION>`) | **WI-S01-005** |

## 14. Quality Standards SOTA

- **14.3.1** Zero unsafe; zero unwrap.
- **14.3.2** Documentação rustdoc + 3 examples.
- **14.3.3** Test coverage ≥ 90%.
- **14.3.4** PUT p99 ≤ 800ms (5 MiB blob); GET p99 ≤ 250ms warm.
- **14.3.5** SAST clean.
- **14.3.6** Métricas RED.
- **14.3.7** Runbook: RB-FM-253 reused.
- **14.3.8** Breaking changes em BlobStore trait = bump major.
- **14.3.9** Memory bounded: single-blob (≤ 5 MiB) path materializes the body as `bytes::Bytes` (one allocation, ref-counted) since the entire payload is already resident in the Worker request body. Streaming/chunked download is WI-S02-001 scope; multipart upload (> 5 MiB) is WI-S05-003 scope. The 5 MiB cap is enforced at the writer (`SINGLE_BLOB_LIMIT_BYTES`).
- **14.3.10** Cost regression gate (§14.10).

## 15. Chaos Experiments

1. **R2 latency injection** (500ms): verify retry + circuit breaker behavior.
2. **R2 5xx errors 1%**: verify exponential backoff + final error mapping.
3. **R2 region failover** (CF region offline): verify graceful degrade vs failover (S-14 future).
4. **Concurrent duplicate writes**: 100 concurrent PUT same digest different tenants → expect 1 success per tenant (paths distinct).

## 16. Production Readiness Review

PRR doc em `specs/04_sprints/S01/PRR-WI-S01-003.md`. Sign-offs 11 canonical incluindo Architect (R2 SDK choice review).

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Module scaffold + BlobStore trait | 1.5h |
| ST-002 | R2Writer impl com path derivation | 3h |
| ST-003 | If-None-Match idempotent semantics | 1.5h |
| ST-004 | R2Reader impl | 2h |
| ST-005 | Per-region binding via env vars + wrangler.toml | 2h |
| ST-006 | error_taxonomy mapping em StoreError | 1h |
| ST-007 | Métricas emit + dashboard widget | 1.5h |
| ST-008 | Property test 100k cross-tenant disjointness | 2.5h |
| ST-009 | Integration test E2E PUT→GET | 2h |
| ST-010 | RB-FM-253 dry-run execution | 1.5h |
| ST-011 | rustdoc + examples | 1h |
| ST-012 | PRR doc + Architect walkthrough | 1.5h |

**Total**: ~21h Optimistic; PERT-weighted ~26h.

## 18. Dependencies

### Hard blockers

- **WI-S01-001 SEALED** (TenantPath crate).
- **WI-S01-002 SEALED** (VerifiedBody envelope).
- **R2 buckets provisioned** (Terraform — pre-S-01).

### Outbound

- WI-S01-005 (REAPI handler) — invokes R2Writer.
- WI-S02-001 (read path) — uses R2Reader.
- WI-S05-003 (R2 multipart) — extends pattern.
- WI-S06-004 (physical delete) — uses R2Writer.delete.

## 19. Effort Estimate (PERT)

- O: 18h, M: 21h, P: 38h
- PERT = (18 + 84 + 38)/6 = **23.3h**

## 20. Time-boxing & Escalation

- Time-box 28h hard limit.
- Escalation ST-002 > 5h → Architect.

## 21. Observability Plan

Métricas listadas em §6.1; dashboard widget em DASH-CAS (Storage tab).

## 22. Cost Analysis

- R2 PutObject: $0.36/M ops + $0.015/GB egress.
- TCO 12m (10M writes/dia × 1 MiB avg × 12m × 30d = 3.6 PB): ~$1300 ops + $54k egress = ~$55k/yr.

## 23. API Contract Impact

`BlobStore` trait é semver-locked at v1.x.x. R2 native binding semver tied a wrangler version.

## 24. Post-mortem Hooks

- Cross-tenant write detected → CRITICAL post-mortem.
- R2 SSE-S3 disabled em prod → CRITICAL post-mortem.
- Idempotent semantics violated (overwrite slipped) → CRITICAL.

## 25. Rollback / Recovery

- Hot rollback: deploy previous WASM.
- Recovery: blob écritas em wrong path detectable via R2 list + path validate; manual sweep.

## 26. Security & Privacy

STRIDE: tampering rejected via If-None-Match; spoofing prevented via TenantPath enforcement; info disclosure mitigated cross-tenant via path prefix.
LINDDUN: linkability — tenant_id hash em path = pseudonymous; identifiability — digest é content-derived.

## 27. Knowledge Transfer

Tech talk: "R2 + HMAC Tenant Path: 5-Layer Defense in Action" — record. Doc `docs/internal/r2-storage-pattern.md`.

## 28. Risk Register

| ID | Risco | P | D | I | E | R | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Path construction bug → cross-tenant write | L | M | CRITICAL | M | LOW | Property test 100k + TLA+ + RB-FM-253 |
| R-002 | If-None-Match não enforced em CF R2 | L | L | HIGH | L | LOW | Integration test verifies; cargo-audit upstream |
| R-003 | SSE-S3 desabilitada em prod (config drift) | L | M | HIGH | L | LOW | Quarterly EVT-028 + alarm |
| R-004 | Per-region binding misconfigured | M | L | HIGH | M | LOW | Wrangler.toml CI verify + integration test |
| R-005 | R2 5xx storm exhausts retry budget | M | M | MEDIUM | M | LOW | PAT-RETRY-IDEMPOTENT-001 + circuit breaker |
| R-006 | Path sharding limits hit (single prefix > 100k objects) | L | L | LOW | L | LOW | 65k prefixes design; documented |

## 29. Review Checkpoints

1. Design (D+0): Architect approves R2 SDK choice.
2. Code review intermédio (D+2): peer 1.
3. Code review final (D+4): peer 2 + Security lead.
4. Adversarial (pre-merge): cross-tenant property test inspection.
5. Pre-merge: PRR + criterion + chaos test.

## 30. Sign-off (HIGH_RISK 11 canonical)

[11 roles canonical HIGH_RISK (per framework §33.5.4.3; Owner + Final Approver já incluídos no count) — Architect (R2 review) + Security + Crypto SME (HMAC review) + ...]

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S01-003 (Lote 10.1). |
| 1.1.0 | 2026-04-29 | Gustavo (S-01 implementation Lote — WI-S01-003 SEAL post codex rounds 1-7; final 9.1/10 SEAL: GRANTED) | **WI-S01-003 SEALED** com codex round-1 3.8/10 → round-2 8.1/10 → round-3 target ≥ 8.5/10 fixes: **Round-1 batch (P0+P1):** (1) `TenantCtx::new` agora deriva o prefix internamente a partir de `(&TDK, tenant_id)` — caller-supplied prefix era P0 cross-tenant misrouting hole; (2) `R2Writer.put` / `R2Reader.get` enforcement de `ctx.region() == self.region` retornando `RegionMismatch` (era dead data); (3) real CF binding adapter formalmente diferido para WI-S01-005 (split em duas camadas: tenant-path/idempotency core aqui, miniflare-driven CF binding shim no S-01-005); (4) métricas observable via `MetricsObserver` trait; (5) `BlobTooLarge` taxonomy code corrigido para `COR_CAS_BLOB_TOO_LARGE`; (6) `BlobStore` (read+write) trait unified seam; (7) property test 100k iter agora drives `R2Writer::put` end-to-end (não mais hand-rolled mirror); (8) chaos #4 separado em same-tenant + cross-tenant cases. **Round-2 batch (2 P1 + 3 P2):** (a) `RegionMismatch` taxonomy code corrigido `COR_VALIDATION_FAILED` → canonical `COR_INTERNAL` (linha 218 do error_taxonomy.md; programmer error, clientes não driveiam o code); (b) `MetricsObserver` extendido com `record_put_duration(region, blob_size_bucket, Duration)` + `record_get_duration(region, Duration)` + canonical `PutSizeBucket` enum (Le1KiB/Le16KiB/Le256KiB/Le1MiB/Le5MiB) — completa o WI §6.1.5 trio (counter + put histogram by size + get histogram); (c) `R2BlobStore::new` agora returnable `Result<_, R2BlobStoreRegionMismatch>` — write-WNAM/read-WEUR mis-pairing rejected at construction; (d) métricas error-label tests adicionados pra oversize/backend/region-mismatch paths (`metrics_observer_records_error_labels_on_oversize_and_backend_fault`); (e) doc fix em `storage/key.rs`: canonical key length corrigido de "96..98" pra "102/103". **Test surface final**: 38 tests verde (2 unit + 6 canonical + 23 integration + 6 property + 1 doctest); property test 100k iter green em 117s debug / 5.5s release; 2 fuzz harnesses 60s 641k runs zero panics; cargo-mutants kill rate ≥ 80%; CI workflow `corelink-worker.yml` PR + nightly lanes; WASM target build verified (workspace-level `uuid` v7 feature dropped — nobody uses v7 generation, gives clean wasm32-unknown-unknown build). **Spec patches in same Lote**: §6 architectural split note v1.1; §6.1.6 error taxonomy mapping corrigido (drop hallucinated `COR_CAS_DUPLICATE_REJECTED`; `RegionMismatch` → `COR_INTERNAL`); §6.2 deferred-to-S-01-005 list expandida; §11 DoD updated; §9.0 trait-abstraction rationale adicionado. |

## 32. Apêndice: Anti-patterns evitados

- ❌ Pre-signed URL cliente-direct (bypass auth).
- ❌ Single global bucket (residency violation).
- ❌ Trust client-supplied path (always derive from ctx).
- ❌ aws-sdk-s3 wrap (native binding only).
- ❌ R2 versioning (immutability via If-None-Match).
- ❌ Manual retry sem backoff.

---

**Fim WI-S01-003.** Próximo: WI-S01-004 (D1 schema + refcount).
