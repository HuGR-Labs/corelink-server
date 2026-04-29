---
id: "WI-S02-001"
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
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s02", "cas", "reapi", "bytestream", "streaming", "tenant-isolation", "foundation"]
---

# WI-S02-001 — REAPI `ByteStream::Read` Handler + HTTP GET Surface + Tenant Context Propagation

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-02](../sprint.md) · **Assignee:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** até ≥ 2 reviewers nomeados
> **inherits_from:** SECURITY-MODEL + AUTH-MODEL + KEY-MANAGEMENT + INVARIANT-REGISTRY + DATA-MODEL + REMOTE-CACHE-PRODUCT-PROFILE + OBSERVABILITY-MODEL + FAILURE-MODES + RESILIENCE-PATTERNS

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S02-001 |
| Título | REAPI `ByteStream::Read` handler + HTTP GET surface + tenant context propagation |
| Sprint | S-02 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-tenant read = catastrophic), FF-HR-005 (CTRL-ISO-002 AuthZ on storage call) |
| Tier | Todos (core read path) |
| Fase produto | Fase 1 — Remote Cache |
| Foundation WI? | ✅ Sim — primeiro WI do sprint S-02; serve como template e dependency hard para WI-S02-002..006 |

## 1. Intent

Implementar handlers REAPI v2 + HTTP REST para **CAS read path streaming** que enforça **5-layer tenant isolation defense** (auth_model §8.1) e **AuthZ on storage call** (CTRL-ISO-002):

```rust
// gRPC REAPI v2
async fn read(
    ctx: TenantCtx,
    req: ReadRequest,
) -> Result<impl Stream<Item = ReadResponse>, Status>;

// HTTP REST equivalent
async fn http_get_blob(
    headers: HeaderMap,
    Path(digest): Path<String>,
) -> Result<impl Stream<Item = Bytes>, ApiError>;
```

Onde:
- **`TenantCtx`** é injetado pelo middleware S-03 (stub durante S-02; real após S-03 SEALED) contendo `tenant_id` + `scopes` validated.
- **Streaming chunked** 1 MiB per chunk via `tokio::io::AsyncReadExt`; Worker memory bounded ≤ 50 MiB peak per request.
- **AuthZ check pré-R2**: D1 lookup `blob_meta WHERE digest=? AND tenant_id=? AND deleted_at IS NULL` antes de R2 GetObject; mismatch = **404 uniform per ADR-0028** (CrossTenantMasked variant) + audit emit `corelink.cas.cross_tenant_attempt` (forensics).
- **Tenant prefix derivation**: reusing `corelink-tenant-path` crate (S-01); zero novo crypto code.

Zero call sites alternativos permitidos para R2 GetObject — todo read path passa pelo handler ByteStream::Read OR HTTP GET.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

O CAS read path é a metade complementar do write path do S-01 e expõe **superfície catastrófica de cross-tenant exposure** caso bug em derivação de tenant prefix, AuthZ check, ou path lookup.

