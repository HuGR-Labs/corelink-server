---
id: "SPEC-CONTRACT-S02"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
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
| SOTA target | Read path tier-1 — sub-100ms p99 cache-warm + client verify default-on + side-channel-resistant 404/403 + negative cache + streaming memory-bounded |

## 1. Objetivo

Implementar **path de leitura CAS production-grade** completando o loop write→read iniciado em S-01: REAPI `ByteStream::Read` (streaming chunked, memory-bounded), `GetBlob` unary (small blobs ≤ 4 MB inline), `FindMissingBlobs` (batch discovery para clients Bazel/Buck2), client-side verify obrigatório default-on (`corelink-client-verify` crate, CTRL-CAS-002), negative caching de 404 (KV TTL curto reduz cost de probe attacks + speed up legitimate misses), constant-time 404 vs 403 (CTRL-ISO-004 — prevent enumeration side-channel). Sem read path, S-01 é write-only e produto inviável.

**Por que SOTA:** competitors entregam read path com client verify opt-in (BuildBuddy) ou ausente (NativeLink em alguns SDKs); sem constant-time response distinction → enumeration side-channel; sem negative cache → cost overhead em probe storms. CoreLink S-02 entrega: (a) **TLA+ tenant_isolation.tla covering read path**; (b) **constant-time 404/403** com p99 diff < 5ms verified; (c) **client verify default-on em 3 languages** via `corelink-client-verify` crate (S-15 reuse); (d) **side-channel benchmarks** (timing attack resistance via criterion); (e) **streaming memory-bounded** (Worker chunk-based, não buffer completo). Reference: **REAPI v2 spec**, **OWASP Side-Channel Testing**, **NIST SP 800-53 SC-4 (Information in Shared Resources)**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
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
| **CAP-CAS-005** | Client-side verify integrity | `corelink-client-verify` crate (Rust) — auto-verifies BLAKE3 hash post-download; default-on em 3 SDK wrappers (S-15). Detect bit rot + cache poisoning. |
| **CAP-CAS-006** | Streaming read | ByteStream chunked (1 MiB chunks); Worker memory bounded; no full-blob buffering. |
| **CAP-CAS-007** | Negative caching (404) | KV `ac_neg:<digest>` TTL 300s (PAT-KV-TTL-001); reduces probe cost; cache invalidation em S-01 write. |
| **CAP-CAS-008** | Side-channel-resistant 404/403 | Constant-time response com p99 diff < 5ms; CTRL-ISO-004 enforcement; criterion benchmark proves. |
| **CAP-CAS-009** | FindMissingBlobs (REAPI batch) | Discover gaps em batch antes de upload (Bazel client optimization); same security envelope. |

## 5. Requirements específicos

### 5.1 REAPI handlers (CAP-CAS-004 + CAP-CAS-006 + CAP-CAS-009)

- **R-S02-1**: REAPI `ByteStream::Read` Worker handler com streaming chunked (1 MiB chunks); `read_offset` + `read_limit` semantics conforme REAPI v2 spec.
- **R-S02-2**: `GetBlob` unary handler para small blobs ≤ 4 MB inline; falls back to ByteStream se larger.
- **R-S02-3**: `FindMissingBlobs` batch endpoint: input `[digest]` → output `[missing_digests]`; enables Bazel client efficient pre-upload check.
- **R-S02-4**: HTTP `GET /v1/cas/<digest>` (REAPI-equivalent REST surface) com Accept: `application/octet-stream`.

### 5.2 Client Verify (CAP-CAS-005)

- **R-S02-5**: Crate `corelink-client-verify` (Rust lib SDK side):
  - Compute BLAKE3 hash do body downloaded.
  - Compare com expected digest (from URL ou response header).
  - Mismatch → throw `DigestMismatchError` (error_taxonomy.md `COR_CAS_DIGEST_MISMATCH`).
  - Default-on em SDK; opt-out via explicit `verify=false` flag (documented + warning).
- **R-S02-6**: Test: inject bit-rot scenario (modify R2 object out-of-band) → client retorna `DigestMismatchError` em 100% das tentativas.

### 5.3 Side-Channel Resistance (CAP-CAS-008)

- **R-S02-7**: Constant-time 404 vs 403 (CTRL-ISO-004) — prevent enumeration side-channel:
  - Worker responde com mesmo timing window para ambos os cases.
  - Implementação: timing-padding via `tokio::time::sleep` + jitter ou pre-computed delay.
  - Benchmark criterion: p99 diff entre 404 vs 403 < 5ms (target SOTA).
- **R-S02-8**: Adversarial test: 10k requests probe digest random → measure timing distribution; statistical test (Mann-Whitney U) validates indistinguishability `p > 0.05`.

### 5.4 Negative Cache (CAP-CAS-007)

- **R-S02-9**: Negative cache KV `ac_neg:<digest>` com TTL 300s (PAT-KV-TTL-001):
  - Hit em digest known-missing → return 404 sem hit R2/D1 (cost saving).
  - Cache invalidation em S-01 PUT (write path coloca digest em `ac_neg:` clear).
  - Per-region KV (residency alignment).

