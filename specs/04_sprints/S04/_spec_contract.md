---
id: "SPEC-CONTRACT-S04"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.6.0"
created: "2026-04-24"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s04", "action-cache", "reapi", "merkle", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-04: Action Cache (REAPI GetActionResult/UpdateActionResult + Merkle + TTL Infrastructure)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-04 |
| Nome | Action Cache (AC) |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (cross-tenant AC = catastrophic FM-303), FF-HR-005 (CTRL-AC-001 + CTRL-AC-002) |
| Duração estimada | 3 semanas + buffer 5 dias |
| WIs antecipados | 6 |
| SOTA target | AC tier-1 — Bazel/Buck2 conformance + Merkle verify dual-side + cache hit ratio > 70% staging + tenant-scoped TLA+ verified |

## 1. Objetivo

Implementar **Action Cache (AC) production-grade** — a "cereja" do remote cache Bazel/Buck2: dado hash de action determinística, retorna resultados de execução previamente computada. Requer **integridade Merkle dual-side** (server reject invalid trees + client verify) + **tenant-scoped strict** + **TTL infrastructure** (defaults per-tier delegados a S-07/ADR-0019). Sem AC, CoreLink é só blob store; com AC, é remote cache real que acelera builds. **Bug em tenant isolation = FM-303 catastrophic** (build result vaza entre customers).

**Por que SOTA:** competitors entregam AC com Merkle verify server-only (cache poisoning vector); REAPI v2 conformance varia (BuildBuddy 95%, NativeLink 80%); cache hit ratio raramente exposto como business métrica. CoreLink S-04 entrega: (a) Merkle verify dual-side (server + client); (b) **REAPI v2 conformance suite passing 100%** (bazelbuild/remote-apis test suite); (c) cache hit ratio em DASH-AC + customer-visible em S-16; (d) AC digest signing CTRL-AC-002 (HKDF). Reference: **REAPI v2 spec**, **Bazel ActionCache proto**, **NIST SP 800-185** (KMAC for Merkle).

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (11 sign-offs canonical per framework §33.5.4.3 + ADR-0034; Crypto SME folds into Architect role; DBA folds into Architect; AppSec é o 11º slot).
- **FF-HR-002**: AC cross-tenant = serve build result de outro tenant = catastrófico (FM-303).
- **FF-HR-005**: implementa CTRL-AC-001 (Merkle verify) + CTRL-AC-002 (digest signing HKDF).

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # REAPI v2 ActionCache semantics
  - "SECURITY-MODEL"                # CTRL-AC-001..002
  - "DATA-MODEL"                    # ac_meta schema §4.2
  - "INVARIANT-REGISTRY"            # INV-AC-OUTPUTS-VALID, INV-AC-TENANT-SCOPED
  - "KEY-MANAGEMENT"                # TDK reuse + HKDF info="ac-sig"
  - "OBSERVABILITY-MODEL"           # DASH-AC, métricas hit ratio
  - "FAILURE-MODES"                 # FM-303 (AC cross-tenant)
  - "SLO-CATALOG"                   # SLO-AVAIL-AC, SLO-LAT-AC-HIT
  - "RESILIENCE-PATTERNS"           # PAT-RETRY-IDEMPOTENT-001
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-AC-001** | REAPI GetActionResult (cache hit) | Returns `ActionResult` proto se cached; 404 se miss + negative cache (S-02 alignment). |
| **CAP-AC-002** | REAPI UpdateActionResult (cache write) | Writes `ActionResult`; pre-write Merkle verify; idempotent via `(tenant_id, action_digest)` UNIQUE. |
| **CAP-AC-003** | Merkle verification dual-side | Server-side reject invalid trees pre-persist; client-side verify post-download (CAP-CAS-005 reuse). |
| **CAP-AC-004** | AC TTL management infrastructure | TTL worker + refresh-on-hit + expiry detection. **Default value supersedido por S-07 CAP-EVICT-002 per-tier table** (ADR-0019). |
| **CAP-AC-005** | AC entry invalidation | Client-requested via DELETE; admin override via S-13 admin plane. |
| **CAP-AC-006** | Cache hit ratio business métrica | `corelink.cache.hit_ratio{type=ac, tenant_tier, region}` exposed em DASH-AC + customer dashboard S-16. |

