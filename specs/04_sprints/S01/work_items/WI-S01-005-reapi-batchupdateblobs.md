---
id: "WI-S01-005"
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

Implementar Worker handlers para REAPI v2 `ContentAddressableStorage::BatchUpdateBlobs` (small blobs ≤ 4 MiB inline) + `ByteStream::Write` (single blob ≤ 5 MiB streaming) que orchestram:

```rust
async fn batch_update_blobs(ctx: TenantCtx, req: BatchUpdateBlobsRequest)
    -> Result<BatchUpdateBlobsResponse, Status>;

async fn bytestream_write(ctx: TenantCtx, req_stream: impl Stream<Item = WriteRequest>)
    -> Result<WriteResponse, Status>;
```

Cada handler:
1. **Auth middleware** valida PAT + injeta `TenantCtx` (S-03 stub).
2. **Body validation**: size ≤ 5 MiB (HTTP 413 se exceeds; multipart é S-05).
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
5. **Audit emission é fail-closed**: se audit emit falha, write é rolled back (R2 best-effort delete; D1 row not inserted).

REAPI v2 conformance: BatchUpdateBlobs request format proto-defined (`google::bytestream::WriteRequest`); response format proto-defined; gRPC status codes mapped consistently (400=INVALID_ARGUMENT, 401=UNAUTHENTICATED, 403=PERMISSION_DENIED, 409=ABORTED ou ALREADY_EXISTS, 413=OUT_OF_RANGE, 500=INTERNAL).

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
   - Per-request: auth → VerifiedBody → R2Writer → BlobMetaStore → audit.
   - Response: `[Response { digest, status }]` (per-blob outcome).
2. **`ByteStream::Write` handler**:
   - Streaming write up to 5 MiB; size guard.
   - Single blob per stream.
3. **Auth middleware** integration (S-03 stub):
   - Reads `Authorization: Bearer <PAT>`.
   - Returns 401 se invalid; 403 se scope insufficient.
   - Injects TenantCtx.
4. **HTTP REST surface** equivalent `POST /v1/cas/<digest>` (multipart form; small blobs).
5. **Métricas + audit** per request.
6. **error_taxonomy mapping** completa para todos error variants.

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

  Scenario: Happy path single blob
    Given Tenant A authenticated with scope "cache:w"
    And BatchUpdateBlobsRequest with 1 blob (digest=D_X, data=5KB hello)
    When handler processes request
    Then VerifiedBody::new succeeds
    And R2Writer.put called with correct path
    And BlobMetaStore.insert returns Inserted
    And response status[0] = OK
    And audit event "corelink.cas.put_completed" emitted

  Scenario: Hash mismatch (cache poisoning attempt)
    Given Request { digest=D_X, data=different_content }
    When handler processes
    Then VerifiedBody::new returns HashMismatch
    And response status[0] = code 13 (ABORTED) com message "digest mismatch"
    And HTTP equivalent maps to 409
    And audit "corelink.cas.poisoning_attempt" emitted
    And R2 NOT called

  Scenario: PAT scope insufficient
    Given PAT scope "cache:r" only (no cache:w)
    When BatchUpdateBlobs called
    Then response status = code 7 (PERMISSION_DENIED)
    And HTTP 403
    And error_code COR_AUTH_SCOPE_INSUFFICIENT

  Scenario: Idempotent retry
    Given digest D_X already persisted
    When BatchUpdateBlobs same request retried
    Then response status[0] = OK (idempotent)
    And R2 If-None-Match returns 412 → mapped to Ok
    And BlobMetaStore.insert returns AlreadyExists → mapped to Ok
    And no duplicate audit event (dedup via request_id)

  Scenario: Size limit exceeded
    Given Request com blob de 6 MiB
    When handler reads body
    Then 5 MiB limit hit; abort
    And response code 11 (OUT_OF_RANGE) → HTTP 413
    And error_code COR_CAS_BLOB_TOO_LARGE
    And next_action hint "Use multipart upload (S-05)"

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

### 9.2 Why audit emission é synchronous (fail-closed)