### 5.5 Tenant Isolation (CAP-CAS-004 envelope)

- **R-S02-10**: Tenant context propagation: `Authorization: Bearer <PAT>` → resolve `tenant_id` (S-03 reuse via auth middleware) → namespace lookup `<tenant_prefix>/<digest>` em R2; reject se prefix mismatch.
- **R-S02-11**: AuthZ check em storage call (CTRL-ISO-002): pre-R2 read, verify `tenant_id` matches blob_meta.tenant_id em D1; mismatch = 403 + audit emit.

### 5.6 Observability + SLO

- **R-S02-12**: Métricas: `corelink.cas.get.requests_total{tenant_tier, region, result}` (result ∈ `hit, miss, neg_cache_hit, integrity_fail, denied`); `corelink.cas.get.duration_seconds{p50, p95, p99}`; `corelink.cas.side_channel.timing_diff_ms` (gauge para CTRL-ISO-004 monitoring).
- **R-S02-13**: SLO-AVAIL-CAS-GET ≥ 99.9% sustained 72h staging; SLO-LAT-CAS-GET p99 < 300ms (cold) / < 100ms (warm).

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **Property test 100k iter** tenant isolation: nenhum read retorna blob fora de namespace do tenant (EVT-002).
- [ ] **TLA+ verdes em CI**: `tenant_isolation.tla` + `cas_integrity.tla` (EVT-022).
- [ ] **Load test** read 50k QPS × 10 min em staging com SLO-AVAIL-CAS-GET preserved (EVT-021).
- [ ] **Client verify default-on**: SDK em 3 languages (Python/Go/JS — S-15 ownership) tested integration; opt-out path documented (EVT-002 + EVT-018).
- [ ] **Side-channel test**: medir latência 404 vs 403 → p99 diff < 5ms via criterion benchmark; statistical test Mann-Whitney p > 0.05 (EVT-002 + EVT-040 if external review).
- [ ] **Runbook `RB-FM-253`** (cross-tenant read) dry-run executado em staging (EVT-017).
- [ ] **SBOM + signed release** (CycloneDX 1.5+ via S-12 R-S12-3) (EVT-010).
- [ ] **Negative cache** test: probe storm 1k QPS de unknown digests → measure cost reduction vs no-cache baseline (EVT-021).
- [ ] **Streaming memory test**: read 1 GiB blob via ByteStream → Worker memory peak < 50 MiB (não load full blob) (EVT-002).
- [ ] **Bit rot integrity test**: corrupt R2 object out-of-band → 100% client verify catches (EVT-002).
- [ ] **PRR HIGH_RISK** (10–12 sign-offs): SRE lead + Security lead + Engineer + QA + Product + Compliance officer + Privacy officer + Architect + AppSec advisor + 2 peers + Crypto SME (BLAKE3 verify review) (EVT-031).
- [ ] **Cost regression gate**: CAS GET hot path benchmark per-op cost; < 10% regression vs S-01 write baseline (Lote 9.4 §14.10).

## 7. Completeness Criteria (delta local)

- [ ] **10.s02.1** E2E: write em S-01 → read em S-02 retorna body idêntico byte-a-byte (EVT-018).
- [ ] **10.s02.2** SLO-AVAIL-CAS-GET: 99.9% em staging sustained 72h (EVT-021).
- [ ] **10.s02.3** SLO-LAT-CAS-GET: 99% < 300ms cold; < 100ms warm (team target) (EVT-021).
- [ ] **10.s02.4** **Side-channel resistance proven**: Mann-Whitney U test 10k samples p > 0.05 (EVT-002 + EVT-040).
- [ ] **10.s02.5** **Negative cache effectiveness**: probe storm cost reduced ≥ 80% vs baseline (EVT-021).
- [ ] **10.s02.6** **Client verify ubiquity**: 100% das SDK calls default-on; opt-out emit warning log (EVT-002).
- [ ] **10.s02.7** **Tenant isolation property test**: 100k iter cross-tenant attempts → 0 successes (EVT-002).
- [ ] **10.s02.8** **Streaming memory bounded**: 1 GiB blob read → Worker peak < 50 MiB (EVT-002).
- [ ] **10.s02.9** **Bit rot detection**: 100% bit rot scenarios caught client-side (EVT-002).

## 8. Invariants

### Mantidas (heredadas de canonical sources)

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): read nunca retorna blob de outro tenant. Coberto por `tenant_isolation.tla` em CI.
- **INV-CAS-INTEGRITY** (CRITICAL, TLA+): client verify detecta bit rot/poisoning. Coberto por `cas_integrity.tla` em CI.
- **INV-CAS-IMMUTABILITY** (CRITICAL): reads após GC soft-delete respeitam tombstone.
- **INV-CAS-IDEMPOTENCY** (CRITICAL): same digest sempre retorna same body.

### Novas (S-02 — adicionar a invariant_registry §3.12 como Lote 9.4 followup se needed)

