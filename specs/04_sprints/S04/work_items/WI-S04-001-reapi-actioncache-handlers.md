---
id: "WI-S04-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-04"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "STORAGE-SEMANTICS"
  - "CAS-PROFILE"
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s04", "ac", "reapi", "grpc", "rest", "action-cache", "high-risk"]
---

# WI-S04-001 — REAPI v2 ActionCache Handlers (GetActionResult + UpdateActionResult) + gRPC + REST Surface + Tenant Context Propagation

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-04](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S04-001 |
| Título | REAPI ActionCache `GetActionResult` + `UpdateActionResult` handlers (gRPC tonic + REST axum); auth middleware reuse S-03; tenant context propagation; idempotent UpdateActionResult via `(tenant_id, action_digest)` UNIQUE; negative cache integration; conformance test suite hooks |
| Sprint | S-04 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (handler bug → wrong tenant returns ActionResult = FM-303 catastrophic), FF-HR-005 (CTRL-AC-001/002 enforcement boundary) |

## 1. Intent

Implementar `crates/corelink-worker/src/reapi/ac.rs` — handlers REAPI v2 `ActionCache::GetActionResult` + `UpdateActionResult` que dão à CoreLink seu valor central pra Bazel/Buck2:

```text
GetActionResult flow:
  Request gRPC (Bazel client) → Tower auth_stack (S-03 WI-S03-003) → TenantCtx ext →
    [1] negative_cache::lookup(action_digest)  → KV `ac_neg:<tenant_prefix>:<action_digest>` TTL 60s (canonical em data_model.md §6.1 pós-Lote 10.4bis amendment)
    [2] ac_meta::lookup(tenant_id, action_digest) → D1 SELECT
    [3] r2::get(ac-<region>/<tenant_prefix>/<action_digest>.json) → AC envelope (signed)
    [4] sig::verify(envelope, tenant_key) → corelink-ac WI-S04-004 HKDF verify
    [5] outputs_check::warn-if-missing (1% sampled per Lote 10.4bis P0 fix; full check em UPDATE) → INV-AC-OUTPUTS-VALID daily reconcile (WI-S04-003)
    [6] ttl::refresh-on-hit(tenant_id, action_digest) → D1 UPDATE last_hit_at, expires_at += renewal
    [7] audit::emit(ac.get.ok | ac.get.miss)
  Response: ActionResult proto OR 404 COR_AC_ACTION_NOT_FOUND (negative cache populated)

UpdateActionResult flow:
  Request gRPC → Tower auth_stack → TenantCtx ext →
    [1] scope_check::cache_w → 403 if missing
    [2] body::decode → ActionResult proto (REAPI v2 schema)
    [3] merkle::verify(result, tenant_id) → corelink-ac (WI-S04-003) pre-persist; reject invalid Merkle
    [4] outputs::resolve_blobs(result.output_files, result.output_directories) → blob_meta lookups
    [5] outputs::assert_alive(blob_refs) → INV-AC-OUTPUTS-VALID enforcement; 422 if any tombstoned
    [6] sig::sign(envelope, tenant_key) → HKDF-SHA256 (WI-S04-004); info="ac-sig"
    [7] r2::put(ac-<region>/<tenant_prefix>/<action_digest>.json) → atomic write
    [8] ac_meta::insert_or_idempotent(tenant_id, action_digest, blob_refs) → D1 ON CONFLICT
    [9] negative_cache::invalidate(action_digest) → KV.delete
    [10] audit::emit(ac.update.ok | ac.update.merkle_invalid | ac.update.outputs_missing)
  Response: ActionResult proto echo OR error
```

```rust
// Public types (corelink-worker reapi module)
pub struct ActionDigest { pub hash: String, pub size_bytes: i64 } // REAPI v2 Digest

pub trait ActionCacheHandler {
    async fn get_action_result(
        &self,
        ctx: &TenantCtx,
        digest: &ActionDigest,
    ) -> Result<ActionResult, AcError>;

    async fn update_action_result(
        &self,
        ctx: &TenantCtx,
        digest: &ActionDigest,
        result: ActionResult,
    ) -> Result<ActionResult, AcError>;  // echo for idempotency
}

#[derive(thiserror::Error, Debug)]
pub enum AcError {
    #[error("action not found")]
    NotFound,                                         // → 404 / NotFound gRPC + COR_AC_ACTION_NOT_FOUND
    #[error("ttl expired")]
    Expired,                                          // → 410 / FailedPrecondition + COR_AC_TTL_EXPIRED
    #[error("merkle verification failed: {0}")]
    MerkleInvalid(String),                            // → 422 + COR_AC_MERKLE_INVALID
    #[error("output blob {0} missing or tombstoned")]
    OutputBlobMissing(String),                        // → 422 + COR_AC_OUTPUTS_MISSING
    #[error("digest signature invalid")]
    SigInvalid,                                       // → 422 + COR_AC_SIG_INVALID
    #[error("scope insufficient: required {required:?}")]
    ScopeInsufficient { required: PatScope },         // → 403 + COR_AUTH_SCOPE_INSUFFICIENT
    #[error("storage backend unavailable: {0}")]
    BackendUnavailable(String),                       // → 503 + COR_AC_BACKEND_UNAVAILABLE
    #[error("internal: {0}")]
    Internal(String),                                 // → 500 + COR_AC_INTERNAL
}
```

**Cripto-driven invariants enforced em cada handler call**:

1. **Tenant-scoped lookup**: `ac_meta::lookup` SQL é `WHERE tenant_id = $1 AND action_digest = $2` — `tenant_id` vem **EXCLUSIVAMENTE** de `TenantCtx.tenant_id` (não query param, não header, não body). **Lote 10.4bis P0 fix**: D1 enforcement is sqlx prepared statement compile-time `tenant_id` parameter binding + handler-level mandatory `WHERE tenant_id = ctx.tenant_id` clause + clippy custom lint forbidding `&str` SQL literals + integration test asserts compile failure for queries lacking the binding. **D1/SQLite has no RLS, no GUCs, no `SET LOCAL` semantic** (those are Postgres primitives consumed em S-03 WI-S03-005 RLS for `account`/`webauthn_credentials` Neon tables; `with_tenant_ctx!` macro is Postgres-only). Layer 2 enforcement em D1 é sqlx + handler discipline; **Layer 4 (HMAC path prefix) carries proportionally more weight in 5-Layer Defense for AC than for auth tables** (which have Postgres RLS as Layer 2). See `auth_model.md §8.1` for Postgres-vs-D1 asymmetry.
2. **Tenant-prefix derivation**: `tenant_prefix = HMAC(TDK_v<path_key_id>, tenant_id)[:16]` reused via `corelink-tenant-path` (S-01 WI-S01-001); R2 path é `ac-<region>/<tenant_prefix_hex>/<action_digest>.json`. **Lote 10.4bis P0 fix**: `tenant_prefix` materialized em `ac_meta` column (BLOB(16)) per WI-S04-002 schema fix; pre-computed em handler INSERT step [7]; cron worker (WI-S04-005) reads column directly **without TDK access** (avoids cron trust boundary expansion); `path_key_id` column tracks TDK rotation version (forward-compat S-14 cross-region rotation). Layer 4 da 5-Layer Defense.
3. **Idempotência**: `UpdateActionResult` em mesmo `(tenant_id, action_digest)` é no-op pós-verify (returns echo); ON CONFLICT (tenant_id, action_digest) DO UPDATE last_hit_at = excluded.last_hit_at apenas.

**Surface dual gRPC + REST**:

- **gRPC** (primary, REAPI v2 spec): `tonic::transport::Server` + `tonic-build` codegen do proto `build/bazel/remote/execution/v2/remote_execution.proto`.
- **REST** (companion, dashboard CLI): `axum::Router` exposes `GET /v2/{instance}/actionResults/{hash}/{size_bytes}` + `PUT` mirror.
- **Single source of truth**: handler trait impl shared; gRPC + REST routes both call mesmo `ActionCacheHandlerImpl`.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

`GetActionResult` + `UpdateActionResult` são o **valor inteiro do remote cache Bazel/Buck2**: dado action_digest determinístico (hash SHA-256 de Action proto = command + inputs + env), retorna ActionResult previamente computado (output_files, exit_code, stderr, stdout). Hit ratio > 70% reduz build time em > 50%; sem AC, CoreLink é só blob store inferior a S3 + bazel-remote (which é grátis).

Bug em handler é **catastrófico em N dimensões**:

1. **Cross-tenant leak via tenant_id source confusion**: handler lê `tenant_id` do body request param (e.g., `ActionResult.metadata.tenant_id` ou query string `?tenant=abc`); atacante envia request authenticated como Tenant A com `?tenant=B`; handler retorna AC entry de B. Mitigação **única-via-construção**: `tenant_id` SOMENTE de `req.extensions::<TenantCtx>().tenant_id()`; nunca body, query, ou header. Compile-time: handler signature recebe `&TenantCtx`, não `tenant_id: TenantId` solto. Property test 100k garante.

2. **Merkle verify ausente em UpdateActionResult**: cliente malicioso UPLOAD ActionResult com output_file digest forjado (apontando para blob de outro tenant). Sem verify pre-persist, AC entry é gravada; Get subsequente entrega bytes de blob alheio. Mitigação: `corelink-ac::verify()` chamado em step [3] de Update; **fail-closed** (default reject); tested via property test (1000 random Merkle trees, 100% caught).

3. **Race em UpdateActionResult concorrente** (mesma `(tenant_id, action_digest)`): dois cliente upload simultaneamente; primeiro escreve R2; segundo escreve R2 também (overwrite); D1 INSERT segundo falha com UNIQUE violation; segundo retorna 200 mas R2 está com bytes do segundo (stale relative ao D1 entry). Mitigação: **R2 first then D1 INSERT-OR-IDEMPOTENT** com retry semantics; `ON CONFLICT (tenant_id, action_digest) DO UPDATE last_hit_at` permite no-op safe; idempotency invariant tested.

4. **Negative cache poisoning** (KV `ac_neg:*`): atacante força miss em N action_digests; KV populates `ac_neg:<tenant_prefix>:<digest>` TTL 60s; legítimo upload subsequente é deflected por 60s. Mitigação: **negative cache invalidate em UpdateActionResult** (step [9]); KV.delete chamado atomicamente após D1 INSERT success; prop test verifica invariante.

5. **REAPI v2 spec compliance drift**: BuildBuddy/NativeLink interpretam ActionResult.execution_metadata fields differently; CoreLink diverge → Bazel client crashes ou builds não-reproducible. Mitigação: bazelbuild/remote-apis conformance suite green em CI; quarterly re-run; reference impl é proto schema fonte-de-verdade.

6. **gRPC vs REST surface drift**: dois endpoints expoem mesma operação; bug-fix em um esquece outro. Mitigação: **single handler impl trait**; REST simplesmente envelopa gRPC method; integration test cobre both surfaces com same fixtures.

7. **TTL expiry handling**: ActionResult expirado (last_hit_at + tier_ttl < now) deve retornar 410 `COR_AC_TTL_EXPIRED` (não 404), pra Bazel re-execute action, não confuse com cache miss. Mitigação: D1 lookup INCLUI `expires_at` check; explicit branch in handler; chaos test em S-07 boundary.

8. **Audit emission asymmetry**: `ac.get.miss` emitted pre-handler; `ac.get.ok` post-handler — mas race em pre→post se handler crashes mid-flight. Mitigação: outbox pattern (reuse WI-S01-004 audit_outbox); pre-emit + post-emit em mesma D1 batch transaction (atomic OR rollback).

**Atacante adversarial scenarios**:

- **Action digest collision attempt**: atacante computes Action proto que collides com legítima Action de outro tenant; uploads ActionResult; expects to override. Mitigação: BLAKE3 256-bit (collision-resistant 2^128); **TENANT-SCOPED** key `(tenant_id, action_digest)` — collision intra-tenant é cripto-impossivel; cross-tenant é blocked by Layer 4 path scoping.
- **Merkle tree malformation** (depth > 100, fanout > 10000): worker memory exhaustion. Mitigação: `corelink-ac` parser tem bounds (max depth 32, max output_files 4096) — exceed → 422 reject; chaos test load-tests max bounds.
- **Parallel UpdateActionResult abuse** (Lote 10.4bis P0 fix: REAPI v2 has NO batch RPC on ActionCache; só CAS has `BatchUpdateBlobs`/`BatchReadBlobs`. Bazel/Buck2 issue parallel singular `UpdateActionResult` RPCs): atacante envia 10000 paralelas; D1 connection pool overflow OR timeout. Mitigação: per-PAT rate limit S-08 forward; per-tenant concurrent UPDATE cap (forward S-13 admin plane); CF Workers automatic concurrency throttling.
- **Large ActionResult body** (output_files contém metadata > 1 MB): DoS via storage exhaustion. Mitigação: D1 ac_meta.blob_refs JSON column max 10 KB (CHECK constraint); R2 envelope max 1 MiB — exceed → 413 `COR_AC_PAYLOAD_TOO_LARGE`.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: handler é endpoint principal AC; bug em tenant context = cross-tenant catastrophic FM-303.
- **FF-HR-005**: handler enforces CTRL-AC-001 (Merkle) + CTRL-AC-002 (signing); skip = security control bypass.
- **Reversibility**: cache poisoning é detectable via INV-AC-OUTPUTS-VALID daily reconcile mas **invisible até customer build fails**; P0 incident exposure.
- **Customer impact**: hit ratio < 50% = perceived broken; > 95% = wow customer.

13 sign-offs incl. Architect (REAPI conformance), AppSec (5-layer scoping), Crypto SME (HKDF signing integration WI-S04-004 boundary).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev usando `--remote_cache=corelink://...`**:
- `bazel build //...` → Bazel computes Action protos → Bazel client RPC `GetActionResult(digest)` para cada action.
- Handler: TenantCtx → ac_meta lookup → R2 GET envelope → sig verify → outputs_check → response ActionResult proto OR NotFound.
- Cache hit (warm): p99 ≤ 30ms; build time savings 50-90% on iterative builds.
- Cache miss → Bazel executes locally → Bazel `UpdateActionResult(digest, result)` populates cache.
- Customer-visible: tempo de build dramatically reduced em CI; métrica `corelink.cache.hit_ratio{type=ac}` em customer dashboard S-16.

**Persona 2 — Buck2 CI dev (similar mas via Buck2 RemoteExecutionClient)**:
- Buck2 implements REAPI v2 client; same gRPC API; transparente swap S3→CoreLink.
- Edge case: Buck2 sends `instance_name` field different from Bazel default; handler must accept any instance (S-04 não enforce instance namespace; S-13 admin plane forward).

**Persona 3 — REAPI conformance reviewer**:
- Runs bazelbuild/remote-apis test suite against staging.
- Expects: 100% AC ops pass (GetActionResult + UpdateActionResult); inclui edge cases (NotFound, idempotency re-update, ActionResult com output_files = []).
- CI gate: nightly conformance run; PR red se regression.

**SLA addendum** (consistente com S-03 WI-S03-003 stale window pattern):
- AC GetActionResult p99 ≤ 150ms warm path (R2 hit + sig verify + ac_meta lookup); cold path ≤ 280ms.
- AC UpdateActionResult p99 ≤ 300ms (Merkle verify + sig + R2 write + D1 batch).
- **TTL refresh-on-hit** updates `last_hit_at` synchronously; `expires_at += refresh_window` (config per-tier S-07/ADR-0019).
- **REAPI conformance**: 100% test suite (bazelbuild/remote-apis) green em CI nightly.
- 5xx storage backend (R2 down OR D1 down) → 503 `COR_AC_BACKEND_UNAVAILABLE` com Retry-After: 5s.

## 4. Capability Mapping

- **CAP-AC-001** (REAPI GetActionResult cache hit) — IMPLEMENTA primary.
- **CAP-AC-002** (REAPI UpdateActionResult cache write) — IMPLEMENTA primary.
- **CAP-AC-005** (AC entry invalidation) — IMPLEMENTA partial (DELETE handler scaffolding; admin override S-13 forward).
- **CAP-AC-006** (Cache hit ratio business métrica) — IMPLEMENTA emit-side; dashboard S-16.
- Trace: `remote_cache_product_profile.md §6 (REAPI conformance)` + `cas_profile.md §1.2 (REAPI API)` + `auth_model.md §8.1 (5-layer Layer 3 enforcement)` + `security_model.md CTRL-AC-001 + CTRL-AC-002`.

## 5. Tipo

REAPI handler (gRPC + REST surface); HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-worker/src/reapi/ac.rs`** — handler module:
   - `ActionCacheHandler` trait + `ActionCacheHandlerImpl` struct.
   - gRPC tonic service impl (REAPI v2 `ActionCache` proto).
   - REST axum routes:
     - `GET /v2/{instance}/actionResults/{hash}/{size_bytes}` → GetActionResult.
     - `PUT /v2/{instance}/actionResults/{hash}/{size_bytes}` → UpdateActionResult.
   - Single trait impl shared between gRPC + REST.
   - **TenantCtx propagation**: `req.extensions().get::<TenantCtx>()` mandatory; no fallback path.

2. **REAPI v2 proto codegen**:
   - `build.rs` invoca `tonic_build::compile_protos` em `proto/build/bazel/remote/execution/v2/remote_execution.proto` (vendored from bazelbuild/remote-apis @ commit fixed).
   - Versão pinned para reproducibility; bump via ADR.

3. **GetActionResult flow**:
   - Step [0] **scope_check**: `TenantCtx::scopes` must contain `PatScope::CacheR` (or `CacheRW`); fail → 403 `COR_AUTH_SCOPE_INSUFFICIENT`.
   - Step [1] **negative_cache::lookup**: KV GET `ac_neg:<tenant_prefix>:<action_digest>`; hit → 404 (avoid D1 hit).
   - Step [2] **ac_meta::lookup**: D1 SELECT result_hash, blob_refs, expires_at, created_at, last_hit_at WHERE tenant_id = $1 AND action_digest = $2 AND (expires_at IS NULL OR expires_at > NOW()); MISS → populate negative cache, 404.
   - Step [3] **r2::get**: R2 GET `ac-<region>/<tenant_prefix>/<action_digest>.json`; envelope JSON.
   - Step [4] **sig::verify** (delegate WI-S04-004): HKDF tenant_key info="ac-sig"; mismatch → 422 `COR_AC_SIG_INVALID` + audit emit.
   - Step [5] **outputs_check** (delegate WI-S04-003 partial; **Lote 10.4bis P0 fix: 1% sampled** to avoid 40M D1 SELECT/day cost on 10M-GET workload): warn-log if any output_file digest tombstoned; **NÃO bloqueia GET** (INV-AC-OUTPUTS-VALID é reconcile-eventual; GET fail-soft to avoid customer breakage during race); enforced strictly em UPDATE step. Métrica `corelink.ac.outputs.tombstoned_warning_total{sampled=true}` reflects sampled rate. Reconcile diário (S-06 forward) é authoritative drift detection.
   - Step [6] **ttl::refresh-on-hit**: D1 UPDATE last_hit_at = now(), expires_at = now() + tier_ttl (per S-07 ADR-0019); single batch.
   - Step [7] **audit emit**: `ac.get.ok` event into outbox (WI-S01-004 audit_outbox table reuse).
   - Response: ActionResult proto serialized.

4. **UpdateActionResult flow**:
   - Step [0] **scope_check**: `PatScope::CacheW` mandatory.
   - Step [1] **body decode**: ActionResult proto deserialize; max body 1 MiB enforce.
   - Step [2] **digest verify** (Bazel client convention): re-compute SHA-256 of canonical Action proto = action_digest provided; mismatch → 422 `COR_AC_DIGEST_MISMATCH`. (Bazel sends digest in URL/path; ActionResult body MUST be consistent.)
   - Step [3] **merkle::verify** (delegate WI-S04-003): tree depth ≤ 32, fanout ≤ 4096, all output_file digests are valid digests; reject → 422.
   - Step [4] **outputs::resolve_blobs**: D1 SELECT blob_meta.deleted_at IS NULL FOR EACH digest in output_files + output_directories.
   - Step [5] **outputs::assert_alive**: ALL alive → proceed; ANY tombstoned → 422 `COR_AC_OUTPUTS_MISSING` + audit emit `ac.update.outputs_missing`.
   - Step [6] **sig::sign** (delegate WI-S04-004): HKDF info="ac-sig" sign envelope (action_digest || result_hash || tenant_id).
   - Step [7] **r2::put**: R2 PUT `ac-<region>/<tenant_prefix>/<action_digest>.json` envelope; If-None-Match: * for create OR overwrite-OK for refresh.
   - Step [8] **ac_meta::insert_or_idempotent**: D1 INSERT INTO ac_meta (...) VALUES (...) ON CONFLICT (tenant_id, action_digest) DO UPDATE SET last_hit_at = excluded.last_hit_at, expires_at = excluded.expires_at.
   - Step [9] **negative_cache::invalidate**: KV.delete `ac_neg:<tenant_prefix>:<action_digest>`.
   - Step [10] **audit emit**: `ac.update.ok` (or `ac.update.merkle_invalid` / `ac.update.outputs_missing` em failure paths).
   - Response: ActionResult proto echo (REAPI convention para idempotency).

5. **Negative cache integration**:
   - Key: `ac_neg:<tenant_prefix>:<action_digest>` (tenant-scoped via prefix).
   - Value: `1` (presence-only marker; semântica = "miss recently confirmed").
   - TTL: **60s** (mais curto que CAS 300s; actions stale rápido pós-rebuild).
   - Set em GetActionResult miss; invalidate em UpdateActionResult success.
   - Constant-time key compare via subtle (defense-em-depth — KV does exact match natively).

6. **Idempotency contract** (REAPI v2):
   - UpdateActionResult mesmo digest twice = no-op pós-verify, returns echo.
   - INSERT ON CONFLICT (tenant_id, action_digest) DO UPDATE last_hit_at apenas; NÃO mutate result_hash ou blob_refs (immutability INV-CAS-IMMUTABILITY).
   - Mismatch (mesmo action_digest mas different result_hash) → 409 `COR_AC_RESULT_HASH_MISMATCH` + audit emit (suspicious; possible non-deterministic build).

7. **Métricas** (RED + AC-specific):
   - `corelink.ac.get.requests_total{result}` (counter; result ∈ ok|miss|merkle_invalid|sig_invalid|expired|backend_unavailable).
   - `corelink.ac.update.requests_total{result}`.
   - `corelink.ac.get.duration_ms_bucket{path}` (histogram; path ∈ warm|cold|merkle_verify|sig_verify).
   - `corelink.ac.update.duration_ms_bucket{path}`.
   - `corelink.cache.hit_ratio{type=ac, tenant_tier, region}` (gauge; emitted via S-09 aggregation).
   - `corelink.ac.negative_cache.{hits, populates, invalidates}_total`.
   - `corelink.ac.outputs.tombstoned_warning_total` (counter; INV-AC-OUTPUTS-VALID drift indicator).

8. **Tracing**:
   - Span `ac.get` com attributes: `tenant_id` (UUIDv7), `action_digest` (hash prefix 16), `result` (ok|miss|...), `r2.bytes`, `cache.hit_ratio.delta`.
   - Span `ac.update` similar; child spans para merkle_verify, sig_sign, r2_put, d1_batch.

9. **Property tests** (10k iter PR; 100k nightly):
   - `prop_ac_get_tenant_isolation`: 1000 random (tenant_a, action_digest_a) + 1000 (tenant_b, ...) — GET tenant_b never returns entry of tenant_a (INV-AC-TENANT-SCOPED).
   - `prop_ac_update_idempotent`: 1000 random ActionResult; insert + insert = same state; result_hash unchanged.
   - `prop_ac_negative_cache_invalidate`: insert miss → negative populate → update → negative invalidated → GET returns hit.
   - `prop_ac_ttl_refresh_monotonic`: GET hit increments last_hit_at >= prev; never goes backward.
   - `prop_ac_outputs_missing_blocks_update`: 100 random ActionResults com 1 tombstoned output → 100% rejected.

10. **Mann-Whitney timing test** (extends WI-S03-003 methodology, 3-prong):
    - Goal: cliente cannot distinguish "action_not_found" vs "action_found but wrong tenant" via timing.
    - Both paths return 404; difference é mid-flight (D1 lookup com tenant filter vs tenant filter mismatch).
    - 10k samples per arm; power 1−β ≥ 0.80 com Cohen's d = 0.2.
    - Šidák 3-trial gate (combined α ≈ 0.000125).
    - |Δmedian| ≤ 5ms com bootstrap 95% CI cruzando 0.

11. **Conformance test integration**:
    - `tests/conformance/reapi_v2_ac.rs` → invoke bazelbuild/remote-apis test suite via subprocess.
    - CI gate em nightly; failure blocks merge of subsequent S-04 WIs.

12. **Integration test E2E**:
    - Real Bazel client + real PAT (S-03) + real R2 + real D1 (staging).
    - Flow: `bazel build //example:hello_world --remote_cache=https://corelink-staging.../v2/{instance}` → 1ª run no cache; 2ª run cache hit; 3ª run after touch source → invalidate hit; new entry.

13. **REST surface details**:
    - `GET /v2/{instance}/actionResults/{hash}/{size_bytes}` → 200 ActionResult JSON OR 404.
    - `PUT /v2/{instance}/actionResults/{hash}/{size_bytes}` body = ActionResult JSON (REAPI v2 schema).
    - `Accept: application/json` mandatory; `application/x-protobuf` for binary.
    - REST mainly for dashboard/CLI; gRPC primary for Bazel/Buck2.

### 6.2 Out-of-scope (deferred)

- **Merkle codec impl** (Builder + Verifier internals): WI-S04-003.
- **HKDF signing/verify impl** (corelink-ac::sign): WI-S04-004.
- **TTL worker (cron DO)**: WI-S04-005 (handler does refresh-on-hit synchronously; expiry job is separate worker).
- **D1 ac_meta migration + R2 bucket**: WI-S04-002.
- **Conformance suite full integration** (PRR + REAPI conformance flow): WI-S04-006.
- **Admin invalidation override** (DELETE batch; admin plane): S-13.
- **Multi-region replication**: S-14.
- **Pre-fetch/pre-warm**: pós-GA.

## 7. Anti-Scope

- ❌ `tenant_id` from request body/query/header (must be from TenantCtx).
- ❌ Skip Merkle verify em UpdateActionResult (fail-closed; reject invalid).
- ❌ Allow result_hash overwrite on idempotent re-update (immutability).
- ❌ Sync emit audit em hot path (outbox pattern).
- ❌ Block GET em INV-AC-OUTPUTS-VALID drift (warn-only; reconcile fixes).
- ❌ Reject GET if expires_at NULL (NULL = no expiry; ADR-0019 per-tier override).
- ❌ Variable-time tenant compare (subtle::ConstantTimeEq em paths).
- ❌ Multi-tenant action_digest namespace (UNIQUE is `(tenant_id, action_digest)`; no global).
- ❌ Custom proto schema (use REAPI v2 vendored proto file).
- ❌ Drop ActionResult metadata fields silently (REAPI compliance — preserve all fields).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: REAPI ActionCache handlers

  Background:
    Given Tenant A has active PAT corelink_pat_xyz with scopes cache_rw
    Given Tenant A has TenantCtx propagated through Tower auth_stack
    Given ac_meta D1 schema deployed (WI-S04-002)
    Given R2 bucket ac-sam exists
    Given corelink-ac WI-S04-003 + WI-S04-004 available

  Scenario: GetActionResult hit (warm path)
    Given Tenant A has ActionResult for action_digest D in ac_meta + R2
    When client gRPC GetActionResult(digest=D) authenticated as Tenant A
    Then handler: scope_check → ac_meta lookup → r2 get → sig verify → outputs_check (warn) → ttl refresh-on-hit → audit emit
    And response 200 ActionResult proto matches D
    And ac_meta.last_hit_at updated to now
    And ac_meta.expires_at += refresh_window (per tier per ADR-0019)
    And metric corelink.ac.get.requests_total{result="ok"} incremented
    And metric corelink.cache.hit_ratio{type=ac, tenant_tier=<tier>} updates
    And p99 latency ≤ 150ms warm

  Scenario: GetActionResult miss (cold path; populate negative cache)
    Given Tenant A has no entry for action_digest D
    When client GetActionResult(digest=D)
    Then handler: scope_check → ac_meta MISS → set ac_neg:<tenant_prefix>:D in KV TTL 60s → audit emit ac.get.miss
    And response 404 + error_code COR_AC_ACTION_NOT_FOUND
    And metric corelink.ac.negative_cache.populates_total incremented

  Scenario: GetActionResult hits negative cache (recently missed)
    Given KV ac_neg:<tenant_prefix>:D exists (populated 30s ago)
    When client GetActionResult(digest=D)
    Then handler short-circuits at step [1] negative_cache lookup
    And response 404 + COR_AC_ACTION_NOT_FOUND
    And NO D1 query executed
    And p99 latency ≤ 10ms

  Scenario: GetActionResult tenant isolation (Tenant B cannot see Tenant A entry)
    Given Tenant A has ActionResult for action_digest D
    Given Tenant B authenticated; same action_digest D requested
    When Tenant B calls GetActionResult(digest=D)
    Then handler ac_meta lookup is WHERE tenant_id = TenantCtx_B AND action_digest = D
    And lookup returns NULL (Tenant A entry invisible)
    And response 404 (NOT 403, to avoid existence leak)
    And audit emit: ac.get.miss with tenant_id = B; never references A
    And property test prop_ac_get_tenant_isolation green

  Scenario: GetActionResult signature invalid (envelope tampering detected)
    Given Tenant A has ActionResult for D in R2 with valid HKDF sig
    When R2 envelope is tampered (byte flip in result_hash field)
    And client GetActionResult(digest=D)
    Then handler: ac_meta lookup ok → r2 get tampered → sig verify FAILS
    Then response 422 + error_code COR_AC_SIG_INVALID
    And audit emit ac.get.sig_invalid (CRITICAL severity; security incident)
    And metric corelink.ac.get.requests_total{result="sig_invalid"} incremented
    And metric corelink.security.tampering_detected_total{type=ac_envelope} incremented (alert)

  Scenario: GetActionResult expired (TTL exceeded; tier-specific)
    Given Tenant A (tier=free) has ActionResult for D with expires_at = now - 1h (free TTL 7d expired)
    When client GetActionResult(digest=D)
    Then handler ac_meta lookup includes expires_at filter; filter excludes
    And response 410 + error_code COR_AC_TTL_EXPIRED + Retry-After: 0
    And audit emit ac.get.expired

  Scenario: UpdateActionResult happy path
    Given Tenant A authenticated; scope cache_w
    Given ActionResult R for action_digest D; output_files all alive in blob_meta
    When client gRPC UpdateActionResult(digest=D, result=R)
    Then handler: scope_check → body decode → digest verify → merkle verify (WI-003) → outputs_alive check → sig sign (WI-004) → r2 put → ac_meta INSERT → negative_cache invalidate → audit emit
    And response 200 ActionResult echo
    And R2 contains envelope at ac-<region>/<tenant_prefix>/D.json
    And ac_meta has row (tenant_id=A, action_digest=D, ...)
    And KV ac_neg:<tenant_prefix>:D NOT present
    And metric corelink.ac.update.requests_total{result="ok"} incremented
    And p99 latency ≤ 300ms

  Scenario: UpdateActionResult Merkle invalid (rejected pre-persist)
    Given ActionResult R contains invalid Merkle tree (depth 50, exceeds bound)
    When client UpdateActionResult(digest=D, result=R)
    Then handler merkle::verify rejects (depth > 32)
    Then response 422 + error_code COR_AC_MERKLE_INVALID + body { reason: "depth exceeds 32" }
    And NO R2 write occurred
    And NO D1 INSERT occurred
    And audit emit ac.update.merkle_invalid

  Scenario: UpdateActionResult outputs missing (tombstoned blob)
    Given ActionResult R references output_file digest D_output
    Given blob_meta has D_output with deleted_at = now - 1h (tombstoned by S-06 GC)
    When client UpdateActionResult(digest=D, result=R)
    Then handler outputs_check finds D_output tombstoned
    Then response 422 + error_code COR_AC_OUTPUTS_MISSING + body { missing: [D_output] }
    And audit emit ac.update.outputs_missing
    And metric corelink.ac.outputs.tombstoned_warning_total incremented

  Scenario: UpdateActionResult idempotent (re-upload same digest, same result)
    Given Tenant A has ActionResult for D in ac_meta
    When client UpdateActionResult(digest=D, result=same R)
    Then handler INSERT ON CONFLICT DO UPDATE last_hit_at + expires_at only
    And R2 envelope NOT re-written (If-None-Match: * fails; or skip via short-circuit)
    And response 200 ActionResult echo (REAPI idempotency)
    And result_hash unchanged

  Scenario: UpdateActionResult result_hash mismatch (suspicious — possible non-deterministic build)
    Given Tenant A has ActionResult for D with result_hash H1
    When client UpdateActionResult(digest=D, result=R2 with result_hash H2 ≠ H1)
    Then handler detects mismatch
    Then response 409 + error_code COR_AC_RESULT_HASH_MISMATCH + body { existing: H1, attempted: H2 }
    And audit emit ac.update.result_mismatch (WARN severity; possible non-determinism)

  Scenario: Mann-Whitney timing — action_not_found vs action_found_but_wrong_tenant indistinguishable
    Given 10k requests com action_digest absent from any tenant
    Given 10k requests com action_digest existing for Tenant B; authenticated as Tenant A
    When latencies collected per arm
    And Mann-Whitney U applied with power analysis
    Then p > 0.05 com Šidák 3-trial
    And |Δmedian| ≤ 5ms com 95% CI cruzando 0
    (cliente cannot distinguish "doesn't exist" vs "exists for someone else")

  Scenario: Backend unavailable (R2 down)
    Given R2 GET returns 5xx
    When client GetActionResult(digest=D)
    Then handler: ac_meta lookup ok → r2 get fails 503
    Then response 503 + error_code COR_AC_BACKEND_UNAVAILABLE + Retry-After: 5s
    And metric corelink.ac.get.requests_total{result="backend_unavailable"} incremented

  Scenario: gRPC + REST surface parity
    Given same TenantCtx + same action_digest
    When request via gRPC GetActionResult vs REST GET /v2/.../actionResults/...
    Then both surfaces return identical ActionResult proto/JSON (semantically equivalent)
    And both surfaces hit same handler trait impl (single source of truth)

  Scenario: REAPI conformance suite passes
    Given staging environment with WI-S04-001..006 deployed
    When bazelbuild/remote-apis test suite runs against staging
    Then 100% AC ops pass (GetActionResult + UpdateActionResult + edge cases)
    And conformance report in CI artifact
```

## 9. Design Decisions

### 9.1 Why single trait impl shared between gRPC + REST (não duplicar)

- gRPC + REST surface mesma operação; duplicate impl = bug surface (fix em um esquece outro); double test maintenance.
- Trait `ActionCacheHandler` impl uma vez; gRPC tonic service envelopa; REST axum route envelopa.
- Single source of truth; DRY enforced via type system.

### 9.2 Why TenantCtx-only tenant_id source (não body/query/header)

- Body/query/header é attacker-controlled (request crafted by client); using as auth fonte = TOCTOU (auth checked Layer 1 against header X; storage call uses query Y).
- TenantCtx é built post-auth-verify; immutable; carried via request.extensions; única source authentified.
- Compile-time enforcement: handler signature `&TenantCtx` parameter; nem possibility de typo `request.tenant_id` em SQL.

### 9.3 Why negative cache TTL 60s (vs 300s CAS)

- AC entries são per-build artifact; shorter half-life que CAS blobs (which are content-addressable forever).
- 60s balanca: avoid storm hits on missed digest (build retry 5s burst); refresh fast post-Update.
- TTL configurable via env `CORELINK_AC_NEG_CACHE_TTL_S`; per-tenant override forward S-13.

### 9.4 Why warn-only INV-AC-OUTPUTS-VALID em GET (não fail-closed)

Trade-off: fail-closed em GET would block customer build if S-06 GC tombstoned a blob during reconcile race. Warn-only + reconcile-eventual:
- GET returns ActionResult com tombstoned output reference → Bazel client fetches output from CAS → CAS returns 404 → Bazel re-executes action.
- Customer saw "stale ActionResult" mas build succeeds (just slower).
- INV-AC-OUTPUTS-VALID daily reconcile fixes drift; metric `corelink.ac.outputs.tombstoned_warning_total` alerts if sustained.
- Strict fail-closed em UPDATE (S-04): prevents new bad entries; reconcile cleans existing.

### 9.5 Why R2-first then D1 INSERT (não D1-first)

R2 is content-addressable + idempotent (PUT same path = overwrite OK; consistent eventual). D1 is the indexed source-of-truth; if D1 INSERT fails, R2 has orphan envelope (cleaned by S-06 GC reconcile).
- Reverse (D1-first then R2) means crash before R2 PUT → D1 has row referencing missing R2 object → GET fails 404 forever (ghost row).
- R2-first: crash before D1 INSERT → orphan R2 (recoverable; GC reconcile cleans); D1 absence = retry-safe Update.

### 9.6 (REMOVED Lote 10.4bis P0 fix) — REAPI v2 has no batch RPC on ActionCache

**Lote 10.4bis P0 fix**: Prior text claimed "BatchUpdateActionResult" — REAPI v2 does **NOT** define a batch RPC on the `ActionCache` service. The batch RPCs in REAPI v2 are on **`ContentAddressableStorage`** only (`BatchUpdateBlobs`, `BatchReadBlobs`). For AC, Bazel/Buck2 issue **parallel singular `UpdateActionResult` RPCs**.

Consequence: server-side "batch cap" doesn't apply (there is no server-side batch). Concurrency control is via:
- **Per-PAT rate limit** (S-08 forward; this WI marks rate_limit hook stub).
- **Per-tenant concurrent UPDATE cap** (forward S-13 admin plane).
- **CF Workers automatic concurrency throttling** (per-isolate request queueing).

`COR_AC_BATCH_TOO_LARGE` error code remains in taxonomy for future BatchUpdateBlobs (CAS WI-S01-005) cross-reference but is NOT emitted from ActionCache handlers in S-04.

### 9.7 Why reject `result_hash` mismatch as 409 (não 200 with overwrite)

Mismatch = same action_digest produced different result_hash = either:
1. Non-deterministic build (compiler bug, hidden time/random dep).
2. Cache poisoning attempt (attacker sends crafted result for known digest).

Both WARRANT customer attention. 409 + audit log → customer investigates. Silent overwrite would mask incidents + corrupt cache.

### 9.8 Why outbox pattern em audit emit (não synchronous)

Synchronous emit to S-09 chain = +20ms per request; AC hot path budget é 150ms p99. Reuse audit_outbox table pattern (WI-S01-004 schema); single D1 batch with handler ops; drain worker emits async.

### 9.9 Why Mann-Whitney "not_found" vs "wrong_tenant_404"

Both endpoints return 404; if attacker can distinguish via timing, learns "action_digest D exists somewhere in CoreLink" — informational leak. Mann-Whitney 3-prong validates indistinguishability < 5ms |Δmedian|.

### 9.10 ADR potencial?

Sim — **ADR-0035**: "AC handler invariants: tenant_id from TenantCtx only; warn-only outputs check em GET; R2-first then D1 INSERT-or-idempotent; 100 batch cap." Whitelist em validate_references.py. Ratificada em final WI-S04-001 review.

## 10. Completeness Criteria SOTA

- [ ] **10.s04.001.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_ac_get_tenant_isolation, prop_ac_update_idempotent, prop_ac_negative_cache_invalidate, prop_ac_ttl_refresh_monotonic, prop_ac_outputs_missing_blocks_update.
- [ ] **10.s04.001.2** Mann-Whitney U + power analysis 3-prong em not_found vs wrong_tenant_404 timing (EVT-002):
  - N ≥ 10000 samples per arm.
  - Power 1−β ≥ 0.80 com effect d = 0.2 via statrs.
  - Šidák 3-trial gate (combined α ≈ 0.000125).
  - |Δmedian| ≤ 5ms com bootstrap 95% CI cruzando 0.
- [ ] **10.s04.001.3** REAPI v2 conformance suite (bazelbuild/remote-apis) — AC ops 100% pass nightly CI (EVT-002 + EVT-018).
- [ ] **10.s04.001.4** Latency p99 GET warm ≤ 150ms; UPDATE p99 ≤ 300ms; sustained 72h staging (EVT-021).
- [ ] **10.s04.001.5** TenantCtx immutability propagation: handler signature `&TenantCtx`; static-checked via clippy custom lint that warns on `tenant_id: TenantId` parameter in handler module (EVT-002).
- [ ] **10.s04.001.6** Cache hit ratio > 70% em workload sintético Bazel (10 actions, 5 rebuilds with ≤ 30% changes) (EVT-021).
- [ ] **10.s04.001.7** gRPC + REST surface parity test: 100 random (action_digest, ActionResult) ops via both surfaces; assertion identical responses.
- [ ] **10.s04.001.8** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s04.001.9** Cost regression gate: per-op cost ≤ $0.000010 (warm GET) + ≤ $0.000020 (UPDATE) (Lote 9.4 §14.10).
- [ ] **10.s04.001.10** Integration E2E test: Bazel client + real PAT + staging green em CI nightly.
- [ ] **10.s04.001.11** OWASP API Security Top 10 checklist pass (BOLA + Mass Assignment + Improper Inventory).
- [ ] **10.s04.001.12** Audit emission outbox atomicity test (D1 batch rollback on R2 fail).

## 11. DoD

- [ ] `crates/corelink-worker/src/reapi/ac.rs` compila + integration tests green.
- [ ] gRPC tonic service + REST axum routes both functional.
- [ ] Single `ActionCacheHandlerImpl` trait shared.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k iter green em CI; 100k nightly green.
- [ ] Mann-Whitney 3-prong test green (not_found vs wrong_tenant_404).
- [ ] REAPI conformance suite 100% AC ops green em CI nightly.
- [ ] gRPC + REST parity test green.
- [ ] Métricas (7 listadas §6.1.7) emitted; dashboard partial DASH-AC (full em WI-S04-006).
- [ ] Trace span `ac.get` + `ac.update` em OTel pipeline.
- [ ] rustdoc 100% public API + 4 examples (basic GET, basic UPDATE, batch UPDATE, idempotency demo).
- [ ] ADR-0035 published.
- [ ] Architect + AppSec + Security Lead reviews.
- [ ] PRR Architect mini sign-off (ship gate full em WI-S04-006).
- [ ] Cost regression gate green.
- [ ] Negative cache invariant test (populate→invalidate→hit) green.

## 12. Invariants Validated

- **INV-AC-TENANT-SCOPED** (CRITICAL, registry §3.3): GET/UPDATE always WHERE tenant_id = TenantCtx.tenant_id; property test 100k.
- **INV-AC-OUTPUTS-VALID** (HIGH, registry §3.3): UPDATE rejects if any output digest tombstoned; GET warns-only (reconcile-eventual).
- **INV-AC-IDEMPOTENT** (HIGH, NEW — promovida em registry §3.15 Lote 10.4bis): mesma `(tenant_id, action_digest)` em UPDATE = no-op pós-verify; result_hash immutable.
- **INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE** (HIGH, NEW): KV ac_neg invalidated atomically em UPDATE success (post D1 INSERT, pre response).
- **INV-AC-RESULT-HASH-IMMUTABLE** (HIGH, NEW): mismatch on re-update → 409 + audit; never silent overwrite.
- **INV-AUTH-TENANTCTX-IMMUTABLE** (CRITICAL, registry §3.14): TenantCtx propagation; reuse S-03 invariant.

TLA+ alignment: `tenant_isolation.tla` AC variant — handler ops respect tenant scoping; INV-AC-TENANT-SCOPED deriva de INV-TENANT-ISOLATION.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| AC handler module | `crates/corelink-worker/src/reapi/ac.rs` | Rust |
| Handler trait + Impl | `crates/corelink-worker/src/reapi/ac/handler.rs` | Rust |
| gRPC tonic service wrapper | `crates/corelink-worker/src/reapi/ac/grpc.rs` | Rust |
| REST axum routes wrapper | `crates/corelink-worker/src/reapi/ac/rest.rs` | Rust |
| Negative cache module | `crates/corelink-worker/src/reapi/ac/neg_cache.rs` | Rust |
| Property tests | `crates/corelink-worker/tests/prop_ac_handlers.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-worker/tests/timing_ac_404.rs` | Rust |
| E2E integration | `tests/e2e_bazel_remote_cache.rs` | Rust |
| Conformance harness | `tests/conformance/reapi_v2_ac.rs` | Rust |
| REAPI proto vendored | `proto/build/bazel/remote/execution/v2/remote_execution.proto` | Proto |
| Build script | `crates/corelink-worker/build.rs` | Rust |
| ADR-0035 | `specs/02_governance/decisions/ADR-0035-ac-handler-invariants.md` | Markdown |
| Examples | `crates/corelink-worker/examples/ac/` (4 examples) | Rust |
| OWASP API Top 10 checklist | `specs/_audits/2026-XX-XX-owasp-api-top10-ac.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s04.001.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s04.001.2** rustdoc 100% public API + 4 examples + threat model README section.
- **14.s04.001.3** Test coverage ≥ 95% (security boundary).
- **14.s04.001.4** Latência: p99 GET warm ≤ 150ms; cold ≤ 280ms; UPDATE ≤ 300ms; trace span overhead ≤ 5%.
- **14.s04.001.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz target em ActionResult deserializer 1h.
- **14.s04.001.6** Métricas RED + cache hit ratio + per-method timing histograms + tampering detected counter.
- **14.s04.001.7** Runbooks: RB-FM-303 (AC cross-tenant) + RB-FM-AC-CACHE-MISS-STORM forward.
- **14.s04.001.8** REAPI v2 spec compliance: vendored proto pinned commit; conformance suite green.
- **14.s04.001.9** Memory bounded: per-request ≤ 64 KiB stack (TenantCtx + ActionResult + transient).
- **14.s04.001.10** Cost regression gate: per-op cost GET ≤ $0.000010; UPDATE ≤ $0.000020.

## 15. Chaos Experiments

1. **R2 outage 1h**: simulate R2 GET 5xx; verify GET returns 503 backend_unavailable + Retry-After. Hypothesis: graceful degradation; no false 404. Procedure: chaos toggle in staging; validate metrics + Bazel client retry behavior.

2. **D1 partial outage** (read-only): D1 reads slow (p99 5s); writes blocked. Hypothesis: GET continues with timeout 30s graceful 503; UPDATE fails fast 503. Validates per-op timeout discipline.

3. **Tampering attempt**: red team flip byte in R2 envelope; GET handler detects via sig::verify; emits ac.get.sig_invalid + alert. Hypothesis: 100% detection within 10ms post-verify.

4. **Cross-tenant attempt via crafted body**: red team sends UPDATE with `result.metadata.tenant_id = "B"` while authenticated as A; handler ignores body field; D1 INSERT uses TenantCtx.tenant_id (A). Hypothesis: 100% rejected via TenantCtx-only enforcement; chaos test asserts D1 row contains tenant_id=A.

5. **Negative cache poisoning**: red team forces 1000 distinct miss action_digests; KV populated heavily; verify legitimate UPDATE still works (invalidate succeeds). Hypothesis: invalidate is O(1) per digest; KV write rate ≤ 100/s sustainable.

6. **Idempotent re-update storm**: 1000 req/s same `(tenant, digest)` UPDATE; verify D1 ON CONFLICT semantics + R2 PUT skipping; verify no UNIQUE violation logs. Hypothesis: idempotency holds; metric `corelink.ac.update.requests_total{result="ok"}` increments correctly.

7. **REAPI conformance regression detector**: chaos PR introduces subtle drift (e.g., ActionResult.execution_metadata.queue_id missing); verify CI nightly conformance suite catches. Hypothesis: 100% detection.

8. **Output blob tombstoned mid-flight**: race between S-06 GC tombstoning blob D and UPDATE referencing D; verify INV-AC-OUTPUTS-VALID is enforced (UPDATE rejected 422); reconcile catches drift if race won by GC. Hypothesis: 100% rejection in UPDATE; warn-only in GET.

9. **Audit outbox D1 batch atomicity**: simulate D1 batch INSERT (ac_meta + audit_outbox) partial fail (audit table missing column drift); verify R2 PUT rolled back via 2-phase compensating action; no orphan envelope. Hypothesis: chaos detects via reconcile job.

10. **Bazel client version mismatch** (Bazel 6 vs Bazel 7 ActionResult schema delta): chaos test rotates Bazel client versions; verify backward + forward compat per REAPI v2 spec. Hypothesis: 100% pass current+next major; document in ADR-0035 if break.

11. **Cold-start under burst**: Worker isolate cold-start during 1k req/s; verify p99 ≤ 500ms graceful (vs 150ms warm). Cold-start metric emitted; alert if sustained > 5min.

## 16. PRR

PRR HIGH_RISK 13 sign-offs gated em WI-S04-006 ship gate. Este WI mini-PRR Architect + AppSec + Security Lead.

- [ ] All Gherkin scenarios green em integration.
- [ ] Property + Mann-Whitney + chaos green.
- [ ] REAPI conformance 100% green nightly.
- [ ] E2E Bazel client staging green.
- [ ] Cost regression gate green.
- [ ] Métricas + partial dashboard.
- [ ] ADR-0035 published.
- [ ] OWASP API Top 10 100%.
- [ ] gRPC + REST parity validated.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | REAPI v2 proto vendoring + tonic-build setup | 2h |
| ST-002 | ActionCacheHandler trait + ActionCacheHandlerImpl skeleton | 3h |
| ST-003 | gRPC tonic service wrapper | 2h |
| ST-004 | REST axum routes wrapper (GET + PUT) | 2h |
| ST-005 | GetActionResult flow steps [0]..[7] impl | 4h |
| ST-006 | UpdateActionResult flow steps [0]..[10] impl | 5h |
| ST-007 | Negative cache module (KV operations + tenant prefix scoping) | 2h |
| ST-008 | Idempotency contract (ON CONFLICT + result_hash mismatch 409) | 2h |
| ST-009 | Métricas emit (7 metrics) + trace spans | 2.5h |
| ST-010 | Property tests 10k iter (5 properties) | 4h |
| ST-011 | Mann-Whitney 3-prong test impl (not_found vs wrong_tenant_404) | 3h |
| ST-012 | gRPC + REST parity test | 2h |
| ST-013 | Conformance harness (bazelbuild/remote-apis subprocess) | 4h |
| ST-014 | Chaos suite (5-layer + tampering + outage scenarios) | 4h |
| ST-015 | E2E integration test (real Bazel + PAT + staging) | 4h |
| ST-016 | rustdoc + 4 examples + threat model README | 3h |
| ST-017 | ADR-0035 redação | 2h |
| ST-018 | Architect + AppSec review feedback iteration | 3h |
| ST-019 | OWASP API Top 10 self-checklist + audit | 2h |
| ST-020 | Cost regression bench setup | 2h |

**Total Optimistic**: ~57h. **PERT** (O=50h, M=58h, P=88h): **~62h**.

## 18. Dependencies

### Hard blockers

- WI-S03-003 (Tower auth_stack + TenantCtx propagation) SEALED.
- WI-S03-002 (corelink-pat scopes cache_r/cache_w) SEALED.
- WI-S04-002 (D1 ac_meta + R2 ac bucket) SEALED.
- WI-S04-003 (corelink-ac Merkle codec + verify) SEALED.
- WI-S04-004 (HKDF signing) SEALED.
- WI-S01-001 (corelink-tenant-path) SEALED (tenant_prefix derivation).
- WI-S01-004 (audit_outbox table — Lote 10.4bis P0 fix; was incorrectly cited as WI-S01-005) SEALED.
- Crypto SME availability (review HKDF integration boundary).

### Soft blockers

- WI-S04-005 (TTL worker) — soft; refresh-on-hit is synchronous in handler; expiry job separate.
- WI-S04-006 (conformance + PRR) — outbound; this WI provides handlers, WI-006 wraps in conformance suite + ship gate.

### Outbound

- WI-S04-006 consumes handler trait for conformance test invocations.
- All future remote execution WIs (Fase 2) extend ActionCache to ExecutionService.
- S-13 admin plane consumes DELETE handler scaffolding.
- S-15 CLI/SDK consume gRPC client; reverse handler.

## 19. Effort PERT

O: 50h, M: 58h, P: 88h → PERT **62h**.

## 20. Time-boxing

**72h hard limit**. Se exceder → escalation: split WI em "core handler" + "conformance integration" sub-WIs.

## 21. Observability

7 métricas listadas §6.1.7. Trace spans:
- `ac.get` com attributes: `tenant_id` (UUIDv7), `action_digest_prefix` (16 hex), `result` (ok|miss|...), `r2.bytes`, `cache.hit_ratio.delta`, `path` (warm|cold), `negative_cache.outcome`.
- `ac.update` com attributes: `tenant_id`, `action_digest_prefix`, `result_hash_prefix`, `outputs.count`, `merkle.depth`, `merkle.fanout`, `sig.verify_ms`, `r2.put_ms`, `d1.batch_ms`.

Logs structured JSON; INFO em ok; WARN em outputs_missing/result_mismatch; ERROR em sig_invalid/backend_unavailable.

Dashboard widget DASH-AC (partial; full em WI-S04-006):
- Request rate per method (GET/UPDATE) per result.
- Cache hit ratio per tenant_tier per region (business métrica).
- p99 latency cold/warm.
- Negative cache hits/populates/invalidates rate.
- Tampering detected counter (alert if > 0 sustained).
- Outputs tombstoned warning rate (INV-AC-OUTPUTS-VALID drift).

## 22. Cost Analysis

**Per-request breakdown** (warm path; GET cache hit):
- Worker invocation: $0.50/M.
- KV read (negative cache lookup): $0.50/M.
- D1 SELECT (ac_meta lookup): $1/M.
- R2 GET (envelope ~5KB): $0.36/M class A ops + $0.0144/GB egress.
- Per-request total GET warm: ~$0.000005 + $0.0001 R2 egress (5KB) = **$0.000105**.

Wait — R2 egress not customer-paid (CF zero egress). Adjust:
- R2 GET op: $0.36/M class A.
- R2 egress: $0/GB (CF Workers).
- Per-request total GET warm: ~$0.000005.

**Per-request breakdown** (UPDATE path):
- Worker + Merkle verify (~5ms CPU) + sig sign (~1ms): negligible additional.
- D1 SELECT (outputs check, N=4 typical): ~$0.000004.
- R2 PUT (envelope write): $4.5/M class A.
- D1 INSERT batch (ac_meta + audit_outbox): ~$1.50/M.
- KV DELETE (negative cache invalidate): $5/M.
- Per-request total UPDATE: ~$0.000015.

**TCO 12m projection** (10M GET req/dia, 1M UPDATE req/dia, ratio 10:1 typical Bazel):
- 10M GET/dia × $0.000005 = $50/dia.
- 1M UPDATE/dia × $0.000015 = $15/dia.
- Total: ~$65/dia × 365 = **~$23.7k/yr**.
- Storage R2 (envelopes ~5KB × 10M unique = 50 GB): ~$0.75/mo × 12 = **$9/yr**.
- Storage D1 (ac_meta rows ~200 bytes × 10M = 2 GB): negligible (D1 free tier 5GB).
- Total handler infra: **~$23.7k/yr** em 10M GET/dia workload.

**Cost regression gate** (§14.10): per-op GET ≤ $0.000010; UPDATE ≤ $0.000020; CI bench fails se exceder.

**Comparison vs alternatives**:
- BuildBuddy Cloud: $99-299/seat/mo × 50 seats × 12 = **$60-180k/yr**.
- bazel-remote (self-hosted): infra ~$5k/yr + ops $50k/yr = **$55k/yr**.
- CoreLink: **$23.7k/yr** (Cloudflare-native; near-zero ops).

## 23. API Contract

**gRPC** (REAPI v2 vendored proto):
```proto
service ActionCache {
  rpc GetActionResult(GetActionResultRequest) returns (ActionResult);
  rpc UpdateActionResult(UpdateActionResultRequest) returns (ActionResult);
}
```

**REST**:
```
GET  /v2/{instance}/actionResults/{hash}/{size_bytes} → 200 ActionResult (JSON) | 404 | 410 | 422 | 503
PUT  /v2/{instance}/actionResults/{hash}/{size_bytes} → 200 ActionResult (JSON) | 400 | 403 | 409 | 422 | 503
```

HTTP error mapping (downstream-visible):
- `Action not found` → 404 `COR_AC_ACTION_NOT_FOUND` (gRPC NotFound).
- `TTL expired` → 410 `COR_AC_TTL_EXPIRED` (gRPC FailedPrecondition).
- `Merkle invalid` → 422 `COR_AC_MERKLE_INVALID` (gRPC InvalidArgument).
- `Outputs missing` → 422 `COR_AC_OUTPUTS_MISSING` + body `{ missing: [digests] }`.
- `Sig invalid` → 422 `COR_AC_SIG_INVALID` (CRITICAL audit event).
- `Result hash mismatch` → 409 `COR_AC_RESULT_HASH_MISMATCH` + body `{ existing, attempted }`.
- `Digest mismatch` (URL vs body) → 422 `COR_AC_DIGEST_MISMATCH`.
- `Scope insufficient` → 403 `COR_AUTH_SCOPE_INSUFFICIENT` (S-03 reuse).
- (removed Lote 10.4bis: `Batch too large` was BatchUpdateActionResult; REAPI v2 has no AC batch RPC; cross-reference CAS WI-S01-005 BatchUpdateBlobs.)
- `Payload too large` (>1 MiB) → 413 `COR_AC_PAYLOAD_TOO_LARGE`.
- `Backend unavailable` → 503 `COR_AC_BACKEND_UNAVAILABLE` + Retry-After.

## 24. Post-mortem Hooks

- Cross-tenant AC leak detected (FM-303) → CRITICAL post-mortem + Privacy + breach notification consideration.
- Sig invalid sustained > 5/h → CRITICAL (suspected tampering; hot key compromise consideration).
- Merkle invalid escape (caught only at customer build) → CRITICAL post-mortem + `corelink-ac` verifier review.
- INV-AC-OUTPUTS-VALID drift > 5 records → 5-Why + GC interaction review.
- REAPI conformance regression → post-mortem + Bazel community engagement + version revert consideration.
- Cache hit ratio < 50% sustained 7d → post-mortem (DX) + workload analysis.
- Result hash mismatch > 10/day → SEV-2 (non-determinism investigation customer-side).
- Negative cache poisoning storm > 1k/min → SEV-2 + RB-FM-AC-CACHE-MISS-STORM.

## 25. Rollback / Recovery

Hot rollback via Wrangler `wrangler deploy --version-id <prev>`. AC handler rollback:
- New ac_meta entries created during bad version: idempotent UPDATE re-overrides last_hit_at; result_hash invariant preserved (no overwrite).
- Negative cache invalidation queue: KV.delete idempotent; replay safe.
- Audit emission outbox: drain worker continues; pre-rollback emissions delivered.
- RTO: ≤ 10 min (Wrangler deploy).
- RPO: 0 (stateless handler; no data loss; D1 + R2 source-of-truth survive).

Fallback degradation: handler 503 if R2 down; Bazel client falls back to local execution (graceful, expected REAPI behavior).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: TenantCtx-only tenant_id source; PAT/JWT verify Layer 1 (S-03 reuse); request body tenant_id field IGNORED.
- **Tampering**: Merkle dual-side verify (server pre-persist + client post-download CAP-CAS-005 reuse); HKDF sig (CTRL-AC-002) detects envelope tampering; ac_meta result_hash IMMUTABLE on idempotent re-update.
- **Repudiation**: pre/post audit emit (ac.{get,update}.{ok,*_error}); outbox guarantees at-least-once durable; reuse WI-S03-007 chain integrity.
- **Information disclosure**: Mann-Whitney constant-time not_found vs wrong_tenant_404 (404 same response body; timing indistinguishable < 5ms |Δmedian|); ac_meta query NEVER references other tenant_id; negative cache key includes tenant_prefix (no global pollution).
- **DoS**: batch cap 100; payload cap 1 MiB; negative cache absorbs miss storms; rate limit S-08 forward; D1 batch atomic with timeout.
- **Elevation of privilege**: scope check Layer 3 enforced (cache_r vs cache_w); 5-layer defense reused; admin invalidation deferred to S-13 with separate scope.

**LINDDUN delta**:
- **Linkability**: tenant_id UUID v7 pseudonymous em audit; action_digest é content hash (non-PII).
- **Identifiability**: ActionResult.metadata pode conter command_line + env_vars; redact policy via S-09 macro (passwords, secrets); customer responsability primary.
- **Non-repudiation**: append-only audit chain; outbox at-least-once; result_hash immutable (no silent rewrite).
- **Detectability**: timing constant via Mann-Whitney 3-prong; tampering detected via sig::verify + alert; INV-AC-OUTPUTS-VALID daily reconcile + drift counter.
- **Disclosure of information**: 404 for both not_found + wrong_tenant (no existence leak); error body never echoes raw tenant_id of others; sig_invalid does NOT echo expected sig (only "invalid").
- **Unawareness**: REAPI conformance reviewer can verify expected behavior via test suite; SLA addendum documents stale window axes; ADR-0035 documents handler invariants.
- **Non-compliance**: LGPD Art. 38 + GDPR Art. 32 satisfied via Argon2 (auth) + audit chain (forensic) + DSR support S-11 (action_digest semantically pseudonymous; correlate via PAT issuance trail).

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "REAPI v2 ActionCache + 5-Layer Defense + Tenant Context Propagation".
- **Doc** `docs/internal/reapi-ac-handler.md` — sequence diagrams (GET hit, GET miss → negative cache, UPDATE happy, UPDATE merkle invalid, UPDATE outputs missing, idempotent re-update, result_hash mismatch).
- **Doc** `docs/internal/ac-tenant-isolation.md` — TenantCtx-only enforcement pattern; clippy custom lint authoring.
- **ADR-0035** — handler invariants design rationale.
- **Workshop** (2h): com Architect + AppSec + Security Lead + downstream WI authors (WI-S04-002..006).
- **Onboarding test** (5 questions): TenantCtx propagation, R2-first vs D1-first ordering, idempotency contract, negative cache TTL choice, Mann-Whitney rationale.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-tenant AC leak via body tenant_id field | L | L | CRITICAL | L | LOW | TenantCtx-only enforcement; clippy lint; property test 100k; chaos test |
| R-002 | Merkle verify bypass em UPDATE (delegate WI-003 boundary mismatch) | L | M | CRITICAL | L | LOW | Trait contract + integration test; WI-003 fail-closed default; conformance suite |
| R-003 | Result hash silent overwrite breaking immutability INV | L | L | HIGH | L | LOW | 409 mismatch enforcement; ON CONFLICT DO UPDATE last_hit_at only; property test |
| R-004 | Negative cache poisoning DoS | M | M | MEDIUM | M | LOW | TTL 60s short; rate limit S-08; per-tenant prefix scoping |
| R-005 | REAPI v2 spec drift (Bazel 7+ schema delta) | M | M | MEDIUM | M | LOW | Conformance suite nightly CI; vendored proto pinned commit; ADR-0035 documents bump policy |
| R-006 | gRPC + REST surface drift (bug-fix asymmetry) | M | L | LOW | L | LOW | Single trait impl; parity test 100 random ops; CI gate |
| R-007 | Mann-Whitney CI flake (1-em-20 false positive) | M | H | LOW | M | LOW | Šidák 3-trial gate; combined α ≈ 0.000125 |
| R-008 | R2 partial outage cascades para AC GET (single point) | M | H | HIGH | M | LOW | 503 graceful + Retry-After; Bazel client local fallback expected; metric alert |
| R-009 | D1 batch atomicity violation (R2 PUT but D1 INSERT fail) | L | M | HIGH | L | LOW | R2-first then D1-INSERT (orphan recoverable via reconcile); chaos test |
| R-010 | INV-AC-OUTPUTS-VALID race (S-06 GC tombstones blob during UPDATE) | M | M | HIGH | M | LOW | Strict fail-closed em UPDATE; warn-only em GET; reconcile diário; INV registry §3.15 promotion |
| R-011 | TenantCtx-only enforcement bypassed via new handler param refactor | L | L | CRITICAL | L | LOW | Clippy custom lint; CI gate; ADR-0035 documented; design review onboarding |
| R-012 | REAPI conformance suite stale (vendored proto outdated) | M | L | LOW | L | LOW | Quarterly bump cadence; ADR for major version changes; community engagement |
| R-013 | Cost regression > 10% per-op | M | L | MEDIUM | L | LOW | §14.10 cost gate; weekly bench; alert on regression |
| R-014 | Bazel client version mismatch (6 vs 7 ActionResult schema) | M | M | MEDIUM | M | LOW | Conformance suite covers Bazel current+next major; chaos test rotates versions |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Security Lead review handler trait + 5-layer enforcement + TenantCtx-only pattern.
2. **AppSec (D+2)**: AppSec review tampering detection + sig integration boundary + audit emission ordering.
3. **Code (D+5)**: peer review (2 engineers).
4. **Crypto (D+6)**: Crypto SME review HKDF signing integration boundary (delegated WI-S04-004) + Mann-Whitney 3-prong.
5. **Adversarial (pre-merge D+8)**: red team session — body tenant_id confusion attempts, Merkle bypass, negative cache poisoning, idempotency edge cases.
6. **Conformance (D+9)**: Bazel community engagement; REAPI v2 conformance suite green em CI nightly.
7. **PRR (D+10)**: Architect mini sign-off (full ship gate em WI-S04-006).

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; **mandatory** — TenantCtx-only enforcement review_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; PII redaction policy review (ActionResult metadata)_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — REAPI conformance + handler trait composability_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — tampering detection + 5-layer enforcement_ | _pending_ | _pending_ |
| 13 | Crypto SME (advisory) | _mandatory; HKDF integration boundary + Mann-Whitney 3-prong_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S04-001 (Lote 10.4); SOTA pós-Lote 10.3bis (32 seções; 13-row sign-off; 14-row risk; Mann-Whitney 3-prong; cost TCO 12m; 11 chaos experiments; STRIDE+LINDDUN delta full). |

## 32. Anti-patterns evitados

- ❌ tenant_id from request body/query/header (TenantCtx-only).
- ❌ Skip Merkle verify em UPDATE (fail-closed via WI-003 trait).
- ❌ Allow result_hash overwrite (immutability INV).
- ❌ Sync emit audit em hot path (outbox pattern).
- ❌ Block GET em outputs drift (warn-only; reconcile fixes).
- ❌ Variable-time tenant compare (subtle).
- ❌ Multi-tenant action_digest namespace (UNIQUE per `(tenant_id, action_digest)`).
- ❌ Custom proto schema (REAPI vendored).
- ❌ Drop ActionResult metadata fields (REAPI compliance).
- ❌ gRPC + REST duplicate impl (single trait).
- ❌ D1-first then R2 (R2-first; orphan recoverable).
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).

---

**Fim WI-S04-001.** Próximo: WI-S04-002 (D1 ac_meta migration + R2 ac bucket per region + UNIQUE constraint).