## 5. Requirements específicos

- **R-S04-1**: REAPI `ActionCache::GetActionResult` + `UpdateActionResult` Worker handlers; gRPC + REST surface.
- **R-S04-2**: D1 schema `ac_meta` (conforme `data_model.md §4.2`) + R2 bucket `ac-<region>`; UNIQUE constraint `(tenant_id, action_digest)`.
- **R-S04-3**: Crate `corelink-ac` com:
  - ActionResult proto codec.
  - Merkle tree builder/verifier.
  - INV-AC-OUTPUTS-VALID enforcement: pre-persist verify outputs digests existem em `blob_meta` alive (não tombstoned).
- **R-S04-4**: AC digest signing via CTRL-AC-002 (HKDF tenant key info="ac-sig"). **Decisão**: HKDF-only (não Ed25519); rationale em ADR-0021 (criar como sub-PR durante S-04 implementation — HKDF é symmetric + faster + suficiente para integrity binding tenant-scoped; Ed25519 over-kill para use case).
- **R-S04-5**: TTL worker (cron DO) que expira entries > `expires_at`; reuses S-07 evicition framework boundary (ADR-0019).
- **R-S04-6**: Metrics + dashboards (DASH-AC conforme `observability_model §8`); cache hit ratio per (tenant_tier, region).
- **R-S04-7**: Negative cache integration: AC miss popula `ac_neg:<action_digest>` KV TTL 60s (mais curto que CAS — actions ficam stale rápido).

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **E2E**: Bazel build com `--remote_cache=corelink://...` → cache hit reduz tempo em > 50% em rebuilds (EVT-018).
- [ ] **Property test 100k**: AC entry de Tenant A nunca retornada a Tenant B (EVT-002).
- [ ] **TLA+ invariantes verdes**: tenant_isolation.tla covers AC via `INV-AC-TENANT-SCOPED` derivation (EVT-022).
- [ ] **Chaos**: TTL expiry sob load + stale read handling (EVT-023).
- [ ] **SLO-AVAIL-AC e SLO-LAT-AC-HIT** sustained 72h staging (EVT-021).
- [ ] **PRR HIGH_RISK** + adversarial review: SRE + Security + Engineer + QA + Product + Compliance + Privacy + Architect + AppSec + 2 peers + Crypto SME (HKDF review) (EVT-031).
- [ ] **REAPI v2 conformance** (bazelbuild/remote-apis test suite) passa para AC ops (EVT-002 + EVT-018).
- [ ] **Merkle verify**: invalid tree rejected pre-persist + post-download client (EVT-002).
- [ ] **Cache hit ratio** > 70% em workload sintético Bazel (EVT-021).
- [ ] **ADR-0021** "AC digest signing HKDF vs Ed25519" decision documented (EVT-046 if Legal review needed; EVT-027 ADR).
- [ ] **Cost regression gate** (§14.10): AC hit hot path benchmark; PR > 10% regression bloqueia (EVT-002).
- [ ] **RB-FM-303** (AC cross-tenant) dry-run executado em staging (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s04.1** REAPI v2 conformance test suite (bazelbuild/remote-apis) passa para AC ops 100% (EVT-002).
- [ ] **10.s04.2** Merkle tree verify: invalid tree rejeitada no read path + write path (EVT-002).
- [ ] **10.s04.3** AC hit ratio > 70% em workload de test Bazel sintético (EVT-021).
- [ ] **10.s04.4** **TTL infrastructure aligned com S-07** (ADR-0019) — defaults per-tier propagated ao deploy time.
- [ ] **10.s04.5** **HKDF digest signing** verified via property test 10k iter (EVT-002).

## 8. Invariants

### Mantidas

- **INV-AC-OUTPUTS-VALID** (HIGH): outputs referenciados existem em `blob_meta` alive; reconcile diário (S-06 GC interaction).
- **INV-AC-TENANT-SCOPED** (CRITICAL, TLA+): deriva de INV-TENANT-ISOLATION; AC cross-tenant impossible.
- **INV-CAS-IMMUTABILITY** (CRITICAL): AC entry é imutável pós-write (atualização = nova entrada via UPDATE com `(tenant_id, action_digest, version)`).

### Não cria invariants novas — INV-AC-* já em registry §3.3.

## 9. Quality Standards (delta local)

- **14.s04.1 Merkle verify p99 ≤ 10ms** pra trees ≤ 100 nodes (criterion benchmark).
- **14.s04.2 AC GetActionResult p99 ≤ 150ms** (SLO-LAT-AC-HIT) sustained.
- **14.s04.3 Cache hit ratio** emitido como métrica business (`corelink.cache.hit_ratio{type=ac}`); customer-visible S-16.
- **14.s04.4 Tenant isolation defense-in-depth**: 5 layers conforme `auth_model.md §8.1` (AC herda CAS isolation pattern).
- **14.s04.5 Cost regression gate** (§14.10): AC GET hot path benchmark; PR > 10% regression bloqueia.
- **14.s04.6 REAPI v2 conformance** mantido CI (bazelbuild/remote-apis suite quarterly re-run).
- **14.s04.7 HKDF discipline**: info string `ac-sig` fixo; nunca dynamic; tenant_key reused do S-01 TDK.

## 10. Anti-scope

- ❌ Execute Action (Fase 2 — Remote Execution; pós-GA).
- ❌ AC cross-tenant dedup (FM-303 risk; requer ADR explicit + bloqueado at GA).
- ❌ AC Content-Delivery Network (CDN layer em S-14 multi-region).
- ❌ AC TTL defaults per-tier (delegado a S-07 / ADR-0019).
- ❌ Ed25519 digest signing (rejected; HKDF é suficiente; ADR-0021).
- ❌ AC entry version migration tooling — entry imutável; mudança = nova entry.
- ❌ AC outputs proativos pre-fetch (caching reactive only at GA).

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS write path; outputs blobs precisam existir).
- **S-02 SEALED** (CAS read path para output blobs referenciados).
- **S-03 SEALED** (auth tenant ctx).

