---
id: "WI-S01-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
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
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
tags: ["wi", "s01", "cas", "reapi", "bytestream", "batchupdateblobs"]
---

# WI-S01-005 — REAPI `BatchUpdateBlobs` + `ByteStream::Write` Handler

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-005 |
| Título | REAPI BatchUpdateBlobs + ByteStream::Write Worker handler |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (tenant ctx propagation crítica), FF-HR-005 (CTRL-CAS-001 + CTRL-AUTH-* enforcement no API surface) |

## 1. Intent

Implementar Worker handlers para REAPI v2 `ContentAddressableStorage::BatchUpdateBlobs` (per-blob inline ≤ 4 MiB; **aggregate request ≤ 4 MiB** = REAPI `MaxBatchTotalSizeBytes` advertised via `Capabilities.GetCapabilities`; blobs > 4 MiB devem usar ByteStream::Write) + `ByteStream::Write` (single blob ≤ 5 MiB streaming) que orchestram:

```rust
async fn batch_update_blobs(ctx: TenantCtx, req: BatchUpdateBlobsRequest)
    -> Result<BatchUpdateBlobsResponse, Status>;

async fn bytestream_write(ctx: TenantCtx, req_stream: impl Stream<Item = WriteRequest>)
    -> Result<WriteResponse, Status>;
```

Cada handler:
1. **Auth middleware** valida PAT + injeta `TenantCtx` (S-03 stub).
2. **Body validation**: BatchUpdateBlobs aggregate ≤ 4 MiB (REAPI MaxBatchTotalSizeBytes; HTTP 413 se exceeds → cliente fragmenta) OR ByteStream::Write single blob ≤ 5 MiB (HTTP 413 se exceeds; multipart é S-05).
3. **VerifiedBody::new** (WI-S01-002) valida hash → reject COR_CAS_DIGEST_MISMATCH se mismatch.
4. **R2Writer.put** (WI-S01-003) escreve com HMAC tenant path + If-None-Match.
5. **BlobMetaStore.insert** (WI-S01-004) idempotent INSERT.
6. **Audit emission** + métricas.

## 2. Narrative (HIGH_RISK ≥ 300)

REAPI handlers são a **superfície externa** do CAS write path — primeira linha de defesa contra requests malformados, atacantes adversariais, e bugs de cliente. Falhas catastróficas:

1. **Tenant ctx leak**: handler usa cached/stale TenantCtx → write atribuído a wrong tenant.
2. **Size limit bypass**: cliente envia 5 GiB blob com `Content-Length: 1MiB` → memory exhaustion + Worker OOM (FM-403).
3. **Concurrent same-digest different-body race**: dois clientes diferentes (ou same cliente) PUT same digest com bodies diferentes → INV-CAS-IDEMPOTENCY violation se R2 não enforce If-None-Match.
4. **Error mapping inconsistente**: hash mismatch retorna 500 em vez de 409 → cliente retry sem corrigir → infinite loop.
5. **Audit emission falha**: write succeeds mas audit não emit → CTRL-AUDIT-003 violation (compliance gap).

Mitigação:
1. **TenantCtx é immutable per request**; criado por middleware; passed by value to handler; impossível corrupt.
2. **Streaming size guard**: count bytes durante read; abort early se exceeds 5 MiB; return 413.
3. **WI-S01-002 + WI-S01-003 design** garantem race-safety: VerifiedBody enforces hash match; R2 If-None-Match enforces write-once.
4. **error_taxonomy mapping** explicit: each Result variant → specific COR_* code → SDK throws specific exception.
5. **Audit OUTBOX INSERT é fail-closed** (handler → `audit_outbox` table via D1 `db.batch` atomic; INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER): se outbox INSERT falha, write é rolled back (R2 best-effort delete; D1 `blob_meta` row not committed). Drain do outbox para S-09 audit chain é assíncrono best-effort com at-least-once + dedup por `request_id`; INV-AUDIT-APPEND-ONLY é enforced no chain (S-09 owner), não aqui.

REAPI v2 conformance: BatchUpdateBlobs request/response format proto-defined em `build.bazel.remote.execution.v2.BatchUpdateBlobsRequest` + `BatchUpdateBlobsResponse` (per `bazelbuild/remote-apis @ v2.13.0` `remote_execution.proto`); ByteStream::Write usa `google::bytestream::WriteRequest` (proto separado; handler distinto §6.1.2). gRPC canonical status codes mapped consistently + gRPC numeric codes: 400=INVALID_ARGUMENT (3), 401=UNAUTHENTICATED (16), 403=PERMISSION_DENIED (7), 409=ABORTED (10) ou ALREADY_EXISTS (6), 413=RESOURCE_EXHAUSTED (8) (per §9.6 — não OUT_OF_RANGE which é read-past-EOF), 500=INTERNAL (13).

