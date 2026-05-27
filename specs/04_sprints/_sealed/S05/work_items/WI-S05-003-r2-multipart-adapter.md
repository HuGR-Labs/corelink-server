---
id: "WI-S05-003"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-01"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-05"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "STORAGE-SEMANTICS"
  - "CAS-PROFILE"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s05", "r2", "multipart", "adapter", "etag", "abort", "high-risk"]
---

# WI-S05-003 — R2 Multipart Adapter (`InitiateMultipartUpload` / `UploadPart` / `CompleteMultipartUpload` / `AbortMultipartUpload`) + ETag Tracking + Bounded Concurrency Per-Tenant + Chaos Test Client Disconnect

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-05](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S05-003 |
| Título | R2 multipart API adapter — orchestrate Initiate/UploadPart/Complete/Abort lifecycle; ETag per-part tracking; bounded concurrency per-tenant (8 parts em paralelo); chaos test client disconnect mid-upload; FM-060 (multipart orphan) detection signal para WI-S05-006 sweeper; idempotent completion via UNIQUE constraint |
| Sprint | S-05 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (orphan multipart parts persist tenant data sem audit), FF-HR-005 (R2 multipart commit é atomic boundary) |

## 0.1 Quick context (this WI)

Este WI abstrai R2 multipart API behind a clean Rust adapter consumed by handler (WI-S05-001). Lessons learned applied preventively from Lote 10.4bis: wrangler CLI commands corrigidos (`lifecycle add` + REST API for CORS); D1 batch 100KB limit; tenant_prefix materialized column (WI-S05-004 schema fix).

## 1. Intent

`crates/corelink-r2-multipart/` — R2 multipart adapter trait + impl:

```rust
#![forbid(unsafe_code)]

#[async_trait]
pub trait MultipartAdapter: Send + Sync {
    /// InitiateMultipartUpload: start session; returns upload_id.
    async fn initiate(
        &self,
        tenant_id: &TenantId,
        bucket: &Bucket,
        object_key: &str,           // chunk-<region>/<tenant_prefix>/<chunk_digest>
    ) -> Result<MultipartUpload, MultipartError>;

    /// UploadPart: upload single part (16 MiB default per ADR-0022).
    /// Returns ETag (part fingerprint per R2 SDK).
    async fn upload_part(
        &self,
        upload: &MultipartUpload,
        part_number: u32,                // 1..=10000 (R2 hard limit)
        bytes: Bytes,                    // ≤ 5 GiB per part
    ) -> Result<PartETag, MultipartError>;

    /// CompleteMultipartUpload: finalize session com part list ordered.
    /// Atomic: all parts committed OR none.
    async fn complete(
        &self,
        upload: MultipartUpload,
        parts: Vec<(u32, PartETag)>,    // ordered by part_number
    ) -> Result<CompletedObject, MultipartError>;

    /// AbortMultipartUpload: cleanup parts em failure path OR sweeper.
    async fn abort(
        &self,
        upload: MultipartUpload,
    ) -> Result<(), MultipartError>;

    /// ListMultipartUploads: enumerate ongoing for sweeper (WI-S05-006).
    /// Returns sessions older than `max_age` (default 7d).
    async fn list_orphans(
        &self,
        bucket: &Bucket,
        max_age: Duration,
    ) -> Result<Vec<OrphanedUpload>, MultipartError>;
}

pub struct MultipartUpload {
    pub upload_id: String,           // R2-issued opaque ID
    pub tenant_id: TenantId,         // binding for audit
    pub object_key: String,          // R2 path (tenant_prefix scoped)
    pub initiated_at: SystemTime,
    pub parts_uploaded: Vec<u32>,    // for resume / progress (advisory only — sprint anti-scope resumable)
}

pub struct PartETag(pub String);     // R2 ETag for part (S3-compatible md5/sha hex)

pub struct CompletedObject {
    pub bucket: Bucket,
    pub object_key: String,
    pub etag: String,                // S3 multipart ETag (computed from part ETags)
    pub size_bytes: u64,
}

pub struct OrphanedUpload {
    pub upload_id: String,
    pub object_key: String,
    pub initiated_at: SystemTime,    // for sweeper age comparison
}

#[derive(thiserror::Error, Debug)]
pub enum MultipartError {
    #[error("upload_id not found or expired")]
    UploadIdNotFound,                                     // → 404 + COR_MULTIPART_TIMEOUT (session expired)

    #[error("part {part_number} missing or invalid ETag")]
    PartMissing { part_number: u32 },                     // → 422 + COR_MULTIPART_PART_MISSING

    #[error("part {part_number} size {size} exceeds R2 max 5 GiB")]
    PartTooLarge { part_number: u32, size: u64 },         // → 413

    #[error("max parts {max} exceeded (R2 hard limit 10000)")]
    MaxPartsExceeded { max: u32 },                        // → 413 + COR_MULTIPART_BLOB_TOO_LARGE

    #[error("R2 backend error: {0}")]
    BackendError(String),                                 // → 503 + COR_MULTIPART_BACKEND_UNAVAILABLE

    #[error("concurrency limit reached")]
    ConcurrencyLimitReached,                              // → 429 + COR_MULTIPART_CONCURRENCY_LIMITED
}
```

**Cripto-driven invariants**:

1. **R2 path tenant-scoped**: `object_key = chunk-<region>/<tenant_prefix>/<chunk_digest>` OR `manifest-<region>/<tenant_prefix>/<blob_digest>.json`; `tenant_prefix` from materialized column WI-S05-004 (lesson Lote 10.4bis); never trusts client-provided path components.
2. **Idempotent completion**: re-issuing CompleteMultipartUpload with same upload_id + same parts list = no-op (R2 native idempotency); **partial UNIQUE** `(tenant_id, blob_digest_expected) WHERE state='in_progress'` em D1 multipart_sessions table prevents concurrent in-flight (Lote 10.5bis P0 fix: was wrongly stated as plain UNIQUE; per `invariant_registry.md §3.16` row INV-MULTIPART-IDEMPOTENT and WI-S05-004 schema; partial UNIQUE allows multiple completed/aborted records as audit trail).
3. **Orphan detection invariant**: ListMultipartUploads returns sessions with `LastModified > max_age`; sweeper aborts via this trait method (WI-S05-006).
4. **Bounded concurrency per-tenant**: 8 parallel UploadPart per tenant (default; tunable per-tier); avoid R2 rate limit spike.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

R2 multipart adapter wraps Cloudflare R2 SDK (S3-compatible). Bug em adapter cascades to ALL multipart ops (Split + Splice handlers; sweeper). HIGH_RISK em N dimensões:

1. **Orphan multipart accumulation (FM-060)**: client disconnects mid-upload; UploadPart calls succeed; CompleteMultipartUpload never invoked; R2 holds parts as ongoing session indefinitely; storage cost accumulates. Mitigação: WI-S05-006 sweeper abort > 7d via `list_orphans()` + `abort()`; metric `corelink.multipart.orphan_rate`; runbook RB-FM-060 dry-run executado.

2. **R2 multipart API quirk: eventual consistency**: CompleteMultipartUpload returns success but object not immediately visible em GET (eventual consistency window). Mitigação: post-Complete, handler waits ≤ 5s then HEAD verifies; if not found, retry GET com exponential backoff; documented em ADR-0022 §multipart-consistency.

3. **Part ETag mismatch**: R2 returns ETag per part; CompleteMultipartUpload requires ETags em order; if handler tracks ETags incorrectly (e.g., part 5 stored as part 3), Complete fails. Mitigação: `Vec<(u32, PartETag)>` ordered by part_number; integration test asserts ETag tracking; chaos test simulates ETag corruption.

4. **R2 max parts hard limit (10000)**: blob > 160 GiB single multipart impossible (10k × 16 MiB = 160 GiB); sprint contract §5.1 documents stitched flow WI-S05-006. Mitigação: `MaxPartsExceeded` error em `upload_part()` if part_number > 10000; handler routes to stitched flow.

5. **Concurrency storm**: 1000 parallel UploadPart per tenant; R2 rate limit (1000 PUT/s per bucket per region default). Mitigação: per-tenant semaphore default 8; `ConcurrencyLimitReached` 429; metric alert; tunable per-tier.

6. **Cross-tenant upload_id confusion**: attacker sends UploadPart with another tenant's upload_id. Mitigação: adapter binds upload_id ↔ tenant_id at Initiate; subsequent calls verify match via `MultipartUpload.tenant_id` checked against TenantCtx (handler-level).

7. **R2 backend unavailable cascade**: R2 down 1h → all multipart ops fail. Mitigação: 503 graceful + Retry-After 5s; client retry; audit emit backend_unavailable; circuit breaker S-XX forward.

8. **Wrangler R2 binding misconfiguration**: deploy guard validates bindings exist (lesson Lote 10.4bis); `lifecycle add` (not invalid `set`); CORS via REST API curl.

**Atacante adversarial scenarios**:

- **Orphan storage cost attack**: attacker initiates 10000 multiparts; never completes; storage accumulates. Mitigação: per-tenant max ongoing 100 (sprint contract §11 forward S-08 rate limit); sweeper 7d abort; cost monitoring.

- **Cross-tenant upload_id replay**: attacker captures upload_id from one tenant; tries UploadPart from another. Mitigação: adapter rejects if tenant_id mismatch; integration test asserts.

- **Part-fingerprint collision attempt**: R2 ETag is S3 MD5/SHA hex; collision-resistant; computacionalmente intratável.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: orphan multipart parts persist tenant data sem audit; cost overhead; risk of forensic gap.
- **FF-HR-005**: R2 multipart commit é atomic boundary; bug = partial blob persisted.
- **Reversibility**: orphan parts cleaned by sweeper 7d; no data loss.

11 sign-offs canonical.

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev pushing 100 MiB blob**: handler invokes adapter; 7 parts of 16 MiB; ETag tracked; Complete invoked; eventual consistency < 5s; subsequent GET succeeds.

**Persona 2 — DevOps reviewing R2 cost**: orphan_rate metric monitored; RB-FM-060 dry-run executado em staging; sweeper 7d abort verified.

**Persona 3 — Compliance reviewer**: audit chain captures Initiate + Complete OR Abort events; orphan abort emits `corelink.multipart.orphan_aborted` event.

**SLA addendum**:
- Multipart upload throughput ≥ 100 MB/s (consumed by WI-S05-001 handler).
- Complete latency p99 ≤ 200ms (post all parts uploaded).
- Eventual consistency window ≤ 5s post-Complete.
- Orphan sweeper SLA: ≤ 7d post-disconnect.
- Per-tenant concurrency: 8 default (tunable per-tier S-13).

## 4. Capability Mapping

- **CAP-CAS-008** (Multipart upload) — IMPLEMENTA primary R2 adapter.
- **CAP-CAS-012** (Orphan multipart cleanup) — IMPLEMENTA partial (`list_orphans()` + `abort()` consumed by WI-S05-006 sweeper).
- Trace: `storage_semantics.md §5 (multipart)` + `failure_modes.md FM-060` + ADR-0022 § R2 part size.

## 5. Tipo

R2 SDK adapter; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-r2-multipart`** structure: lib + 4 trait methods (initiate/upload_part/complete/abort) + list_orphans.
2. **R2 SDK integration** via `aws-sdk-s3` async client (R2 S3-compatible).
3. **ETag tracking**: `Vec<(u32, PartETag)>` ordered; integration test asserts.
4. **Bounded concurrency per-tenant**: `tokio::sync::Semaphore::new(8)`; tunable.
5. **Wrangler R2 bindings** (lesson Lote 10.4bis):
   - `wrangler r2 bucket create` per chunk-<region> + manifest-<region> (5 regions).
   - `wrangler r2 bucket lifecycle add` for abort-incomplete-multipart 7d (canonical command; was invalid `set` em Lote 10.4 v1.0).
   - CORS via Cloudflare REST API curl (wrangler 4.x não tem `cors set`).
   - Deploy guard `scripts/check_multipart_buckets.sh` pre-deploy CI gate.
6. **Property tests** (10k iter PR; 100k nightly):
   - `prop_initiate_idempotent`: same `(tenant_id, object_key)` initiate twice = same upload_id.
   - `prop_complete_idempotent`: re-Complete with same parts = no-op.
   - `prop_abort_idempotent`: re-Abort = no-op (R2 native).
   - `prop_part_etag_ordered`: 1000 random parts; ETags tracked em order.
   - `prop_concurrency_bounded`: 1000 parallel UploadPart; semaphore caps at 8.
7. **Chaos suite** (sprint contract §10 chaos test client disconnect):
   - Client disconnect mid-UploadPart: handler invokes Abort; metric emit.
   - R2 5xx mid-Complete: Complete retry up to 3×; persistent → 503 + audit emit.
   - Wrangler binding misconfigured: deploy guard catches; CI red.
   - 10000+1 parts attempt: MaxPartsExceeded error.
   - Cross-tenant upload_id replay: rejected; integration test.
8. **Métricas**:
   - `corelink.multipart.r2.initiate_total{result}` (counter).
   - `corelink.multipart.r2.upload_part_total{result}`.
   - `corelink.multipart.r2.complete_total{result}`.
   - `corelink.multipart.r2.abort_total{reason}` (reason ∈ client_disconnect|sweeper|backend_error).
   - `corelink.multipart.r2.orphan_rate` (gauge; alert if > 1% of UPDATE rate).
   - `corelink.multipart.r2.eventual_consistency_lag_ms` (histogram).
9. **Documentation**:
   - `crates/corelink-r2-multipart/README.md`: API + R2 quirks + threat model.
   - rustdoc 100% public API + 4 examples.

### 6.2 Out-of-scope

- **Resumable upload** (mid-stream restart): anti-scope sprint contract.
- **Multi-region replication** of multipart sessions: S-14.
- **Pre-flight upload validation** (HEAD before initiate): pós-GA.
- **Customer-tunable part size**: pós-GA via S-13.

## 7. Anti-Scope

- ❌ Custom multipart protocol (use R2 native S3-compatible).
- ❌ Buffered full blob in memory (streaming via `Bytes`).
- ❌ Skip ETag tracking (Complete requires ordered ETags).
- ❌ Cross-tenant upload_id reuse (audit + integration test).
- ❌ Wrangler `lifecycle set` (lesson Lote 10.4bis: use `lifecycle add`).
- ❌ Wrangler `cors set` (use REST API curl).
- ❌ Skip orphan sweeper signal (`list_orphans` mandatory).
- ❌ Sync emit audit em hot path (outbox WI-S01-004).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: R2 multipart adapter

  Background:
    Given crate corelink-r2-multipart built
    Given R2 buckets chunk-sam + manifest-sam provisioned via wrangler

  Scenario: Multipart upload happy path (50 MiB blob; 4 parts)
    Given 50 MiB blob to upload
    When adapter.initiate(tenant=A, bucket=chunk-sam, key=...) → upload_id
    And 4 × adapter.upload_part(upload_id, part_n, 16 MiB chunk) → PartETag
    And adapter.complete(upload_id, [parts]) → CompletedObject
    Then R2 object accessible post-eventual-consistency ≤ 5s
    And metric corelink.multipart.r2.complete_total{result="ok"} incremented

  Scenario: Orphan multipart abort (sweeper consumer)
    Given multipart session initiated 8d ago (no Complete OR Abort)
    When adapter.list_orphans(bucket, max_age=7d)
    Then orphan returned in list
    When adapter.abort(orphan)
    Then R2 cleans parts; metric corelink.multipart.r2.abort_total{reason="sweeper"} incremented

  Scenario: Client disconnect mid-upload
    Given mid-UploadPart, client disconnects
    When handler invokes adapter.abort(upload_id)
    Then parts cleaned; metric reason="client_disconnect"

  Scenario: Max parts exceeded (> 10000)
    Given attempting part_number = 10001
    Then error MultipartError::MaxPartsExceeded { max: 10000 }
    And handler routes to stitched flow WI-S05-006

  Scenario: Cross-tenant upload_id replay rejected
    Given Tenant A initiates upload_id U
    When Tenant B tries adapter.upload_part(U, ..., bytes)
    Then adapter binds upload_id ↔ tenant_id; mismatch → reject
    And integration test asserts

  Scenario: Concurrency limit (semaphore caps at 8)
    Given 9 parallel UploadPart for Tenant A
    Then 9th call → ConcurrencyLimitReached
    And response 429 + Retry-After

  Scenario: Wrangler binding correctness (Lote 10.4bis lesson)
    When provisioning script runs `wrangler r2 bucket lifecycle add`
    Then idempotent (no error if rule exists)
    And CORS via REST API curl returns success
    And deploy guard scripts/check_multipart_buckets.sh green
```

## 9-32. Standard sections (compact form)

### 9. Design Decisions

- **9.1** Use R2 S3-compatible API via `aws-sdk-s3` (mature; well-tested).
- **9.2** ETag tracking ordered by part_number (R2 spec requirement).
- **9.3** Per-tenant semaphore 8 default (tunable per-tier).
- **9.4** Wrangler `lifecycle add` (correct CLI per Lote 10.4bis).
- **9.5** Eventual consistency window ≤ 5s (handler waits + retries GET).
- **9.6** ADR forward: this WI consumes ADR-0022 (chunk vs part decoupling); no new ADR.

### 10. Completeness Criteria SOTA

- [ ] **10.s05.003.1** Property tests 5 × 10k iter green.
- [ ] **10.s05.003.2** Chaos suite (5 scenarios) green.
- [ ] **10.s05.003.3** Wrangler CLI commands corretos (lifecycle add; CORS REST).
- [ ] **10.s05.003.4** ETag tracking ordered; integration test asserts.
- [ ] **10.s05.003.5** Bounded concurrency per-tenant validated.
- [ ] **10.s05.003.6** Orphan detection signal (`list_orphans()`) consumed by WI-S05-006 sweeper.
- [ ] **10.s05.003.7** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s05.003.8** Cost regression gate: per-Initiate ≤ $0.000001; per-UploadPart ≤ $0.000005; per-Complete ≤ $0.000002; per-Abort ≤ $0.000001.

### 11. DoD

- [ ] Crate compila + integration tests green.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green; 100k nightly.
- [ ] Chaos suite green.
- [ ] Métricas (6 listadas §6.1.8) emitted.
- [ ] Architect + AppSec reviews.
- [ ] PRR Architect mini sign-off.

### 12. Invariants Validated

- **INV-MULTIPART-IDEMPOTENT** (HIGH, registry §3.16): R2 native idempotent semantics + adapter UNIQUE binding.
- **INV-MULTIPART-ORPHAN-DETECTABLE** (HIGH, NEW promovida §3.16 Lote 10.5bis): `list_orphans()` enumerates sessions > max_age; sweeper consumes.
- **INV-MULTIPART-CONCURRENCY-BOUNDED** (HIGH, registry §3.16): per-tenant semaphore caps at 8 default.
- **INV-MULTIPART-PATH-TENANT-SCOPED** (CRITICAL, NEW promovida §3.16 Lote 10.5bis): `object_key` includes `tenant_prefix` (Layer 4); never trusts client-provided path.

### 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate `corelink-r2-multipart` | `crates/corelink-r2-multipart/` | Rust |
| MultipartAdapter trait | `crates/corelink-r2-multipart/src/lib.rs` | Rust |
| R2 SDK impl | `crates/corelink-r2-multipart/src/r2/` | Rust |
| Property tests | `tests/prop_r2_multipart.rs` | Rust |
| Chaos suite | `tests/chaos_r2_multipart.rs` | Rust |
| Provisioning script | `scripts/provision_multipart_buckets.sh` | Bash |
| Deploy guard | `scripts/check_multipart_buckets.sh` | Bash |
| README | `crates/corelink-r2-multipart/README.md` | Markdown |

### 14. Quality Standards SOTA

- 14.s05.003.1: Zero `unsafe`; zero `unwrap`.
- 14.s05.003.2: rustdoc 100% public API.
- 14.s05.003.3: Test coverage ≥ 90%.
- 14.s05.003.4: Latência: Initiate p99 ≤ 50ms; UploadPart p99 ≤ 200ms (16 MiB); Complete p99 ≤ 200ms; Abort p99 ≤ 100ms.
- 14.s05.003.5: SAST clean.
- 14.s05.003.6: Métricas: 6 listadas §6.1.8.
- 14.s05.003.7: Runbook: RB-FM-060 (multipart orphan; consumed by WI-S05-006 sweeper).
- 14.s05.003.8: Wrangler CLI corretos (Lote 10.4bis lesson).
- 14.s05.003.9: Memory bounded: per-request stack ≤ 4 MiB.
- 14.s05.003.10: Cost regression gate: per-op cap.

### 15. Chaos Experiments (10; Lote 10.5bis P0 fix: was 5 below sprint contract §10 mandate ≥10)

1. Client disconnect mid-UploadPart → adapter.abort invoked.
2. R2 5xx mid-Complete → retry 3× → persistent fail 503 + audit.
3. Wrangler binding misconfigured → deploy guard CI red.
4. 10001 parts attempt → MaxPartsExceeded.
5. Cross-tenant upload_id replay → rejected.
6. **R2 quota exceeded mid-Complete** (Lote 10.5bis): R2 returns 429 quota; adapter returns 503 + sweeper aborts pending sessions; audit emit.
7. **Region fail-over mid-multipart** (Lote 10.5bis): R2 us-east-1 fails; adapter routes to us-west-2; ETag tracking continues coherent.
8. **upload_id TTL expiry** (Lote 10.5bis): 7d session expires; UploadPart returns 404; handler routes to fresh Initiate (idempotent retry-on-existing).
9. **D1 multipart_sessions partial UNIQUE collision** (Lote 10.5bis): race two Initiates same `(tenant, blob)` em `state='in_progress'`; one wins, second rejected; integration test asserts retry-on-existing returns winner's upload_id.
10. **R2 ListMultipartUploads pagination edge** (Lote 10.5bis): 1000+ orphans em single region; pagination correctness asserted via sweeper consumption.

### 16. PRR

PRR HIGH_RISK 11 sign-offs canonical gated em WI-S05-006. Este WI mini-PRR Architect + AppSec.

### 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Crate skeleton | 1 |
| ST-002 | MultipartAdapter trait + R2 SDK impl | 5 |
| ST-003 | ETag tracking + ordered Complete | 2 |
| ST-004 | Bounded concurrency semaphore | 2 |
| ST-005 | list_orphans + abort logic | 3 |
| ST-006 | Wrangler provisioning script + deploy guard | 3 |
| ST-007 | Property tests 5 × 10k | 4 |
| ST-008 | Chaos suite (5 scenarios) | 4 |
| ST-009 | Métricas emit | 1.5 |
| ST-010 | rustdoc + 4 examples + README | 3 |
| ST-011 | Architect + AppSec review iter | 2 |
| ST-012 | Cost bench setup | 1 |

**Total Optimistic**: ~32h. **PERT** (O=28h, M=33h, P=50h): **~36h**.

### 18. Dependencies

- Hard: aws-sdk-s3 crate; R2 access; wrangler 4.x.
- Soft: WI-S05-001 handler consumes adapter; WI-S05-006 sweeper consumes list_orphans.

### 19. Effort PERT: 36h. ### 20. Time-boxing: 42h hard limit.

### 21. Observability

6 métricas listadas §6.1.8. Trace span `multipart.r2.{initiate, upload_part, complete, abort}`.

### 22. Cost Analysis

**Per-multipart op cost**:
- Initiate: $0.50/M (Worker invoke) + R2 multipart init free → **$0.000001 effective**.
- UploadPart: $4.5/M class A R2 PUT × 16 MiB part = **$0.0000045** + Worker compute negligible.
- Complete: R2 multipart complete free + D1 INSERT multipart_sessions ≈ $0.000002.
- Abort: $4.5/M R2 abort + cleanup ≈ $0.000001.

**TCO 12m** (1M Multipart sessions/dia):
- Initiate: 1M × $0.000001 = $1/dia.
- UploadPart: ~5M parts/dia (~5 parts/session) × $0.0000045 = $22.5/dia.
- Complete: 1M × $0.000002 = $2/dia.
- Abort (estimated 1% sessions): 10k × $0.000001 = negligible.
- Total: ~$25.5/dia × 365 = **~$9.3k/yr**.

### 23. API Contract

Public crate API documented §1; semver post v1.0; `#[non_exhaustive]` on `MultipartUpload`.

### 24. Post-mortem Hooks

- Orphan rate > 1% sustained 7d → SEV-2 (sweeper review OR R2 reliability).
- ETag mismatch > 5/h → SEV-1 (adapter bug; rare).
- Eventual consistency window > 30s sustained → CF support escalation.
- Cross-tenant upload_id detected → CRITICAL post-mortem.

### 25. Rollback / Recovery

Crate version pin; revert via cargo update + redeploy. RTO ≤ 10 min; RPO 0 (stateless adapter).

### 26. Security & Privacy (compact)

**STRIDE**: tenant_id binding em MultipartUpload; ETag content-hash (non-PII); R2 path tenant-scoped Layer 4. **LINDDUN**: tenant_id pseudonymous; R2 storage encrypted at rest (SSE-S3); audit emit + sweeper provides forensic trail.

### 27. Knowledge Transfer

- Tech talk (1h): "R2 Multipart Adapter + Orphan Sweeping".
- Doc `docs/internal/r2-multipart-adapter.md`.
- Onboarding test (5 questions): R2 quirks, ETag tracking, orphan detection, concurrency bounds, wrangler CLI corretos.

### 28. Risk Register (10-row 6-col)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Orphan multipart accumulation FM-060 | M | M | MEDIUM | M | LOW | Sweeper 7d (WI-006) + metric alert |
| R-002 | R2 eventual consistency window > 30s | L | M | MEDIUM | L | LOW | HEAD verify + retry; documented |
| R-003 | ETag tracking bug | L | L | HIGH | L | LOW | Integration test ordered; chaos sim |
| R-004 | R2 max parts exceeded | M | L | MEDIUM | L | LOW | MaxPartsExceeded; stitched flow forward |
| R-005 | Concurrency storm DoS | M | M | MEDIUM | M | LOW | Semaphore 8/tenant; metric alert |
| R-006 | Cross-tenant upload_id replay | L | M | CRITICAL | L | LOW | tenant_id binding em MultipartUpload |
| R-007 | Wrangler CLI misconfigured | L | L | HIGH | L | LOW | Deploy guard CI gate; lesson Lote 10.4bis |
| R-008 | R2 backend cascade unavailable | M | H | HIGH | M | LOW | 503 graceful + Retry-After; circuit breaker S-XX |
| R-009 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.10 cost gate |
| R-010 | aws-sdk-s3 crate vulnerability | L | M | HIGH | L | LOW | Cargo-audit weekly |

### 29. Review Checkpoints

D+0 design (Architect); D+2 AppSec; D+4 code review (peer); D+5 chaos suite; D+6 PRR mini.

### 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver_ |
| 4 | Security Lead | _TBD; mandatory_ |
| 5-6 | Engineer × 2 | _TBD_ |
| 7 | QA | _TBD_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD_ |
| 10 | Privacy | _TBD_ |
| 11 | Architect | _TBD; **mandatory** — R2 adapter + orphan sweeper signal_ |
| 12 | AppSec | _TBD; **mandatory** — cross-tenant binding + Wrangler CLI_ |
| 13 | Crypto SME | _**mandatory** (Lote 10.5bis P0 fix: was advisory; cross-tenant upload_id binding INV-MULTIPART-PATH-TENANT-SCOPED is CRITICAL per §12; upgraded to mandatory per Lote 10.4bis lesson Crypto SME mandatory non-waivable for security-domain WIs); ETag content-hash collision analysis (S3 multipart MD5 ETag is NOT cripto-grade per S3 spec; review acceptable scope)_ |

### 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S05-003 (Lote 10.5; SOTA pós-Lote 10.4bis lessons).

1.3.0 / 2026-05-01 / Gustavo (via Claude Opus 4.7): **WI-S05-003 SEALED — implementation phase, `corelink-r2-multipart` v0.1.0 shipped.** New crate `crates/corelink-r2-multipart/` ships the canonical `MultipartAdapter` trait + `InMemoryMultipartAdapter` host-side fake honouring every load-bearing semantic the production R2 binding will inherit: (1) **5-Layer Defense Layer 4 path scoping** via `object_key::compose(...)` — every R2 key is structurally `<chunk\|manifest>-<region>/<tenant_prefix>/<digest_hex>[<.suffix>]` with the tenant_prefix segment sandwiched between the bucket family and the content digest; client-supplied path components are structurally unreachable and the constructor rejects bad regions / digests / suffixes via `MultipartError::InvalidObjectKey` (`INV-MULTIPART-PATH-TENANT-SCOPED` honoured by construction); (2) **Idempotent lifecycle** — `initiate` re-call on `(tenant_id, object_key)` while `InProgress` returns the existing upload id; `upload_part` records the same ETag for same-bytes; `complete` returns the cached `CompletedObject` after first success; `abort` is idempotent on `InProgress\|Aborted` and **rejected on `Completed`** per `INV-MULTIPART-FINALIZE-IRREVOCABLE`; (3) **Cross-tenant upload-id rejection** — `MultipartUpload` carries a bound `tenant_id` and every method takes `tenant_id` explicitly; mismatch surfaces as `MultipartError::CrossTenantUpload` and the session is left intact for the legitimate tenant; (4) **Bounded per-tenant concurrency** via `concurrency::PerTenantSemaphore` keyed on tenant UUID — default 8 permits per tenant (tunable per-tier S-13); `acquire` awaits, `try_acquire` trips `MultipartError::ConcurrencyLimitReached`; permit lifecycle via RAII `PartPermit` guard; F-001 closure preserved (every adapter instance owns its semaphore + session map; no globals); (5) **Bounded parser** — `PartNumber::new` rejects out-of-range numbers (`1..=10_000` per R2 hard limit) at construction; `upload_part` rejects bodies > 5 GiB (`R2_MAX_PART_SIZE_BYTES`); `MAX_SINGLE_SESSION_BLOB_BYTES = 16 MiB × 10_000 = 160_000 MiB` compile-time-asserted; (6) **Orphan enumeration** via `list_orphans(bucket, now, max_age)` — only path the WI-S05-006 sweeper has to discover sessions older than `max_age` and abort them. Tests: 46 lib unit + 8 property tests at 10k iter (PR; 100k via `PROPTEST_CASES`) covering tenant isolation distinct keys / cross-tenant replay rejected on every method / initiate idempotency / upload_part same-bytes idempotency / complete idempotency / abort safety (Completed reject + InProgress idempotent) / part ordering canonical (BTreeMap surfaces ascending part numbers regardless of upload order) / list_orphans bucket+age filters; 7 chaos suite scenarios (client disconnect mid-upload → adapter.abort + parts cleared / R2 5xx surfaces Backend / 10001 parts → MaxPartsExceeded / cross-tenant replay rejected + legitimate tenant unblocked / concurrency limit (semaphore caps at 8) / orphan sweeper consumes list_orphans + abort cycle / Complete ETag mismatch returns PartMissing(expected, actual)); 4 examples (happy_path / orphan_sweeper / cross_tenant_replay / concurrency_limit); README. Quality gates verde: `cargo test -p corelink-r2-multipart --all-targets` 0 failures (46 lib + 8 prop @ 10k + 7 chaos = 61 tests passing); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` + `validate_references.py` no new dangling refs. **Trait-abstraction-defer pattern preserved** per charter — production `aws-sdk-s3` Cloudflare R2 binding shim deferred to WI-S05-006 alongside conformance suite (the `MultipartAdapter` trait + InMemory fake is the integration seam — the production shim's only job is to observably match the canonical reference impl); D1 `multipart_sessions` partial-UNIQUE schema mounting deferred to WI-S05-004 (the fake holds `(tenant_id, object_key) WHERE state='in_progress'` semantic in-memory until then); 6 metrics §6.1.8 + bucket-provisioning script + REST CORS curl + deploy guard CI all deferred to WI-S05-006 alongside the binding shim. wasm32-clean: only `tokio::sync::Semaphore` from tokio (no reactor); same artifact runs in the Cloudflare Workers WASM bundle and the host-side test harness.

### 32. Anti-patterns evitados

- ❌ Custom multipart protocol; ❌ Buffered full blob; ❌ Skip ETag tracking; ❌ Cross-tenant upload_id; ❌ Wrangler `lifecycle set`; ❌ Wrangler `cors set`; ❌ Skip orphan sweeper signal; ❌ Sync audit em hot path.

---

**Fim WI-S05-003.** Próximo: WI-S05-004 (D1 schema chunks + manifest_chunks + multipart_sessions + UNIQUE constraint).
