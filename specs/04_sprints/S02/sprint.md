---
id: "S-02"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  # Solo-tier per ADR-0034 — schema-conforming role + name (front_matter.schema.json reviewers.items).
  # Owner + Final Approver are above (Gustavo); 9 specialized canonical roles TBD with dual-hat per ADR-0034.
  - role: "architect"
    name: "Gustavo Schneiter (dual-hat per ADR-0034)"
  - role: "security_lead"
    name: "TBD (ADR-0034 dual-hat até hire)"
  - role: "sre_lead"
    name: "TBD (ADR-0034 staffing-blocked)"
  - role: "eng"
    name: "TBD"
  - role: "qa"
    name: "TBD"
  - role: "product_lead"
    name: "Gustavo Schneiter (dual-hat per ADR-0034)"
  - role: "compliance"
    name: "TBD"
  - role: "privacy_lead"
    name: "TBD"
  - role: "security"
    name: "TBD (AppSec; ADR-0034 dual-hat)"
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
  - "RESILIENCE-PATTERNS"
  - "PRIVACY-MODEL"
tags: ["sprint", "s02", "cas", "read-path", "client-verify", "side-channel", "high-risk"]
---

# Sprint S-02 — CAS Read Path + Client Verify (Streaming + Negative Cache + Side-Channel Resistant)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-25**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ Solo-tier per ADR-0034 (CONDITIONALLY_APPROVED via dual-hat com expiry); 9 specialized roles TBD; PRR-S02.md §sign-off matrix documenta status canonical com explicit waiver references (see _spec_contract §6 PRR DoD: 11 sign-offs canonical per framework §33.5.4.3 + ADR-0034)
> **Spec contract base:** `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA elevation)
> **inherits_from:** SECURITY-MODEL + REMOTE-CACHE-PRODUCT-PROFILE + AUTH-MODEL + KEY-MANAGEMENT + INVARIANT-REGISTRY + DATA-MODEL + OBSERVABILITY-MODEL + FAILURE-MODES + SLO-CATALOG + RESILIENCE-PATTERNS + PRIVACY-MODEL

> **🚦 Phase boundary:** este sprint é **Fase 1 — Remote Cache** (não toca execute-action / Remote Execution).

---

## 1. Objetivo

Implementar o **CAS read path production-grade** completando o loop write→read iniciado em S-01:

1. REAPI v2 `ContentAddressableStorage::Read` (ByteStream chunked streaming) + `GetBlob` unary (small ≤ 4 MiB) + `FindMissingBlobs` batch.
2. Client-side BLAKE3 verify obrigatório default-on (CTRL-CAS-002) via crate `corelink-client-verify`.
3. Negative caching de 404s (KV TTL 300s) com invalidation hook em S-01 write.
4. **Constant-time 404 MissReason parity** (ADR-0028 3-arm; CTRL-ISO-004) — prevent enumeration side-channel via timing; criterion + pairwise Mann-Whitney U + Šidák correction test.
5. Tenant context propagation com **5-layer defense** (auth_model §8.1) reaproveitando S-01 TenantPrefix derivation.
6. SLO-AVAIL-CAS-GET ≥ 99.9% + SLO-LAT-CAS-GET p99 < 300ms (cold) / < 100ms (warm) sustained 72h staging.

Sem read path, S-01 é write-only e produto inviável. Bug em tenant isolation = catastrophic blast radius (FM-253).

## 2. Escopo do sprint

### 2.1 In-scope

- **WI-S02-001**: REAPI ByteStream::Read handler + HTTP GET surface + tenant context propagation reusing S-01 TenantPrefix.
- **WI-S02-002**: GetBlob unary (small ≤ 4 MiB inline) + **BatchReadBlobs batch read** (REAPI mandatory per remote_cache_product_profile.md §12.2; aggregate cap 4 MiB) + FindMissingBlobs batch endpoint (REAPI conformance).
- **WI-S02-003**: Crate `corelink-client-verify` (Rust lib) + ABI-stable interface + default-on toggle. **NOTA Lote 9.5**: FFI integration tests Python pyO3 / Go cgo / JS WASM são entregável **S-15** (consumer); S-02 entrega Rust crate + ABI stable; SDK integration tests rodam em S-15 sprint, não bloqueiam SEAL S-02.
- **WI-S02-004**: Constant-time 404 MissReason parity middleware (ADR-0028 3-arm: NotFound × CrossTenantMasked × Tombstoned; timing-padding + jitter) + criterion benchmark + pairwise Mann-Whitney U + Šidák adversarial test 10k samples per arm.
- **WI-S02-005**: Negative cache KV adapter (`ac_neg:<region>:<HMAC16>:<digest_hex>` canonical per remote_cache_product_profile.md §7.1; per-region; TTL 300s) + invalidation hook em S-01 write path.
- **WI-S02-006**: Property test 100k tenant isolation + bit-rot integrity test + RB-FM-253 dry-run + PRR HIGH_RISK doc.

### 2.2 Anti-scope

- ❌ Write path (S-01).
- ❌ AC reads (S-04 — AC tem read path próprio com Merkle verify).
- ❌ Multi-region failover read (S-14 PAT-REGION-FAILOVER-001).
- ❌ Compression de blobs em transit (HTTP gzip OK, mas não custom format).
- ❌ HEAD requests sem auth (REAPI não suporta).
- ❌ Range requests além de REAPI semantics.
- ❌ HTTP/2 multiplexing tuning (Cloudflare handles).
- ❌ Read-after-write aggressive consistency (R2 strong consistency suficiente).

## 3. Customer Impact & Journey

> Herda de `remote_cache_product_profile.md §2` (Customer Journey).

**JTBD coberto:** "Como dev de CI do tenant X, eu quero baixar build outputs de cache CoreLink rapidamente (≤ 100ms warm), com garantia criptográfica de integridade (sem bit rot ou cache poisoning), e ter zero exposição a builds de outros tenants."

**Customer-visible outcomes:**
- Bazel/Buck2 cache HIT em < 100ms warm (vs 300-500ms competitors).
- Bit rot detectado client-side via BLAKE3 verify default-on (CTRL-CAS-002).
- Tenant isolation cryptographic (5 layers; TLA+ verified).
- 404 timing-indistinguishable across MissReason variants (per ADR-0028 uniform 404 freeze; não é enumeration vector contra meu blob digests).

**CAPs entregues** (do _spec_contract.md):
- `CAP-CAS-004` (GET blob by digest)
- `CAP-CAS-005` (Client-side verify integrity)
- `CAP-CAS-006` (Streaming read chunked)
- `CAP-CAS-007` (Negative caching 404)
- `CAP-CAS-008` (Side-channel-resistant 404 MissReason parity per ADR-0028)
- `CAP-CAS-009` (FindMissingBlobs REAPI batch)

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4` para tabela canonical CAPs. Este sprint endereça 6 CAPs CAS read path direct + envelope com:

