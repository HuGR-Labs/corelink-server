---
id: "WI-S01-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
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

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
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

Implementar **`R2Writer` + `R2Reader`** em `crates/corelink-worker/src/storage/r2.rs` que encapsula toda interação com Cloudflare R2 buckets para CAS single-blob ≤ 5 MiB:

```rust
pub struct R2Writer { region: Region, bucket: R2Bucket, tenant_path: TenantPath }
impl R2Writer {
    pub async fn put(&self, ctx: TenantCtx, vb: VerifiedBody) -> Result<(), R2Error>;
}

pub struct R2Reader { region: Region, bucket: R2Bucket, tenant_path: TenantPath }
impl R2Reader {
    pub async fn get(&self, ctx: TenantCtx, digest: Digest) -> Result<Bytes, R2Error>;
}
```

Path canônico (REG-NAMESPACE-001..005):
```
cas-<region>/<TenantPrefix-base32-16chars>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>
```

Onde `TenantPrefix` é construído via S-01 WI-S01-001 `corelink-tenant-path` crate (HMAC-derived). `VerifiedBody` é o envelope from WI-S01-002 (already verify'd integrity).

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

Por isso exige: 10–12 sign-offs, TLA+ tenant_isolation.tla cobrindo storage layer, property test 100k iter cross-tenant, chaos test R2 latency injection, RB-FM-253 dry-run.

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

### 6.1 In-scope

1. **`R2Writer` struct** + `put(ctx, VerifiedBody) -> Result<()>`:
   - Path derivation via `TenantPath::derive(ctx.tenant_id)`.
   - Key format: `cas-<region>/<prefix-base32>/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>`.
   - PutObject com `If-None-Match: *` (reject duplicate; idempotent semantics).
   - SSE-S3 enabled (default em CF R2; verify ativo).
2. **`R2Reader` struct** + `get(ctx, digest) -> Result<Bytes>`:
   - Path derivation idêntico (mesmo tenant_id → mesmo prefix).
   - GetObject; respect 404 vs 403 (CTRL-ISO-004; full constant-time é WI-S02-004 scope, mas this WI emit basic 404).
   - Bytes returned para caller (S-02 read path consume; S-01 não expõe read endpoint mas R2Reader é usado em integration tests).
3. **`BlobStore` trait** abstrata para futuro swap:
   ```rust
   #[async_trait]
   pub trait BlobStore {
       async fn put(&self, ctx: TenantCtx, vb: VerifiedBody) -> Result<(), StoreError>;
       async fn get(&self, ctx: TenantCtx, digest: Digest) -> Result<Bytes, StoreError>;
   }
   impl BlobStore for R2Writer { ... }
   ```
4. **Per-region bucket binding**: 3 regiões S-01 (WNAM, WEUR, SAM); env vars `R2_CAS_WNAM`, `R2_CAS_WEUR`, `R2_CAS_SAM`.
5. **Métricas**:
   - `corelink.storage.r2.put_duration_seconds_bucket{region, blob_size_bucket}`.
   - `corelink.storage.r2.put_total{region, result}` (result ∈ {ok, conflict_duplicate, error}).
   - `corelink.storage.r2.get_duration_seconds_bucket{region}`.
6. **Error taxonomy mapping**:
   - `COR_CAS_BLOB_NOT_FOUND` em GET miss.
   - Genérico `COR_SERVICE_DEGRADED` em R2 5xx.
   - `COR_CAS_DUPLICATE_REJECTED` em PutObject `If-None-Match` rejection (idempotent retry path).

### 6.2 Out-of-scope (deferred)

- **Multipart upload** (blobs > 5 MiB): WI-S05-003.
- **R2 streaming download** (chunked bytes): WI-S02-001 (read path).
- **Cross-region replication**: S-14 (region failover).
- **R2 lifecycle rules** (cold tier transition): S-06 GC interaction.

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
    And TenantPrefix(A) base32-16 = "ABC123XYZ4567PQR"
    And R2 bucket cas-wnam is provisioned

  Scenario: Successful PUT — happy path
    Given VerifiedBody { body=5KB, digest=D_X }
    When R2Writer.put(ctx_A, vb) called
    Then R2 PutObject called with key "cas-wnam/ABC123XYZ4567PQR/blake3/<hex[0:2]>/<hex[2:4]>/<full-hex>"
    And If-None-Match: * header present
    And SSE-S3 encryption active
    And Result::Ok returned
    And metric corelink_storage_r2_put_total{result="ok"} incremented

  Scenario: Cross-tenant attempt — different tenant_id different prefix
    Given Tenant B with prefix "XYZ789..."
    When R2Writer.put(ctx_B, vb) called
    Then R2 key starts with "cas-wnam/XYZ789..." (NOT ABC123)
    And key is verifiably distinct from any Tenant A path

  Scenario: Idempotent duplicate PUT
    Given digest D_X already exists in R2
    When R2Writer.put(ctx_A, vb) called again with same digest
    Then R2 PutObject returns 412 Precondition Failed (If-None-Match: * mismatch)
    And R2Writer maps to Ok(()) — idempotent semantics
    And metric corelink_storage_r2_put_total{result="conflict_duplicate"} incremented
    (cliente sees success; first writer wins; INV-CAS-IDEMPOTENCY ensures both bodies are byte-identical)

  Scenario: Property test — path determinism
    Given any (tenant_id, digest) pair
    When path is derived twice
    Then both paths byte-identical
    (10k iter via proptest)

  Scenario: Property test — cross-tenant disjointness
    Given 1000 distinct tenant_ids and 1000 random digests
    When all 1M paths derived
    Then 0 collisions across distinct tenant_ids
    (HMAC injectivity + property test)

  Scenario: GET happy path
    Given blob D_X persisted at "cas-wnam/ABC123XYZ4567PQR/blake3/.../<hex>"
    When R2Reader.get(ctx_A, D_X) called
    Then R2 GetObject called with same key
    And Bytes returned matches original body
    And metric corelink_storage_r2_get_duration_seconds_bucket recorded

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

- [ ] **10.3.1** Property test 100k iter cross-tenant disjointness 0 collisions (EVT-002).
- [ ] **10.3.2** TLA+ tenant_isolation.tla cobre storage layer (camada 5) sustained CI (EVT-022).
- [ ] **10.3.3** R2 SSE-S3 verify active via R2 admin API check (EVT-028 quarterly).
- [ ] **10.3.4** Idempotent duplicate PUT semantic verified em integration test (EVT-002).
- [ ] **10.3.5** RB-FM-253 (cross-tenant read) dry-run executed (EVT-017).
- [ ] **10.3.6** Per-region binding verified em deploy (3 regions: WNAM/WEUR/SAM) (EVT-027).
- [ ] **10.3.7** Cost regression gate (§14.10): R2 PutObject per-MiB cost benchmark sustained (EVT-002).

## 11. Definition of Done

- [ ] `R2Writer` + `R2Reader` impl completos.
- [ ] `BlobStore` trait + impl.
- [ ] Per-region binding via env vars (3 regions).
- [ ] Path derivation via TenantPath crate.
- [ ] `If-None-Match: *` em PutObject.
- [ ] SSE-S3 verify.
- [ ] error_taxonomy mapping.
- [ ] Métricas emitidas.
- [ ] Property test 100k iter green.
- [ ] Integration test E2E (PUT → R2 → GET → byte-identical).
- [ ] RB-FM-253 dry-run.
- [ ] Code review por 2 peers + Architect + Security lead.

## 12. Invariants

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): camada 5 (R2 path prefix) implementada aqui.
- **INV-CAS-IMMUTABILITY** (CRITICAL): `If-None-Match: *` enforces write-once.
- **INV-CAS-IDEMPOTENCY** (CRITICAL): idempotent PUT (same body → mesmo R2 path → first-writer-wins; semantically Ok pra ambos).
- **INV-CONF-AT-REST** (HIGH): R2 SSE-S3 active.
- **INV-CONF-IN-FLIGHT** (HIGH): TLS 1.3 Worker→R2.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| R2 adapter module | `crates/corelink-worker/src/storage/r2.rs` | Rust source |
| BlobStore trait | `crates/corelink-worker/src/storage/mod.rs` | Rust source |
| Per-region config | `crates/corelink-worker/src/config/storage.rs` | Rust source |
| Property test | `crates/corelink-worker/tests/prop_r2_path.rs` | Rust test |
| Integration test E2E | `crates/corelink-worker/tests/integration_r2.rs` | Rust test |
| Wrangler binding config | `wrangler.toml` (per-region R2 bindings) | TOML |