Async audit emission permitiria write succeed mas audit fail silent → compliance gap. Synchronous: if audit fails, write é rolled back. Trade-off: latency overhead ~10-20ms per request; aceitável em SLO budget (1s p99 50 blobs).

### 9.3 Why streaming size guard (não Content-Length trust)

Atacante pode mentir Content-Length. Stream guard count bytes durante read; abort em 5 MiB threshold antes de OOM.

### 9.4 ADR potencial?

Não. Patterns reused.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** REAPI v2 conformance (bazelbuild/remote-apis) BatchUpdateBlobs 100% pass (EVT-002).
- [ ] **10.5.2** Property test 100k iter cross-tenant attempts → 0 successes (EVT-002).
- [ ] **10.5.3** Latency SLO-LAT-CAS-PUT p99 ≤ 1s sustained 72h staging (EVT-021).
- [ ] **10.5.4** error_taxonomy mapping completa: 100% Result variants → COR_* code (EVT-002).
- [ ] **10.5.5** Audit emission fail-closed verified em chaos test.
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
- INV-AUDIT-APPEND-ONLY (CRITICAL): audit emit fail-closed.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| gRPC handler | `crates/corelink-worker/src/reapi/cas_write.rs` | Rust |
| HTTP REST handler | `crates/corelink-worker/src/http/cas_write.rs` | Rust |
| Auth middleware integration | `crates/corelink-worker/src/middleware/auth.rs` | Rust |
| REAPI conformance test | `tests/reapi_conformance/cas_write.rs` | Rust test |
| Integration test E2E | `tests/integration_cas_write.rs` | Rust test |

## 14. Quality Standards SOTA

Standard 14.5.1..10 (zero unsafe, doc, coverage 90%, perf p99 ≤ 1s, SAST clean, métricas RED, runbook RB-FM-254, breaking semver, memory bounded, cost regression).

## 15. Chaos Experiments

1. Auth stub failure → graceful 401.
2. R2 5xx storm → exponential backoff.
3. Audit emit failure → write rolled back.
4. Concurrent 100 same-digest different-tenants → all succeed (paths distinct).
5. Concurrent 100 same-digest same-tenant different-body → only first succeeds; rest see 409.

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

Per-request cost: auth check + D1 query + R2 PutObject + audit emit ~= $0.0002 per request. TCO 12m com 10M req/dia: ~$22k/yr.

## 23. API Contract

REAPI v2 proto-frozen; gRPC + HTTP semver-locked.

## 24. Post-mortem Hooks

- Cross-tenant write detected → CRITICAL.
- Idempotency violated (same digest different bodies persisted) → CRITICAL.
- Audit emit silent failure → CRITICAL compliance.

## 25. Rollback / Recovery

Hot rollback via WASM previous version.

## 26. Security & Privacy

STRIDE + LINDDUN per S-01 sprint contract §12.

## 27. Knowledge Transfer

Tech talk: "REAPI v2 + 5-Layer Defense in Action".

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TenantCtx leak via shared mutable state | L | M | CRITICAL | M | LOW | Immutable struct + per-request creation |
| R-002 | Size limit bypass via Content-Length lie | M | M | HIGH (FM-403) | M | LOW | Streaming byte counter; abort early |
| R-003 | REAPI conformance regression | M | L | MEDIUM | L | LOW | Conformance suite CI per PR |
| R-004 | Audit emit failure silent | L | M | HIGH | L | LOW | Fail-closed write rollback; SEV-2 alert |
| R-005 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.10 |

## 29. Review Checkpoints

1. Design (D+0): Architect.
2. Code (D+3): peer + Security.
3. Adversarial (pre-merge): cross-tenant property + REAPI conformance.

## 30. Sign-off (HIGH_RISK 10-12)

Standard.

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.1 creation.

## 32. Anti-patterns evitados

- ❌ Implicit tenant from header.
- ❌ Buffer full request before validate.
- ❌ Async audit fail-open.
- ❌ Custom HTTP/2 retry logic.
- ❌ Trust Content-Length.

---

**Fim WI-S01-005.** Próximo: WI-S01-006 (Property tests).
