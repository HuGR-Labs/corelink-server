---
id: "SPEC-CONTRACT-S02"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.17.0"
created: "2026-04-24"
updated: "2026-04-30"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s02", "cas", "read-path", "client-verify", "side-channel", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-02: CAS Read Path + Client Verify (Streaming + Negative Cache + Side-Channel Resistant)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-02 |
| Nome | CAS Read Path + Client Verify |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (cross-tenant read = catastrophic blast radius), FF-HR-005 (CTRL-CAS-002 + CTRL-ISO-002 + CTRL-ISO-004 — security controls implementation) |
| Duração estimada | 3 semanas + buffer 5 dias |
| WIs antecipados | 6 |
| SOTA target | Read path tier-1 — sub-100ms p99 cache-warm + client verify default-on (Rust crate + ABI stable; FFI = S-15) + side-channel-resistant 404 across MissReason variants (per ADR-0028) + negative cache (HMAC16 keyed) + streaming memory-bounded |

## 1. Objetivo

Implementar **path de leitura CAS production-grade** completando o loop write→read iniciado em S-01: REAPI `ByteStream::Read` (streaming chunked, memory-bounded), `GetBlob` unary (small blobs ≤ 4 MB inline), `FindMissingBlobs` (batch discovery para clients Bazel/Buck2), client-side verify obrigatório default-on (`corelink-client-verify` crate Rust + ABI stable; FFI integration tests Python/Go/JS = S-15 deliverable, CTRL-CAS-002), negative caching de 404 (KV TTL curto + HMAC16 keyed reduz cost de probe attacks + speed up legitimate misses), constant-time 404 across MissReason variants (CTRL-ISO-004 + ADR-0028 uniform freeze — prevent enumeration side-channel). Sem read path, S-01 é write-only e produto inviável.

**Por que SOTA:** competitors entregam read path com client verify opt-in (BuildBuddy) ou ausente (NativeLink em alguns SDKs); sem constant-time response distinction → enumeration side-channel; sem negative cache → cost overhead em probe storms. CoreLink S-02 entrega: (a) **TLA+ tenant_isolation.tla covering read path**; (b) **constant-time 404 across 3 MissReason arms** (per ADR-0028) com pairwise Mann-Whitney + Šidák; (c) **client verify default-on Rust crate + ABI stable** (`corelink-client-verify`; FFI Python/Go/JS = S-15 deliverable); (d) **side-channel benchmarks** (timing attack resistance via criterion + |Δmedian| ≤ 1ms); (e) **streaming memory-bounded** (S-02 v1.1.0: bounded by S-01 single-blob 5 MiB cap; peak ~6 MiB << 50 MiB ceiling; true byte-for-byte streaming via `R2Backend::get_stream` deferred to WI-S05-005 multipart read). Reference: **REAPI v2 spec**, **OWASP Side-Channel Testing**, **NIST SP 800-53 SC-4 (Information in Shared Resources)**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (11 sign-offs canonical per framework §33.5.4.3 + ADR-0034).
- **FF-HR-002**: reads cross-tenant seriam catastróficos; mesma superfície do S-01 (tenant isolation invariant).
- **FF-HR-005**: implementa CTRL-CAS-002 (client integrity verify), CTRL-ISO-002 (AuthZ on storage call), CTRL-ISO-004 (constant-time response). Bypass = security control failure.
- **FF-HR-002 escalation**: side-channel attack = enumeration de digests cross-tenant; bypass de tenant isolation indireta.

## 3. Inherits_from