**Risk justification HIGH_RISK:**
- **FF-HR-002**: TenantCtx propagation bug = cross-tenant catastrophic.
- **FF-HR-005**: implementa security controls externally facing (CTRL-CAS-001 + CTRL-AUTH-*); first defense layer.
- **Adversarial surface**: external API; attackers probe.

## 3. Customer Impact & Journey

**JTBD:** "Como Bazel/Buck2 cliente, eu envio `BatchUpdateBlobs` com 50 blobs por request; espero ack imediato + idempotent retry seguro + clear error messages quando algo falha."

**Customer-visible:**
- gRPC + REST surface conformance REAPI v2.
- Latency p99 BatchUpdateBlobs (50 small blobs) ≤ 1s.
- Clear error mapping (COR_CAS_DIGEST_MISMATCH 409 actionable).

## 4. Capability Mapping

CAP-CAS-001/002/003 — implementação API surface.
Trace: `remote_cache_product_profile.md §3 (REAPI semantics)` + `auth_model.md §8.1 (5-layer defense, layer 1 PAT scope)`.

## 5. Tipo

API Handler; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **gRPC `BatchUpdateBlobs` handler** em `crates/corelink-worker/src/reapi/cas_write.rs`:
   - Request: `[Request { digest, data }]` (small blobs inline).
   - Response: `[Response { digest, status }]` (per-blob outcome).
   - Per-blob orchestration **strictly ordered** (vide §6.1.7 dual-write reconciliation contract):
     1. Auth (PAT scope `cache:w`) — fast reject pre-storage.
     2. `VerifiedBody::new(claimed_digest, body)` — BLAKE3 verify; reject pre-R2.
     3. **R2 PUT** com `If-None-Match: *` (blob físico primeiro).
     4. **D1 batch atomic** via `db.batch([INSERT blob_meta, INSERT audit_outbox])` — index + audit em mesma transação.
     5. Response success retornada ao cliente; **audit emission é fire-and-forget pós-response** (drained por background worker; vide §6.1.6).

2. **`ByteStream::Write` handler**:
   - Streaming write up to 5 MiB; size guard byte-counting (não Content-Length trust).
   - Single blob per stream; mesmo pipeline orchestration §6.1.1 acima.

3. **Auth middleware** integration (S-03 stub via `auth_stub_contract.md` Lote 9.5b):
   - Reads `Authorization: Bearer <PAT>`.
   - Returns 401 se invalid; 403 se scope insufficient.
   - Injects `TenantCtx { principal_id, tenant_id, scopes: PatScopes }` (scope-set, não single string — futureproof S-03 fine-grained `cache:w:cas`).

4. **HTTP REST surface** equivalent `POST /v1/cas/<digest>` (multipart form; small blobs).

5. **Métricas + structured logs** per request (audit é separado §6.1.6).

6. **`audit_outbox` table + drain worker** (Outbox Pattern):
   - Schema (criado por WI-S01-004): `audit_outbox(id, tenant_id, digest, request_id, event_type, emitted_at NULL, payload_json)`.
   - Drain worker: scheduled CF Cron `*/30 * * * * *` (30s); SELECT outbox rows com `emitted_at IS NULL` LIMIT 100; emit a S-09 audit chain; UPDATE `emitted_at = now()`. **At-least-once**: re-emit possível em retry; consumer dedup por `request_id`.
   - Métrica `corelink.cas.audit_outbox.lag_seconds` (gauge); alert se > 60s.