- **Inherits**: 5-layer tenant isolation defense (auth_model §8.1) — defense já provada em S-01 write path; S-02 reuses.
- **Forward-looking**: error_taxonomy.md `COR_CAS_*` codes consumed by SDK (S-15 sprint dep).

## 5. Deliverables

| ID | Entregável | Onde | Definition of Done |
|---|---|---|---|
| S02-D1 | ByteStream::Read Worker handler (gRPC + HTTP) | `src/reapi/cas_read.rs` + `src/http/cas_read.rs` | gRPC handler conform REAPI v2; HTTP GET `/v1/cas/<digest>` working; tenant context propagated via TenantPrefix lookup; AuthZ check pre-R2 read |
| S02-D2 | GetBlob unary + FindMissingBlobs batch | `src/reapi/cas_unary.rs` | GetBlob inline ≤ 4 MiB; FindMissingBlobs batch dedup discovery; size-based dispatch |
| S02-D3 | Crate `corelink-client-verify` (lib SDK side) | `crates/corelink-client-verify/src/lib.rs` | BLAKE3 verify post-download; default-on toggle; opt-out warning emit; ABI-stable interface (FFI integration tests Python/Go/JS = S-15 deliverable per spec_contract §6 NOTA Lote 9.5) |
| S02-D4 | Constant-time 404 MissReason parity middleware (ADR-0028 3-arm) + benchmarks | `src/middleware/timing_padding.rs` + `benches/side_channel.rs` | Criterion benchmark |Δmedian| ≤ 1ms + p99 diff < 5ms across 3 arms; pairwise Mann-Whitney U + Šidák all p > 0.05 |
| S02-D5 | Negative cache KV adapter | `src/cache/negative.rs` | `ac_neg:<region>:<HMAC16>:<digest_hex>` (HMAC16 canonical per remote_cache_product_profile.md §7.1) per-region; TTL 300s; invalidation hook em S-01 PUT; cost reduction ≥ 80% probe storms |
| S02-D6 | Property tests 100k tenant isolation + bit-rot test + integration tests | `tests/prop_cas_read.rs` + `tests/integration_e2e.rs` | 100k iter cross-tenant attempts → 0 successes; bit-rot inject → 100% catch; E2E S-01 write → S-02 read byte-identical |
| S02-D7 | RB-FM-253 dry-run + PRR HIGH_RISK doc | `specs/05_quality/runbooks/RB-FM-253.md` (existing) + `PRR-S02.md` | RB dry-run executed em staging com Security + SRE; PRR doc 11 sign-offs canonical (HIGH_RISK matrix per framework §33.5.4.3) |