- **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** (HIGH — Lote 9.4 candidate for §3.12): timing distribution de 404 vs 403 é statistically indistinguishable (Mann-Whitney p > 0.05). **Why:** prevent enumeration of digest existence cross-tenant. **How to apply:** criterion benchmark + adversarial test 10k samples + `corelink.cas.side_channel.timing_diff_ms` < 5ms p99.

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
| **WI-S02-002** | GetBlob unary + FindMissingBlobs + small-blob inline | unary handler; size-based dispatch; FindMissingBlobs batch logic; benchmark | 10h | 16h | 26h | **16.7h** |
| **WI-S02-003** | Crate corelink-client-verify (SDK lib) + 3 language integration tests | BLAKE3 verify lib; default-on toggle; opt-out warning; FFI integration tests Python/Go/JS | 12h | 18h | 30h | **19.0h** |
| **WI-S02-004** | Constant-time 404/403 middleware + criterion benchmark + Mann-Whitney test | timing-padding middleware; jitter; criterion benchmark; statistical test 10k samples | 14h | 22h | 36h | **23.0h** |
| **WI-S02-005** | Negative cache KV adapter + invalidation hook em S-01 write | KV per-region; TTL 300s; invalidation hook; probe storm test | 8h | 14h | 22h | **14.3h** |
| **WI-S02-006** | Property test 100k tenant isolation + RB-FM-253 dry-run + bit-rot test + PRR | property test framework; tenant isolation 100k iter; bit rot inject; RB-FM-253 walkthrough; PRR docs | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~115h ≈ 14 dias work × 1 eng. Buffer 5 dias confere com 3 semanas.

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 5 dias.
- **Marcos:**
  - **D+4:** WI-001 SEALED (ByteStream::Read live em staging).
  - **D+6:** WI-002 SEALED (GetBlob + FindMissingBlobs).
  - **D+9:** WI-003 SEALED (client-verify crate + 3-language integration).
  - **D+12:** WI-004 SEALED (constant-time + side-channel test passed).
  - **D+13:** WI-005 SEALED (negative cache + invalidation).
  - **D+15:** WI-006 SEALED (property test + runbook + PRR).
  - **D+17:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + 72h staging sustained SLO-AVAIL-CAS-GET ≥ 99.9%.
- TLA+ 2 specs verdes.
- Side-channel resistance verified.
- Client verify default-on em 3 SDKs.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Side-channel timing vaza existência de blob** (THR-I-002) | M | M | HIGH | M | LOW | Constant-time middleware + jitter + criterion benchmark p99 < 5ms + Mann-Whitney test. |
| **Streaming memory leak em long reads** (FM-403) | M | M | MEDIUM | M | LOW | Chunk 1 MiB bounded + RB-FM-403 dry-run + memory peak monitoring; trimestral container restart (PAT-RESTART-JIT-001). |
| **SDK client verify default-off por bug** | L | L | CRITICAL (CTRL-CAS-002 bypassed) | L | LOW | Default-on em 3 langs; opt-out path requires explicit flag + warning log; CI gate verifica default. |
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
| Client verify default-on | Opt-in | Limited | Manual | No | **Yes — 3 SDKs default-on + warning opt-out** |
| Constant-time 404/403 | No | No | No | No | **Yes — p99 diff < 5ms + Mann-Whitney p > 0.05** |
| Streaming memory bounded | Yes | Yes | Yes | Yes | **Yes — 1 MiB chunks, 50 MiB peak** |
| Negative caching | No | No | Limited | Yes (TTL 60s) | **Yes — KV TTL 300s + invalidation hook** |
| TLA+ tenant isolation verified | No | No | No | No | **Yes — tenant_isolation.tla green CI** |
| Bit rot detection client-side | Manual | Limited | Manual | Yes | **Yes — 100% bit rot caught** |
| 5-layer defense tenant isolation | 2 layers | 2 layers | 1 layer | 3 layers | **5 layers (auth_model §8.1)** |
| FindMissingBlobs batch | Yes | Yes | Yes | Yes | **Yes — REAPI-compliant** |
| SLO-LAT-CAS-GET p99 cold | 400-600ms | 300-500ms | 200-400ms | 200-300ms | **< 300ms** |

**Veredito SOTA:** S-02 v1.1 atinge feature parity em 7/9 dimensões; **vantagem em constant-time 404/403 + TLA+ verified tenant isolation** (rare/unique).

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
- ❌ Constant-time 404/403 (CTRL-ISO-004) — side-channel resistance baseline.
- ❌ Client verify default-on em 3 SDKs — CTRL-CAS-002 baseline.
- ❌ Bit rot 100% detection — INV-CAS-INTEGRITY baseline.

Itens waivable com Security lead + Architect + ADR:

- ⚠️ Side-channel test Mann-Whitney p > 0.05 → p > 0.01 com explicit risk acceptance e plan to fix.
- ⚠️ SLO-LAT-CAS-GET p99 cold 300ms → 400ms com explicit customer SLA addendum.
- ⚠️ Negative cache TTL 300s → 60s (mais conservador) com cost overhead acceptance.
- ⚠️ FindMissingBlobs batch defer → S-04 se prazo apertado (impacta DX Bazel mas não correctness).

---

**Fim spec contract S-02 v1.1.0 SOTA.**