7. **Dual-write reconciliation contract (R2 ↔ D1)** — resolve a armadilha documentada em `storage_semantics_matrix.md §3.2` (orphan R2 blob se Worker crash entre R2 PUT e D1 INSERT):
   - **Orphan classes definidas**:
     - **R2-only orphan**: R2 PUT 200 OK + D1 batch falhou/Worker crashed. Mitigação: WI-S01-005 handler tenta `R2.delete` best-effort **somente se** R2 PUT retornou 200 (não 412 — 412 = idempotent dup, não nosso). Se delete também falha → relegate ao GC sweep S-06.
     - **D1-only orphan** (impossível por design): D1 batch só executa após R2 PUT success; ordering é R2-first.
     - **audit_outbox orphan**: row em outbox sem `emitted_at` por > 5 min. Drain worker re-attempts; alert SEV-2 se backlog > 100.
   - **Reconciliation cadence**:
     - **Fast path** (in-handler): R2 best-effort delete em D1-batch failure (RTO ~10ms).
     - **Slow path** (S-06 GC sweep, forward dependency): scheduled job (1×/dia) lista R2 prefix; LEFT JOIN com D1 `blob_meta`; órfãos > 5min de idade → ou ingestão de volta (cria blob_meta retroativamente; **rejeitado** — viola idempotency invariant) ou DELETE em R2 (recupera storage). Decisão: DELETE.
   - **Contract referência**: ADR-0027 (forward; whitelisted) — "CoreLink dual-write reconciliation: R2-first + audit outbox + GC sweep".
   - **Não-objetivo**: full distributed transaction (2PC sobre R2/D1) — Cloudflare não oferece coordinator; trade-off aceito = transient inconsistency window ≤ 24h até GC sweep, bounded por monitoring.

8. **error_taxonomy mapping** completa para todos error variants (§23).

### 6.2 Out-of-scope

- Read path (`Read` + `GetBlob` + `FindMissingBlobs`): WI-S02-001/002.
- Multipart `SplitBlob`/`SpliceBlob` (> 5 MiB): WI-S05-001.
- AC handlers (GetActionResult/UpdateActionResult): WI-S04-001.

## 7. Anti-Scope

- ❌ Streaming chunked write para single-blob (S-05 multipart only).
- ❌ Custom HTTP/2 tuning (CF handles).
- ❌ gRPC compression (default; not custom).
- ❌ Request body buffering > 5 MiB (anti-pattern memory; reject 413 early).
- ❌ Implicit tenant_id from header (sempre derive from PAT).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: BatchUpdateBlobs handler

  Scenario: Happy path single blob (Outbox Pattern orchestration)
    Given Tenant A authenticated with scope "cache:w"
    And BatchUpdateBlobsRequest with 1 blob (digest=D_X, data=5KB hello)
    When handler processes request
    Then VerifiedBody::new succeeds
    And R2Writer.put called with correct path AND If-None-Match: * (returns 200)
    And db.batch([INSERT blob_meta, INSERT audit_outbox]) executes atomically (returns 2 rows affected)
    And response status[0] = OK returned to client (T+90ms p50)
    And eventually drain worker emits "corelink.cas.put_completed" to S-09 audit chain (T+30s p50, T+60s p99)
    And outbox row UPDATE emitted_at = now()

  Scenario: Hash mismatch (cache poisoning attempt)
    Given Request { digest=D_X, data=different_content }
    When handler processes
    Then VerifiedBody::new returns HashMismatch
    And response status[0] = code 10 (ABORTED) com message "digest mismatch" (gRPC canonical: ABORTED=10; INTERNAL=13)
    And HTTP equivalent maps to 409
    And audit "corelink.cas.poisoning_attempt" emitted
    And R2 NOT called

  Scenario: PAT scope insufficient
    Given PAT scope "cache:r" only (no cache:w)
    When BatchUpdateBlobs called
    Then response status = code 7 (PERMISSION_DENIED)
    And HTTP 403
    And error_code COR_AUTH_SCOPE_INSUFFICIENT

  Scenario: Idempotent retry (R2 owns; D1 deduplicates)
    Given digest D_X already persisted (R2 200 prior write; D1 row present)
    When BatchUpdateBlobs same request retried
    Then R2 PUT If-None-Match returns 412 → handler treats as idempotent (NOT our blob; do NOT delete)
    And handler skips R2 delete on rollback path (critical: 412 != "we created it")
    And db.batch attempts INSERT OR IGNORE blob_meta + INSERT audit_outbox(retry_dedup=true)
    And blob_meta INSERT is no-op (already exists)
    And outbox INSERT skipped if request_id already in outbox (idempotent ingestion)
    And response status[0] = OK
    And NO duplicate audit event downstream (dedup via request_id at consumer)

  Scenario: D1 batch fails after R2 PUT 200 (orphan reconciliation fast-path)
    Given R2 PUT returned 200 (we created blob)
    When db.batch fails (D1 timeout / network error)
    Then handler invokes R2.delete(path) best-effort (max 1 retry; ≤ 100ms)
    And response status[0] = ABORTED (gRPC code 10) com message "transient_storage_failure"
    And HTTP 503 com Retry-After: 1
    And metric corelink.cas.dual_write_rollback_total{result="r2_delete_ok|r2_delete_failed"} incremented
    And if R2 delete also fails → orphan logged + GC sweep S-06 will reconcile (slow path)

  Scenario: D1 batch fails after R2 PUT 412 (no-op rollback)
    Given R2 PUT returned 412 (blob already existed; we did NOT create)
    When db.batch fails (transient)
    Then handler does NOT call R2.delete (would delete legitimate blob owned by prior writer)
    And response status[0] = ABORTED (transient)
    And metric corelink.cas.dual_write_rollback_total{result="412_skip_delete"} incremented

  Scenario: Size limit exceeded
    Given Request com blob de 6 MiB
    When handler reads body
    Then 5 MiB limit hit; abort
    And response code 8 (RESOURCE_EXHAUSTED) → HTTP 413
    And error_code COR_CAS_BLOB_TOO_LARGE
    And next_action hint "Use multipart upload (S-05)"
    (gRPC canonical mapping verified vs bazelbuild/remote-apis @ v2.13.0)

  Scenario: Audit outbox drain lag alert
    Given audit_outbox table accumulates 100 rows com emitted_at IS NULL
    When drain worker scheduled run completes
    Then if outbox lag > 60s p99 → metric corelink.cas.audit_outbox.lag_seconds emitted
    And alert SEV-2 fires if sustained > 5min
    And runbook RB-FM-OUTBOX-DRAIN engaged

  Scenario: REAPI v2 conformance — BatchUpdateBlobs proto
    Given bazelbuild/remote-apis test suite
    When BatchUpdateBlobs ops invoked
    Then 100% pass rate

  Scenario: Latency SLO
    Given 50 small blobs em single BatchUpdateBlobs
    When processed
    Then p99 latency ≤ 1s sustained 72h staging