### Soft blockers

- **S-09** (observability — DASH-AC; pode ser delayed).

### Outbound

- **S-07** (eviction TTL defaults supersede CAP-AC-004 — ADR-0019).
- **S-13** (admin plane — invalidation override).
- **S-15** (CLI/SDK consume AC API).
- **S-20** (GA exige cache hit ratio sustained + REAPI conformance).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S04-001** | REAPI ActionCache proto handlers + REST surface | gRPC + REST handlers; auth middleware reuse; tenant context propagation | 12h | 18h | 28h | **18.7h** |
| **WI-S04-002** | D1 ac_meta + R2 ac bucket + schema + UNIQUE constraint | D1 migration; R2 bucket per-region; idempotency UNIQUE; integration test | 10h | 14h | 22h | **14.7h** |
| **WI-S04-003** | Crate corelink-ac (Merkle codec + verify dual-side) + INV-AC-OUTPUTS-VALID enforcement | Merkle builder/verifier; ActionResult codec; pre-persist outputs check; criterion benchmark | 14h | 22h | 36h | **23.0h** |
| **WI-S04-004** | CTRL-AC-002 digest signing HKDF + ADR-0021 + property test | HKDF info="ac-sig"; signing/verify; ADR-0021 decision; property test 10k | 10h | 14h | 22h | **14.7h** |
| **WI-S04-005** | TTL worker (cron DO) + S-07 boundary alignment ADR-0019 | TTL worker DO; expiry job; S-07 boundary respect; refresh-on-hit | 10h | 16h | 26h | **16.7h** |
| **WI-S04-006** | REAPI conformance tests + DASH-AC dashboards + RB-FM-303 dry-run + PRR | bazelbuild/remote-apis suite; DASH-AC JSON; cache hit ratio métrica; RB-FM-303 walkthrough; PRR | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~107h ≈ 13 dias work × 1 eng. Buffer 5 dias confere com 3 semanas.

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 5 dias.
- **Marcos:**
  - **D+3:** WI-001 + WI-002 SEALED (REAPI handlers + schema).
  - **D+8:** WI-003 SEALED (Merkle dual-side).
  - **D+10:** WI-004 SEALED (HKDF signing + ADR-0021).
  - **D+12:** WI-005 SEALED (TTL worker + boundary).
  - **D+15:** WI-006 SEALED (conformance + dashboards + PRR).
  - **D+17:** Sprint review.