## 6. Escopo técnico por camada (inherits_from)

Todos os elementos abaixo herdam dos canonical sources listados no `inherits_from` do YAML. Deltas locais do sprint:

### 6.1 Storage (herda `data_model.md §5` + `storage_semantics_matrix.md §3`)

- R2 read em `cas-<region>` buckets (mesmas 3 regiões S-01: WNAM, WEUR, SAM).
- D1 read `blob_meta` para AuthZ check (tenant_id match) pre-R2 GetObject.
- **Delta local:** read path respect tombstone (`deleted_at IS NOT NULL` → 404 + negative cache populate); INV-CAS-IMMUTABILITY enforced via tombstone semantics (S-06 GC alignment).

### 6.2 Auth (herda `auth_model.md §5` + §8.1 5-layer defense)

- PAT scope `cache-r` obrigatório para reads (canonical hyphen-form per auth_model.md §scope; vs `cache-w` em S-01).
- TenantPrefix derivation reusing S-01 `corelink-tenant-path` crate (dependência hard).
- **Delta local:** PAT validation stub continua até S-03 SEALED; integration test cobre stub→real transition.
- **Delta local:** AuthZ check em storage call (CTRL-ISO-002) — pre-R2 read verifica `tenant_id` matches `blob_meta.tenant_id` em D1; mismatch = **404 (uniform per ADR-0028; CrossTenantMasked variant)** + audit emit `corelink.cas.cross_tenant_attempt` (forensics; client observa apenas 404).

### 6.3 Invariantes verificadas (herda `invariant_registry.md §3`)

- `INV-TENANT-ISOLATION` (CRITICAL): TLA+ `tenant_isolation.tla` + property test 100k iter cross-tenant attempts.
- `INV-CAS-INTEGRITY` (CRITICAL): TLA+ `cas_integrity.tla` + client verify default-on (CTRL-CAS-002).
- `INV-CAS-IMMUTABILITY` (CRITICAL): reads após GC soft-delete respeitam tombstone.
- `INV-CAS-IDEMPOTENCY` (CRITICAL): same digest sempre retorna same body byte-identical.
- `INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE` (HIGH — registry §3.12 add Lote 9.4): timing distribution **across all 404 MissReason variants** (NotFound × CrossTenantMasked × Tombstoned per ADR-0028) statistically indistinguishable; pairwise Mann-Whitney U all p > 0.05 com Šidák correction; criterion |Δmedian| ≤ 1ms; p99 diff < 5ms.

### 6.4 SLOs aplicáveis (herda `slo_catalog.md §4`)

- `SLO-AVAIL-CAS-GET`: 99.9% availability em team tier (meta S02; sustained 72h staging).
- `SLO-LAT-CAS-GET`: 99% < 300ms cold reads; < 100ms warm reads (cache hit).
- `SLO-CORRECT-CAS`: 100% — todo bit rot detected client-side; zero false negatives.

### 6.5 Crypto (herda `key_management.md §3.2.1` + ADR-0018)

- BLAKE3 verify SIMD-optimized; ≥ 2 GB/s single-core throughput target.
- TenantPrefix derivation via TDK (HKDF info=`path`); reuses S-01 implementation.
- **Delta local:** client-verify lib é reused em SDKs S-15; FFI ABI stable.

## 7. Definition of Done (lane HIGH_RISK)

Todas abaixo obrigatórias (framework §33.5.4.1 HIGH_RISK matrix):