```

## 9. Design Decisions

### 9.1 Why per-blob result em batch (não fail-fast)

Bazel cliente envia 50 blobs em batch; se 1 hash mismatch fail-fast all → cliente retry todos 50 → wasteful. Per-blob result permite cliente retry only failed → graceful degradation.

### 9.2 Why Outbox Pattern para audit (não synchronous emit no hot path)

Synchronous audit emit em hot path tem dois problemas:
1. **Latency tax**: emit RPC para S-09 audit chain ~10-30ms p99 → afeta SLO p99 ≤ 1s budget.
2. **Failure mode contraditório**: se emit falha após D1 INSERT, "rollback D1 row" requer DELETE — viola INV-CAS-IMMUTABILITY (blob_meta é append-only conceptualmente; DELETE é GC-only após retention window).

**Outbox Pattern resolve**:
- Audit event escrito em `audit_outbox` table **dentro da mesma D1 transaction** que `blob_meta` INSERT (atômico via `db.batch([...])`).
- Background drain worker emit ao S-09 chain com at-least-once guarantee; consumer dedup por `request_id`.
- Failure modes:
  - D1 batch falha → R2 best-effort delete (vide §6.1.7); cliente recebe ABORTED; sem partial state em D1.
  - Drain worker falha persistente → outbox lag métrica + alert SEV-2 + runbook RB-FM-OUTBOX-DRAIN; eventual consistency garantida.
- Trade-off aceito: window de visibilidade audit ≤ 60s p99 (drain cadence 30s + emit ~10s); SOC 2 compliance OK (audit é durable em D1 desde T+0; exposure window é só "delay to S-09 chain").

### 9.3 Why R2-first em ordering (não D1-first)

D1-first criaria index sem blob físico → read-side 404 + AuthZ confusion + cliente cache invalidation paradox. R2-first garante que se index existe, blob existe; se blob existe sem index, GC sweep limpa silently. Asymmetric reconciliation é mais simples + correto.

### 9.4 Why streaming size guard (não Content-Length trust)

Atacante pode mentir Content-Length. Stream guard count bytes durante read; abort em threshold antes de OOM. Memory bounded por surface: (a) **ByteStream::Write**: ≤ 5 MiB single body + buffer overhead = ≤ 8 MiB peak Worker. (b) **BatchUpdateBlobs**: aggregate ≤ 4 MiB total (REAPI MaxBatchTotalSizeBytes) + bounded concurrency 16 inflight + buffer overhead = ≤ 8 MiB peak Worker (todos os blobs in-flight cabem dentro do aggregate cap; 16-concurrency aplica ao R2 PUT round-trip parallelism, não a buffers independentes per blob). Both surfaces unified em ≤ 8 MiB peak budget canonical.

### 9.5 Why bounded concurrency em batch (16 concurrent puts)

Worker CPU 50ms/request budget. Batch 50 blobs sequential = 50 × 30ms = 1.5s (excede). Parallel via `try_join_all` sem limit = R2 round-trip dominates + Worker subrequest count limit (50). Bounded `buffer_unordered(16)` = balance: ~3-4 parallel rounds × 30ms = 90-120ms total. Tunable via env var.

### 9.6 Why gRPC status code `RESOURCE_EXHAUSTED` (8) — não `OUT_OF_RANGE` (11)

REAPI v2 conformance + gRPC canonical mapping: `OUT_OF_RANGE` é semantica read-past-EOF; `RESOURCE_EXHAUSTED` é "operation rejected because system resource exhausted" (size limit fits). Verificado contra `bazelbuild/remote-apis @ v2.13.0` test suite. HTTP equivalent 413 Payload Too Large.

### 9.7 ADR potencial?

**Sim — ADR-0027**: "Dual-write reconciliation contract (R2-first + audit outbox + GC sweep)" — documenta trade-off vs full 2PC + reconciliation cadence + orphan classes + monitoring. Whitelisted em validate_references.py até materializar em S-01 implementation.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** REAPI v2 conformance (bazelbuild/remote-apis) BatchUpdateBlobs 100% pass (EVT-002).
- [ ] **10.5.2** Property test 100k iter cross-tenant attempts → 0 successes (EVT-002).
- [ ] **10.5.3** Latency SLO-LAT-CAS-PUT p99 ≤ 1s sustained 72h staging (EVT-021).
- [ ] **10.5.4** error_taxonomy mapping completa: 100% Result variants → COR_* code (EVT-002).
- [ ] **10.5.5** Audit OUTBOX INSERT fail-closed verified em chaos test (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER); drain to S-09 chain at-least-once verified em integration test.
- [ ] **10.5.6** Cost regression gate: handler hot path benchmark (§14.10).

## 11. DoD

- [ ] gRPC + HTTP handlers impl.
- [ ] REAPI conformance test suite passa.
- [ ] Property test cross-tenant.
- [ ] Auth middleware integration via stub interface (auth_stub_contract.md).
- [ ] Métricas + audit emit.
- [ ] error_taxonomy mapping.
- [ ] PRR + Security + Architect sign-off.

## 12. Invariants

- INV-TENANT-ISOLATION (CRITICAL): TenantCtx propagation correto.
- INV-CAS-INTEGRITY (CRITICAL): VerifiedBody envelope enforced.
- INV-CAS-IMMUTABILITY (CRITICAL): R2 If-None-Match + D1 INSERT OR IGNORE.
- INV-CAS-IDEMPOTENCY (CRITICAL): same digest → same body byte-identical.
- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL): handler→outbox INSERT atomic via D1 batch; fail-closed rollback (canonical scope para este WI).
- INV-AUDIT-APPEND-ONLY (CRITICAL; downstream): chain immutability enforced em S-09 audit chain (reference apenas; não enforced neste WI).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| gRPC handler | `crates/corelink-worker/src/reapi/cas_write.rs` | Rust |
| HTTP REST handler | `crates/corelink-worker/src/http/cas_write.rs` | Rust |
| Auth middleware integration | `crates/corelink-worker/src/middleware/auth.rs` | Rust |
| Outbox drain worker | `crates/corelink-worker/src/scheduled/outbox_drain.rs` | Rust (CF Cron) |
| REAPI conformance test | `tests/reapi_conformance/cas_write.rs` | Rust test |
| Integration test E2E | `tests/integration_cas_write.rs` | Rust test |
| Chaos suite (orphan reconciliation) | `tests/chaos/dual_write.rs` | Rust |
| ADR-0027 | `specs/03_architecture/adrs/ADR-0027-dual-write-reconciliation.md` | Markdown |
| Runbook RB-FM-OUTBOX-DRAIN | `specs/05_runbooks/RB-FM-OUTBOX-DRAIN.md` | Markdown |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.5.2** rustdoc 100% public API + 4 examples (single blob, batch, idempotent retry, error mapping).
- **14.5.3** Test coverage ≥ 90% (`cargo tarpaulin`); critical handler paths 95%+.
- **14.5.4** Latency: p99 ≤ 1s para 50 small blobs (warm); p99 cold ≤ 1.5s; bounded concurrency 16 enforced.
- **14.5.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean; cargo-fuzz 1h corpus em proto deserializer.
- **14.5.6** Métricas RED + outbox lag gauge + dual-write rollback counter.
- **14.5.7** Runbooks: RB-FM-403 (size limit storm), RB-FM-OUTBOX-DRAIN, RB-FM-254 (R2 5xx storm).
- **14.5.8** Breaking changes em REAPI proto = sprint-spec ADR + client migration plan.
- **14.5.9** Memory bounded: ≤ 8 MiB peak Worker per request — ByteStream::Write (5 MiB body + 3 MiB overhead) OR BatchUpdateBlobs (≤ 4 MiB aggregate + ≤ 4 MiB scratch). Surface caps documented em `Capabilities.GetCapabilities` REAPI response.
- **14.5.10** Cost regression gate em CI: per-op cost ≤ $0.000010; weekly bench panel.

## 15. Chaos Experiments

1. **Auth stub failure** → graceful 401; verify no R2/D1 side-effects (fast-fail). Hypothesis: handler short-circuits before storage.
2. **R2 5xx storm** (50% PUT failure rate sustained 5min) → verify exponential backoff (PAT-RETRY-IDEMPOTENT-001) + bounded retries (max 3) + circuit breaker open after 50% sustained 1min. Hypothesis: handler does not pile up Worker subrequest queue.
3. **D1 batch timeout (after R2 PUT 200)** → verify R2 best-effort delete fires; metric `corelink.cas.dual_write_rollback_total{result="r2_delete_ok"}` incremented; orphan blob if delete also fails → GC S-06 path engaged within 5 min.
4. **D1 batch timeout (after R2 PUT 412)** → verify R2 delete is **NOT** called (would delete legitimate blob); metric `result="412_skip_delete"`.
5. **audit_outbox drain worker offline 10min** → outbox accumulates; lag alert SEV-2; verify durability (no lost events; consumer dedup on resume).
6. **Concurrent 100 same-digest different-tenants** → all 100 succeed (paths distinct via tenant_prefix HMAC).
7. **Concurrent 100 same-digest same-tenant different-body** → first writer wins R2 200; rest see VerifiedBody mismatch (digest != computed); 1 success + 99 ABORTED-codes. INV-CAS-IDEMPOTENCY enforced.
8. **Worker isolate cold start during request** → verify p99 cold ≤ 500ms (vs 200ms warm); cold-start metric `corelink.worker.cold_start_total` emitted.
9. **R2 region failover** (wnam offline) → tenant pinned to wnam fails 503; cross-region degradation behavior documented em SLO degradation matrix; S-14 forward implements failover.
10. **Worker subrequest budget exhaustion** (51+ R2 calls em batch) → verify bounded concurrency 16 enforces budget; rest queue + sequential.

## 16. PRR

PRR doc + Architect + Security + Crypto SME.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | gRPC BatchUpdateBlobs handler scaffold | 3h |
| ST-002 | ByteStream::Write streaming size guard | 3h |
| ST-003 | HTTP REST equivalent | 2h |
| ST-004 | Auth middleware integration via stub interface | 2h |
| ST-005 | Per-blob result aggregation | 2h |
| ST-006 | Audit emission fail-closed | 2h |
| ST-007 | error_taxonomy mapping completo | 1.5h |
| ST-008 | REAPI conformance test suite integration | 4h |
| ST-009 | Property test 100k cross-tenant | 2.5h |
| ST-010 | Integration test E2E + chaos | 3h |
| ST-011 | Métricas + dashboard | 1.5h |
| ST-012 | PRR + walkthrough | 2h |

**Total**: ~28.5h Optimistic; PERT ~33h.

## 18. Dependencies

- WI-S01-001/002/003/004 SEALED (todos prereqs).
- Auth stub interface (auth_stub_contract.md Lote 9.5b) ready.

### Outbound

- WI-S02-001 (read path) reuses auth middleware pattern.
- WI-S04-001 (AC handler) reuses pattern.

## 19. Effort PERT

O: 24h, M: 28.5h, P: 50h → PERT 32h.

## 20. Time-boxing

35h hard limit; escalation ST-001 > 6h.

## 21. Observability

8 métricas (sprint contract S-01 §observability).

## 22. Cost Analysis

**Per-request breakdown** (warm path, single 5 KiB blob):

| Component | Cost | Note |
|---|---|---|
| Worker invocation | $0.50/M requests | CF Workers paid plan |
| R2 PUT | $4.50/M Class A ops | dominate fixed cost |
| D1 batch (2 INSERTs) | $1/M reads (estimate; D1 prices per op) | atomic; counted as 1 batch |
| Audit outbox INSERT | included em D1 batch | zero marginal |
| Drain worker emit (S-09) | $0.50/M outbox rows × 0.05 amortized | scheduled; small |
| Subrequest count | 2 (R2 + D1) of 50 budget | well within |

Per-request total: **~$0.000007** (7 µ$/req).

**TCO 12m projection**:
- Workload: 10M req/dia × 365 = 3.65 B req/yr.
- Annual: 3.65 B × $0.000007 = **~$25.5k/yr**.
- Storage (R2): assume 30% writes new content avg 50 KB → 50 TB/yr → R2 storage $0.015/GB-month × 50 TB × 12 = **$9k/yr**.
- D1 storage (blob_meta + audit_outbox): ~30 GB/yr → trivial < $100/yr.
- **Total estimated**: $35-40k/yr (em 10M req/dia workload).

**Cost regression gate** (§14.10): per-request cost ≤ $0.000010 (43% headroom); CI bench fails se exceder. Métrica `corelink.cas.cost_per_op_usd` emitted weekly.

**Comparison vs alternatives**:
- BuildBuddy hosted: ~$50/build × 1000 builds/dia = $1.5M/yr (CoreLink CapEx model wins ≥ 50× para enterprise tier).
- Self-hosted Bazel remote: ~$20k/yr infra + 0.5 FTE ops ($75k) = $95k/yr (CoreLink wins 2-3×).

## 23. API Contract

REAPI v2 proto-frozen; gRPC + HTTP semver-locked.

## 24. Post-mortem Hooks

- Cross-tenant write detected → CRITICAL.
- Idempotency violated (same digest different bodies persisted) → CRITICAL.
- Audit emit silent failure → CRITICAL compliance.

## 25. Rollback / Recovery

Hot rollback via WASM previous version.

## 26. Security & Privacy

**STRIDE delta** (vs S-01 sprint contract §12 baseline):

- **Spoofing**: PAT scope `cache:w` enforcement em auth middleware (5-layer defense Layer 1); request com PAT tampered → 401 antes de R2/D1 touch. TenantCtx imutável construído por `auth.rs` com TDK-backed tenant_id resolution; impossível forge downstream.
- **Tampering**: VerifiedBody envelope (WI-S01-002) garante body bytes match claimed_digest; adversary modifying body durante in-flight → digest mismatch detected pre-R2 PUT. R2 If-None-Match: * previne overwrite de blob existente; idempotent write semantics.
- **Repudiation**: audit_outbox row criado **dentro da mesma D1 transaction** que blob_meta INSERT (impossível write succeed sem audit row). Drain worker emit ao S-09 chain (forward) com chain hash integrity. CloudEvents 1.0 spec format; `request_id` propagado client → handler → outbox → S-09 chain → SIEM.
- **Information disclosure**: claimed_digest, computed_digest, request_id em audit; **body bytes nunca em audit/logs/error messages** — INV-NO-BODY-IN-LOGS aplicada via clippy custom lint + grep CI gate. PAT plaintext nunca emitted (S-09 redact macro `redact_pat!` em error paths).
- **DoS**: per-PAT rate limit S-08 forward (cap 100 req/s); size limit 5 MiB streaming guard previne memory exhaustion; bounded concurrency 16 em batch previne Worker subrequest budget exhaustion (50 limit).
- **Elevation of privilege**: PAT scope é DB-fonte-de-verdade (não inferred from prefix); cliente não pode escalar `cache:r` → `cache:w` mudando PAT format.

**LINDDUN delta**:

- **Linkability**: tenant_id em audit é UUID v7 (pseudonymous); cross-tenant linkage impossível sem D1 access (privileged).
- **Identifiability**: principal_id (PAT owner) em audit é PII; redact em external SIEM forwarding via S-09 PII filter; full-fidelity em internal audit chain (hashed at-rest).
- **Non-repudiation**: append-only audit chain (S-09); blob_meta + audit em mesma D1 transaction garante consistency.
- **Detectability**: timing constant em error paths (vide WI-S02-004 forward); cliente não distingue "blob exists em outro tenant" vs "blob não existe".
- **Disclosure of information**: response payload contém apenas digest + status; nunca body content em error response.
- **Unawareness**: SLA documenta "audit eventual visibility ≤ 60s p99 via outbox pattern" — cliente não pode assumir audit emit é synchronous.
- **Non-compliance**: outbox row durability garante SOC 2 audit trail completeness; LGPD Art. 38 (registro de operações) satisfied via at-least-once + dedup.

## 27. Knowledge Transfer

- **Tech talk** (1h): "REAPI v2 + Outbox Pattern + 5-Layer Defense in Action" — internal team + external candidates onboarding.
- **Doc** `docs/internal/reapi-write-path.md` — sequence diagram cliente → Worker → R2 → D1 batch → outbox → drain worker → S-09 chain.
- **Runbook RB-FM-OUTBOX-DRAIN** — outbox lag spike triage (consumer offline, D1 query slow, S-09 chain ingestion failure, drain worker config drift).
- **Doc** `docs/internal/dual-write-reconciliation.md` — orphan classes, GC interaction, monitoring playbook.
- **Workshop** (2h): com Architect + AppSec + SRE Lead pós-merge — adversarial walkthrough (D1 partition, R2 partial-write, drain worker failure modes).
- **ADR-0027** — formal decision record para reuse em S-04 (AC handler), S-05 (multipart), S-06 (GC reconciliation reference).

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TenantCtx leak via shared mutable state | L | M | CRITICAL | M | LOW | Immutable struct + per-request construction; cargo-audit weekly |
| R-002 | Size limit bypass via Content-Length lie (FM-403 amplification) | M | M | HIGH | M | LOW | Streaming byte counter; abort em 5 MiB; fuzz test |
| R-003 | REAPI conformance regression em proto upgrade | M | L | MEDIUM | L | LOW | bazelbuild/remote-apis pinned @ v2.13.0; CI per PR |
| R-004 | R2-only orphan blob (Worker crash entre R2 PUT 200 e D1 batch) | M | M | MEDIUM | M | LOW | R2 best-effort delete + GC sweep S-06 (≤ 24h); cost monitoring |
| R-005 | audit_outbox drain worker offline > 1h | L | M | HIGH | L | LOW | Outbox durability D1; lag alert SEV-2; runbook; at-least-once |
| R-006 | R2 PUT 412 + D1 fail → wrong rollback (delete legitimate blob) | L | H | CRITICAL | M | LOW | Strict guard: only delete em 200, never em 412; chaos test §15.4 |
| R-007 | Bounded concurrency 16 inadequate sob load (Worker subrequest 50 budget) | M | M | MEDIUM | M | LOW | Tunable env var; load test em CI; cost regression gate |
| R-008 | gRPC status code drift (REAPI v2 spec change) | L | L | LOW | L | LOW | Conformance test; quarterly review |
| R-009 | Drain worker double-emit em retry (consumer dedup miss) | L | M | MEDIUM | L | LOW | At-least-once contract + request_id dedup em S-09 consumer |
| R-010 | Cost regression > 10% per-op | M | L | MEDIUM | L | LOW | §14.10 cost gate; weekly bench |
| R-011 | D1 batch latency drift (> 50ms p99) impacta SLO | M | M | MEDIUM | M | LOW | D1 metric panel; investigate query plan; reindex policy |
| R-012 | Audit emit lag > 60s p99 sustained → SOC 2 visibility gap | L | M | HIGH | L | LOW | SEV-2 alert + drain worker auto-scaling (S-13 forward) |

## 29. Review Checkpoints

1. Design (D+0): Architect.
2. Code (D+3): peer + Security.
3. Adversarial (pre-merge): cross-tenant property + REAPI conformance.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD_ | _pending_ | _pending_ |
| 11 | Architect | _TBD_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD_ | _pending_ | _pending_ |
| 13 | Crypto SME | _advisory; required para validação VerifiedBody integration_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S01-005 (Lote 10.1). |
| 1.1.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Lote 10.2bis P0 fixes (Agent R4 review remediation): outbox pattern explícito (§6.1.6/§9.2); R2-first orchestration (§6.1.1/§9.3); reconciliation contract ADR-0027 (§6.1.7/§9.7); gRPC code RESOURCE_EXHAUSTED (não OUT_OF_RANGE) (§9.6); bounded concurrency 16 (§9.5); §22 cost expanded; §26 STRIDE+LINDDUN delta full; §28 Risk Register 12-row; §15 chaos 10 experiments; §30 sign-off 13-row table; §14 standards expanded. |

## 32. Anti-patterns evitados

- ❌ Implicit tenant from header.
- ❌ Buffer full request before validate.
- ❌ Async audit fail-open.
- ❌ Custom HTTP/2 retry logic.
- ❌ Trust Content-Length.

---

**Fim WI-S01-005.** Próximo: WI-S01-006 (Property tests).