## 14. Quality Standards SOTA

- **14.3.1** Zero unsafe; zero unwrap.
- **14.3.2** Documentação rustdoc + 3 examples.
- **14.3.3** Test coverage ≥ 90%.
- **14.3.4** PUT p99 ≤ 800ms (5 MiB blob); GET p99 ≤ 250ms warm.
- **14.3.5** SAST clean.
- **14.3.6** Métricas RED.
- **14.3.7** Runbook: RB-FM-253 reused.
- **14.3.8** Breaking changes em BlobStore trait = bump major.
- **14.3.9** Memory bounded: R2 SDK doesn't buffer full blob; chunk streaming pattern.
- **14.3.10** Cost regression gate (§14.10).

## 15. Chaos Experiments

1. **R2 latency injection** (500ms): verify retry + circuit breaker behavior.
2. **R2 5xx errors 1%**: verify exponential backoff + final error mapping.
3. **R2 region failover** (CF region offline): verify graceful degrade vs failover (S-14 future).
4. **Concurrent duplicate writes**: 100 concurrent PUT same digest different tenants → expect 1 success per tenant (paths distinct).

## 16. Production Readiness Review

PRR doc em `specs/04_sprints/S01/PRR-WI-S01-003.md`. Sign-offs 10-12 incluindo Architect (R2 SDK choice review).

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

## 30. Sign-off (HIGH_RISK 10-12)

[Owner + Final Approver + 11 roles incluindo Architect (R2 review) + Security + Crypto SME (HMAC review)]

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S01-003 (Lote 10.1). |

## 32. Apêndice: Anti-patterns evitados

- ❌ Pre-signed URL cliente-direct (bypass auth).
- ❌ Single global bucket (residency violation).
- ❌ Trust client-supplied path (always derive from ctx).
- ❌ aws-sdk-s3 wrap (native binding only).
- ❌ R2 versioning (immutability via If-None-Match).
- ❌ Manual retry sem backoff.

---

**Fim WI-S01-003.** Próximo: WI-S01-004 (D1 schema + refcount).