- [ ] Todos 6 WIs do sprint em status `SEALED` (EVT-031)
- [ ] Property test 100k iter tenant isolation verde (EVT-002)
- [ ] TLA+ checks verdes em CI (EVT-022): `tenant_isolation`, `cas_integrity`, `audit_immutability`, `gc_correctness`
- [ ] SAST clean (clippy -D warnings + semgrep) (EVT-005)
- [ ] Fuzz targets para REAPI parsers + digest parsing: 1h nightly em CI (EVT-008)
- [ ] Integration tests E2E em staging CF: write em S-01 → read em S-02 byte-identical (EVT-002 + EVT-018)
- [ ] Load test 50k QPS read sustained × 10 min em staging (EVT-024)
- [ ] Chaos experiments: inject R2 latency + D1 primary failover + KV outage (EVT-023)
- [ ] **Side-channel test**: criterion benchmark p99 diff < 5ms; Mann-Whitney U test p > 0.05 (EVT-002 + EVT-040 if external review)
- [ ] **Bit rot detection test**: corrupt R2 object out-of-band → 100% client verify catches (EVT-002)
- [ ] **Negative cache effectiveness**: probe storm 1k QPS unknown digests → cost reduction ≥ 80% vs no-cache (EVT-021)
- [ ] **Streaming memory test**: read 1 GiB blob via ByteStream → Worker memory peak < 50 MiB (EVT-002)
- [ ] **Client verify default-on**: Rust crate verify path complete + ABI-stable interface (FFI integration tests Python/Go/JS = S-15 sprint scope per spec_contract §6 NOTA Lote 9.5; NÃO bloqueia SEAL S-02); opt-out warning log emitted (EVT-002)
- [ ] Progressive rollout dry-run em staging (EVT-038)
- [ ] Observability plan executado (métricas emitindo, dashboards criados) (EVT-013)
- [ ] PRR HIGH_RISK 11 sign-offs canonical per framework §33.5.4.3 + ADR-0034 (EVT-031)
- [ ] Runbook `RB-FM-253` (cross-tenant read) dry-run executado (EVT-017)
- [ ] SBOM gerado + assinado CycloneDX 1.5+ (EVT-010)
- [ ] Adversarial review executado (EVT-025 pentest interno)
- [ ] **Cost regression gate** (Lote 9.4 §14.10): CAS GET hot path benchmark per-op cost; PR > 10% regression bloqueia (EVT-002)

## 8. Dependencies

### Hard blockers (must SEAL antes de S-02 start)

- **S-01 SEALED** (CAS write path; sem write não há blob para read).
- **S-00 roadmap** (capabilities catalog).

### Soft blockers (preferencial mas não impeditivo)

- **S-03** (auth real Clerk; staging stub OK até S-03 SEALED — drift risk documented em §10).
- TLA+ `tenant_isolation.tla` + `cas_integrity.tla` verdes (já satisfeito desde Lote 5.13).

### Outbound

- S-02 desbloqueia: S-04 (AC consume CAS read primitives), S-07 (eviction depende de read patterns), S-15 (CLI/SDK consume read API + client-verify crate), S-20 (GA exige read SLOs sustained 30d).

## 9. Timeline

- **Sprint kick-off**: 2026-05-22 (após S-01 SEALED target 2026-05-19)
- **D+4 milestone**: WI-S02-001 SEALED (ByteStream::Read live em staging).
- **D+6 milestone**: WI-S02-002 SEALED (GetBlob + FindMissingBlobs).
- **D+9 milestone**: WI-S02-003 SEALED (client-verify Rust crate + ABI stable; FFI integration = S-15).
- **D+12 milestone**: WI-S02-004 SEALED (constant-time + side-channel test passed).
- **D+13 milestone**: WI-S02-005 SEALED (negative cache + invalidation hook).
- **D+15 milestone**: WI-S02-006 SEALED (property + RB-FM-253 + PRR).
- **Mid-sprint review**: 2026-05-29 (D+7).
- **Sprint close target**: 2026-06-12 (3 semanas — HIGH_RISK timing).
- **Buffer**: 5 dias úteis (HIGH_RISK não-buffer = anti-pattern).

## 10. Risk Register