## 14. Critérios de promoção

- DoD complete + 72h staging sustained.
- Bazel REAPI conformance passa 100%.
- Cache hit ratio measurable.
- HKDF property test 10k green.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Merkle tree invalid detected late** (após persist) | M | M | HIGH (stale AC entries) | M | LOW | Dual-side verify (server pre-persist + client post-download); pre-persist reject invalid; INV-AC-OUTPUTS-VALID daily reconcile. |
| **AC TTL too aggressive** → hit ratio baixo | M | L | MEDIUM (customer perception) | L | LOW | Defaults per-tier ADR-0019; refresh-on-hit; customer-visible métrica; tunable via admin S-13. |
| **AC tenant leak via bug de digest computation** | L | M | CRITICAL (FM-303) | M | LOW | INV-AC-TENANT-SCOPED TLA+ + property test 100k + tenant prefix derivation pattern; RB-FM-303. |
| **REAPI v2 spec interpretation divergente de Bazel** | M | M | MEDIUM (rework) | M | LOW | Conformance suite bazelbuild/remote-apis CI; quarterly re-run; community engagement. |
| **HKDF info string drift** (typo "ac-sig" → "acsig") | L | L | HIGH (signature mismatch global) | L | LOW | Constant em código; CI test verifica; ADR-0021 documented. |
| **Cache hit ratio < 70% em workload real** | M | L | MEDIUM (DX miss) | L | LOW | Workload tuning + DX research; customer dashboard surface; iterate post-launch. |
| **Outputs referenciados deletados via GC** (race S-06) | M | M | HIGH (INV-AC-OUTPUTS-VALID violated) | M | LOW | INV-GC-004 mark-phase-aware re-ref protection (S-06); reconcile diário detect; auto-fix. |
| **REAPI gRPC vs REST surface drift** | M | L | LOW | L | LOW | Single source proto; codec generated; conformance suite cobre. |

## 16. Benchmarks SOTA externos

| Critério | BuildBuddy | NativeLink | bazel-remote | **CoreLink target S-04** |
|---|---|---|---|---|
| REAPI v2 conformance | 95% | 80% | 90% | **100% suite green** |
| Merkle verify dual-side | Server only | Server only | Server only | **Yes — server + client** |
| Cache hit ratio business métrica | Yes (UI) | No | No | **Yes — DASH-AC + customer dashboard** |
| TLA+ tenant isolation AC | No | No | No | **Yes — derivada INV-AC-TENANT-SCOPED** |
| AC GetActionResult p99 | 200-400ms | 150-300ms | 100-200ms | **≤ 150ms warm** |
| TTL infrastructure tunable | Manual | Manual | Manual | **Yes — config-singleton S-13** |
| HKDF tenant signing | No | No | No | **Yes — CTRL-AC-002** |

## 17. References (RFCs, papers, standards)