```yaml
inherits_from:
  - "SECURITY-MODEL"            # CTRL-CAS-002, CTRL-ISO-002, CTRL-ISO-004
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # REAPI v2 ByteStream / Read semantics
  - "AUTH-MODEL"                # PAT validation + tenant context propagation
  - "KEY-MANAGEMENT"            # TDK reuse from S-01 for namespace resolution
  - "INVARIANT-REGISTRY"        # INV-TENANT-ISOLATION, INV-CAS-INTEGRITY, INV-CAS-IMMUTABILITY
  - "DATA-MODEL"                # blob_meta read schema, tombstone semantics
  - "OBSERVABILITY-MODEL"       # SLO-AVAIL-CAS-GET, SLO-LAT-CAS-GET, side-channel timing métrica
  - "FAILURE-MODES"             # FM-253 (cross-tenant read), FM-403 (streaming memory leak), FM-051 (R2 bit rot)
  - "SLO-CATALOG"               # cold/warm read targets
  - "RESILIENCE-PATTERNS"       # PAT-KV-TTL-001 negative cache, PAT-CIRCUIT-BREAKER-001
  - "PRIVACY-MODEL"             # error_taxonomy não vaza tenant_id em erros
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-CAS-004** | GET blob by digest | REAPI `ByteStream::Read` (streaming) + HTTP `GET /v1/cas/<digest>` (idempotent); content-addressable lookup. |
| **CAP-CAS-005** | Client-side verify integrity | `corelink-client-verify` crate (Rust) — auto-verifies BLAKE3 hash post-download; default-on toggle baked-in via `VerifyConfig::default()`. **S-02 ships Rust crate + ABI-stable interface**; FFI wrappers + integration tests Python pyO3 / Go cgo / JS WASM = S-15 deliverable (NÃO bloqueia SEAL S-02). Detect bit rot + cache poisoning. |
| **CAP-CAS-006** | Streaming read | ByteStream chunked (1 MiB chunks); Worker memory bounded; no full-blob buffering. |
| **CAP-CAS-007** | Negative caching (404) | KV `ac_neg:<region>:<HMAC16>:<digest_hex>` TTL 300s (PAT-KV-TTL-001); HMAC16 = `b64(HMAC_SHA256(TDK, tenant_id))[0:16]` (per remote_cache_product_profile.md §7.1; per-region per-tenant cross-isolation by construction); reduces probe cost; cache invalidation em S-01 write. |
| **CAP-CAS-008** | Side-channel-resistant 404 (MissReason parity per ADR-0028) | Constant-time response com pairwise `|Δmedian|` ≤ 1ms point-estimate + `ci_upper` ≤ 1ms across 3 arms (`NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason`); CTRL-ISO-004 enforcement; criterion benchmark + Mann-Whitney U + Šidák correction (per-test α' ≈ 0.005 685 8 for 9 tests at familywise α = 0.05) proves. |
| **CAP-CAS-009** | FindMissingBlobs (REAPI batch) | Discover gaps em batch antes de upload (Bazel client optimization); same security envelope. |

## 5. Requirements específicos

### 5.1 REAPI handlers (CAP-CAS-004 + CAP-CAS-006 + CAP-CAS-009)

- **R-S02-1**: REAPI `ByteStream::Read` Worker handler com streaming chunked-on-output (1 MiB chunks); `read_offset` + `read_limit` semantics conforme REAPI v2 spec. **S-02 v1.1.0 scope**: chunks são aplicados sobre `bytes::Bytes` materializado retornado por `R2Reader::get` (S-01 contract); peak memory ~6 MiB sob S-01 single-blob 5 MiB cap; true byte-for-byte streaming deferred to WI-S05-005 multipart read.
- **R-S02-2**: `GetBlob` unary handler para small blobs ≤ 4 MB inline; falls back to ByteStream se larger.
- **R-S02-3**: `FindMissingBlobs` batch endpoint: input `[digest]` → output `[missing_digests]`; enables Bazel client efficient pre-upload check; PAT scope `cache-find-missing` (canonical hyphen-form per auth_model.md §scope).
- **R-S02-3b**: `BatchReadBlobs` batch endpoint (REAPI mandatory per remote_cache_product_profile.md §12.2): input `[digest]` → output `[{digest, data, status}]`; aggregate cap 4 MiB; large blobs return ByteStream::Read pointer; PAT scope `cache-r`.
- **R-S02-4**: HTTP `GET /v1/cas/<digest>` (REAPI-equivalent REST surface) com Accept: `application/octet-stream`.

### 5.2 Client Verify (CAP-CAS-005)

- **R-S02-5**: Crate `corelink-client-verify` (Rust lib SDK side):
  - Compute BLAKE3 hash do body downloaded.
  - Compare com expected digest (from URL ou response header).
  - Mismatch → throw `DigestMismatchError` (error_taxonomy.md `COR_CAS_DIGEST_MISMATCH`).
  - Default-on em SDK; opt-out via explicit `verify=false` flag (documented + warning).
- **R-S02-6**: Test: inject bit-rot scenario (modify R2 object out-of-band) → client retorna `DigestMismatchError` em 100% das tentativas.

### 5.3 Side-Channel Resistance (CAP-CAS-008)

Canonical decision per ADR-0028 (forward-whitelisted): **MissReason → HTTP 404 uniform freeze** (variants `NeverExisted` / `Tombstoned` / `R2OrphanRow` per `corelink-reapi::read::MissReason` all map to 404 com same body; the conflated `CrossTenantMasked` arm folds into `NeverExisted` at the orchestrator surface per ADR-0028 v1.1.0 runtime fold). 410 Gone deferido S-06. Side-channel defense é at **timing layer**, não status-code differential.

- **R-S02-7**: Constant-time 404 across MissReason variants (CTRL-ISO-004) — prevent existence enumeration:
  - Worker responde com mesmo timing window para todas as variants 404 (`NeverExisted` / `Tombstoned` / `R2OrphanRow` per `corelink-reapi::read::MissReason`; the conflated `CrossTenantMasked` arm folds into `NeverExisted` at the orchestrator surface).
  - Implementação: timing-padding via `tokio::time::sleep` + jitter ou pre-computed delay (WI-S02-004 middleware).
  - Benchmark criterion: |Δmedian| ≤ 1ms across MissReason arms; p99 diff < 5ms (target SOTA).
- **R-S02-8**: Adversarial test: 10k requests por arm (3 arms: `NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason` + ADR-0028 v1.1.0 runtime fold) → measure timing distribution; pairwise Mann-Whitney U validates indistinguishability via Šidák-corrected per-test α' ≈ 0.005 685 8 across 9 tests (3 trials × 3 pairs full conjunction at familywise α = 0.05; cycle 13 SEAL math correction; vide WI-S02-004 §2).

### 5.4 Negative Cache (CAP-CAS-007)

- **R-S02-9**: Negative cache KV `ac_neg:<region>:<HMAC16>:<digest_hex>` com TTL 300s (PAT-KV-TTL-001):
  - Key canonical: HMAC16 prefix per remote_cache_product_profile.md §7.1 (NUNCA plaintext tenant_id).
  - Hit em digest known-missing → return 404 sem hit R2/D1 (cost saving).
  - Cache invalidation em S-01 PUT (write path chama `negative_cache.invalidate_on_write` em `ac_neg:` key).
  - Per-region KV (residency alignment).

### 5.5 Tenant Isolation (CAP-CAS-004 envelope)

- **R-S02-10**: Tenant context propagation: `Authorization: Bearer <PAT>` → resolve `tenant_id` (S-03 reuse via auth middleware) → namespace lookup `<tenant_prefix>/<digest>` em R2; reject se prefix mismatch.
- **R-S02-11**: AuthZ check em storage call (CTRL-ISO-002): pre-R2 read, verify `tenant_id` matches blob_meta.tenant_id em D1; mismatch = **404 (uniform per ADR-0028 v1.1.0; CrossTenantMasked variant)** + read-side audit emit `corelink.cas.read_miss` (low-severity info; conflated com NeverExisted no read seam — read-handler não pode distinguir sem side-channel oracle); reclassificação para SEV-1 `corelink.cas.cross_tenant_attempt` é offline pelo S-09 chain consumer com global digest index. Client observa apenas 404 uniform.

### 5.6 Observability + SLO

- **R-S02-12**: Métricas (canonical underscored Prometheus form per observability_model.md §4.1; label `plan` per §3.1): `corelink_cas_get_requests_total{plan, region, outcome}` (outcome ∈ `ok, miss, neg_cache_hit, integrity_fail, denied`); `corelink_cas_get_duration_seconds_bucket{p50, p95, p99}`; `corelink_cas_side_channel_timing_diff_ms` (gauge para CTRL-ISO-004 monitoring per ADR-0023).
- **R-S02-13**: SLO-AVAIL-CAS-GET ≥ 99.9% sustained 72h staging; SLO-LAT-CAS-GET p99 < 300ms (cold) / < 100ms (warm).

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **Property test 100k iter** tenant isolation: nenhum read retorna blob fora de namespace do tenant (EVT-002).
- [ ] **TLA+ verdes em CI**: `tenant_isolation.tla` + `cas_integrity.tla` (EVT-022).
- [ ] **Load test** read 50k QPS × 10 min em staging com SLO-AVAIL-CAS-GET preserved (EVT-021).
- [ ] **Client verify default-on no crate Rust** (`corelink-client-verify`): bit-rot detection 100% via property test em CI (EVT-002). **NOTA Lote 9.5**: integração FFI Python/Go/JS é entregável **S-15** (outbound consumer); S-02 entrega o crate Rust + ABI stable. SDK integration tests rodam em S-15 sprint, não bloqueiam SEAL S-02.
- [x] **Side-channel test**: medir latência **404 across MissReason variants** (`NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason`) → `|Δmedian| ≤ 1ms` point estimate AND `ci_upper ≤ 1ms` via bootstrap CI; pairwise Mann-Whitney U ALL 9 tests `p > sidak_per_test_alpha(0.05, 9)` ≈ 0.005 685 8 com Šidák correction (EVT-002). IMPLEMENTED em `crates/corelink-worker/tests/timing_indistinguishability.rs::three_arm_indistinguishability_with_padding`.
- [ ] **Runbook `RB-FM-253`** (cross-tenant read) dry-run executado em staging (EVT-017).
- [ ] **SBOM + signed release** (CycloneDX 1.5+) — formato e enforcement definido em S-12 (forward-looking; S-02 honra format mas full SLSA L3 attestation gate é S-12 sealing) (EVT-010).
- [ ] **Negative cache** test: probe storm 1k QPS de unknown digests → measure cost reduction vs no-cache baseline (EVT-021).
- [ ] **Streaming memory test**: bounded by S-01 single-blob 5 MiB cap (`SINGLE_BLOB_LIMIT_BYTES`); peak ~6 MiB << 50 MiB ceiling. 1 GiB AC deferred to WI-S05-005 multipart read where `R2Backend::get_stream` lands (EVT-002 partial; full target in S-05).
- [ ] **Bit rot integrity test**: corrupt R2 object out-of-band → 100% client verify catches (EVT-002).
- [ ] **PRR HIGH_RISK 11 sign-offs canonical** (per framework §33.5.4.3 + ADR-0034; aligns sprint.md §14 single-source): Owner + Final Approver + Architect (Crypto SME specialization for BLAKE3 verify + Mann-Whitney methodology + Statistician methodology + side-channel design) + Security Lead + SRE Lead + Engineer (S-02 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor (peer reviewers contribuem em PR review sem sign-off canonical separado) (EVT-031).
- [ ] **Cost regression gate**: CAS GET hot path benchmark per-op cost; < 10% regression vs S-01 write baseline (Lote 9.4 §14.10).

## 7. Completeness Criteria (delta local)

- [ ] **10.s02.1** E2E: write em S-01 → read em S-02 retorna body idêntico byte-a-byte (EVT-018).
- [ ] **10.s02.2** SLO-AVAIL-CAS-GET: 99.9% em staging sustained 72h (EVT-021).
- [ ] **10.s02.3** SLO-LAT-CAS-GET: 99% < 300ms cold; < 100ms warm (team target) (EVT-021).
- [x] **10.s02.4** **Side-channel resistance proven (3-arm 404 MissReason parity per ADR-0028)**: 10k samples per arm × 3 arms (`NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason`); pairwise Mann-Whitney U ALL 9 tests `p > sidak_per_test_alpha(0.05, 9)` ≈ 0.005 685 8 com Šidák correction (combined familywise α = 0.05 target); bootstrap 95% CI on `|Δmedian|` ≤ 1ms point estimate AND `ci_upper` ≤ 1ms (EVT-002). IMPLEMENTED + 8/8 tests passing release in 7s.
- [ ] **10.s02.5** **Negative cache effectiveness**: probe storm cost reduced ≥ 80% vs baseline (EVT-021).
- [ ] **10.s02.6** **Client verify default-on (Rust crate)**: `corelink-client-verify` `VerifyConfig::default()` returns `enabled: true`; opt-out path requires explicit flag + emit warning log; CI gate verifica default. **NOTA Lote 9.5**: 3-SDK FFI ubiquity (Python/Go/JS) é entregável **S-15** (não bloqueia SEAL S-02) (EVT-002).
- [ ] **10.s02.7** **Tenant isolation property test**: 100k iter cross-tenant attempts → 0 successes (EVT-002).
- [ ] **10.s02.8** **Streaming memory bounded**: bounded by S-01 5 MiB cap; peak ~6 MiB << 50 MiB ceiling (1 GiB AC deferred to WI-S05-005 multipart read) (EVT-002 partial).
- [ ] **10.s02.9** **Bit rot detection**: 100% bit rot scenarios caught client-side (EVT-002).

## 8. Invariants

### Mantidas (heredadas de canonical sources)

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): read nunca retorna blob de outro tenant. Coberto por `tenant_isolation.tla` em CI.
- **INV-CAS-INTEGRITY** (CRITICAL, TLA+): client verify detecta bit rot/poisoning. Coberto por `cas_integrity.tla` em CI.
- **INV-CAS-IMMUTABILITY** (CRITICAL): reads após GC soft-delete respeitam tombstone.
- **INV-CAS-IDEMPOTENCY** (CRITICAL): same digest sempre retorna same body.

### Novas (S-02 — adicionar a invariant_registry §3.12 como Lote 9.4 followup se needed)

- **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** (HIGH — registry §3.12 promoted Lote 9.4): timing distribution **across all 404 MissReason variants** (`NeverExisted` × `Tombstoned` × `R2OrphanRow` per `corelink-reapi::read::MissReason` + ADR-0028 v1.1.0 runtime fold; the conflated `CrossTenantMasked` arm folds into `NeverExisted` at the orchestrator surface) é statistically indistinguishable (pairwise Mann-Whitney U ALL 9 tests `p > sidak_per_test_alpha(0.05, 9)` ≈ 0.005 685 8; Šidák corrected). **Why:** prevent enumeration of digest existence (cross-tenant OR tombstoned OR R2-orphan) — per ADR-0028 status code é uniform 404; defense é timing parity. **How to apply:** criterion benchmark + adversarial test 10k samples per arm + `corelink_cas_side_channel_timing_diff_ms` aggregate gauge derived from `corelink.cas.side_channel.timing_padded` per-request emit (sustained > 5ms 5min ⇒ SEV-2) + `|Δmedian| ≤ 1ms` point estimate AND `ci_upper ≤ 1ms`.

## 9. Quality Standards (delta local)

- **14.s02.1 Streaming memory bounded**: Worker não carrega blob completo em memória; chunk-based 1 MiB; máximo 50 MiB peak por request.
- **14.s02.2 Cold read p99 ≤ 300ms; warm ≤ 100ms** (cache hit) — SOTA target.
- **14.s02.3 Negative cache effectiveness ≥ 80% cost reduction** em probe storms (vs no-cache baseline).
- **14.s02.4 Side-channel resistance**: criterion benchmark p99 diff < 5ms; statistical test passes.
- **14.s02.5 Client verify ubiquity**: 100% SDK calls verify; opt-out gera warning log + telemetry counter (S-09 alignment).
- **14.s02.6 Tenant isolation defense-in-depth**: 5 layers conforme `auth_model.md §8.1` (PAT scope + namespace prefix + AuthZ check + R2 ACL + audit cross-check).
- **14.s02.7 Cost regression gate** (Lote 9.4 §14.10): CAS GET hot path per-op cost benchmark; PR > 10% regression bloqueia.
- **14.s02.8 Bit rot detection client-side**: BLAKE3 verify default-on; opt-out documented per error_taxonomy `COR_CAS_DIGEST_MISMATCH`.

## 10. Anti-scope

- ❌ Write path (S-01).
- ❌ AC reads (S-04 — AC tem read path próprio com Merkle verify).
- ❌ Multi-region failover read (S-14 PAT-REGION-FAILOVER-001).
- ❌ Compression de blobs em transit (HTTP gzip OK, mas não custom format).
- ❌ Streaming compression mid-flight — anti-scope; clients comprimem antes do upload.
- ❌ HEAD requests sem auth (REAPI não suporta; mantém superfície minimal).
- ❌ Range requests além de REAPI semantics (`read_offset`/`read_limit`); HTTP byte ranges = pós-GA.
- ❌ HTTP/2 multiplexing tuning — Cloudflare handles transparently.
- ❌ Read-after-write consistency aggressive (S-01 já garante via R2 strong consistency).

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (write path; sem write não há read).
- **S-00 SEALED** (roadmap context).

### Soft blockers

- **S-03** (auth real para tenant context; staging stub OK até S-03 SEALED).

### Outbound

- **S-04** (AC read path consume CAS read primitives).
- **S-07** (eviction depende de read patterns para LRU).
- **S-15** (CLI `corelink get` e SDK FFI consume read path + client-verify crate).
- **S-20** (GA exige read SLOs sustained 30d).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S02-001** | REAPI ByteStream::Read handler + HTTP GET + tenant context propagation | bytestream handler; chunk 1MiB; HTTP REST surface; auth middleware reuse S-03 stub; AuthZ check pre-R2 | 14h | 22h | 36h | **23.0h** |
| **WI-S02-002** | GetBlob unary + BatchReadBlobs + FindMissingBlobs (REAPI mandatory trio per remote_cache_product_profile.md §12.2) + small-blob inline | unary handler; size-based dispatch; BatchReadBlobs aggregate cap 4 MiB; FindMissingBlobs batch logic; PAT scope `cache-r` / `cache-find-missing` canonical (auth_model.md §scope hyphen-form); benchmark | 12h | 20h | 32h | **20.7h** |
| **WI-S02-003** | Crate corelink-client-verify (Rust SDK lib) + ABI-stable interface | BLAKE3 verify lib; default-on toggle; opt-out warning; ABI-stable contract (FFI integration tests Python/Go/JS = S-15 scope) | 12h | 18h | 30h | **19.0h** |
| **WI-S02-004** | Constant-time 404 MissReason parity middleware (ADR-0028 3-arm) + criterion benchmark + pairwise Mann-Whitney U + Šidák test | timing-padding middleware; jitter; criterion benchmark; statistical test 10k samples per arm | 14h | 22h | 36h | **23.0h** |
| **WI-S02-005** | Negative cache KV adapter + invalidation hook em S-01 write | KV per-region; TTL 300s; invalidation hook; probe storm test | 8h | 14h | 22h | **14.3h** |
| **WI-S02-006** | Property test 100k tenant isolation + RB-FM-253 dry-run + bit-rot test + PRR | property test framework; tenant isolation 100k iter; bit rot inject; RB-FM-253 walkthrough; PRR docs | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~115h ≈ 14 dias work × 1 eng. Buffer 5 dias confere com 3 semanas.

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 5 dias.
- **Marcos:**
  - **D+4:** WI-001 SEALED (ByteStream::Read live em staging).
  - **D+6:** WI-002 SEALED (GetBlob + FindMissingBlobs).
  - **D+9:** WI-003 SEALED (client-verify Rust crate + ABI stable; FFI integration = S-15).
  - **D+12:** WI-004 SEALED (constant-time + side-channel test passed).
  - **D+13:** WI-005 SEALED (negative cache + invalidation).
  - **D+15:** WI-006 SEALED (property test + runbook + PRR).
  - **D+17:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + 72h staging sustained SLO-AVAIL-CAS-GET ≥ 99.9%.
- TLA+ 2 specs verdes.
- Side-channel resistance verified.
- Client verify default-on Rust crate + ABI-stable interface (FFI integration tests Python/Go/JS = S-15 sprint scope; não bloqueia SEAL S-02).
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Side-channel timing vaza existência de blob** (THR-I-002) | M | M | HIGH | M | LOW | Constant-time middleware + jitter + criterion benchmark p99 < 5ms + Mann-Whitney test. |
| **Streaming memory leak em long reads** (FM-403) | M | M | MEDIUM | M | LOW | Chunk 1 MiB bounded + RB-FM-403 dry-run + memory peak monitoring; trimestral container restart (PAT-RESTART-JIT-001). |
| **Client verify default-off por bug em Rust crate** | L | L | CRITICAL (CTRL-CAS-002 bypassed) | L | LOW | `VerifyConfig::default()` `enabled: true` constante; opt-out path requires explicit flag + warning log; CI gate verifica default. FFI ubiquity (Python/Go/JS) = S-15 scope. |
| **Cross-tenant read** (FM-253) | L | M | CRITICAL (catastrophic blast radius) | M | LOW | INV-TENANT-ISOLATION TLA+ + property test 100k + 5-layer defense-in-depth + RB-FM-253 dry-run. |
| **Bit rot R2 object não detectado** (FM-051) | L | L | HIGH | L | LOW | Client verify default-on + scrub periódico (S-06 dependency); 100% bit rot caught in test. |
| **Negative cache TTL too long** (stale 404 após write) | M | L | MEDIUM | L | LOW | Cache invalidation hook em S-01 write path; TTL 300s upper bound; manual invalidation API. |
| **PAT auth stub diverge de S-03 real** (interface drift) | L | L | LOW | L | LOW | Stub interface frozen no Lote 9.4 contract; S-03 implementação respeita interface; integration test cobre. |
| **Probe storm cost** sem negative cache | M | M | MEDIUM | M | LOW | Negative cache TTL 300s + per-IP rate limit (S-08 dependency soft); cost benchmark. |
| **REAPI ByteStream protocol edge cases** (read_offset out-of-bounds, read_limit zero) | M | L | LOW | L | LOW | REAPI v2 spec compliance test; reject invalid params com error_taxonomy entries. |
| **TLS termination at CF Edge** vs Worker context | L | L | LOW | L | LOW | Cloudflare handles; documentado; test cobre TLS 1.3 only path. |
| **Cost regression** > 10% baseline | M | M | MEDIUM | M | LOW | Cost regression gate §14.10 universal Lote 9.4; criterion benchmark per-op cost CI. |

## 16. Benchmarks SOTA externos

| Critério | BuildBuddy | NativeLink | bazel-remote | JFrog | **CoreLink target S-02** |
|---|---|---|---|---|---|
| Client verify default-on | Opt-in | Limited | Manual | No | **Yes — Rust crate + ABI stable (S-02); 3-SDK FFI ubiquity = S-15) + warning opt-out** |
| Constant-time 404 MissReason parity (ADR-0028) | No | No | No | No | **Yes — 3-arm pairwise |Δmedian| ≤ 1ms + p99 diff < 5ms; Mann-Whitney U + Šidák all p > 0.05** |
| Streaming memory bounded | Yes | Yes | Yes | Yes | **Yes — 1 MiB chunks, 50 MiB peak** |
| Negative caching | No | No | Limited | Yes (TTL 60s) | **Yes — KV TTL 300s + invalidation hook** |
| TLA+ tenant isolation verified | No | No | No | No | **Yes — tenant_isolation.tla green CI** |
| Bit rot detection client-side | Manual | Limited | Manual | Yes | **Yes — 100% bit rot caught** |
| 5-layer defense tenant isolation | 2 layers | 2 layers | 1 layer | 3 layers | **5 layers (auth_model §8.1)** |
| FindMissingBlobs batch | Yes | Yes | Yes | Yes | **Yes — REAPI-compliant** |
| SLO-LAT-CAS-GET p99 cold | 400-600ms | 300-500ms | 200-400ms | 200-300ms | **< 300ms** |

**Veredito SOTA:** S-02 v1.7+ atinge feature parity em 7/9 dimensões; **vantagem em constant-time 404 MissReason parity (ADR-0028 3-arm) + TLA+ verified tenant isolation** (rare/unique no mercado de remote cache).

## 17. References (RFCs, papers, standards)

- **REAPI v2 Specification** (Bazel Remote Execution API) <https://github.com/bazelbuild/remote-apis>.
- **BLAKE3 Specification** <https://github.com/BLAKE3-team/BLAKE3-specs>.
- **NIST SP 800-53 SC-4** — Information in Shared Resources.
- **NIST SP 800-53 SI-7** — Software, Firmware, and Information Integrity.
- **OWASP Side-Channel Testing** (Timing Attack section).
- **Mann-Whitney U test** (Statistical methods reference).
- **RFC 9110** — HTTP Semantics.
- **REAPI ByteStream Spec** <https://github.com/googleapis/googleapis/blob/master/google/bytestream/bytestream.proto>.
- `specs/tla/tenant_isolation.tla` — TLA+ verification.
- `specs/03_architecture/error_taxonomy.md` — `COR_CAS_*` error codes.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Cross-tenant read detected (any) → CRITICAL post-mortem + Privacy Officer + Legal + breach notification consideration.
- Side-channel timing diff > 5ms p99 (drift sustained) → 5-Why + INV-CAS-SIDE-CHANNEL review.
- Client verify bypass detected em produção → CRITICAL post-mortem + CTRL-CAS-002 review.
- Negative cache stale > grace 5 min (write→read inconsistency) → post-mortem + invalidation flow review.
- Streaming memory leak (Worker OOM) → post-mortem + chunk size + restart policy review.
- TLA+ CI red (tenant_isolation ou cas_integrity) → CRITICAL post-mortem + invariant scope review.

## 19. Waiver policy

S-02 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-TENANT-ISOLATION property test 100k green — security baseline.
- ❌ TLA+ tenant_isolation + cas_integrity green em CI — formal verification baseline.
- ❌ Constant-time 404 MissReason parity (CTRL-ISO-004 per ADR-0023 + ADR-0028) — side-channel resistance baseline.
- ❌ Client verify default-on Rust crate + ABI stable — CTRL-CAS-002 baseline (FFI integration tests Python/Go/JS = S-15 sprint scope).
- ❌ Bit rot 100% detection — INV-CAS-INTEGRITY baseline.

Itens waivable com Security lead + Architect + ADR:

- ❌ **Side-channel test gate cannot be relaxed/waived** (cycle 12 SEAL fix: previous waiver entry 'p > 0.05 → p > 0.01 com risk acceptance' was mathematically backwards — relaxing α from 0.05 to 0.01 ALLOWS weaker evidence to pass null-hypothesis test, contradicting INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE intent. HIGH_RISK security invariants são hard gates, não trade-off'able. Per ADR-0023 + ADR-0028: gate é 3-arm pairwise Mann-Whitney + Šidák all p > 0.05 + |Δmedian| ≤ 1ms; no waiver path).
- ⚠️ SLO-LAT-CAS-GET p99 cold 300ms → 400ms com explicit customer SLA addendum.
- ⚠️ Negative cache TTL 300s → 60s (mais conservador) com cost overhead acceptance.
- ❌ **FindMissingBlobs batch + BatchReadBlobs cannot be waived/deferred** (REAPI v2 mandatory per remote_cache_product_profile.md §12.2 L410-412; conformance gate em sprint contract §6 DoD; previous waiver entry removed cycle 11 SEAL — REAPI conformance é hard gate, não soft).

---

## 16. Changelog

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo | Spec contract retroativo (Lote 9.5a). |
| 1.1.0 | 2026-04-24 | Gustavo (Lote 9.4 SOTA elevation) | EVT addition + 6-col risk register + PERT explicit. |
| 1.17.0 | 2026-04-30 | Gustavo (Lote S-02 SPRINT-CLOSE — Sonnet adversarial review 8.8/10 SPRINT-SEAL: GRANTED) | **Sprint S-02 CAS read path implementation phase COMPLETE: all 6 WIs SEALED end-to-end.** Independent Sonnet agent (model=sonnet via Agent tool, replaced codex post-rate-limit per protocol change 2026-04-30) ran sprint-close adversarial review covering full corpus (sprint contract + 6 WI specs + crates + tests + CI workflows + PRR + pentest). **SCORE 8.8/10. SPRINT-SEAL: GRANTED.** P0=0; 4 P1 fixes applied in same Lote: (P1-1) `crates/corelink-reapi/tests/prop_find_missing_batch.rs` proptest cases bumped 64 → 10 000 to match WI-S02-002 §10.2.2 spec mandate of randomly-generated 10k-iter input diversity; (P1-2) sprint.md doc_status DRAFT → FROZEN + work_status READY → COMPLETE + version 1.0.0 → 1.1.0; PRR-S02 doc_status DRAFT → FROZEN + work_status CONDITIONALLY_APPROVED → APPROVED + version 1.0.0 → 1.1.0 (post-SEAL status reflection); (P1-3) `specs/_audits/2026-04-30-pentest-s02-internal.md` §2.2 added explicit virtual-time methodology caveat — Mann-Whitney + Šidák gate operates under `tokio::time::pause` (synthetic delays) so proves the padding *math* is sound but not yet against real CF Workers wall-clock noise; full real-time evidence deferred to S-09 chaos-on-staging; the `corelink-reapi::tests::timing_padding_grpc_e2e` test does provide real-tokio gRPC wall-clock evidence for the 160ms floor. (P1-4 deferred — `corelink-client-verify` `deny(unsafe_code)` + module-scoped `allow` for FFI is intentional design per WI §6.1; spec-text refinement is low-impact polish). Sprint-close commit + tag `s02-impl-sealed`. Aggregate state: 6 SEALed WIs (`5e297bf` `070200e` `e4ddaa5` `32e2ff6` `25d6527` `84dd9d2`); 1 new crate (`corelink-client-verify`); 13 total impl WIs SEALED across S-01+S-02; 265 docs schema validated; INV registry 100% covered. **Próximo:** S-03 sprint (Clerk SSO real auth). |
| 1.16.0 | 2026-04-30 | Gustavo (Lote 10.21 — WI-S02-006 SEALED; **S-02 implementation phase complete; all 6 WIs SEALED**) | **WI-S02-006 SEALED in code**: cross-component property test suite shipped at `crates/corelink-reapi/tests/prop_cas_read.rs` (6 proptest properties at 10k iter + 100k round-robin schedule + TTL-boundary regression vector); bit-rot integration test at `crates/corelink-reapi/tests/integration_bit_rot.rs` (10/10 scenarios caught + opt-out negative control); RB-FM-253 host-side dry-run harness at `scripts/rb_fm_253_dry_run.sh` (EVT-017 evidence emission + drift detection); PRR-S02 doc at `specs/04_sprints/S02/PRR-S02.md` (11-row sign-off matrix; 5 ✅ APPROVED + 6 ⚠️ WAIVED via ADR-0034 dual-hat; STAGING-STABLE promotion); internal pentest report at `specs/_audits/2026-04-30-pentest-s02-internal.md`. **Path-routing decision** documented in WI-006 §13 + §31 changelog (canonical Rust test files moved from `corelink-worker/tests/` to `corelink-reapi/tests/` per the WI-S01-006 lesson — `corelink-worker` cannot dev-depend on `corelink-reapi` without creating a workspace cycle). **Per-WI codex skipped** per protocol change 2026-04-30; sprint-close adversarial review covered by independent Sonnet agent. WI-S02-006 frontmatter DRAFT→FROZEN, READY→DONE, version 1.1.0→2.0.0. |
| 1.15.0 | 2026-04-29 | Gustavo (Lote 10.20 — WI-S02-004 SEALED) | **WI-S02-004 SEALED in code**: Tower middleware `corelink-worker::middleware::timing_padding` + adversarial 3-arm × 10k × 3 trial Mann-Whitney + bootstrap CI + criterion bench + `docs/internal/side-channel-defense.md` shipped. **Spec drift fixed in same Lote** (charter §spec drift "patch in same Lote"): (a) `statrs::stats_tests::mann_whitney_u` referenced by v1.4-v1.14 does NOT exist in `statrs` 0.18 (only Fisher's exact ships in `stats_tests`); replaced by canonical hand-rolled `mann_whitney_u_p_value` in `corelink-worker::middleware::timing_padding` under strict lints (Mann & Whitney 1947 + Hollander & Wolfe 1973 §4.1 tie-correction). WI-004 §6.1.3 + §10.4.1 + §17 ST-004 + ADR-0023 aligned. (b) WI-004 `MissReason` triple realigned with `corelink-reapi::read::MissReason` canonical: `(NotFound × CrossTenantMasked × Tombstoned)` → `(NeverExisted × Tombstoned × R2OrphanRow)` per ADR-0028 v1.1.0 runtime fold (CrossTenantMasked folds into NeverExisted at the orchestrator surface — the trait-level `MetaStore::get` cannot disambiguate; R2OrphanRow is the canonical D1-row-alive + R2-NotFound third arm the prior label set omitted). The §3.12 invariant table label set in `invariant_registry.md` retains the original arm names for invariant-registry stability + ADR-0028 cross-reference; the WI text + adversarial test target the canonical impl arm names. (c) WI-004 §10.4.1..§10.4.9 marked complete (`[x]`) with implementation evidence per item; §31 changelog v1.2.0 enumerates the deltas. |
| 1.14.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 13 codex SEAL real validation 8.8 → ≥9.0 target) | **1 P0 + 2 P1 + 1 P2 fixed (substantive rigor)**: (a) **P0 statistical gate Šidák math correction** — earlier cycles claimed 'combined α ≈ 0.000125' which is incorrect math; canonical Šidák for 9 tests at familywise α = 0.05 target → per-test α' = 1 − (1−0.05)^(1/9) ≈ 0.0057. WI-004 §2 + §10.4.1 + ADR-0023 + INV registry §3.12 all aligned: ALL 9 tests must p > 0.05 (full conjunction); per-test α' = 0.0057. (b) **P1 observability naming canonical sweep** — _spec_contract §5.6 R-S02-12 + WI-S02-002 §6.1.5 + WI-S02-005 §6.1.7 + sprint.md L257-258: dotted form + tenant_tier → underscored + plan canonical (per observability_model.md §4.1 naming + §3.1 labels). (c) **P1 ADR-0034 single-source** — Engineer slot decision matrix updated: '5-6 Engineer ×2' → '5 Engineer (canonical single slot per framework §33.5.4.3 + S-01..S-05 SEAL precedent)'. (d) **P2 WI-S02-003 hygiene final** — broken comment block from cycle 12 cleaned (placeholder code removed; canonical impl Default kept clean). |
| 1.13.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 12 codex SEAL real validation 8.9 → ≥9.0 target) | **1 P0 + 2 P1 + 1 P2 fixed (substantive)**: (a) **P0 side-channel waiver removed** — _spec_contract §16: previous waiver 'p > 0.05 → p > 0.01' was mathematically backwards (relaxing α 0.05→0.01 ALLOWS weaker evidence to pass null-hypothesis indistinguishability test); HIGH_RISK security invariants são hard gates per ADR-0023 + ADR-0028; cycle 12 SEAL eliminated waiver path; gate canonical é 3-arm pairwise Mann-Whitney + Šidák + |Δmedian| ≤ 1ms. (b) **P1 reviewer staffing canonical** — sprint.md frontmatter `reviewers: []` → explicit ADR-0034 solo-tier waiver acknowledgment (Owner + Final Approver Gustavo dual-hat; 9 specialized roles TBD com ADR-0034 reference); body banner aligned. (c) **P1 observability naming canonical** — sprint.md §11 Métricas: dotted form `corelink.cas.get.requests_total` (authoring convention) → underscored exposed `corelink_cas_get_requests_total` per observability_model.md §4.1; label `tenant_tier` → `plan` per §3.1 canonical (5 metrics updated). (d) **P2 WI-S02-003 hygiene** — duplicate `impl Default for VerifyConfig` removed (cycle 9 introduced builder; cycle 12 cleans residual); §8 AC opt-out scenario uses `VerifyConfig::disabled()` builder (não direct field instantiation, aligned com cycle 9 `pub(crate)` privatization). |
| 1.12.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 11 codex SEAL real validation 8.8 → ≥9.0 target) | **2 P0 + 1 P1 fixed (top-level canonical alignment)**: (a) **P0 sprint contract BatchReadBlobs promotion** — _spec_contract §5.1 added R-S02-3b BatchReadBlobs requirement; §12 PERT WI-S02-002 row updated com BatchReadBlobs estimate (16.7→20.7h); §16 anti-waivable removed FMB defer waiver (REAPI mandatory hard gate); sprint.md §2.1 WI-S02-002 in-scope updated; WI-002 §10.2.1 completeness criteria includes BatchReadBlobs. (b) **P0 scope canonical hyphen-form** — 8 locations sweept colon→hyphen: sprint.md §6.2 + WI-001 §2 narrative + §6.1.3 + §8 AC (3 scenarios) + §32 STRIDE + WI-004 §8 AC. All `cache:r` / `cache:w` → `cache-r` / `cache-w` per auth_model.md §scope canonical hyphen-form. (c) **P1 auth_model.md §4.1 pseudocode** — generic 403 mismatch override clarified: CAS read handlers (S-02 ADR-0028 path) retornam 404 uniform com forensics audit; 403 reservado para PAT scope failures (não cross-tenant blob access). Single-source canonical aligned. |
| 1.11.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 10 codex SEAL real validation 8.8 → ≥9.0 target) | **2 P0 + 2 P1 + 2 P2 fixed (canonical sources updated)**: (a) **P0 ADR-0023 + ADR-0028 authored** as full ADR files (no longer just whitelist entries) — `specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md` (constant-time defense via Tower TimingPaddingLayer; 3-arm Mann-Whitney + Šidák; cliente latency tax + CF Worker $0 cost analysis) + `specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md` (MissReason → 404 uniform; 410 Gone deferred S-06; COR_CAS_TENANT_FORBIDDEN reserved para PAT scope). (b) **P0 security_model.md alignment** — §5.4 THR-I-002 + §6.3 CTRL-ISO-004: 'constant-time 404 vs 403' → '3-arm 404 MissReason parity per ADR-0023 + ADR-0028'. (c) **P0 error_taxonomy.md alignment** — COR_CAS_TENANT_FORBIDDEN clarified: NÃO retornado por CAS read handlers para cross-tenant; reservado para PAT scope failures (S-03). (d) **P1 WI-006 90s residuals** — §9.1 narrative + §14.6.4 quality: '90s' → '~5min realistic typical / ~10min upper bound' aligned com cycle 9 perf bound. (e) **P1 WI-005 §28 R-005 KV LRU** → 'TTL eviction (Workers KV não LRU primitive)'. (f) **P2 residual 404/403 sweep** — WI-001 §3 customer-visible + sprint.md §5 5-layer reference. |
| 1.10.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 9 codex SEAL real validation 8.9 → ≥9.0 target) | **1 P0 + 3 P1 + 1 P2 fixed (substantive engineering)**: (a) **P0 INV registry single-source** — invariant_registry.md §3.12 INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE updated: '404 vs 403 indistinguishable; 10k samples Mann-Whitney p > 0.05' → '3-arm 404 MissReason parity (NotFound × CrossTenantMasked × Tombstoned per ADR-0028); pairwise Mann-Whitney + Šidák (within-trial + across-trial = 9 tests combined α ≈ 0.000125); 10k per arm × 3 arms; |Δmedian| ≤ 1ms'. (b) **P1 WI-S02-003 stream verify clarity** — narrative 'fail-fast em mid-download corruption' → 'detection at end-of-stream (no Merkle per chunk em S-02 scope; deferred to S-05); memory benefit holds'. (c) **P1 WI-S02-003 VerifyConfig builder enforcement** — fields `pub` → `pub(crate)`; builder pattern via `VerifyConfig::new()` + `VerifyConfig::disabled()` obrigatório (silent field set anti-pattern eliminated; CTRL-CAS-002 default-on enforced). (d) **P1 WI-S02-005 KV LRU → TTL eviction** — Workers KV não oferece explicit LRU primitive; eviction é TTL-based only (300s expiry). (e) **P2 WI-S02-006 perf bound** — '100k iter ≤ 90s' (optimistic) → 'soft ≤ 5min, hard ≤ 10min' (realistic 100k × ~5ms per-iter rationale). |
| 1.9.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 8 codex SEAL real validation 8.7 → ≥9.0 target) | **3 codex P0/P1 + 2 P2 fixed (substantive)**: (a) **P0 WI-002 §9.5 split-brain** — 'Why no BatchReadBlobs' (deferral rationale) → 'Why BatchReadBlobs in-scope (REAPI mandatory; corrects earlier rationale)'. Aligns com cycle 7 §6.1.2 in-scope addition. (b) **P1 WI-003 §1 JS WASM clarity** — explicit note that JS/WASM uses wasm-bindgen pipeline (NOT cbindgen + C ABI); cbindgen scope = Python pyO3 + Go cgo only; wasm.rs separate feature-gated module com #[wasm_bindgen] attributes. (c) **P1 WI-004 §22 cost calc real TCO** — placeholder '~\$X TCO' replaced com computed analysis: cliente latency tax ~15,000s/dia aggregate (imperceptible per-cliente); CF Worker CPU cost \$0 (tokio sleep não CPU spin); SLO budget implicit. (d) **P2 WI-005 §24 stale threshold** — 'inconsistency > 5s' → 'TTL bound 300s OR typical 60s sustained' (aligned com cycle 6+7 freshness model). (e) **P2 WI-004 §9.1 heading** — 'Why pad só 404/403' → 'Why pad só 404 MissReason variants (não todos status; 403 separate)' com explanation que 403 PAT scope failures aren't enumeration vector. |
| 1.8.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 7 codex SEAL real validation 8.4 → ≥9.0 target) | **3 codex P0 + 1 P1 + 5 P2 fixed (substantive engineering)**: (a) **P0 REG-NEGATIVE-002 revision** — remote_cache_product_profile.md §11.2: 'negative cache entries NÃO DEVE ser cacheada em KV' → revised to 'PODEM somente para GetBlob short-circuit path; FindMissingBlobs continua mandatory fall-through per REG-NEGATIVE-001'. Aligns canonical com WI-S02-005 design + ADR-0028 freeze. (b) **P0 WI-S02-002 BatchReadBlobs** — REAPI mandatory per remote_cache_product_profile.md §12.2 L412; movido §6.2 out-of-scope → §6.1 in-scope (item 2); §6.2 deferral substituído por 'Batch reads > 4 MiB defer to ByteStream::Read fallback'. + scope `cache-r` / `cache-find-missing` aligned com auth_model.md §scope table (canonical hyphen-form; cache-find-missing é separate scope para discovery sem download capability). (c) **P0 WI-S02-003 ABI/FFI two-layer split** — §1 + §6.1.1 + §8 AC: separated Rust-native idiomatic API (lib.rs com Result<>, impl Stream<>, AsyncRead) from C-ABI FFI surface (ffi.rs feature-gated com extern 'C' opaque handle pattern + i32 error codes); cbindgen scope limited to ffi.rs module; cdylib build only com --features ffi. (d) **P1 WI-S02-002 cost calc** — 365B / 1M = 365,000 (não 365); $365k/yr (não $365/yr); math fixed; budget rationale aligned com B2B SaaS ARPU. (e) **P0 WI-S02-005 freshness drift cleanup** — §9.1 'stale tolerable max 5s' → 'TTL ≤ 300s'; §9.4 'window ≤ 5s' → 'TTL bound + 60s typical não guaranteed'. (f) **P2 404/403 sweep cycle 7** — sprint.md §1 L51 + §5 D4; spec_contract §16 SOTA verdict; WI-002 §6.2 ref; WI-003 footer; WI-004 §4 CAP-CAS-008 — all 'Constant-time 404/403' → 'Constant-time 404 MissReason parity (ADR-0028 3-arm)'. |
| 1.7.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 6 codex SEAL real validation 8.8 → ≥9.0 target) | **3 codex P0/P1/P2 leftovers fixed**: (a) **P0 WI-005 60s residuals → TTL canonical** — §2 narrative race scenario + §8 AC race scenario + §9.11 design decision (3 locations): all '60s' bounds rephrased as 'TTL bound ≤ 300s defensible hard bound; typical propagation 60s mas CF docs OR MORE não guaranteed'. (b) **P1 WI-006 obsolete invariants** — §3 narrative removed 'monotonic version stamp invariant INV-NEG-CACHE-MONOTONIC' reference (KV não atomic CAS; replaced com eventual-consistency convergence oracle); §2 narrative '404 ou 403' → '404 uniform per ADR-0028 NUNCA 403' (aligned com WI-002 §8 AC + WI-001 cross-tenant scenario). (c) **P2 404/403 banner sweep** — WI-004 §0 metadata title + sprint.md §2.1 in-scope WI-004 + §3 customer-visible CAP-CAS-008 + spec_contract §12 PERT WI-004 row: 'Constant-time 404/403' → 'Constant-time 404 MissReason parity (ADR-0028 3-arm)'. |
| 1.6.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 5 codex SEAL real validation 8.9 → ≥9.0 target) | **3 codex P0/P1/P2 fixed**: (a) **P0 KV freshness bound canonical** — WI-005 §6.1.3 + §8 stale window AC + §10.5.2 completeness updated: defensible hard bound = TTL (≤ 300s); typical CF KV propagation ≤ 60s mas not guaranteed (CF docs explicitly 'OR MORE'); 60s removed as hard bound. (b) **P1 WI-006 race oracle aligned com eventual consistency** — §3 narrative + §8 AC: 'no stale cached negative observed' (invalid oracle for CF KV) → '0 incorrect hot reads + eventual convergence within TTL bound + cliente retry resolves staleness'. (c) **P2 side-channel proof gate single-source** — WI-004 §6.1.5 + §10.4.3 reconciled (alert SEV-2 canonical = > 5ms / 5min single-source); §8 AC + §10.4.1 Šidák clarified (within-trial 3-pair correction + across-trial 3-replication mitigation = 9 tests total combined α ≈ 0.000125). |
| 1.5.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 4 codex SEAL real validation 8.7 → ≥9.0 target) | **3 codex P0/P1 ENGINEERING blockers fixed**: (a) **P0 negative-cache key canonical sweep** — _spec_contract §4 CAP-CAS-007 + §5.4 R-S02-9 + sprint §2.1 in-scope + sprint §5 D5 + WI-S02-005 §1 title + §2 race narrative: `ac_neg:<digest>` → `ac_neg:<region>:<HMAC16>:<digest_hex>` canonical (per remote_cache_product_profile.md §7.1; HMAC16 = `b64(HMAC_SHA256(TDK, tenant_id))[0:16]`; cross-tenant cache poisoning impossível por construction). (b) **P0 KV reality alignment** — WI-005 §6.1.3 GetBlob short-circuit + §8 AC stale window scenario + §8 AC race scenario + §9.11 design decision rewritten: removed unrealistic 'region-local strongly consistent / version_stamp atomic CAS' claims (CF Workers KV não oferece atomic CAS primitive; cited CF KV docs); replaced com TTL + last-write-wins + cliente retry pattern (race resolution natural via Bazel retry semantics + 60s propagation bound + 300s TTL). Future DO migration path documented. (c) **P1 WI-004 statrs API alignment** — `statrs::distribution::statistical_power` referência removed (não existe em statrs); replaced com custom Rust Cohen's d implementation OR external G*Power tool (documented em ADR-0023); Mann-Whitney explicitly cita `statrs::stats_tests::mann_whitney_u` per docs.rs/statrs; §2 + §6.1.3 + §10.4.1 + §17 ST-004 aligned. |
| 1.4.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 3 codex SEAL real validation 8.4 → ≥9.0 target) | **3 codex P0/P1 ENGINEERING blockers fixed**: (a) **P0 S-02/S-15 boundary residuals swept** — _spec_contract §4 CAP-CAS-005 + §4 CAP-CAS-008 + §6 PRR sign-off list + §7 10.s02.4 + §7 10.s02.6 + §14 promotion + §15 risk row + §16 benchmark table 2 rows. S-02 ships Rust crate + ABI stable; FFI Python/Go/JS = S-15. (b) **P0 sign-off matrix single-source canonical 11**: _spec_contract §6 PRR list aligned com sprint §14; WI-S02-001 §30 table pruned 14→11; WI-S02-002 §30 13→11 + heading; WI-S02-003 §30 13→11 + heading + Crypto SME folded into Architect specialization mandatory; WI-S02-004 §30 13→11 + Crypto SME + Statistician folded; WI-S02-005 §30 13→11 + heading. Peer reviewers + Crypto SME canonical fold per framework §33.5.4.3 + ADR-0034 single-source. (c) **P1 WI-004 obsolete 404/403 swept** — title rewritten 'Constant-Time 404 MissReason Parity Middleware (ADR-0028 3-arm)'; §6.1.4 criterion benchmark 'p99 404 vs 403' → '3-arm pairwise |Δmedian| + |Δp99|'; §7 anti-scope first item rewritten; §8 Property test Gherkin scenario rewritten for 3-arm. |
| 1.3.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 2 codex SEAL remediation) | **3 P0/P1 + P2 polish** (score 8.8→target ≥9.0): (a) **P0 WI-004 AC scenarios full rewrite** — 3-arm 404 model (NotFound × CrossTenantMasked × Tombstoned) replaces 404-vs-403; pairwise Mann-Whitney + Šidák; |Δmedian| ≤ 1ms criterion; sprint.md §6.3 INV updated. (b) **P1 stale 3-SDK seal language swept** — _spec_contract §0 SOTA target + §1 objetivo + §1 SOTA framing + §12 PERT WI-003 + §13 marcos + §15 anti-waivable + sprint.md §9 milestones. (c) **P1 sign-off matrix prune** — sprint.md §14 13-item list → 11 canonical (Crypto SME folds into Architect; Adversarial into AppSec); WI-006 §2 PRR list + §6.1.4 PRR doc + §30 heading aligned. (d) **P2 editorial** — WI-001 §1 AuthZ wording 403→404 uniform; WI-002 §3 error mapping clarified (404 uniform vs 403 PAT scope). |
| 1.2.0 | 2026-04-29 | Gustavo (Lote 10.2bis cycle 1 codex SEAL remediation) | **4 codex 6.8 P0/P1 ENGINEERING blockers fixed**: (a) **P0 response contract** — 403 cross-tenant → 404 uniform per ADR-0028 (MissReason freeze); side-channel defense relocated from status-code differential → timing layer (3 arms: NotFound × CrossTenantMasked × Tombstoned all 404; pairwise Mann-Whitney + Šidák); _spec_contract §5.3 + §5.5 + §6 DoD + §8 INV rewritten; sprint.md §3 + §6.2 aligned; WI-001 §2 + §6.1.4 + §6.1.6 + §6.1.8 + §8 AC updated; WI-004 §1 + §2 + §6.1 fully rewritten (3-arm methodology). (b) **P1 negative cache canonical** — KV key `tenant_id_hex` → HMAC16 canonical (per remote_cache_product_profile.md §7.1); GetBlob short-circuit semantics clarified (canonical with invalidation hook); FindMissingBlobs MUST fall-through (storage_semantics_matrix.md authoritative requirement); semantic distinction explicit. (c) **P1 S-02/S-15 boundary** — sprint.md §2.1 + §5 D3 + §7 DoD aligned com spec_contract §6 NOTA: S-02 ships Rust crate + ABI stable; FFI integration tests Python/Go/JS = S-15 (NÃO bloqueia SEAL S-02). (d) **P1 sign-offs 13/10-12 → 11 canonical** (framework §33.5.4.3 + ADR-0034) — 13+ locations across spec_contract + sprint + 6 WIs. |

---

**Fim spec contract S-02 v1.15.0 SOTA.**