| ID | Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação | Owner |
|---|---|---|---|---|---|---|---|---|
| R-S02-001 | Side-channel timing vaza existência de blob (THR-I-002) | M | M | HIGH | M | LOW | Constant-time middleware + jitter + criterion p99 < 5ms + Mann-Whitney U test 10k samples | AppSec |
| R-S02-002 | Streaming memory leak em long reads (FM-403) | M | M | MEDIUM | M | LOW | Chunk 1 MiB bounded + RB-FM-403 dry-run + memory peak monitoring + trimestral container restart | SRE Lead |
| R-S02-003 | SDK client verify default-off por bug | L | L | CRITICAL (CTRL-CAS-002 bypassed) | L | LOW | Default-on em 3 langs; opt-out path requires explicit flag + warning log; CI gate verifica default; integration tests | Crypto SME |
| R-S02-004 | Cross-tenant read (FM-253) | L | M | CRITICAL (catastrophic blast radius) | M | LOW | INV-TENANT-ISOLATION TLA+ + property test 100k + 5-layer defense (auth_model §8.1) + RB-FM-253 dry-run | Architect |
| R-S02-005 | Bit rot R2 object não detectado (FM-051) | L | L | HIGH | L | LOW | Client verify default-on + scrub periódico (S-06 dep); 100% bit rot test em CI | Security Lead |
| R-S02-006 | Negative cache TTL too long (stale 404 após write) | M | L | MEDIUM | L | LOW | Cache invalidation hook em S-01 write path; TTL 300s upper bound; manual invalidation API | Tech Lead |
| R-S02-007 | PAT auth stub diverge de S-03 real (interface drift) | L | L | LOW | L | LOW | Stub interface frozen no Lote 9.4 contract; S-03 implementação respeita interface; integration test cobre transição | Tech Lead |
| R-S02-008 | Probe storm cost sem negative cache | M | M | MEDIUM | M | LOW | Negative cache TTL 300s + per-IP rate limit (S-08 soft dep); cost benchmark | SRE Lead |
| R-S02-009 | REAPI ByteStream protocol edge cases (read_offset out-of-bounds, read_limit zero) | M | L | LOW | L | LOW | REAPI v2 spec compliance test suite; reject invalid params com error_taxonomy entries `COR_CAS_*` | Engineer |
| R-S02-010 | TLS termination at CF Edge vs Worker context | L | L | LOW | L | LOW | Cloudflare handles; documentado; test cobre TLS 1.3 only path | SRE Lead |
| R-S02-011 | Cost regression > 10% baseline | M | M | MEDIUM | M | LOW | Cost regression gate §14.10 universal Lote 9.4; criterion benchmark per-op cost CI | Tech Lead |
| R-S02-012 | Mann-Whitney U test false positive (timing diff p < 0.05) | M | L | MEDIUM (delays merge) | L | LOW | Multiple measurement sessions; statistical noise reduction; 10k → 50k samples se needed | AppSec |

## 11. Observability Plan (delta do sprint)

Herda de `observability_model.md §4`. Delta local:

**Métricas novas emitidas** (canonical underscored Prometheus form per observability_model.md §4.1; label `plan` canonical per §3.1; spec uses dotted form as authoring convention, exposed name uses underscores):
- `corelink_cas_get_requests_total{plan, region, outcome}` (outcome ∈ `ok, miss, neg_cache_hit, integrity_fail, denied` per observability_model.md §3.1 enum).
- `corelink_cas_get_duration_seconds_bucket{outcome, blob_size_bucket}` (p50/p95/p99 per region/plan).
- `corelink_cas_get_bytes_total{plan, region}` (egress tracking; S-10 billing aligned).
- `corelink_cas_side_channel_timing_diff_ms` (gauge para CTRL-ISO-004 monitoring; p99 budget 5ms; alert SEV-2 sustained 5min per ADR-0023).
- `corelink_cas_client_verify_fail_total{reason}` (CTRL-CAS-002 violations: bit_rot, cache_poison_detect).
- `corelink_cas_negative_cache_hits_total{plan}` (cost saving signal).
- `corelink_cas_negative_cache_invalidation_total{trigger=s01_write|manual}` (S-01 write path coupling).
- `corelink_cas_streaming_peak_bytes{operation_id}` (memory bound monitoring).

**Dashboards novos:**
- `DASH-CAS` extended (já planejado em `observability_model.md §8`); add widgets: read latency p50/p95/p99 per region; integrity failure timeline; side-channel timing distribution histogram; negative cache hit ratio.