- **REAPI v2 Specification** <https://github.com/bazelbuild/remote-apis>.
- **bazelbuild/remote-apis conformance test suite**.
- **NIST SP 800-185** — KMAC and SHA-3 (Merkle alignment).
- **NIST SP 800-108** — Key Derivation (HKDF).
- **Bazel ActionCache proto** <https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto>.
- `specs/03_architecture/error_taxonomy.md` — `COR_AC_*` error codes.
- ADR-0019 (TTL ownership S-04↔S-07).

## 18. Post-mortem hooks

- Cross-tenant AC leak detected → CRITICAL post-mortem + Privacy + breach notification consideration.
- Merkle invalid persisted (escape from dual-side verify) → CRITICAL post-mortem + verifier code review.
- INV-AC-OUTPUTS-VALID violation > 5 records → 5-Why obrigatório + GC interaction review.
- REAPI conformance regression → post-mortem + Bazel community engagement.
- Cache hit ratio < 50% sustained 7d → post-mortem (DX) + workload analysis.

## 19. Waiver policy

S-04 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-AC-TENANT-SCOPED property test 100k green — security baseline.
- ❌ Merkle verify dual-side — INV-CAS-INTEGRITY alignment.
- ❌ REAPI v2 conformance suite passa **100%** — DX baseline (Lote 10.4bis tightening; HIGH_RISK GA não deve shippar com 5% conformance gap; emergency bumps via ADR forward).
- ❌ HKDF digest signing CTRL-AC-002 — security baseline.

Itens waivable com Architect + Crypto SME + ADR:

- ⚠️ Cache hit ratio target 70% → 50% com plan to improve next sprint.
- ⚠️ AC GetActionResult p99 150ms → 200ms com customer SLA addendum.