O ataque direto é: **Tenant B autentica com PAT legítimo, request URL contém digest D que pertence a Tenant A. Bug em path lookup permite Worker calcular prefix `P_A` (do digest's owner) em vez de `P_B` (do requester) → R2 GetObject retorna blob de A → cliente B vê dado alheio**. Sistema responde "200 OK" porque caminho aponta a blob existente; nenhum signal externo de que isolation foi quebrada. Cliente B agora tem visibilidade de Tenant A.

A mitigação é arquitetural: **AuthZ check pre-R2** (CTRL-ISO-002 em `security_model.md §6.4`). Implementação: D1 query `SELECT 1 FROM blob_meta WHERE digest=? AND tenant_id=? AND deleted_at IS NULL LIMIT 1` antes de R2 GetObject. Se row count = 0 → **404 (uniform per ADR-0028; CrossTenantMasked variant from S-02 WI-S02-005 NegativeCache MissReason taxonomy)** + audit emit `corelink.cas.cross_tenant_attempt` (forensics retém reason real; client observa apenas 404). Esta query é O(1) (UNIQUE index `(tenant_id, digest)`); overhead < 5ms p99.

Adicional: **5-layer defense** propagation:
- **Layer 1**: PAT scope `cache:r` enforced (S-03 middleware).
- **Layer 2**: Tenant prefix derivation HMAC (corelink-tenant-path crate; type-safe via TenantPrefix newtype).
- **Layer 3**: AuthZ check D1 (este WI).
- **Layer 4**: R2 GetObject path includes tenant prefix; impossível bypassar by construction.
- **Layer 5**: Audit cross-check (S-09 chain integrity) detects post-hoc anomalies.

Streaming complexity: REAPI ByteStream::Read suporta `read_offset` + `read_limit`; clients podem requisitar partial reads (resumable downloads). Worker NÃO carrega blob completo em memória — usa R2 SDK streaming response → forward chunks 1 MiB ao cliente. Memory peak per request ≤ 50 MiB hard.

**Risk justification para HIGH_RISK:**
- **FF-HR-002**: tocar `INV-TENANT-ISOLATION` direta. Framework §33.5.3 → HIGH_RISK automático.
- **FF-HR-005**: implementa CTRL-ISO-002 (AuthZ on storage call) + CTRL-CAS-002 envelope (read path expõe à client verify). Controles de segurança canônicos.
- **Blast radius**: todos os tenants em todas as regiões; first-touch surface do read path.
- **Reversibility**: one-way-door — bug shipped = dados de tenant A vistos por outros tenants até detection.

Por isso exige: 11 sign-offs canonical HIGH_RISK, TLA+ tenant_isolation.tla green sustained, property test 100k iter, SAST clean, adversarial review (pentest internal), chaos test (R2 latency + D1 failover), PRR completa, RB-FM-253 dry-run.

## 3. Customer Impact & Journey

**JTBD:** "Como tenant enterprise, eu preciso garantia de que meus blobs **NUNCA** são lidos por outro tenant, mesmo se PAT do meu colaborador for comprometido OR se houver bug em código interno OR se atacante tentar enumerar digests via timing side-channel."

**Journey touch-points:**
- **Direct**: cada CAS GET request bate este handler.
- **Customer-visible**: latency p99 < 300ms cold / < 100ms warm; bit rot detection client-side; clear error_taxonomy responses.
- **Indirect**: foundation pra Bazel/Buck2 cache HIT + SDK FFI wrapper functionality.
- **Critical para tier `enterprise`**: contratos DPA + BYOK herdam isolation guarantee.

## 4. Capability Mapping

- **CAP-CAS-004** (GET blob by digest) — IMPLEMENTA primary.
- **CAP-CAS-006** (Streaming read) — IMPLEMENTA chunked.
- **CAP-CAS-005** (Client-side verify) — DEPENDENCY hard upstream (S-02 WI-S02-003).
- **CAP-CAS-008** (Side-channel-resistant 404/403) — DEPENDENCY hard upstream (S-02 WI-S02-004).
- **CAP-CAS-007** (Negative caching) — DEPENDENCY soft upstream (S-02 WI-S02-005).

Trace canônico: `auth_model.md §8.1 (5 camadas de defesa)` → camada 3 (AuthZ check) implementada aqui; camadas 1-2 reused de S-01; camadas 4-5 herdadas via R2 path + audit.

## 5. Tipo e Classificação

- **Tipo:** Foundation (infra core read surface)
- **Lane:** HIGH_RISK
- **Lane forcing factors:** FF-HR-002, FF-HR-005

## 6. Escopo

### 6.1 In-scope

1. **Handler gRPC REAPI**:
   - `ByteStream::Read` request/response stream.
   - `read_offset` + `read_limit` semantics conforme REAPI v2 spec.
   - Chunk size 1 MiB.
2. **Handler HTTP**:
   - `GET /v1/cas/<digest>` REST endpoint.
   - `Accept: application/octet-stream` content negotiation.
   - Range header support REAPI-equivalent semantics.
3. **Tenant context propagation**:
   - `TenantCtx { tenant_id, user_id, scopes, mfa_ts }` injected via middleware (S-03 stub OK até real).
   - Validation: scope `cache:r` required; rejected outras scopes.
4. **AuthZ check pre-R2** (CTRL-ISO-002):
   - D1 query `SELECT 1 FROM blob_meta WHERE digest=? AND tenant_id=? AND deleted_at IS NULL LIMIT 1`.
   - Row count 0 → **404 uniform** (per ADR-0028; MissReason ∈ {NotFound, CrossTenantMasked, Tombstoned} all map to 404 com same body) + audit emit `corelink.cas.cross_tenant_attempt` (forensics retém reason real; S-09 alignment).
   - Query overhead p99 ≤ 5ms (criterion).
5. **R2 streaming read**:
   - Path: `cas-<region>/<TenantPrefix>/blake3/<hex[0:2]>/<hex[2:4]>/<hex>` reusing S-01 path lib.
   - R2 SDK `GetObject` com `Range` header se applicable.
   - Forward chunks 1 MiB ao cliente sem buffering full blob.
6. **Tombstone respect**: 404 se `blob_meta.deleted_at IS NOT NULL` (S-06 GC alignment; ADR-0028 uniform 404 freeze; 410 Gone deferido S-06).
7. **Métricas + observability**: 8 métricas novas (S-02 sprint.md §11).
8. **Error responses**: error_taxonomy.md `COR_CAS_BLOB_NOT_FOUND` (uniform 404 per ADR-0028 — covers NotFound + CrossTenantMasked + Tombstoned MissReason variants), `COR_CAS_DIGEST_MISMATCH` (client-verify path; bit rot). NOTE: `COR_CAS_TENANT_FORBIDDEN` (403) reservado para PAT scope failures (S-03), NÃO para cross-tenant blob access (which is uniform 404 + audit forensics).

### 6.2 Out-of-scope (deferred to other WIs)

- Client verify implementation: WI-S02-003.
- Constant-time 404 timing parity across MissReason variants: WI-S02-004 (this WI emits uniform 404 com normal timing; padding adicionado em WI-004 middleware).
- Negative cache populate/lookup: WI-S02-005.
- GetBlob unary + FindMissingBlobs batch: WI-S02-002.

## 7. Anti-Scope (expandido para HIGH_RISK)

- ❌ Write path mutation (S-01 owns).
- ❌ AC reads (S-04 owns; ByteStream::Read aqui só CAS).
- ❌ Cross-region failover (S-14).
- ❌ Compression on-the-fly (HTTP gzip OK transparente CF; não custom).
- ❌ Streaming compression (Zstd anti-scope GA).
- ❌ HEAD requests sem auth (REAPI não suporta).
- ❌ Custom Range headers além de REAPI semantics (`read_offset`/`read_limit`).
- ❌ HTTP/2 multiplexing tuning (CF handles).
- ❌ Read-after-write strong consistency aggressive (R2 strong consistency suficiente).
- ❌ Customer-facing telemetry opt-in (S-15 SDK owns; this WI emits server-side métricas).
- ❌ Batch GET multi-digest em single call (não REAPI; FindMissingBlobs é discovery, não download batch).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: REAPI ByteStream::Read CAS handler

  Background:
    Given Tenant A exists with tenant_id "uuid-A"
    And Tenant B exists with tenant_id "uuid-B"
    And TenantPrefix(A) = "P_A"; TenantPrefix(B) = "P_B"
    And Blob X exists in R2 at "cas-wnam/P_A/blake3/aa/bb/aabb..." with digest D_X
    And blob_meta has row (tenant_id="uuid-A", digest=D_X, deleted_at=NULL)

  Scenario: Successful read same-tenant (happy path)
    Given Tenant A is authenticated with PAT scope "cache:r"
    When Tenant A requests GET /v1/cas/<D_X>
    Then response status is 200
    And response body bytes equal blob X content
    And BLAKE3 hash of response body equals D_X
    And response time p99 < 300ms cold / < 100ms warm
    And metric corelink_cas_get_requests_total{result="hit"} incremented

  Scenario: Cross-tenant attempt masked as 404 (CRITICAL — ADR-0028 uniform freeze)
    Given Tenant B is authenticated with PAT scope "cache:r"
    When Tenant B requests GET /v1/cas/<D_X>
    Then response status is 404 (uniform per ADR-0028; CrossTenantMasked MissReason variant)
    And response body contains error_code "COR_CAS_BLOB_NOT_FOUND" (uniform)
    And no R2 GetObject was called (verified via R2 mock)
    And audit event "corelink.cas.cross_tenant_attempt" emitted with tenant_id="uuid-B", digest=D_X (forensics retém reason real)
    And metric corelink_cas_isolation_assertion_total{outcome="rejected"} incremented
    And response timing matches MissReason="never_existed" arm via WI-S02-004 padding (constant-time defense)

  Scenario: Tombstoned blob (deleted_at != NULL)
    Given blob X is soft-deleted (blob_meta.deleted_at = now())
    When Tenant A requests GET /v1/cas/<D_X>
    Then response status is 404
    And response body contains error_code "COR_CAS_BLOB_NOT_FOUND"

  Scenario: Streaming memory bounded
    Given Blob Y exists at 1 GiB size in R2
    When Tenant A requests GET /v1/cas/<D_Y> via ByteStream::Read
    Then Worker memory peak during request < 50 MiB
    And response is streamed in 1 MiB chunks
    And total bytes received equal 1 GiB

  Scenario: REAPI ByteStream read_offset semantics
    Given Blob Z exists at 10 MiB size
    When Tenant A requests ByteStream::Read with read_offset=5MiB, read_limit=2MiB
    Then response stream contains exactly bytes [5MiB..7MiB) of blob Z
    And response time p99 < 200ms warm

  Scenario: PAT scope insufficient
    Given Tenant A is authenticated with PAT scope "cache:w" only (no cache:r)
    When Tenant A requests GET /v1/cas/<D_X>
    Then response status is 403
    And error_code "COR_AUTH_SCOPE_INSUFFICIENT"

  Scenario: PAT invalid/expired
    Given PAT is revoked (S-03 revocation)
    When request comes with revoked PAT
    Then response status is 401
    And error_code "COR_AUTH_PAT_REVOKED"

  Scenario: Property test 100k iter cross-tenant
    When 100k concurrent reads execute with random (tenant_id, digest) pairs
    Then 0 successful cross-tenant reads
    And all rejections logged with tenant_id mismatch reason

  Scenario: AuthZ check D1 query overhead
    When 1000 reads execute steady-state
    Then D1 AuthZ query p99 ≤ 5ms (criterion benchmark)
```

## 9. Design Decisions

### 9.1 Why D1 AuthZ check pre-R2 (vs R2 ACL only)

R2 ACLs operam a nível bucket, não a nível object com tenant scoping. D1 query custa ~3-5ms p99 (UNIQUE index hit) — pequeno overhead vs R2 GetObject cold ~50-100ms. Trade-off justificado: defense-in-depth + audit trail emit (CTRL-AUDIT-003 alignment).

### 9.2 Why streaming chunk 1 MiB (vs smaller/larger)

- 1 MiB = 1 round-trip TCP/TLS típico em CF Edge.
- Worker memory budget 128 MiB; peak buffer 1 chunk + overhead = < 5 MiB per concurrent request → suporta 25 concurrent reads sem swap.
- Larger (16 MiB): reduz chunk count mas aumenta memory pressure.
- Smaller (256 KiB): excessive chunk overhead em RPC.

### 9.3 Why HTTP GET endpoint além de gRPC REAPI

Browsers + curl + admin tools precisam HTTP. REAPI gRPC nativo não é universalmente acessível. HTTP wraps mesma logic interna; same audit trail, same isolation defense.

### 9.4 Tombstone semantics (S-06 GC alignment)

- `deleted_at IS NOT NULL` em blob_meta → 404 + negative cache populate.
- Reads durante grace period (72h soft-delete) ainda respondem 404 — soft-delete é "not visible" not "physically gone".
- Undelete via S-01 re-write (S-07 CAP-EVICT-004 alignment) flips `deleted_at = NULL`.

### 9.5 ADR potencial?

Não identificada decisão arquitetural nova requerendo ADR. Patterns reused do S-01 + REAPI v2 spec compliance.

## 10. Completeness Criteria SOTA (HIGH_RISK = TODAS aplicáveis)

- [ ] **10.1.1 Property test 100k iter** cross-tenant → 0 successes (EVT-002).
- [ ] **10.1.2 TLA+** `tenant_isolation.tla` verde sustained CI (EVT-022).
- [ ] **10.1.3 Streaming memory** test: 1 GiB blob → Worker peak < 50 MiB (EVT-002).
- [ ] **10.1.4 D1 AuthZ overhead** ≤ 5ms p99 (criterion benchmark).
- [ ] **10.1.5 REAPI v2 conformance** test suite (bazelbuild/remote-apis) AC ops passa (EVT-002).
- [ ] **10.1.6 Tombstone respect**: read após soft-delete = 404 (EVT-018).
- [ ] **10.1.7 Audit emission**: cross-tenant attempt → CloudEvent emitted + chain integrity (EVT-049).
- [ ] **10.1.8 Cost regression gate** (Lote 9.4 §14.10): hot path ≤ 5ms overhead per request (EVT-002).

## 11. Definition of Done

- [ ] gRPC `ByteStream::Read` handler implementado + integration test.
- [ ] HTTP `GET /v1/cas/<digest>` handler implementado + integration test.
- [ ] AuthZ check D1 pre-R2 — verified via mock R2 (no GetObject call em rejected path).
- [ ] Streaming chunks 1 MiB — memory test 1 GiB blob green.
- [ ] Tombstone semantics — soft-delete blob retorna 404.
- [ ] 8 métricas emitindo (sprint.md §11).
- [ ] Property test 100k iter cross-tenant green em CI.
- [ ] TLA+ `tenant_isolation.tla` green sustained.
- [ ] SAST clean (clippy + semgrep).
- [ ] Code review por 2 peers + Security lead + Architect.
- [ ] Documentação inline (rustdoc) + arch diagram.
- [ ] PRR sign-offs 11 roles canonical documented.

## 12. Invariants

### Implementadas/enforced por este WI

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): AuthZ check D1 + tenant prefix derivation reused S-01 — read path honra isolation.
- **INV-CAS-IMMUTABILITY** (CRITICAL): tombstone semantics; reads após soft-delete respeitam.

### Foundation para outros WIs

- WI-S02-003 client verify consume body retornado por este handler.
- WI-S02-004 constant-time middleware envelopa este handler em timing-padding.
- WI-S02-005 negative cache populate em 404 paths gerados aqui.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| gRPC handler | `crates/corelink-worker/src/reapi/cas_read.rs` | Rust source |
| HTTP handler | `crates/corelink-worker/src/http/cas_read.rs` | Rust source |
| AuthZ check helper | `crates/corelink-worker/src/auth/storage_check.rs` | Rust source |
| Streaming utility | `crates/corelink-worker/src/storage/r2_stream.rs` | Rust source |
| Property test | `crates/corelink-worker/tests/prop_cas_read.rs` | Rust test |
| Integration test | `crates/corelink-worker/tests/integration_cas_read.rs` | Rust test |
| Criterion benchmark | `crates/corelink-worker/benches/cas_read_bench.rs` | Rust benchmark |

## 14. Quality Standards SOTA (HIGH_RISK = TODAS inclui 14.9 + 14.10)

- **14.1.1** Zero unsafe; zero unwrap em lib code (rustc -D warnings).
- **14.1.2** Documentação inline rustdoc + 3+ examples por public function.
- **14.1.3** Test coverage ≥ 90% via cargo-tarpaulin.
- **14.1.4** Perf p99 read warm ≤ 100ms; cold ≤ 300ms (criterion sustained).
- **14.1.5** SAST clean: clippy + semgrep + cargo-audit (zero HIGH/CRITICAL).
- **14.1.6** Métricas RED emitidas per request.
- **14.1.7** Runbook RB-FM-253 dry-run executed.
- **14.1.8** Breaking changes = bump major + migration doc.
- **14.1.9** Memory footprint Worker peak ≤ 50 MiB per concurrent request.
- **14.1.10** Cost regression gate § (Lote 9.4 §14.10): per-op cost benchmark; PR > 10% regression bloqueia.

## 15. Chaos Experiments (HIGH_RISK = obrigatório)

1. **R2 latency injection**: inject 500ms latency em R2 GetObject → handler stream timeout?
2. **R2 5xx errors 1%**: verify retry logic + circuit breaker (PAT-CIRCUIT-BREAKER-001 forward-looking).
3. **D1 primary failover**: verify AuthZ check graceful degrade (read replica fallback).
4. **Mid-stream client disconnect**: verify R2 stream cleanup + Worker memory release.
5. **Concurrent 100 reads same digest**: verify no race em D1 AuthZ check.

## 16. Production Readiness Review (HIGH_RISK = obrigatório)

- PRR doc em `specs/04_sprints/S02/PRR-WI-S02-001.md` (a criar pré-merge).
- Sign-offs 11 roles canonical (sprint.md §14).
- Adversarial review: pentest internal cross-tenant attempts.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | gRPC ByteStream::Read handler scaffold + REAPI proto codegen | 3h |
| ST-002 | HTTP GET handler + REST surface | 2h |
| ST-003 | AuthZ check D1 helper + integration | 2h |
| ST-004 | R2 streaming read (chunk 1 MiB; memory bounded) | 3h |
| ST-005 | Tombstone semantics + negative cache hook (forward to WI-005) | 2h |
| ST-006 | 8 métricas emit + dashboard JSON update | 2h |
| ST-007 | Integration test E2E (S-01 write → S-02 read) | 2h |
| ST-008 | Property test 100k cross-tenant | 3h |
| ST-009 | Criterion benchmark (warm + cold + AuthZ overhead) | 2h |
| ST-010 | RB-FM-253 dry-run + PRR doc + adversarial review | 2h |

**Total**: ~23h Optimistic; ~28h PERT-weighted (PERT (O+4M+P)/6 com M=23h, P=44h).

## 18. Dependencies

### Hard blockers

- **S-01 SEALED** — `corelink-tenant-path` crate; `blob_meta` D1 schema.
- **REAPI v2 proto** + `tonic` toolchain.
- **R2 SDK Rust** (Cloudflare official ou aws-sdk-s3 compatible).

### Soft blockers

- **S-03** auth real Clerk — staging stub interface OK até real.
- **S-09** observability stack — métricas emitidas mas dashboard live é nice-to-have.

### Outbound (este WI desbloqueia)

- WI-S02-002 (GetBlob unary) — reusa AuthZ check.
- WI-S02-003 (client verify) — consume response body de read handler.
- WI-S02-004 (constant-time middleware) — envelopa este handler.
- WI-S02-005 (negative cache) — populate em 404 paths.
- WI-S02-006 (property test E2E) — testa este handler em conjunto.

## 19. Effort Estimate (PERT, HIGH_RISK)

- **Optimistic (O)**: 18h (no surprises; familiar REAPI patterns).
- **Most-likely (M)**: 23h (sub-tasks como listed).
- **Pessimistic (P)**: 36h (REAPI quirks + chaos test rework).
- **PERT** = (O + 4M + P) / 6 = (18 + 92 + 36) / 6 = **24.3h**

## 20. Time-boxing & Escalation

- **Time-box**: 28h hard limit (PERT + 15% buffer).
- **Escalation**: se ST-001 (gRPC handler scaffold) > 6h → Architect intervention.
- **Mid-WI review**: D+2 (T+50% time-box).

## 21. Observability Plan

8 métricas novas (sprint.md §11 list completa); 1 dashboard widget (DASH-CAS read tab); 3 alerts (cross_tenant_attempt SEV-1 + side_channel_drift SEV-2 + SLO burn SEV-3).

## 22. Cost Analysis (HIGH_RISK = obrigatório + TCO 12m)

- **Per-request cost**: D1 query ($X micro-cents) + R2 GetObject ($Y micro-cents) + Worker CPU ms ($Z).
- **TCO 12 meses estimate** (1k tenants, 10M reads/dia avg):
  - R2: ~$2k/mo egress.
  - D1: ~$200/mo query volume.
  - Worker: ~$500/mo CPU.
  - Total: ~$32k/yr CAS read TCO (excludes negative cache savings; cobrir budget plan tier).

## 23. API / Contract Impact

- gRPC REAPI v2 ByteStream::Read — public API; SemVer commitment.
- HTTP REST `GET /v1/cas/<digest>` — public API; SemVer commitment.
- error_taxonomy.md `COR_CAS_*` — public error codes.
- Breaking changes futuros = major bump (S-15 SDK + S-18 docs alignment).

## 24. Post-mortem Hooks (HIGH_RISK = obrigatório)

Triggers que automaticamente abrem post-mortem doc:

- Cross-tenant read detected em produção (any) → CRITICAL post-mortem + Privacy Officer + breach notification consideration.
- AuthZ check D1 timeout sustained > 1min → post-mortem + escalation S-09 SLO breach.
- Streaming memory leak (Worker OOM) → post-mortem + chunk size + restart policy review.
- TLA+ tenant_isolation.tla CI red → CRITICAL post-mortem + invariant scope review.

## 25. Rollback / Recovery

- **Hot rollback**: deploy previous WASM binary; takes ≤ 5 min via S-13 progressive rollout.
- **Recovery**: zero data corruption (read-only path); rollback é safe.
- **Inadvertent cross-tenant exposure**: forensic via audit log (S-09 R2 7y); contact affected tenants; legal review.

## 26. Security & Privacy (HIGH_RISK = STRIDE + LINDDUN completos)

### STRIDE delta

- **Spoofing**: PAT validated via S-03 middleware; tenant_id derived from PAT, não trust client header.
- **Tampering**: Read path is pull-only; no mutation; INV-CAS-IMMUTABILITY enforced.
- **Repudiation**: audit emission per request (CTRL-AUDIT-003 alignment).
- **Information disclosure**: ✅ THE primary threat — mitigated via 5-layer defense + AuthZ check + property test 100k.
- **Denial of service**: per-PAT + per-IP rate limit (S-08 forward); negative cache reduces probe storm cost.
- **Elevation of privilege**: scope `cache:r` insufficient para writes; type-safe enforcement.

### LINDDUN delta

- **Linkability**: tenant_id é PII → never em logs externos sem redaction (S-09 R-S09-6).
- **Identifiability**: digest é content-derived; não identifies tenant directly.
- **Non-repudiation**: audit chain S-09.
- **Detectability**: side-channel timing 404/403 — covered em WI-S02-004.
- **Disclosure of information**: covered acima STRIDE.
- **Unawareness**: customer informed via SDK warning logs em verify failures.
- **Non-compliance**: GDPR Art. 32 (security measures) + LGPD Art. 46 — coberto via 5-layer defense + audit.

## 27. Knowledge Transfer (HIGH_RISK = obrigatório + onboarding test)

- Tech talk: "CAS Read Path: 5-Layer Tenant Isolation Defense" (≤ 30 min) — record + arquivar.
- Onboarding test: novo engenheiro lê este WI + corelink-tenant-path crate (1h) → answers 5 questions sobre cross-tenant defense → > 80% correto.
- Doc: `docs/internal/cas-read-path-architecture.md` (criar pre-merge).

## 28. Risk Register (HIGH_RISK = inclui detectability + exposure + residual)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|---|
| R-WI-001 | Cross-tenant read via AuthZ bypass | L | M | CRITICAL | M | LOW | Property test 100k + TLA+ + 5-layer defense + RB-FM-253 |
| R-WI-002 | Streaming memory leak Worker OOM | M | M | MEDIUM (FM-403) | M | LOW | Chunk 1 MiB bounded + memory test + trimestral restart |
| R-WI-003 | D1 AuthZ check overhead > 5ms p99 | M | L | LOW (DX) | L | LOW | UNIQUE index `(tenant_id, digest)` + criterion benchmark |
| R-WI-004 | REAPI ByteStream protocol edge cases | M | L | LOW | L | LOW | REAPI v2 conformance suite + reject invalid params |
| R-WI-005 | PAT auth stub diverge S-03 real | L | L | LOW | L | LOW | Stub interface frozen + integration test cobre transition |
| R-WI-006 | R2 SDK eventual consistency window | L | L | LOW | L | LOW | Strong consistency in R2 documented + retry policy |
| R-WI-007 | TLS 1.3 only enforcement em CF Edge | L | L | LOW | L | LOW | CF handles transparente + integration test cobre TLS path |

## 29. Review Checkpoints (HIGH_RISK = code + design + pre-merge + adversarial)

1. **Design review** (D+0 antes start): Architect + Security lead approve approach.
2. **Code review intermédio** (D+1): peer 1 review ST-001..005.
3. **Code review final** (D+3): peer 2 + Security lead approve full.
4. **Adversarial review** (pre-merge): pentest internal — cross-tenant attempts + property test inspection.
5. **Pre-merge gate**: 11 sign-offs canonical documented.

## 30. Sign-off (HIGH_RISK = 11 roles canonical; framework §33.5.4.3)

Sprint S-02 sign-off matrix (sprint.md §14). Para WI-S02-001 (foundation), exige toda a matriz:

| Role | Name | Signed (Y/N) | Date |
|---|---|---|---|
| Owner | Gustavo Schneiter | | |
| Final Approver | Gustavo Schneiter | | |
| SRE Lead | TBD | | |
| Security Lead | TBD | | |
| Engineer (S-02 lead) | TBD | | |
| QA Lead | TBD | | |
| Product | TBD | | |
| Compliance Officer | TBD | | |
| Privacy Officer | TBD | | |
| Architect | TBD | | |
| AppSec advisor | TBD | | |
| Peer reviewer 1 | TBD | | |
| Peer reviewer 2 | TBD | | |
| Crypto SME | TBD | | |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S02-001 — foundation WI sprint S-02. HIGH_RISK FF-HR-002 + FF-HR-005. Reusa S-01 corelink-tenant-path crate + blob_meta schema. |

## 32. Apêndice: Anti-patterns evitados

- ❌ R2 ACL como única defesa (insuficiente; bucket-level not object-level tenant scoping).
- ❌ Trust client-supplied tenant_id (sempre derive de PAT validated).
- ❌ Buffer full blob em memória antes streaming (memory unbounded; rejected; chunk 1 MiB obrigatório).
- ❌ R2 GetObject sem AuthZ check (CTRL-ISO-002 violation).
- ❌ Type confusion em TenantPrefix (S-01 newtype private field prevents).
- ❌ Audit emission opcional em rejected paths (CTRL-AUDIT-003 mandatory).
- ❌ Tombstone bypass via direct R2 read (handler é único path; S-06 GC alignment).
- ❌ HTTP byte ranges custom além de REAPI (anti-scope estrito).

---

**Fim WI-S02-001 — foundation WI sprint S-02.** Próximo: WI-S02-002 (GetBlob unary + FindMissingBlobs).