**Alertas:**
- `corelink_cas_client_verify_fail_total > 0` → SEV-1 imediato (FM-254 cache poisoning OR FM-051 bit rot).
- `corelink_cas_cross_tenant_attempt > 0` → SEV-1 imediato (FM-253).
- `corelink_cas_side_channel_timing_diff_ms{quantile="0.99"} > 5` sustained 5min → SEV-2 (CTRL-ISO-004 drift).
- `SLO-AVAIL-CAS-GET burn_rate(1h) > 14.4` → page (multi-burn-rate per `slo_catalog.md §9.3`).
- `SLO-LAT-CAS-GET p99(cold) > 300ms` sustained 10min → SEV-3 ticket.

## 12. Security & Privacy (delta)

Herda de `security_model.md` + `privacy_model.md`. Delta local:

- **STRIDE delta:** introduz superfície:
  - THR-I-002 (information disclosure via timing side-channel) — mitigado por CTRL-ISO-004 + criterion + Mann-Whitney.
  - THR-I-003 (information disclosure via MissReason existence oracle per ADR-0028) — mesmo CTRL (CTRL-ISO-004 + ADR-0023).
  - THR-T-001 (cache poisoning at read time se R2 corrompido) — mitigado por CTRL-CAS-002 client verify default-on.
- **LINDDUN delta:** introduz processamento de tenant_id em path lookup; mitigado por TenantPrefix HMAC (não vaza tenant_id em logs externos).
- **Novos trust boundaries:** nenhum (mantém TB-0..TB-5 canônicos).
- **Privacy impact:** zero direto (read path é pull-only; client verifica integridade; nenhum PII novo introduzido).
- **Adversarial scenarios** (pentest scope):
  - Enumerate digests via timing across 404 MissReason variants (NotFound × CrossTenantMasked × Tombstoned per ADR-0028).
  - Bypass client verify via opt-out flag em SDK.
  - Cross-tenant read via crafted PAT + path manipulation.
  - Probe storm exploitando negative cache TTL.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Cross-tenant read detected (any) → CRITICAL post-mortem + Privacy Officer + Legal + breach notification consideration (LGPD Art. 48 / GDPR Art. 33 trigger).
- Side-channel timing diff > 5ms p99 sustained > 1h → 5-Why + INV-CAS-SIDE-CHANNEL review.
- Client verify bypass detectado em produção → CRITICAL post-mortem + CTRL-CAS-002 review.
- Negative cache stale > grace 5 min (write→read inconsistency) → post-mortem + invalidation flow review.
- Streaming memory leak (Worker OOM) → post-mortem + chunk size + restart policy review.
- TLA+ CI red (tenant_isolation ou cas_integrity) → CRITICAL post-mortem + invariant scope review.
- Bit rot detected em produção (não em CI) → post-mortem + R2 scrub frequency review.

## 14. Sign-off (HIGH_RISK — 11 roles canonical)

Ver tabela canônica em `00_framework.md §33.5.4.3`. Sprint S-02 exige **os 11 papéis HIGH_RISK** (Compliance + Adversarial + AppSec já incluídos no count canonical; alinhado com ADR-0034 solo-tier waiver). Crypto SME (BLAKE3 verify review) + AppSec (side-channel test review) são specialized reviewers dentro dos 11 (não extensão).

Roles canonical (11 per framework §33.5.4.3 HIGH_RISK matrix):
1. Owner (Gustavo Schneiter)
2. Final Approver (Gustavo Schneiter)
3. Architect (design + Crypto SME specialization for BLAKE3 + Mann-Whitney methodology review)
4. Security Lead (STRIDE + Crypto compliance specialization)
5. SRE Lead
6. Engineer (S-02 implementation lead)
7. QA Lead
8. Product
9. Compliance Officer
10. Privacy Officer
11. AppSec advisor / Adversarial reviewer (side-channel + pentest)

> Peer reviewers contribuem feedback durante PR review mas não são sign-off canonical separado (folded into Engineer + Architect roles).

> Sign-off será preenchido ao final do sprint em `sprint.md §14.1`. Template em `specs/_templates/sprint_contract.md §20.1`.

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação do sprint S-02 — CAS Read Path. HIGH_RISK com FF-HR-002 (tenant isolation read surface) + FF-HR-005 (CTRL-CAS-002 + CTRL-ISO-002 + CTRL-ISO-004). Baseado em `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA elevation). |

---

**Fim de S-02 sprint contract.** Próximos docs: `work_items/WI-S02-001-bytestream-read.md` … `WI-S02-006-property-test-prr.md`.