**Lote 10.4bis amendment**: 95%-com-waiver path para conformance REMOVIDO (era ⚠️ no v1.1.0). HIGH_RISK GA exige 100% conformance non-negotiable. Emergency Bazel security release (CVE-driven) bypass cadence quarterly via Architect approval + ADR within 1 sprint (vide WI-S04-006 §6.1.10).

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo | Initial sprint contract S-04 (Action Cache + HKDF signing; HIGH_RISK 11 sign-offs canonical). |
| 1.1.0 | 2026-04-25 | Gustavo | Lote 10.4bis P0 fixes (Agent R4 review remediation): HKDF info bytes domain separation; 95%-with-waiver REMOVED; INV §3.15 promotion; ADR canonical path adrs/; TenantCtx-only; audit fail-closed; 4-tier P0/P1/P2/P3. |
| 1.2.0 | 2026-04-25 | Gustavo | **Lote 10.4-tris fixes** (Sonnet R5 independent review; 6 NEW P0s + 11 P1s + ADR creation): (a) **P0-R5-001** key_id=0 reserved sentinel + `SigError::KeyIdReserved`; (b) **P0-R5-002** HKDF-Extract composition with path-HMAC TDK documented in ADR-0021 §Risks (Option B accepted under HMAC security; attack cost 2^128); (c) **P0-R5-003** canonical_bytes 121 bytes retained; result_hash = BLAKE3(merkle_root).hex() D1 INDEX column ONLY (NOT cripto binding); ADR-0037; (d) **P0-R5-004** Gherkin BatchUpdateActionResult struck; (e) **P0-R5-005** Crypto SME mandatory (WI-004 SEAL non-waivable) vs advisory (WI-006 PRR ceremony) split explicit em BOTH WIs + ADR-0034; (f) **P0-R5-006** §30.1 key rotation procedures `sig_key_id` + `path_key_id` independent lifecycles; (g) **P1-R5-016** ADRs 0021/0034/0035/0036/0037 stubs created. |
| 1.4.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | **WI-S04-001 SEALED** — first S-04 impl WI shipped. Pure-logic AC handler module landed at `crates/corelink-worker/src/reapi/ac/` (8 sub-modules: handler / types / meta / sig / merkle / outputs / neg_cache / audit) with full 5-Layer Defense structural enforcement, 8-variant `AcEventType` audit taxonomy, 121-byte canonical envelope preimage per ADR-0021, integration with S-02 `NegativeCache` (60 s AC TTL per WI §9.3) + S-03 `AuthCtx`. Tests: 49 lib unit + 13 Gherkin e2e + 5 property × 10k iter. Tonic gRPC + axum REST wrappers deferred per charter trait-abstraction-defer pattern (alongside REAPI conformance suite WI-S04-006); Merkle real impl WI-S04-003; HKDF real impl WI-S04-004; D1 binding WI-S04-002 + WI-S04-006. Commit reference: `[this commit]`. |
| 1.6.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S04-003 SEALED** — new crate `crates/corelink-ac/` ships the canonical Merkle codec + dual-side verifier (BLAKE3-256 with RFC 6962-style `\x00`-leaf / `\x01`-inner domain separation; lex-sorted leaf canonicalization yielding byte-deterministic `merkle_root` per ADR-0037; bounded parser pinning `MAX_TREE_DEPTH=32` / `MAX_TREE_FANOUT=4096` / `MAX_NODE_COUNT=100 000` / `MAX_PAYLOAD_BYTES=1 MiB` / `MAX_OUTPUT_FILES=4096` / `MAX_OUTPUT_DIRECTORIES=4096` matching the worker's `corelink-worker::reapi::ac::merkle` constants verbatim). Surface delivered: `MerkleVerifier` trait + `CanonicalMerkleVerifier` Send/Sync zero-state real impl; `build_root` + `verify_root` + `compute_result_hash` (= `BLAKE3(merkle_root)` per ADR-0037 D1 INDEX column derivation); `AcEnvelope` `#[non_exhaustive]` wire struct with hex-encoded `merkle_root` / `result_hash` for canonical JSON encoding; `codec::encode` + `codec::decode` enforcing `MAX_PAYLOAD_BYTES` BEFORE `serde_json::from_slice` (pre-allocation rejection); `BlobMetaReader` + `StrictOutputsValidator` enforcing `INV-AC-OUTPUTS-VALID` strict-fail with tenant-scoped batch lookup; 11-variant `MerkleError` `#[non_exhaustive]` taxonomy with `audit_code()` short-id contract 1:1-aligned with the worker's existing enum. Worker integration: `corelink-worker` adds `corelink-ac` dependency + `CanonicalAcMerkleVerifier` adapter that projects local `ActionResult` into the `corelink-ac` wire shape, invokes `CanonicalMerkleVerifier`, and maps every canonical-error variant back to the worker's local taxonomy preserving `audit_code` semantics; the worker's existing `InMemoryMerkleVerifier` test fake stays untouched and every existing `prop_ac_handlers` / `ac_handler_e2e` integration test continues to pass (forward-compat per WI §10). Tests: **63 new** (30 lib unit + 7 canonical vectors + 9 round-trip + 11 mutation-resistance + 6 property × 10 000 iter [`prop_merkle_determinism`, `prop_merkle_root_independent_of_insertion_order`, `prop_merkle_tampering_detected`, `prop_merkle_round_trip_codec_stable`, `prop_outputs_missing_rejected`, `prop_bounds_enforcement`]) **+ 3 worker adapter unit tests**, all passing on debug. wasm32-clean (no tokio). **Deferred per charter trait-abstraction-defer**: criterion p99 ≤ 10 ms benchmarks (consolidated WI-S04-006 alongside REAPI conformance suite); cargo-fuzz 1 h CI nightly (alongside conformance suite); Mann-Whitney 3-prong verify-timing test (alongside conformance gRPC surface); REAPI Directory-proto cycle traversal (current bounded parser rejects oversized outer slice at decode; nested-Directory walk lands with the Tree proto codec in WI-S04-006); 100 k nightly property iter; 50 + 50 published Annex A/B fixtures (the canonical-vector + tampering tests cover the full algorithmic boundary; external-consumer fixtures land with the conformance suite). Quality gates verde: full workspace `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `python3 scripts/validate_specs.py` clean; `python3 scripts/validate_references.py` no new dangling refs. |
| 1.5.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S04-002 SEALED** — D1 `ac_meta` schema + R2 `ac-<region>` bucket provisioning + UNIQUE constraint + tenant-scoped indices + migration idempotency. Ship list: (a) **Migration `migrations/d1/0002_ac_meta.sql`** — 16-column `ac_meta` table; PK composite `(tenant_id, action_digest)` tenant-leftmost; 11 inline CHECK constraints (digest length / blob_refs size + count / result_size_bytes / region IN 5-list / sig_alg / lifecycle / tenant_prefix length / path_key_id / sig_key_id); 3 indices (idx_ac_meta_tenant_expires PARTIAL on `expires_at IS NOT NULL`, idx_ac_meta_tenant_last_hit, idx_ac_meta_region); idempotent via `IF NOT EXISTS`; additive-only (gates `scripts/check_migrations_additive.py`). (b) **New crate `corelink-ac-schema`** — embeds the migration via `include_str!`; `AcSchema` host-side simulator pinning every load-bearing invariant (PK uniqueness, every CHECK predicate, idempotent `ON CONFLICT` semantic, INV-AC-RESULT-HASH-IMMUTABLE preservation on idempotent refresh, tenant-isolation envelope); `AcRegion` enum mirrors 5-region SQL CHECK list (sam/iad/lhr/nrt/syd) with R2 bucket + wrangler binding accessors. (c) **Tests**: 21 lib unit + 14 migration_canonical + 8 idempotency_canonical + 11 property tests @ 10 k iter (PK uniqueness, cross-tenant isolation, CHECK enforcement, lifecycle, blob_refs_count round-trip, multi-tenant population isolation, migration re-apply preserves rows). (d) **`wrangler.toml`** updated with 5 R2 bindings (`AC_BUCKET_{SAM,IAD,LHR,NRT,SYD}`) for default + `[env.prod]`. (e) **Scripts**: `scripts/provision_ac_buckets.sh` (idempotent CF API CORS PUT + lifecycle abort-multipart-7d); `scripts/check_ac_infra.sh` (4-step pre-deploy guard with graceful skip on missing CF secrets); `scripts/seed_ac_staging.sh` (DEV/STAGING-only seeder; refuses prod via env guard); `scripts/migrate_d1.sh` extended for the 5 AC regions. (f) **CI workflow** `.github/workflows/ac-bucket-acl-cron.yml` — nightly + on-dispatch CORS audit + self-heal PUT step on drift. (g) **Schema doc** `docs/internal/d1-schema-ac-meta.md` (column-by-column rationale, CHECK / index rationale, R2 layout, sizing projection, governance). (h) **Runbooks** `RB-FM-AC-MIGRATION-BUG` + `RB-FM-AC-BUCKET-LEAK`. **Deferred per charter trait-abstraction-defer**: real Cloudflare D1 binding shim, live wrangler-d1 + miniflare integration smoke, EXPLAIN QUERY PLAN benchmarks, 100 k nightly property iter, REAPI conformance — all consolidate in WI-S04-006. Quality gates verde: full workspace `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures (54 new tests); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `python3 scripts/validate_specs.py` clean (273 docs incl. 2 new runbooks); `python3 scripts/check_migrations_additive.py` clean (4 migration files). |
| 1.7.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S04-004 SEALED** — CTRL-AC-002 HKDF-SHA256 + BLAKE3-keyed digest signing infrastructure. Ship list: (a) **`crates/corelink-ac/src/sig/`** — 4 sub-modules: `error.rs` 6-variant `SigError` taxonomy with `audit_code()` short-id contract; `canonical.rs` 121-byte preimage `compose()` mirroring `corelink-worker::reapi::ac::sig::AcEnvelope::canonicalize` 1:1 with compile-time `assert!` parity check on the field offsets; `tdk.rs` `TdkHandle` trait + `MockTdkHandle` deterministic test fixture (BLAKE3-derived per-(tenant_id, sig_key_id) TDK) + `Tdk` newtype wrapping `Zeroizing<Vec<u8>>` zero-on-drop with redacted Debug + crate-private `as_bytes` accessor (no Display/PartialEq/AsRef public surface — leak-by-print is a compile error); `hkdf_signer.rs` `HkdfSigner` + `HkdfVerifier` real impl with `salt = sig_key_id.to_le_bytes()` + `info = b"ac-sig"` per ADR-0021 + `subtle::ConstantTimeEq` verify path + `accepted_key_ids` rotation grace API (`new` / `admit` / `retire` / `rotate_to`) + reserved-sentinel rejection on every entry point + free-fn `compute_signature(&[u8; 32], u32, &[u8])` for client SDK dual-side verify. (b) **Public traits**: `SignatureSigner` + `SignatureVerifier` Send+Sync surfaces. (c) **Worker integration**: `corelink-worker::reapi::ac::sig::CanonicalAcSigner` adapter (new — wraps `Arc<HkdfSigner>` + `Arc<HkdfVerifier>` and satisfies the worker's local `Signer` trait + maps every canonical `SigError` arm to the worker's local taxonomy preserving cripto-mismatch identity); existing `InMemoryFakeSigner` test fake stays untouched. (d) **Tests**: 31 lib unit + 13 canonical_vectors_sig + 9 key_rotation_sig + 6 property × 10 000 iter (`prop_sign_verify_roundtrip` / `prop_tampered_sig_rejected` / `prop_wrong_key_id_rejected` / `prop_canonical_bytes_byte_stable` / `prop_cross_tenant_signature_rejected` / `prop_key_rotation_grace`) + Mann-Whitney 3-prong cripto-grade timing test (3 arms × 10 000 samples × 3 trials; 9 pair-tests; Šidák-corrected α' ≈ 0.005 686; 10/80/10 trimmed-mean central estimator per WI-S03-008 ct-variance lesson; bootstrap 95 % CI on `\|Δ(trimmed_mean)\|` ≤ 0.5 ms cripto-grade gate; release-mode-only via `#[cfg_attr(debug_assertions, ignore)]`; dudect §III.A batched measurement window of 256 verify ops per sample so timer-resolution ties don't overwhelm the rank statistic) + 4 worker adapter integration tests (`canonical_ac_signer_round_trips_real_hkdf` / `canonical_ac_signer_rejects_byte_flip` / `canonical_ac_signer_rejects_reserved_key_id` / `canonical_ac_signer_unknown_key_id_maps_to_backend`). (e) **HKDF info CI gate**: `b"ac-sig"` byte-equal asserted in lib unit + integration test (drift detection — typo `b"acsig"` triggers test failure on first PR). (f) **Hardening**: `#![forbid(unsafe_code)]`; zero `unwrap`/`expect`/`panic` in src/; `clippy::indexing_slicing = "deny"` + canonical-offset compile-time `assert!` proof of in-range; `mod_module_files = "deny"` honored (`sig.rs` parent + `sig/` children); `Tdk` redacted Debug ensures TDK bytes never reach a tracing log line. **Deferred per charter trait-abstraction-defer**: real `CfSecretsTdkHandle` Cloudflare Secrets binding shim (alongside WI-S04-006 conformance suite + miniflare/wrangler-dev integration); 100 k nightly property iter; cargo-fuzz 1 h CI nightly target (alongside conformance suite); test vectors Annex C/D 50-vector publication (canonical_vectors_sig already covers the algorithmic boundary; external-consumer fixtures land alongside conformance suite); criterion p99 sign/verify ≤ 1 ms benchmarks (alongside conformance suite). Quality gates verde: full workspace `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `python3 scripts/validate_specs.py` clean; `python3 scripts/validate_references.py` no new dangling refs. ADR-0021 v1.0.0 (FROZEN) ratified — implementation matches canonical decision matrix byte-for-byte. |

---

**Fim spec contract S-04 v1.7.0 SOTA.**
